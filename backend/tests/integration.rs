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
