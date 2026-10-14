//! Integration tests hit the live backend. Run against a docker-compose stack:
//!   docker compose up -d postgres backend
//!   cargo test -- --ignored
//!
//! By default only the pure unit-style test below runs in CI (no DB required).
//! The v1 tests (schema, units and guards) only need a live database:
//!   DATABASE_URL=postgres://tempox:tempox@localhost:5432/tempox cargo test --test integration -- --ignored

#[test]
fn smoke_arithmetic() {
    // Sanity placeholder so `cargo test` always has something to run.
    assert_eq!(2 + 2, 4);
}

#[tokio::test]
#[ignore]
async fn health_endpoint_returns_ok() {
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());
    let resp = reqwest::get(format!("{base}/health"))
        .await
        .expect("request");
    assert!(resp.status().is_success());
    let json: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
#[ignore]
async fn notes_crud_roundtrip() {
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{base}/api/notes"))
        .json(&serde_json::json!({ "text": "hello from tests" }))
        .send()
        .await
        .expect("post")
        .error_for_status()
        .expect("2xx")
        .json()
        .await
        .expect("json");
    let id = created["id"].as_str().expect("id").to_string();

    let list: serde_json::Value = client
        .get(format!("{base}/api/notes"))
        .send()
        .await
        .expect("get")
        .json()
        .await
        .expect("json");
    assert!(
        list.as_array()
            .expect("array")
            .iter()
            .any(|n| n["id"] == id)
    );

    let del = client
        .delete(format!("{base}/api/notes/{id}"))
        .send()
        .await
        .expect("delete");
    assert_eq!(del.status(), 204);
}

// v1 schema (work-unit 1): the migration creates the five tables and seeds the
// walk-in customer. Ignored by default: needs a live database.
#[tokio::test]
#[ignore]
async fn v1_schema_seeds_walkin_customers() {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");

    for table in [
        "customers",
        "products",
        "sales",
        "sale_items",
        "stock_movements",
    ] {
        let regclass: (Option<String>,) = sqlx::query_as("SELECT to_regclass($1)::text")
            .bind(format!("public.{table}"))
            .fetch_one(&pool)
            .await
            .expect("to_regclass");
        assert!(regclass.0.is_some(), "table {table} is missing");
    }

    let name: (String,) = sqlx::query_as(
        "SELECT name FROM customers WHERE id = '00000000-0000-0000-0000-000000000001'",
    )
    .fetch_one(&pool)
    .await
    .expect("walk-in seed row");
    assert_eq!(name.0, "walk-in");
}

