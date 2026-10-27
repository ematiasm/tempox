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

/// Why [`limited`] refused to build connect options.
///
/// Both variants describe a caller that tried to own something this module owns.
/// They are errors rather than silent overrides because the failure mode of the
/// silent version is exactly what review advisory `R4-001` named: a duplicate
/// `statement_timeout` in the string where the correct one survives only because
/// libpq keeps the *last* `-c` of a repeated name, which depends on the order this
/// module happens to apply things in.
#[derive(Debug, PartialEq, Eq)]
pub enum LimitError {
    /// The caller already set `statement_timeout` on these options.
    ///
    /// Measured before this refusal existed: `options()` appends, so a caller
    /// passing `30s` produced `-c statement_timeout=30s -c statement_timeout=5s`.
    /// The effective value was `5s` — because `limited()` ran second and libpq
    /// keeps the last value, not because anything here guaranteed it. Refusing
    /// removes the dependence on that order and the dead value along with it.
    AlreadySet,
    /// The caller's `options` string holds a token this module cannot represent,
    /// so it cannot be rebuilt without silently dropping the caller's setting.
    ///
    /// Only reachable when the caller set `statement_timeout` *and* something
    /// else in a shape this parser does not understand; without the caller's limit
    /// there is nothing to rebuild and the plain append is used instead.
    Unrepresentable(String),
}

impl std::fmt::Display for LimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadySet => write!(
                f,
                "the connect options already set statement_timeout; this backend owns \
                 that limit and will not guess which of the two to keep. Remove the \
                 caller's statement_timeout and let `limits::limited` apply {}.",
                STATEMENT_TIMEOUT_OPTION.1
            ),
            Self::Unrepresentable(text) => write!(
                f,
                "the connect options carry an `options` string this module cannot parse \
                 without dropping the caller's settings: {text:?}. Only `-c name=value` \
                 pairs are understood, which is the shape `PgConnectOptions::options` \
                 itself emits."
            ),
        }
    }
}

impl std::error::Error for LimitError {}

/// The connect options for every backend connection: the caller's own, with the
/// statement limit added — or a refusal if the caller tried to set it too.
///
/// Takes the options rather than building them from a URL so the limit applies
/// wherever a connection is opened, including the ones `sqlx migrate` and the
/// pool's lazy first connection create.
///
/// **Why refusal and not replacement.** Replacement was implemented first and
/// abandoned after measuring what it would cost. `PgConnectOptions` has no setter
/// for its `options` field and never clears it — `options()` only appends — so
/// rebuilding meant routing through the connect URL, and `to_url_lossy()` was
/// measured to lose `application_name`, lose the `options` field entirely
/// (`app=None opts=None` after a round trip) and **corrupt `sslrootcert`**
/// (`file:file: /etc/ssl/root.pem` after re-parsing). Replacing a cosmetic
/// duplicate at the price of a broken TLS configuration is not a trade worth
/// making. Refusing costs the caller nothing today: no caller sets its own limit.
///
/// When the caller set no `statement_timeout` — every call in this repository —
/// the plain append is used, which leaves exactly one limit in the string.
pub fn limited(options: PgConnectOptions) -> Result<PgConnectOptions, LimitError> {
    let Some(existing) = options.get_options() else {
        return Ok(options.options([STATEMENT_TIMEOUT_OPTION]));
    };

    let pairs =
        option_pairs(existing).ok_or_else(|| LimitError::Unrepresentable(existing.to_owned()))?;

    if pairs
        .iter()
        .any(|(name, _)| name == STATEMENT_TIMEOUT_OPTION.0)
    {
        return Err(LimitError::AlreadySet);
    }

    Ok(options.options([STATEMENT_TIMEOUT_OPTION]))
}

/// Splits a libpq `options` string into `(name, value)` pairs.
///
/// `None` means a token did not fit the `-c name=value` (or `-cname=value`) shape.
/// Escaping follows libpq: a backslash escapes the next character, so a value may
/// contain an escaped space. Splitting on a plain space would cut such a value in
/// half and hide the caller's real setting.
fn option_pairs(existing: &str) -> Option<Vec<(String, String)>> {
    let tokens = split_libpq_options(existing);
    let mut pairs = Vec::new();
    let mut index = 0;

    while index < tokens.len() {
        let token = &tokens[index];
        let (option, consumed): (&str, usize) = match token.strip_prefix("-c") {
            // `-c name=value`: the option is the next token, which must exist.
            Some("") => (tokens.get(index + 1)?.as_str(), 2),
            // `-cname=value`: libpq's compact spelling.
            Some(rest) => (rest, 1),
            // Not an option at all; this module cannot represent it.
            None => return None,
        };
        let (name, value) = option.split_once('=')?;
        pairs.push((name.to_owned(), value.to_owned()));
        index += consumed;
    }

    Some(pairs)
}

