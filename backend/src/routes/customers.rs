use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde::Deserialize;

use crate::AppState;
use crate::db::conflict_or_500;
use crate::models::customer::Customer;

pub fn routes() -> Router<AppState> {
    Router::new().route("/customers", get(list_customers).post(create_customer))
}

#[derive(Debug, Deserialize)]
pub struct NewCustomer {
    pub name: String,
}

async fn list_customers(State(state): State<AppState>) -> Result<Json<Vec<Customer>>, StatusCode> {
    let customers = sqlx::query_as!(
        Customer,
        "SELECT id, name, created_at FROM customers ORDER BY name"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "list_customers failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(customers))
}

async fn create_customer(
    State(state): State<AppState>,
    Json(body): Json<NewCustomer>,
) -> Result<(StatusCode, Json<Customer>), StatusCode> {
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let customer = sqlx::query_as!(
        Customer,
        "INSERT INTO customers (name) VALUES ($1) RETURNING id, name, created_at",
        name
    )
    .fetch_one(&state.db)
    .await
    .map_err(conflict_or_500)?;
    Ok((StatusCode::CREATED, Json(customer)))
}