// v1 units + history guards: quantities are integers in the product's base
// unit, and the database enforces the immutability rules.
// Ignored by default: needs a live database.
#[tokio::test]
#[ignore]
async fn v1_units_and_history_guards() {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");

    let has_unit: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM information_schema.columns
         WHERE table_name = 'products' AND column_name = 'unit'",
    )
    .fetch_one(&pool)
    .await
    .expect("unit column check");
    assert_eq!(has_unit.0, 1, "products.unit is missing");

    // Scenario 1: a product WITH history cannot change its unit.
    // The transaction is dropped without commit, so it rolls back.
    {
        let mut tx = pool.begin().await.expect("begin");
        let pid: (uuid::Uuid,) = sqlx::query_as(
            "INSERT INTO products (name, list_price, unit) VALUES ('guard-test-rice', 250, 'g')
             RETURNING id",
        )
        .fetch_one(&mut *tx)
        .await
        .expect("insert product");
        sqlx::query(
            "INSERT INTO stock_movements (product_id, quantity, reason) VALUES ($1, 1500, 'initial')",
        )
        .bind(pid.0)
        .execute(&mut *tx)
        .await
        .expect("insert movement");
        let blocked = sqlx::query("UPDATE products SET unit = 'unit' WHERE id = $1")
            .bind(pid.0)
            .execute(&mut *tx)
            .await;
        assert!(blocked.is_err(), "unit changed despite history");
    }

    // Scenario 2: a product WITHOUT history can change its unit.
    {
        let mut tx = pool.begin().await.expect("begin");
        let pid: (uuid::Uuid,) = sqlx::query_as(
            "INSERT INTO products (name, list_price, unit) VALUES ('guard-test-no-history', 100, 'unit')
             RETURNING id",
        )
        .fetch_one(&mut *tx)
        .await
        .expect("insert product");
        let allowed = sqlx::query("UPDATE products SET unit = 'g' WHERE id = $1")
            .bind(pid.0)
            .execute(&mut *tx)
            .await;
        assert!(allowed.is_ok(), "unit change blocked without history");
    }

    // Scenario 3: sale lines are immutable and a sale's economics are guarded.
    // Each expected failure runs inside a SAVEPOINT and rolls back to it:
    // in Postgres a failed statement aborts the whole transaction (25P02),
    // so without the savepoint the remaining probes would fail for the wrong
    // reason and the allowed transition would never get a fair check.
    let mut tx = pool.begin().await.expect("begin");
    let pid: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO products (name, list_price, unit) VALUES ('guard-test-rice-2', 250, 'g')
         RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("insert product");
    let sid: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO sales (customer_id, total)
         VALUES ('00000000-0000-0000-0000-000000000001', 3750) RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("insert sale");
    let iid: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO sale_items (sale_id, product_id, product_name, quantity, unit_price, line_total, unit)
         VALUES ($1, $2, 'guard-test-rice-2', 1500, 250, 3750, 'g') RETURNING id",
    )
    .bind(sid.0)
    .bind(pid.0)
    .fetch_one(&mut *tx)
    .await
    .expect("insert sale item");
    let cid: (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO customers (name) VALUES ('guard-test-other') RETURNING id")
            .fetch_one(&mut *tx)
            .await
            .expect("insert second customer");

    // Probe helper: run one expected-to-fail statement against a savepoint,
    // then recover the transaction. Returns the statement's result.
    // (Declared as a macro to keep the call sites one-liners.)
    macro_rules! probe_denied {
        ($label:expr, $sql:expr $(, $bind:expr)*) => {{
            sqlx::query("SAVEPOINT probe").execute(&mut *tx).await.expect("savepoint");
            let result = sqlx::query($sql) $(.bind($bind))*.execute(&mut *tx).await;
            sqlx::query("ROLLBACK TO SAVEPOINT probe")
                .execute(&mut *tx)
                .await
                .expect("rollback to savepoint");
            assert!(result.is_err(), "{} was allowed", $label);
        }};
    }

    probe_denied!(
        "sale item update",
        "UPDATE sale_items SET quantity = 1 WHERE id = $1",
        iid.0
    );
    probe_denied!(
        "sale item delete",
        "DELETE FROM sale_items WHERE id = $1",
        iid.0
    );
    probe_denied!(
        "sale total change",
        "UPDATE sales SET total = 1 WHERE id = $1",
        sid.0
    );
    probe_denied!(
        "sale customer change",
        "UPDATE sales SET customer_id = $2 WHERE id = $1",
        sid.0,
        cid.0
    );

    // The status transition (reversal) must still work.
    let transition =
        sqlx::query("UPDATE sales SET status = 'cancel', reversed_at = now() WHERE id = $1")
            .bind(sid.0)
            .execute(&mut *tx)
            .await;
    assert!(transition.is_ok(), "status transition was blocked");

    tx.rollback().await.expect("rollback");
}

