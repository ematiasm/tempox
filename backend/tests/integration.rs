//! Integration tests, in three levels, none of which needs `docker compose up`.
//!
//! Most tests run on the database `#[sqlx::test]` creates for each one, so they need
//! only a reachable postgres — `make test-backend` provides it through
//! `scripts/test-stack.sh`. Two attribute styles live here:
//!
//! - `#[sqlx::test]` + `common::client(pool)` drives the router **in process**, via
//!   `tower::oneshot`. This is the bulk of the suite: routing, extractors, status
//!   codes, bodies, and the rows they wrote.
//! - `#[sqlx::test(migrations = false)]` + `common::spawn_backend(pool)` starts the
//!   real `CARGO_BIN_EXE_tempox` on an ephemeral port against an **empty** database.
//!   That is the only level that covers what `oneshot` cannot: migrations running at
//!   boot, a real socket, and `serve()`.
//!
//! Run them with `make test-backend`, or directly:
//!   ./scripts/test-stack.sh sh -c 'cd backend && cargo test --all-features --test integration -- --include-ignored'
//!
//! `--ignored` is no longer the way to reach the HTTP tests; it now selects only
//! legacy cases, if any remain.

mod common;

#[sqlx::test(migrations = false)]
async fn f4_the_spawned_binary_migrates_an_empty_database_and_serves(pool: sqlx::PgPool) {
    // migrations = false leaves this database empty until the spawned binary boots,
    // so this migrated-table query proves that the binary applied migrations.
    let backend = common::spawn_backend(&pool).await;
    let response = reqwest::get(format!("{}/api/notes", backend.base_url()))
        .await
        .expect("request spawned backend");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
}

#[test]
fn smoke_arithmetic() {
    // Sanity placeholder so `cargo test` always has something to run.
    assert_eq!(2 + 2, 4);
}

// `/health` needs no table, so this one keeps the default `#[sqlx::test]`
// migrations and does not assert anything about boot's migration step — the test
// above owns that claim. What this covers is the endpoint answering over a real
// socket, on the port the binary actually bound.
#[sqlx::test]
async fn health_endpoint_returns_ok(pool: sqlx::PgPool) {
    let backend = common::spawn_backend(&pool).await;
    let resp = reqwest::get(format!("{}/health", backend.base_url()))
        .await
        .expect("request spawned backend");
    assert!(resp.status().is_success());
    let json: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(json["status"], "ok");
}

// A full write round trip over the real socket. Everything the test asserts is
// read back from the database the spawned binary was pointed at, which is the
// pool this test also holds — the desync the old harness could not rule out.
#[sqlx::test]
async fn notes_crud_roundtrip(pool: sqlx::PgPool) {
    let backend = common::spawn_backend(&pool).await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/notes", backend.base_url()))
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
        .get(format!("{}/api/notes", backend.base_url()))
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

    // The row is in the database the test owns, not merely in the response the
    // binary produced. This is the assertion the old two-process harness could
    // only hope for, because it agreed on the database by configuration.
    let stored: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM notes WHERE id = $1::uuid")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .expect("count the note in the test's own database");
    assert_eq!(stored, 1, "the note the binary created is in this pool");

    let del = client
        .delete(format!("{}/api/notes/{id}", backend.base_url()))
        .send()
        .await
        .expect("delete");
    assert_eq!(del.status(), 204);

    let after: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM notes WHERE id = $1::uuid")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .expect("count after delete");
    assert_eq!(after, 0, "the delete reached the same database");
}

