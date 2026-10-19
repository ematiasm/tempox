//! Tempox backend, as a library.
//!
//! This module exists so the router and the configuration can be constructed
//! without running `main`: a test can build the app and drive it in-process, and
//! the boot sequence keeps only the things that cannot be tested — binding a
//! socket and serving.
//!
//! `main.rs` is the binary and does nothing else. Everything that decides *what*
//! the app is lives here.

use std::net::SocketAddr;

use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

pub mod day;
pub mod db;
pub mod models;
pub mod money;
pub mod routes;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    /// The shop's timezone. A shop day is a local calendar fact, and the
    /// machine's own timezone is not a reliable statement about where the shop
    /// is — a rebuilt PC defaults to UTC. Configured once, read by `/api/day`.
    pub shop_timezone: chrono_tz::Tz,
}

/// Everything the process reads from its environment at boot.
///
/// Built through [`Config::from_source`] rather than straight from
/// `std::env::var`, so it can be tested without touching the global environment:
/// in edition 2024 `std::env::set_var` is `unsafe`, and a test that mutates the
/// process environment races every other test in the binary. Passing a lookup
/// function is the seam that makes the values testable at all.
#[derive(Clone, PartialEq, Eq)]
pub struct Config {
    pub database_url: String,
    pub port: u16,
    pub shop_timezone: chrono_tz::Tz,
}

/// A hand-written `Debug`, because the derived one prints `database_url`, and a
/// connection string carries the password. A single `tracing::debug!("{:?}",
/// config)` would then write it into the logs — the same leak this repository
/// spent a review round removing from `scripts/test-db.sh`. The URL is reduced to
/// its scheme, host and database; the credentials are never formatted.
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        /// Everything after the last `@` in the authority, i.e. without
        /// `user:password`. A string with no `@` is returned unchanged.
        fn without_credentials(url: &str) -> &str {
            match url.split_once("@") {
                Some((_, rest)) => rest,
                None => url,
            }
        }

        f.debug_struct("Config")
            .field("database_url", &without_credentials(&self.database_url))
            .field("port", &self.port)
            .field("shop_timezone", &self.shop_timezone)
            .finish()
    }
}

/// The two values that still fall back, named so the behaviour is stated once
/// instead of being embedded in two `unwrap_or_else` calls.
///
/// `DATABASE_URL` is deliberately **not** here. A missing one used to fall back to
/// a concrete local URL, which is the same class of defect as the template's
/// invented `skilluv` credential: a default that connects somewhere real, in code,
/// with nothing saying it happened. Missing configuration must name itself, so
/// that value is an error instead. Neither of these can connect a process to the
/// wrong database, which is why they keep their forgiving behaviour.
const DEFAULT_PORT: u16 = 3001;
const DEFAULT_SHOP_TIMEZONE: &str = "America/Argentina/Buenos_Aires";

impl Config {
    /// Read the configuration from the process environment.
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_source(|key| std::env::var(key).ok())
    }

    /// Read the configuration from an arbitrary key lookup.
    ///
    /// Two values fall back, one does not, and the difference is the point:
    ///
    /// - a missing `DATABASE_URL` is an **error** that names the variable. It is
    ///   the one value that decides which database the process writes to, and a
    ///   silent default there means a misplaced environment reaches real data
    ///   without a word;
    /// - a missing or unparseable `BACKEND_PORT` falls back to `3001`;
    /// - a missing `SHOP_TIMEZONE` falls back to the shop's zone;
    /// - a *present but invalid* `SHOP_TIMEZONE` is an error, because silently
    ///   falling back to UTC would file every late-evening sale under the wrong
    ///   day — the defect `/api/day` exists to prevent.
    pub fn from_source<F>(lookup: F) -> anyhow::Result<Self>
    where
        F: Fn(&str) -> Option<String>,
    {
        let database_url = lookup("DATABASE_URL").ok_or_else(|| {
            anyhow::anyhow!(
                "DATABASE_URL is not set. Copy .env.example to .env, or export it. \
                 It is required: this process must be told which database to use, \
                 because guessing one means writing to whatever happens to be there."
            )
        })?;

        let port = lookup("BACKEND_PORT")
            .and_then(|raw| raw.parse().ok())
            .unwrap_or(DEFAULT_PORT);

        let raw_zone = lookup("SHOP_TIMEZONE").unwrap_or_else(|| DEFAULT_SHOP_TIMEZONE.to_owned());
        let shop_timezone: chrono_tz::Tz = raw_zone
            .parse()
            .map_err(|e| anyhow::anyhow!("SHOP_TIMEZONE is not an IANA timezone name: {e}"))?;

        Ok(Self {
            database_url,
            port,
            shop_timezone,
        })
    }

    /// The address to bind. `0.0.0.0` on every interface, as before: the binary
    /// runs in a container and must be reachable from outside it.
    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::from(([0, 0, 0, 0], self.port))
    }
}

