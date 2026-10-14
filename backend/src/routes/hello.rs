use axum::{Json, Router, extract::Query, routing::get};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/hello", get(hello))
}

#[derive(Debug, Deserialize)]
struct HelloParams {
    #[serde(default)]
    name: Option<String>,
}

async fn hello(Query(q): Query<HelloParams>) -> Json<Value> {
    let name = q.name.as_deref().unwrap_or("Skilluv");
    Json(json!({
        "message": format!("Hello {name}!"),
        "server_time": Utc::now().to_rfc3339(),
    }))
}
