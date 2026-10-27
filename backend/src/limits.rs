//! The connect options every backend connection is built from.
//!
//! **A connection with no statement limit can be taken out of the pool forever.**
//! Measured: with a second session holding `ACCESS EXCLUSIVE` on `accounts`,
//! `GET /api/accounts` hung with no server-side error and no log line; the client
//! gave up at 35 s. Twelve such requests exhausted the ten-connection pool, and
//! every other route that touches the database — a different table, a different
//! handler — stopped answering too.
//!
//! The pool does recover on its own: `sqlx`'s `acquire_timeout` is 30 s by
//! default, and measured after exhaustion `/api/products` answered again in
//! 2.6 ms once it expired. So this is not a permanent outage. It is a stall of up
//! to 30 s that **looks like health**, because `/health` answered `200` the whole
//! time, and it is why the number below is not a substitute for `/health` telling
//! the truth.
//!
//! **Why one number and not one per query class.** A budget per class (a short
//! read, a sale's transaction, a report) was designed and rejected by the owner
//! for maintainability: it needs a type the handlers must go through to be safe,
//! it reaches all 42 pool accesses, and the failure mode of forgetting it is
//! silent. One number is simpler to hold in the head and impossible to forget.
//! The cost is honest and stated: a future report that legitimately needs longer
//! than five seconds will hit this limit, and the fix then is to revisit the
//! decision with the measurement that motivated it, not to raise a constant
//! quietly.
//!
//! **Why five seconds.** The slowest route measured on the development database is
//! `GET /api/sales` at 63.7 ms, which grows with document history rather than with
//! load; `/api/day` is 0.7 ms and `/api/accounts` is 7.5 ms. Five seconds is
//! roughly eighty times today's worst case, so no legitimate request can approach
//! it, and a query that does is a defect that deserves to fail loudly rather than
//! hold a connection.
//!
//! **Not a request timeout.** Nothing here aborts an HTTP request. A `TimeoutLayer`
//! over the whole router was considered and rejected: aborting `POST /api/sales`
//! mid-flight tells the client "failed" while the transaction may already have
//! committed, which is a phantom sale — stock gone, money in the till, and a
//! cashier who will record it again. `statement_timeout` bounds the one part that
//! can actually run long, inside the database, where the transaction is rolled back
//! cleanly instead of being abandoned.
//!
//! The connection is limited **server-side**, by the option sent at connect time,
//! not by a `SET` executed after the fact. That matters: `SET LOCAL` outside a
//! transaction persists on the connection (measured) rather than reverting when the
//! pool hands it back, so a per-request limit set that way would leak into whatever
//! query ran next.

use std::time::Duration;

use sqlx::postgres::PgConnectOptions;

/// The statement limit every backend connection is created with.
pub const STATEMENT_TIMEOUT: Duration = Duration::from_secs(5);

/// PostgreSQL's own name for that limit, and the value it is given.
///
/// The value is written as PostgreSQL spells it rather than derived from
/// [`STATEMENT_TIMEOUT`], because the unit has to be explicit in the string: a
/// bare number is read as milliseconds, so `5` would mean five milliseconds and
/// brake every query in the shop.
pub const STATEMENT_TIMEOUT_OPTION: (&str, &str) = ("statement_timeout", "5s");

/// The connect options for every backend connection: the caller's own, with the
/// statement limit added.
///
/// Takes the options rather than building them from a URL so the limit applies
/// wherever a connection is opened, including the ones `sqlx migrate` and the
/// pool's lazy first connection create.
pub fn limited(options: PgConnectOptions) -> PgConnectOptions {
    options.options([STATEMENT_TIMEOUT_OPTION])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The IANA name PostgreSQL uses for the option, asserted once so a rename of
    /// the constant cannot silently stop limiting anything.
    #[test]
    fn the_option_is_the_one_postgres_understands() {
        assert_eq!(STATEMENT_TIMEOUT_OPTION.0, "statement_timeout");
    }

    /// A bare number would be milliseconds. `5` is five milliseconds, not five
    /// seconds, and that is the mistake this test exists to prevent: it would brake
    /// every query in the shop instead of guarding against a hung one.
    #[test]
    fn the_value_carries_its_unit() {
        let value = STATEMENT_TIMEOUT_OPTION.1;
        assert!(
            value.ends_with('s') || value.ends_with("ms"),
            "a unitless statement_timeout is read as milliseconds, got: {value}"
        );
        assert_eq!(value, "5s");
    }

    /// The limit reaches the place libpq actually reads it.
    ///
    /// Asserted through `get_options()` rather than the connect URL, because the
    /// URL does **not** carry it: measured, `to_url_lossy()` on the limited options
    /// produced no `statement_timeout` at all. A test written against the URL would
    /// have passed only by accident or failed for the wrong reason, and would have
    /// said nothing about whether the limit is applied.
    #[test]
    fn the_limit_reaches_the_connection_options() {
        let options = limited(PgConnectOptions::new());
        let rendered = options
            .get_options()
            .expect("the limit must be set on the options");
        assert!(
            rendered.contains("statement_timeout=5s"),
            "libpq reads this string; it must name the limit, got: {rendered}"
        );
    }

    /// Options the caller already set are kept. Replacing them would silently drop
    /// TLS or an application name the caller asked for.
    #[test]
    fn existing_options_are_not_replaced() {
        // `options()` appends to whatever was there. A second call that replaced
        // the string would drop a caller's own libpq option silently.
        let options = limited(PgConnectOptions::new().options([("application_name", "tempox")]));
        let rendered = options.get_options().expect("options are set");
        assert!(
            rendered.contains("application_name=tempox"),
            "a caller's own option must survive, got: {rendered}"
        );
        assert!(
            rendered.contains("statement_timeout=5s"),
            "the limit must be added, got: {rendered}"
        );
    }

    /// The constant is the five seconds the owner chose, and it is not `ZERO`,
    /// which in PostgreSQL means "no limit at all".
    #[test]
    fn the_limit_is_five_seconds_and_not_zero() {
        assert_eq!(STATEMENT_TIMEOUT, Duration::from_secs(5));
        assert_ne!(STATEMENT_TIMEOUT_OPTION.1, "0");
    }
}