// v1 schema (work-unit 1): the migration creates the five tables and seeds the
// walk-in customer.
#[sqlx::test]
async fn v1_schema_seeds_walkin_customers(pool: sqlx::PgPool) {
    let (db,): (String,) = sqlx::query_as("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .expect("current_database");
    assert!(
        db.starts_with("_sqlx_test_"),
        "expected a throwaway database, got {db}"
    );
    assert_ne!(
        db, "tempox_test",
        "the shared test database must not be used"
    );

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
//
#[sqlx::test]
async fn v1_units_and_history_guards(pool: sqlx::PgPool) {
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
        "INSERT INTO sales (customer_id, total, payment_method_id)
         VALUES ('00000000-0000-0000-0000-000000000001', 3750.00, '00000000-0000-0000-0000-000000000004') RETURNING id",
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
            "INSERT INTO sales (customer_id, total, payment_method_id)
             VALUES ('00000000-0000-0000-0000-000000000001', 1.235, '00000000-0000-0000-0000-000000000004')",
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

    // **No column of a sale is writable any more.** v1 permitted the status
    // transition here because `status`/`reversed_at` were reserved for a lifecycle
    // `0015` has since dropped; there is nothing left to transition, so the guard now
    // refuses every UPDATE rather than all but one. What that costs is a capability
    // nothing ever used; what it buys is that "the row exists" and "it counts" are one
    // statement, as they already were for the other three documents.
    // The write has to CHANGE something, or PostgreSQL does not fire the trigger at
    // all: `SET total = total` is a no-op and the row is not rewritten. Measured —
    // the first version of this assertion passed for the wrong reason.
    let any_write = sqlx::query("UPDATE sales SET total = total + 0.01 WHERE id = $1")
        .bind(sid.0)
        .execute(&mut *tx)
        .await;
    let refusal = any_write.expect_err("a sale must have no writable column at all");
    // **The failure has to be the guard, not a CHECK.** `total + 0.01` still lands on
    // whole cents, so a whole-cents constraint cannot be what refuses it; but the
    // assertion would accept any error at all, and that is the class of test this
    // slice has already been caught by twice. `R3-002` named it.
    let message = refusal
        .as_database_error()
        .map(|e| e.message().to_string())
        .unwrap_or_default();
    assert!(
        message.contains("economics are immutable"),
        "the refusal must come from the sale guard, got: {message}"
    );

    tx.rollback().await.expect("rollback");
}

// v1 (P1) — the stock ledger is append-only and reason-coherent.
// plan-v1.md: "Stock is never a mutable `quantity` field ... every change is an
// append-only row", plus decision 15: the database enforces the history guards.
//
#[sqlx::test]
async fn v1_ledger_is_append_only_and_reason_coherent(pool: sqlx::PgPool) {
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
        "INSERT INTO sales (customer_id, total, payment_method_id)
         VALUES ('00000000-0000-0000-0000-000000000001', 3750, '00000000-0000-0000-0000-000000000004') RETURNING id",
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
//
#[sqlx::test]
async fn v1_sale_of_bonus_lines_is_recorded(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    // A product with stock, so the bonus movement has something to take.
    let gift_name = common::unique("p3-gift");
    let created = client
        .post(
            "/api/products",
            serde_json::json!({
                "name": gift_name, "list_price": "2.50", "unit": "unit", "initial_stock": 10
            }),
        )
        .await;
    created.expect_status(axum::http::StatusCode::CREATED);
    let product = created.json().clone();
    assert_eq!(
        product["list_price"], "2.50",
        "a decimal string with two decimals comes back"
    );

    let cash_before: (rust_decimal::Decimal,) =
        sqlx::query_as("SELECT COALESCE(SUM(total), 0)::numeric FROM sales")
            .fetch_one(&pool)
            .await
            .expect("cash before");

    // Three units, given away: the price override is zero.
    let resp = client
        .post(
            "/api/sales",
            serde_json::json!({
                "items": [{ "product_id": product["id"], "quantity": 3, "unit_price": "0" }]
            }),
        )
        .await;
    assert_eq!(
        resp.status(),
        201,
        "a bonus sale must be created, not rejected by the schema"
    );
    let sale = resp.json().clone();

    // The sale exists and is worth nothing. Amounts are decimal strings: `0.00`
    // is money, the number `0` would not be.
    assert_eq!(sale["total"], "0.00", "a bonus sale totals zero");
    // No lifecycle on the wire since 0015; the row is the record.
    assert!(sale.get("status").is_none());
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
    let cash_after: (rust_decimal::Decimal,) =
        sqlx::query_as("SELECT COALESCE(SUM(total), 0)::numeric FROM sales")
            .fetch_one(&pool)
            .await
            .expect("cash after");
    assert_eq!(cash_after.0, cash_before.0, "a bonus sale moves no money");
}

// v1 writes (work-unit 3): a multi-item sale lands in one transaction with its
// stock movements. Unique product names keep repeated runs collision-free.
//
#[sqlx::test]
async fn v1_create_sale_drops_stock_in_one_transaction(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());
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
    let cash_before: (rust_decimal::Decimal,) =
        sqlx::query_as("SELECT COALESCE(SUM(total), 0)::numeric FROM sales")
            .fetch_one(&pool)
            .await
            .expect("cash before");

    // Two items in one sale: no customer (walk-in), second line overrides the price.
    let resp = client
        .post(
            "/api/sales",
            serde_json::json!({
                "items": [
                    { "product_id": rice.0, "quantity": 1500 },
                    { "product_id": beans.0, "quantity": 3, "unit_price": "2.00" }
                ]
            }),
        )
        .await;
    assert_eq!(resp.status(), 201, "sale was not created");
    let sale = resp.json().clone();

    // Header: total = 1500 x $2.50 + 3 x $2.00 = $3756.00, confirmed, on the walk-in.
    assert_eq!(sale["total"], "3756.00");
    // No lifecycle on the wire since 0015; the row is the record.
    assert!(sale.get("status").is_none());
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
    let cash_after: (rust_decimal::Decimal,) =
        sqlx::query_as("SELECT COALESCE(SUM(total), 0)::numeric FROM sales")
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
#[sqlx::test]
async fn v1_read_endpoints_reflect_writes(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    // Two products created through the API, with initial stock.
    let rice_name = common::unique("w4-rice");
    let resp = client
        .post(
            "/api/products",
            serde_json::json!({
                "name": rice_name, "list_price": "2.50", "unit": "g", "initial_stock": 2000
            }),
        )
        .await;
    assert_eq!(resp.status(), 201);
    let rice = resp.json().clone();
    assert_eq!(rice["stock"], 2000, "initial stock not reflected");

    let beans_name = common::unique("w4-beans");
    let resp = client
        .post(
            "/api/products",
            serde_json::json!({
                "name": beans_name, "list_price": "1.00", "unit": "unit", "initial_stock": 10
            }),
        )
        .await;
    assert_eq!(resp.status(), 201);
    let beans = resp.json().clone();

    // A customer through the API.
    let customer_name = common::unique("w4-customer");
    let resp = client
        .post(
            "/api/customers",
            serde_json::json!({ "name": customer_name }),
        )
        .await;
    assert_eq!(resp.status(), 201);
    let created = resp.json().clone();
    let customer_id = created["id"].as_str().expect("customer id").to_owned();

    // A sale with both products; the first line overrides its price.
    let resp = client
        .post(
            "/api/sales",
            serde_json::json!({
                "customer_id": customer_id,
                "items": [
                    { "product_id": rice["id"], "quantity": 1500, "unit_price": "2.00" },
                    { "product_id": beans["id"], "quantity": 3 }
                ]
            }),
        )
        .await;
    assert_eq!(resp.status(), 201);
    let sale = resp.json().clone();
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
        let client = &client;
        async move {
            let query = common::encode_query(&name);
            let response = client.get(&format!("/api/products?q={query}")).await;
            response.expect_status(axum::http::StatusCode::OK);
            response
                .json()
                .as_array()
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
    let customers_resp = client.get("/api/customers").await;
    customers_resp.expect_status(axum::http::StatusCode::OK);
    let customers = customers_resp.json().as_array().expect("customers array");
    assert!(customers.iter().any(|c| c["name"] == customer_name));
    assert!(
        customers
            .iter()
            .any(|c| c["name"] == "walk-in" && c["id"] == "00000000-0000-0000-0000-000000000001")
    );

    // GET /api/sales: the sale appears with its nested lines and customer name.
    let sales_resp = client.get("/api/sales?limit=50").await;
    sales_resp.expect_status(axum::http::StatusCode::OK);
    let sales = sales_resp.json().as_array().expect("sales array");
    let sale_read = sales
        .iter()
        .find(|s| s["id"] == sale["id"])
        .expect("sale in list");
    assert_eq!(sale_read["customer_name"], customer_name);
    assert!(sale_read.get("status").is_none());
    let lines = sale_read["items"].as_array().expect("lines");
    assert_eq!(lines.len(), 2);
    assert!(
        lines
            .iter()
            .any(|l| l["product_name"] == rice_name && l["unit"] == "g")
    );
    assert!(lines.iter().any(|l| l["product_name"] == beans_name));

    // GET /api/cash: the total moved by exactly this sale. The endpoint is net
    // of credit notes (v1.5), of purchases (v2) and adds back supplier returns
    // (v2.5), so the test's own arithmetic does too — otherwise it would assert
    // the old definition of a changed endpoint, which is precisely how this test
    // failed both times a term was added.
    let cash_before: (rust_decimal::Decimal,) = sqlx::query_as(
        "SELECT COALESCE((SELECT SUM(total) FROM sales), 0)::numeric
              - COALESCE((SELECT SUM(total) FROM credit_notes), 0)::numeric
              - COALESCE((SELECT SUM(total) FROM purchases), 0)::numeric
              + COALESCE((SELECT SUM(total) FROM supplier_returns), 0)::numeric
              - $1::numeric",
    )
    .bind(rust_decimal::Decimal::new(300300, 2))
    .fetch_one(&pool)
    .await
    .expect("cash arithmetic");
    // `T6` retired `/api/cash`; the same figure is now the default account's
    // derived balance.
    let cash_resp = client.get("/api/accounts").await;
    cash_resp.expect_status(axum::http::StatusCode::OK);
    let accounts = cash_resp.json().clone();
    let till = accounts
        .as_array()
        .expect("accounts array")
        .iter()
        .find(|a| a["is_default"].as_bool().unwrap_or(false))
        .expect("a default account")
        .clone();
    assert_eq!(
        till["balance"],
        format!(
            "{:.2}",
            cash_before.0 + rust_decimal::Decimal::new(300300, 2)
        ),
        "cash is a decimal string summing to the sales total"
    );
}

// A product created without initial stock starts at 0 with no movement row.
#[sqlx::test]
async fn v1_product_without_initial_stock_starts_at_zero(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());
    let name = common::unique("w4-empty");

    let created = client
        .post(
            "/api/products",
            serde_json::json!({ "name": name, "list_price": "1.00" }),
        )
        .await;
    created.expect_status(axum::http::StatusCode::CREATED);
    let product = created.json().clone();
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
// The instants sit in 2001 because a boundary rule reads better with concrete
// dates than with "now", not because other tests could interfere: this test has
// its own database.
#[sqlx::test]
async fn v1a_sales_list_is_scoped_by_date_and_offset(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

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
            // **Both halves of the event, in one transaction.** `0021` made "a sale that
            // names a method wrote an entry" a rule checked at COMMIT, so a header alone is
            // refused. The route writes the pair; a fixture that cares about `created_at`
            // writes it too.
            let mut tx = pool.begin().await.expect("begin");
            let row: (uuid::Uuid,) = sqlx::query_as(
                "INSERT INTO sales (customer_id, total, created_at, payment_method_id)
                 VALUES ('00000000-0000-0000-0000-000000000001', 1.00, $1::timestamptz, '00000000-0000-0000-0000-000000000004') RETURNING id",
            )
            .bind(created_at.clone())
            .fetch_one(&mut *tx)
            .await
            .expect("insert sale");
            sqlx::query(
                "INSERT INTO money_entries
                    (account_id, payment_method_id, document_kind, document_id, amount,
                     occurred_at, created_by)
                 SELECT pm.account_id, '00000000-0000-0000-0000-000000000004', 'sale', $1,
                        1.00, $2::timestamptz, 'test'
                   FROM payment_methods pm WHERE pm.id = '00000000-0000-0000-0000-000000000004'",
            )
            .bind(row.0)
            .bind(created_at)
            .execute(&mut *tx)
            .await
            .expect("insert the sale's entry");
            tx.commit().await.expect("commit");
            row.0
        }
    };

    // The 2001 window used to be a collision-avoidance device: the database was
    // shared, so this test had to pick dates no other test wrote and clear its
    // window before and after. `#[sqlx::test]` gives this test its own database,
    // so the window is now only what it says — four documents placed around one
    // midnight — and nothing needs clearing. The dates stayed because the
    // boundary rule is easier to read with concrete instants.

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
        let client = &client;
        async move {
            let response = client.get(&format!("/api/sales?{query}")).await;
            response.expect_status(axum::http::StatusCode::OK);
            response.json().clone()
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

    // No bounds: the date filter is not applied, so an unbounded read returns
    // recent documents regardless of their date. That is the behaviour v1
    // shipped.
    //
    // The assertion is about the PROPERTY, not about reaching this particular
    // 2001 row. It used to be `contains(&late_today)` against `limit=200`, which
    // held only while the shared development database carried fewer than 200
    // newer sales — an assumption about how much data other people's test runs
    // had left behind. It was measured failing at 223 sales, 219 of them newer,
    // after an independent verifier's probe traffic. Scoping to this test's own
    // fixture measures the same rule without depending on the population of a
    // database this test does not own.
    let unbounded = get(format!("from={}&to={}&limit=200", bounds.1, bounds.2)).await;
    assert_eq!(
        ids_of(&unbounded).len(),
        3,
        "an unbounded-by-absense read is not filtered by the clock; it is paged"
    );
    let truly_unbounded = get("limit=1".to_string()).await;
    assert!(
        !ids_of(&truly_unbounded).is_empty(),
        "with no bounds the endpoint still answers with recent documents"
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
    assert!(row.get("status").is_none());

    // Nothing above may have written, and this database holds only this test's
    // rows, so the count is exact rather than a window into a shared ledger.
    let mine: (i64,) = sqlx::query_as("SELECT count(*) FROM sales")
        .fetch_one(&pool)
        .await
        .expect("probe count");
    assert_eq!(mine.0, 4, "listing a range must not change stored sales");
}

// v1a (work-unit 2, S3) — GET /api/products is searchable by name and
// offset-paginated, while stock stays derived and the name ordering holds.
#[sqlx::test]
async fn v1a_products_list_is_searchable_and_paginated(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    // A marker unique to this run, so a failure message names its own rows. The
    // database is this test's alone now, so the marker is no longer what keeps
    // the assertions true; it is what makes a failure readable.
    //
    // Mixed case on purpose: `q` is a case-insensitive substring match, so a
    // lowercase needle must find it.
    let marker = common::unique("v1a-q");
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
        let client = &client;
        async move {
            let response = client.get(&format!("/api/products?{query}")).await;
            response.expect_status(axum::http::StatusCode::OK);
            response.json().clone()
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

/// Reads the till's balance: the account whose `is_default` is true, from
/// `GET /api/accounts`, which is where the figure lives since `T6` retired
/// `/api/cash`.
///
/// The replacement is deliberately the *same number* by a different route: `0014`'s
/// view sums what that endpoint used to sum, and `v3_t3_the_four_documents…` asserts
/// the agreement directly, which is what made the retirement safe rather than a leap.
async fn cash(client: &common::Client) -> rust_decimal::Decimal {
    let response = client.get("/api/accounts").await;
    response.expect_status(axum::http::StatusCode::OK);
    let body = response.json();
    let till = body
        .as_array()
        .expect("accounts array")
        .iter()
        .find(|a| a["is_default"].as_bool().unwrap_or(false))
        .expect("exactly one account is the default");
    rust_decimal::Decimal::from_str_exact(till["balance"].as_str().expect("balance string"))
        .expect("balance is a decimal")
}

/// Refunds part of a sale through the API and returns the response, whatever
/// its status — the error probes want to see the refusal, not an unwind.
async fn post_refund(
    client: &common::Client,
    sale_id: uuid::Uuid,
    items: serde_json::Value,
) -> common::TestResponse {
    client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({ "items": items }),
        )
        .await
}

/// Records one two-line sale and returns it ready to be corrected.
async fn reversal_fixture(client: &common::Client, pool: &sqlx::PgPool, prefix: &str) -> Reversal {
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

    let resp = client
        .post(
            "/api/sales",
            serde_json::json!({
                "items": [
                    { "product_id": rice.0, "quantity": 400 },
                    { "product_id": promo.0, "quantity": 2 }
                ]
            }),
        )
        .await;
    assert_eq!(resp.status(), 201, "fixture sale was not created");
    let sale = resp.json().clone();

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
#[sqlx::test]
async fn v1_5_credit_note_returns_stock_and_money_without_touching_the_sale(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());
    let f = reversal_fixture(&client, &pool, "w5").await;
    let (rice_item, rice_product, rice_qty, rice_price) = f.lines[0].clone();

    let sale_before: (String,) = sqlx::query_as("SELECT total::text FROM sales WHERE id = $1")
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

    let cash_before = cash(&client).await;

    // Refund 150 of the 400 grams: 150 x $2.50 = $375.00.
    let resp = post_refund(
        &client,
        f.sale_id,
        serde_json::json!([{ "sale_item_id": rice_item, "quantity": 150 }]),
    )
    .await;
    assert_eq!(resp.status(), 201, "a partial refund must be accepted");
    let note: serde_json::Value = resp.json().clone();

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

    // The corrected sale is untouched, byte for byte. What is compared is what now
    // exists: the total, and — since `0015` dropped the lifecycle — the whole row's
    // identity, which no credit note may move.
    let sale_after: (String,) = sqlx::query_as("SELECT total::text FROM sales WHERE id = $1")
        .bind(f.sale_id)
        .fetch_one(&pool)
        .await
        .expect("sale after");
    assert_eq!(
        sale_after.0, sale_before.0,
        "the corrected sale's total must not change"
    );

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
        cash_before - cash(&client).await,
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
#[sqlx::test]
async fn v1_5_sales_list_carries_credit_notes_with_their_lines(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());
    let f = reversal_fixture(&client, &pool, "w5-list").await;
    let (rice_item, _, _, _) = f.lines[0].clone();

    // Before any refund the field is present and empty, never absent: the
    // client must not have to tell "no notes" from "the server forgot".
    let sale = find_sale(&client, f.sale_id).await;
    assert_eq!(
        sale["credit_notes"].as_array().expect("array").len(),
        0,
        "an uncorrected sale carries an empty array"
    );

    let resp = post_refund(
        &client,
        f.sale_id,
        serde_json::json!([{ "sale_item_id": rice_item, "quantity": 100 }]),
    )
    .await;
    assert_eq!(resp.status(), 201);

    let sale = find_sale(&client, f.sale_id).await;
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
#[sqlx::test]
async fn v1_5_refusals_are_distinct_and_write_nothing(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());
    let f = reversal_fixture(&client, &pool, "w5-refuse").await;
    let (rice_item, _, _, _) = f.lines[0].clone();

    // A sale that does not exist is a 404, not a 400 and not a 500.
    let nowhere = post_refund(
        &client,
        uuid::Uuid::new_v4(),
        serde_json::json!([{ "sale_item_id": rice_item, "quantity": 1 }]),
    )
    .await;
    assert_eq!(nowhere.status(), 404, "an unknown sale is a 404");

    // A line that belongs to another sale is also a 404: it is the wrong
    // document, not a conflict.
    let other = reversal_fixture(&client, &pool, "w5-other").await;
    let foreign = post_refund(
        &client,
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
        let resp = post_refund(&client, f.sale_id, items).await;
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
    let cash_before = cash(&client).await;

    // S3: more than was sold is a 409.
    let too_much = post_refund(
        &client,
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
    assert_eq!(cash_before, cash(&client).await, "and cash is unmoved");
}

// S3: refunds accumulate per line, and the whole sale may come back in pieces.
#[sqlx::test]
async fn v1_5_cumulative_refunds_stop_at_the_quantity_sold(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());
    let f = reversal_fixture(&client, &pool, "w5-cum").await;
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
    let cash_start = cash(&client).await;

    // Two notes, each a fourth of the sale. The second is allowed because the
    // guard is cumulative, not per-note.
    let quarter = rice_qty / 4;
    for _ in 0..2 {
        let resp = post_refund(
            &client,
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
        &client,
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
        &client,
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
        &client,
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
    let final_note: serde_json::Value = resp.json().clone();
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
        find_sale(&client, f.sale_id).await["total"]
            .as_str()
            .expect("sale total is a decimal string"),
    )
    .expect("decimal");
    assert_eq!(
        cash(&client).await,
        cash_start - sale_total,
        "a fully reversed sale is net zero on cash"
    );

    // The sale itself never moved.
    let sale = find_sale(&client, f.sale_id).await;
    // No `status` and no `reversed_at` any more: the sale being an existing row IS
    // its validity (`0015`), and its annulment is read from the notes below. What this
    // asserts is the absence, because a wire that grew the field back would be a
    // contract change nobody asked for.
    assert!(
        sale.get("status").is_none(),
        "a sale has no lifecycle column since 0015"
    );
    assert!(sale.get("reversed_at").is_none());
    assert_eq!(
        sale["credit_notes"].as_array().expect("notes").len(),
        4,
        "the correction is four documents, and the sale is not one of them"
    );
}

/// Finds one sale in the list endpoint by id.
async fn find_sale(client: &common::Client, sale_id: uuid::Uuid) -> serde_json::Value {
    let response = client.get("/api/sales?limit=200").await;
    response.expect_status(axum::http::StatusCode::OK);
    response
        .json()
        .as_array()
        .expect("sales array")
        .iter()
        .find(|sale| sale["id"] == sale_id.to_string())
        .expect("the sale is in the list")
        .clone()
}

// S2, S6, S15: the guards the new tables carry, probed the way 0003's are.
#[sqlx::test]
async fn v1_5_credit_note_history_guards_are_enforced(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());
    let f = reversal_fixture(&client, &pool, "w5-guard").await;
    let (rice_item, _, _, _) = f.lines[0].clone();
    let resp = post_refund(
        &client,
        f.sale_id,
        serde_json::json!([{ "sale_item_id": rice_item, "quantity": 10 }]),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let note: serde_json::Value = resp.json().clone();
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
#[sqlx::test]
async fn v1_5_cash_still_counts_every_confirmed_sale_and_nothing_else(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    // A sale of our own, then a partial refund of it: the note this test measures
    // against exists because the test made it, on a clean database or a dirty one.
    let f = reversal_fixture(&client, &pool, "w5-preserve").await;
    let partial = post_refund(
        &client,
        f.sale_id,
        serde_json::json!([{ "sale_item_id": f.lines[0].0, "quantity": 50 }]),
    )
    .await;
    assert_eq!(partial.status(), 201, "the fixture's own refund must land");
    let note: serde_json::Value = partial.json().clone();
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
    let confirmed: rust_decimal::Decimal =
        sqlx::query_scalar("SELECT COALESCE(SUM(total), 0)::numeric FROM sales")
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
    // Since v2 the endpoint is also net of purchases. The test's own arithmetic
    // has to be too, or it asserts the previous definition of an endpoint whose
    // meaning changed — which is what happened here first and read as a
    // regression. The definition is restated in full on purpose: a test that
    // mirrors the implementation's query instead of the rule would agree with a
    // wrong query.
    let spent: rust_decimal::Decimal =
        sqlx::query_scalar("SELECT COALESCE(SUM(total), 0)::numeric FROM purchases")
            .fetch_one(&pool)
            .await
            .expect("purchases sum");
    // Since v2.5 the endpoint also adds back what suppliers returned. Same reason
    // as above: the test restates the RULE, never the implementation's query,
    // because a test that mirrors the query would agree with a wrong query. This
    // is the second time this assertion had to grow a term, and it is the shape
    // changing meaning rather than the code breaking.
    let returned: rust_decimal::Decimal =
        sqlx::query_scalar("SELECT COALESCE(SUM(total), 0)::numeric FROM supplier_returns")
            .fetch_one(&pool)
            .await
            .expect("supplier returns sum");
    assert_eq!(
        cash(&client).await,
        confirmed - refunded - spent + returned,
        "cash on hand is confirmed sales, net of every credit note and purchase, plus every supplier return"
    );

    // And the sale path itself still only moves cash up: a second sale raises it
    // by exactly its own total.
    let cash_before = cash(&client).await;
    let g = reversal_fixture(&client, &pool, "w5-preserve-2").await;
    let sale = find_sale(&client, g.sale_id).await;
    let total = rust_decimal::Decimal::from_str_exact(
        sale["total"]
            .as_str()
            .expect("sale total is a decimal string"),
    )
    .expect("decimal");
    assert_eq!(
        cash(&client).await,
        cash_before + total,
        "a recorded sale still raises cash by its total"
    );
}

// S3 regression (found by the independent verifier, not by the author): one
// request naming the same sale line twice must not bypass the cumulative guard.
// The handler used to read the refunded sum from the database per entry, so two
// entries of one `sale_item_id` each passed against the same pre-existing sum
// and together over-refunded the line: 2 x 6 against a line of 10 was accepted.
#[sqlx::test]
async fn v1_5_one_request_cannot_over_refund_a_line_by_repeating_it(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());
    let f = reversal_fixture(&client, &pool, "w5-dup").await;
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
    let cash_before = cash(&client).await;

    let resp = post_refund(
        &client,
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
    assert_eq!(cash_before, cash(&client).await, "and cash is unmoved");
}

// S3 regression (boundary, from the verifier's recheck): the cumulative guard is
// an inequality, and the interesting cases are the two sides of it. A request
// whose entries sum exactly to what the line has left must be accepted; one unit
// more must be refused. The first fix for the repeated-line bypass could have
// been written off-by-one in either direction, so the boundary is pinned here.
#[sqlx::test]
async fn v1_5_repeated_line_is_decided_on_the_exact_boundary(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());
    let f = reversal_fixture(&client, &pool, "b2-boundary").await;
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
        &client,
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
    let cash_before = cash(&client).await;
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
        let resp = post_refund(&client, f.sale_id, items).await;
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
    assert_eq!(cash_before, cash(&client).await, "and cash is unmoved");
}

// ── v1.5 es-AR locale: the shop's day (S7, S9) ──────────────────────────────
//
// The day boundary is the one piece of the locale the server owns, because the
// browser's timezone is whatever the machine happens to be configured with.
// These tests pin the wire contract the ledger depends on: four ISO instants, a
// half-open window, and a refusal rather than a guess for a malformed instant.

/// Reads `GET /api/day` for one instant and returns the parsed body.
async fn shop_day(client: &common::Client, at: &str) -> serde_json::Value {
    let query = common::encode_query(at);
    let resp = client.get(&format!("/api/day?at={query}")).await;
    assert_eq!(resp.status(), 200, "the day endpoint answers for {at}");
    resp.json().clone()
}

#[sqlx::test]
async fn v1_5_the_shop_day_is_half_open_and_adjacent_days_meet_at_midnight(pool: sqlx::PgPool) {
    let client = common::client(pool);

    let body = shop_day(&client, "2026-10-16T04:00:00Z").await;

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
        shop_day(&client, "2026-10-16T04:00:00Z").await["zone"],
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
    let next = shop_day(&client, body["to"].as_str().unwrap()).await;
    assert_eq!(
        next["from"], body["to"],
        "adjacent days partition the ledger"
    );

    // Paging one day back lands on this day's start: the ledger walks local days.
    let prev = shop_day(&client, body["prev"].as_str().unwrap()).await;
    assert_eq!(prev["to"], body["from"], "prev's end is this day's start");
}

#[sqlx::test]
async fn v1_5_the_shop_day_puts_a_late_evening_sale_inside_its_own_day(pool: sqlx::PgPool) {
    let client = common::client(pool);

    // 22:30 on 16 Oct in Buenos Aires is already 01:30 on 17 Oct in UTC. A UTC
    // day would file it under the wrong date, which is the defect this closes.
    let body = shop_day(&client, "2026-10-17T01:30:00Z").await;
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

#[sqlx::test]
async fn v1_5_a_malformed_day_is_a_refusal_not_a_guess(pool: sqlx::PgPool) {
    let client = common::client(pool);

    for raw in ["not-a-date", "2026-13-01T00:00:00Z", "16/10/2026", " "] {
        let query = common::encode_query(raw);
        let resp = client.get(&format!("/api/day?at={query}")).await;
        assert_eq!(
            resp.status(),
            400,
            "{raw:?} is not an instant; guessing one would file sales under a day the shop never chose"
        );
    }

    // No `at` at all is not an error: the server answers for its own now.
    let resp = client.get("/api/day").await;
    assert_eq!(resp.status(), 200, "an absent at means now");
    let body = resp.json().clone();
    let from: chrono::DateTime<chrono::Utc> = body["from"].as_str().unwrap().parse().unwrap();
    let to: chrono::DateTime<chrono::Utc> = body["to"].as_str().unwrap().parse().unwrap();
    let now = chrono::Utc::now();
    assert!(
        from <= now && now < to,
        "the answer to 'what day is it' must contain the moment it was asked"
    );
}

// ── v2 redesign: the ledger can be scoped to a week or a month ──────────────
//
// Same contract as the day: the server names the boundaries, the client sends a
// word. These pin the three things that would be silent if wrong — the default
// stays exactly as it was, an unknown scope is refused rather than guessed, and
// every scope actually contains the instant it was asked about.

async fn scoped(pool: sqlx::PgPool, at: &str, scope: &str) -> serde_json::Value {
    let client = common::client(pool);
    let at_q = common::encode_query(at);
    let resp = client
        .get(&format!("/api/day?at={at_q}&scope={scope}"))
        .await;
    assert_eq!(resp.status(), 200, "{scope} answers for {at}");
    resp.json().clone()
}

// ── The redesign fix: the period's totals, not the page's ───────────────────
//
// The ledger showed a period total built from the rows it had loaded — a page of
// at most 50 — and a running balance that therefore started at zero in the
// middle of the period. These pin the summary the screen now reads instead,
// including the case the native review found: a partly refunded sale counts its
// net, never its gross.

async fn summary(client: &common::Client, from: &str, to: &str) -> serde_json::Value {
    let f = common::encode_query(from);
    let t = common::encode_query(to);
    let resp = client
        .get(&format!("/api/sales/summary?from={f}&to={t}"))
        .await;
    assert_eq!(resp.status(), 200, "the summary answers for {from}..{to}");
    resp.json().clone()
}

/// Creates a product and records one sale of `quantity` at `price`.
async fn sell(client: &common::Client, price: &str, quantity: i64) -> serde_json::Value {
    let name = common::unique("summary-product");
    let created = client
        .post(
            "/api/products",
            serde_json::json!({
                "name": name, "list_price": price, "unit": "unit", "initial_stock": 500
            }),
        )
        .await;
    created.expect_status(axum::http::StatusCode::CREATED);
    let product = created.json().clone();

    let sale = client
        .post(
            "/api/sales",
            serde_json::json!({
                "items": [{ "product_id": product["id"], "quantity": quantity }]
            }),
        )
        .await;
    sale.expect_status(axum::http::StatusCode::CREATED);
    sale.json().clone()
}

#[sqlx::test]
async fn v2_the_period_summary_is_empty_for_a_period_with_no_sales(pool: sqlx::PgPool) {
    let client = common::client(pool);
    let body = summary(&client, "2026-01-01T00:00:00Z", "2026-01-02T00:00:00Z").await;

    assert_eq!(body["gross"], "0.00");
    assert_eq!(body["refunded"], "0.00");
    assert_eq!(body["net"], "0.00");
    assert_eq!(body["documents"], 0);
}

#[sqlx::test]
async fn v2_a_partly_refunded_sale_counts_its_net_not_its_gross(pool: sqlx::PgPool) {
    // The defect the native review found. A 100.00 sale with a 30.00 credit note
    // put 70.00 in the till; a total built from gross figures says 100.00 and
    // overstates the shop's money by exactly what it gave back.
    // Four units at 25.00 = a 100.00 sale; one unit comes back for 25.00. The
    // refund's price is read from the sale line, never sent by the client — that
    // rule is `plan-v2.5`'s and it is what makes "a 30.00 credit note" a
    // quantity story, not an amount one.
    let client = common::client(pool);
    let sale = sell(&client, "25.00", 4).await;
    assert_eq!(sale["total"], "100.00");
    let sale_id = sale["id"].as_str().unwrap();
    let item_id = sale["items"][0]["id"].as_str().unwrap();

    client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({
                "items": [{ "sale_item_id": item_id, "quantity": 1 }]
            }),
        )
        .await
        .expect_status(axum::http::StatusCode::CREATED);

    let body = summary(&client, "2000-01-01T00:00:00Z", "2100-01-01T00:00:00Z").await;
    assert_eq!(
        body["gross"], "100.00",
        "the document's own figure is unchanged"
    );
    assert_eq!(body["refunded"], "25.00");
    assert_eq!(body["net"], "75.00", "what the till actually kept");
    assert_eq!(body["documents"], 1);
}

#[sqlx::test]
async fn v2_a_fully_refunded_sale_contributes_nothing_to_the_net(pool: sqlx::PgPool) {
    let client = common::client(pool);
    let sale = sell(&client, "100.00", 1).await;
    let sale_id = sale["id"].as_str().unwrap();
    let item_id = sale["items"][0]["id"].as_str().unwrap();

    client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({
                "items": [{ "sale_item_id": item_id, "quantity": 1 }]
            }),
        )
        .await
        .expect_status(axum::http::StatusCode::CREATED);

    let body = summary(&client, "2000-01-01T00:00:00Z", "2100-01-01T00:00:00Z").await;
    assert_eq!(
        body["net"], "0.00",
        "money that came straight back is not money kept"
    );
    // It still counts as a document: the shop worked it, and the ledger shows it.
    assert_eq!(body["documents"], 1);
}

#[sqlx::test]
async fn v2_the_summary_covers_the_whole_period_not_a_page_of_it(pool: sqlx::PgPool) {
    // More documents than one page holds. The page-based total this endpoint
    // replaced stopped at the limit and quietly undercounted.
    let client = common::client(pool);
    for _ in 0..60 {
        sell(&client, "10.00", 1).await;
    }

    let body = summary(&client, "2000-01-01T00:00:00Z", "2100-01-01T00:00:00Z").await;
    assert_eq!(body["documents"], 60);
    assert_eq!(
        body["net"], "600.00",
        "sixty tens, not the fifty a page shows"
    );
}

#[sqlx::test]
async fn v2_the_default_scope_is_still_exactly_one_day(pool: sqlx::PgPool) {
    // The selector is new and every existing caller sends no `scope`. If the
    // default moved, every older client would silently start showing a week.
    let at = "2026-10-16T04:00:00Z";
    let client = common::client(pool);
    let at_q = common::encode_query(at);

    let bare = client
        .get(&format!("/api/day?at={at_q}"))
        .await
        .json()
        .clone();
    let explicit = client
        .get(&format!("/api/day?at={at_q}&scope=day"))
        .await
        .json()
        .clone();
    assert_eq!(
        bare, explicit,
        "an absent scope and `day` are the same answer"
    );

    // And it really is one day, not a week that happens to match itself.
    let from: chrono::DateTime<chrono::Utc> = bare["from"].as_str().unwrap().parse().unwrap();
    let to: chrono::DateTime<chrono::Utc> = bare["to"].as_str().unwrap().parse().unwrap();
    assert_eq!((to - from).num_days(), 1, "the default is one shop day");
}

#[sqlx::test]
async fn v2_a_week_scope_spans_seven_shop_days_and_contains_the_instant(pool: sqlx::PgPool) {
    let body = scoped(pool, "2026-10-16T04:00:00Z", "week").await;
    let from: chrono::DateTime<chrono::Utc> = body["from"].as_str().unwrap().parse().unwrap();
    let to: chrono::DateTime<chrono::Utc> = body["to"].as_str().unwrap().parse().unwrap();
    let at: chrono::DateTime<chrono::Utc> = "2026-10-16T04:00:00Z".parse().unwrap();

    assert!(
        from <= at && at < to,
        "the week must contain the instant asked about"
    );
    assert_eq!((to - from).num_days(), 7, "an Argentine week is seven days");
}

#[sqlx::test]
async fn v2_a_month_scope_spans_the_calendar_month(pool: sqlx::PgPool) {
    let body = scoped(pool, "2026-10-16T04:00:00Z", "month").await;
    let from: chrono::DateTime<chrono::Utc> = body["from"].as_str().unwrap().parse().unwrap();
    let to: chrono::DateTime<chrono::Utc> = body["to"].as_str().unwrap().parse().unwrap();

    assert_eq!((to - from).num_days(), 31, "October is 31 days");
}

#[sqlx::test]
async fn v2_an_unknown_scope_is_refused_rather_than_answered_with_a_day(pool: sqlx::PgPool) {
    // Answering a `year` request with a day would show a page whose button and
    // whose numbers disagree, which is worse than an error the client can show.
    let client = common::client(pool);
    let at = common::encode_query("2026-10-16T04:00:00Z");
    for scope in ["year", "WEEK", "weeks", "7"] {
        let resp = client.get(&format!("/api/day?at={at}&scope={scope}")).await;
        assert_eq!(
            resp.status(),
            400,
            "{scope:?} is not a scope this endpoint knows"
        );
    }
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

#[sqlx::test]
async fn v2_schema_creates_purchases_tables_and_seeds_one_off_supplier(pool: sqlx::PgPool) {
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

#[sqlx::test]
async fn v2_a_purchase_is_immutable_after_it_exists(pool: sqlx::PgPool) {
    let mut tx = pool.begin().await.expect("begin");

    let pid: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO products (name, list_price) VALUES ('v2-guard-p', 10.00) RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("insert product");

    let purchase_id: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO purchases (supplier_id, total, payment_method_id) VALUES ($1, 20.00, '00000000-0000-0000-0000-000000000004') RETURNING id",
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
        "INSERT INTO purchases (supplier_id, total, payment_method_id) VALUES ($1, 20.005, '00000000-0000-0000-0000-000000000004')",
        uuid::Uuid::parse_str(ONE_OFF_SUPPLIER_ID).expect("fixed uuid")
    );

    tx.rollback().await.expect("rollback");
}

#[sqlx::test]
async fn v2_the_ledger_refuses_a_purchase_movement_without_its_document(pool: sqlx::PgPool) {
    let mut tx = pool.begin().await.expect("begin");
    let pid: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO products (name, list_price) VALUES ('v2-coherence-p', 5.00) RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("insert product");

    let purchase_id: (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO purchases (supplier_id, total, payment_method_id) VALUES ($1, 0.00, '00000000-0000-0000-0000-000000000004') RETURNING id")
            .bind(uuid::Uuid::parse_str(ONE_OFF_SUPPLIER_ID).expect("fixed uuid"))
            .fetch_one(&mut *tx)
            .await
            .expect("insert zero-total purchase");

    // The product must be ON the purchase for a movement to explain its stock.
    // This fixture did not do that, and it passed until 0011 added the rule that
    // a movement's product has to be on the document it names. The insert is not
    // ceremony now: without it the positive case below is refused, correctly, for
    // a reason this test is not about.
    sqlx::query(
        "INSERT INTO purchase_items
            (purchase_id, product_id, product_name, quantity, unit_price, line_total, unit)
         VALUES ($1, $2, 'v2-coherence-p', 10, 0.00, 0.00, 'unit')",
    )
    .bind(purchase_id.0)
    .bind(pid.0)
    .execute(&mut *tx)
    .await
    .expect("insert purchase item");

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
        "INSERT INTO sales (customer_id, total, payment_method_id) VALUES ('00000000-0000-0000-0000-000000000001', 0.00, '00000000-0000-0000-0000-000000000004') RETURNING id",
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

// ── v2 (T2): the suppliers endpoints ───────────────────────────────────────
//
// plan-v2.md S4/S5: a supplier is the mirror of a customer — name only, no
// editing, no delete, and the seeded one-off supplier is reachable and is what
// a purchase without a named supplier falls back to.
//
// Every test here is self-relative (S17): it creates the rows it measures and
// never asserts a count or a total it did not cause. This suite runs against the
// shared development database, which already holds real suppliers by the time
// it runs a second time.

/// Create a supplier over HTTP and return it. Names are made unique per call so
/// a second run against the same database does not collide with the first.
async fn v2_create_supplier(client: &common::Client, name: &str) -> serde_json::Value {
    let response = client
        .post("/api/suppliers", serde_json::json!({ "name": name }))
        .await;
    assert!(
        response.status().is_success(),
        "posting a supplier must succeed, got {}: {}",
        response.status(),
        response.text
    );
    response.json().clone()
}

#[sqlx::test]
async fn v2_suppliers_list_and_create(pool: sqlx::PgPool) {
    let client = common::client(pool);

    // A name unique to this run, so a failure message names its own row. The
    // database is this test's own now, so the uniqueness is for readability
    // rather than to dodge the constraint.
    let name = common::unique("v2-supplier");
    let created = v2_create_supplier(&client, &name).await;
    assert_eq!(
        created["name"], name,
        "the server echoes the name it stored"
    );
    let id = created["id"].as_str().expect("id").to_string();

    // It appears in the list, and the seeded one-off supplier is there too —
    // both are what a caller relies on: the picker needs the seed in the list,
    // and a created supplier must be visible without a second endpoint.
    let list_resp = client.get("/api/suppliers").await;
    list_resp.expect_status(axum::http::StatusCode::OK);
    let list = list_resp.json().clone();
    let names: Vec<&str> = list
        .as_array()
        .expect("array")
        .iter()
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert!(
        names.contains(&name.as_str()),
        "the created supplier is listed"
    );
    assert!(
        names.contains(&"one-off-supplier"),
        "the seeded supplier must be listable: a purchase falls back to it"
    );

    // The seeded supplier's id is a contract with the migration and with the
    // purchase default, so the list must carry exactly that id.
    let seeded = list
        .as_array()
        .expect("array")
        .iter()
        .find(|s| s["name"] == "one-off-supplier")
        .expect("seeded supplier in list");
    assert_eq!(
        seeded["id"], "00000000-0000-0000-0000-000000000002",
        "the seeded supplier keeps its fixed id"
    );

    // Duplicate name -> 409, not 500 and not a silent second row.
    let duplicate = client
        .post("/api/suppliers", serde_json::json!({ "name": name }))
        .await;
    assert_eq!(
        duplicate.status(),
        409,
        "a duplicate supplier name is a conflict, {id} already holds it"
    );

    // A blank name is a client error, not a row: whitespace only is empty.
    for blank in ["", "   ", "\t\n"] {
        let resp = client
            .post("/api/suppliers", serde_json::json!({ "name": blank }))
            .await;
        assert_eq!(
            resp.status(),
            400,
            "{blank:?} is not a supplier name; storing it would make the picker unreadable"
        );
    }
}

// ── v2 (T3): POST /api/purchases ───────────────────────────────────────────
//
// plan-v2.md S1/S7/S8/S11: one transaction writes the header, its lines and one
// POSITIVE movement per line, and a rejected purchase writes nothing at all.
//
// Every measurement here is self-relative (S17). The suite runs against the
// shared development database, so "stock went up by 7" is stated as
// after - before, never as an absolute figure.

/// Read one product's derived stock, which is the SUM of its movements.
async fn v2_stock(pool: &sqlx::PgPool, product_id: uuid::Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(quantity), 0)::bigint FROM stock_movements WHERE product_id = $1",
    )
    .bind(product_id)
    .fetch_one(pool)
    .await
    .expect("derived stock")
}

#[sqlx::test]
async fn v2_a_purchase_raises_stock_and_returns_its_lines(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    // Its own product: a shared one would make the before/after meaningless as
    // soon as another test ran concurrently in CI.
    let stamp = uuid::Uuid::new_v4();
    let product: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO products (name, list_price, unit) VALUES ($1, 12.50, 'unit') RETURNING id",
    )
    .bind(format!("v2-purchase-{stamp}"))
    .fetch_one(&pool)
    .await
    .expect("insert product");

    let stock_before = v2_stock(&pool, product.0).await;

    // The supplier is omitted on purpose: this is the seeded default path (S4).
    // The price on the first line is omitted too, which must fall back to the
    // product's list price (S7); the second line overrides it, because a
    // purchase price that differs from the catalogue is the normal case.
    let resp = client
        .post(
            "/api/purchases",
            serde_json::json!({
                "items": [
                    { "product_id": product.0, "quantity": 4 },
                    { "product_id": product.0, "quantity": 2, "unit_price": "10.00" }
                ]
            }),
        )
        .await;
    assert_eq!(resp.status(), 201, "a valid purchase is created");
    let body = resp.json().clone();

    // The header carries the seeded supplier and the summed total: 4 * 12.50 +
    // 2 * 10.00 = 70.00.
    assert_eq!(
        body["purchase"]["supplier_id"], "00000000-0000-0000-0000-000000000002",
        "an omitted supplier falls back to the seeded one-off supplier"
    );
    assert_eq!(
        body["purchase"]["total"], "70.00",
        "total = SUM(line_total), with the list price where none was given"
    );

    let items = body["items"].as_array().expect("items array");
    assert_eq!(items.len(), 2, "both lines are returned");

    // The snapshot and the price rules, line by line. Repeatable product in one
    // purchase is deliberate: two lots at two costs is a real purchase.
    assert_eq!(items[0]["product_name"], format!("v2-purchase-{stamp}"));
    assert_eq!(
        items[0]["unit_price"], "12.50",
        "omitted price = list price"
    );
    assert_eq!(items[0]["line_total"], "50.00", "4 * 12.50");
    assert_eq!(items[1]["unit_price"], "10.00", "the override is kept");
    assert_eq!(items[1]["line_total"], "20.00", "2 * 10.00");
    assert_eq!(items[0]["unit"], "unit", "the unit is snapshotted");

    // The point of the slice: stock went UP by exactly what arrived, and the
    // movements are positive and carry the purchase document.
    let stock_after = v2_stock(&pool, product.0).await;
    assert_eq!(
        stock_after - stock_before,
        6,
        "a purchase of 4 + 2 raises stock by 6, never lowers it"
    );

    let purchase_id = body["purchase"]["id"].as_str().expect("purchase id");
    let movements: (i64, i64) = sqlx::query_as(
        "SELECT count(*), COALESCE(SUM(quantity), 0)::bigint FROM stock_movements
          WHERE purchase_id = $1 AND reason = 'purchase'",
    )
    .bind(uuid::Uuid::parse_str(purchase_id).expect("uuid"))
    .fetch_one(&pool)
    .await
    .expect("movements");
    assert_eq!(movements.0, 2, "one movement per line");
    assert_eq!(movements.1, 6, "the movements sum to what arrived");
}

#[sqlx::test]
async fn v2_a_rejected_purchase_writes_nothing(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    let stamp = uuid::Uuid::new_v4();
    let product: (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO products (name, list_price) VALUES ($1, 5.00) RETURNING id")
            .bind(format!("v2-reject-{stamp}"))
            .fetch_one(&pool)
            .await
            .expect("insert product");
    let unknown = uuid::Uuid::new_v4();

    // S11 measured the only way that means anything: count every row that a
    // failed purchase could have left behind, then count again.
    async fn counts(pool: &sqlx::PgPool) -> (i64, i64, i64) {
        let p: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM purchases")
            .fetch_one(pool)
            .await
            .expect("purchases count");
        let i: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM purchase_items")
            .fetch_one(pool)
            .await
            .expect("items count");
        let m: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM stock_movements")
            .fetch_one(pool)
            .await
            .expect("movements count");
        (p, i, m)
    }
    let before = counts(&pool).await;

    // Each shape that must be refused, with the status it owes.
    let cases: Vec<(&str, serde_json::Value, u16)> = vec![
        (
            "an empty item list",
            serde_json::json!({ "items": [] }),
            400,
        ),
        (
            "a quantity of zero",
            serde_json::json!({ "items": [{ "product_id": product.0, "quantity": 0 }] }),
            400,
        ),
        (
            "a negative quantity",
            serde_json::json!({ "items": [{ "product_id": product.0, "quantity": -3 }] }),
            400,
        ),
        (
            "a price finer than a cent",
            serde_json::json!({ "items": [
                { "product_id": product.0, "quantity": 1, "unit_price": "5.005" }
            ] }),
            400,
        ),
        (
            "an unknown product",
            serde_json::json!({ "items": [{ "product_id": unknown, "quantity": 1 }] }),
            404,
        ),
        (
            "an unknown supplier",
            serde_json::json!({
                "supplier_id": unknown,
                "items": [{ "product_id": product.0, "quantity": 1 }]
            }),
            404,
        ),
        (
            // The mixed case is the one a per-line check can get wrong: a valid
            // line before the invalid one must not be written either.
            "a valid line followed by an unknown product",
            serde_json::json!({ "items": [
                { "product_id": product.0, "quantity": 2 },
                { "product_id": unknown, "quantity": 1 }
            ] }),
            404,
        ),
    ];

    for (label, payload, expected) in cases {
        let resp = client.post("/api/purchases", payload).await;
        assert_eq!(resp.status(), expected, "{label} owes {expected}");
    }

    let after = counts(&pool).await;
    assert_eq!(
        before, after,
        "a rejected purchase wrote something: header, line or movement survived"
    );
}

// ── v2 (T4): GET /api/purchases ────────────────────────────────────────────
//
// plan-v2.md S12: the same half-open [from, to) day scoping the sales ledger
// uses, the same clamped paging, and each purchase returned with its lines.
// The contract is shared with sales on purpose — a day filter that means one
// thing on one screen and another on the next is a bug that reads as a UI fault.

// ── Purchases: the period's totals, not the page's ─────────────────────────
//
// Same finding the sales ledger produced, applied before this surface ships it:
// a footer built from the loaded page stops at row fifty and calls itself a
// period. Purchases and returns are counted separately because a return is its
// own document correcting a purchase, not a negative purchase.

async fn purchases_summary(client: &common::Client, from: &str, to: &str) -> serde_json::Value {
    let f = common::encode_query(from);
    let t = common::encode_query(to);
    let resp = client
        .get(&format!("/api/purchases/summary?from={f}&to={t}"))
        .await;
    assert_eq!(resp.status(), 200, "the summary answers for {from}..{to}");
    resp.json().clone()
}

/// One product and one purchase of `quantity` at `price`.
async fn buy(
    pool: &sqlx::PgPool,
    client: &common::Client,
    price: &str,
    quantity: i64,
) -> serde_json::Value {
    let stamp = uuid::Uuid::new_v4();
    let product: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO products (name, list_price) VALUES ($1, $2::numeric) RETURNING id",
    )
    .bind(format!("summary-buy-{stamp}"))
    .bind(price)
    .fetch_one(pool)
    .await
    .expect("insert product");

    let resp = client
        .post(
            "/api/purchases",
            serde_json::json!({
                "items": [{ "product_id": product.0, "quantity": quantity, "unit_price": price }]
            }),
        )
        .await;
    resp.expect_status(axum::http::StatusCode::CREATED);
    resp.json().clone()
}

#[sqlx::test]
async fn v2_purchases_summary_is_empty_for_a_period_with_no_documents(pool: sqlx::PgPool) {
    let client = common::client(pool);
    let body = purchases_summary(&client, "2026-01-01T00:00:00Z", "2026-01-02T00:00:00Z").await;

    assert_eq!(body["gross"], "0.00");
    assert_eq!(body["returned"], "0.00");
    assert_eq!(body["net"], "0.00");
    assert_eq!(body["documents"], 0);
}

#[sqlx::test]
async fn v2_purchases_summary_covers_the_whole_period_not_a_page_of_it(pool: sqlx::PgPool) {
    // Sixty purchases is more than one page holds. A footer derived from the
    // loaded page would stop at fifty and undercount by a fifth.
    let client = common::client(pool.clone());
    for _ in 0..60 {
        buy(&pool, &client, "10.00", 1).await;
    }

    let body = purchases_summary(&client, "2000-01-01T00:00:00Z", "2100-01-01T00:00:00Z").await;
    assert_eq!(body["documents"], 60);
    assert_eq!(
        body["gross"], "600.00",
        "sixty tens, not the fifty a page shows"
    );
}

#[sqlx::test]
async fn v2_a_supplier_return_reduces_the_net_but_not_the_gross(pool: sqlx::PgPool) {
    // The shop reads two facts: what it spent, and what the supplier gave back.
    // A return is not a negative purchase — the purchase keeps its own figure.
    let client = common::client(pool.clone());
    let created = buy(&pool, &client, "100.00", 2).await;
    let purchase_id = created["purchase"]["id"].as_str().unwrap();
    let item_id = created["items"][0]["id"].as_str().unwrap();

    client
        .post(
            &format!("/api/purchases/{purchase_id}/supplier-returns"),
            serde_json::json!({
                "items": [{ "purchase_item_id": item_id, "quantity": 1 }]
            }),
        )
        .await
        .expect_status(axum::http::StatusCode::CREATED);

    let body = purchases_summary(&client, "2000-01-01T00:00:00Z", "2100-01-01T00:00:00Z").await;
    assert_eq!(
        body["gross"], "200.00",
        "the purchase keeps the figure it was recorded with"
    );
    assert_eq!(body["returned"], "100.00", "one of two units came back");
    assert_eq!(body["net"], "100.00", "what the period actually cost");
    assert_eq!(body["documents"], 1);
}

#[sqlx::test]
async fn v2_purchases_list_carries_its_lines_and_is_scoped_by_day(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    let stamp = uuid::Uuid::new_v4();
    let product: (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO products (name, list_price) VALUES ($1, 3.00) RETURNING id")
            .bind(format!("v2-list-{stamp}"))
            .fetch_one(&pool)
            .await
            .expect("insert product");

    let resp = client
        .post(
            "/api/purchases",
            serde_json::json!({
                "items": [{ "product_id": product.0, "quantity": 2, "unit_price": "3.00" }]
            }),
        )
        .await;
    assert_eq!(resp.status(), 201, "a valid purchase is created");
    let created = resp.json().clone();
    let id = created["purchase"]["id"].as_str().expect("id").to_string();

    // Its own recorded instant is the boundary. Measuring the contract against
    // the real timestamp needs no fixture surgery: disabling the immutability
    // trigger to rewrite `created_at` would leave a shared database unguarded if
    // this test panicked halfway, which is a worse failure than the one being
    // tested for.
    let at = created["purchase"]["created_at"]
        .as_str()
        .expect("created_at");

    async fn listed(client: &common::Client, from: &str, to: &str) -> Vec<String> {
        let query = format!(
            "from={}&to={}&limit=200",
            common::encode_query(from),
            common::encode_query(to)
        );
        let response = client.get(&format!("/api/purchases?{query}")).await;
        response.expect_status(axum::http::StatusCode::OK);
        response
            .json()
            .as_array()
            .expect("array")
            .iter()
            .filter_map(|p| p["purchase"]["id"].as_str().map(str::to_string))
            .collect()
    }

    // `from` is INCLUSIVE: a window that starts exactly at the document's own
    // instant contains it. An exclusive `from` would lose a document recorded at
    // midnight from both neighbouring days.
    let after_boundary = "9999-01-01T00:00:00Z";
    assert!(
        listed(&client, at, after_boundary).await.contains(&id),
        "`from` is inclusive: a window starting at the document's instant contains it"
    );

    // `to` is EXCLUSIVE: a window ending exactly at that instant does not. This
    // is the assertion an inclusive `to` fails, and the reason adjacent days
    // partition the ledger instead of double-counting.
    let before_boundary = "1970-01-01T00:00:00Z";
    assert!(
        !listed(&client, before_boundary, at).await.contains(&id),
        "`to` is exclusive: a window ending at the document's instant excludes it"
    );

    // And the document travels with its lines, so a ledger row can be expanded
    // without a second request per document.
    let query = format!(
        "from={}&to={}&limit=200",
        common::encode_query(at),
        common::encode_query(after_boundary)
    );
    let body_resp = client.get(&format!("/api/purchases?{query}")).await;
    body_resp.expect_status(axum::http::StatusCode::OK);
    let body = body_resp.json().clone();
    let mine = body
        .as_array()
        .expect("array")
        .iter()
        .find(|p| p["purchase"]["id"] == id.as_str())
        .expect("the purchase is in its own day");
    let items = mine["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "the line travels with its purchase");
    assert_eq!(items[0]["quantity"], 2);
    assert_eq!(items[0]["unit_price"], "3.00");
    assert_eq!(items[0]["line_total"], "6.00");

    // Paging is a page size, not a filter: a limit of one returns one document.
    let one_query = format!(
        "from={}&to={}&limit=1",
        common::encode_query(at),
        common::encode_query(after_boundary)
    );
    let one_resp = client.get(&format!("/api/purchases?{one_query}")).await;
    one_resp.expect_status(axum::http::StatusCode::OK);
    let one = one_resp.json().clone();
    assert_eq!(
        one.as_array().expect("array").len(),
        1,
        "limit is a page size, not a filter"
    );

    // A negative offset is a client mistake, not a server error. Postgres refuses
    // a negative OFFSET, and turning that into a 500 blames the server for a bad
    // query — the same clamp the sales ledger applies.
    let negative = client.get("/api/purchases?offset=-5").await;
    assert_eq!(
        negative.status(),
        200,
        "a negative offset is clamped, not a 500"
    );
}

// ── v2 (T5): GET /api/cash nets purchases ──────────────────────────────────
//
// plan-v2.md S13: the response shape does not change — one decimal string — but
// its meaning does. Money in the till is sales, minus refunds, minus purchases.
//
// The second test here is the "existing behaviour still holds" test the slice
// owes: the query changed, so a sale must still move cash by exactly its total
// and a purchase must move it by exactly its own. Without it, a subtraction
// added in the wrong direction would look green in the new test alone.

#[sqlx::test]
async fn v2_cash_nets_purchases_and_still_counts_sales(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    let stamp = uuid::Uuid::new_v4();
    let product: (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO products (name, list_price) VALUES ($1, 100.00) RETURNING id")
            .bind(format!("v2-cash-{stamp}"))
            .fetch_one(&pool)
            .await
            .expect("insert product");

    async fn cash(client: &common::Client) -> String {
        // `T6` retired `/api/cash`; the till's balance is the default account's
        // derived balance, and it is the same number by the same view.
        let response = client.get("/api/accounts").await;
        response.expect_status(axum::http::StatusCode::OK);
        response
            .json()
            .as_array()
            .expect("accounts array")
            .iter()
            .find(|a| a["is_default"].as_bool().unwrap_or(false))
            .expect("a default account")["balance"]
            .as_str()
            .expect("balance string")
            .to_string()
    }
    fn cents(s: &str) -> i64 {
        // Test-only arithmetic on a wire amount: parse the two-decimal string
        // exactly, never through a float.
        let (sign, rest) = match s.strip_prefix('-') {
            Some(r) => (-1i64, r),
            None => (1i64, s),
        };
        let (whole, frac) = rest.split_once('.').expect("two decimals");
        sign * (whole.parse::<i64>().expect("whole") * 100 + frac.parse::<i64>().expect("fraction"))
    }

    let before = cents(&cash(&client).await);

    // A purchase of 4 * 100.00 = 400.00 must take exactly 400.00 out.
    let resp = client
        .post(
            "/api/purchases",
            serde_json::json!({
                "items": [{ "product_id": product.0, "quantity": 4, "unit_price": "100.00" }]
            }),
        )
        .await;
    assert_eq!(resp.status(), 201);

    let after_purchase = cents(&cash(&client).await);
    assert_eq!(
        before - after_purchase,
        40_000,
        "a purchase lowers the till by its total: money left"
    );

    // And the sale side still counts in the other direction, by its own total.
    // This is the regression the changed query could have introduced: a sign
    // error would still produce a plausible number.
    let resp = client
        .post(
            "/api/sales",
            serde_json::json!({
                "items": [{ "product_id": product.0, "quantity": 2, "unit_price": "25.00" }]
            }),
        )
        .await;
    assert_eq!(resp.status(), 201);

    let after_sale = cents(&cash(&client).await);
    assert_eq!(
        after_sale - after_purchase,
        5_000,
        "a sale still raises the till by its total: money came in"
    );

    // The shape is unchanged: one decimal string with two places, which is what
    // every consumer already reads.
    let raw = cash(&client).await;
    assert!(
        raw.split('.').nth(1).map(|f| f.len()) == Some(2),
        "cash is still a two-decimal string, got {raw:?}"
    );
}

/// S10: `purchases` and `purchase_items` refuse TRUNCATE as well as UPDATE and
/// DELETE, and every refusal leaves both tables byte-identical.
///
/// The gap this closes was found by an independent verifier and then reproduced:
/// dropping `purchases_no_truncate` and `purchase_items_no_truncate` together
/// left `v2_a_purchase_is_immutable_after_it_exists` GREEN, because that test
/// only probes UPDATE and DELETE. A guard no test would miss is not a guard.
#[sqlx::test]
async fn v2_purchase_tables_refuse_truncate_and_keep_their_state(pool: sqlx::PgPool) {
    let mut tx = pool.begin().await.expect("begin");

    let pid: (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO products (name, list_price) VALUES ($1, 1.00) RETURNING id")
            .bind(format!("v2-truncate-{}", uuid::Uuid::new_v4()))
            .fetch_one(&mut *tx)
            .await
            .expect("insert product");
    let purchase_id: (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO purchases (supplier_id, total, payment_method_id) VALUES ($1, 1.00, '00000000-0000-0000-0000-000000000004') RETURNING id")
            .bind(uuid::Uuid::parse_str(ONE_OFF_SUPPLIER_ID).expect("fixed uuid"))
            .fetch_one(&mut *tx)
            .await
            .expect("insert purchase");
    sqlx::query(
        "INSERT INTO purchase_items
            (purchase_id, product_id, product_name, quantity, unit_price, line_total, unit)
         VALUES ($1, $2, 'v2-truncate', 1, 1.00, 1.00, 'unit')",
    )
    .bind(purchase_id.0)
    .bind(pid.0)
    .execute(&mut *tx)
    .await
    .expect("insert purchase item");
    sqlx::query(
        "INSERT INTO stock_movements (product_id, quantity, reason, purchase_id)
         VALUES ($1, 1, 'purchase', $2)",
    )
    .bind(pid.0)
    .bind(purchase_id.0)
    .execute(&mut *tx)
    .await
    .expect("insert purchase movement");

    // The state a refusal must leave untouched, hashed rather than counted: a
    // TRUNCATE followed by a re-insert could keep the count and lose the row.
    async fn state(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> (String, String) {
        let purchases: String = sqlx::query_scalar(
            "SELECT COALESCE(md5(string_agg(t::text, '|' ORDER BY t::text)), 'empty')
             FROM purchases t",
        )
        .fetch_one(&mut **tx)
        .await
        .expect("purchases hash");
        let items: String = sqlx::query_scalar(
            "SELECT COALESCE(md5(string_agg(t::text, '|' ORDER BY t::text)), 'empty')
             FROM purchase_items t",
        )
        .fetch_one(&mut **tx)
        .await
        .expect("items hash");
        (purchases, items)
    }

    let before = state(&mut tx).await;

    macro_rules! probe_denied {
        ($label:expr, $sql:expr) => {{
            sqlx::query("SAVEPOINT probe")
                .execute(&mut *tx)
                .await
                .expect("savepoint");
            let result = sqlx::query($sql).execute(&mut *tx).await;
            sqlx::query("ROLLBACK TO SAVEPOINT probe")
                .execute(&mut *tx)
                .await
                .expect("rollback to savepoint");
            assert!(result.is_err(), "{} was allowed", $label);
            assert_eq!(
                state(&mut tx).await,
                before,
                "{} changed the state it was supposed to refuse",
                $label
            );
        }};
    }

    probe_denied!("TRUNCATE purchases", "TRUNCATE purchases");
    probe_denied!("TRUNCATE purchase_items", "TRUNCATE purchase_items");
    probe_denied!("TRUNCATE purchases CASCADE", "TRUNCATE purchases CASCADE");
    probe_denied!(
        "TRUNCATE purchases, purchase_items, stock_movements in one statement",
        "TRUNCATE purchases, purchase_items, stock_movements"
    );

    tx.rollback().await.expect("rollback");
}

/// S9: `0009` must run its defensive pre-check **before** it adds the coherence
/// constraints.
///
/// This is a property of the migration text, so it is tested against the text.
/// `ADD CONSTRAINT` validates existing rows, which means a pre-check placed after
/// it can never fire: PostgreSQL reports the violation first and the migration's
/// own count is never printed. That is exactly what an independent verifier
/// found in the previous revision of this file, and what this test now prevents
/// from coming back. An incoherent legacy database is something the shared
/// development stack cannot supply on demand, which is why this is read rather
/// than executed.
#[test]
fn v2_the_coherence_pre_check_runs_before_the_constraints_it_protects() {
    let sql = include_str!("../migrations/0009_purchases.sql");
    let pre_check = sql
        .find("contradict the v2 coherence rule")
        .expect("0009 must keep its v2 coherence pre-check");
    for constraint in [
        "ADD CONSTRAINT stock_movements_document_coherent",
        "ADD CONSTRAINT stock_movements_purchase_coherent",
        "ADD CONSTRAINT stock_movements_purchase_is_incoming",
    ] {
        let added = sql
            .find(constraint)
            .unwrap_or_else(|| panic!("0009 must add {constraint}"));
        assert!(
            pre_check < added,
            "the defensive pre-check must run BEFORE {constraint}; placed after \
             it, PostgreSQL reports the violation first and the pre-check can \
             never fire"
        );
    }
}

// ── v2.5 (T1): the supplier-returns schema ─────────────────────────────────
//
// plan-v2.5.md: a return is a second document that corrects a purchase, never an
// edit of it. These tests pin the schema's own rules — the tables, the ledger's
// fifth reason, its direction and coherence, and the immutability of both new
// tables. Every one is self-relative and rolls back.

/// Insert a purchase with one line and its movement, and return the ids the
/// return tests need.
#[allow(dead_code)]
async fn v2_5_fixture(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tag: &str,
) -> (uuid::Uuid, uuid::Uuid, uuid::Uuid) {
    let pid: (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO products (name, list_price) VALUES ($1, 10.00) RETURNING id")
            .bind(format!("v2_5-{tag}-{}", uuid::Uuid::new_v4()))
            .fetch_one(&mut **tx)
            .await
            .expect("insert product");
    let purchase_id: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO purchases (supplier_id, total, payment_method_id) VALUES ($1, 30.00, '00000000-0000-0000-0000-000000000004') RETURNING id",
    )
    .bind(uuid::Uuid::parse_str(ONE_OFF_SUPPLIER_ID).expect("fixed uuid"))
    .fetch_one(&mut **tx)
    .await
    .expect("insert purchase");
    let item_id: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO purchase_items
            (purchase_id, product_id, product_name, quantity, unit_price, line_total, unit)
         VALUES ($1, $2, 'v2_5 line', 3, 10.00, 30.00, 'unit') RETURNING id",
    )
    .bind(purchase_id.0)
    .bind(pid.0)
    .fetch_one(&mut **tx)
    .await
    .expect("insert purchase item");
    (pid.0, purchase_id.0, item_id.0)
}

#[sqlx::test]
async fn v2_5_schema_creates_supplier_returns_tables(pool: sqlx::PgPool) {
    for table in ["supplier_returns", "supplier_return_items"] {
        let reg: Option<String> = sqlx::query_scalar("SELECT to_regclass($1)::text")
            .bind(format!("public.{table}"))
            .fetch_one(&pool)
            .await
            .expect("to_regclass");
        assert_eq!(
            reg.as_deref(),
            Some(table),
            "{table} does not exist: the v2.5 migration did not run"
        );
    }
}

#[sqlx::test]
async fn v2_5_a_return_takes_stock_out_and_names_both_documents(pool: sqlx::PgPool) {
    let mut tx = pool.begin().await.expect("begin");

    let (pid, purchase_id, item_id) = v2_5_fixture(&mut tx, "happy").await;

    let return_id: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO supplier_returns (purchase_id, total, payment_method_id) VALUES ($1, 10.00, '00000000-0000-0000-0000-000000000004') RETURNING id",
    )
    .bind(purchase_id)
    .fetch_one(&mut *tx)
    .await
    .expect("a return can be created");

    sqlx::query(
        "INSERT INTO supplier_return_items
            (supplier_return_id, purchase_item_id, product_id, product_name, quantity,
             unit_price, line_total, unit)
         VALUES ($1, $2, $3, 'v2_5 line', 1, 10.00, 10.00, 'unit')",
    )
    .bind(return_id.0)
    .bind(item_id)
    .bind(pid)
    .execute(&mut *tx)
    .await
    .expect("a return line can be created");

    // POSITIVE case first, and it is load-bearing: without it a missing feature
    // would satisfy the refusals below for the wrong reason — a rule absent from
    // the vocabulary refuses everything, which proves nothing (measured in v2).
    sqlx::query(
        "INSERT INTO stock_movements (product_id, quantity, reason, purchase_id, supplier_return_id)
         VALUES ($1, -1, 'supplier_return', $2, $3)",
    )
    .bind(pid)
    .bind(purchase_id)
    .bind(return_id.0)
    .execute(&mut *tx)
    .await
    .expect("a coherent supplier_return movement must be accepted");

    tx.rollback().await.expect("rollback");
}

#[sqlx::test]
async fn v2_5_the_ledger_refuses_incoherent_supplier_return_movements(pool: sqlx::PgPool) {
    let mut tx = pool.begin().await.expect("begin");

    let (pid, purchase_id, _item_id) = v2_5_fixture(&mut tx, "coherence").await;
    let return_id: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO supplier_returns (purchase_id, total, payment_method_id) VALUES ($1, 10.00, '00000000-0000-0000-0000-000000000004') RETURNING id",
    )
    .bind(purchase_id)
    .fetch_one(&mut *tx)
    .await
    .expect("insert return");
    let sale_id: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO sales (customer_id, total, payment_method_id) VALUES ('00000000-0000-0000-0000-000000000001', 0.00, '00000000-0000-0000-0000-000000000004') RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("insert sale");

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

    // The direction. Stock LEAVES on a return; the ledger accepts negatives in
    // general, so without this rule a return could add stock while its document
    // says the goods went back — the mirror of 0009's measured purchase rule.
    probe_denied!(
        "a positive supplier_return movement",
        "INSERT INTO stock_movements (product_id, quantity, reason, purchase_id, supplier_return_id)
         VALUES ($1, 1, 'supplier_return', $2, $3)",
        pid,
        purchase_id,
        return_id.0
    );

    // The document is required: a movement that cannot be traced to the return
    // it corrects is not auditable.
    probe_denied!(
        "a supplier_return movement with no supplier_return_id",
        "INSERT INTO stock_movements (product_id, quantity, reason, purchase_id)
         VALUES ($1, -1, 'supplier_return', $2)",
        pid,
        purchase_id
    );

    // The equality, not a one-sided check: a `sale` row carrying a
    // supplier_return_id must be refused. 0009 measured that a one-sided rule
    // lets this through.
    probe_denied!(
        "a sale movement carrying a supplier_return_id",
        "INSERT INTO stock_movements (product_id, quantity, reason, sale_id, supplier_return_id)
         VALUES ($1, -1, 'sale', $2, $3)",
        pid,
        sale_id.0,
        return_id.0
    );

    tx.rollback().await.expect("rollback");
}

#[sqlx::test]
async fn v2_5_the_return_tables_are_immutable(pool: sqlx::PgPool) {
    let mut tx = pool.begin().await.expect("begin");

    let (pid, purchase_id, item_id) = v2_5_fixture(&mut tx, "immutable").await;
    let return_id: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO supplier_returns (purchase_id, total, payment_method_id) VALUES ($1, 10.00, '00000000-0000-0000-0000-000000000004') RETURNING id",
    )
    .bind(purchase_id)
    .fetch_one(&mut *tx)
    .await
    .expect("insert return");
    let line_id: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO supplier_return_items
            (supplier_return_id, purchase_item_id, product_id, product_name, quantity,
             unit_price, line_total, unit)
         VALUES ($1, $2, $3, 'v2_5 line', 1, 10.00, 10.00, 'unit') RETURNING id",
    )
    .bind(return_id.0)
    .bind(item_id)
    .bind(pid)
    .fetch_one(&mut *tx)
    .await
    .expect("insert return line");

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
        "supplier_return update",
        "UPDATE supplier_returns SET total = 1.00 WHERE id = $1",
        return_id.0
    );
    probe_denied!(
        "supplier_return delete",
        "DELETE FROM supplier_returns WHERE id = $1",
        return_id.0
    );
    probe_denied!(
        "supplier_return created_at rewrite",
        "UPDATE supplier_returns SET created_at = now() WHERE id = $1",
        return_id.0
    );
    probe_denied!(
        "supplier_return_item update",
        "UPDATE supplier_return_items SET quantity = 99 WHERE id = $1",
        line_id.0
    );
    probe_denied!(
        "supplier_return_item delete",
        "DELETE FROM supplier_return_items WHERE id = $1",
        line_id.0
    );
    // A finer-than-a-cent amount is refused on the new money columns.
    probe_denied!(
        "a supplier_return total finer than a cent",
        "INSERT INTO supplier_returns (purchase_id, total, payment_method_id) VALUES ($1, 10.005, '00000000-0000-0000-0000-000000000004')",
        purchase_id
    );

    tx.rollback().await.expect("rollback");
}

/// The pre-check ordering is a property of the migration text, so it is asserted
/// against the text. Placed after the constraints it protects it can never fire:
/// `ADD CONSTRAINT` validates existing rows first and PostgreSQL reports the
/// symptom instead of the cause — a defect an independent verifier found in
/// `0009` and that `46e93ca` fixed.
#[test]
fn v2_5_the_pre_check_runs_before_the_constraints_it_protects() {
    let sql = include_str!("../migrations/0010_supplier_returns.sql");
    let pre_check = sql
        .find("contradict the v2.5 coherence rule")
        .expect("0010 must keep its v2.5 coherence pre-check");
    for constraint in [
        "ADD CONSTRAINT stock_movements_document_coherent",
        "ADD CONSTRAINT stock_movements_supplier_return_coherent",
        "ADD CONSTRAINT stock_movements_supplier_return_is_outgoing",
    ] {
        let added = sql
            .find(constraint)
            .unwrap_or_else(|| panic!("0010 must add {constraint}"));
        assert!(
            pre_check < added,
            "the defensive pre-check must run BEFORE {constraint}; placed after \
             it, PostgreSQL reports the violation first and the pre-check can \
             never fire"
        );
    }
}

// ── v2.5 (T2): POST /api/purchases/{id}/supplier-returns ───────────────────
//
// plan-v2.5.md S3/S6/S8/S9. The guard has three levels and each closes a case the
// others do not: a row lock on the purchase, a running total summed inside the
// request loop, and the database's constraints. The repeated-line test below is
// the one that pins the second level — a per-entry check against uncommitted
// state is not a guard, and v1.5 measured exactly that hole in its own mirror.

async fn v2_5_create_purchase(
    client: &common::Client,
    product_id: uuid::Uuid,
    quantity: i32,
    unit_price: &str,
) -> serde_json::Value {
    let response = client
        .post(
            "/api/purchases",
            serde_json::json!({
                "items": [{ "product_id": product_id, "quantity": quantity, "unit_price": unit_price }]
            }),
        )
        .await;
    assert!(
        response.status.is_success(),
        "posting a purchase must succeed, got {}: {}",
        response.status,
        response.text
    );
    response.body.clone()
}

async fn v2_5_product(pool: &sqlx::PgPool, tag: &str) -> uuid::Uuid {
    let row: (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO products (name, list_price) VALUES ($1, 10.00) RETURNING id")
            .bind(format!("v2_5-{tag}-{}", uuid::Uuid::new_v4()))
            .fetch_one(pool)
            .await
            .expect("insert product");
    row.0
}

#[sqlx::test]
async fn v2_5_a_return_takes_stock_out_and_returns_the_money_paid(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    let product = v2_5_product(&pool, "return").await;
    // Buy 5 at 10.00 = 50.00, then return 2 of them.
    let purchase = v2_5_create_purchase(&client, product, 5, "10.00").await;
    let purchase_id = purchase["purchase"]["id"].as_str().expect("purchase id");
    let item_id = purchase["items"][0]["id"].as_str().expect("item id");

    let stock_before = v2_stock(&pool, product).await;

    let resp = client
        .post(
            &format!("/api/purchases/{purchase_id}/supplier-returns"),
            serde_json::json!({ "items": [{ "purchase_item_id": item_id, "quantity": 2 }] }),
        )
        .await;
    assert_eq!(resp.status(), 201, "a valid return is created");
    let body = resp.json().clone();

    // The document values what was PAID, read server-side (S3): the client sent
    // no price, and 2 × 10.00 is 20.00 whatever today's catalogue says.
    assert_eq!(
        body["return"]["total"], "20.00",
        "the return is worth what was paid"
    );
    assert_eq!(body["return"]["purchase_id"], purchase_id);

    let items = body["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["quantity"], 2);
    assert_eq!(
        items[0]["unit_price"], "10.00",
        "the snapshot of the purchase line"
    );
    assert_eq!(items[0]["line_total"], "20.00");
    assert_eq!(
        items[0]["purchase_item_id"], item_id,
        "the line names the purchase line it corrects"
    );

    // The point of the slice: stock went DOWN, by exactly what went back.
    let stock_after = v2_stock(&pool, product).await;
    assert_eq!(
        stock_before - stock_after,
        2,
        "a return of 2 takes 2 out of stock, never adds"
    );

    // And the movement names both documents and is negative.
    // `quantity` is INTEGER, so it decodes as i32 — asking for i64 is a mismatch
    // the driver reports at runtime rather than compile time.
    let movement: (i32, bool, bool) = sqlx::query_as(
        "SELECT quantity, purchase_id IS NOT NULL, supplier_return_id IS NOT NULL
           FROM stock_movements WHERE reason = 'supplier_return' AND purchase_id = $1",
    )
    .bind(uuid::Uuid::parse_str(purchase_id).expect("uuid"))
    .fetch_one(&pool)
    .await
    .expect("supplier_return movement");
    assert_eq!(movement.0, -2, "the movement is negative");
    assert!(
        movement.1 && movement.2,
        "it names both the purchase and the return"
    );
}

#[sqlx::test]
async fn v2_5_the_cumulative_guard_bounds_every_line_in_both_directions(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    let product = v2_5_product(&pool, "boundary").await;
    let purchase = v2_5_create_purchase(&client, product, 10, "1.00").await;
    let purchase_id = purchase["purchase"]["id"].as_str().expect("purchase id");
    let item_id = purchase["items"][0]["id"].as_str().expect("item id");

    async fn post_return(
        client: &common::Client,
        purchase_id: &str,
        item_id: &str,
        qty: i32,
    ) -> u16 {
        client
            .post(
                &format!("/api/purchases/{purchase_id}/supplier-returns"),
                serde_json::json!({ "items": [{ "purchase_item_id": item_id, "quantity": qty }] }),
            )
            .await
            .status()
            .as_u16()
    }

    // Partial returns accumulate: 4 then 6 is exactly the 10 bought.
    assert_eq!(
        post_return(&client, purchase_id, item_id, 4).await,
        201,
        "a partial return is accepted"
    );
    // The boundary itself: 6 more sums to exactly what was sold, so it is allowed.
    assert_eq!(
        post_return(&client, purchase_id, item_id, 6).await,
        201,
        "the boundary is inclusive: 4 + 6 == 10"
    );
    // One unit more would exceed it.
    assert_eq!(
        post_return(&client, purchase_id, item_id, 1).await,
        409,
        "one unit past the boundary is refused"
    );

    // The repeated-line case, which is the second level of the guard. Each entry
    // on its own is within what was bought; together they are not. A per-entry
    // check against uncommitted state passes both and over-returns the line —
    // v1.5 measured exactly this against its own mirror.
    let product2 = v2_5_product(&pool, "repeated").await;
    let p2 = v2_5_create_purchase(&client, product2, 10, "1.00").await;
    let p2_id = p2["purchase"]["id"].as_str().expect("purchase id");
    let p2_item = p2["items"][0]["id"].as_str().expect("item id").to_string();
    let resp = client
        .post(
            &format!("/api/purchases/{p2_id}/supplier-returns"),
            serde_json::json!({
                "items": [
                    { "purchase_item_id": p2_item, "quantity": 6 },
                    { "purchase_item_id": p2_item, "quantity": 6 }
                ]
            }),
        )
        .await;
    assert_eq!(
        resp.status(),
        409,
        "a request naming the same line twice must be judged on the total, not per entry"
    );

    // And nothing was written by any refusal.
    let returned: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(sri.quantity), 0)::bigint
           FROM supplier_return_items sri
           JOIN supplier_returns sr ON sr.id = sri.supplier_return_id
          WHERE sr.purchase_id = $1",
    )
    .bind(uuid::Uuid::parse_str(p2_id).expect("uuid"))
    .fetch_one(&pool)
    .await
    .expect("returned quantity");
    assert_eq!(returned, 0, "the refused request left no line behind");
}

#[sqlx::test]
async fn v2_5_a_rejected_return_writes_nothing(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    let product = v2_5_product(&pool, "reject").await;
    let purchase = v2_5_create_purchase(&client, product, 3, "5.00").await;
    let purchase_id = purchase["purchase"]["id"].as_str().expect("purchase id");
    let item_id = purchase["items"][0]["id"].as_str().expect("item id");

    // Another purchase's line, to prove a line of the wrong document is a 404.
    let other_product = v2_5_product(&pool, "other").await;
    let other = v2_5_create_purchase(&client, other_product, 1, "5.00").await;
    let other_item = other["items"][0]["id"]
        .as_str()
        .expect("other item")
        .to_string();

    async fn counts(pool: &sqlx::PgPool) -> (i64, i64, i64) {
        let r: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM supplier_returns")
            .fetch_one(pool)
            .await
            .expect("returns");
        let i: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM supplier_return_items")
            .fetch_one(pool)
            .await
            .expect("items");
        let m: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM stock_movements WHERE reason = 'supplier_return'",
        )
        .fetch_one(pool)
        .await
        .expect("movements");
        (r, i, m)
    }
    let before = counts(&pool).await;

    let cases: Vec<(&str, serde_json::Value, u16)> = vec![
        (
            "an empty item list",
            serde_json::json!({ "items": [] }),
            400,
        ),
        (
            "a quantity of zero",
            serde_json::json!({ "items": [{ "purchase_item_id": item_id, "quantity": 0 }] }),
            400,
        ),
        (
            "a negative quantity",
            serde_json::json!({ "items": [{ "purchase_item_id": item_id, "quantity": -1 }] }),
            400,
        ),
        (
            "a line that belongs to another purchase",
            serde_json::json!({ "items": [{ "purchase_item_id": other_item, "quantity": 1 }] }),
            404,
        ),
        (
            "a valid entry followed by a foreign line",
            serde_json::json!({ "items": [
                { "purchase_item_id": item_id, "quantity": 1 },
                { "purchase_item_id": other_item, "quantity": 1 }
            ] }),
            404,
        ),
        (
            "more than was bought",
            serde_json::json!({ "items": [{ "purchase_item_id": item_id, "quantity": 4 }] }),
            409,
        ),
    ];

    for (label, payload, expected) in cases {
        let resp = client
            .post(
                &format!("/api/purchases/{purchase_id}/supplier-returns"),
                payload,
            )
            .await;
        assert_eq!(resp.status(), expected, "{label} owes {expected}");
    }

    // An unknown purchase is a 404, not a 500 and not an empty document.
    let unknown = uuid::Uuid::new_v4();
    let resp = client
        .post(
            &format!("/api/purchases/{unknown}/supplier-returns"),
            serde_json::json!({ "items": [{ "purchase_item_id": item_id, "quantity": 1 }] }),
        )
        .await;
    assert_eq!(resp.status(), 404, "an unknown purchase is not found");

    assert_eq!(
        before,
        counts(&pool).await,
        "a rejected return wrote something: document, line or movement survived"
    );
}

// ── v2.5 (T3): GET /api/purchases carries its returns ──────────────────────
//
// plan-v2.5.md S7. A purchase and its returns are read together, the way the sales
// ledger carries its credit notes: the corrected document is never rewritten, so
// the only place a correction is visible is alongside the thing it corrects.

#[sqlx::test]
async fn v2_5_the_purchase_list_carries_its_returns(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    let product = v2_5_product(&pool, "nested").await;
    let purchase = v2_5_create_purchase(&client, product, 4, "2.50").await;
    let purchase_id = purchase["purchase"]["id"].as_str().expect("purchase id");
    let item_id = purchase["items"][0]["id"].as_str().expect("item id");

    // A purchase with no returns must carry an EMPTY ARRAY, never an absent field:
    // a client that has to tell "no returns" from "field missing" is a client with
    // two code paths for one state, which is what the credit-note shape already
    // avoids.
    let before = client.get("/api/purchases?limit=200").await;
    before.expect_status(axum::http::StatusCode::OK);
    let mine = before
        .body
        .as_array()
        .expect("array")
        .iter()
        .find(|p| p["purchase"]["id"] == purchase_id)
        .expect("the purchase is listed");
    assert_eq!(
        mine["supplier_returns"],
        serde_json::json!([]),
        "a purchase with no returns carries an empty array, not a missing field"
    );

    // Record two partial returns, so ordering is observable.
    for qty in [1, 2] {
        let resp = client
            .post(
                &format!("/api/purchases/{purchase_id}/supplier-returns"),
                serde_json::json!({ "items": [{ "purchase_item_id": item_id, "quantity": qty }] }),
            )
            .await;
        assert_eq!(resp.status(), 201);
    }

    let after = client.get("/api/purchases?limit=200").await;
    after.expect_status(axum::http::StatusCode::OK);
    let mine = after
        .body
        .as_array()
        .expect("array")
        .iter()
        .find(|p| p["purchase"]["id"] == purchase_id)
        .expect("the purchase is still listed");

    let returns = mine["supplier_returns"]
        .as_array()
        .expect("returns array present");
    assert_eq!(returns.len(), 2, "both returns travel with the purchase");

    // Oldest first: the ledger reads top-down, so a correction appears in the
    // order it happened, not the order a query happened to return it.
    assert_eq!(
        returns[0]["return"]["total"], "2.50",
        "the 1-unit return came first"
    );
    assert_eq!(
        returns[1]["return"]["total"], "5.00",
        "the 2-unit return came second"
    );

    // Each return carries its own lines, nested — a reader expands the purchase,
    // then the return, without a request per document.
    let first_items = returns[0]["items"].as_array().expect("return items");
    assert_eq!(first_items.len(), 1);
    assert_eq!(first_items[0]["quantity"], 1);
    assert_eq!(
        first_items[0]["unit_price"], "2.50",
        "the snapshot of the purchase line"
    );
    assert_eq!(first_items[0]["purchase_item_id"], item_id);

    // And the purchase itself is unchanged: its total is what was bought, not
    // what is left after returns.
    assert_eq!(
        mine["purchase"]["total"], "10.00",
        "the purchase keeps the total it was recorded with"
    );
}

// ── v2.5 (T4): GET /api/cash adds returns back ─────────────────────────────
//
// plan-v2.5.md S5: cash = sales − credit_notes − purchases + supplier_returns.
//
// The response shape does not change — one two-decimal string — but this is the
// second slice to change what the number means. The worked example from the plan
// is the test: sell 100.00, buy 40.00, return 15.00, and the till reads 75.00.
//
// The sale and purchase cases are asserted in the same test on purpose. A sign
// error in any of the four terms produces a plausible number, so each direction
// has to move the figure by exactly its own amount for the test to pass.

#[sqlx::test]
async fn v2_5_cash_adds_returns_back_and_still_counts_sales_and_purchases(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    let product = v2_5_product(&pool, "cash").await;

    async fn cash(client: &common::Client) -> String {
        // `T6` retired `/api/cash`; the till's balance is the default account's
        // derived balance, and it is the same number by the same view.
        let response = client.get("/api/accounts").await;
        response.expect_status(axum::http::StatusCode::OK);
        response
            .json()
            .as_array()
            .expect("accounts array")
            .iter()
            .find(|a| a["is_default"].as_bool().unwrap_or(false))
            .expect("a default account")["balance"]
            .as_str()
            .expect("balance string")
            .to_string()
    }
    fn cents(s: &str) -> i64 {
        // Test-only parsing of a wire amount, exact: never through a float.
        let (sign, rest) = match s.strip_prefix('-') {
            Some(r) => (-1i64, r),
            None => (1i64, s),
        };
        let (whole, frac) = rest.split_once('.').expect("two decimals");
        sign * (whole.parse::<i64>().expect("whole") * 100 + frac.parse::<i64>().expect("fraction"))
    }

    let before = cents(&cash(&client).await);

    // A purchase of 4 × 10.00 = 40.00 takes money out.
    let purchase = v2_5_create_purchase(&client, product, 4, "10.00").await;
    let purchase_id = purchase["purchase"]["id"].as_str().expect("purchase id");
    let item_id = purchase["items"][0]["id"].as_str().expect("item id");
    let after_purchase = cents(&cash(&client).await);
    assert_eq!(
        before - after_purchase,
        4000,
        "a purchase lowers the till by its total"
    );

    // A return of 1 × 10.00 must put 10.00 BACK. This is the assertion the slice
    // exists for, and the one that fails if the term is subtracted instead of
    // added.
    let resp = client
        .post(
            &format!("/api/purchases/{purchase_id}/supplier-returns"),
            serde_json::json!({ "items": [{ "purchase_item_id": item_id, "quantity": 1 }] }),
        )
        .await;
    assert_eq!(resp.status(), 201);
    let after_return = cents(&cash(&client).await);
    assert_eq!(
        after_return - after_purchase,
        1000,
        "a return raises the till by what the supplier gave back"
    );

    // And the sale side still moves the other way, by its own total: the
    // existing-behaviour test this slice owes, because the query changed.
    let resp = client
        .post(
            "/api/sales",
            serde_json::json!({
                "items": [{ "product_id": product, "quantity": 2, "unit_price": "25.00" }]
            }),
        )
        .await;
    assert_eq!(resp.status(), 201);
    let after_sale = cents(&cash(&client).await);
    assert_eq!(
        after_sale - after_return,
        5000,
        "a sale still raises the till by its total"
    );

    // The shape is unchanged: one two-decimal string.
    let raw = cash(&client).await;
    assert!(
        raw.split('.').nth(1).map(|f| f.len()) == Some(2),
        "cash is still a two-decimal string, got {raw:?}"
    );
}

// ── Ledger integrity (0011): the rules the constraints never encoded ────────
//
// The six constraints 0009/0010 left behind enumerate the vocabulary and validate
// the DOCUMENTS a movement names. Two things they never check, both measured on
// the live schema before this test was written:
//
//   1. that the movement's product actually belongs to the document it names.
//      A `supplier_return` movement for a product that is not in the purchase is
//      ACCEPTED by the database today. So is a `credit_note` movement for a
//      product that is not in the sale.
//   2. the vocabulary is enumerated in six places, so every migration that adds a
//      reason has to touch three or four of them. 0007, 0009 and 0010 all did.
//
// The fix is one trigger that derives the rule from the row's own shape instead of
// naming the reasons. These tests are the evidence that the replacement refuses
// everything the constraints refused — which is the only way replacing them is
// safe.
//
// `fixture` helpers insert directly rather than through the API, because the
// point is to test what the DATABASE accepts, not what a handler happens to send.

/// A product, a sale with one line, and a purchase with one line — plus a second
/// product belonging to neither. Everything a movement test needs.
#[allow(dead_code)]
struct LedgerFixture {
    in_sale: uuid::Uuid,
    in_purchase: uuid::Uuid,
    stranger: uuid::Uuid,
    sale_id: uuid::Uuid,
    sale_item_id: uuid::Uuid,
    purchase_id: uuid::Uuid,
    purchase_item_id: uuid::Uuid,
}

#[allow(dead_code)]
async fn ledger_fixture(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tag: &str,
) -> LedgerFixture {
    // Inline rather than a closure: an `async move` closure would capture `tx`
    // and every later use of it in this function would be a borrow after move.
    let stamp = uuid::Uuid::new_v4();
    let mut product_ids = Vec::with_capacity(3);
    for slot in ["sale", "purch", "stranger"] {
        let row: (uuid::Uuid,) = sqlx::query_as(
            "INSERT INTO products (name, list_price) VALUES ($1, 10.00) RETURNING id",
        )
        .bind(format!("li-{tag}-{slot}-{stamp}"))
        .fetch_one(&mut **tx)
        .await
        .expect("insert product");
        product_ids.push(row.0);
    }
    let in_sale = product_ids[0];
    let in_purchase = product_ids[1];
    let stranger = product_ids[2];

    let sale_id: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO sales (customer_id, total, payment_method_id) VALUES ('00000000-0000-0000-0000-000000000001', 10.00, '00000000-0000-0000-0000-000000000004') RETURNING id",
    )
    .fetch_one(&mut **tx)
    .await
    .expect("insert sale");
    let sale_item_id: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO sale_items (sale_id, product_id, product_name, quantity, unit_price, line_total, unit)
         VALUES ($1, $2, 'li sale line', 1, 10.00, 10.00, 'unit') RETURNING id",
    )
    .bind(sale_id.0)
    .bind(in_sale)
    .fetch_one(&mut **tx)
    .await
    .expect("insert sale item");

    let purchase_id: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO purchases (supplier_id, total, payment_method_id) VALUES ('00000000-0000-0000-0000-000000000002', 10.00, '00000000-0000-0000-0000-000000000004') RETURNING id",
    )
    .fetch_one(&mut **tx)
    .await
    .expect("insert purchase");
    let purchase_item_id: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO purchase_items (purchase_id, product_id, product_name, quantity, unit_price, line_total, unit)
         VALUES ($1, $2, 'li purchase line', 1, 10.00, 10.00, 'unit') RETURNING id",
    )
    .bind(purchase_id.0)
    .bind(in_purchase)
    .fetch_one(&mut **tx)
    .await
    .expect("insert purchase item");

    LedgerFixture {
        in_sale,
        in_purchase,
        stranger,
        sale_id: sale_id.0,
        sale_item_id: sale_item_id.0,
        purchase_id: purchase_id.0,
        purchase_item_id: purchase_item_id.0,
    }
}

#[sqlx::test]
async fn ledger_refuses_a_movement_whose_product_is_not_in_its_document(pool: sqlx::PgPool) {
    let mut tx = pool.begin().await.expect("begin");
    let f = ledger_fixture(&mut tx, "belongs").await;

    let cn: (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO credit_notes (sale_id, total, payment_method_id) VALUES ($1, 10.00, '00000000-0000-0000-0000-000000000004') RETURNING id")
            .bind(f.sale_id)
            .fetch_one(&mut *tx)
            .await
            .expect("insert credit note");
    let ret: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO supplier_returns (purchase_id, total, payment_method_id) VALUES ($1, 10.00, '00000000-0000-0000-0000-000000000004') RETURNING id",
    )
    .bind(f.purchase_id)
    .fetch_one(&mut *tx)
    .await
    .expect("insert supplier return");

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

    // The positive cases FIRST. Without them these refusals would be satisfied by
    // a rule that refuses everything, which proves nothing — the lesson v2 learned
    // when a guard test passed before the migration existed.
    sqlx::query(
        "INSERT INTO stock_movements (product_id, quantity, reason, sale_id)
         VALUES ($1, -1, 'sale', $2)",
    )
    .bind(f.in_sale)
    .bind(f.sale_id)
    .execute(&mut *tx)
    .await
    .expect("a sale movement for a product in the sale must be accepted");
    sqlx::query(
        "INSERT INTO stock_movements (product_id, quantity, reason, sale_id, credit_note_id)
         VALUES ($1, 1, 'credit_note', $2, $3)",
    )
    .bind(f.in_sale)
    .bind(f.sale_id)
    .bind(cn.0)
    .execute(&mut *tx)
    .await
    .expect("a credit-note movement for a product in the sale must be accepted");
    sqlx::query(
        "INSERT INTO stock_movements (product_id, quantity, reason, purchase_id)
         VALUES ($1, 1, 'purchase', $2)",
    )
    .bind(f.in_purchase)
    .bind(f.purchase_id)
    .execute(&mut *tx)
    .await
    .expect("a purchase movement for a product in the purchase must be accepted");
    sqlx::query(
        "INSERT INTO stock_movements (product_id, quantity, reason, purchase_id, supplier_return_id)
         VALUES ($1, -1, 'supplier_return', $2, $3)",
    )
    .bind(f.in_purchase)
    .bind(f.purchase_id)
    .bind(ret.0)
    .execute(&mut *tx)
    .await
    .expect("a return movement for a product in the purchase must be accepted");

    // NOW the hole. Both of these are ACCEPTED by the schema as it stands, which is
    // the defect this migration exists to close.
    probe_denied!(
        "a sale movement for a product that is not in the sale",
        "INSERT INTO stock_movements (product_id, quantity, reason, sale_id)
         VALUES ($1, -1, 'sale', $2)",
        f.stranger,
        f.sale_id
    );
    probe_denied!(
        "a credit-note movement for a product that is not in the sale",
        "INSERT INTO stock_movements (product_id, quantity, reason, sale_id, credit_note_id)
         VALUES ($1, 1, 'credit_note', $2, $3)",
        f.stranger,
        f.sale_id,
        cn.0
    );
    probe_denied!(
        "a purchase movement for a product that is not in the purchase",
        "INSERT INTO stock_movements (product_id, quantity, reason, purchase_id)
         VALUES ($1, 1, 'purchase', $2)",
        f.stranger,
        f.purchase_id
    );
    probe_denied!(
        "a supplier-return movement for a product that is not in the purchase",
        "INSERT INTO stock_movements (product_id, quantity, reason, purchase_id, supplier_return_id)
         VALUES ($1, -1, 'supplier_return', $2, $3)",
        f.stranger,
        f.purchase_id,
        ret.0
    );

    tx.rollback().await.expect("rollback");
}

/// The replacement must refuse everything the six enumerated constraints refused.
/// This is the test that makes replacing them safe: if the trigger misses a rule,
/// one of these probes succeeds.
#[sqlx::test]
async fn ledger_still_refuses_everything_the_old_constraints_refused(pool: sqlx::PgPool) {
    let mut tx = pool.begin().await.expect("begin");
    let f = ledger_fixture(&mut tx, "equivalence").await;

    let cn: (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO credit_notes (sale_id, total, payment_method_id) VALUES ($1, 10.00, '00000000-0000-0000-0000-000000000004') RETURNING id")
            .bind(f.sale_id)
            .fetch_one(&mut *tx)
            .await
            .expect("insert credit note");
    let ret: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO supplier_returns (purchase_id, total, payment_method_id) VALUES ($1, 10.00, '00000000-0000-0000-0000-000000000004') RETURNING id",
    )
    .bind(f.purchase_id)
    .fetch_one(&mut *tx)
    .await
    .expect("insert supplier return");

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

    // `reason_known`: an unknown reason.
    probe_denied!(
        "an unknown reason",
        "INSERT INTO stock_movements (product_id, quantity, reason) VALUES ($1, 1, 'inventado')",
        f.in_sale
    );

    // `document_coherent`: each document reason without its document.
    probe_denied!(
        "a sale with no sale_id",
        "INSERT INTO stock_movements (product_id, quantity, reason) VALUES ($1, -1, 'sale')",
        f.in_sale
    );
    probe_denied!(
        "a credit_note with no sale_id",
        "INSERT INTO stock_movements (product_id, quantity, reason, credit_note_id) VALUES ($1, 1, 'credit_note', $2)",
        f.in_sale,
        cn.0
    );
    probe_denied!(
        "a purchase with no purchase_id",
        "INSERT INTO stock_movements (product_id, quantity, reason) VALUES ($1, 1, 'purchase')",
        f.in_purchase
    );
    probe_denied!(
        "a supplier_return with no purchase_id",
        "INSERT INTO stock_movements (product_id, quantity, reason, supplier_return_id) VALUES ($1, -1, 'supplier_return', $2)",
        f.in_purchase,
        ret.0
    );
    // `initial` must carry no document at all — every combination.
    probe_denied!(
        "an initial carrying a sale_id",
        "INSERT INTO stock_movements (product_id, quantity, reason, sale_id) VALUES ($1, 1, 'initial', $2)",
        f.in_sale,
        f.sale_id
    );
    probe_denied!(
        "an initial carrying a purchase_id",
        "INSERT INTO stock_movements (product_id, quantity, reason, purchase_id) VALUES ($1, 1, 'initial', $2)",
        f.in_purchase,
        f.purchase_id
    );

    // `credit_note_coherent`: the equality, so a sale row may not carry a note.
    probe_denied!(
        "a sale carrying a credit_note_id",
        "INSERT INTO stock_movements (product_id, quantity, reason, sale_id, credit_note_id)
         VALUES ($1, -1, 'sale', $2, $3)",
        f.in_sale,
        f.sale_id,
        cn.0
    );
    // `purchase_coherent`: and a sale may not carry a purchase either.
    probe_denied!(
        "a sale carrying a purchase_id",
        "INSERT INTO stock_movements (product_id, quantity, reason, sale_id, purchase_id)
         VALUES ($1, -1, 'sale', $2, $3)",
        f.in_sale,
        f.sale_id,
        f.purchase_id
    );
    // `supplier_return_coherent`: a sale may not carry a return.
    probe_denied!(
        "a sale carrying a supplier_return_id",
        "INSERT INTO stock_movements (product_id, quantity, reason, sale_id, supplier_return_id)
         VALUES ($1, -1, 'sale', $2, $3)",
        f.in_sale,
        f.sale_id,
        ret.0
    );

    // The two direction rules.
    probe_denied!(
        "a positive purchase movement",
        "INSERT INTO stock_movements (product_id, quantity, reason, purchase_id)
         VALUES ($1, -1, 'purchase', $2)",
        f.in_purchase,
        f.purchase_id
    );
    probe_denied!(
        "a negative supplier_return movement",
        "INSERT INTO stock_movements (product_id, quantity, reason, purchase_id, supplier_return_id)
         VALUES ($1, 1, 'supplier_return', $2, $3)",
        f.in_purchase,
        f.purchase_id,
        ret.0
    );

    // And the pre-existing quantity rule survives the replacement.
    probe_denied!(
        "a zero quantity",
        "INSERT INTO stock_movements (product_id, quantity, reason) VALUES ($1, 0, 'initial')",
        f.in_sale
    );

    tx.rollback().await.expect("rollback");
}

/// A credit note must correct the sale it is attached to; a return must correct
/// the purchase. The FK proves the document exists, never that the pair agrees.
#[sqlx::test]
async fn ledger_refuses_a_document_from_another_transaction(pool: sqlx::PgPool) {
    let mut tx = pool.begin().await.expect("begin");
    let a = ledger_fixture(&mut tx, "pairA").await;
    let b = ledger_fixture(&mut tx, "pairB").await;

    // A credit note on sale B, used on a movement naming sale A.
    let cn_on_b: (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO credit_notes (sale_id, total, payment_method_id) VALUES ($1, 10.00, '00000000-0000-0000-0000-000000000004') RETURNING id")
            .bind(b.sale_id)
            .fetch_one(&mut *tx)
            .await
            .expect("insert credit note");
    let ret_on_b: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO supplier_returns (purchase_id, total, payment_method_id) VALUES ($1, 10.00, '00000000-0000-0000-0000-000000000004') RETURNING id",
    )
    .bind(b.purchase_id)
    .fetch_one(&mut *tx)
    .await
    .expect("insert supplier return");

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
        "a credit note attached to a different sale",
        "INSERT INTO stock_movements (product_id, quantity, reason, sale_id, credit_note_id)
         VALUES ($1, 1, 'credit_note', $2, $3)",
        a.in_sale,
        a.sale_id,
        cn_on_b.0
    );
    probe_denied!(
        "a return attached to a different purchase",
        "INSERT INTO stock_movements (product_id, quantity, reason, purchase_id, supplier_return_id)
         VALUES ($1, -1, 'supplier_return', $2, $3)",
        a.in_purchase,
        a.purchase_id,
        ret_on_b.0
    );

    tx.rollback().await.expect("rollback");
}

