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
    assert!(list
        .as_array()
        .expect("array")
        .iter()
        .any(|n| n["id"] == id));

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

    for table in ["customers", "products", "sales", "sale_items", "stock_movements"] {
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
    let cid: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO customers (name) VALUES ('guard-test-other') RETURNING id",
    )
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

    probe_denied!("sale item update", "UPDATE sale_items SET quantity = 1 WHERE id = $1", iid.0);
    probe_denied!("sale item delete", "DELETE FROM sale_items WHERE id = $1", iid.0);
    probe_denied!("sale total change", "UPDATE sales SET total = 1 WHERE id = $1", sid.0);
    probe_denied!("sale customer change", "UPDATE sales SET customer_id = $2 WHERE id = $1", sid.0, cid.0);

    // The status transition (reversal) must still work.
    let transition = sqlx::query(
        "UPDATE sales SET status = 'cancel', reversed_at = now() WHERE id = $1",
    )
    .bind(sid.0)
    .execute(&mut *tx)
    .await;
    assert!(transition.is_ok(), "status transition was blocked");

    tx.rollback().await.expect("rollback");
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
    sqlx::query("INSERT INTO stock_movements (product_id, quantity, reason) VALUES ($1, 2000, 'initial')")
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
    sqlx::query("INSERT INTO stock_movements (product_id, quantity, reason) VALUES ($1, 10, 'initial')")
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
        assert_eq!(line["line_total"].as_i64().expect("line_total"), qty * price);
    }
    assert!(lines.iter().any(|l| l["product_name"] == rice_name && l["unit"] == "g"));
    assert!(lines
        .iter()
        .any(|l| l["product_name"] == beans_name && l["unit"] == "unit"));

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
    assert!(customers
        .iter()
        .any(|c| c["name"] == "walk-in" && c["id"] == "00000000-0000-0000-0000-000000000001"));

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
    assert!(lines.iter().any(|l| l["product_name"] == rice_name && l["unit"] == "g"));
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

    let moves: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM stock_movements WHERE product_id = $1",
    )
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
    assert_eq!(moves.0, 0, "no movement row should exist for zero initial stock");
}