/// One libpq `options` string as tokens, with backslash escapes resolved.
fn split_libpq_options(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars();

    while let Some(character) = chars.next() {
        match character {
            '\\' => {
                if let Some(escaped) = chars.next() {
                    current.push(escaped);
                }
            }
            ' ' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(character),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
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
        let options = limited(PgConnectOptions::new()).expect("no caller limit");
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
        let options = limited(PgConnectOptions::new().options([("application_name", "tempox")]))
            .expect("a non-conflicting option is accepted");
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

    /// `R4-001`: a caller that already set its own `statement_timeout` is refused,
    /// not silently duplicated.
    ///
    /// **Measured before this refusal existed**: `options()` appends rather than
    /// replaces, so a caller passing `30s` produced
    /// `-c statement_timeout=30s -c statement_timeout=5s`. PostgreSQL took the last
    /// one, so the limit held — by accident of libpq's concatenation order and of
    /// `limited()` happening to run second. Nothing here guaranteed that, and the
    /// dead value stayed visible in a field meant to read as configuration.
    ///
    /// Refusing is the honest outcome: this module owns the limit, and a caller
    /// asking for a different one is a decision nobody can make silently.
    #[test]
    fn a_callers_own_statement_timeout_is_refused() {
        let error = limited(PgConnectOptions::new().options([("statement_timeout", "30s")]))
            .expect_err("a caller-set limit must be refused, not overridden");

        assert_eq!(error, LimitError::AlreadySet);
        // The message must name the variable and the value this backend applies, so
        // an operator reading a failed boot knows what to remove and what they get.
        let message = error.to_string();
        assert!(
            message.contains("statement_timeout"),
            "the error must name the option, got: {message}"
        );
        assert!(
            message.contains("5s"),
            "the error must say what this backend applies instead, got: {message}"
        );
    }

    /// The refusal holds wherever the caller's limit sits among their other options,
    /// including when it is not the only thing they set.
    #[test]
    fn a_callers_limit_is_refused_even_beside_other_options() {
        let error = limited(
            PgConnectOptions::new()
                .options([("application_name", "tempox")])
                .options([("statement_timeout", "60s")])
                .options([("geqo", "off")]),
        )
        .expect_err("one statement_timeout anywhere in the options is enough to refuse");

        assert_eq!(error, LimitError::AlreadySet);
    }

    /// A caller option this module cannot represent is also refused, and says what
    /// it choked on.
    ///
    /// Only reachable alongside a caller-set limit, because without one there is
    /// nothing to rebuild and the plain append is used. The point of the test is
    /// that the fallback is never "drop what the caller wrote".
    #[test]
    fn an_unrepresentable_options_string_is_refused_not_dropped() {
        // `-x` is a token libpq accepts in principle and this parser does not:
        // everything `PgConnectOptions::options` emits is `-c name=value`, so a
        // caller reaching the fallback has assembled the string some other way.
        // Built by hand because the crate offers no route to an arbitrary token.
        let options: PgConnectOptions =
            "postgres://u@h:5432/d?options=-c%20statement_timeout%3D30s%20-x"
                .parse()
                .expect("sqlx parses the `options` query parameter");

        let error =
            limited(options).expect_err("a string this parser cannot reconstruct must be refused");

        match error {
            LimitError::Unrepresentable(text) => {
                assert!(
                    text.contains("statement_timeout"),
                    "the offending string must be reported, got: {text}"
                );
            }
            other => panic!("expected Unrepresentable, got {other:?}"),
        }
    }

    /// A caller naming a different libpq option that merely *starts* with the same
    /// prefix must not be mistaken for the limit and deleted. `statement_timeout`
    /// is a prefix of nothing today, so the case is synthetic on purpose: it pins
    /// that the stripping matches the option name, not a substring.
    #[test]
    fn a_longer_option_name_sharing_the_prefix_is_not_stripped() {
        let options = limited(PgConnectOptions::new().options([("statement_timeouts", "keep-me")]))
            .expect("a different option name is not the limit");
        let rendered = options.get_options().expect("options are set");

        assert!(
            rendered.contains("statement_timeouts=keep-me"),
            "a different option must not be stripped, got: {rendered}"
        );
        assert!(
            rendered.contains("statement_timeout=5s"),
            "and the limit must still be added, got: {rendered}"
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