// ── v3: accounts, payment methods and the four destinations (0012) ──────────
//
// The migration's own claims, measured. Every assertion here is about something
// `0012` created: the seeds, the backfill, the four `NOT NULL`s, and the four
// guards that make "where the money moved" an invariant instead of a convention.
//
// The guard tests assert the **positive** case first, on purpose. `v2`'s T1
// recorded why: a guard test that only checks refusals can pass before the feature
// exists, because the refusal comes from a missing table or a narrower vocabulary
// rather than from the rule under test.

#[sqlx::test]
async fn v3_seeds_cash_and_an_inactive_credit_account_and_method(pool: sqlx::PgPool) {
    let accounts: Vec<(uuid::Uuid, String, bool)> =
        sqlx::query_as("SELECT id, name, is_default FROM accounts ORDER BY name")
            .fetch_all(&pool)
            .await
            .expect("read accounts");

    assert_eq!(accounts.len(), 2, "exactly the two seeded accounts");
    let (cash_id, cash_name, cash_default) = &accounts[0];
    let (credit_id, credit_name, credit_default) = &accounts[1];
    assert_eq!(cash_name, "cash");
    assert!(cash_default, "the till is the default account");
    assert_eq!(credit_name, "credit");
    assert!(!credit_default, "the debt account is not the till");

    // Fixed ids, the mechanism walk-in and one-off-supplier established.
    assert_eq!(*cash_id, uuid::Uuid::from_u128(3));
    assert_eq!(*credit_id, uuid::Uuid::from_u128(5));

    let methods: Vec<(String, bool, uuid::Uuid)> =
        sqlx::query_as("SELECT name, is_active, account_id FROM payment_methods ORDER BY name")
            .fetch_all(&pool)
            .await
            .expect("read methods");
    assert_eq!(methods.len(), 2);
    assert_eq!(methods[0].0, "cash");
    assert!(methods[0].1, "cash is offered at the counter");
    assert_eq!(methods[0].2, *cash_id);
    assert_eq!(methods[1].0, "credit");
    assert!(
        !methods[1].1,
        "credit is seeded INACTIVE: v3 has no way to collect a credit sale, so the \
         picker must not offer a debt nothing can settle"
    );
    assert_eq!(methods[1].2, *credit_id);
}

