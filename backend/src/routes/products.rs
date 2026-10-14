use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::AppState;
use crate::db::conflict_or_500;

/// The units a product's quantities are expressed in.
const UNITS: [&str; 3] = ["unit", "g", "ml"];

/// Upper bound for a single catalog query, whatever the client asks for.
const MAX_LIST_LIMIT: i64 = 200;

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

#[derive(Debug, Deserialize)]
pub struct ProductsQuery {
    /// Case-insensitive substring match on `name`. Absent lists everything.
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

async fn list_products(
    State(state): State<AppState>,
    Query(params): Query<ProductsQuery>,
) -> Result<Json<Vec<ProductWithStock>>, StatusCode> {
    // An empty or whitespace-only `q` is not a search for the empty string:
    // it is the caller asking for the catalog, which is what absent `q` means.
    let needle = params
        .q
        .map(|q| q.trim().to_string())
        .filter(|q| !q.is_empty());
    let limit = params.limit.unwrap_or(50).clamp(1, MAX_LIST_LIMIT);
    // A negative offset is not a page request: OFFSET refuses it in Postgres,
    // and turning that into a 500 would blame the server for a bad query.
    let offset = params.offset.unwrap_or(0).max(0);

    // One statement for both cases: a NULL needle is not a filter. The match is
    // on the whole name, not its prefix, and `%`/`_` in the caller's text are
    // literal — without the escape an `_` would match any character and a `%`
    // would match the entire catalog.
    let rows = sqlx::query_as!(
        ProductWithStock,
        r#"SELECT p.id, p.name, p.list_price, p.unit, p.created_at,
                  COALESCE(SUM(m.quantity), 0)::bigint AS "stock!"
           FROM products p
           LEFT JOIN stock_movements m ON m.product_id = p.id
           WHERE ($1::text IS NULL OR p.name ILIKE '%' || $1 || '%' ESCAPE '\')
           GROUP BY p.id
           ORDER BY p.name
           LIMIT $2 OFFSET $3"#,
        needle.as_deref().map(escape_like),
        limit,
        offset
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "list_products failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(rows))
}

/// Neutralizes the LIKE metacharacters in caller-supplied text, so a search for
/// `50_ml` looks for that literal name instead of matching `50Xml` too. Pairs
/// with `ESCAPE '\'` in the query.
fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
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
