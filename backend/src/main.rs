use std::net::SocketAddr;

use axum::Router;
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

mod day;
mod db;
mod models;
mod money;
mod routes;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    /// The shop's timezone. A shop day is a local calendar fact, and the
    /// machine's own timezone is not a reliable statement about where the shop
    /// is — a rebuilt PC defaults to UTC. Configured once, read by `/api/day`.
    pub shop_timezone: chrono_tz::Tz,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://skilluv:skilluv@localhost:5432/skilluv".into());
    let port: u16 = std::env::var("BACKEND_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3001);

    let db = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&db).await?;

    // Where the shop is, for the calendar. A named zone rather than a fixed
    // offset, so a future DST rule travels with the tz database instead of with
    // this code. An unparseable zone fails the boot: silently falling back to
    // UTC would file every late-evening sale under the wrong day, which is the
    // defect `/api/day` exists to prevent.
    let shop_timezone: chrono_tz::Tz = std::env::var("SHOP_TIMEZONE")
        .unwrap_or_else(|_| "America/Argentina/Buenos_Aires".into())
        .parse()
        .map_err(|e| anyhow::anyhow!("SHOP_TIMEZONE is not an IANA timezone name: {e}"))?;

    let state = AppState { db, shop_timezone };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .merge(routes::router())
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(cors);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("backend listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