#[sqlx::test]
async fn v3_every_existing_document_names_where_its_money_moved(pool: sqlx::PgPool) {
    // The backfill could not invent a destination, so it records the only one that
    // existed. `0002`'s DEFAULT 'cash' is the evidence that every existing row means
    // "the till". What is asserted here is the outcome: no document is stranded.
    let cash_method = uuid::Uuid::from_u128(4);
    // Literal statements rather than a formatted loop: sqlx 0.9 requires
    // `&'static str` for queries, and building the table name at runtime defeats the
    // compile-time checking this crate relies on everywhere else anyway.
    const STRANDED: [(&str, &str); 4] = [
        (
            "sales",
            "SELECT count(*) FROM sales WHERE payment_method_id IS DISTINCT FROM $1",
        ),
        (
            "credit_notes",
            "SELECT count(*) FROM credit_notes WHERE payment_method_id IS DISTINCT FROM $1",
        ),
        (
            "purchases",
            "SELECT count(*) FROM purchases WHERE payment_method_id IS DISTINCT FROM $1",
        ),
        (
            "supplier_returns",
            "SELECT count(*) FROM supplier_returns WHERE payment_method_id IS DISTINCT FROM $1",
        ),
    ];
    for (table, statement) in STRANDED {
        let stranded: i64 = sqlx::query_scalar(statement)
            .bind(cash_method)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|e| panic!("count {table}: {e}"));
        assert_eq!(
            stranded, 0,
            "{table} rows must be backfilled to the cash method"
        );
    }

    // And the old TEXT column is gone, with the guard that froze it re-cut to the
    // foreign key. Dropping the column without re-cutting the function would leave a
    // plpgsql body that resolves only at the next UPDATE.
    let old_column: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
          WHERE table_name = 'sales' AND column_name = 'payment_method'",
    )
    .fetch_one(&pool)
    .await
    .expect("read columns");
    assert_eq!(old_column, 0, "sales.payment_method must be dropped");
}

#[sqlx::test]
async fn v3_the_economics_guard_follows_the_destination_to_its_new_column(pool: sqlx::PgPool) {
    // The regression this closes: a recorded sale's destination must not be
    // rewritable. Before the re-cut, the guard compared a column that no longer
    // exists; after it, the comparison follows the foreign key.
    let sale_id = insert_paid_sale(
        &pool,
        uuid::Uuid::from_u128(1),
        "10.00",
        uuid::Uuid::from_u128(4),
    )
    .await;

    let moved = sqlx::query("UPDATE sales SET payment_method_id = $1 WHERE id = $2")
        .bind(uuid::Uuid::from_u128(6))
        .bind(sale_id)
        .execute(&pool)
        .await;
    assert!(
        moved.is_err(),
        "a sale's payment method must be immutable: moving it rewrites which account \
         the money reached"
    );

    // The status transition this test used to permit is gone with the column
    // (`0015`): there is no column left whose write the guard allows, so the
    // permitted case is now the absence of one.
    let status_column: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
          WHERE table_name = 'sales' AND column_name IN ('status', 'reversed_at')",
    )
    .fetch_one(&pool)
    .await
    .expect("read columns");
    assert_eq!(
        status_column, 0,
        "the lifecycle columns are dropped, so the guard has nothing to permit"
    );
}

#[sqlx::test]
async fn v3_a_referenced_method_cannot_move_between_accounts(pool: sqlx::PgPool) {
    // Positive case first: an UNREFERENCED method is a typo and must be correctable.
    let (free_id,): (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO payment_methods (account_id, name)
         VALUES ('00000000-0000-0000-0000-000000000003', 'transfer') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert an unreferenced method");

    // The destination is a fresh account. It used to be the seeded `credit` account,
    // which `S2` of `odd/tasks/v3-seed-identities.md` now forbids: nothing may be
    // attached to the debt account. What this case covers — an unreferenced method is
    // a typo and may be corrected — needs some real destination and keeps it.
    let (destination,): (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO accounts (name) VALUES ('destino') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("insert a destination account");

    sqlx::query("UPDATE payment_methods SET account_id = $1 WHERE id = $2")
        .bind(destination)
        .bind(free_id)
        .execute(&pool)
        .await
        .expect("an unreferenced method may move: it is a typo, not history");

    // Now the refusal. The cash method is named by the seeded history of this test
    // database only after a document exists, so create one.
    insert_paid_sale(
        &pool,
        uuid::Uuid::from_u128(1),
        "5.00",
        uuid::Uuid::from_u128(4),
    )
    .await;

    // **A second active method on the same account, deliberately.** Without it, the
    // move below would also be refused by the *default-account* invariant (the till
    // would be left with no active method), and this test would pass while the guard
    // under test was absent — measured by mutating `guard_payment_method_account`
    // into a no-op and watching the test stay green. With the second method present
    // the default-account rule is satisfied, so the only thing left to refuse the
    // move is the rule this test is about.
    sqlx::query(
        "INSERT INTO payment_methods (account_id, name)
         VALUES ('00000000-0000-0000-0000-000000000003', 'card')",
    )
    .execute(&pool)
    .await
    .expect("give the default account another active method");

    let moved = sqlx::query("UPDATE payment_methods SET account_id = $1 WHERE id = $2")
        .bind(uuid::Uuid::from_u128(5))
        .bind(uuid::Uuid::from_u128(4))
        .execute(&pool)
        .await;
    assert!(
        moved.is_err(),
        "a referenced method must not move accounts: yesterday's balance would change today"
    );
}

#[sqlx::test]
async fn v3_a_rename_corrects_history_instead_of_freezing_a_typo(pool: sqlx::PgPool) {
    // The whole reason the name is a join and not a snapshot. Renaming is one UPDATE
    // of one row, and the document's rendered name follows it.
    insert_paid_sale(
        &pool,
        uuid::Uuid::from_u128(1),
        "7.00",
        uuid::Uuid::from_u128(4),
    )
    .await;

    sqlx::query("UPDATE payment_methods SET name = 'MervadoPago' WHERE id = $1")
        .bind(uuid::Uuid::from_u128(4))
        .execute(&pool)
        .await
        .expect("a rename is allowed: the id is the identity, the name is a label");

    let (rendered,): (String,) = sqlx::query_as(
        "SELECT pm.name FROM sales s JOIN payment_methods pm ON pm.id = s.payment_method_id
          WHERE s.customer_id = '00000000-0000-0000-0000-000000000001'",
    )
    .fetch_one(&pool)
    .await
    .expect("read the joined name");
    assert_eq!(
        rendered, "MervadoPago",
        "the typo is corrected everywhere, because the document stores the id"
    );

    // And the other half of the trade, stated as a test: the balance did not move.
    let (cash_total, credit_total): (rust_decimal::Decimal, rust_decimal::Decimal) =
        sqlx::query_as(
            "SELECT (SELECT COALESCE(SUM(total), 0) FROM sales s
                  JOIN payment_methods pm ON pm.id = s.payment_method_id
                  JOIN accounts a ON a.id = pm.account_id WHERE a.name = 'cash'),
                (SELECT COALESCE(SUM(total), 0) FROM sales s
                  JOIN payment_methods pm ON pm.id = s.payment_method_id
                  JOIN accounts a ON a.id = pm.account_id WHERE a.name = 'credit')",
        )
        .fetch_one(&pool)
        .await
        .expect("read both balances");
    assert_eq!(cash_total, rust_decimal::Decimal::from(7));
    assert_eq!(credit_total, rust_decimal::Decimal::ZERO);
}

#[sqlx::test]
async fn v3_the_default_account_always_keeps_an_active_method(pool: sqlx::PgPool) {
    // The owner's invariant, and it is the interesting one because no CHECK can
    // express a statement about other rows. Three paths are measured.
    let cash_method = uuid::Uuid::from_u128(4);
    let cash_account = uuid::Uuid::from_u128(3);
    // No `credit_account`: the till-move case below uses a fresh account, because the
    // seeded debt account can never be the default (`S3`,
    // `odd/tasks/v3-seed-identities.md`). A binding left here would be an unused one.

    // Positive case: a second active method on the default account is fine, and so is
    // deactivating one of two.
    let (second,): (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO payment_methods (account_id, name) VALUES ($1, 'card') RETURNING id",
    )
    .bind(cash_account)
    .fetch_one(&pool)
    .await
    .expect("insert a second active method");
    sqlx::query("UPDATE payment_methods SET is_active = false WHERE id = $1")
        .bind(cash_method)
        .execute(&pool)
        .await
        .expect("deactivating one of two active methods is allowed");

    // Restore, then measure the three refusals.
    sqlx::query("UPDATE payment_methods SET is_active = true WHERE id = $1")
        .bind(cash_method)
        .execute(&pool)
        .await
        .expect("reactivate");

    // (1) the last active method of the default account may not be deactivated
    sqlx::query("UPDATE payment_methods SET is_active = false WHERE id = $1")
        .bind(second)
        .execute(&pool)
        .await
        .expect("deactivate the second");
    let refusal = sqlx::query("UPDATE payment_methods SET is_active = false WHERE id = $1")
        .bind(cash_method)
        .execute(&pool)
        .await;
    assert!(
        refusal.is_err(),
        "the default account would be left with no active payment method"
    );

    // (2) an account with no active method may not become the default.
    //
    // **Asserted by the failure's MESSAGE, not merely by its existence.** Two
    // guards can refuse this statement — the trigger under test and the partial
    // unique index, since the till is already the default — so a bare `is_err()`
    // passes when either one is present. Measured: mutating this trigger into a
    // no-op left the test green, because the unique index answered instead. Pinning
    // the message is what makes the assertion belong to one guard.
    let (bare,): (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO accounts (name) VALUES ('bare') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("insert an account with no methods");
    let refusal = sqlx::query("UPDATE accounts SET is_default = true WHERE id = $1")
        .bind(bare)
        .execute(&pool)
        .await
        .expect_err("an account with no active method cannot be the till");
    let message = refusal
        .as_database_error()
        .map(|e| e.message().to_string())
        .unwrap_or_default();
    assert!(
        message.contains("no active payment method"),
        "the refusal must come from the default-account rule, got: {message}"
    );

    // (3) the shop may never END a transaction with no till at all — but it MAY
    //     move the default, which is what the deferred constraint exists for.
    //
    // **This case was originally written the other way round, and the test encoded
    // the bug.** It asserted that unsetting the default was refused, which is only
    // half of a rule; combined with the partial unique index (which refuses a second
    // true row while the first is still true) it made the default permanently fixed
    // to the seeded account, with no ordering able to move it. Native review found
    // it (`R3-001`, critical) and both failure modes were reproduced by hand.
    //
    // So the assertion is now about the *state at COMMIT*, which is what the rule
    // really says: two statements in one transaction always work, and ending with
    // zero defaults never does.
    let mut tx = pool.begin().await.expect("begin");
    sqlx::query("UPDATE accounts SET is_default = false WHERE id = $1")
        .bind(cash_account)
        .execute(&mut *tx)
        .await
        .expect("unsetting the default inside a transaction is allowed");
    let refusal = tx.commit().await;
    assert!(
        refusal.is_err(),
        "ending a transaction with no default must be refused at COMMIT"
    );

    // And the legal move: unset one and set another in the same transaction, which
    // is the operation the shop needs if the till ever changes hands.
    //
    // The candidate is a **fresh account**, not the seeded `credit` one. It used to be
    // `credit`, and `S3` of `odd/tasks/v3-seed-identities.md` now forbids the debt
    // account from ever being the till. The case this paragraph covers — moving the
    // default is one transaction ending with exactly one `true` — is unchanged, and it
    // needs a candidate that is allowed to hold the flag.
    //
    // It needs an ACTIVE method first, which is the other half of the rule: a
    // candidate with no active method would be refused for that reason instead.
    let (candidate,): (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO accounts (name) VALUES ('caja-nueva') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("insert the candidate account");
    sqlx::query("INSERT INTO payment_methods (account_id, name) VALUES ($1, 'efectivo-nuevo')")
        .bind(candidate)
        .execute(&pool)
        .await
        .expect("the new till needs an active payment method");

    let mut tx = pool.begin().await.expect("begin");
    sqlx::query("UPDATE accounts SET is_default = false WHERE id = $1")
        .bind(cash_account)
        .execute(&mut *tx)
        .await
        .expect("unset the old default");
    sqlx::query("UPDATE accounts SET is_default = true WHERE id = $1")
        .bind(candidate)
        .execute(&mut *tx)
        .await
        .expect("set the new default");
    tx.commit()
        .await
        .expect("a transaction that ends with exactly one default must commit");
    let (now_default,): (uuid::Uuid,) = sqlx::query_as("SELECT id FROM accounts WHERE is_default")
        .fetch_one(&pool)
        .await
        .expect("read the default");
    assert_eq!(now_default, candidate, "the till moved, as allowed");

    // Put it back so the rest of the assertions below read the seeded state.
    let mut tx = pool.begin().await.expect("begin");
    sqlx::query("UPDATE accounts SET is_default = false WHERE id = $1")
        .bind(candidate)
        .execute(&mut *tx)
        .await
        .expect("unset");
    sqlx::query("UPDATE accounts SET is_default = true WHERE id = $1")
        .bind(cash_account)
        .execute(&mut *tx)
        .await
        .expect("restore");
    tx.commit().await.expect("restore commits");

    // (4) a second default is refused by the partial unique index.
    //
    // The candidate here HAS an active method on purpose: with an account that had
    // none, the trigger of (2) would refuse it first and this assertion would pass
    // while the index was absent — the same masking, in the other direction. Its
    // message is the index's own, which is what identifies the cause. For the same
    // reason the candidate is not the seeded `credit` account: its own guard would
    // answer first and the index would go unmeasured.
    sqlx::query("INSERT INTO payment_methods (account_id, name) VALUES ($1, 'credit-card')")
        .bind(candidate)
        .execute(&pool)
        .await
        .expect("give the candidate a second active method");
    let refusal = sqlx::query("UPDATE accounts SET is_default = true WHERE id = $1")
        .bind(candidate)
        .execute(&pool)
        .await
        .expect_err("at most one account may be the default");
    let message = refusal
        .as_database_error()
        .map(|e| e.message().to_string())
        .unwrap_or_default();
    assert!(
        message.contains("accounts_one_default_idx"),
        "the second default must be refused by the partial unique index, got: {message}"
    );
}

#[sqlx::test]
async fn v3_accounts_and_methods_are_never_deleted(pool: sqlx::PgPool) {
    // Deletion would erase the explanation of a balance. `is_active` is the only
    // retirement either table needs, and TRUNCATE needs its own statement-level
    // guard because it does not fire FOR EACH ROW — the mistake `0007` shipped and
    // `0008` repaired.
    for (description, statement) in [
        (
            "delete a method",
            "DELETE FROM payment_methods WHERE id = '00000000-0000-0000-0000-000000000006'",
        ),
        (
            "delete an account",
            "DELETE FROM accounts WHERE id = '00000000-0000-0000-0000-000000000005'",
        ),
    ] {
        let result = sqlx::query(statement).execute(&pool).await;
        assert!(result.is_err(), "{description} must be refused");
    }

    // TRUNCATE is blocked by the foreign key before the trigger is even reached, so
    // this asserts the effect: the rows survive.
    let _ = sqlx::query("TRUNCATE accounts CASCADE")
        .execute(&pool)
        .await;
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM accounts")
        .fetch_one(&pool)
        .await
        .expect("count accounts");
    assert!(remaining >= 2, "TRUNCATE must not empty the accounts table");
}

#[sqlx::test]
async fn v3_names_are_unique_across_the_board(pool: sqlx::PgPool) {
    // Global uniqueness, so the picker can never show two entries the cashier cannot
    // tell apart. Measured on both tables.
    let dup_account = sqlx::query("INSERT INTO accounts (name) VALUES ('cash')")
        .execute(&pool)
        .await;
    assert!(dup_account.is_err(), "two accounts may not share a name");

    let dup_method = sqlx::query(
        "INSERT INTO payment_methods (account_id, name)
         VALUES ('00000000-0000-0000-0000-000000000005', 'cash')",
    )
    .execute(&pool)
    .await;
    assert!(
        dup_method.is_err(),
        "two methods may not share a name, not even on different accounts"
    );

    // A blank name is refused after trimming, on both tables.
    let blank_method = sqlx::query(
        "INSERT INTO payment_methods (account_id, name)
         VALUES ('00000000-0000-0000-0000-000000000003', '   ')",
    )
    .execute(&pool)
    .await;
    assert!(blank_method.is_err(), "a blank method name is refused");
}

#[sqlx::test]
async fn v3_the_till_guard_locks_the_account_row_so_a_racing_writer_waits(pool: sqlx::PgPool) {
    // **The race this closes, reproduced before the fix.** The invariant "the default
    // account always has an active method" was an `EXISTS` check in a BEFORE trigger,
    // which cannot see another transaction's uncommitted work. Measured with two real
    // sessions on the pre-fix schema: A deactivated `m1`, B deactivated `m2`
    // concurrently, each saw the other method as still active, both committed, and the
    // default account was left with **zero** active methods.
    //
    // The fix is `SELECT ... FOR UPDATE` on the account row. This test asserts the
    // observable consequence of that lock rather than the SQL: a second writer's
    // statement cannot complete while the first holds the account, and the state it
    // then judges is the committed one.
    let cash_account = uuid::Uuid::from_u128(3);
    sqlx::query("INSERT INTO payment_methods (account_id, name) VALUES ($1, 'm1'), ($1, 'm2')")
        .bind(cash_account)
        .execute(&pool)
        .await
        .expect("insert two active methods");
    sqlx::query("UPDATE payment_methods SET is_active = false WHERE name = 'cash'")
        .execute(&pool)
        .await
        .expect("retire the seeded method");

    // A takes the account lock by deactivating m1, and holds it.
    let mut a = pool.begin().await.expect("begin A");
    sqlx::query("UPDATE payment_methods SET is_active = false WHERE name = 'm1'")
        .execute(&mut *a)
        .await
        .expect("A deactivates m1");

    // B tries to deactivate m2 through a second connection. With the lock it cannot
    // complete; without it, it would sail through in milliseconds.
    let pool_b = pool.clone();
    let mut b_task = tokio::spawn(async move {
        let mut b = pool_b.begin().await.expect("begin B");
        let result = sqlx::query("UPDATE payment_methods SET is_active = false WHERE name = 'm2'")
            .execute(&mut *b)
            .await;
        match result {
            Ok(_) => b.commit().await.map(|_| ()).map_err(|e| e.to_string()),
            Err(e) => {
                let _ = b.rollback().await;
                Err(e.to_string())
            }
        }
    });

    // If B finishes while A still holds the account, there was no lock and the race
    // is open. That is the assertion: it must still be pending.
    let raced = tokio::time::timeout(std::time::Duration::from_millis(500), &mut b_task).await;
    assert!(
        raced.is_err(),
        "the second writer completed while the first held the account: the guard does \
         not serialize them, so two concurrent deactivations could empty the till"
    );

    // Release: A commits. B now proceeds against the committed state and must refuse,
    // because m1 is already inactive and m2 is the last one left.
    a.commit().await.expect("commit A");
    let b_outcome = b_task.await.expect("B finished");

    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM payment_methods WHERE account_id = $1 AND is_active",
    )
    .bind(cash_account)
    .fetch_one(&pool)
    .await
    .expect("count active methods");

    assert!(
        active >= 1,
        "the default account must never be left with no active method, even when two \
         deactivations race; B's outcome was {b_outcome:?}"
    );
}

// ── v3 T0: accounts and payment methods write path ──────────────────────────
//
// The endpoints v3 cannot work without. Without them `cash` would be the only
// method in the system and a sale on a bank transfer would be untestable — the
// central claim of the slice would have no way to be expressed.
//
// Every refusal asserted here is also enforced in `0012`'s schema. The API check
// exists for the status code; the schema check is the one that holds when a psql
// session bypasses this file, which is why the same rule is measured twice.

#[sqlx::test]
async fn v3_t0_lists_the_seeded_accounts_with_their_methods(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    let response = client.get("/api/accounts").await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let accounts = response.body.as_array().expect("array");

    // The default first, then by name: the till is what the sales footer reads and
    // the common case must not depend on the shop's language ordering.
    assert_eq!(accounts.len(), 2);
    assert_eq!(accounts[0]["name"], "cash");
    assert_eq!(accounts[0]["is_default"], true);
    assert_eq!(accounts[1]["name"], "credit");
    assert_eq!(accounts[1]["is_default"], false);

    // Each carries its methods nested, and an account's methods are complete even
    // when the account itself has none.
    let cash_methods = accounts[0]["methods"].as_array().expect("methods array");
    assert_eq!(cash_methods.len(), 1);
    assert_eq!(cash_methods[0]["name"], "cash");
    assert_eq!(cash_methods[0]["is_active"], true);

    let credit_methods = accounts[1]["methods"].as_array().expect("methods array");
    assert_eq!(
        credit_methods[0]["is_active"], false,
        "credit is seeded inactive: v3 cannot collect a credit sale"
    );
}

#[sqlx::test]
async fn v3_t0_creates_an_account_and_refuses_a_duplicate_or_blank_name(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    let created = client
        .post(
            "/api/accounts",
            serde_json::json!({ "name": "  Banco Galicia  " }),
        )
        .await;
    assert_eq!(created.status(), axum::http::StatusCode::CREATED);
    assert_eq!(
        created.body["name"], "Banco Galicia",
        "the name is trimmed before it is stored"
    );
    assert_eq!(
        created.body["is_default"], false,
        "creating an account does not move the shop's till unless the caller says so"
    );

    let duplicate = client
        .post(
            "/api/accounts",
            serde_json::json!({ "name": "Banco Galicia" }),
        )
        .await;
    assert_eq!(
        duplicate.status(),
        axum::http::StatusCode::CONFLICT,
        "names are globally unique"
    );

    let blank = client
        .post("/api/accounts", serde_json::json!({ "name": "   " }))
        .await;
    assert_eq!(
        blank.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "a name of spaces is empty"
    );
}

#[sqlx::test]
async fn v3_t0_creates_a_method_and_refuses_an_unknown_account(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    let created = client
        .post(
            "/api/payment-methods",
            serde_json::json!({
                "name": "transfer",
                "account_id": "00000000-0000-0000-0000-000000000003"
            }),
        )
        .await;
    assert_eq!(created.status(), axum::http::StatusCode::CREATED);
    assert_eq!(created.body["name"], "transfer");
    assert_eq!(
        created.body["is_active"], true,
        "creating a method is how the shop starts using it"
    );

    // A blank name is refused here as well as in `accounts`. Measured while mutating
    // this file: with the trim removed from THIS handler the test stayed green,
    // because the only blank-name assertion was the account one. A rule asserted on
    // one endpoint is not a rule asserted on both.
    let blank = client
        .post(
            "/api/payment-methods",
            serde_json::json!({
                "name": "   ",
                "account_id": "00000000-0000-0000-0000-000000000003"
            }),
        )
        .await;
    assert_eq!(
        blank.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "a method name of spaces is empty"
    );

    let unknown = client
        .post(
            "/api/payment-methods",
            serde_json::json!({
                "name": "orphan",
                "account_id": "00000000-0000-0000-0000-0000000000ff"
            }),
        )
        .await;
    assert_eq!(
        unknown.status(),
        axum::http::StatusCode::NOT_FOUND,
        "an unknown account is a 404, not a 500 from the foreign key"
    );

    // Global uniqueness across accounts, not per account: the picker must never show
    // two entries the cashier cannot tell apart. The second account is created here
    // rather than borrowed from the seeds — the seeded `credit` account refuses every
    // method now (`S2`, `odd/tasks/v3-seed-identities.md`), which would make this a
    // test of that rule instead of this one.
    let other = client
        .post("/api/accounts", serde_json::json!({ "name": "otra" }))
        .await;
    assert_eq!(other.status(), axum::http::StatusCode::CREATED);
    let other_id = other.body["id"].as_str().expect("id").to_string();

    let cross_account_duplicate = client
        .post(
            "/api/payment-methods",
            serde_json::json!({
                "name": "transfer",
                "account_id": other_id
            }),
        )
        .await;
    assert_eq!(
        cross_account_duplicate.status(),
        axum::http::StatusCode::CONFLICT,
        "a name is unique across the whole table, not per account"
    );
}

#[sqlx::test]
async fn v3_t0_a_rename_corrects_the_name_everywhere_and_moves_no_balance(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    // A sale naming the cash method, so the rename has history to act on.
    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "rename-probe", "list_price": "10.00" }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("product id");
    client
        .post(
            "/api/sales",
            serde_json::json!({ "items": [{ "product_id": product_id, "quantity": 1 }] }),
        )
        .await;

    let before = client.get("/api/sales").await;
    // The wire is flat since T4: an id and a joined name, no `account_id`.
    assert_eq!(
        before.body[0]["payment_method_id"],
        "00000000-0000-0000-0000-000000000004"
    );
    assert_eq!(before.body[0]["payment_method_name"], "cash");

    let renamed = client
        .patch(
            "/api/payment-methods/00000000-0000-0000-0000-000000000004",
            serde_json::json!({ "name": "MervadoPago" }),
        )
        .await;
    assert_eq!(renamed.status(), axum::http::StatusCode::OK);

    let after = client.get("/api/sales").await;
    assert_eq!(
        after.body[0]["payment_method_name"], "MervadoPago",
        "renaming CORRECTS history: documents store the id, so the typo is fixed everywhere"
    );
    assert_eq!(
        after.body[0]["payment_method_id"], "00000000-0000-0000-0000-000000000004",
        "and the identity is unchanged by the rename"
    );
}

#[sqlx::test]
async fn v3_t0_a_referenced_method_cannot_move_but_an_unreferenced_one_can(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    // The destination is a fresh account, not the seeded credit one. It used to be
    // `…0005`, which `S2` of `odd/tasks/v3-seed-identities.md` now forbids: no method
    // may be attached to the debt account. The case this test was written for — an
    // unreferenced method is a typo and must be movable — is unchanged, and it needs
    // *some* real destination, so it creates one.
    let destination = client
        .post("/api/accounts", serde_json::json!({ "name": "destino" }))
        .await;
    assert_eq!(destination.status(), axum::http::StatusCode::CREATED);
    let destination_id = destination.body["id"].as_str().expect("id").to_string();

    // Positive case first: an unreferenced method is a typo and must be correctable.
    let free = client
        .post(
            "/api/payment-methods",
            serde_json::json!({
                "name": "free-method",
                "account_id": "00000000-0000-0000-0000-000000000003"
            }),
        )
        .await;
    let free_id = free.body["id"].as_str().expect("id").to_string();
    let moved = client
        .patch(
            &format!("/api/payment-methods/{free_id}"),
            serde_json::json!({ "account_id": destination_id }),
        )
        .await;
    assert_eq!(
        moved.status(),
        axum::http::StatusCode::OK,
        "an unreferenced method may move: it is a typo, not history"
    );

    // Then the refusal: the cash method is named by the sale above only if one
    // exists, so create it here.
    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "move-probe", "list_price": "5.00" }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("product id");
    client
        .post(
            "/api/sales",
            serde_json::json!({ "items": [{ "product_id": product_id, "quantity": 1 }] }),
        )
        .await;

    let refused = client
        .patch(
            "/api/payment-methods/00000000-0000-0000-0000-000000000004",
            serde_json::json!({ "account_id": "00000000-0000-0000-0000-000000000005" }),
        )
        .await;
    assert_eq!(
        refused.status(),
        axum::http::StatusCode::CONFLICT,
        "a referenced method must not move accounts: yesterday's balance would change today"
    );
}