// v1 (P1) — the stock ledger is append-only and reason-coherent.
// plan-v1.md: "Stock is never a mutable `quantity` field ... every change is an
// append-only row", plus decision 15: the database enforces the history guards.
// Ignored by default: needs a live database.
#[tokio::test]
#[ignore]
async fn v1_ledger_is_append_only_and_reason_coherent() {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");

    let mut tx = pool.begin().await.expect("begin");

    let pid: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO products (name, list_price, unit) VALUES ('p1-ledger-probe', 250, 'g')
         RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("insert product");
    let mid: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO stock_movements (product_id, quantity, reason) VALUES ($1, 1500, 'initial')
         RETURNING id",
    )
    .bind(pid.0)
    .fetch_one(&mut *tx)
    .await
    .expect("insert initial movement");
    let sid: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO sales (customer_id, total)
         VALUES ('00000000-0000-0000-0000-000000000001', 3750) RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("insert sale");

    // One expected-to-fail statement per rule, each isolated in a SAVEPOINT:
    // in Postgres a failed statement aborts the whole transaction (25P02), so
    // without the savepoint every probe after the first would fail for the
    // wrong reason.
    macro_rules! probe_denied {
        ($label:expr, $sql:expr $(, $bind:expr)*) => {{
            sqlx::query("SAVEPOINT probe")
                .execute(&mut *tx)
                .await
                .expect("savepoint");
            let result = sqlx::query($sql) $(.bind($bind))*.execute(&mut *tx).await;
            sqlx::query("ROLLBACK TO SAVEPOINT probe")
                .execute(&mut *tx)
                .await
                .expect("rollback to savepoint");
            assert!(result.is_err(), "{} was allowed", $label);
        }};
    }

    probe_denied!(
        "ledger update",
        "UPDATE stock_movements SET quantity = 999999 WHERE id = $1",
        mid.0
    );
    probe_denied!(
        "ledger delete",
        "DELETE FROM stock_movements WHERE id = $1",
        mid.0
    );
    probe_denied!(
        "unknown movement reason",
        "INSERT INTO stock_movements (product_id, quantity, reason) VALUES ($1, 5, 'theft')",
        pid.0
    );
    probe_denied!(
        "sale movement without its sale",
        "INSERT INTO stock_movements (product_id, quantity, reason) VALUES ($1, 5, 'sale')",
        pid.0
    );
    probe_denied!(
        "initial movement carrying a sale",
        "INSERT INTO stock_movements (product_id, quantity, reason, sale_id) VALUES ($1, 5, 'initial', $2)",
        pid.0,
        sid.0
    );
    probe_denied!(
        "deleting a product that has history",
        "DELETE FROM products WHERE id = $1",
        pid.0
    );
    // TRUNCATE is a set operation: a FOR EACH ROW trigger never fires for it,
    // so it needs its own statement-level guard. Without it the whole ledger
    // can be emptied in one statement that no row trigger ever sees.
    probe_denied!("truncating the ledger", "TRUNCATE stock_movements");

    // Every rejection must leave the ledger unchanged: the product keeps
    // exactly its one initial movement and nothing else landed on it.
    let rows: (i64,) = sqlx::query_as("SELECT count(*) FROM stock_movements WHERE product_id = $1")
        .bind(pid.0)
        .fetch_one(&mut *tx)
        .await
        .expect("count movements");
    assert_eq!(rows.0, 1, "a rejected write changed the ledger");

    tx.rollback().await.expect("rollback");
}

// v1 (P3) — a bonus line is a real line whose price is zero.
// The money arithmetic and the database constraints have to agree: if the
// handler accepts a zero line but the schema still says `> 0`, the insert
// fails and the caller gets a 500 instead of a created sale.
// Ignored by default: needs the live stack.
#[tokio::test]
#[ignore]
async fn v1_sale_of_bonus_lines_is_recorded() {
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");
    let client = reqwest::Client::new();
    let suffix = uuid::Uuid::new_v4().simple();

    // A product with stock, so the bonus movement has something to take.
    let gift_name = format!("p3-gift-{suffix}");
    let product: serde_json::Value = client
        .post(format!("{base}/api/products"))
        .json(&serde_json::json!({
            "name": gift_name, "list_price": 250, "unit": "unit", "initial_stock": 10
        }))
        .send()
        .await
        .expect("post product")
        .json()
        .await
        .expect("product json");

    let cash_before: (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(total), 0)::bigint FROM sales WHERE status = 'confirmed'",
    )
    .fetch_one(&pool)
    .await
    .expect("cash before");

    // Three units, given away: the price override is zero.
    let resp = client
        .post(format!("{base}/api/sales"))
        .json(&serde_json::json!({
            "items": [{ "product_id": product["id"], "quantity": 3, "unit_price": 0 }]
        }))
        .send()
        .await
        .expect("post sale");
    assert_eq!(
        resp.status(),
        201,
        "a bonus sale must be created, not rejected by the schema"
    );
    let sale: serde_json::Value = resp.json().await.expect("sale json");

    // The sale exists and is worth nothing.
    assert_eq!(sale["total"], 0, "a bonus sale totals zero");
    assert_eq!(sale["status"], "confirmed");
    let lines = sale["items"].as_array().expect("items");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["unit_price"], 0, "the bonus price is zero");
    assert_eq!(lines[0]["line_total"], 0, "and so is its line total");

    // The stock left the shop even though no money came in: that is the whole
    // point of recording a bonus line instead of silently ignoring it.
    let product_id: uuid::Uuid = product["id"].as_str().expect("id").parse().expect("uuid");
    let stock: (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(quantity), 0) FROM stock_movements WHERE product_id = $1",
    )
    .bind(product_id)
    .fetch_one(&pool)
    .await
    .expect("stock");
    assert_eq!(stock.0, 7, "10 in, 3 given away");

    // A zero-total sale contributes zero to cash, so the balance is untouched.
    let cash_after: (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(total), 0)::bigint FROM sales WHERE status = 'confirmed'",
    )
    .fetch_one(&pool)
    .await
    .expect("cash after");
    assert_eq!(cash_after.0, cash_before.0, "a bonus sale moves no money");
}

