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

// v1 units + history guards, as amended by 0006: quantities are integers in the
// product's base unit, the unit itself is decorative, and the database enforces
// the immutability rules.
//
// 0003 locked a product's unit once it had history, reasoning that its
// movements and sale lines were "denominated in the old unit". 0006 dropped that
// trigger because the unit no longer qualifies a price, so the assertions below
// are inverted on purpose: a product WITH history may now change its unit, and
// what remains guarded is the line/economics immutability.
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

    // Scenario 1: a product WITH history may change its unit (0006 removed the
    // lock). The unit does not denominate anything, so no history is rewritten.
    // The transaction is dropped without commit, so it rolls back.
    {
        let mut tx = pool.begin().await.expect("begin");
        let pid: (uuid::Uuid,) = sqlx::query_as(
            "INSERT INTO products (name, list_price, unit) VALUES ('guard-test-rice', 2.50, 'g')
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
        let changed = sqlx::query("UPDATE products SET unit = 'unit' WHERE id = $1")
            .bind(pid.0)
            .execute(&mut *tx)
            .await;
        assert!(
            changed.is_ok(),
            "0006 removed guard_product_unit: the unit is decorative and must be changeable"
        );
    }

    // Scenario 2: the guard function is gone, and with it the trigger.
    {
        let trigger: (i64,) =
            sqlx::query_as("SELECT count(*) FROM pg_trigger WHERE tgname = 'products_unit_guard'")
                .fetch_one(&pool)
                .await
                .expect("trigger check");
        assert_eq!(trigger.0, 0, "products_unit_guard still exists after 0006");
        let function: (i64,) =
            sqlx::query_as("SELECT count(*) FROM pg_proc WHERE proname = 'guard_product_unit'")
                .fetch_one(&pool)
                .await
                .expect("function check");
        assert_eq!(function.0, 0, "guard_product_unit still exists after 0006");
    }

    // The `unit` columns themselves survive: the ledger still snapshots one.

    // Scenario 3: sale lines are immutable and a sale's economics are guarded.
    // Each expected failure runs inside a SAVEPOINT and rolls back to it:
    // in Postgres a failed statement aborts the whole transaction (25P02),
    // so without the savepoint the remaining probes would fail for the wrong
    // reason and the allowed transition would never get a fair check.
    let mut tx = pool.begin().await.expect("begin");
    let pid: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO products (name, list_price, unit) VALUES ('guard-test-rice-2', 2.50, 'g')
         RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("insert product");
    let sid: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO sales (customer_id, total)
         VALUES ('00000000-0000-0000-0000-000000000001', 3750.00) RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("insert sale");
    let iid: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO sale_items (sale_id, product_id, product_name, quantity, unit_price, line_total, unit)
         VALUES ($1, $2, 'guard-test-rice-2', 1500, 2.50, 3750.00, 'g') RETURNING id",
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
        "UPDATE sales SET total = 1.00 WHERE id = $1",
        sid.0
    );
    probe_denied!(
        "a price finer than a cent",
        "INSERT INTO sale_items (sale_id, product_id, product_name, quantity, unit_price, line_total, unit)
         VALUES ($1, $2, 'x', 1, 0.012, 0.012, 'g')",
        sid.0,
        pid.0
    );
    // A value finer than a cent has no bind parameter, so it is probed directly
    // rather than through the macro (whose pattern requires one).
    {
        sqlx::query("SAVEPOINT probe")
            .execute(&mut *tx)
            .await
            .expect("savepoint");
        let result = sqlx::query(
            "INSERT INTO sales (customer_id, total)
             VALUES ('00000000-0000-0000-0000-000000000001', 1.235)",
        )
        .execute(&mut *tx)
        .await;
        sqlx::query("ROLLBACK TO SAVEPOINT probe")
            .execute(&mut *tx)
            .await
            .expect("rollback to savepoint");
        assert!(
            result.is_err(),
            "a sale total finer than a cent was allowed: the guard is not working"
        );
    }
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
            "name": gift_name, "list_price": "2.50", "unit": "unit", "initial_stock": 10
        }))
        .send()
        .await
        .expect("post product")
        .json()
        .await
        .expect("product json");
    assert_eq!(
        product["list_price"], "2.50",
        "a decimal string with two decimals comes back"
    );

    let cash_before: (rust_decimal::Decimal,) = sqlx::query_as(
        "SELECT COALESCE(SUM(total), 0)::numeric FROM sales WHERE status = 'confirmed'",
    )
    .fetch_one(&pool)
    .await
    .expect("cash before");

    // Three units, given away: the price override is zero.
    let resp = client
        .post(format!("{base}/api/sales"))
        .json(&serde_json::json!({
            "items": [{ "product_id": product["id"], "quantity": 3, "unit_price": "0" }]
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

    // The sale exists and is worth nothing. Amounts are decimal strings: `0.00`
    // is money, the number `0` would not be.
    assert_eq!(sale["total"], "0.00", "a bonus sale totals zero");
    assert_eq!(sale["status"], "confirmed");
    let lines = sale["items"].as_array().expect("items");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["unit_price"], "0.00", "the bonus price is zero");
    assert_eq!(lines[0]["line_total"], "0.00", "and so is its line total");

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
    let cash_after: (rust_decimal::Decimal,) = sqlx::query_as(
        "SELECT COALESCE(SUM(total), 0)::numeric FROM sales WHERE status = 'confirmed'",
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
        "INSERT INTO products (name, list_price, unit) VALUES ($1, 2.50, 'g') RETURNING id",
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
        "INSERT INTO products (name, list_price, unit) VALUES ($1, 1.00, 'unit') RETURNING id",
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
    let cash_before: (rust_decimal::Decimal,) = sqlx::query_as(
        "SELECT COALESCE(SUM(total), 0)::numeric FROM sales WHERE status = 'confirmed'",
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
                { "product_id": beans.0, "quantity": 3, "unit_price": "2.00" }
            ]
        }))
        .send()
        .await
        .expect("post");
    assert_eq!(resp.status(), 201, "sale was not created");
    let sale: serde_json::Value = resp.json().await.expect("json");

    // Header: total = 1500 x $2.50 + 3 x $2.00 = $3756.00, confirmed, on the walk-in.
    assert_eq!(sale["total"], "3756.00");
    assert_eq!(sale["status"], "confirmed");
    assert_eq!(sale["customer_id"], "00000000-0000-0000-0000-000000000001");
    let lines = sale["items"].as_array().expect("items array");
    assert_eq!(lines.len(), 2, "expected two sale lines");

    // Lines snapshot name and unit; a line total is quantity x price, exactly.
    use rust_decimal::prelude::FromPrimitive;
    for line in lines {
        let qty = line["quantity"].as_i64().expect("quantity");
        let price = line["unit_price"]
            .as_str()
            .expect("unit_price is a decimal string");
        let price = rust_decimal::Decimal::from_str_exact(price).expect("decimal price");
        let expected = price * rust_decimal::Decimal::from_i64(qty).expect("qty");
        assert_eq!(
            line["line_total"].as_str().expect("line_total string"),
            expected.round_dp(2).to_string(),
            "line_total must be quantity x unit_price"
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
    let cash_after: (rust_decimal::Decimal,) = sqlx::query_as(
        "SELECT COALESCE(SUM(total), 0)::numeric FROM sales WHERE status = 'confirmed'",
    )
    .fetch_one(&pool)
    .await
    .expect("cash after");
    assert_eq!(
        cash_after.0 - cash_before.0,
        rust_decimal::Decimal::new(375600, 2),
        "cash moved by $3756.00"
    );
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
            "name": rice_name, "list_price": "2.50", "unit": "g", "initial_stock": 2000
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
            "name": beans_name, "list_price": "1.00", "unit": "unit", "initial_stock": 10
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
                { "product_id": rice["id"], "quantity": 1500, "unit_price": "2.00" },
                { "product_id": beans["id"], "quantity": 3 }
            ]
        }))
        .send()
        .await
        .expect("post sale");
    assert_eq!(resp.status(), 201);
    let sale: serde_json::Value = resp.json().await.expect("sale json");
    // 1500 x $2.00 + 3 x $1.00 = $3003.00
    let expected_total = "3003.00";
    assert_eq!(sale["total"], expected_total);

    // GET /api/products: derived stock reflects the ledger, not a stored field.
    // Each product is read back by name through the catalog search the screen
    // uses, not from the unpaginated list. That list is paginated and ordered by
    // name, so reading it whole made this test depend on how full the database
    // was: past one page of products, the row being asserted fell off the end
    // and the failure named the test rather than the cause.
    let read_product = |name: String| {
        let client = client.clone();
        let base = base.clone();
        async move {
            let rows: serde_json::Value = client
                .get(format!("{base}/api/products"))
                .query(&[("q", name.clone())])
                .send()
                .await
                .expect("get products")
                .json()
                .await
                .expect("products json");
            rows.as_array()
                .expect("products array")
                .iter()
                .find(|p| p["name"] == name.as_str())
                .cloned()
                .expect("product in search results")
        }
    };
    let rice_read = read_product(rice_name.clone()).await;
    assert_eq!(rice_read["stock"], 500, "stock must be 2000 - 1500");
    assert_eq!(rice_read["unit"], "g");
    assert_eq!(rice_read["list_price"], "2.50");
    let beans_read = read_product(beans_name.clone()).await;
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

    // GET /api/cash: the total moved by exactly this sale. Since v1.5 the
    // endpoint is net of credit notes, so the test's own arithmetic is too —
    // otherwise it would assert the old definition of a changed endpoint.
    let cash_before: (rust_decimal::Decimal,) = sqlx::query_as(
        "SELECT COALESCE((SELECT SUM(total) FROM sales WHERE status = 'confirmed'), 0)::numeric
              - COALESCE((SELECT SUM(total) FROM credit_notes), 0)::numeric
              - $1::numeric",
    )
    .bind(rust_decimal::Decimal::new(300300, 2))
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
    assert_eq!(
        cash["total"],
        format!(
            "{:.2}",
            cash_before.0 + rust_decimal::Decimal::new(300300, 2)
        ),
        "cash is a decimal string summing to the sales total"
    );
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
        .json(&serde_json::json!({ "name": name, "list_price": "1.00" }))
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
                 VALUES ('00000000-0000-0000-0000-000000000001', 1.00, $1::timestamptz) RETURNING id",
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
        .bind(rust_decimal::Decimal::new(100 + idx as i64, 2))
        .execute(&pool)
        .await
        .expect("insert product");
    }
    // One more with the marker but a different substring, to prove the match
    // is on the whole name and not just the prefix.
    let decoy = format!("zzz-{marker}-Yerba");
    sqlx::query("INSERT INTO products (name, list_price, unit) VALUES ($1, 1.00, 'unit')")
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
        row["list_price"], "1.00",
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