#[sqlx::test]
async fn v3_t0_moving_the_till_is_a_single_operation_not_a_deadlock(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    // The deadlock T1's first review found: with a BEFORE trigger forbidding every
    // true→false, no ordering could move the default. The API path must be able to
    // do it in one call.
    //
    // The destination is a fresh account with an active method, not the seeded
    // `credit` one. It used to be `credit`, and `S3` of
    // `odd/tasks/v3-seed-identities.md` now forbids the debt account from ever being
    // the till — so that destination can no longer express "the till moved" and the
    // case needs one that can. The rule being tested is unchanged: the move is a
    // single legal operation, whatever the destination.
    let candidate = client
        .post("/api/accounts", serde_json::json!({ "name": "caja-nueva" }))
        .await;
    assert_eq!(candidate.status(), axum::http::StatusCode::CREATED);
    let candidate_id = candidate.body["id"].as_str().expect("id").to_string();

    // The candidate needs an ACTIVE method first, and that is the rule working rather
    // than an obstacle to it: a till nobody can take money at is not a till. Asserted
    // as a prerequisite so the test cannot be read as "the till moves freely".
    let activated = client
        .post(
            "/api/payment-methods",
            serde_json::json!({ "name": "efectivo-nuevo", "account_id": candidate_id }),
        )
        .await;
    assert_eq!(
        activated.status(),
        axum::http::StatusCode::CREATED,
        "the new till must be able to accept money"
    );

    let promoted = client
        .patch(
            &format!("/api/accounts/{candidate_id}"),
            serde_json::json!({ "is_default": true }),
        )
        .await;
    assert_eq!(
        promoted.status(),
        axum::http::StatusCode::OK,
        "moving the till must be a legal, single operation"
    );

    let accounts = client.get("/api/accounts").await;
    let defaults: Vec<&str> = accounts
        .body
        .as_array()
        .expect("array")
        .iter()
        .filter(|a| a["is_default"].as_bool().unwrap_or(false))
        .map(|a| a["name"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(defaults.len(), 1, "exactly one default at commit");
    assert_eq!(
        defaults[0], "caja-nueva",
        "the till moved where it was told"
    );

    // Demoting without promoting another is refused by the deferred constraint, and
    // the caller sees an error rather than a silent zero-till state. Asserted as "not
    // 2xx" because the mapping from a deferred-constraint violation to a status code
    // is the framework's, and what matters is that it does not succeed.
    let demoted = client
        .patch(
            &format!("/api/accounts/{candidate_id}"),
            serde_json::json!({ "is_default": false }),
        )
        .await;
    assert!(
        demoted.status().is_server_error() || demoted.status().is_client_error(),
        "ending with no default must not succeed, got {}",
        demoted.status()
    );
}

#[sqlx::test]
async fn v3_t0_patch_with_nothing_to_change_is_refused(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());

    // An empty PATCH would be a 200 that changed nothing while looking like it did.
    let empty_account = client
        .patch(
            "/api/accounts/00000000-0000-0000-0000-000000000003",
            serde_json::json!({}),
        )
        .await;
    assert_eq!(empty_account.status(), axum::http::StatusCode::BAD_REQUEST);

    let empty_method = client
        .patch(
            "/api/payment-methods/00000000-0000-0000-0000-000000000004",
            serde_json::json!({}),
        )
        .await;
    assert_eq!(empty_method.status(), axum::http::StatusCode::BAD_REQUEST);

    let missing = client
        .patch(
            "/api/accounts/00000000-0000-0000-0000-0000000000ff",
            serde_json::json!({ "name": "nope" }),
        )
        .await;
    assert_eq!(missing.status(), axum::http::StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn v3_t0_creating_an_account_as_default_fails_because_it_has_no_method_yet(
    pool: sqlx::PgPool,
) {
    // **Found by native review, and the gap was a missing assertion.**
    //
    // `POST /api/accounts` accepts `is_default: true`, and a freshly inserted account
    // has no payment methods — so the schema's `guard_new_default_account_has_active_method`
    // refuses it. That is the invariant working, but the route offered an option that
    // could never succeed, and no test ever passed `is_default: true` to `POST`, so
    // nothing said so.
    //
    // What the assertion pins is the **status code**: a caller asking for something
    // the schema forbids must get a refusal it can act on, not a 500 that reads as a
    // server fault. The positive path exists in `PATCH` (create the account, attach a
    // method, then promote), and `v3_t0_moving_the_till_is_a_single_operation…` covers
    // it.
    let client = common::client(pool.clone());

    let refused = client
        .post(
            "/api/accounts",
            serde_json::json!({ "name": "nueva-caja", "is_default": true }),
        )
        .await;

    assert!(
        refused.status().is_client_error(),
        "creating a default account with no method must be refused as a client error, \
         not reported as a server fault; got {}",
        refused.status()
    );

    // And nothing was written: the refusal is total, not partial.
    let accounts = client.get("/api/accounts").await;
    let names: Vec<&str> = accounts
        .body
        .as_array()
        .expect("array")
        .iter()
        .map(|a| a["name"].as_str().unwrap_or(""))
        .collect();
    assert!(
        !names.contains(&"nueva-caja"),
        "a refused creation must not leave an account behind: {names:?}"
    );

    // The supported path, stated so the refusal above cannot be read as "cannot ever
    // promote": create without the flag, attach a method, then PATCH the default.
    let created = client
        .post("/api/accounts", serde_json::json!({ "name": "caja-2" }))
        .await;
    assert_eq!(created.status(), axum::http::StatusCode::CREATED);
    let id = created.body["id"].as_str().expect("id").to_string();
    client
        .post(
            "/api/payment-methods",
            serde_json::json!({ "name": "efectivo-2", "account_id": id }),
        )
        .await;
    let promoted = client
        .patch(
            &format!("/api/accounts/{id}"),
            serde_json::json!({ "is_default": true }),
        )
        .await;
    assert_eq!(
        promoted.status(),
        axum::http::StatusCode::OK,
        "the supported sequence must work: create, attach a method, promote"
    );
}

// ── v3 T3: `money_movements` and derived balances ───────────────────────────
//
// The view is the single definition of "money moved", and `GET /api/accounts`
// carries its sum. What these tests pin is the SIGN of every document and the fact
// that a balance is a sum over documents rather than a stored field — the same
// property `stock = SUM(movements)` already has.

#[sqlx::test]
async fn v3_the_credit_seed_keeps_its_identity_in_the_schema(pool: sqlx::PgPool) {
    // `odd/tasks/v3-seed-identities.md`, S1-S4. The server names these two rows by
    // fixed id (`CREDIT_ACCOUNT_ID`, `CREDIT_PAYMENT_METHOD_ID`) without a lookup, so
    // the rows holding those ids have to keep meaning what the constants say. What is
    // pinned here is the schema's verdict, because a `psql` session never passes
    // through `accounts.rs`.
    //
    // Measured on the tree this test was written against: both statements succeeded.
    const CREDIT_ACCOUNT: &str = "00000000-0000-0000-0000-000000000005";
    const CREDIT_METHOD: &str = "00000000-0000-0000-0000-000000000006";
    const CASH_ACCOUNT: &str = "00000000-0000-0000-0000-000000000003";

    // S1: the seeded method never leaves its account.
    let moved_away = sqlx::query("UPDATE payment_methods SET account_id = $1 WHERE id = $2")
        .bind(uuid::Uuid::from_u128(3))
        .bind(uuid::Uuid::from_u128(6))
        .execute(&pool)
        .await;
    assert!(
        moved_away.is_err(),
        "the seeded credit method must never change account: the server resolves it \
         by fixed id and a move makes the constant and the row disagree"
    );

    // S2: nothing may be attached to the debt account later either. This is the same
    // rule from the other side, and the side that was reachable in practice —
    // measured as `PATCH /api/payment-methods/…0006 {{"account_id":"…0003"}}` -> 200.
    let moved_onto =
        sqlx::query("INSERT INTO payment_methods (account_id, name) VALUES ($1, 'onto-credit')")
            .bind(uuid::Uuid::from_u128(5))
            .execute(&pool)
            .await;
    assert!(
        moved_onto.is_err(),
        "no method may be attached to the seeded credit account: a method there would \
         mean 'a customer debt was settled this way', which no v3 document says"
    );

    // S3: the debt account is never the shop's till. `getCash()` reads `is_default`,
    // so promoting it makes the counter show the debt balance while an omitted-method
    // sale still posts to cash.
    //
    // The credit method is activated first on purpose: without that the promotion is
    // refused for a *different* reason (`the default account always has an active
    // method`), and the assertion would pass while proving the wrong rule.
    sqlx::query("UPDATE payment_methods SET is_active = true WHERE id = $1")
        .bind(uuid::Uuid::from_u128(6))
        .execute(&pool)
        .await
        .expect("activating the seeded method is allowed and must stay allowed");
    let promoted = sqlx::query("UPDATE accounts SET is_default = true WHERE id = $1")
        .bind(uuid::Uuid::from_u128(5))
        .execute(&pool)
        .await;
    assert!(
        promoted.is_err(),
        "the seeded credit account must never be the shop's till"
    );

    // S4's other half: the seeded rows are still the seeded rows, unchanged by any of
    // the refusals above.
    let still: Vec<(uuid::Uuid, String, bool)> =
        sqlx::query_as("SELECT id, name, is_default FROM accounts ORDER BY name")
            .fetch_all(&pool)
            .await
            .expect("read accounts");
    assert_eq!(still.len(), 2, "the refusals wrote nothing");
    assert_eq!(still[1].0, uuid::Uuid::from_u128(5));
    assert_eq!(still[1].1, "credit");
    assert!(!still[1].2, "credit is not the till");
    assert_eq!(still[0].0, uuid::Uuid::from_u128(3));
    assert!(still[0].2, "cash is still the till");

    let method: (uuid::Uuid, bool) =
        sqlx::query_as("SELECT account_id, is_active FROM payment_methods WHERE id = $1")
            .bind(uuid::Uuid::from_u128(6))
            .fetch_one(&pool)
            .await
            .expect("read the seeded method");
    assert_eq!(
        method.0,
        uuid::Uuid::from_u128(5),
        "the seeded method still belongs to the seeded account"
    );
    assert!(
        method.1,
        "S6: activating it is not this work's business, so the UPDATE above stands"
    );

    // S6: the two things that *are* editable still are, so the new guard cannot be
    // satisfied by freezing the rows. `name` corrects a typo; documents store the id.
    sqlx::query("UPDATE accounts SET name = 'Crédito' WHERE id = $1")
        .bind(uuid::Uuid::from_u128(5))
        .execute(&pool)
        .await
        .expect("renaming the credit account must stay allowed");
    sqlx::query("UPDATE payment_methods SET name = 'fiado' WHERE id = $1")
        .bind(uuid::Uuid::from_u128(6))
        .execute(&pool)
        .await
        .expect("renaming the credit method must stay allowed");

    // **And now the same two edits through the handler, which is a different
    // statement.** This block exists because the two above passed while both requests
    // answered `500` (review `R3-001`, CRITICAL). `accounts.rs` sends
    // `account_id = COALESCE($4, account_id)` on every PATCH, so it *names* the column
    // even when the caller is renaming; `SET name = …` does not, and a trigger declared
    // `UPDATE OF account_id` therefore never fired in the test but always fired in
    // production. Measured before the fix: `PATCH {"name":"fiado"}` -> 500 and
    // `PATCH {"is_active":true}` -> 500.
    let client = common::client(pool.clone());
    let renamed = client
        .patch(
            "/api/payment-methods/00000000-0000-0000-0000-000000000006",
            serde_json::json!({ "name": "fiado" }),
        )
        .await;
    assert_eq!(
        renamed.status(),
        axum::http::StatusCode::OK,
        "S6: renaming the seeded credit method through the API must stay allowed; \
         the handler names account_id on every PATCH and a guard must not read that \
         as an attempt to move it. body was: {}",
        renamed.text
    );
    assert_eq!(renamed.body["name"], "fiado");
    assert_eq!(
        renamed.body["account_id"], "00000000-0000-0000-0000-000000000005",
        "a rename moves nothing"
    );

    let reactivated = client
        .patch(
            "/api/payment-methods/00000000-0000-0000-0000-000000000006",
            serde_json::json!({ "is_active": false }),
        )
        .await;
    assert_eq!(
        reactivated.status(),
        axum::http::StatusCode::OK,
        "S6: flipping is_active on the seeded credit method through the API must stay \
         allowed, for the same reason; body was: {}",
        reactivated.text
    );
    assert_eq!(reactivated.body["is_active"], false);

    // **And the no-op: the destination it already has.** A client that sends a full
    // object names `account_id` with the value it already holds, and that is a
    // statement about nothing. The schema says so with `IS DISTINCT FROM`; this
    // assertion is what makes the handler say the same thing. Measured before the fix:
    // the API answered `409` while `psql` answered `UPDATE 1` for the same write
    // (`R3-001`, second occurrence).
    let same_destination = client
        .patch(
            "/api/payment-methods/00000000-0000-0000-0000-000000000006",
            serde_json::json!({ "account_id": "00000000-0000-0000-0000-000000000005" }),
        )
        .await;
    assert_eq!(
        same_destination.status(),
        axum::http::StatusCode::OK,
        "PATCHing the destination a method already has changes nothing and must be \
         allowed, exactly as the schema's IS DISTINCT FROM allows it; body was: {}",
        same_destination.text
    );
    assert_eq!(
        same_destination.body["account_id"],
        "00000000-0000-0000-0000-000000000005"
    );

    // The same courtesy for any other method: sending its own account is not a move.
    let cash_noop = client
        .patch(
            "/api/payment-methods/00000000-0000-0000-0000-000000000004",
            serde_json::json!({ "account_id": "00000000-0000-0000-0000-000000000003" }),
        )
        .await;
    assert_eq!(
        cash_noop.status(),
        axum::http::StatusCode::OK,
        "a no-op move of an ordinary method is allowed too; body was: {}",
        cash_noop.text
    );

    // And the cash side still moves freely: the guard is about seed identity, not
    // about forbidding the till's own method from being corrected before it has
    // history.
    sqlx::query("UPDATE payment_methods SET account_id = $1 WHERE id = $2")
        .bind(uuid::Uuid::from_u128(3))
        .bind(uuid::Uuid::from_u128(4))
        .execute(&pool)
        .await
        .expect("the cash method is already on the cash account; a no-op update must pass");
    let _ = (CREDIT_ACCOUNT, CREDIT_METHOD, CASH_ACCOUNT);
}

#[sqlx::test]
async fn v3_the_credit_seed_answers_with_a_cause_not_a_server_fault(pool: sqlx::PgPool) {
    // `odd/tasks/v3-seed-identities.md`, S5. Both refusals are the caller's mistake,
    // understood and refused, so both are 409 with a body naming the rule. Measured
    // before the fix: moving the method answered 200, and promoting the account while
    // its method was inactive answered 500 — `conflict_or_500` maps only `23505`, so
    // any `RAISE EXCEPTION` from a guard trigger read as an internal error.
    let client = common::client(pool.clone());

    let moved = client
        .patch(
            "/api/payment-methods/00000000-0000-0000-0000-000000000006",
            serde_json::json!({ "account_id": "00000000-0000-0000-0000-000000000003" }),
        )
        .await;
    assert_eq!(
        moved.status(),
        axum::http::StatusCode::CONFLICT,
        "moving the seeded credit method must be a refusal the caller can act on, \
         not a success and not a server fault; body was: {}",
        moved.text
    );

    let promoted_while_inactive = client
        .patch(
            "/api/accounts/00000000-0000-0000-0000-000000000005",
            serde_json::json!({ "is_default": true }),
        )
        .await;
    assert_eq!(
        promoted_while_inactive.status(),
        axum::http::StatusCode::CONFLICT,
        "promoting the debt account must be refused as a client error, not reported \
         as a server fault; body was: {}",
        promoted_while_inactive.text
    );

    // Nothing was written by either refusal: the state the seeds describe is intact.
    let accounts = client.get("/api/accounts").await;
    let till = accounts
        .body
        .as_array()
        .expect("array")
        .iter()
        .find(|a| a["is_default"].as_bool() == Some(true))
        .expect("a till");
    assert_eq!(
        till["name"].as_str(),
        Some("cash"),
        "cash is still the till after two refused writes"
    );

    let methods = client.get("/api/payment-methods").await;
    let credit = methods
        .body
        .as_array()
        .expect("array")
        .iter()
        .find(|m| m["name"].as_str() == Some("credit"))
        .expect("the credit method");
    assert_eq!(
        credit["account_id"].as_str(),
        Some("00000000-0000-0000-0000-000000000005"),
        "the credit method still belongs to the credit account"
    );
}

#[sqlx::test]
async fn v3_t3_a_fresh_ledger_balances_to_zero_on_every_account(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());
    let response = client.get("/api/accounts").await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    // COALESCE, not NULL: an account with no movements must read `0.00` so the client
    // has one shape to render, and the string must carry two decimals like every
    // other amount in this API.
    for account in response.body.as_array().expect("array") {
        assert_eq!(
            account["balance"], "0.00",
            "every account starts at zero, as a two-decimal string"
        );
    }
}

#[sqlx::test]
async fn v3_t3_the_four_documents_move_money_with_the_right_sign(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());
    let cash = "00000000-0000-0000-0000-000000000003";

    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "sign-probe", "list_price": "100.00" }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("product id").to_string();

    // A sale brings 100 in.
    let sale = client
        .post(
            "/api/sales",
            serde_json::json!({ "items": [{ "product_id": product_id, "quantity": 1 }] }),
        )
        .await;
    assert_eq!(sale.status(), axum::http::StatusCode::CREATED);
    let sale_id = sale.body["id"].as_str().expect("sale id").to_string();
    let sale_item = sale.body["items"][0]["id"]
        .as_str()
        .expect("item id")
        .to_string();

    assert_eq!(
        balance_of(&client, cash).await,
        "100.00",
        "a sale puts money INTO the account its method names"
    );

    // A purchase takes 40 out.
    client
        .post(
            "/api/purchases",
            serde_json::json!({ "items": [{ "product_id": product_id, "quantity": 2, "unit_price": "20.00" }] }),
        )
        .await;
    assert_eq!(
        balance_of(&client, cash).await,
        "60.00",
        "a purchase takes money OUT"
    );

    // A refund of 100 gives it back to the customer: 60 - 100 = -40.
    client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({ "items": [{ "sale_item_id": sale_item, "quantity": 1 }] }),
        )
        .await;
    assert_eq!(
        balance_of(&client, cash).await,
        "-40.00",
        "a credit note takes money OUT, and a balance may go negative"
    );

    // The figure the retired `/api/cash` computed by hand, checked against the same
    // arithmetic read straight from the documents. That agreement is what made the
    // retirement safe: `T6` removed an endpoint whose number the view already
    // reproduces, not one whose number changed.
    let (hand_computed,): (rust_decimal::Decimal,) = sqlx::query_as(
        "SELECT (COALESCE((SELECT SUM(total) FROM sales), 0)
               - COALESCE((SELECT SUM(total) FROM credit_notes), 0)
               - COALESCE((SELECT SUM(total) FROM purchases), 0)
               + COALESCE((SELECT SUM(total) FROM supplier_returns), 0))::numeric",
    )
    .fetch_one(&pool)
    .await
    .expect("hand-computed till");
    assert_eq!(
        hand_computed,
        rust_decimal::Decimal::from_str_exact("-40.00").expect("decimal"),
        "the view must reproduce the figure /api/cash computed by hand"
    );
}

#[sqlx::test]
async fn v3_t3_a_supplier_return_brings_money_back(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());
    let cash = "00000000-0000-0000-0000-000000000003";

    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "return-probe", "list_price": "10.00" }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("id").to_string();

    let purchase = client
        .post(
            "/api/purchases",
            serde_json::json!({ "items": [{ "product_id": product_id, "quantity": 3, "unit_price": "10.00" }] }),
        )
        .await;
    // The purchase response nests the header under `purchase` (`PurchaseWithItems`),
    // where a sale flattens it. Measured here rather than assumed: reading the root
    // gave no id at all.
    let purchase_id = purchase.body["purchase"]["id"]
        .as_str()
        .expect("purchase id")
        .to_string();
    let purchase_item = purchase.body["items"][0]["id"]
        .as_str()
        .expect("item id")
        .to_string();
    assert_eq!(balance_of(&client, cash).await, "-30.00");

    client
        .post(
            &format!("/api/purchases/{purchase_id}/supplier-returns"),
            serde_json::json!({ "items": [{ "purchase_item_id": purchase_item, "quantity": 1 }] }),
        )
        .await;
    assert_eq!(
        balance_of(&client, cash).await,
        "-20.00",
        "a supplier return brings money BACK, the mirror of a purchase"
    );
}

#[sqlx::test]
async fn v3_t3_a_second_account_keeps_its_own_balance(pool: sqlx::PgPool) {
    // The whole reason a destination is derivable: two accounts, one sale each, and
    // neither balance is the other's. This is what `T6` will use to show the till's
    // footer no longer means "every sale".
    let client = common::client(pool.clone());
    let cash = "00000000-0000-0000-0000-000000000003";
    let bank = "00000000-0000-0000-0000-000000000005";

    let created = client
        .post(
            "/api/payment-methods",
            serde_json::json!({ "name": "transferencia", "account_id": cash }),
        )
        .await;
    let method_id = created.body["id"].as_str().expect("method id").to_string();

    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "split-probe", "list_price": "50.00" }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("id").to_string();

    // A sale through the new method still lands in `cash`, because that is the
    // account the method points at.
    client
        .post(
            "/api/sales",
            serde_json::json!({
                "items": [{ "product_id": product_id, "quantity": 1 }],
                "payment_method_id": method_id
            }),
        )
        .await;
    assert_eq!(balance_of(&client, cash).await, "50.00");
    assert_eq!(
        balance_of(&client, bank).await,
        "0.00",
        "the credit account is untouched by a sale that names a cash method"
    );
}

/// Reads one account's balance from `GET /api/accounts`.
async fn balance_of(client: &common::Client, account_id: &str) -> String {
    let response = client.get("/api/accounts").await;
    response
        .body
        .as_array()
        .expect("array")
        .iter()
        .find(|a| a["id"] == account_id)
        .unwrap_or_else(|| panic!("account {account_id} missing from /api/accounts"))["balance"]
        .as_str()
        .expect("balance string")
        .to_string()
}

#[sqlx::test]
async fn v3_t2_the_lifecycle_drop_changed_no_balance(pool: sqlx::PgPool) {
    // **This replaces `v3_t3_the_view_counts_only_confirmed_sales`, and the reason is
    // the whole point of `0015`.**
    //
    // That test proved a sale whose `status` was not confirmed did not move the
    // balance. It made the predicate observable — and mutation had shown nothing else
    // was. But `0015` drops the column, so the state it constructed can no longer
    // exist, and a test asserting an impossible state is vacuous rather than passing.
    //
    // What replaces it is the risk `0015` actually carries: a `DROP COLUMN` on
    // populated tables is the one irreversible act in the slice. The migration
    // already asserts this inside its own transaction — every account total measured
    // before the drop must be identical after — and this test asserts the same
    // property from outside, on data a sale and a document actually created.
    let client = common::client(pool.clone());

    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "t2-probe", "list_price": "70.00" }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("id").to_string();
    client
        .post(
            "/api/sales",
            serde_json::json!({ "items": [{ "product_id": product_id, "quantity": 1 }] }),
        )
        .await;

    // The lifetime totals, computed the way the retired `/api/cash` did — from the
    // documents, with no filter, because the column that used to carry the filter is
    // gone.
    let (from_documents,): (rust_decimal::Decimal,) = sqlx::query_as(
        "SELECT (COALESCE((SELECT SUM(total) FROM sales), 0)
               - COALESCE((SELECT SUM(total) FROM credit_notes), 0)
               - COALESCE((SELECT SUM(total) FROM purchases), 0)
               + COALESCE((SELECT SUM(total) FROM supplier_returns), 0))::numeric",
    )
    .fetch_one(&pool)
    .await
    .expect("totals from documents");

    // The same total through the view, which `0015` recreated without a predicate.
    let (from_view,): (rust_decimal::Decimal,) =
        sqlx::query_as("SELECT COALESCE(SUM(amount), 0)::numeric FROM money_movements")
            .fetch_one(&pool)
            .await
            .expect("totals from the view");

    assert_eq!(
        from_documents, from_view,
        "the view must still reproduce the documents after the lifecycle drop"
    );
    assert_eq!(
        from_view,
        rust_decimal::Decimal::from_str_exact("70.00").expect("decimal"),
        "and the sale that exists must still count, unfiltered"
    );

    // The columns are gone, which is what makes the assertion above unconditional.
    let lifecycle_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
          WHERE table_name = 'sales' AND column_name IN ('status', 'reversed_at')",
    )
    .fetch_one(&pool)
    .await
    .expect("read columns");
    assert_eq!(lifecycle_columns, 0);
}

#[sqlx::test]
async fn v3_t4_a_sale_on_another_method_lands_in_that_account(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());
    let cash = "00000000-0000-0000-0000-000000000003";

    // A real bank account with its own method, which is the whole point of v3: a
    // sale whose money did not go to the till.
    let account = client
        .post("/api/accounts", serde_json::json!({ "name": "banco" }))
        .await;
    let account_id = account.body["id"].as_str().expect("account id").to_string();
    let method = client
        .post(
            "/api/payment-methods",
            serde_json::json!({ "name": "transferencia", "account_id": account_id }),
        )
        .await;
    let method_id = method.body["id"].as_str().expect("method id").to_string();

    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "t4-probe", "list_price": "80.00" }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("product id").to_string();

    let sale = client
        .post(
            "/api/sales",
            serde_json::json!({
                "items": [{ "product_id": product_id, "quantity": 1 }],
                "payment_method_id": method_id
            }),
        )
        .await;
    assert_eq!(sale.status(), axum::http::StatusCode::CREATED);
    assert_eq!(sale.body["payment_method_name"], "transferencia");

    // The money is in the bank, NOT in the till. Before T4 this assertion was
    // impossible to write, and the till would have read 80.00 it never saw.
    assert_eq!(balance_of(&client, cash).await, "0.00");
    assert_eq!(balance_of(&client, &account_id).await, "80.00");
}

#[sqlx::test]
async fn v3_t4_an_omitted_method_still_means_the_till(pool: sqlx::PgPool) {
    // The differential guarantee: an older client sends no method and must keep
    // exactly the behaviour it had. This is the one assertion that makes the new
    // field additive rather than a breaking change.
    let client = common::client(pool.clone());
    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "t4-default", "list_price": "25.00" }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("id").to_string();

    let sale = client
        .post(
            "/api/sales",
            serde_json::json!({ "items": [{ "product_id": product_id, "quantity": 2 }] }),
        )
        .await;
    assert_eq!(sale.status(), axum::http::StatusCode::CREATED);
    assert_eq!(
        sale.body["payment_method_id"], "00000000-0000-0000-0000-000000000004",
        "an omitted method resolves to the seeded cash method"
    );
    assert_eq!(
        balance_of(&client, "00000000-0000-0000-0000-000000000003").await,
        "50.00"
    );
}