// v1 writes (work-unit 3): a multi-item sale lands in one transaction with its
// stock movements. Unique product names keep repeated runs collision-free.
// Ignored by default: needs the live stack (backend + db).
#[tokio::test]
#[ignore]
async fn v1_create_sale_drops_stock_in_one_transaction() {
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");

    let suffix = uuid::Uuid::new_v4().simple();
    let rice_name = format!("w3-rice-{suffix}");
    let rice: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO products (name, list_price, unit) VALUES ($1, 250, 'g') RETURNING id",
    )
    .bind(&rice_name)
    .fetch_one(&pool)
    .await
    .expect("insert rice");
    sqlx::query(
        "INSERT INTO stock_movements (product_id, quantity, reason) VALUES ($1, 2000, 'initial')",
    )
    .bind(rice.0)
    .execute(&pool)
    .await
    .expect("rice initial stock");

    let beans_name = format!("w3-beans-{suffix}");
    let beans: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO products (name, list_price, unit) VALUES ($1, 100, 'unit') RETURNING id",
    )
    .bind(&beans_name)
    .fetch_one(&pool)
    .await
    .expect("insert beans");
    sqlx::query(
        "INSERT INTO stock_movements (product_id, quantity, reason) VALUES ($1, 10, 'initial')",
    )
    .bind(beans.0)
    .execute(&pool)
    .await
    .expect("beans initial stock");

    // Cash measured as a delta: prior runs' sales must not matter.
    let cash_before: (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(total), 0)::bigint FROM sales WHERE status = 'confirmed'",
    )
    .fetch_one(&pool)
    .await
    .expect("cash before");

    // Two items in one sale: no customer (walk-in), second line overrides the price.
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{base}/api/sales"))
        .json(&serde_json::json!({
            "items": [
                { "product_id": rice.0, "quantity": 1500 },
                { "product_id": beans.0, "quantity": 3, "unit_price": 200 }
            ]
        }))
        .send()
        .await
        .expect("post");
    assert_eq!(resp.status(), 201, "sale was not created");
    let sale: serde_json::Value = resp.json().await.expect("json");

    // Header: total = 1500×250 + 3×200 = 375600 cents, confirmed, on the walk-in.
    assert_eq!(sale["total"], 375600);
    assert_eq!(sale["status"], "confirmed");
    assert_eq!(sale["customer_id"], "00000000-0000-0000-0000-000000000001");
    let lines = sale["items"].as_array().expect("items array");
    assert_eq!(lines.len(), 2, "expected two sale lines");

    // Lines snapshot name and unit; line totals follow the money rules.
    for line in lines {
        let qty = line["quantity"].as_i64().expect("quantity");
        let price = line["unit_price"].as_i64().expect("unit_price");
        assert_eq!(
            line["line_total"].as_i64().expect("line_total"),
            qty * price
        );
    }
    assert!(
        lines
            .iter()
            .any(|l| l["product_name"] == rice_name && l["unit"] == "g")
    );
    assert!(
        lines
            .iter()
            .any(|l| l["product_name"] == beans_name && l["unit"] == "unit")
    );

    // Ledger: exactly one negative movement per line, on this sale.
    let sale_id: uuid::Uuid = sale["id"]
        .as_str()
        .expect("sale id")
        .parse()
        .expect("sale id uuid");
    let rice_moves: (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(quantity), 0) FROM stock_movements
         WHERE product_id = $1 AND reason = 'sale' AND sale_id = $2",
    )
    .bind(rice.0)
    .bind(sale_id)
    .fetch_one(&pool)
    .await
    .expect("rice movements");
    assert_eq!(rice_moves.0, -1500);
    let beans_moves: (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(quantity), 0) FROM stock_movements
         WHERE product_id = $1 AND reason = 'sale' AND sale_id = $2",
    )
    .bind(beans.0)
    .bind(sale_id)
    .fetch_one(&pool)
    .await
    .expect("beans movements");
    assert_eq!(beans_moves.0, -3);

    // Derived stock: 2000 − 1500 = 500 and 10 − 3 = 7.
    let rice_stock: (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(quantity), 0) FROM stock_movements WHERE product_id = $1",
    )
    .bind(rice.0)
    .fetch_one(&pool)
    .await
    .expect("rice stock");
    assert_eq!(rice_stock.0, 500);
    let beans_stock: (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(quantity), 0) FROM stock_movements WHERE product_id = $1",
    )
    .bind(beans.0)
    .fetch_one(&pool)
    .await
    .expect("beans stock");
    assert_eq!(beans_stock.0, 7);

    // Cash moved by exactly the sale total.
    let cash_after: (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(total), 0)::bigint FROM sales WHERE status = 'confirmed'",
    )
    .fetch_one(&pool)
    .await
    .expect("cash after");
    assert_eq!(cash_after.0 - cash_before.0, 375600);
}

