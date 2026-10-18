use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing::post,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;
use crate::models::purchase::{Purchase, PurchaseItem, PurchaseWithItems};
use crate::money;

/// The seeded one-off supplier. Its id is a constant here for the same reason
/// `WALK_IN_CUSTOMER_ID` is a constant in `sales.rs`: an omitted supplier must
/// resolve without a lookup, and the migration seeds exactly this row.
const ONE_OFF_SUPPLIER_ID: Uuid = Uuid::from_u128(2);

pub fn routes() -> Router<AppState> {
    Router::new().route("/purchases", post(create_purchase).get(list_purchases))
}

/// Upper bound for a single list query, whatever the client asks for. Mirrors
/// the sales ledger's limit, for the same reason: an unbounded list is a way to
/// make the server do the client's paging badly.
const MAX_LIST_LIMIT: i64 = 200;

#[derive(Debug, Deserialize)]
pub struct PurchasesQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
    /// Inclusive lower bound on `created_at`. Absent means no lower bound.
    #[serde(default)]
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    /// Exclusive upper bound on `created_at`. Absent means no upper bound.
    /// Exclusive is what makes adjacent days partition the ledger instead of
    /// double-counting a document recorded at midnight.
    #[serde(default)]
    pub to: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct NewPurchase {
    /// Omitted means the seeded one-off supplier.
    #[serde(default)]
    pub supplier_id: Option<Uuid>,
    pub items: Vec<NewPurchaseItem>,
}

#[derive(Debug, Deserialize)]
pub struct NewPurchaseItem {
    pub product_id: Uuid,
    pub quantity: i32,
    /// Omitted defaults to the product's list price. A decimal string, not a
    /// JSON number: a float would lose the cent.
    #[serde(default)]
    pub unit_price: Option<Decimal>,
}

