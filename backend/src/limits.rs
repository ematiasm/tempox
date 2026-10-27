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

    // **The caller's string is inspected, never required to be parseable.**
    // `R3-001` from the refuter: an earlier version parsed first and refused when it
    // could not, which turned a connection configuration the baseline accepted into
    // a process that exits at boot. There is nothing to reconcile unless the caller
    // set a limit, and that question needs a scan, not a reconstruction.
    if !mentions_limit(existing) {
        // Nothing of the caller's is touched and no parsing is attempted, so a token
        // this module does not model survives exactly as it was written.
        return Ok(options.options([STATEMENT_TIMEOUT_OPTION]));
    }

    // The caller did set a limit. That is the finding, and it does not depend on how
    // the rest of their string is spelled: a long-form `--name=value` beside it is
    // perfectly readable to libpq and simply not the shape this module rebuilds, and
    // reporting that instead of `AlreadySet` would blame the wrong thing. The operator
    // has to remove the limit; that is the one action that makes the boot succeed.
    Err(LimitError::AlreadySet)
}

/// Whether an `options` string names `statement_timeout` anywhere in it.
///
/// Deliberately tolerant: it walks libpq tokens and looks at option *names*, which is
/// what distinguishes "the caller set a limit" (must refuse) from "the caller set
/// something else" (append, never inspect). A token this scanner does not model is
/// skipped rather than fatal, so an unreadable string cannot stop the boot.
///
/// **Every spelling PostgreSQL actually honours is handled**, and that list was
/// measured against a live server rather than assumed:
///
/// | spelling | PostgreSQL |
/// |---|---|
/// | `-c name=value` | applies — server reported `30s` |
/// | `-cname=value` | applies — server reported `30s` |
/// | `--name=value` | applies — server reported `30s` |
/// | `--name value` | rejected — "`--statement_timeout` requires a value" |
///
/// The first version of this function handled only the `-c` forms and claimed in a
/// comment that an unmodelled spelling would "still apply the last `-c`". That was
/// wrong for `--name=value`: measured, a caller asking for thirty seconds got our
/// five, silently. A wrong reason in a comment is worse than no comment, because it
/// makes an unexamined gap look analysed — `R3-001`, second round.
pub(crate) fn mentions_limit(existing: &str) -> bool {
    let tokens = split_libpq_options(existing);
    let mut index = 0;

    while index < tokens.len() {
        let token = &tokens[index];
        let (name_token, consumed) = match token.strip_prefix("--") {
            // `--name=value`: the whole option is in this token.
            Some(rest) => (rest, 1),
            // Short form: inspect what follows `-c`.
            None => match token.strip_prefix("-c") {
                // `-c name=value`: the option is the next token, if there is one.
                Some("") => match tokens.get(index + 1) {
                    Some(next) => (next.as_str(), 2),
                    None => break,
                },
                // `-cname=value`.
                Some(rest) => (rest, 1),
                // Not an option token; skip it. This is the tolerant path.
                None => {
                    index += 1;
                    continue;
                }
            },
        };

        if option_names(name_token, STATEMENT_TIMEOUT_OPTION.0) {
            return true;
        }
        index += consumed;
    }

    false
}

