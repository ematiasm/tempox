use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde::Deserialize;

use crate::AppState;
use crate::db::conflict_or_500;
use crate::models::supplier::Supplier;

pub fn routes() -> Router<AppState> {
    Router::new().route("/suppliers", get(list_suppliers).post(create_supplier))
}

#[derive(Debug, Deserialize)]
pub struct NewSupplier {
    pub name: String,
}

async fn list_suppliers(State(state): State<AppState>) -> Result<Json<Vec<Supplier>>, StatusCode> {
    // Ordered by name because this is what a picker renders, and the seeded
    // one-off supplier sorts among the real ones rather than being special-cased
    // out of the list: a purchase without a named supplier names it explicitly.
    let suppliers = sqlx::query_as!(
        Supplier,
        "SELECT id, name, created_at FROM suppliers ORDER BY name"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "list_suppliers failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(suppliers))
}

async fn create_supplier(
    State(state): State<AppState>,
    Json(body): Json<NewSupplier>,
) -> Result<(StatusCode, Json<Supplier>), StatusCode> {
    // Trimmed before the emptiness check: a name of spaces is empty, and storing
    // it would put an unclickable, unreadable row in every picker.
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let supplier = sqlx::query_as!(
        Supplier,
        "INSERT INTO suppliers (name) VALUES ($1) RETURNING id, name, created_at",
        name
    )
    .fetch_one(&state.db)
    .await
    .map_err(conflict_or_500)?;
    Ok((StatusCode::CREATED, Json(supplier)))
}