/// The application, assembled and ready to serve.
///
/// Keeping this separate from `main` is what lets a test drive the real router —
/// routing, extractors, status codes, response bodies, the CORS and trace layers —
/// without a socket and without a second process.
pub fn build_app(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .merge(routes::router())
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one value a config cannot be built without, now that B made it required.
    const A_DATABASE_URL: (&str, &str) = ("DATABASE_URL", "postgres://u:p@localhost:5432/db_test");

    /// A lookup over a fixed set of pairs. `None` for anything not listed.
    fn source<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn a_missing_database_url_is_an_error_that_names_the_variable() {
        // B: a missing DATABASE_URL used to fall back silently to a concrete local
        // URL. That is the same class of defect as the template's invented
        // `skilluv` credential: a default that connects somewhere real, in code,
        // with nothing saying it happened. Missing configuration must name itself.
        let error = Config::from_source(source(&[]))
            .expect_err("a missing DATABASE_URL must fail the boot");

        let message = error.to_string();
        assert!(
            message.contains("DATABASE_URL"),
            "the message must name the variable, got: {message}"
        );
    }

    #[test]
    fn the_other_values_still_fall_back_when_database_url_is_present() {
        // B changes exactly one value. The port and the zone keep their forgiving
        // behaviour, because neither can connect the process to the wrong database.
        let config = Config::from_source(source(&[(
            "DATABASE_URL",
            "postgres://someone:secret@db.example:6543/shop",
        )]))
        .expect("a present DATABASE_URL is enough to boot");

        assert_eq!(config.port, DEFAULT_PORT);
        assert_eq!(config.shop_timezone.to_string(), DEFAULT_SHOP_TIMEZONE);
    }

    #[test]
    fn debug_does_not_print_the_credentials() {
        // A connection string carries the password. The derived `Debug` would print
        // it, and one `tracing::debug!("{:?}", config)` would put it in the logs.
        let config = Config {
            database_url: "postgres://someone:LEAKME@db.example:6543/shop".into(),
            port: 3001,
            shop_timezone: "UTC".parse().expect("UTC is a valid zone"),
        };

        let printed = format!("{config:?}");
        assert!(
            !printed.contains("LEAKME"),
            "Debug must not print the password, got: {printed}"
        );
        assert!(
            printed.contains("db.example:6543/shop"),
            "Debug should still say which database, got: {printed}"
        );
    }

    #[test]
    fn debug_leaves_a_url_without_credentials_alone() {
        let config = Config {
            database_url: "postgres://db.example:5432/shop".into(),
            port: 3001,
            shop_timezone: "UTC".parse().expect("UTC is a valid zone"),
        };

        let printed = format!("{config:?}");
        assert!(
            printed.contains("db.example:5432/shop"),
            "a URL with no userinfo must survive intact, got: {printed}"
        );
    }

    #[test]
    fn every_value_is_read_from_the_source() {
        let config = Config::from_source(source(&[
            (
                "DATABASE_URL",
                "postgres://someone:secret@db.example:6543/shop",
            ),
            ("BACKEND_PORT", "3099"),
            ("SHOP_TIMEZONE", "Europe/Madrid"),
        ]))
        .expect("explicit values are valid");

        assert_eq!(
            config.database_url,
            "postgres://someone:secret@db.example:6543/shop"
        );
        assert_eq!(config.port, 3099);
        assert_eq!(config.shop_timezone.to_string(), "Europe/Madrid");
    }

    #[test]
    fn an_unparseable_port_falls_back_instead_of_failing() {
        // Preserved from the original: `BACKEND_PORT=abc` used to be ignored, not
        // fatal. A port is easy to get wrong and costs nothing to recover from.
        let config = Config::from_source(source(&[A_DATABASE_URL, ("BACKEND_PORT", "not-a-port")]))
            .expect("an unparseable port is not fatal");

        assert_eq!(config.port, DEFAULT_PORT);
    }

    #[test]
    fn a_port_above_u16_is_rejected_by_the_parser_and_falls_back() {
        let config = Config::from_source(source(&[A_DATABASE_URL, ("BACKEND_PORT", "70000")]))
            .expect("out of range is not fatal either");

        assert_eq!(config.port, DEFAULT_PORT);
    }

    #[test]
    fn an_invalid_timezone_is_an_error_that_names_the_variable() {
        // This is the one value that must NOT fall back: a wrong zone files sales
        // under the wrong day, silently.
        let error =
            Config::from_source(source(&[A_DATABASE_URL, ("SHOP_TIMEZONE", "Mars/Olympus")]))
                .expect_err("an invalid timezone must fail the boot");

        let message = error.to_string();
        assert!(
            message.contains("SHOP_TIMEZONE"),
            "the message must name the variable, got: {message}"
        );
    }

    #[test]
    fn a_valid_timezone_that_differs_from_the_default_is_kept() {
        let config = Config::from_source(source(&[A_DATABASE_URL, ("SHOP_TIMEZONE", "UTC")]))
            .expect("UTC is a valid IANA name");

        assert_eq!(config.shop_timezone.to_string(), "UTC");
    }

    #[test]
    fn the_bind_address_is_port_zero_safe() {
        // F4 spawns servers on an OS-assigned port; the config has to carry `0`
        // through unchanged so the caller can then read the assigned one.
        let config = Config::from_source(source(&[A_DATABASE_URL, ("BACKEND_PORT", "0")]))
            .expect("0 is a valid port");

        assert_eq!(config.port, 0);
        assert_eq!(config.bind_addr().port(), 0);
    }
}
