use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;
use crate::models::account::CASH_PAYMENT_METHOD_ID;
use crate::models::purchase::{Purchase, PurchaseItem, PurchaseWithItems};
use crate::models::supplier_return::{SupplierReturn, SupplierReturnItem, SupplierReturnWithItems};
use crate::money;

/// The seeded one-off supplier. Its id is a constant here for the same reason
/// `WALK_IN_CUSTOMER_ID` is a constant in `sales.rs`: an omitted supplier must
/// resolve without a lookup, and the migration seeds exactly this row.
const ONE_OFF_SUPPLIER_ID: Uuid = Uuid::from_u128(2);

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/purchases", post(create_purchase).get(list_purchases))
        .route("/purchases/summary", get(purchases_summary))
        .route(
            "/purchases/{id}/supplier-returns",
            post(create_supplier_return),
        )
}

/// What a scoped period of purchases is worth, computed by the database.
///
/// The sales ledger learned this the hard way: a footer derived from the loaded
/// page silently omits every document the page does not hold, and a set labelled
/// "the period" that stops at row fifty is a wrong number the shop reads as a
/// total. Same shape as `GET /api/sales/summary`, minus the refund term — a
/// purchase has no credit notes, and a supplier return is its own document.
#[derive(Debug, serde::Serialize)]
pub struct PurchasesSummary {
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub gross: Decimal,
    /// What suppliers gave back on returns inside the same period.
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub returned: Decimal,
    /// What the period cost the shop: purchases less returns.
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub net: Decimal,
    pub documents: i64,
}

#[derive(Debug, Deserialize)]
pub struct SummaryQuery {
    #[serde(default)]
    pub from: Option<DateTime<Utc>>,
    #[serde(default)]
    pub to: Option<DateTime<Utc>>,
}

async fn purchases_summary(
    State(state): State<AppState>,
    Query(params): Query<SummaryQuery>,
) -> Result<Json<PurchasesSummary>, StatusCode> {
    // Absent bound means unbounded, the same contract `list_purchases` has. A
    // sentinel instant cannot be used: `MIN_UTC` is outside Postgres's timestamp
    // range and the whole period would read as empty — a silent zero.
    let from = params.from;
    let to = params.to;

    let row = sqlx::query!(
        // Purchases and returns are separate counts on purpose. A return is not
        // a negative purchase: it is its own document correcting one, and the
        // shop reads "what did I spend" and "what came back" as two facts.
        r#"
        WITH bought AS (
            SELECT total FROM purchases
             WHERE ($1::timestamptz IS NULL OR created_at >= $1)
               AND ($2::timestamptz IS NULL OR created_at < $2)
        ),
        returned AS (
            SELECT total FROM supplier_returns
             WHERE ($1::timestamptz IS NULL OR created_at >= $1)
               AND ($2::timestamptz IS NULL OR created_at < $2)
        )
        SELECT COALESCE((SELECT SUM(total) FROM bought), 0)::numeric   AS "gross!",
               COALESCE((SELECT SUM(total) FROM returned), 0)::numeric AS "returned!",
               (COALESCE((SELECT SUM(total) FROM bought), 0)
                - COALESCE((SELECT SUM(total) FROM returned), 0))::numeric AS "net!",
               (SELECT COUNT(*) FROM bought)::bigint AS "documents!"
        "#,
        from,
        to
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "purchases summary failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(PurchasesSummary {
        gross: row.gross,
        returned: row.returned,
        net: row.net,
        documents: row.documents,
    }))
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
        "INSERT INTO purchases (supplier_id, total, payment_method_id) VALUES ($1, $2, $3) RETURNING id",
        supplier_id,
        total,
        CASH_PAYMENT_METHOD_ID
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
        Json(PurchaseWithItems {
            purchase,
            items,
            // A purchase that was just recorded cannot have been returned yet.
            // An empty array rather than an omitted field, so a client reads one
            // shape in both responses.
            supplier_returns: Vec::new(),
        }),
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
        // The returns that corrected this purchase, oldest first, each with its
        // own lines. Two extra queries per purchase, bounded by `limit` and
        // deliberately simple: the json-aggregated single query is the
        // optimization, later, exactly as the items read above says.
        let return_rows = sqlx::query_as!(
            SupplierReturn,
            "SELECT id, purchase_id, round(total, 2) AS \"total!\", created_at
             FROM supplier_returns WHERE purchase_id = $1 ORDER BY created_at, id",
            row.id
        )
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(?e, "supplier returns read failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let mut supplier_returns = Vec::with_capacity(return_rows.len());
        for sup_return in return_rows {
            let return_items = sqlx::query_as!(
                SupplierReturnItem,
                "SELECT id, supplier_return_id, purchase_item_id, product_id, product_name,
                        quantity, round(unit_price, 2) AS \"unit_price!\",
                        round(line_total, 2) AS \"line_total!\", unit
                 FROM supplier_return_items WHERE supplier_return_id = $1
                 ORDER BY product_name, id",
                sup_return.id
            )
            .fetch_all(&state.db)
            .await
            .map_err(|e| {
                tracing::error!(?e, "supplier return items read failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            supplier_returns.push(SupplierReturnWithItems {
                supplier_return: sup_return,
                items: return_items,
            });
        }

        purchases.push(PurchaseWithItems {
            purchase: Purchase {
                id: row.id,
                supplier_id: row.supplier_id,
                total: row.total,
                created_at: row.created_at,
            },
            items,
            supplier_returns,
        });
    }

    Ok(Json(purchases))
}