// v1 reads (work-unit 4): the read surface reflects what the writes put in.
// Ignored by default: needs the live stack.
#[tokio::test]
#[ignore]
async fn v1_read_endpoints_reflect_writes() {
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");
    let client = reqwest::Client::new();
    let suffix = uuid::Uuid::new_v4().simple();

    // Two products created through the API, with initial stock.
    let rice_name = format!("w4-rice-{suffix}");
    let resp = client
        .post(format!("{base}/api/products"))
        .json(&serde_json::json!({
            "name": rice_name, "list_price": 250, "unit": "g", "initial_stock": 2000
        }))
        .send()
        .await
        .expect("post rice");
    assert_eq!(resp.status(), 201);
    let rice: serde_json::Value = resp.json().await.expect("rice json");
    assert_eq!(rice["stock"], 2000, "initial stock not reflected");

    let beans_name = format!("w4-beans-{suffix}");
    let resp = client
        .post(format!("{base}/api/products"))
        .json(&serde_json::json!({
            "name": beans_name, "list_price": 100, "unit": "unit", "initial_stock": 10
        }))
        .send()
        .await
        .expect("post beans");
    assert_eq!(resp.status(), 201);
    let beans: serde_json::Value = resp.json().await.expect("beans json");

    // A customer through the API.
    let customer_name = format!("w4-customer-{suffix}");
    let resp = client
        .post(format!("{base}/api/customers"))
        .json(&serde_json::json!({ "name": customer_name }))
        .send()
        .await
        .expect("post customer");
    assert_eq!(resp.status(), 201);
    let created: serde_json::Value = resp.json().await.expect("customer json");
    let customer_id = created["id"].as_str().expect("customer id");

    // A sale with both products; the first line overrides its price.
    let resp = client
        .post(format!("{base}/api/sales"))
        .json(&serde_json::json!({
            "customer_id": customer_id,
            "items": [
                { "product_id": rice["id"], "quantity": 1500, "unit_price": 200 },
                { "product_id": beans["id"], "quantity": 3 }
            ]
        }))
        .send()
        .await
        .expect("post sale");
    assert_eq!(resp.status(), 201);
    let sale: serde_json::Value = resp.json().await.expect("sale json");
    let expected_total: i64 = 1500 * 200 + 3 * 100;
    assert_eq!(sale["total"], expected_total);

    // GET /api/products: derived stock reflects the ledger, not a stored field.
    let products: serde_json::Value = client
        .get(format!("{base}/api/products"))
        .send()
        .await
        .expect("get products")
        .json()
        .await
        .expect("products json");
    let products = products.as_array().expect("products array");
    let rice_read = products
        .iter()
        .find(|p| p["name"] == rice_name)
        .expect("rice in list");
    assert_eq!(rice_read["stock"], 500, "stock must be 2000 - 1500");
    assert_eq!(rice_read["unit"], "g");
    assert_eq!(rice_read["list_price"], 250);
    let beans_read = products
        .iter()
        .find(|p| p["name"] == beans_name)
        .expect("beans in list");
    assert_eq!(beans_read["stock"], 7, "stock must be 10 - 3");

    // GET /api/customers: the new customer and the seeded walk-in are both there.
    let customers: serde_json::Value = client
        .get(format!("{base}/api/customers"))
        .send()
        .await
        .expect("get customers")
        .json()
        .await
        .expect("customers json");
    let customers = customers.as_array().expect("customers array");
    assert!(customers.iter().any(|c| c["name"] == customer_name));
    assert!(
        customers
            .iter()
            .any(|c| c["name"] == "walk-in" && c["id"] == "00000000-0000-0000-0000-000000000001")
    );

    // GET /api/sales: the sale appears with its nested lines and customer name.
    let sales: serde_json::Value = client
        .get(format!("{base}/api/sales?limit=50"))
        .send()
        .await
        .expect("get sales")
        .json()
        .await
        .expect("sales json");
    let sales = sales.as_array().expect("sales array");
    let sale_read = sales
        .iter()
        .find(|s| s["id"] == sale["id"])
        .expect("sale in list");
    assert_eq!(sale_read["customer_name"], customer_name);
    assert_eq!(sale_read["status"], "confirmed");
    let lines = sale_read["items"].as_array().expect("lines");
    assert_eq!(lines.len(), 2);
    assert!(
        lines
            .iter()
            .any(|l| l["product_name"] == rice_name && l["unit"] == "g")
    );
    assert!(lines.iter().any(|l| l["product_name"] == beans_name));

    // GET /api/cash: the total moved by exactly this sale.
    let cash_before: (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(total), 0)::bigint - $1 FROM sales WHERE status = 'confirmed'",
    )
    .bind(expected_total)
    .fetch_one(&pool)
    .await
    .expect("cash arithmetic");
    let cash: serde_json::Value = client
        .get(format!("{base}/api/cash"))
        .send()
        .await
        .expect("get cash")
        .json()
        .await
        .expect("cash json");
    assert_eq!(cash["total_cents"], cash_before.0 + expected_total);
}

