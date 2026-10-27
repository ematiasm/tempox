use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde_json::{Value, json};

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

async fn health(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    // A SQL probe would queue behind the exhausted pool it needs to report on.
    if state.db.size() == 0 || state.db.num_idle() == 0 {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "unavailable" })),
        )
    } else {
        (StatusCode::OK, Json(json!({ "status": "ok" })))
    }
}