async fn create_purchase(
    State(state): State<AppState>,
    Json(body): Json<NewPurchase>,
) -> Result<(StatusCode, Json<PurchaseWithItems>), StatusCode> {
    if body.items.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // One transaction: the purchase, its lines, and the stock movements land
    // together or not at all. This is what makes a rejected purchase write
    // nothing — there is no partial state to clean up because there is no
    // partial state to observe.
    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!(?e, "tx begin failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Supplier: an unknown id is a 404; omitted means the seeded one.
    let supplier_id = match body.supplier_id {
        Some(id) => {
            let found = sqlx::query_scalar!(
                "SELECT count(*) AS \"count!\" FROM suppliers WHERE id = $1",
                id
            )
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "supplier lookup failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            if found == 0 {
                return Err(StatusCode::NOT_FOUND);
            }
            id
        }
        None => ONE_OFF_SUPPLIER_ID,
    };

    // Resolve every line against the product: snapshot name and unit, resolve
    // the price (override or list price), and build the money input.
    //
    // Every line is resolved before anything is written, so an unknown product
    // on the *last* line still leaves the earlier ones unwritten. Checking per
    // line inside the insert loop would have written the valid prefix and then
    // returned 404 — the mixed case in the test measures exactly this.
    let mut lines: Vec<money::LineInput> = Vec::with_capacity(body.items.len());
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
        lines.push(money::LineInput {
            quantity: item.quantity,
            unit_price,
        });
    }

    // Money before any write: an invalid line is a 400 with nothing persisted.
    // `sale_total` is the same arithmetic a sale uses — a purchase total is a
    // sum of lines, not a different kind of number.
    let total = money::sale_total(&lines).ok_or(StatusCode::BAD_REQUEST)?;
    let mut line_totals = Vec::with_capacity(lines.len());
    for line in &lines {
        line_totals.push(
            money::line_total(line.quantity, line.unit_price).ok_or(StatusCode::BAD_REQUEST)?,
        );
    }

    let purchase_id = sqlx::query_scalar!(
        "INSERT INTO purchases (supplier_id, total) VALUES ($1, $2) RETURNING id",
        supplier_id,
        total
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "purchase insert failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut items = Vec::with_capacity(body.items.len());
    for (idx, item) in body.items.iter().enumerate() {
        let line = &lines[idx];
        let (product_name, unit) = &snapshots[idx];
        let line_total = line_totals[idx];

        let item_id = sqlx::query_scalar!(
            "INSERT INTO purchase_items
                (purchase_id, product_id, product_name, quantity, unit_price, line_total, unit)
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
            purchase_id,
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
            tracing::error!(?e, "purchase item insert failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        // The sign is the slice in one character: `+quantity`, where the sale
        // path writes `-item.quantity`. Stock comes in. The database also
        // refuses a negative purchase movement, so this is belt and braces —
        // but a wrong sign here would move stock the wrong way while every
        // constraint still passed, which is why the test measures the delta.
        sqlx::query!(
            "INSERT INTO stock_movements (product_id, quantity, reason, purchase_id)
             VALUES ($1, $2, 'purchase', $3)",
            item.product_id,
            item.quantity,
            purchase_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(?e, "stock movement insert failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        items.push(PurchaseItem {
            id: item_id,
            purchase_id,
            product_id: item.product_id,
            product_name: product_name.clone(),
            quantity: item.quantity,
            unit_price: line.unit_price,
            line_total,
            unit: unit.clone(),
        });
    }

    // Read the persisted header so the response carries the real `created_at`
    // instead of echoing the request, and `round(...,2)` so the amount leaves as
    // `70.00` rather than `70.000000` (see 0006 for why the column declares 6).
    let purchase = sqlx::query_as!(
        Purchase,
        "SELECT id, supplier_id, round(total, 2) AS \"total!\", created_at
         FROM purchases WHERE id = $1",
        purchase_id
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "purchase readback failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "tx commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::CREATED,
        Json(PurchaseWithItems { purchase, items }),
    ))
}

async fn list_purchases(
    State(state): State<AppState>,
    Query(params): Query<PurchasesQuery>,
) -> Result<Json<Vec<PurchaseWithItems>>, StatusCode> {
    let limit = params.limit.unwrap_or(50).clamp(1, MAX_LIST_LIMIT);
    // A negative offset is not a page request: OFFSET refuses it in Postgres, and
    // turning that into a 500 would blame the server for a bad query. The sales
    // ledger clamps it the same way, because a day filter that behaves
    // differently on two screens is a bug that reads as a UI fault.
    let offset = params.offset.unwrap_or(0).max(0);

    // One statement for both cases, the same shape as `list_sales`: a NULL bound
    // is not a filter, `from` is inclusive and `to` is exclusive, so the two
    // ranges that meet at midnight partition the ledger — no purchase is counted
    // twice or lost between them.
    let rows = sqlx::query!(
        "SELECT id, supplier_id, round(total, 2) AS \"total!\", created_at
         FROM purchases
         WHERE ($1::timestamptz IS NULL OR created_at >= $1)
           AND ($2::timestamptz IS NULL OR created_at < $2)
         ORDER BY created_at DESC
         LIMIT $3 OFFSET $4",
        params.from,
        params.to,
        limit,
        offset
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "list_purchases failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // One extra query per purchase for its lines: bounded by `limit` and simple
    // on purpose — the json-aggregated single query is the optimization, later.
    let mut purchases = Vec::with_capacity(rows.len());
    for row in rows {
        let items = sqlx::query_as!(
            PurchaseItem,
            "SELECT id, purchase_id, product_id, product_name, quantity,
                    round(unit_price, 2) AS \"unit_price!\",
                    round(line_total, 2) AS \"line_total!\", unit
             FROM purchase_items WHERE purchase_id = $1 ORDER BY product_name, id",
            row.id
        )
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(?e, "purchase items read failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        purchases.push(PurchaseWithItems {
            purchase: Purchase {
                id: row.id,
                supplier_id: row.supplier_id,
                total: row.total,
                created_at: row.created_at,
            },
            items,
        });
    }

    Ok(Json(purchases))
}
