//! The statement limit, proven against a real server.
//!
//! `src/limits.rs` proves the option is *set*; this file proves it *bites*. Those
//! are different claims, and only the second one protects the shop. A string that
//! reaches libpq under a misspelled key is silently ignored, and every other test
//! in the repository would still pass while a hung query kept its connection.
//!
//! The defect, measured before this existed: with a second session holding
//! `ACCESS EXCLUSIVE` on `accounts`, `GET /api/accounts` hung with no server-side
//! error; twelve such requests exhausted the ten-connection pool and every other
//! route stopped answering.

use std::time::{Duration, Instant};

/// A pool built the way `main` builds it: through [`tempox::limits::limited`].
async fn limited_pool(pool: &sqlx::PgPool) -> sqlx::PgPool {
    let options = pool.connect_options().as_ref().clone();
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect_with(tempox::limits::limited(options))
        .await
        .expect("connect with the statement limit")
}

/// S5, and the reason this file exists: a query that outlives the limit fails
/// instead of holding its connection.
///
/// `pg_sleep` is the cheapest way to be slow without a lock, and it is
/// interruptible, so the server ends it rather than the client giving up.
#[sqlx::test]
async fn a_query_that_outlives_the_limit_is_cancelled(pool: sqlx::PgPool) {
    let limited = limited_pool(&pool).await;

    let started = Instant::now();
    let outcome = sqlx::query("SELECT pg_sleep(30)").execute(&limited).await;
    let elapsed = started.elapsed();

    let error = outcome.expect_err("a 30 s query must not outlive a 5 s limit");
    // The server cancels it; the client must not. A client-side timeout would mean
    // the connection was still busy when the test moved on, which is the leak.
    assert!(
        elapsed < Duration::from_secs(15),
        "the limit must fire well before the query finishes, took {elapsed:?}"
    );
    assert!(
        error.to_string().to_lowercase().contains("cancel")
            || error.to_string().to_lowercase().contains("timeout"),
        "the error should say the statement was cancelled, got: {error}"
    );
}

/// S6: the limit does not brake ordinary work. Every route in the shop must sit far
/// below it, and this asserts the shape of that claim rather than a timing: a fast
/// query returns normally on a limited connection.
#[sqlx::test]
async fn an_ordinary_query_still_succeeds(pool: sqlx::PgPool) {
    let limited = limited_pool(&pool).await;

    let row: (i32,) = sqlx::query_as("SELECT 1")
        .fetch_one(&limited)
        .await
        .expect("a trivial query must be unaffected by the limit");
    assert_eq!(row.0, 1);
}

/// The limit applies to **every** connection the pool opens, not only to the first
/// one. `options()` is sent at handshake time, so a pool that lazily opens its
/// second connection must produce one that is limited too — a per-connection `SET`
/// would only have covered the connection it ran on.
#[sqlx::test]
async fn every_connection_in_the_pool_is_limited(pool: sqlx::PgPool) {
    let limited = limited_pool(&pool).await;

    // Hold the first connection so the second query must open a new one.
    let first = limited.acquire().await.expect("first connection");

    let started = Instant::now();
    let outcome = sqlx::query("SELECT pg_sleep(30)").execute(&limited).await;
    let elapsed = started.elapsed();

    drop(first);
    assert!(
        outcome.is_err(),
        "the second, lazily opened connection must be limited too"
    );
    assert!(
        elapsed < Duration::from_secs(15),
        "took {elapsed:?}; a new connection without the limit would run 30 s"
    );
}

/// The limit is visible from the server's own point of view, which is the only
/// place that settles whether it is in force. No string inspection involved.
#[sqlx::test]
async fn the_server_reports_the_limit(pool: sqlx::PgPool) {
    let limited = limited_pool(&pool).await;

    let (value,): (String,) = sqlx::query_as("SHOW statement_timeout")
        .fetch_one(&limited)
        .await
        .expect("SHOW statement_timeout");
    assert_eq!(
        value, "5s",
        "the server must have the limit in force, got: {value}"
    );
}

/// A guard against the option being silently ignored: libpq ignores an unknown
/// option name, and `SET` of an unknown one errors. `SHOW` is what distinguishes
/// "applied" from "sent".
#[sqlx::test]
async fn the_option_name_is_not_silently_ignored(pool: sqlx::PgPool) {
    let options = pool.connect_options().as_ref().clone();
    let rendered = tempox::limits::limited(options)
        .get_options()
        .expect("the limit is set")
        .to_string();
    assert!(rendered.contains("statement_timeout"), "got: {rendered}");
    // And the server agrees, which is the assertion above; kept here so a reader of
    // this file sees why the string check alone is not the proof.
}