// ── v1.5 reversal (credit notes) ────────────────────────────────────────────
//
// S1–S9, S15. The reversal is a compensating document: the corrected sale keeps
// every byte it was recorded with, and the note is what returns stock and money.

/// Everything one refund probe needs: a fresh product, a sale of it, and the
/// sale's lines as the API returned them.
struct Reversal {
    sale_id: uuid::Uuid,
    /// (sale_item_id, product_id, quantity, unit_price)
    lines: Vec<(uuid::Uuid, uuid::Uuid, i64, String)>,
}

/// Reads the cash figure the screen shows, straight from its endpoint.
async fn cash(base: &str) -> rust_decimal::Decimal {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{base}/api/cash"))
        .send()
        .await
        .expect("get cash");
    assert_eq!(resp.status(), 200, "cash endpoint is up");
    let body: serde_json::Value = resp.json().await.expect("cash json");
    rust_decimal::Decimal::from_str_exact(body["total"].as_str().expect("cash total string"))
        .expect("cash total is a decimal")
}

/// Refunds part of a sale through the API and returns the response, whatever
/// its status — the error probes want to see the refusal, not an unwind.
async fn post_refund(
    base: &str,
    sale_id: uuid::Uuid,
    items: serde_json::Value,
) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{base}/api/sales/{sale_id}/credit-notes"))
        .json(&serde_json::json!({ "items": items }))
        .send()
        .await
        .expect("post credit note")
}

/// Records one two-line sale and returns it ready to be corrected.
async fn reversal_fixture(base: &str, pool: &sqlx::PgPool, prefix: &str) -> Reversal {
    let suffix = uuid::Uuid::new_v4().simple();
    let marker = format!("{prefix}-{suffix}");

    let rice: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO products (name, list_price, unit) VALUES ($1, 2.50, 'g') RETURNING id",
    )
    .bind(format!("{marker}-rice"))
    .fetch_one(pool)
    .await
    .expect("insert rice");
    sqlx::query(
        "INSERT INTO stock_movements (product_id, quantity, reason) VALUES ($1, 1000, 'initial')",
    )
    .bind(rice.0)
    .execute(pool)
    .await
    .expect("rice initial");

    // A bonus line: priced at zero, it still moves stock and still comes back.
    let promo: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO products (name, list_price, unit) VALUES ($1, 0, 'unit') RETURNING id",
    )
    .bind(format!("{marker}-promo"))
    .fetch_one(pool)
    .await
    .expect("insert promo");
    sqlx::query(
        "INSERT INTO stock_movements (product_id, quantity, reason) VALUES ($1, 10, 'initial')",
    )
    .bind(promo.0)
    .execute(pool)
    .await
    .expect("promo initial");

    let resp = reqwest::Client::new()
        .post(format!("{base}/api/sales"))
        .json(&serde_json::json!({
            "items": [
                { "product_id": rice.0, "quantity": 400 },
                { "product_id": promo.0, "quantity": 2 }
            ]
        }))
        .send()
        .await
        .expect("post sale");
    assert_eq!(resp.status(), 201, "fixture sale was not created");
    let sale: serde_json::Value = resp.json().await.expect("sale json");

    let lines: Vec<(uuid::Uuid, uuid::Uuid, i64, String)> = sale["items"]
        .as_array()
        .expect("sale items")
        .iter()
        .map(|line| {
            (
                line["id"].as_str().expect("item id").parse().expect("uuid"),
                line["product_id"]
                    .as_str()
                    .expect("product id")
                    .parse()
                    .expect("uuid"),
                line["quantity"].as_i64().expect("quantity"),
                line["unit_price"].as_str().expect("price").to_string(),
            )
        })
        .collect();

    // The fixture is identifiable: this is how a human finds this run's rows in
    // a development database that cannot be cleaned row by row.
    for name in [format!("{marker}-rice"), format!("{marker}-promo")] {
        assert!(
            sale["items"]
                .as_array()
                .expect("items")
                .iter()
                .any(|line| line["product_name"] == name.as_str()),
            "the fixture sale must carry both snapshotted names"
        );
    }

    Reversal {
        sale_id: sale["id"].as_str().expect("sale id").parse().expect("uuid"),
        lines,
    }
}

