use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::AppState;
use crate::db::conflict_or_500;

/// The units a product's quantities are expressed in.
const UNITS: [&str; 3] = ["unit", "g", "ml"];

pub fn routes() -> Router<AppState> {
    Router::new().route("/products", get(list_products).post(create_product))
}

/// A product with its current stock — derived from the movement ledger, never
/// a stored field.
#[derive(Debug, Serialize, FromRow)]
pub struct ProductWithStock {
    pub id: Uuid,
    pub name: String,
    pub list_price: i64,
    pub unit: String,
    pub stock: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct NewProduct {
    pub name: String,
    pub list_price: i64,
    /// Defaults to `"unit"`.
    #[serde(default)]
    pub unit: Option<String>,
    /// Seeds an `initial` movement when greater than zero; otherwise stock is 0.
    #[serde(default)]
    pub initial_stock: Option<i32>,
}

async fn list_products(
    State(state): State<AppState>,
) -> Result<Json<Vec<ProductWithStock>>, StatusCode> {
    let products = sqlx::query_as!(
        ProductWithStock,
        "SELECT p.id, p.name, p.list_price, p.unit, p.created_at,
                COALESCE(SUM(m.quantity), 0)::bigint AS \"stock!\"
         FROM products p
         LEFT JOIN stock_movements m ON m.product_id = p.id
         GROUP BY p.id
         ORDER BY p.name"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "list_products failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(products))
}

async fn create_product(
    State(state): State<AppState>,
    Json(body): Json<NewProduct>,
) -> Result<(StatusCode, Json<ProductWithStock>), StatusCode> {
    let name = body.name.trim().to_string();
    if name.is_empty() || body.list_price < 0 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let unit = body.unit.unwrap_or_else(|| "unit".to_string());
    if !UNITS.contains(&unit.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    if body.initial_stock.is_some_and(|q| q < 0) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!(?e, "tx begin failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let id = sqlx::query_scalar!(
        "INSERT INTO products (name, list_price, unit) VALUES ($1, $2, $3) RETURNING id",
        name,
        body.list_price,
        unit
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(conflict_or_500)?;

    // Omitted or zero initial stock means stock starts at 0 with no movement row.
    if let Some(initial) = body.initial_stock.filter(|q| *q > 0) {
        sqlx::query!(
            "INSERT INTO stock_movements (product_id, quantity, reason)
             VALUES ($1, $2, 'initial')",
            id,
            initial
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(?e, "initial movement insert failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

    // Respond with the same derived-stock projection the list endpoint uses.
    let product = sqlx::query_as!(
        ProductWithStock,
        "SELECT p.id, p.name, p.list_price, p.unit, p.created_at,
                COALESCE(SUM(m.quantity), 0)::bigint AS \"stock!\"
         FROM products p
         LEFT JOIN stock_movements m ON m.product_id = p.id
         WHERE p.id = $1
         GROUP BY p.id",
        id
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "product readback failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "tx commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(product)))
}