// A product created without initial stock starts at 0 with no movement row.
// Ignored by default: needs the live stack.
#[tokio::test]
#[ignore]
async fn v1_product_without_initial_stock_starts_at_zero() {
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");
    let client = reqwest::Client::new();
    let name = format!("w4-empty-{}", uuid::Uuid::new_v4().simple());

    let product: serde_json::Value = client
        .post(format!("{base}/api/products"))
        .json(&serde_json::json!({ "name": name, "list_price": 100 }))
        .send()
        .await
        .expect("post product")
        .json()
        .await
        .expect("product json");
    assert_eq!(product["stock"], 0);
    assert_eq!(product["unit"], "unit", "unit must default to 'unit'");

    let moves: (i64,) =
        sqlx::query_as("SELECT count(*) FROM stock_movements WHERE product_id = $1")
            .bind(
                product["id"]
                    .as_str()
                    .expect("id")
                    .parse::<uuid::Uuid>()
                    .expect("uuid"),
            )
            .fetch_one(&pool)
            .await
            .expect("movement count");
    assert_eq!(
        moves.0, 0,
        "no movement row should exist for zero initial stock"
    );
}

// v1a (work-unit 1, S2) — GET /api/sales is scoped by an explicit date range
// and offset-paginated. `from` is inclusive and `to` is exclusive, so adjacent
// days never double-count; absent bounds keep the recent-50 behaviour.
// The window lives in 2001 so rows other tests created never interfere.
// Ignored by default: needs the live stack (backend + db).
#[tokio::test]
#[ignore]
async fn v1a_sales_list_is_scoped_by_date_and_offset() {
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");
    let client = reqwest::Client::new();

    // Three documents around one midnight boundary, plus one at the exact
    // boundary instant. The boundary row is the whole point: it belongs to the
    // later day and to that day only.
    let day = "2001-03-04";
    let next = "2001-03-05";
    let bounds = (
        format!("{day}T00:00:00Z"),
        format!("{next}T00:00:00Z"),
        "2001-03-06T00:00:00Z".to_string(),
    );

    let insert = |created_at: &str| {
        let created_at = created_at.to_string();
        let pool = pool.clone();
        async move {
            let row: (uuid::Uuid,) = sqlx::query_as(
                "INSERT INTO sales (customer_id, total, created_at)
                 VALUES ('00000000-0000-0000-0000-000000000001', 100, $1::timestamptz) RETURNING id",
            )
            .bind(created_at)
            .fetch_one(&pool)
            .await
            .expect("insert sale");
            row.0
        }
    };

    // This test asserts absolute counts inside its window, so the window has
    // to start empty: a previous run's probe rows would otherwise be counted as
    // this run's. Probe sales carry no lines and no movements, so deleting them
    // touches neither the ledger nor another test's data.
    let clear_window = || {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "DELETE FROM sales WHERE created_at >= $1::timestamptz AND created_at < $2::timestamptz",
            )
            .bind("2001-03-04T00:00:00Z")
            .bind("2001-03-06T00:00:00Z")
            .execute(&pool)
            .await
            .expect("clear probe window")
        }
    };
    clear_window().await;

    let late_today = insert("2001-03-04T23:30:00Z").await;
    let midnight = insert("2001-03-05T00:00:00Z").await;
    let early_next = insert("2001-03-05T00:30:00Z").await;
    let noon_next = insert("2001-03-05T12:00:00Z").await;

    let ids_of = |body: &serde_json::Value| -> Vec<String> {
        body.as_array()
            .expect("array")
            .iter()
            .map(|s| s["id"].as_str().expect("id").to_string())
            .collect()
    };

    let get = |query: String| {
        let client = client.clone();
        let base = base.clone();
        async move {
            let resp = client
                .get(format!("{base}/api/sales?{query}"))
                .send()
                .await
                .expect("get");
            assert!(resp.status().is_success(), "list failed: {}", resp.status());
            resp.json::<serde_json::Value>().await.expect("json")
        }
    };

    // Day one: `to` is exclusive, so the midnight document is not here.
    let day_one = get(format!("from={}&to={}&limit=200", bounds.0, bounds.1)).await;
    let day_one_ids = ids_of(&day_one);
    assert!(
        day_one_ids.contains(&late_today.to_string()),
        "the day's last sale is missing"
    );
    assert!(
        !day_one_ids.contains(&midnight.to_string()),
        "`to` must be exclusive: the midnight document belongs to the next day"
    );

    // Day two: `from` is inclusive, so the midnight document is here, with the
    // two later ones. Nothing from day one follows it across.
    let day_two = get(format!("from={}&to={}&limit=200", bounds.1, bounds.2)).await;
    let day_two_ids = ids_of(&day_two);
    assert!(
        day_two_ids.contains(&midnight.to_string()),
        "`from` must be inclusive: the midnight document belongs to this day"
    );
    assert!(day_two_ids.contains(&early_next.to_string()));
    assert!(day_two_ids.contains(&noon_next.to_string()));
    assert!(
        !day_two_ids.contains(&late_today.to_string()),
        "an adjacent day's sale leaked into this range"
    );

    // Adjacent days partition the boundary: no document is counted twice and
    // none is lost across the two ranges that meet at midnight.
    assert_eq!(day_one_ids.len(), 1, "day one holds exactly its own sale");
    assert_eq!(day_two_ids.len(), 3, "day two holds exactly its own sales");
    assert!(
        !day_one_ids.contains(&early_next.to_string()),
        "the boundary was double-counted"
    );

    // Order is newest first, whatever the range.
    let ordered = get(format!("from={}&to={}&limit=200", bounds.1, bounds.2)).await;
    let ordered_ids = ids_of(&ordered);
    assert_eq!(
        ordered_ids,
        vec![
            noon_next.to_string(),
            early_next.to_string(),
            midnight.to_string()
        ],
        "the ledger must read newest first"
    );

    // offset pages through the same range without repeating a row.
    let offset_one = get(format!(
        "from={}&to={}&limit=2&offset=1",
        bounds.1, bounds.2
    ))
    .await;
    assert_eq!(
        ids_of(&offset_one),
        vec![early_next.to_string(), midnight.to_string()],
        "offset 1 of the same range must resume after the first row"
    );

    // No bounds: the date filter is not applied, so the old window of recent
    // documents still reaches this 2001 row. That is the behaviour v1 shipped.
    let unbounded = get("limit=200".to_string()).await;
    assert!(
        ids_of(&unbounded).contains(&late_today.to_string()),
        "absent bounds must keep the unscoped recent-documents behaviour"
    );

    // limit clamps to 1..200: zero is not an empty page, it is one row.
    let clamped = get(format!("from={}&to={}&limit=0", bounds.1, bounds.2)).await;
    assert_eq!(
        clamped.as_array().expect("array").len(),
        1,
        "limit must clamp up to 1"
    );

    // The shape the frontend reads is unchanged: rows still carry the sale,
    // its customer's name and its nested lines.
    let row = &day_one.as_array().expect("array")[0];
    assert!(row["customer_name"].is_string(), "customer_name is missing");
    assert!(row["items"].is_array(), "items is missing");
    assert_eq!(row["status"], "confirmed");

    // Nothing above may have written: the probe rows are the only ones this
    // test added, and they are still exactly four.
    let mine: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM sales WHERE created_at >= $1::timestamptz AND created_at < $2::timestamptz",
    )
    .bind("2001-03-04T00:00:00Z")
    .bind("2001-03-06T00:00:00Z")
    .fetch_one(&pool)
    .await
    .expect("probe count");
    assert_eq!(mine.0, 4, "listing a range must not change stored sales");

    // Leave the window as it was found.
    clear_window().await;
}