// S1, S5, S6, S7: a partial refund is a new document that returns stock and
// money, and the sale it corrects is not written.
#[tokio::test]
#[ignore]
async fn v1_5_credit_note_returns_stock_and_money_without_touching_the_sale() {
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");
    let f = reversal_fixture(&base, &pool, "w5").await;
    let (rice_item, rice_product, rice_qty, rice_price) = f.lines[0].clone();

    let sale_before: (String, String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT status, total::text, reversed_at FROM sales WHERE id = $1")
            .bind(f.sale_id)
            .fetch_one(&pool)
            .await
            .expect("sale before");

    // Stock before: 1000 initial − 400 sold = 600.
    let stock_before: (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(quantity), 0) FROM stock_movements WHERE product_id = $1",
    )
    .bind(rice_product)
    .fetch_one(&pool)
    .await
    .expect("stock before");
    assert_eq!(stock_before.0, 600);

    let cash_before = cash(&base).await;

    // Refund 150 of the 400 grams: 150 x $2.50 = $375.00.
    let resp = post_refund(
        &base,
        f.sale_id,
        serde_json::json!([{ "sale_item_id": rice_item, "quantity": 150 }]),
    )
    .await;
    assert_eq!(resp.status(), 201, "a partial refund must be accepted");
    let note: serde_json::Value = resp.json().await.expect("note json");

    // The document: a total of what was charged for the returned units, at the
    // price the sale line snapshotted, and its line pointing back at that line.
    assert_eq!(note["total"], "375.00");
    assert_eq!(note["sale_id"], f.sale_id.to_string());
    let note_items = note["items"].as_array().expect("note items");
    assert_eq!(note_items.len(), 1);
    assert_eq!(note_items[0]["sale_item_id"], rice_item.to_string());
    assert_eq!(note_items[0]["quantity"], 150);
    assert_eq!(note_items[0]["unit_price"], rice_price.as_str());
    assert_eq!(note_items[0]["line_total"], "375.00");
    assert_eq!(note_items[0]["unit"], "g");
    let note_id: uuid::Uuid = note["id"].as_str().expect("note id").parse().expect("uuid");

    // The corrected sale is untouched, byte for byte.
    let sale_after: (String, String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT status, total::text, reversed_at FROM sales WHERE id = $1")
            .bind(f.sale_id)
            .fetch_one(&pool)
            .await
            .expect("sale after");
    assert_eq!(sale_after.0, sale_before.0, "status must not change");
    assert_eq!(sale_after.1, sale_before.1, "total must not change");
    assert_eq!(sale_after.2, sale_before.2, "reversed_at must stay NULL");

    // Stock came back: 600 + 150 = 750, as a new positive movement that names
    // both the sale and the note it belongs to.
    let stock_after: (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(quantity), 0) FROM stock_movements WHERE product_id = $1",
    )
    .bind(rice_product)
    .fetch_one(&pool)
    .await
    .expect("stock after");
    assert_eq!(
        stock_after.0, 750,
        "the refunded stock is back on the shelf"
    );
    let movement: (i32, uuid::Uuid, uuid::Uuid) = sqlx::query_as(
        "SELECT quantity, sale_id, credit_note_id FROM stock_movements
         WHERE credit_note_id = $1",
    )
    .bind(note_id)
    .fetch_one(&pool)
    .await
    .expect("reversal movement");
    assert_eq!(movement.0, 150, "a reversal movement is positive");
    assert_eq!(
        movement.1, f.sale_id,
        "the movement names the sale it reverses"
    );
    assert_eq!(movement.2, note_id, "and the note that caused it");

    // Cash is net of the refund.
    assert_eq!(
        cash_before - cash(&base).await,
        rust_decimal::Decimal::new(37500, 2),
        "cash must drop by the refunded amount"
    );

    // The sale's own movements were appended to, never edited.
    let sale_movements: (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(quantity), 0) FROM stock_movements
         WHERE sale_id = $1 AND reason = 'sale'",
    )
    .bind(f.sale_id)
    .fetch_one(&pool)
    .await
    .expect("sale movements");
    assert_eq!(
        sale_movements.0,
        -rice_qty - 2,
        "the sale's own rows are intact"
    );
}

// S8: the list carries the correction, so the screen can render it.
#[tokio::test]
#[ignore]
async fn v1_5_sales_list_carries_credit_notes_with_their_lines() {
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");
    let f = reversal_fixture(&base, &pool, "w5-list").await;
    let (rice_item, _, _, _) = f.lines[0].clone();

    // Before any refund the field is present and empty, never absent: the
    // client must not have to tell "no notes" from "the server forgot".
    let sale = find_sale(&base, f.sale_id).await;
    assert_eq!(
        sale["credit_notes"].as_array().expect("array").len(),
        0,
        "an uncorrected sale carries an empty array"
    );

    let resp = post_refund(
        &base,
        f.sale_id,
        serde_json::json!([{ "sale_item_id": rice_item, "quantity": 100 }]),
    )
    .await;
    assert_eq!(resp.status(), 201);

    let sale = find_sale(&base, f.sale_id).await;
    let notes = sale["credit_notes"].as_array().expect("credit_notes array");
    assert_eq!(notes.len(), 1, "the correction is listed under its sale");
    assert_eq!(notes[0]["total"], "250.00");
    // Amounts are decimal strings with two decimals, like every other amount.
    let items = notes[0]["items"].as_array().expect("note items");
    assert_eq!(items[0]["line_total"], "250.00");
    assert_eq!(items[0]["unit_price"], "2.50");
    assert!(
        notes[0]["created_at"].is_string(),
        "the note carries when it was issued"
    );
}

