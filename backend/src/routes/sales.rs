use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::models::{sale::Sale, sale_item::SaleItem};
use crate::money::{self, LineInput};

/// The customer seeded by the migration: the default for sales with no named
/// customer. Must match the fixed id in `0002_v1_sales_stock.sql`.
const WALK_IN_CUSTOMER_ID: Uuid = Uuid::from_u128(1);

/// Upper bound for a single list query, whatever the client asks for.
const MAX_LIST_LIMIT: i64 = 200;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sales", post(create_sale).get(list_sales))
        .route("/cash", get(cash_total))
}

#[derive(Debug, Deserialize)]
pub struct NewSale {
    /// Omitted means the seeded walk-in customer.
    #[serde(default)]
    pub customer_id: Option<Uuid>,
    pub items: Vec<NewSaleItem>,
}

#[derive(Debug, Deserialize)]
pub struct NewSaleItem {
    pub product_id: Uuid,
    pub quantity: i32,
    /// Omitted defaults to the product's list price.
    #[serde(default)]
    pub unit_price: Option<i64>,
}

/// A sale with its lines, as returned by the create endpoint.
#[derive(Debug, Serialize)]
pub struct SaleWithItems {
    #[serde(flatten)]
    pub sale: Sale,
    pub items: Vec<SaleItem>,
}

/// A sale in list form: the sale, its customer's name, and its lines.
#[derive(Debug, Serialize)]
pub struct SaleListItem {
    #[serde(flatten)]
    pub sale: Sale,
    pub customer_name: String,
    pub items: Vec<SaleItem>,
}

/// The cash balance: the sum of confirmed sales, in cents.
#[derive(Debug, Serialize)]
pub struct Cash {
    pub total_cents: i64,
}

#[derive(Debug, Deserialize)]
pub struct SalesQuery {
    #[serde(default)]
    pub limit: Option<i64>,
}

async fn create_sale(
    State(state): State<AppState>,
    Json(body): Json<NewSale>,
) -> Result<(StatusCode, Json<SaleWithItems>), StatusCode> {
    if body.items.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // One transaction: the sale, its lines, and the stock movements land
    // together or not at all.
    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!(?e, "tx begin failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Customer: an unknown id is a 404; omitted means the seeded walk-in.
    let customer_id = match body.customer_id {
        Some(id) => {
            let found = sqlx::query_scalar!(
                "SELECT count(*) AS \"count!\" FROM customers WHERE id = $1",
                id
            )
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "customer lookup failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            if found == 0 {
                return Err(StatusCode::NOT_FOUND);
            }
            id
        }
        None => WALK_IN_CUSTOMER_ID,
    };

    // Resolve every line against the product: snapshot name and unit, resolve
    // the price (override or list price), and build the money input.
    let mut lines: Vec<LineInput> = Vec::with_capacity(body.items.len());
    let mut snapshots: Vec<(String, String)> = Vec::with_capacity(body.items.len());
    for item in &body.items {
        let product = sqlx::query!(
            "SELECT name, list_price, unit FROM products WHERE id = $1",
            item.product_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(?e, "product lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
        let unit_price = item.unit_price.unwrap_or(product.list_price);
        snapshots.push((product.name, product.unit));
        lines.push(LineInput {
            quantity: item.quantity,
            unit_price,
        });
    }

    // Money before any write: an invalid line is a 400 with nothing persisted.
    let total = money::sale_total(&lines).ok_or(StatusCode::BAD_REQUEST)?;
    let mut line_totals = Vec::with_capacity(lines.len());
    for line in &lines {
        line_totals.push(
            money::line_total(line.quantity, line.unit_price).ok_or(StatusCode::BAD_REQUEST)?,
        );
    }

    // The sale header.
    let sale_id = sqlx::query_scalar!(
        "INSERT INTO sales (customer_id, total) VALUES ($1, $2) RETURNING id",
        customer_id,
        total
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "sale insert failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // One line and one stock movement per item.
    let mut items = Vec::with_capacity(body.items.len());
    for (idx, item) in body.items.iter().enumerate() {
        let line = &lines[idx];
        let (product_name, unit) = &snapshots[idx];
        let line_total = line_totals[idx];

        let item_id = sqlx::query_scalar!(
            "INSERT INTO sale_items
                (sale_id, product_id, product_name, quantity, unit_price, line_total, unit)
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
            sale_id,
            item.product_id,
            product_name,
            item.quantity,
            line.unit_price,
            line_total,
            unit
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(?e, "sale item insert failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        sqlx::query!(
            "INSERT INTO stock_movements (product_id, quantity, reason, sale_id)
             VALUES ($1, $2, 'sale', $3)",
            item.product_id,
            -item.quantity,
            sale_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(?e, "stock movement insert failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        items.push(SaleItem {
            id: item_id,
            sale_id,
            product_id: item.product_id,
            product_name: product_name.clone(),
            quantity: item.quantity,
            unit_price: line.unit_price,
            line_total,
            unit: unit.clone(),
        });
    }

    // Read the persisted sale so the response carries the real defaults
    // (status, created_at) instead of echoing the request.
    let sale = sqlx::query_as!(
        Sale,
        "SELECT id, customer_id, total, payment_method, status, reversed_at, created_at
         FROM sales WHERE id = $1",
        sale_id
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "sale readback failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "tx commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(SaleWithItems { sale, items })))
}

async fn list_sales(
    State(state): State<AppState>,
    Query(params): Query<SalesQuery>,
) -> Result<Json<Vec<SaleListItem>>, StatusCode> {
    let limit = params.limit.unwrap_or(50).clamp(1, MAX_LIST_LIMIT);

    let rows = sqlx::query!(
        "SELECT s.id, s.customer_id, s.total, s.payment_method, s.status, s.reversed_at,
                s.created_at, c.name AS customer_name
         FROM sales s
         JOIN customers c ON c.id = s.customer_id
         ORDER BY s.created_at DESC
         LIMIT $1",
        limit
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "list_sales failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // One extra query per sale for its lines: bounded by `limit` and simple on
    // purpose — a single json-aggregated query is the optimization, later.
    let mut sales = Vec::with_capacity(rows.len());
    for row in rows {
        let items = sqlx::query_as!(
            SaleItem,
            "SELECT id, sale_id, product_id, product_name, quantity, unit_price, line_total, unit
             FROM sale_items WHERE sale_id = $1",
            row.id
        )
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(?e, "sale items read failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        sales.push(SaleListItem {
            sale: Sale {
                id: row.id,
                customer_id: row.customer_id,
                total: row.total,
                payment_method: row.payment_method,
                status: row.status,
                reversed_at: row.reversed_at,
                created_at: row.created_at,
            },
            customer_name: row.customer_name,
            items,
        });
    }

    Ok(Json(sales))
}

async fn cash_total(State(state): State<AppState>) -> Result<Json<Cash>, StatusCode> {
    // SUM(bigint) is NUMERIC in Postgres: the cast keeps the scalar i64.
    let total = sqlx::query_scalar!(
        "SELECT COALESCE(SUM(total), 0)::bigint AS \"total!\" FROM sales WHERE status = 'confirmed'"
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "cash total failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(Cash { total_cents: total }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_in_id_matches_the_migration_seed() {
        assert_eq!(
            WALK_IN_CUSTOMER_ID.to_string(),
            "00000000-0000-0000-0000-000000000001"
        );
    }
}