// ── Supplier returns (v2.5) ─────────────────────────────────────────────────

/// One line of a return: which purchase line it corrects, and how many units go
/// back.
///
/// The client sends **no price**. What the return is worth is read server-side
/// from the purchase line, which is what makes it impossible for a return to be
/// worth more than the purchase that caused it.
#[derive(Debug, Deserialize)]
pub struct NewSupplierReturn {
    pub items: Vec<NewSupplierReturnItem>,
}

#[derive(Debug, Deserialize)]
pub struct NewSupplierReturnItem {
    /// The line being corrected. Must belong to the purchase in the path.
    pub purchase_item_id: Uuid,
    /// How many units go back. May be less than the quantity bought: a partial
    /// return is the normal case, and several returns may follow.
    pub quantity: i32,
}

async fn create_supplier_return(
    State(state): State<AppState>,
    Path(purchase_id): Path<Uuid>,
    Json(body): Json<NewSupplierReturn>,
) -> Result<(StatusCode, Json<SupplierReturnWithItems>), StatusCode> {
    // Shape first: an empty return, or one asking for a quantity of zero or
    // less, is a 400 with nothing opened and nothing locked.
    if body.items.is_empty() || body.items.iter().any(|item| item.quantity <= 0) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!(?e, "tx begin failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // ── Guard, level 1: serialize returns of this purchase ──────────────────
    //
    // Two concurrent returns would otherwise both read the same returned sums,
    // both find room on a line, and both write — over-returning it. The row lock
    // is taken before any read of those sums, which is what makes it a guard.
    let locked: Option<Uuid> = sqlx::query_scalar!(
        "SELECT id FROM purchases WHERE id = $1 FOR UPDATE",
        purchase_id
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "purchase lock failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if locked.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    // ── Guard, level 2: sum the running total inside this loop ──────────────
    //
    // NOT re-read per entry from the database: the write has not happened yet, so
    // two entries naming the same line would each read the same pre-existing sum
    // and each pass on their own, over-returning the line in one request. v1.5
    // measured exactly that hole in its own mirror — `[{line, 6}, {line, 6}]`
    // against a line of 10 returned 12 — and this slice inherits the shape, so it
    // inherits the defence.
    struct ReturnLine {
        product_id: Uuid,
        product_name: String,
        unit_price: Decimal,
        unit: String,
        quantity: i32,
    }
    let mut lines: Vec<ReturnLine> = Vec::with_capacity(body.items.len());
    let mut requested: std::collections::HashMap<Uuid, i64> = std::collections::HashMap::new();
    for item in &body.items {
        let line = sqlx::query!(
            "SELECT pi.product_id, pi.product_name, pi.quantity, pi.unit_price, pi.unit,
                    COALESCE(SUM(sri.quantity), 0)::int AS \"returned!\"
             FROM purchase_items pi
             LEFT JOIN supplier_return_items sri ON sri.purchase_item_id = pi.id
             WHERE pi.id = $1 AND pi.purchase_id = $2
             GROUP BY pi.product_id, pi.product_name, pi.quantity, pi.unit_price, pi.unit",
            item.purchase_item_id,
            purchase_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(?e, "purchase line lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        // A line of another purchase is not a conflict: it is the wrong document.
        let line = line.ok_or(StatusCode::NOT_FOUND)?;
        // What this line would have gone back in total if the request went
        // through: what is already on record, plus every entry naming it.
        let already = requested
            .entry(item.purchase_item_id)
            .or_insert(line.returned as i64);
        *already += item.quantity as i64;
        if *already > line.quantity as i64 {
            return Err(StatusCode::CONFLICT);
        }
        lines.push(ReturnLine {
            product_id: line.product_id,
            product_name: line.product_name,
            unit_price: line.unit_price,
            unit: line.unit,
            quantity: item.quantity,
        });
    }

    // Money before any write: an invalid line is a 400 with nothing persisted.
    // The return values what was PAID, at the price snapshotted on the purchase
    // line — never today's catalogue price, and never a price the client chose.
    let mut return_total = Decimal::ZERO;
    let mut line_totals = Vec::with_capacity(lines.len());
    for line in &lines {
        let total =
            money::line_total(line.quantity, line.unit_price).ok_or(StatusCode::BAD_REQUEST)?;
        return_total = return_total
            .checked_add(total)
            .filter(|sum| money::is_whole_cents(*sum))
            .ok_or(StatusCode::BAD_REQUEST)?;
        line_totals.push(total);
    }
    return_total.rescale(money::MONEY_SCALE);

    // The return inherits the corrected purchase's method, the mirror of a credit
    // note inheriting the sale's: the supplier gives money back through the same
    // rail that paid them. No override exists in v3.
    let inherited_method: Uuid = sqlx::query_scalar!(
        "SELECT payment_method_id FROM purchases WHERE id = $1",
        purchase_id
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "purchase payment method read failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let return_id = sqlx::query_scalar!(
        "INSERT INTO supplier_returns (purchase_id, total, payment_method_id) VALUES ($1, $2, $3) RETURNING id",
        purchase_id,
        return_total,
        inherited_method
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "supplier return insert failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut items = Vec::with_capacity(body.items.len());
    for (idx, item) in body.items.iter().enumerate() {
        let line = &lines[idx];
        let line_total = line_totals[idx];

        let item_id = sqlx::query_scalar!(
            "INSERT INTO supplier_return_items
                (supplier_return_id, purchase_item_id, product_id, product_name,
                 quantity, unit_price, line_total, unit)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
            return_id,
            item.purchase_item_id,
            line.product_id,
            line.product_name,
            line.quantity,
            line.unit_price,
            line_total,
            line.unit
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(?e, "supplier return item insert failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        // The sign is the slice in one character: `-quantity`, where the purchase
        // path writes `+quantity`. Stock leaves. The database also refuses a
        // positive return movement, so this is belt and braces — but a wrong sign
        // here would move stock the wrong way while every constraint still
        // passed, which is why the test measures the delta.
        sqlx::query!(
            "INSERT INTO stock_movements (product_id, quantity, reason, purchase_id, supplier_return_id)
             VALUES ($1, $2, 'supplier_return', $3, $4)",
            line.product_id,
            -line.quantity,
            purchase_id,
            return_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(?e, "stock movement insert failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        items.push(SupplierReturnItem {
            id: item_id,
            supplier_return_id: return_id,
            purchase_item_id: item.purchase_item_id,
            product_id: line.product_id,
            product_name: line.product_name.clone(),
            quantity: line.quantity,
            unit_price: line.unit_price,
            line_total,
            unit: line.unit.clone(),
        });
    }

    // Read the persisted header so the response carries the real `created_at`
    // rather than echoing the request, and `round(...,2)` so the amount leaves as
    // `20.00` and not `20.000000` (see 0006).
    let supplier_return = sqlx::query_as!(
        SupplierReturn,
        "SELECT id, purchase_id, round(total, 2) AS \"total!\", created_at
         FROM supplier_returns WHERE id = $1",
        return_id
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "supplier return readback failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "tx commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::CREATED,
        Json(SupplierReturnWithItems {
            supplier_return,
            items,
        }),
    ))
}