// S7: the refusals, and that a refused refund writes nothing at all.
#[tokio::test]
#[ignore]
async fn v1_5_refusals_are_distinct_and_write_nothing() {
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");
    let f = reversal_fixture(&base, &pool, "w5-refuse").await;
    let (rice_item, _, _, _) = f.lines[0].clone();

    // A sale that does not exist is a 404, not a 400 and not a 500.
    let nowhere = post_refund(
        &base,
        uuid::Uuid::new_v4(),
        serde_json::json!([{ "sale_item_id": rice_item, "quantity": 1 }]),
    )
    .await;
    assert_eq!(nowhere.status(), 404, "an unknown sale is a 404");

    // A line that belongs to another sale is also a 404: it is the wrong
    // document, not a conflict.
    let other = reversal_fixture(&base, &pool, "w5-other").await;
    let foreign = post_refund(
        &base,
        f.sale_id,
        serde_json::json!([{ "sale_item_id": other.lines[0].0, "quantity": 1 }]),
    )
    .await;
    assert_eq!(
        foreign.status(),
        404,
        "a foreign line is not refundable here"
    );

    // Shape errors: empty, zero, negative.
    for (label, items) in [
        ("empty", serde_json::json!([])),
        (
            "zero",
            serde_json::json!([{ "sale_item_id": rice_item, "quantity": 0 }]),
        ),
        (
            "negative",
            serde_json::json!([{ "sale_item_id": rice_item, "quantity": -5 }]),
        ),
    ] {
        let resp = post_refund(&base, f.sale_id, items).await;
        assert_eq!(resp.status(), 400, "{label} refund must be a 400");
    }

    // Before the over-refund probe: what the line actually sold, and the state
    // every refusal must leave exactly as it is.
    let sold: i32 = sqlx::query_scalar("SELECT quantity FROM sale_items WHERE id = $1")
        .bind(rice_item)
        .fetch_one(&pool)
        .await
        .expect("sold quantity");
    let notes_before: (i64,) =
        sqlx::query_as("SELECT count(*) FROM credit_notes WHERE sale_id = $1")
            .bind(f.sale_id)
            .fetch_one(&pool)
            .await
            .expect("notes before");
    let movements_before: (i64,) =
        sqlx::query_as("SELECT count(*) FROM stock_movements WHERE sale_id = $1")
            .bind(f.sale_id)
            .fetch_one(&pool)
            .await
            .expect("movements before");
    let cash_before = cash(&base).await;

    // S3: more than was sold is a 409.
    let too_much = post_refund(
        &base,
        f.sale_id,
        serde_json::json!([{ "sale_item_id": rice_item, "quantity": sold + 1 }]),
    )
    .await;
    assert_eq!(too_much.status(), 409, "over-refunding a line is a 409");

    // S7: and the refusal wrote nothing — no note, no note line, no movement,
    // and not one cent of cash movement.
    let notes_after: (i64,) =
        sqlx::query_as("SELECT count(*) FROM credit_notes WHERE sale_id = $1")
            .bind(f.sale_id)
            .fetch_one(&pool)
            .await
            .expect("notes after");
    let note_items_after: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM credit_note_items cni
         JOIN credit_notes cn ON cn.id = cni.credit_note_id WHERE cn.sale_id = $1",
    )
    .bind(f.sale_id)
    .fetch_one(&pool)
    .await
    .expect("note items after");
    let movements_after: (i64,) =
        sqlx::query_as("SELECT count(*) FROM stock_movements WHERE sale_id = $1")
            .bind(f.sale_id)
            .fetch_one(&pool)
            .await
            .expect("movements after");
    assert_eq!(notes_before.0, 0, "the fixture starts with no note");
    assert_eq!(notes_after.0, 0, "a refused refund creates no document");
    assert_eq!(note_items_after.0, 0, "and no note line");
    assert_eq!(
        movements_after.0, movements_before.0,
        "and no stock movement: rejected input leaves stored data unchanged"
    );
    assert_eq!(cash_before, cash(&base).await, "and cash is unmoved");
}

