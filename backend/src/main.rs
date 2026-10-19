//! The Tempox backend binary.
//!
//! Boot only: read the configuration, open the pool, apply migrations, build the
//! app and serve it. Everything that decides *what* the app is lives in the
//! library, so it can be constructed and driven by a test without binding a
//! socket.

use sqlx::postgres::PgPoolOptions;
use tempox::{AppState, Config, build_app};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env()?;

    let db = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&db).await?;

    let state = AppState {
        db,
        shop_timezone: config.shop_timezone,
    };

    let app = build_app(state);

    let addr = config.bind_addr();
    let listener = tokio::net::TcpListener::bind(addr).await?;

    // The address the OS actually assigned, not the one that was requested. With
    // `BACKEND_PORT=0` the requested port is zero and says nothing about where the
    // server is listening; a caller that cannot read the real one cannot connect.
    let bound = listener.local_addr()?;
    tracing::info!("backend listening on http://{bound}");

    axum::serve(listener, app).await?;
    Ok(())
}