#[sqlx::test]
async fn v3_t4_an_unknown_method_is_a_404_and_writes_nothing(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());
    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "t4-unknown", "list_price": "10.00" }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("id").to_string();

    // Positive case first: the same body with a real method succeeds, so the refusal
    // below cannot be an artefact of a broken fixture.
    let ok = client
        .post(
            "/api/sales",
            serde_json::json!({
                "items": [{ "product_id": product_id, "quantity": 1 }],
                "payment_method_id": "00000000-0000-0000-0000-000000000004"
            }),
        )
        .await;
    assert_eq!(ok.status(), axum::http::StatusCode::CREATED);

    let refused = client
        .post(
            "/api/sales",
            serde_json::json!({
                "items": [{ "product_id": product_id, "quantity": 1 }],
                "payment_method_id": "00000000-0000-0000-0000-0000000000ff"
            }),
        )
        .await;
    assert_eq!(
        refused.status(),
        axum::http::StatusCode::NOT_FOUND,
        "an unknown method is a 404, not a foreign-key 500"
    );

    // Nothing was written by the refusal: the till still holds exactly one sale.
    assert_eq!(
        balance_of(&client, "00000000-0000-0000-0000-000000000003").await,
        "10.00"
    );
}

#[sqlx::test]
async fn v3_t4_a_credit_sale_must_name_a_customer(pool: sqlx::PgPool) {
    // S9, and it is written positive-first on purpose: a refusal alone could pass
    // because the customer is anonymous, rather than because on-account sales require
    // a named customer.
    let client = common::client(pool.clone());

    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "t4-credit", "list_price": "30.00" }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("id").to_string();

    // The sentinel bypasses `is_active` entirely; that flag governs the payment-method
    // picker, not the on-account API path.
    let customer = client
        .post(
            "/api/customers",
            serde_json::json!({ "name": "T4 Cliente" }),
        )
        .await;
    let customer_id = customer.body["id"]
        .as_str()
        .expect("customer id")
        .to_string();
    let accepted = client
        .post(
            "/api/sales",
            serde_json::json!({
                "customer_id": customer_id,
                "items": [{ "product_id": product_id, "quantity": 1 }],
                "payment_method_id": "on_account"
            }),
        )
        .await;
    assert_eq!(
        accepted.status(),
        axum::http::StatusCode::CREATED,
        "a sale on account to a named customer is accepted"
    );

    // Refused for the anonymous walk-in customer.
    let refused = client
        .post(
            "/api/sales",
            serde_json::json!({
                "items": [{ "product_id": product_id, "quantity": 1 }],
                "payment_method_id": "on_account"
            }),
        )
        .await;
    assert_eq!(
        refused.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "anonymous debt is not debt"
    );

    // The accepted sale has exactly one obligation; refusal left no sale or debt behind.
    let accepted_sale_id: uuid::Uuid = accepted.body["id"]
        .as_str()
        .expect("accepted sale id")
        .parse()
        .expect("sale UUID");
    let accepted_obligations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM obligations
          WHERE source_document_kind = 'sale' AND source_document_id = $1",
    )
    .bind(accepted_sale_id)
    .fetch_one(&pool)
    .await
    .expect("count accepted sale obligations");
    assert_eq!(accepted_obligations, 1, "accepted sale has exactly one debt row");

    let (sales, obligations): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM sales), (SELECT count(*) FROM obligations)",
    )
    .fetch_one(&pool)
    .await
    .expect("count sale and obligation rows");
    assert_eq!(sales, 1, "the refused sale left no sale behind");
    assert_eq!(obligations, 1, "the refused sale left no obligation behind");
}

// ── v3 T5: purchases, and the two corrections ───────────────────────────────
//
// The mirror of T4 for the paying-out side, plus the two things a correction
// decides: inherit the corrected document's method, or name its own.

#[sqlx::test]
async fn v3_t5_a_purchase_on_another_method_leaves_that_account(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());
    let cash = "00000000-0000-0000-0000-000000000003";

    let account = client
        .post(
            "/api/accounts",
            serde_json::json!({ "name": "banco-pagos" }),
        )
        .await;
    let account_id = account.body["id"].as_str().expect("account id").to_string();
    let method = client
        .post(
            "/api/payment-methods",
            serde_json::json!({ "name": "debito", "account_id": account_id }),
        )
        .await;
    let method_id = method.body["id"].as_str().expect("method id").to_string();

    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "t5-probe", "list_price": "40.00" }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("id").to_string();

    let purchase = client
        .post(
            "/api/purchases",
            serde_json::json!({
                "items": [{ "product_id": product_id, "quantity": 1 }],
                "payment_method_id": method_id
            }),
        )
        .await;
    assert_eq!(purchase.status(), axum::http::StatusCode::CREATED);
    assert_eq!(
        purchase.body["purchase"]["payment_method_name"], "debito",
        "the wire is flat, inside the nested purchase header"
    );

    // Money left the bank, not the till.
    assert_eq!(balance_of(&client, cash).await, "0.00");
    assert_eq!(balance_of(&client, &account_id).await, "-40.00");
}

#[sqlx::test]
async fn v3_t5_a_purchase_may_never_use_the_credit_method(pool: sqlx::PgPool) {
    // S9b. The positive case first: a named, ordinary method is accepted, so the
    // refusal below cannot be an artefact of a broken fixture.
    let client = common::client(pool.clone());
    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "t5-credit", "list_price": "20.00" }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("id").to_string();

    let accepted = client
        .post(
            "/api/purchases",
            serde_json::json!({
                "items": [{ "product_id": product_id, "quantity": 1 }],
                "payment_method_id": "00000000-0000-0000-0000-000000000004"
            }),
        )
        .await;
    assert_eq!(
        accepted.status(),
        axum::http::StatusCode::CREATED,
        "a purchase through a real method is accepted, even though the credit method is inactive"
    );

    let refused = client
        .post(
            "/api/purchases",
            serde_json::json!({
                "items": [{ "product_id": product_id, "quantity": 1 }],
                "payment_method_id": "00000000-0000-0000-0000-000000000006"
            }),
        )
        .await;
    assert_eq!(
        refused.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "a purchase may not subtract from what customers owe"
    );

    // The debt account is untouched by the refused purchase.
    assert_eq!(
        balance_of(&client, "00000000-0000-0000-0000-000000000005").await,
        "0.00"
    );
}

#[sqlx::test]
async fn v3_t5_a_credit_note_inherits_the_sales_method_by_default(pool: sqlx::PgPool) {
    // An unpaid sale has no method to inherit, so its note lowers the obligation
    // without making a cash movement.
    let client = common::client(pool.clone());
    let cash = "00000000-0000-0000-0000-000000000003";

    let customer = client
        .post("/api/customers", serde_json::json!({ "name": "T5 Fiado" }))
        .await;
    let customer_id = customer.body["id"]
        .as_str()
        .expect("customer id")
        .to_string();
    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "t5-inherit", "list_price": "90.00" }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("id").to_string();

    let sale = client
        .post(
            "/api/sales",
            serde_json::json!({
                "customer_id": customer_id,
                "items": [{ "product_id": product_id, "quantity": 1 }],
                "payment_method_id": "on_account"
            }),
        )
        .await;
    assert_eq!(sale.status(), axum::http::StatusCode::CREATED);
    let sale_id: uuid::Uuid = sale.body["id"]
        .as_str()
        .expect("sale id")
        .parse()
        .expect("sale UUID");
    let sale_id_path = sale_id.to_string();
    let item_id = sale.body["items"][0]["id"]
        .as_str()
        .expect("item id")
        .to_string();

    // **`outstanding_of`, not a local query** (`R2-003`). This test used to derive the figure
    // itself with a `LEFT JOIN`, which is a second definition of "outstanding" — and it was
    // already the weaker one: it subtracted adjustments only, so the day a settlement lowered
    // the same debt this assertion and the helper would have disagreed about the same number.
    assert_eq!(outstanding_of(&pool, &sale_id_path).await, "90");

    // With no method named, the note inherits NULL and lowers the obligation without
    // paying out cash.
    let note = client
        .post(
            &format!("/api/sales/{sale_id_path}/credit-notes"),
            serde_json::json!({ "items": [{ "sale_item_id": item_id, "quantity": 1 }] }),
        )
        .await;
    assert_eq!(note.status(), axum::http::StatusCode::CREATED);

    assert_eq!(
        outstanding_of(&pool, &sale_id_path).await,
        "0",
        "the refund lowered the debt"
    );
    assert_eq!(balance_of(&client, cash).await, "0.00", "no money left the till");

    let note_id: uuid::Uuid = note.body["id"].as_str().expect("note id").parse().expect("note UUID");
    let note_method_is_null: bool = sqlx::query_scalar(
        "SELECT payment_method_id IS NULL FROM credit_notes WHERE id = $1",
    )
    .bind(note_id)
    .fetch_one(&pool)
    .await
    .expect("read note method");
    assert!(note_method_is_null, "the note inherits the sale's NULL method");
}

#[sqlx::test]
async fn v3_t5_a_credit_note_may_name_its_own_method_but_never_the_debt(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());
    // The **account** id, for reading balances.
    let cash_account = "00000000-0000-0000-0000-000000000003";
    // The **method** id. The note names a method, never an account — passing the
    // account id here returned 404 and cost a debugging round, which is the whole
    // reason the two names are separated in this test.
    let cash_method = "00000000-0000-0000-0000-000000000004";

    let customer = client
        .post(
            "/api/customers",
            serde_json::json!({ "name": "T5 Override" }),
        )
        .await;
    let customer_id = customer.body["id"].as_str().expect("id").to_string();
    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "t5-override", "list_price": "50.00" }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("id").to_string();

    let sale = client
        .post(
            "/api/sales",
            serde_json::json!({
                "customer_id": customer_id,
                "items": [{ "product_id": product_id, "quantity": 1 }],
                "payment_method_id": "on_account"
            }),
        )
        .await;
    assert_eq!(sale.status(), axum::http::StatusCode::CREATED);
    let sale_id = sale.body["id"].as_str().expect("sale id").to_string();
    let item_id = sale.body["items"][0]["id"]
        .as_str()
        .expect("item id")
        .to_string();

    // **The override, refused, and this assertion inverted when T6.4 landed.** Until this
    // slice the explicit cash method was accepted on an unpaid sale and took 50.00 out of
    // the till — measured as `v3-accounts.md` L107. A sale that never brought money in
    // cannot pay money back, so the guard is now the rule and the pin below states it.
    let overridden = client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({
                "items": [{ "sale_item_id": item_id, "quantity": 1 }],
                "payment_method_id": cash_method
            }),
        )
        .await;
    assert_eq!(
        overridden.status(),
        axum::http::StatusCode::CONFLICT,
        "a refund through a rail nobody paid into is refused; body: {}",
        overridden.text
    );
    assert_eq!(
        balance_of(&client, cash_account).await,
        "0.00",
        "and the till kept what it had, which is the number L107 measured going negative"
    );
    // 2. The override that must be refused: a note may not target the debt account.
    //    Moving debt between customers is a different document.
    let second_sale = client
        .post(
            "/api/sales",
            serde_json::json!({
                "customer_id": customer_id,
                "items": [{ "product_id": product_id, "quantity": 1 }],
                "payment_method_id": "on_account"
            }),
        )
        .await;
    assert_eq!(
        second_sale.status(),
        axum::http::StatusCode::CREATED,
        "the second sale must exist before its refund is attempted; body: {}",
        second_sale.body
    );
    let second_id = second_sale.body["id"]
        .as_str()
        .expect("sale id")
        .to_string();
    let second_item = second_sale.body["items"][0]["id"]
        .as_str()
        .expect("item")
        .to_string();

    let refused = client
        .post(
            &format!("/api/sales/{second_id}/credit-notes"),
            serde_json::json!({
                "items": [{ "sale_item_id": second_item, "quantity": 1 }],
                "payment_method_id": "00000000-0000-0000-0000-000000000006"
            }),
        )
        .await;
    assert_eq!(
        refused.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "an explicit credit method on a note would move debt, not reduce it"
    );
}

#[sqlx::test]
async fn v3_t5_a_supplier_return_inherits_the_purchases_method(pool: sqlx::PgPool) {
    // The mirror of the credit note. The supplier gives money back through the same
    // rail that paid them, so a purchase paid from the bank returns to the bank.
    let client = common::client(pool.clone());
    let cash = "00000000-0000-0000-0000-000000000003";

    let account = client
        .post("/api/accounts", serde_json::json!({ "name": "banco-dev" }))
        .await;
    let account_id = account.body["id"].as_str().expect("id").to_string();
    let method = client
        .post(
            "/api/payment-methods",
            serde_json::json!({ "name": "transferencia-dev", "account_id": account_id }),
        )
        .await;
    let method_id = method.body["id"].as_str().expect("id").to_string();

    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "t5-return", "list_price": "15.00" }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("id").to_string();

    let purchase = client
        .post(
            "/api/purchases",
            serde_json::json!({
                "items": [{ "product_id": product_id, "quantity": 4 }],
                "payment_method_id": method_id
            }),
        )
        .await;
    let purchase_id = purchase.body["purchase"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    let item_id = purchase.body["items"][0]["id"]
        .as_str()
        .expect("item")
        .to_string();
    assert_eq!(balance_of(&client, &account_id).await, "-60.00");
    assert_eq!(balance_of(&client, cash).await, "0.00");

    client
        .post(
            &format!("/api/purchases/{purchase_id}/supplier-returns"),
            serde_json::json!({ "items": [{ "purchase_item_id": item_id, "quantity": 1 }] }),
        )
        .await;

    // The return credited the same account the purchase debited — not the till.
    assert_eq!(balance_of(&client, &account_id).await, "-45.00");
    assert_eq!(
        balance_of(&client, cash).await,
        "0.00",
        "a return to the bank must not appear in the till"
    );
}

// ── v3 T6: `/api/cash` is retired; the till is the default account ──────────

#[sqlx::test]
async fn v3_t6_the_cash_endpoint_is_gone(pool: sqlx::PgPool) {
    // Retiring an endpoint is a contract change, and the test says so rather than
    // leaving a stale caller to discover it: a request to the old path must not
    // answer with a balance.
    let client = common::client(pool.clone());
    let gone = client.get("/api/cash").await;
    assert_eq!(
        gone.status(),
        axum::http::StatusCode::NOT_FOUND,
        "/api/cash was retired in T6; its figure now lives on /api/accounts"
    );
}

#[sqlx::test]
async fn v3_t6_the_till_is_the_default_account_not_every_sale(pool: sqlx::PgPool) {
    // **The behaviour this task exists to change.** Before v3, a footer labelled
    // "cash on hand" summed every confirmed sale and nothing else. Once a sale can
    // name a bank transfer, that label is a lie: the sale happened, the money never
    // reached the till.
    let client = common::client(pool.clone());
    let till = "00000000-0000-0000-0000-000000000003";

    let account = client
        .post(
            "/api/accounts",
            serde_json::json!({ "name": "banco-cobros" }),
        )
        .await;
    let account_id = account.body["id"].as_str().expect("account id").to_string();
    let method = client
        .post(
            "/api/payment-methods",
            serde_json::json!({ "name": "cobro-transf", "account_id": account_id }),
        )
        .await;
    let method_id = method.body["id"].as_str().expect("method id").to_string();

    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "t6-probe", "list_price": "100.00" }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("product id").to_string();

    // One sale into the till, one into the bank.
    client
        .post(
            "/api/sales",
            serde_json::json!({ "items": [{ "product_id": product_id, "quantity": 1 }] }),
        )
        .await;
    client
        .post(
            "/api/sales",
            serde_json::json!({
                "items": [{ "product_id": product_id, "quantity": 1 }],
                "payment_method_id": method_id
            }),
        )
        .await;

    // The till holds the first only; the bank holds the second.
    assert_eq!(
        balance_of(&client, till).await,
        "100.00",
        "the till must count only the money that reached it"
    );
    assert_eq!(balance_of(&client, &account_id).await, "100.00");

    // And the footer's new source agrees with the till, not with "every sale".
    let response = client.get("/api/accounts").await;
    let accounts = response.body.as_array().expect("accounts array");
    let defaults: Vec<&str> = accounts
        .iter()
        .filter(|a| a["is_default"].as_bool().unwrap_or(false))
        .map(|a| a["name"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(
        defaults,
        vec!["cash"],
        "exactly one account is the till, so the footer has one number to show"
    );
}

// ── v4 S1: the money book is a table, not a view ────────────────────────────
//
// `odd/tasks/v4-settlement.md` S1-S3. Until `0017` the only way to know where money
// moved was `money_movements`, a `VIEW`: a calculation over four document tables with
// an `INNER JOIN` to `payment_methods`, so a misaligned row vanished from every balance
// without an error and nothing recorded what should have been there.
//
// What is asserted here is the property that makes auditing possible: the effect of a
// document is a **row that exists**, and it cannot be rewritten.

#[sqlx::test]
async fn v4_the_book_is_a_table_and_refuses_every_rewrite(pool: sqlx::PgPool) {
    // S1/S2. Measured before `0017`: `money_entries` did not exist at all, so the book
    // was a calculation and there was nothing to make append-only.
    let kind: String = sqlx::query_scalar(
        "SELECT table_type FROM information_schema.tables WHERE table_name = 'money_entries'",
    )
    .fetch_one(&pool)
    .await
    .expect("money_entries must exist as a real table");
    assert_eq!(
        kind, "BASE TABLE",
        "the book is a table: a view is a calculation, and a lost UNION branch leaves no \
         row behind to notice"
    );

    // Give it one entry to work with.
    let client = common::client(pool.clone());
    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "book-probe", "list_price": "10.00" }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("product id").to_string();
    let sale = client
        .post(
            "/api/sales",
            serde_json::json!({ "items": [{ "product_id": product_id, "quantity": 1 }] }),
        )
        .await;
    assert_eq!(sale.status(), axum::http::StatusCode::CREATED);
    let sale_id = sale.body["id"].as_str().expect("sale id").to_string();

    let entry: (uuid::Uuid, uuid::Uuid, rust_decimal::Decimal, String) = sqlx::query_as(
        "SELECT id, account_id, amount, document_kind FROM money_entries WHERE document_id = $1",
    )
    .bind(uuid::Uuid::parse_str(&sale_id).expect("uuid"))
    .fetch_one(&pool)
    .await
    .expect("a sale must write exactly one money entry");

    assert_eq!(
        entry.3, "sale",
        "the entry names the document kind it came from"
    );
    assert_eq!(
        entry.2,
        rust_decimal::Decimal::from(10),
        "positive is money entering the account"
    );

    // S2: an entry is the effect of a document, never a document of its own.
    let orphan = sqlx::query(
        "INSERT INTO money_entries (account_id, payment_method_id, document_kind, document_id, amount, occurred_at, created_by)
         VALUES ('00000000-0000-0000-0000-000000000003', '00000000-0000-0000-0000-000000000004',
                 'sale', NULL, 5, now(), 'test')",
    )
    .execute(&pool)
    .await;
    assert!(
        orphan.is_err(),
        "an entry with no document_id is a bug, not a document; the column is NOT NULL"
    );

    // The three rewrite paths `0004` had to close for stock, closed here from the start.
    let update = sqlx::query("UPDATE money_entries SET amount = 999 WHERE id = $1")
        .bind(entry.0)
        .execute(&pool)
        .await;
    assert!(
        update.is_err(),
        "the book is append-only: corrections are compensating rows"
    );

    let delete = sqlx::query("DELETE FROM money_entries WHERE id = $1")
        .bind(entry.0)
        .execute(&pool)
        .await;
    assert!(
        delete.is_err(),
        "an entry may not be deleted: that erases a balance's explanation"
    );

    let truncate = sqlx::query("TRUNCATE money_entries").execute(&pool).await;
    assert!(
        truncate.is_err(),
        "TRUNCATE does not fire FOR EACH ROW, so it needs its own statement-level guard"
    );

    // And the row is still there, untouched by any of the refusals.
    let still: rust_decimal::Decimal =
        sqlx::query_scalar("SELECT amount FROM money_entries WHERE id = $1")
            .bind(entry.0)
            .fetch_one(&pool)
            .await
            .expect("the entry survived three refused rewrites");
    assert_eq!(still, rust_decimal::Decimal::from(10));
}

#[sqlx::test]
async fn v4_every_document_writes_its_own_entries_and_the_view_agrees(pool: sqlx::PgPool) {
    // S1's other half: the book is the source and `money_movements` is now a view over
    // it, so the four existing write paths must each write their entries — and the view
    // must still total the same as the table. A view that disagrees with its own table is
    // the defect this slice exists to make visible.
    let client = common::client(pool.clone());
    let cash = "00000000-0000-0000-0000-000000000003";

    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "ledger-probe", "list_price": "100.00", "initial_stock": 20 }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("product id").to_string();

    // 1. A sale: money in.
    let sale = client
        .post(
            "/api/sales",
            serde_json::json!({ "items": [{ "product_id": product_id, "quantity": 2 }] }),
        )
        .await;
    assert_eq!(sale.status(), axum::http::StatusCode::CREATED);
    let sale_id = sale.body["id"].as_str().expect("sale id").to_string();
    let sale_item = sale.body["items"][0]["id"]
        .as_str()
        .expect("item")
        .to_string();

    // 2. A purchase: money out.
    let purchase = client
        .post(
            "/api/purchases",
            serde_json::json!({
                "items": [{ "product_id": product_id, "quantity": 5, "unit_price": "20.00" }]
            }),
        )
        .await;
    assert_eq!(
        purchase.status(),
        axum::http::StatusCode::CREATED,
        "a purchase must be recorded; body was: {}",
        purchase.text
    );
    // The purchase response nests the document under `purchase` and its lines beside it,
    // unlike a sale which returns the document flat. Measured, not assumed.
    let purchase_id = purchase.body["purchase"]["id"]
        .as_str()
        .expect("purchase id")
        .to_string();
    let purchase_item = purchase.body["items"][0]["id"]
        .as_str()
        .expect("purchase item id")
        .to_string();

    // 3. A credit note: money back to the customer.
    let note = client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({ "items": [{ "sale_item_id": sale_item, "quantity": 1 }] }),
        )
        .await;
    assert_eq!(note.status(), axum::http::StatusCode::CREATED);
    let note_id = note.body["id"].as_str().expect("note id").to_string();

    // 4. A supplier return: money back from the supplier.
    let returned = client
        .post(
            &format!("/api/purchases/{purchase_id}/supplier-returns"),
            serde_json::json!({ "items": [{ "purchase_item_id": purchase_item, "quantity": 1 }] }),
        )
        .await;
    assert_eq!(
        returned.status(),
        axum::http::StatusCode::CREATED,
        "a supplier return must be recorded; body was: {}",
        returned.text
    );
    // Measured, not assumed: the supplier return nests the document under `return`,
    // unlike the purchase's `purchase`. Guessing this twice cost two red runs.
    let return_id = returned.body["return"]["id"]
        .as_str()
        .expect("supplier return id")
        .to_string();

    // Each document names its own kind, once. `assert_eq!` on the counts is what makes a
    // missing insert a failure instead of a silently smaller balance.
    for (kind, id) in [
        ("sale", &sale_id),
        ("purchase", &purchase_id),
        ("credit_note", &note_id),
        ("supplier_return", &return_id),
    ] {
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM money_entries WHERE document_kind = $1")
                .bind(kind)
                .fetch_one(&pool)
                .await
                .expect("count entries");
        assert_eq!(count, 1, "{kind} must write exactly one money entry");

        let by_id: i64 =
            sqlx::query_scalar("SELECT count(*) FROM money_entries WHERE document_id = $1")
                .bind(uuid::Uuid::parse_str(id).expect("uuid"))
                .fetch_one(&pool)
                .await
                .expect("count by document id");
        assert_eq!(
            by_id, 1,
            "{kind} {id} must be the documented cause of that entry"
        );
    }

    // The view and the table agree, per account. A view that drifted from its own source
    // would make every balance wrong while every document looked right.
    let from_view: Vec<(uuid::Uuid, rust_decimal::Decimal)> =
        sqlx::query_as("SELECT account_id, COALESCE(SUM(amount), 0) FROM money_movements GROUP BY account_id ORDER BY account_id")
            .fetch_all(&pool)
            .await
            .expect("read the view");
    let from_table: Vec<(uuid::Uuid, rust_decimal::Decimal)> =
        sqlx::query_as("SELECT account_id, COALESCE(SUM(amount), 0) FROM money_entries GROUP BY account_id ORDER BY account_id")
            .fetch_all(&pool)
            .await
            .expect("read the table");
    assert_eq!(
        from_view, from_table,
        "money_movements is a view over money_entries, so the two can never disagree"
    );

    // 2 x 100 sold (+200), 5 x 20 bought (-100), 1 x 100 refunded (-100),
    // 1 x 20 returned (+20) = 20. The refund covers one unit, not the whole sale.
    assert_eq!(
        balance_of(&client, cash).await,
        "20.00",
        "the view totals the four documents with the sign each one has"
    );
}

// ── v4 S3: money that did not come from a sale ──────────────────────────────
//
// `odd/tasks/v4-settlement.md` S12. Three ordinary events had no document:
// the opening balance (so the book's starting point was the first sale rather than
// the first morning), a transfer between the shop's own accounts (today expressible
// only as a fake sale), and a count adjustment (a real difference found by counting).
//
// Each is a document with its own id, and each writes its entries through the book.

#[sqlx::test]
async fn v4_an_opening_balance_starts_the_book_and_cannot_be_repeated(pool: sqlx::PgPool) {
    // The till's opening figure. Before `0018` there was no way to record it: the book's
    // first row was the first sale, and the 3,000 the development database shows had no
    // opening document behind it.
    let client = common::client(pool.clone());
    let cash = "00000000-0000-0000-0000-000000000003";

    let opened = client
        .post(
            "/api/opening-balances",
            serde_json::json!({ "account_id": cash, "amount": "500.00" }),
        )
        .await;
    assert_eq!(
        opened.status(),
        axum::http::StatusCode::CREATED,
        "an opening balance must be recordable; body was: {}",
        opened.text
    );

    // It moves the balance, because it is money that was already there.
    assert_eq!(
        balance_of(&client, cash).await,
        "500.00",
        "the opening balance is the first row of the book"
    );

    // And the kind is named, so "where did this money come from" has an answer.
    let kind: String = sqlx::query_scalar(
        "SELECT document_kind FROM money_entries WHERE document_kind = 'opening_balance'",
    )
    .fetch_one(&pool)
    .await
    .expect("the opening balance writes a book entry");
    assert_eq!(kind, "opening_balance");

    // **A second opening for the same account is refused.** It is not a second
    // beginning, it is a correction wearing a beginning's clothes.
    let again = client
        .post(
            "/api/opening-balances",
            serde_json::json!({ "account_id": cash, "amount": "100.00" }),
        )
        .await;
    assert_eq!(
        again.status(),
        axum::http::StatusCode::CONFLICT,
        "an account has one beginning, not two; body was: {}",
        again.text
    );

    // A different account is a different beginning — **but only once it has a method**.
    //
    // This is the same shape as `0012`'s "the default account always has an active
    // method", and it is not an obstacle invented here: the book derives `account_id`
    // from the method (`0017`), so an account with no method cannot be written to at all.
    // The refusal is a 404 because what is missing is the method, not the account.
    let bank = client
        .post("/api/accounts", serde_json::json!({ "name": "banco" }))
        .await;
    assert_eq!(bank.status(), axum::http::StatusCode::CREATED);
    let bank_id = bank.body["id"].as_str().expect("bank id").to_string();

    let no_method_yet = client
        .post(
            "/api/opening-balances",
            serde_json::json!({ "account_id": bank_id, "amount": "900.00" }),
        )
        .await;
    assert_eq!(
        no_method_yet.status(),
        axum::http::StatusCode::NOT_FOUND,
        "an account with no payment method cannot carry money: the book derives the \
         account from a method. body was: {}",
        no_method_yet.text
    );

    // Give it a method, and now it can start.
    let bank_method = client
        .post(
            "/api/payment-methods",
            serde_json::json!({ "name": "transferencia-banco", "account_id": bank_id }),
        )
        .await;
    assert_eq!(bank_method.status(), axum::http::StatusCode::CREATED);

    let bank_open = client
        .post(
            "/api/opening-balances",
            serde_json::json!({ "account_id": bank_id, "amount": "900.00" }),
        )
        .await;
    assert_eq!(
        bank_open.status(),
        axum::http::StatusCode::CREATED,
        "every account may have its own beginning; body was: {}",
        bank_open.text
    );
}

#[sqlx::test]
async fn v4_a_transfer_moves_money_between_two_accounts_and_nowhere_else(pool: sqlx::PgPool) {
    // A transfer leaves one account and arrives at another, in one document, with two
    // entries. Today the only way to express it is a fake sale, which pollutes the
    // sales ledger with a document that never sold anything.
    let client = common::client(pool.clone());
    let cash = "00000000-0000-0000-0000-000000000003";

    let bank = client
        .post("/api/accounts", serde_json::json!({ "name": "banco" }))
        .await;
    let bank_id = bank.body["id"].as_str().expect("bank id").to_string();
    let bank_method = client
        .post(
            "/api/payment-methods",
            serde_json::json!({ "name": "transferencia", "account_id": bank_id }),
        )
        .await;
    let bank_method_id = bank_method.body["id"].as_str().expect("method").to_string();

    client
        .post(
            "/api/opening-balances",
            serde_json::json!({ "account_id": cash, "amount": "1000.00" }),
        )
        .await;
    assert_eq!(balance_of(&client, cash).await, "1000.00");

    let moved = client
        .post(
            "/api/transfers",
            serde_json::json!({
                "from_account_id": cash,
                "to_account_id": bank_id,
                "from_payment_method_id": "00000000-0000-0000-0000-000000000004",
                "to_payment_method_id": bank_method_id,
                "amount": "300.00"
            }),
        )
        .await;
    assert_eq!(
        moved.status(),
        axum::http::StatusCode::CREATED,
        "a transfer between the shop's own accounts must be recordable; body was: {}",
        moved.text
    );

    // The point: the total across the shop is unchanged, and each account moved.
    assert_eq!(
        balance_of(&client, cash).await,
        "700.00",
        "the source account lost exactly what left it"
    );
    assert_eq!(
        balance_of(&client, &bank_id).await,
        "300.00",
        "the destination gained exactly what arrived"
    );

    let entries: i64 =
        sqlx::query_scalar("SELECT count(*) FROM money_entries WHERE document_kind = 'transfer'")
            .fetch_one(&pool)
            .await
            .expect("count transfer entries");
    assert_eq!(entries, 2, "a transfer is two effects, not one");

    let net: rust_decimal::Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount), 0) FROM money_entries WHERE document_kind = 'transfer'",
    )
    .fetch_one(&pool)
    .await
    .expect("sum transfer entries");
    assert_eq!(
        net,
        rust_decimal::Decimal::ZERO,
        "a transfer moves money; it does not create or destroy it"
    );

    // To the account it is already in is not a transfer.
    let same = client
        .post(
            "/api/transfers",
            serde_json::json!({
                "from_account_id": cash,
                "to_account_id": cash,
                "from_payment_method_id": "00000000-0000-0000-0000-000000000004",
                "to_payment_method_id": "00000000-0000-0000-0000-000000000004",
                "amount": "10.00"
            }),
        )
        .await;
    assert!(
        same.status().is_client_error(),
        "a transfer to the same account is a typo, not a document; got {}",
        same.status()
    );
}

#[sqlx::test]
async fn v4_a_count_adjustment_records_a_real_difference_without_rewriting_history(
    pool: sqlx::PgPool,
) {
    // "I counted 2,850 and the book says 3,000." The answer is a document, never a
    // rewritten balance — and this is the test that pins it, because a system that can
    // silently adjust a balance cannot be audited afterwards.
    let client = common::client(pool.clone());
    let cash = "00000000-0000-0000-0000-000000000003";

    client
        .post(
            "/api/opening-balances",
            serde_json::json!({ "account_id": cash, "amount": "3000.00" }),
        )
        .await;
    assert_eq!(balance_of(&client, cash).await, "3000.00");

    let adjusted = client
        .post(
            "/api/count-adjustments",
            serde_json::json!({
                "account_id": cash,
                "payment_method_id": "00000000-0000-0000-0000-000000000004",
                "difference": "-150.00"
            }),
        )
        .await;
    assert_eq!(
        adjusted.status(),
        axum::http::StatusCode::CREATED,
        "a count adjustment must be recordable; body was: {}",
        adjusted.text
    );

    assert_eq!(
        balance_of(&client, cash).await,
        "2850.00",
        "the counting found less money than the book claimed, and the book now says so"
    );

    // The opening balance is untouched: the correction is a new row, not an edit.
    let opening: rust_decimal::Decimal = sqlx::query_scalar(
        "SELECT amount FROM money_entries WHERE document_kind = 'opening_balance'",
    )
    .fetch_one(&pool)
    .await
    .expect("the opening balance still exists, unchanged");
    assert_eq!(
        opening,
        rust_decimal::Decimal::from(3000),
        "an adjustment adds a row; it never rewrites the history it corrected"
    );

    // An adjustment of zero is not a document.
    let pointless = client
        .post(
            "/api/count-adjustments",
            serde_json::json!({
                "account_id": cash,
                "payment_method_id": "00000000-0000-0000-0000-000000000004",
                "difference": "0"
            }),
        )
        .await;
    assert!(
        pointless.status().is_client_error(),
        "counting nothing different is not a document; got {}",
        pointless.status()
    );
}