// S3: refunds accumulate per line, and the whole sale may come back in pieces.
#[tokio::test]
#[ignore]
async fn v1_5_cumulative_refunds_stop_at_the_quantity_sold() {
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");
    let f = reversal_fixture(&base, &pool, "w5-cum").await;
    let (rice_item, rice_product, rice_qty, _) = f.lines[0].clone();
    let (promo_item, promo_product, promo_qty, promo_price) = f.lines[1].clone();

    let stock_of = |product: uuid::Uuid| {
        let pool = pool.clone();
        async move {
            let (stock,): (i64,) = sqlx::query_as(
                "SELECT COALESCE(SUM(quantity), 0) FROM stock_movements WHERE product_id = $1",
            )
            .bind(product)
            .fetch_one(&pool)
            .await
            .expect("stock");
            stock
        }
    };
    let rice_before = stock_of(rice_product).await;
    let promo_before = stock_of(promo_product).await;
    let cash_start = cash(&base).await;

    // Two notes, each a fourth of the sale. The second is allowed because the
    // guard is cumulative, not per-note.
    let quarter = rice_qty / 4;
    for _ in 0..2 {
        let resp = post_refund(
            &base,
            f.sale_id,
            serde_json::json!([{ "sale_item_id": rice_item, "quantity": quarter }]),
        )
        .await;
        assert_eq!(
            resp.status(),
            201,
            "a second note within the limit is allowed"
        );
    }

    // A third note is still within what was sold, and it is the guard's
    // cumulative nature that makes it legal; two notes of a quarter were not
    // refused either.
    let third = post_refund(
        &base,
        f.sale_id,
        serde_json::json!([{ "sale_item_id": rice_item, "quantity": quarter }]),
    )
    .await;
    assert_eq!(
        third.status(),
        201,
        "three quarters of a line may come back"
    );

    // The fourth asks for one unit more than the line has left (300 of 400 are
    // already back), and it is refused on the sum, not on the note: 101 units
    // is a perfectly valid quantity on its own.
    let over = post_refund(
        &base,
        f.sale_id,
        serde_json::json!([{ "sale_item_id": rice_item, "quantity": quarter + 1 }]),
    )
    .await;
    assert_eq!(
        over.status(),
        409,
        "the cumulative sum is what the guard reads, not the note alone"
    );

    // Refund the remainder of the rice line and the whole bonus line. The bonus
    // line returns `0.00`: it was charged nothing, so nothing comes back — but
    // the stock still does, which is the point of a bonus line.
    let remaining = rice_qty - 3 * quarter;
    let resp = post_refund(
        &base,
        f.sale_id,
        serde_json::json!([
            { "sale_item_id": rice_item, "quantity": remaining },
            { "sale_item_id": promo_item, "quantity": promo_qty }
        ]),
    )
    .await;
    assert_eq!(
        resp.status(),
        201,
        "a fully corrected sale is still refundable"
    );
    let final_note: serde_json::Value = resp.json().await.expect("note json");
    let expected_rice = rust_decimal::Decimal::from(remaining) * rust_decimal::Decimal::new(250, 2);
    assert_eq!(
        final_note["total"],
        expected_rice.round_dp(2).to_string(),
        "the note totals what was charged for the returned units"
    );
    let promo_line = final_note["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|line| line["product_id"] == promo_product.to_string())
        .expect("the bonus line is on the note");
    assert_eq!(promo_line["unit_price"], promo_price.as_str());
    assert_eq!(
        promo_line["line_total"], "0.00",
        "a bonus line refunds nothing"
    );

    // Both products are whole again: every unit sold came back.
    assert_eq!(
        stock_of(rice_product).await,
        rice_before + rice_qty,
        "the rice line is back to what it was before the sale"
    );
    assert_eq!(
        stock_of(promo_product).await,
        promo_before + promo_qty,
        "and so is the bonus line"
    );

    // Cash returns to what it was before this sale existed: a fully reversed
    // sale is net zero. `cash_start` was read after the fixture's sale, so the
    // reference point is that figure minus the sale's own total.
    let sale_total = rust_decimal::Decimal::from_str_exact(
        find_sale(&base, f.sale_id).await["total"]
            .as_str()
            .expect("sale total is a decimal string"),
    )
    .expect("decimal");
    assert_eq!(
        cash(&base).await,
        cash_start - sale_total,
        "a fully reversed sale is net zero on cash"
    );

    // The sale itself never moved.
    let sale = find_sale(&base, f.sale_id).await;
    assert_eq!(
        sale["status"], "confirmed",
        "the sale is still a confirmed record"
    );
    assert!(sale["reversed_at"].is_null());
    assert_eq!(
        sale["credit_notes"].as_array().expect("notes").len(),
        4,
        "the correction is four documents, and the sale is not one of them"
    );
}

/// Finds one sale in the list endpoint by id.
async fn find_sale(base: &str, sale_id: uuid::Uuid) -> serde_json::Value {
    reqwest::Client::new()
        .get(format!("{base}/api/sales?limit=200"))
        .send()
        .await
        .expect("list sales")
        .json::<Vec<serde_json::Value>>()
        .await
        .expect("sales json")
        .into_iter()
        .find(|sale| sale["id"] == sale_id.to_string())
        .expect("the sale is in the list")
}

// S2, S6, S15: the guards the new tables carry, probed the way 0003's are.
#[tokio::test]
#[ignore]
async fn v1_5_credit_note_history_guards_are_enforced() {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());
    let f = reversal_fixture(&base, &pool, "w5-guard").await;
    let (rice_item, _, _, _) = f.lines[0].clone();
    let resp = post_refund(
        &base,
        f.sale_id,
        serde_json::json!([{ "sale_item_id": rice_item, "quantity": 10 }]),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let note: serde_json::Value = resp.json().await.expect("note json");
    let note_id: uuid::Uuid = note["id"].as_str().expect("id").parse().expect("uuid");
    let note_item_id: uuid::Uuid = note["items"][0]["id"]
        .as_str()
        .expect("item id")
        .parse()
        .expect("uuid");

    let mut tx = pool.begin().await.expect("begin");
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
        "credit note item update",
        "UPDATE credit_note_items SET quantity = 1 WHERE id = $1",
        note_item_id
    );
    probe_denied!(
        "credit note item delete",
        "DELETE FROM credit_note_items WHERE id = $1",
        note_item_id
    );
    probe_denied!(
        "credit note total change",
        "UPDATE credit_notes SET total = 1.00 WHERE id = $1",
        note_id
    );
    // A note is immutable, row and all — not only its economics. Rewriting
    // `created_at` alone would move the document to another day's ledger.
    probe_denied!(
        "credit note created_at change",
        "UPDATE credit_notes SET created_at = now() WHERE id = $1",
        note_id
    );
    probe_denied!(
        "credit note delete",
        "DELETE FROM credit_notes WHERE id = $1",
        note_id
    );
    probe_denied!("credit note truncate", "TRUNCATE credit_note_items");
    probe_denied!("credit notes truncate", "TRUNCATE credit_notes");

    // The reason grammar stays closed: an unknown reason is a balance change
    // whose cause cannot be named.
    probe_denied!(
        "an unknown movement reason",
        "INSERT INTO stock_movements (product_id, quantity, reason)
         VALUES ((SELECT product_id FROM credit_note_items WHERE id = $1), 1, 'mystery')",
        note_item_id
    );
    // A credit-note movement with no note contradicts itself, and a 'sale'
    // movement must still name its sale.
    probe_denied!(
        "a credit-note movement without its note",
        "INSERT INTO stock_movements (product_id, quantity, reason, sale_id)
         VALUES ((SELECT product_id FROM credit_note_items WHERE id = $1), 1, 'credit_note', $2)",
        note_item_id,
        f.sale_id
    );
    probe_denied!(
        "a sale movement without its sale",
        "INSERT INTO stock_movements (product_id, quantity, reason)
         VALUES ((SELECT product_id FROM credit_note_items WHERE id = $1), 1, 'sale')",
        note_item_id
    );
    // The ledger is still append-only: the movement a refund wrote cannot be
    // edited or removed either.
    probe_denied!(
        "reversal movement update",
        "UPDATE stock_movements SET quantity = 1 WHERE credit_note_id = $1",
        note_id
    );
    probe_denied!(
        "reversal movement delete",
        "DELETE FROM stock_movements WHERE credit_note_id = $1",
        note_id
    );

    tx.rollback().await.expect("rollback");
}

// S15 (preserve): the sale path, the list shape and the cash figure keep
// behaving as they did. `/api/cash` is the one whose definition changed, so the
// test states the new contract explicitly and proves nothing else moved with it.
//
// This test creates the credit note it needs instead of assuming one exists. The
// first version asserted `!refunded.is_zero()` against whatever the database
// happened to hold, so it passed on a development database carrying test residue
// and failed on CI's clean one (measured: `this assertion is only meaningful once
// a credit note exists`, on a database where no note had ever been written). A
// test that needs the artifact it is measuring has to make that artifact, or it
// is asserting on the state of someone else's machine.
#[tokio::test]
#[ignore]
async fn v1_5_cash_still_counts_every_confirmed_sale_and_nothing_else() {
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");

    // A sale of our own, then a partial refund of it: the note this test measures
    // against exists because the test made it, on a clean database or a dirty one.
    let f = reversal_fixture(&base, &pool, "w5-preserve").await;
    let partial = post_refund(
        &base,
        f.sale_id,
        serde_json::json!([{ "sale_item_id": f.lines[0].0, "quantity": 50 }]),
    )
    .await;
    assert_eq!(partial.status(), 201, "the fixture's own refund must land");
    let note: serde_json::Value = partial.json().await.expect("note json");
    let note_total = rust_decimal::Decimal::from_str_exact(
        note["total"]
            .as_str()
            .expect("note total is a decimal string"),
    )
    .expect("decimal");
    assert!(
        note_total > rust_decimal::Decimal::ZERO,
        "the fixture refunds a priced line"
    );

    // The endpoint and the two sums it is defined as, read independently.
    let confirmed: rust_decimal::Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(total), 0)::numeric FROM sales WHERE status = 'confirmed'",
    )
    .fetch_one(&pool)
    .await
    .expect("confirmed sum");
    let refunded: rust_decimal::Decimal =
        sqlx::query_scalar("SELECT COALESCE(SUM(total), 0)::numeric FROM credit_notes")
            .fetch_one(&pool)
            .await
            .expect("refunded sum");
    assert!(
        !refunded.is_zero(),
        "the fixture's own refund must appear in the refunded sum"
    );
    assert_eq!(
        cash(&base).await,
        confirmed - refunded,
        "cash on hand is confirmed sales net of every credit note"
    );

    // And the sale path itself still only moves cash up: a second sale raises it
    // by exactly its own total.
    let cash_before = cash(&base).await;
    let g = reversal_fixture(&base, &pool, "w5-preserve-2").await;
    let sale = find_sale(&base, g.sale_id).await;
    let total = rust_decimal::Decimal::from_str_exact(
        sale["total"]
            .as_str()
            .expect("sale total is a decimal string"),
    )
    .expect("decimal");
    assert_eq!(
        cash(&base).await,
        cash_before + total,
        "a recorded sale still raises cash by its total"
    );
}