// v1a (work-unit 2, S3) — GET /api/products is searchable by name and
// offset-paginated, while stock stays derived and the name ordering holds.
// Ignored by default: needs the live stack (backend + db).
#[tokio::test]
#[ignore]
async fn v1a_products_list_is_searchable_and_paginated() {
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");
    let client = reqwest::Client::new();

    // A marker no other row can carry, so the assertions never depend on what
    // the rest of the suite left in the catalog. Mixed case on purpose: `q` is
    // a case-insensitive substring match, so a lowercase needle must find it.
    let suffix = uuid::Uuid::new_v4().simple();
    let marker = format!("v1a-q-{suffix}");
    let names = [
        format!("{marker}-Alfajor"),
        format!("{marker}-banana"),
        format!("{marker}-Cafe"),
    ];

    for (idx, name) in names.iter().enumerate() {
        sqlx::query(
            "INSERT INTO products (name, list_price, unit) VALUES ($1, $2, 'unit') RETURNING id",
        )
        .bind(name)
        .bind(100 + idx as i64)
        .execute(&pool)
        .await
        .expect("insert product");
    }
    // One more with the marker but a different substring, to prove the match
    // is on the whole name and not just the prefix.
    let decoy = format!("zzz-{marker}-Yerba");
    sqlx::query("INSERT INTO products (name, list_price, unit) VALUES ($1, 100, 'unit')")
        .bind(&decoy)
        .execute(&pool)
        .await
        .expect("insert decoy");

    let get = |query: String| {
        let client = client.clone();
        let base = base.clone();
        async move {
            let resp = client
                .get(format!("{base}/api/products?{query}"))
                .send()
                .await
                .expect("get");
            assert!(resp.status().is_success(), "list failed: {}", resp.status());
            resp.json::<serde_json::Value>().await.expect("json")
        }
    };

    let names_of = |body: &serde_json::Value| -> Vec<String> {
        body.as_array()
            .expect("array")
            .iter()
            .map(|p| p["name"].as_str().expect("name").to_string())
            .collect()
    };

    // `q` is a case-insensitive substring match over the whole name, so the
    // decoy is found too; the three marker rows and it are the complete set.
    let found = get(format!("q={marker}&limit=200")).await;
    let found_names = names_of(&found);
    assert_eq!(
        found_names.len(),
        4,
        "substring match must find all four rows, found: {found_names:?}"
    );
    assert!(found_names.contains(&decoy), "the decoy was not matched");
    assert!(
        !names_of(&get(format!("q={marker}xx&limit=200")).await)
            .iter()
            .any(|n| n.contains(&marker)),
        "a needle that matches nothing must return nothing"
    );

    // Ordering by name is preserved under a filter.
    let mut expected = names.to_vec();
    expected.push(decoy.clone());
    expected.sort();
    assert_eq!(
        names_of(&get(format!("q={marker}&limit=200")).await),
        expected,
        "a filtered catalog must still read in name order"
    );

    // Two adjacent pages of the same query partition the result: no row is
    // repeated across them and none is skipped.
    let first = get(format!("q={marker}&limit=2&offset=0")).await;
    let second = get(format!("q={marker}&limit=2&offset=2")).await;
    let first_names = names_of(&first);
    let second_names = names_of(&second);
    assert_eq!(first_names.len(), 2, "limit must cap the page");
    assert_eq!(second_names.len(), 2, "offset must resume the same query");
    assert!(
        first_names.iter().all(|n| !second_names.contains(n)),
        "adjacent pages repeated a row: {first_names:?} then {second_names:?}"
    );
    let mut paged = first_names.clone();
    paged.extend(second_names.clone());
    paged.sort();
    assert_eq!(
        paged, expected,
        "the two pages must reconstruct the whole filtered set"
    );

    // The offset is a real one, not a synonym for the first page.
    let beyond = get(format!("q={marker}&limit=200&offset=4")).await;
    assert!(
        names_of(&beyond).is_empty(),
        "an offset past the end must return an empty page"
    );

    // Absent `q` lists the catalog: the unscoped call still returns rows.
    let unscoped = get("limit=5".to_string()).await;
    assert!(
        !names_of(&unscoped).is_empty(),
        "absent q must keep listing the catalog"
    );

    // Derived stock and unit survive the filter: each new product starts at 0
    // and no list query may invent a movement.
    let row = &found.as_array().expect("array")[0];
    assert_eq!(row["stock"], 0, "a fresh product starts at zero stock");
    assert_eq!(row["unit"], "unit");
    assert_eq!(
        row["list_price"], 100,
        "the list price reads back unchanged"
    );
    let movements: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM stock_movements m
         JOIN products p ON p.id = m.product_id
         WHERE p.name LIKE $1",
    )
    .bind(format!("%{marker}%"))
    .fetch_one(&pool)
    .await
    .expect("movement count");
    assert_eq!(movements.0, 0, "listing products must not write movements");
}