// ── v4 S4: the reversal budget lives on the sale, not on the credit-note table ──
//
// `odd/tasks/v4-settlement.md` S9. Until `0019` the rule "a line may not be reversed
// more than it was sold" lived in **two places that could disagree**: the handler
// computed `SUM(credit_note_items.quantity)` inside the sale's row lock, and the schema
// enforced nothing at all.
//
// Measured before `0019`: a `psql` session inserted 5 units against a line of 1 and
// nothing objected. That is the hole this slice closes, and it closes it in the schema —
// because the annulment (`S10`) will also reverse stock, and two documents consuming one
// ledger-side `SUM` is how a line gets reversed twice.

#[sqlx::test]
async fn v4_the_reversal_budget_lives_on_the_sale(pool: sqlx::PgPool) {
    // S9's shape, asserted directly: the budget is a column, and it starts at zero.
    let column: Option<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns
          WHERE table_name = 'sale_items' AND column_name = 'reversed_quantity'",
    )
    .fetch_optional(&pool)
    .await
    .expect("read the column");
    assert_eq!(
        column.as_deref(),
        Some("reversed_quantity"),
        "the budget belongs to the resource two documents act on, not to one of them"
    );

    let client = common::client(pool.clone());
    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "budget-probe", "list_price": "10.00", "initial_stock": 10 }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("product id").to_string();
    let sale = client
        .post(
            "/api/sales",
            serde_json::json!({ "items": [{ "product_id": product_id, "quantity": 3 }] }),
        )
        .await;
    assert_eq!(sale.status(), axum::http::StatusCode::CREATED);
    let sale_id = sale.body["id"].as_str().expect("sale id").to_string();
    let item_id = sale.body["items"][0]["id"]
        .as_str()
        .expect("item")
        .to_string();

    let start: i32 = sqlx::query_scalar("SELECT reversed_quantity FROM sale_items WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&item_id).expect("uuid"))
        .fetch_one(&pool)
        .await
        .expect("read the budget");
    assert_eq!(start, 0, "a fresh line has reversed nothing");

    // The handler still refuses to over-reverse — that was already true.
    let note = client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({ "items": [{ "sale_item_id": item_id, "quantity": 2 }] }),
        )
        .await;
    assert_eq!(note.status(), axum::http::StatusCode::CREATED);

    // **And now the budget moved, which is what makes the rule one rule.** The handler
    // consumes it rather than recomputing a ledger-side sum.
    let after: i32 = sqlx::query_scalar("SELECT reversed_quantity FROM sale_items WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&item_id).expect("uuid"))
        .fetch_one(&pool)
        .await
        .expect("read the budget");
    assert_eq!(after, 2, "the credit note consumed two of the three sold");

    // One left, so one more is allowed and two more are not.
    let last = client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({ "items": [{ "sale_item_id": item_id, "quantity": 1 }] }),
        )
        .await;
    assert_eq!(
        last.status(),
        axum::http::StatusCode::CREATED,
        "the last unit is still refundable; body was: {}",
        last.text
    );

    let over = client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({ "items": [{ "sale_item_id": item_id, "quantity": 1 }] }),
        )
        .await;
    assert_eq!(
        over.status(),
        axum::http::StatusCode::CONFLICT,
        "a line may not be reversed more than it was sold; body was: {}",
        over.text
    );
}

#[sqlx::test]
async fn v4_the_schema_refuses_an_override_reversal_even_without_the_handler(pool: sqlx::PgPool) {
    // The measured hole: before `0019` this exact statement succeeded through `psql`.
    // It has to fail now, because the annulment is a **second** document that reverses
    // stock, and two documents sharing one ledger-side sum is how a line gets reversed
    // twice. A guard that only the handler applies is a guard one route can forget.
    let client = common::client(pool.clone());
    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "schema-probe", "list_price": "10.00", "initial_stock": 5 }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("product id").to_string();
    let sale = client
        .post(
            "/api/sales",
            serde_json::json!({ "items": [{ "product_id": product_id, "quantity": 1 }] }),
        )
        .await;
    let sale_id = sale.body["id"].as_str().expect("sale id").to_string();
    let item_id = sale.body["items"][0]["id"]
        .as_str()
        .expect("item")
        .to_string();
    let note = client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({ "items": [{ "sale_item_id": item_id, "quantity": 1 }] }),
        )
        .await;
    assert_eq!(note.status(), axum::http::StatusCode::CREATED);
    let note_id = note.body["id"].as_str().expect("note id").to_string();

    // The line is fully reversed. A second item naming it must be refused by the schema.
    let bypass = sqlx::query(
        "INSERT INTO credit_note_items
            (credit_note_id, sale_item_id, product_id, product_name, quantity, unit_price, line_total, unit)
         SELECT $1, si.id, si.product_id, si.product_name, 1, si.unit_price, si.unit_price, si.unit
           FROM sale_items si WHERE si.id = $2",
    )
    .bind(uuid::Uuid::parse_str(&note_id).expect("uuid"))
    .bind(uuid::Uuid::parse_str(&item_id).expect("uuid"))
    .execute(&pool)
    .await;

    // **Two more holes the first version of the guard left, both found by review and both
    // reproduced before the fix.** They share a root: the narrowed guard asked "did only
    // `reversed_quantity` change?" and never asked who was asking or what else moved.
    let item_uuid = uuid::Uuid::parse_str(&item_id).expect("uuid");

    // `R3-001`: an `UPDATE` that rewrote only the primary key fell through the equality
    // list, because `id` was not in it.
    let id_rewrite = sqlx::query("UPDATE sale_items SET id = gen_random_uuid() WHERE id = $1")
        .bind(item_uuid)
        .execute(&pool)
        .await;
    assert!(
        id_rewrite.is_err(),
        "a sale line's primary key is history like every other column: the guard must \
         compare `id` too. Measured before the fix: UPDATE 1."
    );

    // `R3-002`: the counter could be inflated with no credit note at all, and the handler
    // — which reads it — would then refuse a refund that was legitimate.
    //
    // **This line is already fully reversed, so the statement has to be a real change to
    // mean anything.** Written first as `SET reversed_quantity = quantity` it was a no-op
    // on this row and the guard — which permits no-ops, as it must — let it through, and
    // the test failed for the wrong reason. Asserting on a *fresh* line is the version
    // that measures the hole.
    let fresh = client
        .post(
            "/api/sales",
            serde_json::json!({ "items": [{ "product_id": product_id, "quantity": 2 }] }),
        )
        .await;
    let fresh_item = fresh.body["items"][0]["id"]
        .as_str()
        .expect("item")
        .to_string();
    let inflate = sqlx::query("UPDATE sale_items SET reversed_quantity = quantity WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&fresh_item).expect("uuid"))
        .execute(&pool)
        .await;
    assert!(
        inflate.is_err(),
        "the budget may only move through the refund path; a bare UPDATE is refused. \
         Measured before the fix: the counter went 0 -> 2 with no note on record."
    );

    // **Asserted by the message, not merely by the refusal.** Two guards can refuse this
    // statement — the consumer that counts what is left, and the budget guard that refuses
    // a counter above the quantity sold — and a bare `is_err()` passes when either one is
    // present. Measured while writing this: neutralising the consumer's `RAISE` left the
    // test green, because its `UPDATE` still ran and the *other* guard answered. Pinning
    // the message is what makes the assertion belong to one guard, the same lesson
    // `v3_the_default_account_always_keeps_an_active_method` records.
    let message = bypass
        .as_ref()
        .err()
        .and_then(|e| e.as_database_error())
        .map(|e| e.message().to_string())
        .unwrap_or_default();
    assert!(
        message.contains("left to reverse"),
        "the refusal must come from the consumer that counts what remains, got: {message}. \
         Measured before `0019`: this inserted 5 units against a line of 1 and nothing \
         objected."
    );
}

// ── v4 S5: a sale reported wrongly can be annulled, not "returned" ──────────
//
// `odd/tasks/v4-settlement.md` S10, S11. Measured before this slice: `sales` has no
// `status` and no `reversed_at` (v3 dropped them), the only routes are create, list and
// `POST /sales/{id}/credit-notes`, and there is **no way to correct a sale that was
// loaded wrong**.
//
// The only exit today is a full credit note, which means "the customer returned the
// goods" — so the stock moves as if merchandise came back, and it never did. That is
// worse than the original mistake, and it was already happening.

#[sqlx::test]
async fn v4_an_annulment_reverses_a_sale_without_pretending_goods_came_back(pool: sqlx::PgPool) {
    let client = common::client(pool.clone());
    let cash = "00000000-0000-0000-0000-000000000003";

    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "annul-probe", "list_price": "50.00", "initial_stock": 10 }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("product id").to_string();

    let sale = client
        .post(
            "/api/sales",
            serde_json::json!({ "items": [{ "product_id": product_id, "quantity": 2 }] }),
        )
        .await;
    assert_eq!(sale.status(), axum::http::StatusCode::CREATED);
    let sale_id = sale.body["id"].as_str().expect("sale id").to_string();
    assert_eq!(balance_of(&client, cash).await, "100.00");

    let annulled = client
        .post(
            &format!("/api/sales/{sale_id}/annulments"),
            serde_json::json!({ "reason": "wrong_amount" }),
        )
        .await;
    assert_eq!(
        annulled.status(),
        axum::http::StatusCode::CREATED,
        "a sale reported wrongly must be correctable; body was: {}",
        annulled.text
    );

    // The money comes back out, and the sale itself is untouched.
    assert_eq!(
        balance_of(&client, cash).await,
        "0.00",
        "the annulment reverses what the sale moved"
    );

    // **The stock comes back, and that is the decision (`S10`): always.**
    let recovered = client.get("/api/products").await;
    let row = recovered
        .body
        .as_array()
        .expect("array")
        .iter()
        .find(|p| p["id"].as_str() == Some(&product_id))
        .expect("the product");
    assert_eq!(
        row["stock"], 10,
        "an annulled sale returns the stock: the goods never left"
    );

    // The document is its own thing, with its own reason — not a credit note.
    let kind: String = sqlx::query_scalar(
        "SELECT document_kind FROM money_entries WHERE document_kind = 'annulment'",
    )
    .fetch_one(&pool)
    .await
    .expect("the annulment writes its own book entry");
    assert_eq!(kind, "annulment");

    let notes: i64 = sqlx::query_scalar("SELECT count(*) FROM credit_notes WHERE sale_id = $1")
        .bind(uuid::Uuid::parse_str(&sale_id).expect("uuid"))
        .fetch_one(&pool)
        .await
        .expect("count notes");
    assert_eq!(
        notes, 0,
        "an annulment is not a refund: no credit note is invented for it"
    );

    // A sale is annulled once. A second annulment is not a second correction.
    let again = client
        .post(
            &format!("/api/sales/{sale_id}/annulments"),
            serde_json::json!({ "reason": "wrong_customer" }),
        )
        .await;
    assert!(
        again.status().is_client_error(),
        "a sale cannot be annulled twice; got {}",
        again.status()
    );
}

#[sqlx::test]
async fn v4_an_annulment_and_a_credit_note_cannot_reverse_the_same_unit(pool: sqlx::PgPool) {
    // `S9`'s whole reason for existing: two documents consume one budget. Before `0019`
    // the budget was a `SUM` over `credit_note_items`, so an annulment that also reversed
    // stock would not have counted there — and the same line could be reversed twice.
    let client = common::client(pool.clone());
    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "budget-share", "list_price": "10.00", "initial_stock": 5 }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("product id").to_string();
    let sale = client
        .post(
            "/api/sales",
            serde_json::json!({ "items": [{ "product_id": product_id, "quantity": 3 }] }),
        )
        .await;
    let sale_id = sale.body["id"].as_str().expect("sale id").to_string();
    let item_id = sale.body["items"][0]["id"]
        .as_str()
        .expect("item")
        .to_string();

    // Refund one unit of three.
    let note = client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({ "items": [{ "sale_item_id": item_id, "quantity": 1 }] }),
        )
        .await;
    assert_eq!(note.status(), axum::http::StatusCode::CREATED);

    // Annulling now would reverse the other two, and the budget guard has to see that the
    // credit note already took one. This asserts the two documents share one counter.
    let annulled = client
        .post(
            &format!("/api/sales/{sale_id}/annulments"),
            serde_json::json!({ "reason": "wrong_document" }),
        )
        .await;
    assert_eq!(
        annulled.status(),
        axum::http::StatusCode::CREATED,
        "annulling what a credit note did not already reverse is allowed; body was: {}",
        annulled.text
    );

    let total: i32 = sqlx::query_scalar("SELECT reversed_quantity FROM sale_items WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&item_id).expect("uuid"))
        .fetch_one(&pool)
        .await
        .expect("read the budget");
    assert_eq!(
        total, 3,
        "one unit by the credit note, two by the annulment: 3 of 3, never more"
    );
}


/// Inserts a sale **and its ledger entry** through raw SQL, in one transaction.
///
/// `0021` turned the sale's two states into a rule checked at COMMIT: a sale that names a
/// method must write an entry, and a sale that moved no money must name an obligation. A
/// raw fixture that inserted only the header would now be refused — correctly — so the
/// fixtures below record the whole event, as the routes do.
///
/// It is a helper rather than a copy of the pair in six places because the next slice to
/// add a document kind would otherwise have to find all six.
async fn insert_paid_sale(
    pool: &sqlx::PgPool,
    customer: uuid::Uuid,
    total: &str,
    method: uuid::Uuid,
) -> uuid::Uuid {
    let mut tx = pool.begin().await.expect("begin the paid sale");
    // **`created_at` comes back from the insert, not from `NOW()`** (`R2-004`). Production
    // writes the entry at the sale's own `created_at` — one transaction, one instant — so a
    // helper that stamped a second timestamp would answer a different question than the route
    // it claims to mirror. That matters the moment a test compares the book's ordering
    // against the document's.
    let (sale_id, sale_created_at): (uuid::Uuid, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as(
            "INSERT INTO sales (customer_id, total, payment_method_id)
             VALUES ($1, $2::numeric, $3) RETURNING id, created_at",
        )
        .bind(customer)
        .bind(total)
        .bind(method)
        .fetch_one(&mut *tx)
        .await
        .expect("insert the sale header");
    sqlx::query(
        "INSERT INTO money_entries
            (account_id, payment_method_id, document_kind, document_id, amount,
             occurred_at, created_by)
         SELECT pm.account_id, $1, 'sale', $2, $3::numeric, $4, 'test'
           FROM payment_methods pm WHERE pm.id = $1",
    )
    .bind(method)
    .bind(sale_id)
    .bind(total)
    .bind(sale_created_at)
    .execute(&mut *tx)
    .await
    .expect("insert the sale's ledger entry");
    tx.commit().await.expect("commit the paid sale");
    sale_id
}

// ── v4 S6a: what we are owed is a document, not a wallet ────────────────────
//
// `odd/tasks/v4-receivables.md` S1-S3, S7, S8. The slice exists for one measured
// sequence, recorded as `v3-accounts.md` L107:
//
//     sale on credit 100.00, never collected      credit +100.00   cash    0.00
//     refund naming `cash` (nobody paid anything) credit +100.00   cash -100.00
//
// The shop paid a customer who never paid it, and still believes it is owed the money.
// These tests pin the replacement: a debt is an `obligations` row, a sale on account
// moves no money at all, and a refund of an unpaid sale lowers the debt instead of
// emptying the till.

/// The seeded `credit` account, `…0005`. Named here rather than imported so a test
/// cannot be silently re-pointed by a constant moving.
const DEBT_ACCOUNT: &str = "00000000-0000-0000-0000-000000000005";
const DEBT_METHOD: &str = "00000000-0000-0000-0000-000000000006";

/// A named customer and a product, because a sale on account needs both.
async fn receivable_fixture(
    client: &common::Client,
    tag: &str,
    price: &str,
) -> (String, String) {
    let customer = client
        .post("/api/customers", serde_json::json!({ "name": format!("recv-{tag}") }))
        .await;
    let customer_id = customer.body["id"].as_str().expect("customer id").to_string();
    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": format!("recv-prod-{tag}"), "list_price": price }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("product id").to_string();
    (customer_id, product_id)
}

/// Creates a sale on account through the API and returns `(sale_id, customer_id)`.
async fn sale_on_account(
    client: &common::Client,
    tag: &str,
    price: &str,
    quantity: i32,
) -> (String, String) {
    let (customer_id, product_id) = receivable_fixture(client, tag, price).await;
    let sale = client
        .post(
            "/api/sales",
            serde_json::json!({
                "customer_id": customer_id,
                "items": [{ "product_id": product_id, "quantity": quantity }],
                "payment_method_id": "on_account"
            }),
        )
        .await;
    assert_eq!(
        sale.status(),
        axum::http::StatusCode::CREATED,
        "a sale on account must be accepted; body: {}",
        sale.text
    );
    let sale_id = sale.body["id"].as_str().expect("sale id").to_string();
    (sale_id, customer_id)
}

/// What a sale still owes: the debt less every document that lowered it and every
/// payment that settled it. Derived in SQL, never stored — the same reason a balance is.
async fn outstanding_of(pool: &sqlx::PgPool, sale_id: &str) -> String {
    let value: rust_decimal::Decimal = sqlx::query_scalar(
        "SELECT o.amount
                - COALESCE((SELECT SUM(a.amount) FROM obligations_adjustments a
                             WHERE a.obligation_id = o.id), 0)
                - COALESCE((SELECT SUM(s.amount) FROM settlements s
                             WHERE s.obligation_id = o.id), 0)
           FROM obligations o
          WHERE o.source_document_kind = 'sale' AND o.source_document_id = $1",
    )
    .bind(uuid::Uuid::parse_str(sale_id).expect("uuid"))
    .fetch_one(pool)
    .await
    .expect("outstanding");
    value.normalize().to_string()
}

/// The obligation a sale on account owes, as an id string.
async fn obligation_id_of(pool: &sqlx::PgPool, sale_id: &str) -> String {
    sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT id FROM obligations
          WHERE source_document_kind = 'sale' AND source_document_id = $1",
    )
    .bind(uuid::Uuid::parse_str(sale_id).expect("uuid"))
    .fetch_one(pool)
    .await
    .expect("the sale owes one obligation")
    .to_string()
}

#[sqlx::test]
async fn v4_s6a_a_sale_on_account_owes_money_and_moves_none(pool: sqlx::PgPool) {
    // S1 and S2. The whole slice rests on this: the debt is one row in `obligations`,
    // and the book stays silent.
    //
    // Written positive-first — the sale must be ACCEPTED — for the reason
    // `v3_t4_a_credit_sale_must_name_a_customer` gives: a refusal alone could pass
    // because the `credit` method is seeded inactive rather than because the rule holds.
    let client = common::client(pool.clone());
    let cash_account = "00000000-0000-0000-0000-000000000003";
    let (customer_id, product_id) = receivable_fixture(&client, "onaccount", "100.00").await;

    let sale = client
        .post(
            "/api/sales",
            serde_json::json!({
                "customer_id": customer_id,
                "items": [{ "product_id": product_id, "quantity": 1 }],
                "payment_method_id": "on_account"
            }),
        )
        .await;
    assert_eq!(
        sale.status(),
        axum::http::StatusCode::CREATED,
        "a sale on account is accepted; body was: {}",
        sale.text
    );
    let sale_id = sale.body["id"].as_str().expect("sale id").to_string();

    // The sale says, in its own row, that it moved no money.
    assert!(
        sale.body["payment_method_id"].is_null(),
        "a sale on account names no method; body was: {}",
        sale.body
    );

    // The debt exists, once, for the amount of the sale, and belongs to its customer.
    let owed: (rust_decimal::Decimal, String) = sqlx::query_as(
        "SELECT amount, party_id::text FROM obligations
          WHERE source_document_kind = 'sale' AND source_document_id = $1",
    )
    .bind(uuid::Uuid::parse_str(&sale_id).expect("uuid"))
    .fetch_one(&pool)
    .await
    .expect("the sale must owe exactly one obligation");
    assert_eq!(owed.0.to_string(), "100.000000");
    assert_eq!(owed.1, customer_id);

    // **And nothing entered the book.** Not the till, not the debt account.
    let entries: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM money_entries WHERE document_kind = 'sale' AND document_id = $1",
    )
    .bind(uuid::Uuid::parse_str(&sale_id).expect("uuid"))
    .fetch_one(&pool)
    .await
    .expect("count entries");
    assert_eq!(entries, 0, "a sale on account moves no money");
    assert_eq!(balance_of(&client, cash_account).await, "0.00");
    assert_eq!(balance_of(&client, DEBT_ACCOUNT).await, "0.00");
}

#[sqlx::test]
async fn v4_s6a_a_paid_sale_writes_the_book_and_owes_nothing(pool: sqlx::PgPool) {
    // The other half of S2, and the one that protects every existing sale: a sale that
    // names a method must write its entry and must NOT acquire an obligation.
    let client = common::client(pool.clone());
    let product = client
        .post(
            "/api/products",
            serde_json::json!({ "name": "recv-paid", "list_price": "40.00" }),
        )
        .await;
    let product_id = product.body["id"].as_str().expect("id").to_string();

    let sale = client
        .post(
            "/api/sales",
            serde_json::json!({ "items": [{ "product_id": product_id, "quantity": 1 }] }),
        )
        .await;
    assert_eq!(sale.status(), axum::http::StatusCode::CREATED);
    let sale_id = sale.body["id"].as_str().expect("sale id").to_string();

    let entries: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM money_entries WHERE document_kind = 'sale' AND document_id = $1",
    )
    .bind(uuid::Uuid::parse_str(&sale_id).expect("uuid"))
    .fetch_one(&pool)
    .await
    .expect("count entries");
    assert_eq!(entries, 1, "a paid sale writes one entry");

    let owed: i64 = sqlx::query_scalar("SELECT count(*) FROM obligations WHERE source_document_id = $1")
        .bind(uuid::Uuid::parse_str(&sale_id).expect("uuid"))
        .fetch_one(&pool)
        .await
        .expect("count obligations");
    assert_eq!(owed, 0, "money moved, so there is no debt");
}

#[sqlx::test]
async fn v4_s6a_a_refund_of_a_sale_nobody_paid_lowers_the_debt_and_moves_no_money(
    pool: sqlx::PgPool,
) {
    // **This is `L107`'s measured sequence, and the reason the slice exists.** The
    // refund is accepted — the customer did bring the goods back — but it must reduce
    // what is owed, not hand out money.
    let client = common::client(pool.clone());
    let cash_account = "00000000-0000-0000-0000-000000000003";
    let (customer_id, product_id) = receivable_fixture(&client, "refundowed", "100.00").await;

    let sale = client
        .post(
            "/api/sales",
            serde_json::json!({
                "customer_id": customer_id,
                "items": [{ "product_id": product_id, "quantity": 2 }],
                "payment_method_id": "on_account"
            }),
        )
        .await;
    assert_eq!(sale.status(), axum::http::StatusCode::CREATED);
    let sale_id = sale.body["id"].as_str().expect("sale id").to_string();
    let item_id = sale.body["items"][0]["id"].as_str().expect("item").to_string();

    // One unit of two comes back, inheriting the sale's method — which is now NULL.
    let note = client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({ "items": [{ "sale_item_id": item_id, "quantity": 1 }] }),
        )
        .await;
    assert_eq!(
        note.status(),
        axum::http::StatusCode::CREATED,
        "the refund of an unpaid sale is accepted; body was: {}",
        note.text
    );

    // The book is untouched on both accounts, which is the contrast with L107.
    assert_eq!(
        balance_of(&client, cash_account).await,
        "0.00",
        "the shop did not pay a customer who never paid it"
    );
    assert_eq!(balance_of(&client, DEBT_ACCOUNT).await, "0.00");

    // The debt dropped by the value of the returned unit, and the original stands.
    // Through the helper, so this and every other assertion mean the same thing by
    // "outstanding" (`R2-003`).
    assert_eq!(
        outstanding_of(&pool, &sale_id).await,
        "100",
        "200 owed less 100 returned"
    );

    // And the note itself claims to have moved no money.
    let note_id: uuid::Uuid = note.body["id"].as_str().expect("id").parse().expect("uuid");
    let note_entries: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM money_entries WHERE document_kind = 'credit_note' AND document_id = $1",
    )
    .bind(note_id)
    .fetch_one(&pool)
    .await
    .expect("count entries");
    assert_eq!(note_entries, 0);
}

#[sqlx::test]
async fn v4_s6a_a_null_method_is_never_an_arbitrary_null(pool: sqlx::PgPool) {
    // S2's second half: the NULL is a claim, and the schema is what holds it. Three ways
    // to break the pair, each refused at COMMIT, each asserted by message rather than by
    // `is_err()` alone — `v6c` of `v4-settlement.md` records what an `is_err()`-only test
    // cost this repo: a guard it was not measuring.
    let client = common::client(pool.clone());
    let (customer_id, product_id) = receivable_fixture(&client, "pair", "10.00").await;

    // A sale with no method and no obligation: debt out of nowhere.
    let mut tx = pool.begin().await.expect("begin");
    let _no_cause = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO sales (customer_id, total, payment_method_id)
         VALUES ($1, 10.00, NULL) RETURNING id",
    )
    .bind(uuid::Uuid::parse_str(&customer_id).expect("uuid"))
    .fetch_one(&mut *tx)
    .await
    .expect("insert the sale");
    let failure = tx
        .commit()
        .await
        .expect_err("a sale claiming to have moved no money with no obligation must be refused");
    let message = failure.to_string();
    assert!(
        message.contains("0 obligation(s) name it"),
        "the refusal must name the missing obligation, got: {message}"
    );

    // An obligation on a sale that named a method: debt on top of money that moved.
    let paid = client
        .post(
            "/api/sales",
            serde_json::json!({
                "customer_id": customer_id,
                "items": [{ "product_id": product_id, "quantity": 1 }]
            }),
        )
        .await;
    assert_eq!(paid.status(), axum::http::StatusCode::CREATED);
    let paid_id = paid.body["id"].as_str().expect("id").to_string();

    let mut tx = pool.begin().await.expect("begin");
    let insert = sqlx::query(
        "INSERT INTO obligations
            (source_document_kind, source_document_id, party_kind, party_id, amount,
             occurred_at, created_by)
         VALUES ('sale', $1, 'customer', $2, 10.00, NOW(), 'test')",
    )
    .bind(uuid::Uuid::parse_str(&paid_id).expect("uuid"))
    .bind(uuid::Uuid::parse_str(&customer_id).expect("uuid"))
    .execute(&mut *tx)
    .await;

    // **The failure may arrive at the INSERT or at the COMMIT, and the test must accept
    // neither as a pass without naming the reason.** Writing it as
    // `if insert.is_ok() { ... }` would let an insert that failed for an unrelated reason
    // — a typo in a column name, a missing table — look like the guard biting. That is
    // `V6c` of `v4-settlement.md` all over again, where an `is_err()`-only assertion stayed
    // green while the guard it claimed to measure was neutered.
    let message = match insert {
        Err(e) => e.to_string(),
        Ok(_) => tx
            .commit()
            .await
            .expect_err("a debt on a sale that moved money must be refused at COMMIT")
            .to_string(),
    };
    assert!(
        message.contains("names payment method"),
        "the refusal must name the method that already claimed the money, got: {message}"
    );
}

#[sqlx::test]
async fn v4_s6a_the_book_refuses_to_name_the_debt_account_again(pool: sqlx::PgPool) {
    // S1 read backwards, and the guard that keeps S7 a delete rather than a data
    // migration: once no sale on account writes an entry, nothing may ever point at the
    // debt account.
    let mut tx = pool.begin().await.expect("begin");
    let refused = sqlx::query(
        "INSERT INTO money_entries
            (account_id, payment_method_id, document_kind, document_id, amount,
             occurred_at, created_by)
         VALUES ($1, $2, 'opening_balance', gen_random_uuid(), 5.00, NOW(), 'test')",
    )
    .bind(uuid::Uuid::parse_str(DEBT_ACCOUNT).expect("uuid"))
    .bind(uuid::Uuid::parse_str(DEBT_METHOD).expect("uuid"))
    .execute(&mut *tx)
    .await;
    assert!(
        refused.is_err(),
        "a ledger row naming the debt account must be refused"
    );
    let message = refused.unwrap_err().to_string();
    assert!(
        message.contains("debt is a document"),
        "the refusal must say what the debt is instead, got: {message}"
    );
    tx.rollback().await.expect("rollback");
}

// ── v4 S6a, second half: a collection is a document ─────────────────────────
//
// `odd/tasks/v4-receivables.md` S4-S6, migration `0022`. The first half made a sale on
// account owe without moving money. This half is the money arriving, and the rows saying
// which debt it paid — which is what turns "what is owed" from a number somebody
// remembers into a subtraction of stored rows.

#[sqlx::test]
async fn v4_s6a_a_collection_moves_money_and_settles_the_debt(pool: sqlx::PgPool) {
    // The whole point, in one test: a customer owing 100 pays 60, and afterwards the shop
    // has 60 in the till and is owed 40. Both numbers are derived, and neither is stored.
    let client = common::client(pool.clone());
    let cash_account = "00000000-0000-0000-0000-000000000003";
    let cash_method = "00000000-0000-0000-0000-000000000004";
    let (sale_id, customer_id) =
        sale_on_account(&client, "collect", "100.00", 1).await;
    assert_eq!(outstanding_of(&pool, &sale_id).await, "100");

    let payment = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "60.00",
                "direction": "in"
            }),
        )
        .await;
    assert_eq!(
        payment.status(),
        axum::http::StatusCode::CREATED,
        "a collection must be accepted; body: {}",
        payment.text
    );
    let payment_id = payment.body["id"].as_str().expect("payment id").to_string();

    // The money is in the till, once.
    assert_eq!(balance_of(&client, cash_account).await, "60.00");

    // Settle the debt against that payment.
    let applied = client
        .post(
            "/api/settlements",
            serde_json::json!({
                "obligation_id": obligation_id_of(&pool, &sale_id).await,
                "payment_id": payment_id,
                "amount": "60.00"
            }),
        )
        .await;
    assert_eq!(
        applied.status(),
        axum::http::StatusCode::CREATED,
        "the settlement must be accepted; body: {}",
        applied.text
    );

    assert_eq!(
        outstanding_of(&pool, &sale_id).await,
        "40",
        "the debt is what is left, not a state anyone wrote"
    );

    // And the till did not move again: settling is not paying.
    assert_eq!(
        balance_of(&client, cash_account).await,
        "60.00",
        "settlement allocates money that already moved; it moves none"
    );
}

#[sqlx::test]
async fn v4_s6a_a_payment_may_cover_several_debts_and_a_debt_may_be_paid_in_parts(
    pool: sqlx::PgPool,
) {
    // The two facts that made `settlements` a table instead of a column on either side
    // (`v4-settlement.md` L18, owner's words). This test is the argument for the design,
    // so it asserts both shapes, with the numbers agreeing at the end.
    let client = common::client(pool.clone());
    let cash_method = "00000000-0000-0000-0000-000000000004";
    let cash_account = "00000000-0000-0000-0000-000000000003";

    let (first_sale, customer_id) = sale_on_account(&client, "multi-a", "100.00", 1).await;
    // **Both debts must belong to the paying customer, and the settlement guard now says
    // so** (`R1-002` / `R3-002`): one customer's money may not settle another's debt. The
    // first version of this test gave each sale its own customer and settled both from one
    // payment, which a total-and-ceiling check let through and a party check does not.
    let (second_sale, _) = {
        let (_, product_id) = receivable_fixture(&client, "multi-b", "50.00").await;
        let sale = client
            .post(
                "/api/sales",
                serde_json::json!({
                    "customer_id": customer_id,
                    "items": [{ "product_id": product_id, "quantity": 1 }],
                    "payment_method_id": "on_account"
                }),
            )
            .await;
        assert_eq!(sale.status(), axum::http::StatusCode::CREATED);
        (
            sale.body["id"].as_str().expect("sale id").to_string(),
            customer_id.clone(),
        )
    };

    let payment = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "180.00",
                "direction": "in"
            }),
        )
        .await;
    assert_eq!(payment.status(), axum::http::StatusCode::CREATED);
    let payment_id = payment.body["id"].as_str().expect("payment id").to_string();
    assert_eq!(balance_of(&client, cash_account).await, "180.00");

    // **Two payments and three settlements, because "in parts" means across payments.**
    // The first version of this test wrote 40 then 60 against the same pair, and `0022`'s
    // `UNIQUE (obligation_id, payment_id)` refused the second row — correctly: two rows
    // from the same payment to the same debt sum to exactly what one row of 100 would say,
    // so allowing them would be storing the same fact twice. A debt paid in parts is paid
    // by different collections over time, which is what this now expresses.
    let second_payment = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "110.00",
                "direction": "in"
            }),
        )
        .await;
    assert_eq!(second_payment.status(), axum::http::StatusCode::CREATED);
    let second_payment_id = second_payment.body["id"].as_str().expect("id").to_string();

    for (sale, amount, payer) in [
        (&first_sale, "40.00", &payment_id),
        (&first_sale, "60.00", &second_payment_id),
        (&second_sale, "50.00", &second_payment_id),
    ] {
        let applied = client
            .post(
                "/api/settlements",
                serde_json::json!({
                    "obligation_id": obligation_id_of(&pool, sale).await,
                    "payment_id": payer,
                    "amount": amount
                }),
            )
            .await;
        assert_eq!(
            applied.status(),
            axum::http::StatusCode::CREATED,
            "settling {amount} must be accepted; body: {}",
            applied.text
        );
    }

    assert_eq!(outstanding_of(&pool, &first_sale).await, "0");
    assert_eq!(outstanding_of(&pool, &second_sale).await, "0");

    // **The two balances say different things, which is why both are asserted.** The
    // second payment is fully applied — 60 + 50 is its 110 — so it carries nothing
    // forward. The first covered only 40 of its 180, so 140 stay as this customer's credit
    // balance: money already in the till, owed back as goods or as a refund (S6, and the
    // reason `unapplied` is derived rather than stored).
    for (payer, expected) in [(&payment_id, "140"), (&second_payment_id, "0")] {
        let unapplied: rust_decimal::Decimal = sqlx::query_scalar(
            "SELECT p.total - COALESCE((SELECT SUM(amount) FROM settlements s
                                         WHERE s.payment_id = p.id), 0)
               FROM payments p WHERE p.id = $1",
        )
        .bind(uuid::Uuid::parse_str(payer).expect("uuid"))
        .fetch_one(&pool)
        .await
        .expect("unapplied");
        assert_eq!(unapplied.normalize().to_string(), expected);
    }

    // **And the money in the till moved once per payment, never per settlement.** 180 +
    // 110 arrived; the three settlements applied it all without moving another cent.
    assert_eq!(balance_of(&client, cash_account).await, "290.00");
}