// S3 regression (found by the independent verifier, not by the author): one
// request naming the same sale line twice must not bypass the cumulative guard.
// The handler used to read the refunded sum from the database per entry, so two
// entries of one `sale_item_id` each passed against the same pre-existing sum
// and together over-refunded the line: 2 x 6 against a line of 10 was accepted.
#[tokio::test]
#[ignore]
async fn v1_5_one_request_cannot_over_refund_a_line_by_repeating_it() {
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");
    let f = reversal_fixture(&base, &pool, "w5-dup").await;
    let (rice_item, rice_product, rice_qty, _) = f.lines[0].clone();

    // Two halves of a line that is odd: each half alone is legal, together they
    // are one unit more than was sold.
    let half = rice_qty / 2 + 1;
    let notes_before: (i64,) =
        sqlx::query_as("SELECT count(*) FROM credit_notes WHERE sale_id = $1")
            .bind(f.sale_id)
            .fetch_one(&pool)
            .await
            .expect("notes before");
    let cash_before = cash(&base).await;

    let resp = post_refund(
        &base,
        f.sale_id,
        serde_json::json!([
            { "sale_item_id": rice_item, "quantity": half },
            { "sale_item_id": rice_item, "quantity": half }
        ]),
    )
    .await;
    assert_eq!(
        resp.status(),
        409,
        "repeating a line in one request must not over-refund it"
    );

    // And the refusal wrote nothing, like every other refusal.
    let refunded: (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(quantity), 0) FROM credit_note_items WHERE sale_item_id = $1",
    )
    .bind(rice_item)
    .fetch_one(&pool)
    .await
    .expect("refunded sum");
    assert_eq!(refunded.0, 0, "no part of a refused request is written");
    let notes_after: (i64,) =
        sqlx::query_as("SELECT count(*) FROM credit_notes WHERE sale_id = $1")
            .bind(f.sale_id)
            .fetch_one(&pool)
            .await
            .expect("notes after");
    assert_eq!(notes_after.0, notes_before.0, "no document was created");
    let stock: (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(quantity), 0) FROM stock_movements WHERE product_id = $1",
    )
    .bind(rice_product)
    .fetch_one(&pool)
    .await
    .expect("stock");
    assert_eq!(stock.0, 600, "no stock came back from a refused refund");
    assert_eq!(cash_before, cash(&base).await, "and cash is unmoved");
}

// S3 regression (boundary, from the verifier's recheck): the cumulative guard is
// an inequality, and the interesting cases are the two sides of it. A request
// whose entries sum exactly to what the line has left must be accepted; one unit
// more must be refused. The first fix for the repeated-line bypass could have
// been written off-by-one in either direction, so the boundary is pinned here.
#[tokio::test]
#[ignore]
async fn v1_5_repeated_line_is_decided_on_the_exact_boundary() {
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");
    let f = reversal_fixture(&base, &pool, "b2-boundary").await;
    let (line, _, sold, _) = f.lines[0].clone();
    let refunded = |line: uuid::Uuid, pool: sqlx::PgPool| async move {
        let sum: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(quantity), 0) FROM credit_note_items WHERE sale_item_id = $1",
        )
        .bind(line)
        .fetch_one(&pool)
        .await
        .expect("refunded sum");
        sum
    };

    // Two entries that together are exactly the whole line: accepted, and both
    // count. This is the case a guard written with the wrong comparison refuses.
    let half = sold / 2;
    let rest = sold - half;
    let exact = post_refund(
        &base,
        f.sale_id,
        serde_json::json!([
            { "sale_item_id": line, "quantity": half },
            { "sale_item_id": line, "quantity": rest }
        ]),
    )
    .await;
    assert_eq!(
        exact.status(),
        201,
        "entries summing exactly to what was sold must be accepted"
    );
    assert_eq!(
        refunded(line, pool.clone()).await,
        sold,
        "and both entries count"
    );

    // The line is exhausted. Anything more is refused, whether it names the line
    // once or repeats it, and none of the refusals writes anything.
    let notes_before: (i64,) =
        sqlx::query_as("SELECT count(*) FROM credit_notes WHERE sale_id = $1")
            .bind(f.sale_id)
            .fetch_one(&pool)
            .await
            .expect("notes before");
    let cash_before = cash(&base).await;
    for (label, items) in [
        (
            "one entry past the boundary",
            serde_json::json!([{ "sale_item_id": line, "quantity": 1 }]),
        ),
        (
            "two repeats past the boundary",
            serde_json::json!([
                { "sale_item_id": line, "quantity": 1 },
                { "sale_item_id": line, "quantity": 1 }
            ]),
        ),
        (
            "three repeats past the boundary",
            serde_json::json!([
                { "sale_item_id": line, "quantity": 1 },
                { "sale_item_id": line, "quantity": 1 },
                { "sale_item_id": line, "quantity": 1 }
            ]),
        ),
    ] {
        let resp = post_refund(&base, f.sale_id, items).await;
        assert_eq!(resp.status(), 409, "{label} must be refused");
    }
    let notes_after: (i64,) =
        sqlx::query_as("SELECT count(*) FROM credit_notes WHERE sale_id = $1")
            .bind(f.sale_id)
            .fetch_one(&pool)
            .await
            .expect("notes after");
    assert_eq!(
        notes_before.0, notes_after.0,
        "a refused repeat creates no document"
    );
    assert_eq!(
        refunded(line, pool.clone()).await,
        sold,
        "and no note line was written"
    );
    assert_eq!(cash_before, cash(&base).await, "and cash is unmoved");
}

// ── v1.5 es-AR locale: the shop's day (S7, S9) ──────────────────────────────
//
// The day boundary is the one piece of the locale the server owns, because the
// browser's timezone is whatever the machine happens to be configured with.
// These tests pin the wire contract the ledger depends on: four ISO instants, a
// half-open window, and a refusal rather than a guess for a malformed instant.