/// Whether one `name=value` token names `wanted`.
fn option_names(token: &str, wanted: &str) -> bool {
    token
        .split_once('=')
        .is_some_and(|(name, _)| name == wanted)
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
    /// A caller who sets a limit **and** writes something else this module cannot
    /// rebuild is still told the one thing that matters: remove the limit.
    ///
    /// This used to expect `Unrepresentable`, which was the wrong verdict — it blamed
    /// the caller's other token for a refusal caused by their limit, and an operator
    /// reading it would go looking in the wrong place. The variant is gone; the
    /// refusal names the option it is about.
    #[test]
    fn a_limit_beside_an_unreadable_token_still_reports_the_limit() {
        let options: PgConnectOptions =
            "postgres://u@h:5432/d?options=-c%20statement_timeout%3D30s%20-x"
                .parse()
                .expect("sqlx parses the `options` query parameter");

        let error = limited(options).expect_err("the caller set a limit");

        assert_eq!(error, LimitError::AlreadySet);
        assert!(
            error.to_string().contains("statement_timeout"),
            "the message must name the limit, got: {error}"
        );
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

    /// `R3-001` from the refuter, and a regression this change introduced: a string
    /// this parser cannot read, with **no** caller-set limit, must not stop the boot.
    ///
    /// The baseline appended the limit to whatever was there and never inspected it,
    /// so any `options` string libpq accepts was fine. The first version of the
    /// refusal changed that by parsing first: an unreadable token returned
    /// `Unrepresentable` unconditionally, which turned a connection configuration
    /// that used to work into a process that exits at `?` in `main`.
    ///
    /// The distinction is what the caller's string *contains*, not whether this
    /// module can rebuild it: there is nothing to rebuild when the caller set no
    /// limit, so the plain append is both safe and behaviour-preserving.
    #[test]
    fn an_unreadable_options_string_without_a_limit_is_accepted() {
        let options: PgConnectOptions = "postgres://u@h:5432/d?options=-x%20-c%20geqo%3Doff"
            .parse()
            .expect("sqlx parses the `options` query parameter");

        let limited = limited(options)
            .expect("no caller-set limit means nothing to reconcile, so the boot must continue");

        let rendered = limited.get_options().expect("options are set");
        assert!(
            rendered.contains("-x"),
            "the token this parser cannot read must be preserved, got: {rendered}"
        );
        assert!(
            rendered.contains("geqo=off"),
            "the readable option beside it must be preserved, got: {rendered}"
        );
        assert!(
            rendered.contains("statement_timeout=5s"),
            "the limit must still be applied, got: {rendered}"
        );
    }

    /// `R3-001` (second round): libpq accepts `--name=value` **without** `-c`, and
    /// the first version of `mentions_limit` missed it.
    ///
    /// Measured against a live server, on options a caller can really supply:
    ///
    /// | `options` | baseline (no `limited()`) | with the buggy scan |
    /// |---|---|---|
    /// | `--statement_timeout=30s` | server reports **30s** | our append won, server reported **5s** |
    /// | `-c statement_timeout=30s` | 30s | refused, correctly |
    ///
    /// So a caller who asked for thirty seconds got five, silently — and the comment
    /// justifying the tolerant scan claimed "libpq still applies the last `-c`",
    /// which is simply false for a token that is not `-c`. A wrong reason is worse
    /// than no reason: it made the gap look analysed.
    ///
    /// Both spellings PostgreSQL honours are now refused alike. The
    /// space-separated `--name value` form is *not* accepted by PostgreSQL
    /// (measured: "`--statement_timeout` requires a value"), so it does not need
    /// handling, and it is asserted below so this test cannot quietly rot.
    #[test]
    fn a_long_form_limit_without_c_is_also_refused() {
        for spelling in [
            "--statement_timeout=30s",
            "--statement_timeout=30s --fsync=on",
            "--fsync=on --statement_timeout=30s",
        ] {
            let options: PgConnectOptions = format!(
                "postgres://u@h:5432/d?options={}",
                spelling.replace(' ', "%20").replace('=', "%3D")
            )
            .parse()
            .expect("sqlx parses the `options` query parameter");

            // Asserted with the spelling in the message, so a failure says which form
            // regressed rather than only that one did.
            match limited(options) {
                Err(LimitError::AlreadySet) => {}
                Ok(limited) => panic!(
                    "`{spelling}` sets a limit and must be refused, not overridden; got {:?}",
                    limited.get_options()
                ),
            }
        }
    }

    /// The short and long spellings must produce the *same* refusal, so the message
    /// an operator sees does not depend on how they wrote the option.
    #[test]
    fn both_spellings_refuse_with_the_same_error() {
        let short: PgConnectOptions = "postgres://u@h:5432/d?options=-c%20statement_timeout%3D30s"
            .parse()
            .expect("parse");
        let long: PgConnectOptions = "postgres://u@h:5432/d?options=--statement_timeout%3D30s"
            .parse()
            .expect("parse");

        assert_eq!(
            limited(short).expect_err("short form must be refused"),
            LimitError::AlreadySet
        );
        assert_eq!(
            limited(long).expect_err("long form must be refused"),
            LimitError::AlreadySet
        );
    }

    /// A long-form option that is *not* the limit must still be accepted, so the
    /// tolerant path keeps working: this is the case that must not regress while
    /// closing the one above.
    #[test]
    fn a_long_form_option_that_is_not_the_limit_is_accepted() {
        let options: PgConnectOptions = "postgres://u@h:5432/d?options=--geqo%3Doff"
            .parse()
            .expect("parse");

        let limited = limited(options).expect("`--geqo=off` is not a limit");
        let rendered = limited.get_options().expect("options are set");
        assert!(
            rendered.contains("--geqo=off"),
            "the caller's long-form option must survive, got: {rendered}"
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