#[sqlx::test]
async fn v4_s6a_both_ceilings_refuse_and_say_which_one(pool: sqlx::PgPool) {
    // A ceiling that refuses for the wrong reason is a ceiling nobody can act on. Each
    // refusal names the number it compared — the reason `V6c` of `v4-settlement.md` exists:
    // an `is_err()`-only assertion stayed green while the guard it claimed to measure was
    // neutered.
    let client = common::client(pool.clone());
    let cash_method = "00000000-0000-0000-0000-000000000004";
    let (sale_a, customer_id) = sale_on_account(&client, "ceil-a", "100.00", 1).await;
    // A second debt for the SAME customer, for the reason recorded in the multi-debt test
    // above: a payment settles only its own party's debts.
    let sale_b = {
        let (_, product_id) = receivable_fixture(&client, "ceil-b", "100.00").await;
        let sale = client
            .post(
                "/api/sales",
                serde_json::json!({
                    "customer_id": customer_id,
                    "items": [{ "product_id": product_id, "quantity": 1 }],
                    "payment_method_id": "on_account"
                }),
            )
            .await;
        assert_eq!(sale.status(), axum::http::StatusCode::CREATED);
        sale.body["id"].as_str().expect("sale id").to_string()
    };

    // A payment of 60.
    let payment = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "60.00",
                "direction": "in"
            }),
        )
        .await;
    let payment_id = payment.body["id"].as_str().expect("id").to_string();
    let obligation_a = obligation_id_of(&pool, &sale_a).await;
    let obligation_b = obligation_id_of(&pool, &sale_b).await;

    // The payment ceiling: 70 out of a payment of 60. Refused by the handler with a
    // status the caller can act on.
    let over_payment = client
        .post(
            "/api/settlements",
            serde_json::json!({
                "obligation_id": obligation_a,
                "payment_id": payment_id,
                "amount": "70.00"
            }),
        )
        .await;
    assert_eq!(
        over_payment.status(),
        axum::http::StatusCode::CONFLICT,
        "applying more than was paid is a conflict; body: {}",
        over_payment.text
    );

    // The obligation ceiling: 60 on the first debt, then 60 more on the second from a
    // payment that has nothing left. The FIRST half must succeed, so the refusal below
    // cannot be the payment ceiling answering instead.
    let first = client
        .post(
            "/api/settlements",
            serde_json::json!({
                "obligation_id": obligation_a,
                "payment_id": payment_id,
                "amount": "60.00"
            }),
        )
        .await;
    assert_eq!(
        first.status(),
        axum::http::StatusCode::CREATED,
        "the legitimate half must be accepted; body: {}",
        first.text
    );

    let nothing_left = client
        .post(
            "/api/settlements",
            serde_json::json!({
                "obligation_id": obligation_b,
                "payment_id": payment_id,
                "amount": "1.00"
            }),
        )
        .await;
    assert_eq!(
        nothing_left.status(),
        axum::http::StatusCode::CONFLICT,
        "a payment with nothing left cannot cover another debt; body: {}",
        nothing_left.text
    );

    // Nothing was written by either refusal.
    assert_eq!(outstanding_of(&pool, &sale_a).await, "40");
    assert_eq!(outstanding_of(&pool, &sale_b).await, "100");
}

#[sqlx::test]
async fn v4_s6a_a_settlement_past_the_debt_is_refused_by_the_handler(pool: sqlx::PgPool) {
    // The obligation ceiling on its own, with a payment large enough that only the debt
    // can be the thing that refused. Written after the combined test above, because that
    // one could pass on the wrong ceiling if the payment were also small.
    let client = common::client(pool.clone());
    let cash_method = "00000000-0000-0000-0000-000000000004";
    let (sale, customer_id) = sale_on_account(&client, "debt-ceil", "30.00", 1).await;

    let payment = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "500.00",
                "direction": "in"
            }),
        )
        .await;
    let payment_id = payment.body["id"].as_str().expect("id").to_string();

    let too_much = client
        .post(
            "/api/settlements",
            serde_json::json!({
                "obligation_id": obligation_id_of(&pool, &sale).await,
                "payment_id": payment_id,
                "amount": "31.00"
            }),
        )
        .await;
    assert_eq!(
        too_much.status(),
        axum::http::StatusCode::CONFLICT,
        "settling more than is owed is a conflict; body: {}",
        too_much.text
    );
    assert_eq!(
        outstanding_of(&pool, &sale).await,
        "30",
        "the refusal changed nothing"
    );
}

#[sqlx::test]
async fn v4_s6a_a_payment_never_names_the_debt_account(pool: sqlx::PgPool) {
    // A collection is money arriving, so it names a real rail. The debt account stopped
    // being writable in `0021`; this is the payment-side statement of the same rule.
    let client = common::client(pool.clone());
    let (_, customer_id) = sale_on_account(&client, "pay-debt", "10.00", 1).await;

    let refused = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": DEBT_METHOD,
                "total": "10.00",
                "direction": "in"
            }),
        )
        .await;
    assert_eq!(
        refused.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "a payment through the debt account is not money arriving; body: {}",
        refused.text
    );

    let entries: i64 = sqlx::query_scalar("SELECT count(*) FROM payments")
        .fetch_one(&pool)
        .await
        .expect("count payments");
    assert_eq!(entries, 0, "the refusal left nothing behind");
}

#[sqlx::test]
async fn v4_s6a_a_refund_of_a_credit_balance_is_a_payment_out(pool: sqlx::PgPool) {
    // S8's second consequence (`v4-settlement.md` L21): a credit balance is refundable, and
    // refunding it is the same document with the opposite sign — `direction = out`. Without
    // this, a business keeps money that is not its, which is the defect rejected option 1
    // of that document had under a different name.
    let client = common::client(pool.clone());
    let cash_method = "00000000-0000-0000-0000-000000000004";
    let cash_account = "00000000-0000-0000-0000-000000000003";
    let (_, customer_id) = sale_on_account(&client, "refund-bal", "10.00", 1).await;

    // 100 handed over for a 10 debt: 90 stay on the customer's account.
    let payment = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "100.00",
                "direction": "in"
            }),
        )
        .await;
    assert_eq!(payment.status(), axum::http::StatusCode::CREATED);
    assert_eq!(balance_of(&client, cash_account).await, "100.00");

    let refund = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "90.00",
                "direction": "out"
            }),
        )
        .await;
    assert_eq!(
        refund.status(),
        axum::http::StatusCode::CREATED,
        "a credit balance must be refundable; body: {}",
        refund.text
    );

    // The money left, and it left as a negative entry — one row, not an edit.
    assert_eq!(balance_of(&client, cash_account).await, "10.00");
}

#[sqlx::test]
async fn v4_s6a_a_refund_through_a_real_rail_needs_money_that_actually_arrived(pool: sqlx::PgPool) {
    // **`v3-accounts.md` L107, closed.** The measured sequence this whole slice exists for:
    //
    //     sale on credit 100.00, never collected      credit +100.00   cash    0.00
    //     refund naming `cash` (nobody paid anything) credit +100.00   cash -100.00
    //
    // `0014`'s `-total` against a rail nobody paid into is how the shop paid a customer
    // who had not paid it. The rule now: **a note that names a method other than the sale's
    // requires the sale to have been settled for at least the note's total.** The inherited
    // path stays free, because it lowers the debt instead of moving money — the two paths
    // are no longer the same statement with a different account.
    let client = common::client(pool.clone());
    let cash_account = "00000000-0000-0000-0000-000000000003";
    let cash_method = "00000000-0000-0000-0000-000000000004";

    // A sale on account of 100, nobody has paid anything.
    let (sale_id, customer_id) = sale_on_account(&client, "l107", "100.00", 1).await;
    let sale = client.get(&format!("/api/sales?limit=200")).await;
    let item_id = sale
        .json()
        .as_array()
        .expect("array")
        .iter()
        .find(|s| s["id"] == sale_id.as_str())
        .expect("the sale is listed")["items"][0]["id"]
        .as_str()
        .expect("item id")
        .to_string();

    // 1. The hole, refused: a refund out of the till of a sale nobody paid.
    let refused = client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({
                "items": [{ "sale_item_id": item_id, "quantity": 1 }],
                "payment_method_id": cash_method
            }),
        )
        .await;
    assert_eq!(
        refused.status(),
        axum::http::StatusCode::CONFLICT,
        "a refund through a rail nobody paid into must be refused; body: {}",
        refused.text
    );

    // And the till did not move, which is the number L107 measured going to -100.
    assert_eq!(
        balance_of(&client, cash_account).await,
        "0.00",
        "the shop did not pay a customer who never paid it"
    );
    assert_eq!(
        outstanding_of(&pool, &sale_id).await,
        "100",
        "the refusal left the debt exactly where it was"
    );

    // 2. The legitimate half, so the refusal cannot be a blanket one: once 100 has really
    //    been collected, the same refund is allowed and the money leaves the till.
    let payment = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "100.00",
                "direction": "in"
            }),
        )
        .await;
    assert_eq!(payment.status(), axum::http::StatusCode::CREATED);
    let payment_id = payment.body["id"].as_str().expect("payment id").to_string();
    let applied = client
        .post(
            "/api/settlements",
            serde_json::json!({
                "obligation_id": obligation_id_of(&pool, &sale_id).await,
                "payment_id": payment_id,
                "amount": "100.00"
            }),
        )
        .await;
    assert_eq!(applied.status(), axum::http::StatusCode::CREATED);
    assert_eq!(balance_of(&client, cash_account).await, "100.00");

    let accepted = client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({
                "items": [{ "sale_item_id": item_id, "quantity": 1 }],
                "payment_method_id": cash_method
            }),
        )
        .await;
    assert_eq!(
        accepted.status(),
        axum::http::StatusCode::CREATED,
        "a refund of a sale that was actually collected is allowed; body: {}",
        accepted.text
    );
    assert_eq!(
        balance_of(&client, cash_account).await,
        "0.00",
        "and the money went back out of the till it came into"
    );
}

// ── v4 S6a, the five severe findings the review of this candidate measured ──────
//
// Each of these reproduced against a live server before the fix, and each is a case of
// the slice's own invariant leaking through a new door: money that never arrived being
// paid out, or a debt being resolved twice — once by money that left.
//
// The invariant, stated once so the six tests below are read against it:
//
//     money may only leave a customer's balance up to what arrived, and a debt may only
//     be resolved by money that came IN.
//
// Every test is written positive-first: the legitimate operation is asserted to succeed
// before the illegitimate one is asserted to be refused, because a refusal alone cannot
// prove it was refused for the right reason.

#[sqlx::test]
async fn v4_s6a_a_refund_may_not_exceed_what_the_customer_actually_has(pool: sqlx::PgPool) {
    // `R1-001` and `R3-001`, the same defect seen by two lenses: `create_payment`
    // accepted any positive `direction = "out"` total. Measured before the fix: a
    // customer with no collections at all received a 500.00 refund, the request answered
    // 201, and the till went to -500.00.
    let client = common::client(pool.clone());
    let cash_account = "00000000-0000-0000-0000-000000000003";
    let cash_method = "00000000-0000-0000-0000-000000000004";
    let (_, customer_id) = sale_on_account(&client, "refund-gate", "10.00", 1).await;

    // Nothing arrived, so nothing may leave — not even a cent.
    let nothing_to_refund = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "0.01",
                "direction": "out"
            }),
        )
        .await;
    assert_eq!(
        nothing_to_refund.status(),
        axum::http::StatusCode::CONFLICT,
        "a customer with no credit balance has nothing to refund; body: {}",
        nothing_to_refund.text
    );
    assert_eq!(balance_of(&client, cash_account).await, "0.00");

    // Money that really arrived may really leave. 100 in, 90 out.
    let collected = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "100.00",
                "direction": "in"
            }),
        )
        .await;
    assert_eq!(collected.status(), axum::http::StatusCode::CREATED);
    assert_eq!(balance_of(&client, cash_account).await, "100.00");

    let refund = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "90.00",
                "direction": "out"
            }),
        )
        .await;
    assert_eq!(
        refund.status(),
        axum::http::StatusCode::CREATED,
        "refunding within the balance is allowed; body: {}",
        refund.text
    );
    assert_eq!(balance_of(&client, cash_account).await, "10.00");

    // And the balance is now 10, so 90 more is refused. This is the half that a naive
    // "does the customer have any payments at all" check would let through.
    let too_much = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "90.00",
                "direction": "out"
            }),
        )
        .await;
    assert_eq!(
        too_much.status(),
        axum::http::StatusCode::CONFLICT,
        "the balance is the ceiling, not the customer's history; body: {}",
        too_much.text
    );
    assert_eq!(balance_of(&client, cash_account).await, "10.00");
}

#[sqlx::test]
async fn v4_s6a_only_money_that_came_in_can_settle_a_debt(pool: sqlx::PgPool) {
    // `R1-002` and `R3-002`. `create_settlement` read only `payments.total`, never the
    // direction, so a refund that took money OUT could be applied to a debt. Measured:
    // a 100.00 on-account sale, a 100.00 outbound "refund", settle it — debt 0, till
    // -700.00 accumulated across the probe. The customer's money was resolved twice.
    let client = common::client(pool.clone());
    let cash_method = "00000000-0000-0000-0000-000000000004";
    let (sale_id, customer_id) = sale_on_account(&client, "dir-gate", "100.00", 1).await;

    let refund = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "100.00",
                "direction": "in"
            }),
        )
        .await;
    assert_eq!(refund.status(), axum::http::StatusCode::CREATED);

    let outbound = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "100.00",
                "direction": "out"
            }),
        )
        .await;
    assert_eq!(outbound.status(), axum::http::StatusCode::CREATED);
    let outbound_id = outbound.body["id"].as_str().expect("id").to_string();

    let applied = client
        .post(
            "/api/settlements",
            serde_json::json!({
                "obligation_id": obligation_id_of(&pool, &sale_id).await,
                "payment_id": outbound_id,
                "amount": "100.00"
            }),
        )
        .await;
    assert_eq!(
        applied.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "money that left the till cannot settle a debt; body: {}",
        applied.text
    );
    assert_eq!(
        outstanding_of(&pool, &sale_id).await,
        "100",
        "the debt stands, because nothing has actually been collected"
    );
}

#[sqlx::test]
async fn v4_s6a_a_refund_through_a_rail_counts_what_left_not_just_what_arrived(
    pool: sqlx::PgPool,
) {
    // `R1-003` and `R3-003`, and this is the one that most resembles L107: the guard
    // compared each note against the settled total and never against what earlier
    // explicit notes had already taken out. Measured: a 200.00 on-account sale settled
    // for 100.00 admitted two 100.00 cash notes, and the till went 200 lower than it had
    // ever been higher.
    let client = common::client(pool.clone());
    let cash_account = "00000000-0000-0000-0000-000000000003";
    let cash_method = "00000000-0000-0000-0000-000000000004";
    let (sale_id, customer_id) = sale_on_account(&client, "double-refund", "100.00", 2).await;
    let items = client.get("/api/sales?limit=200").await;
    let item_id = items
        .json()
        .as_array()
        .expect("array")
        .iter()
        .find(|s| s["id"] == sale_id.as_str())
        .expect("listed")["items"][0]["id"]
        .as_str()
        .expect("item id")
        .to_string();

    // 100.00 really collected against a 200.00 debt.
    let collected = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "100.00",
                "direction": "in"
            }),
        )
        .await;
    assert_eq!(collected.status(), axum::http::StatusCode::CREATED);
    let payment_id = collected.body["id"].as_str().expect("id").to_string();
    let applied = client
        .post(
            "/api/settlements",
            serde_json::json!({
                "obligation_id": obligation_id_of(&pool, &sale_id).await,
                "payment_id": payment_id,
                "amount": "100.00"
            }),
        )
        .await;
    assert_eq!(applied.status(), axum::http::StatusCode::CREATED);
    assert_eq!(balance_of(&client, cash_account).await, "100.00");

    // The first note may take out exactly what came in.
    let first = client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({
                "items": [{ "sale_item_id": item_id, "quantity": 1 }],
                "payment_method_id": cash_method
            }),
        )
        .await;
    assert_eq!(
        first.status(),
        axum::http::StatusCode::CREATED,
        "the first note is within what arrived; body: {}",
        first.text
    );
    assert_eq!(balance_of(&client, cash_account).await, "0.00");

    // The second may not: every cent that arrived has already gone back out.
    let second = client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({
                "items": [{ "sale_item_id": item_id, "quantity": 1 }],
                "payment_method_id": cash_method
            }),
        )
        .await;
    assert_eq!(
        second.status(),
        axum::http::StatusCode::CONFLICT,
        "a second note would pay out money that never arrived; body: {}",
        second.text
    );
    assert_eq!(
        balance_of(&client, cash_account).await,
        "0.00",
        "the till did not move a second time"
    );
}

#[sqlx::test]
async fn v4_s6a_returning_goods_on_a_settled_debt_is_a_conflict_not_a_500(pool: sqlx::PgPool) {
    // `R1-004` and `R4-001`, the same defect from two lenses: the inherited no-method
    // path inserted the whole note total as an adjustment, and when the obligation had
    // no room left the ceiling trigger raised at COMMIT and the caller got a 500. The
    // status-code discipline this slice applies everywhere else applies here too.
    let client = common::client(pool.clone());
    let cash_method = "00000000-0000-0000-0000-000000000004";
    // A 100.00 on-account sale, already collected in full.
    let (sale_id, customer_id) = sale_on_account(&client, "settled-return", "100.00", 1).await;
    let items = client.get("/api/sales?limit=200").await;
    let item_id = items
        .json()
        .as_array()
        .expect("array")
        .iter()
        .find(|s| s["id"] == sale_id.as_str())
        .expect("listed")["items"][0]["id"]
        .as_str()
        .expect("item id")
        .to_string();

    let collected = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "100.00",
                "direction": "in"
            }),
        )
        .await;
    let payment_id = collected.body["id"].as_str().expect("id").to_string();
    client
        .post(
            "/api/settlements",
            serde_json::json!({
                "obligation_id": obligation_id_of(&pool, &sale_id).await,
                "payment_id": payment_id,
                "amount": "100.00"
            }),
        )
        .await;

    // Returning the goods without naming a method would try to lower a debt that money
    // already cleared in full. That is a conflict the caller can act on, not a fault.
    let returned = client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({ "items": [{ "sale_item_id": item_id, "quantity": 1 }] }),
        )
        .await;
    assert_eq!(
        returned.status(),
        axum::http::StatusCode::CONFLICT,
        "a return against an already-settled debt is a conflict; body: {}",
        returned.text
    );
}

#[sqlx::test]
async fn v4_s6a_the_new_endpoints_refuse_an_amount_that_is_not_whole_cents(pool: sqlx::PgPool) {
    // `R2-001`. The comment claimed every database CHECK was pre-checked, and the
    // whole-cents CHECK was not: three decimals reached the database and the caller got a
    // 500 from `conflict_or_500`. Worth a test rather than a comment fix, because the
    // claim was the reason nobody looked.
    let client = common::client(pool.clone());
    let cash_method = "00000000-0000-0000-0000-000000000004";
    let (sale_id, customer_id) = sale_on_account(&client, "cents", "10.00", 1).await;

    let fractional = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "10.001",
                "direction": "in"
            }),
        )
        .await;
    assert_eq!(
        fractional.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "an amount that is not whole cents is the caller's mistake; body: {}",
        fractional.text
    );

    let payment = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "10.00",
                "direction": "in"
            }),
        )
        .await;
    let payment_id = payment.body["id"].as_str().expect("id").to_string();

    let fractional_settlement = client
        .post(
            "/api/settlements",
            serde_json::json!({
                "obligation_id": obligation_id_of(&pool, &sale_id).await,
                "payment_id": payment_id,
                "amount": "1.005"
            }),
        )
        .await;
    assert_eq!(
        fractional_settlement.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "a settlement is money too; body: {}",
        fractional_settlement.text
    );
}

#[sqlx::test(migrations = false)]
async fn v4_s6a_migrations_refuse_a_legacy_credit_sale_instead_of_half_migrating(
    pool: sqlx::PgPool,
) {
    // **The BLOCKER of review `review-14d05c7f7f92e0bf`, pinned so it cannot return.**
    //
    // `0021` first carried a backfill for pre-existing credit sales. It was broken twice and
    // only visible on a database that HAD one: `UPDATE sales SET payment_method_id = NULL`
    // collided with `0013`'s `guard_sale_economics`, and the ledger rows it left behind
    // cannot be deleted because `money_entries` is append-only. Every other test in this
    // file starts from an empty database, so 119 of them passed while the migration could
    // not upgrade anything real — `v4-settlement.md` V1b's own warning, ignored.
    //
    // The owner's decision is that no database worth carrying exists, so the fix is a
    // refusal with a remedy rather than a backfill. This test drives the real migration
    // chain and asserts **which** refusal: "some error" would also pass if the migration
    // broke for an unrelated reason, which is the lesson `V6c` of `v4-settlement.md`
    // records.
    //
    // `migrations = false` leaves the database empty so the pre-slice state can be built
    // by hand: apply the chain, then insert a legacy sale, then run the chain again.
    let migrator = sqlx::migrate!("./migrations");

    // Everything the chain can apply to an empty database applies. `0021` is part of this
    // chain, so this first pass is the *fresh install* case, and it must succeed — that is
    // half of what this test claims.
    migrator
        .run(&pool)
        .await
        .expect("a fresh database must migrate cleanly");

    // Rewind to the pre-slice state: drop what `0021` created and forget it was applied, so
    // the second pass is a real upgrade rather than a no-op.
    // **`0022`'s tables go first, and this test learned it the hard way.** In a worktree
    // whose chain stops at `0021` these do not exist and `IF EXISTS` covers it; on a branch
    // that also carries `0022`, `settlements` references `obligations` and a bare `DROP TABLE
    // obligations` fails with "other objects depend on it". The rewind must undo the chain in
    // reverse order or it is not a rewind.
    for sql in [
        "DROP TRIGGER IF EXISTS money_entries_never_name_the_debt ON money_entries",
        "DROP TRIGGER IF EXISTS credit_notes_claim_their_effect ON credit_notes",
        "DROP TRIGGER IF EXISTS sales_payment_method_claimed ON sales",
        "DROP TRIGGER IF EXISTS payments_claim_their_entry ON payments",
        "DROP TABLE IF EXISTS settlements",
        "DROP TABLE IF EXISTS payments",
        "DROP TABLE IF EXISTS obligations_adjustments",
        "DROP TABLE IF EXISTS obligations",
        "ALTER TABLE credit_notes ALTER COLUMN payment_method_id SET NOT NULL",
        "ALTER TABLE sales ALTER COLUMN payment_method_id SET NOT NULL",
        "DELETE FROM _sqlx_migrations WHERE description LIKE '%obligations%'",
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap_or_else(|e| {
            panic!("rewinding to the pre-slice state failed on {sql}: {e}")
        });
    }

    // A legacy credit sale, exactly as the pre-slice code wrote it.
    sqlx::query(
        "INSERT INTO customers (id, name)
         VALUES ('00000000-0000-0000-0000-0000000000aa', 'legacy-fixture')",
    )
    .execute(&pool)
    .await
    .expect("legacy customer");
    let sale: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO sales (customer_id, total, payment_method_id)
         VALUES ('00000000-0000-0000-0000-0000000000aa', 100.00,
                 '00000000-0000-0000-0000-000000000006') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("legacy credit sale");
    sqlx::query(
        "INSERT INTO money_entries
            (account_id, payment_method_id, document_kind, document_id, amount,
             occurred_at, created_by)
         VALUES ('00000000-0000-0000-0000-000000000005',
                 '00000000-0000-0000-0000-000000000006', 'sale', $1, 100.00, NOW(), 'legacy')",
    )
    .bind(sale)
    .execute(&pool)
    .await
    .expect("the legacy ledger row");

    // Now the upgrade. It must refuse, and it must name what is in the way and the remedy.
    let failure = migrator
        .run(&pool)
        .await
        .expect_err("the upgrade must refuse a database holding legacy credit sales");
    let message = failure.to_string();
    assert!(
        message.contains("name the debt account") && message.contains("down -v"),
        "the refusal must say what is in the way and what to do, got: {message}"
    );
}

#[sqlx::test]
async fn v4_s6a_a_settlement_cannot_spend_money_that_was_already_refunded(pool: sqlx::PgPool) {
    // **`R1-001` and `R3-001` of review `review-1becd57b99cb3a1f`, found independently by both
    // lenses and left open as a residual.** Every guard the correction added passes and the
    // money is still resolved twice:
    //
    //     collect 100.00        available = 100
    //     refund  100.00        passes: 100 <= 100, the till is now 0, balance -100
    //     settle the 100 debt with the ORIGINAL inbound payment
    //                           direction ok, party ok, per-payment remainder ok,
    //                           obligation room ok -> debt 0, and the cash is gone
    //
    // The correction closed only the variant that settles *with the outbound refund itself*
    // (400, tested). The per-payment ceiling it kept counts one payment's own settlements and
    // cannot see that the party already took that money back out.
    //
    // The rule this test pins: **a settlement may not spend more than the party still has.**
    let client = common::client(pool.clone());
    let cash_account = "00000000-0000-0000-0000-000000000003";
    let cash_method = "00000000-0000-0000-0000-000000000004";
    let (sale_id, customer_id) = sale_on_account(&client, "residual", "100.00", 1).await;

    let collected = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "100.00",
                "direction": "in"
            }),
        )
        .await;
    assert_eq!(collected.status(), axum::http::StatusCode::CREATED);
    let inbound_id = collected.body["id"].as_str().expect("id").to_string();

    // The refund takes the whole balance back out. It is legitimate: 100 arrived, 100 leaves.
    let refund = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "100.00",
                "direction": "out"
            }),
        )
        .await;
    assert_eq!(
        refund.status(),
        axum::http::StatusCode::CREATED,
        "refunding the full available balance is allowed; body: {}",
        refund.text
    );
    assert_eq!(balance_of(&client, cash_account).await, "0.00");

    // **Now the residual.** The money has left, so the debt cannot be settled with it.
    let settled = client
        .post(
            "/api/settlements",
            serde_json::json!({
                "obligation_id": obligation_id_of(&pool, &sale_id).await,
                "payment_id": inbound_id,
                "amount": "100.00"
            }),
        )
        .await;
    assert_eq!(
        settled.status(),
        axum::http::StatusCode::CONFLICT,
        "the customer's money already left; it cannot also clear the debt; body: {}",
        settled.text
    );
    assert_eq!(
        outstanding_of(&pool, &sale_id).await,
        "100",
        "the debt stands, because nothing is left to pay it with"
    );

    // And the legitimate sequence still works, so the refusal above cannot be a blanket one:
    // 50 more arrives, and 50 of the debt is settled with it.
    let topup = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "50.00",
                "direction": "in"
            }),
        )
        .await;
    assert_eq!(topup.status(), axum::http::StatusCode::CREATED);
    let topup_id = topup.body["id"].as_str().expect("id").to_string();
    let partial = client
        .post(
            "/api/settlements",
            serde_json::json!({
                "obligation_id": obligation_id_of(&pool, &sale_id).await,
                "payment_id": topup_id,
                "amount": "50.00"
            }),
        )
        .await;
    assert_eq!(
        partial.status(),
        axum::http::StatusCode::CREATED,
        "money that really is available may settle; body: {}",
        partial.text
    );
    assert_eq!(outstanding_of(&pool, &sale_id).await, "50");
}

#[sqlx::test]
async fn v4_s6a_a_refund_may_not_leave_a_rail_that_never_received_the_money(pool: sqlx::PgPool) {
    // **`R1-refund-guard-rail-blind` of review `review-2e03230018397328`.** The reworked guard
    // measures what arrived **in aggregate** and never on the rail the note names, so a sale
    // collected in cash can be refunded through a different account and drive that account
    // negative — the same harm the guard exists to close, through a side door.
    //
    // The distinction the guard was missing: money is not fungible across rails *for the
    // purposes of a refund*. A customer pays into one account; the refund goes back through
    // an account the note names, and that account must have received enough through this sale.
    let client = common::client(pool.clone());
    let cash_account = "00000000-0000-0000-0000-000000000003";
    let cash_method = "00000000-0000-0000-0000-000000000004";

    // A second account with its own rail, so the refund has somewhere else to go.
    let account = client
        .post("/api/accounts", serde_json::json!({ "name": "banco-R1" }))
        .await;
    let account_id = account.body["id"].as_str().expect("account id").to_string();
    let method = client
        .post(
            "/api/payment-methods",
            serde_json::json!({ "name": "transferencia-R1", "account_id": account_id }),
        )
        .await;
    let bank_method = method.body["id"].as_str().expect("method id").to_string();

    // A sale on account, then collected IN CASH.
    let (sale_id, customer_id) = sale_on_account(&client, "rail", "100.00", 1).await;
    let sales = client.get("/api/sales?limit=200").await;
    let item_id = sales
        .json()
        .as_array()
        .expect("array")
        .iter()
        .find(|s| s["id"] == sale_id.as_str())
        .expect("listed")["items"][0]["id"]
        .as_str()
        .expect("item id")
        .to_string();

    let collected = client
        .post(
            "/api/payments",
            serde_json::json!({
                "party_kind": "customer",
                "party_id": customer_id,
                "payment_method_id": cash_method,
                "total": "100.00",
                "direction": "in"
            }),
        )
        .await;
    assert_eq!(collected.status(), axum::http::StatusCode::CREATED);
    let payment_id = collected.body["id"].as_str().expect("id").to_string();
    let applied = client
        .post(
            "/api/settlements",
            serde_json::json!({
                "obligation_id": obligation_id_of(&pool, &sale_id).await,
                "payment_id": payment_id,
                "amount": "100.00"
            }),
        )
        .await;
    assert_eq!(applied.status(), axum::http::StatusCode::CREATED);
    assert_eq!(balance_of(&client, cash_account).await, "100.00");
    assert_eq!(balance_of(&client, &account_id).await, "0.00");

    // **The refund through the bank, which never received a cent for this sale.**
    let wrong_rail = client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({
                "items": [{ "sale_item_id": item_id, "quantity": 1 }],
                "payment_method_id": bank_method
            }),
        )
        .await;
    assert_eq!(
        wrong_rail.status(),
        axum::http::StatusCode::CONFLICT,
        "a rail that never received this sale's money cannot refund it; body: {}",
        wrong_rail.text
    );
    assert_eq!(
        balance_of(&client, &account_id).await,
        "0.00",
        "the bank never went negative"
    );
    assert_eq!(
        balance_of(&client, cash_account).await,
        "100.00",
        "and the till kept what it received"
    );

    // The rail that DID receive the money can refund it, so the refusal above is not a veto
    // on refunding a collected sale at all.
    let right_rail = client
        .post(
            &format!("/api/sales/{sale_id}/credit-notes"),
            serde_json::json!({
                "items": [{ "sale_item_id": item_id, "quantity": 1 }],
                "payment_method_id": cash_method
            }),
        )
        .await;
    assert_eq!(
        right_rail.status(),
        axum::http::StatusCode::CREATED,
        "the rail that took the money gives it back; body: {}",
        right_rail.text
    );
    assert_eq!(balance_of(&client, cash_account).await, "0.00");
}