/// Reads `GET /api/day` for one instant and returns the parsed body.
async fn shop_day(base: &str, at: &str) -> serde_json::Value {
    let resp = reqwest::Client::new()
        .get(format!("{base}/api/day"))
        .query(&[("at", at)])
        .send()
        .await
        .expect("get day");
    assert_eq!(resp.status(), 200, "the day endpoint answers for {at}");
    resp.json().await.expect("day json")
}

#[tokio::test]
#[ignore]
async fn v1_5_the_shop_day_is_half_open_and_adjacent_days_meet_at_midnight() {
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());

    let body = shop_day(&base, "2026-10-16T04:00:00Z").await;

    // Five keys, and nothing invented: the shape is the contract.
    let keys: Vec<&str> = body
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys.len(),
        5,
        "exactly from/to/prev/next/zone, got {keys:?}"
    );
    // The zone travels with the window: the client renders every document's
    // time in it, and a browser cannot know which zone the shop keeps (S19).
    assert_eq!(
        body["zone"], "America/Argentina/Buenos_Aires",
        "the answering zone is named, not implied"
    );
    assert_eq!(
        shop_day(&base, "2026-10-16T04:00:00Z").await["zone"],
        body["zone"],
        "the same shop answers with the same zone for any instant"
    );
    for key in ["from", "to", "prev", "next"] {
        let value = body[key]
            .as_str()
            .unwrap_or_else(|| panic!("{key} is a string"));
        assert!(
            value.ends_with('Z') || value.contains('+'),
            "{key} is an ISO 8601 instant with an offset, got {value:?}"
        );
        assert!(
            chrono::DateTime::parse_from_rfc3339(value).is_ok(),
            "{key} must parse as an instant: {value:?}"
        );
    }

    let from = body["from"].as_str().unwrap();
    let to = body["to"].as_str().unwrap();
    assert_ne!(from, to, "a day has length");
    assert_eq!(
        body["next"], body["to"],
        "next day's start is this day's exclusive end"
    );

    // The instant asked about falls inside the answer...
    let asked: chrono::DateTime<chrono::Utc> = "2026-10-16T04:00:00Z".parse().unwrap();
    let from_at: chrono::DateTime<chrono::Utc> = from.parse().unwrap();
    let to_at: chrono::DateTime<chrono::Utc> = to.parse().unwrap();
    assert!(
        from_at <= asked && asked < to_at,
        "the answer contains {asked}"
    );

    // ...and the next day's window starts exactly where this one ends, so a sale
    // at midnight is counted once and not twice.
    let next = shop_day(&base, body["to"].as_str().unwrap()).await;
    assert_eq!(
        next["from"], body["to"],
        "adjacent days partition the ledger"
    );

    // Paging one day back lands on this day's start: the ledger walks local days.
    let prev = shop_day(&base, body["prev"].as_str().unwrap()).await;
    assert_eq!(prev["to"], body["from"], "prev's end is this day's start");
}

#[tokio::test]
#[ignore]
async fn v1_5_the_shop_day_puts_a_late_evening_sale_inside_its_own_day() {
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());

    // 22:30 on 16 Oct in Buenos Aires is already 01:30 on 17 Oct in UTC. A UTC
    // day would file it under the wrong date, which is the defect this closes.
    let body = shop_day(&base, "2026-10-17T01:30:00Z").await;
    let from: chrono::DateTime<chrono::Utc> = body["from"].as_str().unwrap().parse().unwrap();
    let to: chrono::DateTime<chrono::Utc> = body["to"].as_str().unwrap().parse().unwrap();
    let asked: chrono::DateTime<chrono::Utc> = "2026-10-17T01:30:00Z".parse().unwrap();
    assert!(
        from <= asked && asked < to,
        "the shop day must contain its own 22:30"
    );

    // The default zone is Argentina, whose offset is UTC-3 with no DST, so a
    // local midnight is 03:00Z. The instant is UTC (that is the wire format);
    // what makes it "midnight" is the zone the response names, and the two are
    // checked together so neither can drift alone.
    assert_eq!(
        body["zone"], "America/Argentina/Buenos_Aires",
        "the window is Buenos Aires', and it says so"
    );
    assert_eq!(
        from.format("%H:%M").to_string(),
        "03:00",
        "local midnight in UTC-3 is 03:00Z, the instant the day starts at ({from})"
    );
    assert_eq!((to - from).num_hours(), 24, "an Argentine day is 24 hours");
}

#[tokio::test]
#[ignore]
async fn v1_5_a_malformed_day_is_a_refusal_not_a_guess() {
    let base = std::env::var("BACKEND_BASE").unwrap_or_else(|_| "http://localhost:3001".into());
    let client = reqwest::Client::new();

    for raw in ["not-a-date", "2026-13-01T00:00:00Z", "16/10/2026", " "] {
        let resp = client
            .get(format!("{base}/api/day"))
            .query(&[("at", raw)])
            .send()
            .await
            .expect("get day");
        assert_eq!(
            resp.status(),
            400,
            "{raw:?} is not an instant; guessing one would file sales under a day the shop never chose"
        );
    }

    // No `at` at all is not an error: the server answers for its own now.
    let resp = client
        .get(format!("{base}/api/day"))
        .send()
        .await
        .expect("get day without at");
    assert_eq!(resp.status(), 200, "an absent at means now");
    let body: serde_json::Value = resp.json().await.expect("day json");
    let from: chrono::DateTime<chrono::Utc> = body["from"].as_str().unwrap().parse().unwrap();
    let to: chrono::DateTime<chrono::Utc> = body["to"].as_str().unwrap().parse().unwrap();
    let now = chrono::Utc::now();
    assert!(
        from <= now && now < to,
        "the answer to 'what day is it' must contain the moment it was asked"
    );
}

// ── v2 (T1): the purchases schema ──────────────────────────────────────────
//
// plan-v2.md, "Data model" and the `0008` lesson: the tables exist, the seeded
// supplier is there with its fixed id, and the guards are enforced by the
// database rather than by a writer's discipline. Every test is self-relative —
// it creates its own rows and rolls back — because this suite runs against the
// shared development database (plan-v2.md, "The testing hazard this slice must
// not repeat").

/// The seeded supplier's id, mirroring the walk-in customer's fixed id.
/// Spelled out rather than imported: the test is what pins the constant the
/// migration writes, the same way `walk_in_id_matches_the_migration_seed` does.
const ONE_OFF_SUPPLIER_ID: &str = "00000000-0000-0000-0000-000000000002";

