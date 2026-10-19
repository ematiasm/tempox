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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub database_url: String,
    pub port: u16,
    pub shop_timezone: chrono_tz::Tz,
}

/// The three defaults, named so the fallback behaviour is stated once and is
/// visible in one place instead of embedded in three `unwrap_or_else` calls.
const DEFAULT_DATABASE_URL: &str = "postgres://tempox:tempox@localhost:5432/tempox";
const DEFAULT_PORT: u16 = 3001;
const DEFAULT_SHOP_TIMEZONE: &str = "America/Argentina/Buenos_Aires";

impl Config {
    /// Read the configuration from the process environment.
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_source(|key| std::env::var(key).ok())
    }

    /// Read the configuration from an arbitrary key lookup.
    ///
    /// The behaviour is exactly what `main` did inline before this module existed:
    ///
    /// - a missing `DATABASE_URL` falls back to the local development database;
    /// - a missing or unparseable `BACKEND_PORT` falls back to `3001`;
    /// - a missing `SHOP_TIMEZONE` falls back to the shop's zone;
    /// - a *present but invalid* `SHOP_TIMEZONE` is an error, because silently
    ///   falling back to UTC would file every late-evening sale under the wrong
    ///   day — the defect `/api/day` exists to prevent.
    pub fn from_source<F>(lookup: F) -> anyhow::Result<Self>
    where
        F: Fn(&str) -> Option<String>,
    {
        let database_url =
            lookup("DATABASE_URL").unwrap_or_else(|| DEFAULT_DATABASE_URL.to_owned());

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
    fn empty_environment_yields_the_documented_defaults() {
        let config = Config::from_source(source(&[])).expect("defaults are valid");

        assert_eq!(
            config.database_url, DEFAULT_DATABASE_URL,
            "a missing DATABASE_URL still falls back, which is the behaviour this \
             slice deliberately preserves"
        );
        assert_eq!(config.port, DEFAULT_PORT);
        assert_eq!(config.shop_timezone.to_string(), DEFAULT_SHOP_TIMEZONE);
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
        let config = Config::from_source(source(&[("BACKEND_PORT", "not-a-port")]))
            .expect("an unparseable port is not fatal");

        assert_eq!(config.port, DEFAULT_PORT);
    }

    #[test]
    fn a_port_above_u16_is_rejected_by_the_parser_and_falls_back() {
        let config = Config::from_source(source(&[("BACKEND_PORT", "70000")]))
            .expect("out of range is not fatal either");

        assert_eq!(config.port, DEFAULT_PORT);
    }

    #[test]
    fn an_invalid_timezone_is_an_error_that_names_the_variable() {
        // This is the one value that must NOT fall back: a wrong zone files sales
        // under the wrong day, silently.
        let error = Config::from_source(source(&[("SHOP_TIMEZONE", "Mars/Olympus")]))
            .expect_err("an invalid timezone must fail the boot");

        let message = error.to_string();
        assert!(
            message.contains("SHOP_TIMEZONE"),
            "the message must name the variable, got: {message}"
        );
    }

    #[test]
    fn a_valid_timezone_that_differs_from_the_default_is_kept() {
        let config = Config::from_source(source(&[("SHOP_TIMEZONE", "UTC")]))
            .expect("UTC is a valid IANA name");

        assert_eq!(config.shop_timezone.to_string(), "UTC");
    }

    #[test]
    fn the_bind_address_is_port_zero_safe() {
        // F4 spawns servers on an OS-assigned port; the config has to carry `0`
        // through unchanged so the caller can then read the assigned one.
        let config =
            Config::from_source(source(&[("BACKEND_PORT", "0")])).expect("0 is a valid port");

        assert_eq!(config.port, 0);
        assert_eq!(config.bind_addr().port(), 0);
    }
}