#[tokio::test]
#[ignore]
async fn v2_schema_creates_purchases_tables_and_seeds_one_off_supplier() {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");

    for table in ["suppliers", "purchases", "purchase_items"] {
        let reg: Option<String> = sqlx::query_scalar("SELECT to_regclass($1)::text")
            .bind(format!("public.{table}"))
            .fetch_one(&pool)
            .await
            .expect("to_regclass");
        assert_eq!(
            reg.as_deref(),
            Some(table),
            "{table} does not exist: the v2 migration did not run"
        );
    }

    // The seed is what lets a purchase omit its supplier, exactly as the walk-in
    // seed lets a sale omit its customer. Its name is the stored English string;
    // the es-AR display string lives in the frontend dictionary.
    let name: String = sqlx::query_scalar("SELECT name FROM suppliers WHERE id = $1")
        .bind(uuid::Uuid::parse_str(ONE_OFF_SUPPLIER_ID).expect("fixed uuid"))
        .fetch_one(&pool)
        .await
        .expect("seeded supplier");
    assert_eq!(
        name, "one-off-supplier",
        "the seeded supplier's stored name is a contract: it reaches the API and exports"
    );
}

#[tokio::test]
#[ignore]
async fn v2_a_purchase_is_immutable_after_it_exists() {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");

    let mut tx = pool.begin().await.expect("begin");

    let pid: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO products (name, list_price) VALUES ('v2-guard-p', 10.00) RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("insert product");

    let purchase_id: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO purchases (supplier_id, total) VALUES ($1, 20.00) RETURNING id",
    )
    .bind(uuid::Uuid::parse_str(ONE_OFF_SUPPLIER_ID).expect("fixed uuid"))
    .fetch_one(&mut *tx)
    .await
    .expect("insert purchase");

    let item_id: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO purchase_items
            (purchase_id, product_id, product_name, quantity, unit_price, line_total, unit)
         VALUES ($1, $2, 'v2-guard-p', 2, 10.00, 20.00, 'unit') RETURNING id",
    )
    .bind(purchase_id.0)
    .bind(pid.0)
    .fetch_one(&mut *tx)
    .await
    .expect("insert purchase item");

    // The ledger must accept the new reason and cohere with it: a purchase row is
    // money out of the till and stock in, so its quantity is positive.
    sqlx::query(
        "INSERT INTO stock_movements (product_id, quantity, reason, purchase_id)
         VALUES ($1, 2, 'purchase', $2)",
    )
    .bind(pid.0)
    .bind(purchase_id.0)
    .execute(&mut *tx)
    .await
    .expect("a purchase movement must be accepted by the ledger");

    // Probe helper: run one expected-to-fail statement inside a savepoint and
    // recover the transaction, so the next probe sees unchanged state.
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
        "purchase update",
        "UPDATE purchases SET total = 1.00 WHERE id = $1",
        purchase_id.0
    );
    probe_denied!(
        "purchase delete",
        "DELETE FROM purchases WHERE id = $1",
        purchase_id.0
    );
    probe_denied!(
        "purchase item update",
        "UPDATE purchase_items SET quantity = 1 WHERE id = $1",
        item_id.0
    );
    probe_denied!(
        "purchase item delete",
        "DELETE FROM purchase_items WHERE id = $1",
        item_id.0
    );
    // A purchase has no status, so unlike a sale there is no permitted edit at
    // all — that is the design, not an omission (plan-v2.md, "Why not one
    // document with a status").
    probe_denied!(
        "purchase created_at rewrite",
        "UPDATE purchases SET created_at = now() WHERE id = $1",
        purchase_id.0
    );

    // A finer-than-a-cent amount is refused on both new money columns.
    probe_denied!(
        "a purchase total finer than a cent",
        "INSERT INTO purchases (supplier_id, total) VALUES ($1, 20.005)",
        uuid::Uuid::parse_str(ONE_OFF_SUPPLIER_ID).expect("fixed uuid")
    );

    tx.rollback().await.expect("rollback");
}

#[tokio::test]
#[ignore]
async fn v2_the_ledger_refuses_a_purchase_movement_without_its_document() {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tempox:tempox@localhost:5432/tempox".into());
    let pool = sqlx::PgPool::connect(&url).await.expect("db connect");

    let mut tx = pool.begin().await.expect("begin");
    let pid: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO products (name, list_price) VALUES ('v2-coherence-p', 5.00) RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("insert product");

    let purchase_id: (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO purchases (supplier_id, total) VALUES ($1, 0.00) RETURNING id")
            .bind(uuid::Uuid::parse_str(ONE_OFF_SUPPLIER_ID).expect("fixed uuid"))
            .fetch_one(&mut *tx)
            .await
            .expect("insert zero-total purchase");

    // FIRST, the positive case, and it is not ceremony: without it this test
    // would pass before the migration exists, because 'purchase' was not yet a
    // known reason and the refusal it measures would be the *wrong* one. A guard
    // test that a missing feature satisfies proves nothing.
    sqlx::query(
        "INSERT INTO stock_movements (product_id, quantity, reason, purchase_id)
         VALUES ($1, 3, 'purchase', $2)",
    )
    .bind(pid.0)
    .bind(purchase_id.0)
    .execute(&mut *tx)
    .await
    .expect("a coherent purchase movement must be accepted: the reason is in the vocabulary");

    // NOW the two refusals the coherence rule owes. Each runs in its own
    // savepoint, because a failed statement aborts the enclosing transaction.
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

    // One side: a 'purchase' row that names no document.
    probe_denied!(
        "a purchase movement with no purchase_id",
        "INSERT INTO stock_movements (product_id, quantity, reason) VALUES ($1, 1, 'purchase')",
        pid.0
    );

    // The other side: an 'initial' row that claims one.
    probe_denied!(
        "an initial movement carrying a purchase_id",
        "INSERT INTO stock_movements (product_id, quantity, reason, purchase_id)
         VALUES ($1, 1, 'initial', $2)",
        pid.0,
        purchase_id.0
    );

    // And the sign convention: stock comes IN on a purchase, so a negative
    // purchase movement means goods left on a document that only ever receives.
    // The ledger accepts negative quantities in general (a sale), which is why
    // this has to be asserted rather than assumed.
    probe_denied!(
        "a negative purchase movement",
        "INSERT INTO stock_movements (product_id, quantity, reason, purchase_id)
         VALUES ($1, -3, 'purchase', $2)",
        pid.0,
        purchase_id.0
    );

    // The trailing side of the same rule, and it needs its own probe: a `sale`
    // row that also claims a purchase would satisfy the reason/document rule
    // above (it carries a sale_id) while pointing at two documents at once. The
    // equality in `stock_movements_purchase_coherent` is what refuses it — a
    // one-sided `purchase => purchase_id` check would let this through, which is
    // exactly what a mutation run measured.
    let sale_id: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO sales (customer_id, total) VALUES ('00000000-0000-0000-0000-000000000001', 0.00) RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("insert zero-total sale");

    probe_denied!(
        "a sale movement carrying a purchase_id",
        "INSERT INTO stock_movements (product_id, quantity, reason, sale_id, purchase_id)
         VALUES ($1, -3, 'sale', $2, $3)",
        pid.0,
        sale_id.0,
        purchase_id.0
    );

    tx.rollback().await.expect("rollback");
}
