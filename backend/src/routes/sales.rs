use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::models::account::{CASH_ACCOUNT_ID, CASH_PAYMENT_METHOD_ID, CREDIT_ACCOUNT_ID};
use crate::models::{
    credit_note::CreditNote, credit_note_item::CreditNoteItem, sale::Sale, sale_item::SaleItem,
};
use crate::money::{self, LineInput};

/// The customer seeded by the migration: the default for sales with no named
/// customer. Must match the fixed id in `0002_v1_sales_stock.sql`.
const WALK_IN_CUSTOMER_ID: Uuid = Uuid::from_u128(1);

/// Upper bound for a single list query, whatever the client asks for.
const MAX_LIST_LIMIT: i64 = 200;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sales", post(create_sale).get(list_sales))
        .route("/sales/{id}/credit-notes", post(create_credit_note))
        .route("/sales/summary", get(sales_summary))
        .route("/cash", get(cash_total))
}

#[derive(Debug, Deserialize)]
pub struct NewSale {
    /// Omitted means the seeded walk-in customer.
    #[serde(default)]
    pub customer_id: Option<Uuid>,
    /// Where the money went. Omitted means the seeded `cash` method, which is what
    /// every sale meant before this existed — so an older client keeps its exact
    /// behaviour rather than breaking.
    ///
    /// An unknown id is a 404, and a method whose account is the debt account is
    /// refused when the sale names `walk-in` (S9): anonymous debt is not debt.
    #[serde(default)]
    pub payment_method_id: Option<Uuid>,
    pub items: Vec<NewSaleItem>,
}

#[derive(Debug, Deserialize)]
pub struct NewSaleItem {
    pub product_id: Uuid,
    pub quantity: i32,
    /// Omitted defaults to the product's list price. A decimal string, not a
    /// JSON number: a float would lose the cent.
    #[serde(default)]
    pub unit_price: Option<Decimal>,
}

/// A sale with its lines, as returned by the create endpoint.
#[derive(Debug, Serialize)]
pub struct SaleWithItems {
    #[serde(flatten)]
    pub sale: Sale,
    pub items: Vec<SaleItem>,
}

/// A sale in list form: the sale, its customer's name, its lines, and every
/// credit note issued against it.
#[derive(Debug, Serialize)]
pub struct SaleListItem {
    #[serde(flatten)]
    pub sale: Sale,
    pub customer_name: String,
    pub items: Vec<SaleItem>,
    /// Oldest first. Empty when the sale was never corrected — an empty array,
    /// never an absent field, so the client never has to guess.
    pub credit_notes: Vec<CreditNoteWithItems>,
}

/// A credit note with its lines, which is how every endpoint returns one.
#[derive(Debug, Serialize)]
pub struct CreditNoteWithItems {
    #[serde(flatten)]
    pub note: CreditNote,
    pub items: Vec<CreditNoteItem>,
}

/// The body of a refund: how much of each sale line comes back.
#[derive(Debug, Deserialize)]
pub struct NewCreditNote {
    /// Which method the refund goes back through. **Omitted means the corrected
    /// sale's method**, which is what makes a refund of an unpaid credit sale move no
    /// money: the note credits the same account the sale debited, so the debt drops.
    ///
    /// An explicit method exists for the case inheritance gets wrong: a credit sale
    /// that was already collected, refunded out of the till instead of leaving a
    /// balance in the customer's favour.
    ///
    /// The client names **the method, never the account** — the server resolves it,
    /// exactly as a supplier return never sends a price.
    #[serde(default)]
    pub payment_method_id: Option<Uuid>,
    pub items: Vec<NewCreditNoteItem>,
}

#[derive(Debug, Deserialize)]
pub struct NewCreditNoteItem {
    /// The line being corrected. Must belong to the sale in the path.
    pub sale_item_id: Uuid,
    /// How many units come back. May be less than the quantity sold: a partial
    /// refund is the normal case, and several notes may follow.
    pub quantity: i32,
}

/// The cash balance: the sum of confirmed sales, as a currency amount.
#[derive(Debug, Serialize)]
pub struct Cash {
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub total: Decimal,
}

#[derive(Debug, Deserialize)]
pub struct SalesQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
    /// Inclusive lower bound on `created_at`. Absent means no lower bound.
    #[serde(default)]
    pub from: Option<DateTime<Utc>>,
    /// Exclusive upper bound on `created_at`. Absent means no upper bound.
    /// Exclusive is what makes adjacent days partition the ledger instead of
    /// double-counting the sale recorded at midnight.
    #[serde(default)]
    pub to: Option<DateTime<Utc>>,
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

    // Resolve the destination before any write, the same way the customer is
    // resolved: an unknown method is a 404 rather than a foreign-key 500, and the
    // account it points at is read here because the credit rule below needs it.
    //
    // This reads the method's account rather than trusting the client to send one:
    // the client names the method, the method knows its account. That is the whole
    // point of the direction chosen in v3.
    let (payment_method_id, method_account_id) = match body.payment_method_id {
        Some(id) => {
            let row = sqlx::query!("SELECT account_id FROM payment_methods WHERE id = $1", id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| {
                    tracing::error!(?e, "payment method lookup failed");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            let row = row.ok_or(StatusCode::NOT_FOUND)?;
            (id, row.account_id)
        }
        None => (CASH_PAYMENT_METHOD_ID, CASH_ACCOUNT_ID),
    };

    // S9: a sale paid on credit must name a real customer. The check is written
    // **positively first** in the tests — a credit sale to a named customer is
    // accepted — because a refusal on its own could pass for the wrong reason.
    //
    // `is_active` is deliberately NOT consulted: it is a picker concern, and gating
    // writes on it would make this refusal fire because the seeded `credit` method is
    // inactive rather than because the customer is anonymous.
    if method_account_id == CREDIT_ACCOUNT_ID && customer_id == WALK_IN_CUSTOMER_ID {
        return Err(StatusCode::BAD_REQUEST);
    }

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
        "INSERT INTO sales (customer_id, total, payment_method_id) VALUES ($1, $2, $3) RETURNING id",
        customer_id,
        total,
        payment_method_id
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
        // round(...,2): the column declares scale 6 and returns its value at that
        // scale, so an amount would leave as `3750.000000`. See 0006 for why the
        // column declares 6, and `money::MONEY_SCALE` for the contract.
        //
        // `payment_method` is joined, not read off `sales`: 0012 dropped the TEXT
        // column and the name lives on `payment_methods`. Joining by id is what
        // lets a rename correct history instead of freezing a typo.
        "SELECT s.id, s.customer_id, round(s.total, 2) AS \"total!\",
                s.payment_method_id, pm.name AS \"payment_method_name!\", s.status,
                s.reversed_at, s.created_at
         FROM sales s
         JOIN payment_methods pm ON pm.id = s.payment_method_id
         WHERE s.id = $1",
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

async fn create_credit_note(
    State(state): State<AppState>,
    Path(sale_id): Path<Uuid>,
    Json(body): Json<NewCreditNote>,
) -> Result<(StatusCode, Json<CreditNoteWithItems>), StatusCode> {
    // Shape first: an empty refund, or one asking for a quantity of zero or
    // less, is a 400 with nothing opened and nothing locked.
    if body.items.is_empty() || body.items.iter().any(|item| item.quantity <= 0) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!(?e, "tx begin failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Serialize refunds of this sale, and prove it exists. Two concurrent
    // notes would otherwise both read the same refunded sums, both find room on
    // a line, and both write — over-refunding it. The row lock is taken before
    // any read of those sums, which is what makes the guard a guard.
    let locked: Option<Uuid> =
        sqlx::query_scalar!("SELECT id FROM sales WHERE id = $1 FOR UPDATE", sale_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "sale lock failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    if locked.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    // Resolve every refund against the sale line it corrects: the line must
    // belong to this sale, and what it already gives back plus what is asked
    // for now must not exceed what was sold on it.
    //
    // The running total is summed *in this loop*, not read again per entry from
    // the database, because the write has not happened yet: two entries naming
    // the same line would each read the same pre-existing sum and each pass on
    // their own, over-refunding the line in one request. That hole was measured
    // before this comment was written — `[{same line, 6}, {same line, 6}]`
    // against a line of 10 was accepted and returned 12.
    struct RefundLine {
        product_id: Uuid,
        product_name: String,
        unit_price: Decimal,
        unit: String,
        quantity: i32,
    }
    let mut lines: Vec<RefundLine> = Vec::with_capacity(body.items.len());
    let mut requested: std::collections::HashMap<Uuid, i64> = std::collections::HashMap::new();
    for item in &body.items {
        let line = sqlx::query!(
            "SELECT si.product_id, si.product_name, si.quantity, si.unit_price, si.unit,
                    COALESCE(SUM(cni.quantity), 0)::int AS \"refunded!\"
             FROM sale_items si
             LEFT JOIN credit_note_items cni ON cni.sale_item_id = si.id
             WHERE si.id = $1 AND si.sale_id = $2
             GROUP BY si.product_id, si.product_name, si.quantity, si.unit_price, si.unit",
            item.sale_item_id,
            sale_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(?e, "sale line lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        // A line of another sale is not a conflict: it is the wrong document.
        let line = line.ok_or(StatusCode::NOT_FOUND)?;
        // What this line would be refunded in total if the request went
        // through: what is already on record, plus every entry naming it.
        let already = requested
            .entry(item.sale_item_id)
            .or_insert(line.refunded as i64);
        *already += item.quantity as i64;
        if *already > line.quantity as i64 {
            return Err(StatusCode::CONFLICT);
        }
        lines.push(RefundLine {
            product_id: line.product_id,
            product_name: line.product_name,
            unit_price: line.unit_price,
            unit: line.unit,
            quantity: item.quantity,
        });
    }

    // Money before any write: an invalid line is a 400 with nothing persisted.
    // The note totals what was actually charged for the returned units, at the
    // price snapshotted on the sale line — a refund returns what the customer
    // paid, not the catalog's current price.
    let mut note_total = Decimal::ZERO;
    let mut line_totals = Vec::with_capacity(lines.len());
    for line in &lines {
        let total =
            money::line_total(line.quantity, line.unit_price).ok_or(StatusCode::BAD_REQUEST)?;
        note_total = note_total
            .checked_add(total)
            .filter(|sum| money::is_whole_cents(*sum))
            .ok_or(StatusCode::BAD_REQUEST)?;
        line_totals.push(total);
    }
    note_total.rescale(money::MONEY_SCALE);

    // Inherit the corrected sale's method, unless the note names one.
    //
    // Inheritance is the default because it is what makes a refund of an unpaid
    // credit sale move no money: the note credits the same account the sale debited,
    // so the debt drops instead of a till paying out. The override exists for the
    // sale that was already collected.
    let sale_method: Uuid =
        sqlx::query_scalar!("SELECT payment_method_id FROM sales WHERE id = $1", sale_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "sale payment method read failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let note_method: Uuid = match body.payment_method_id {
        Some(id) => {
            let account_id: Option<Uuid> =
                sqlx::query_scalar!("SELECT account_id FROM payment_methods WHERE id = $1", id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(|e| {
                        tracing::error!(?e, "payment method lookup failed");
                        StatusCode::INTERNAL_SERVER_ERROR
                    })?;
            let account_id = account_id.ok_or_else(|| {
                tracing::error!("METHOD-LOOKUP-404");
                StatusCode::NOT_FOUND
            })?;
            // A note may not target the debt account. Moving debt between customers
            // is a different document, and a refund is not where that belongs. The
            // inherited value can legitimately be that account, which is why this
            // check applies only to an explicit one — that asymmetry is the whole
            // difference between "the debt drops" and "the debt moved".
            if account_id == CREDIT_ACCOUNT_ID {
                return Err(StatusCode::BAD_REQUEST);
            }
            id
        }
        None => sale_method,
    };

    let note_id = sqlx::query_scalar!(
        "INSERT INTO credit_notes (sale_id, total, payment_method_id) VALUES ($1, $2, $3) RETURNING id",
        sale_id,
        note_total,
        note_method
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "credit note insert failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // One line and one POSITIVE stock movement per refunded line: the mirror
    // image of the sale path, which writes negative ones. The sale's own
    // movements are never touched — a reversal appends, it does not edit.
    let mut items = Vec::with_capacity(lines.len());
    for (idx, line) in lines.iter().enumerate() {
        let line_total = line_totals[idx];
        let item_id = sqlx::query_scalar!(
            "INSERT INTO credit_note_items
                (credit_note_id, sale_item_id, product_id, product_name, quantity,
                 unit_price, line_total, unit)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
            note_id,
            body.items[idx].sale_item_id,
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
            tracing::error!(?e, "credit note item insert failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        sqlx::query!(
            "INSERT INTO stock_movements (product_id, quantity, reason, sale_id, credit_note_id)
             VALUES ($1, $2, 'credit_note', $3, $4)",
            line.product_id,
            line.quantity,
            sale_id,
            note_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(?e, "reversal movement insert failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        items.push(CreditNoteItem {
            id: item_id,
            credit_note_id: note_id,
            sale_item_id: body.items[idx].sale_item_id,
            product_id: line.product_id,
            product_name: line.product_name.clone(),
            quantity: line.quantity,
            unit_price: line.unit_price,
            line_total,
            unit: line.unit.clone(),
        });
    }

    // Read the persisted note so the response carries the server's created_at
    // rather than the caller's assumption about it.
    let note = sqlx::query_as!(
        CreditNote,
        "SELECT id, sale_id, round(total, 2) AS \"total!\", created_at
         FROM credit_notes WHERE id = $1",
        note_id
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "credit note readback failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "tx commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::CREATED,
        Json(CreditNoteWithItems { note, items }),
    ))
}

/// Every credit note of one sale, oldest first, each with its lines.
///
/// Two queries per note, bounded by how many times a sale was corrected —
/// which is normally zero or one.
async fn credit_notes_for(
    db: &sqlx::PgPool,
    sale_id: Uuid,
) -> Result<Vec<CreditNoteWithItems>, StatusCode> {
    // `ORDER BY created_at, id`: two notes of the same sale can share a
    // `now()` (it is transaction time), and an unstable order would make the
    // day's rendering flicker between reloads.
    let notes = sqlx::query_as!(
        CreditNote,
        "SELECT id, sale_id, round(total, 2) AS \"total!\", created_at
         FROM credit_notes WHERE sale_id = $1 ORDER BY created_at, id",
        sale_id
    )
    .fetch_all(db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "credit notes read failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut with_items = Vec::with_capacity(notes.len());
    for note in notes {
        let items = sqlx::query_as!(
            CreditNoteItem,
            "SELECT id, credit_note_id, sale_item_id, product_id, product_name, quantity,
                    round(unit_price, 2) AS \"unit_price!\",
                    round(line_total, 2) AS \"line_total!\", unit
             FROM credit_note_items WHERE credit_note_id = $1",
            note.id
        )
        .fetch_all(db)
        .await
        .map_err(|e| {
            tracing::error!(?e, "credit note items read failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        with_items.push(CreditNoteWithItems { note, items });
    }
    Ok(with_items)
}

async fn list_sales(
    State(state): State<AppState>,
    Query(params): Query<SalesQuery>,
) -> Result<Json<Vec<SaleListItem>>, StatusCode> {
    let limit = params.limit.unwrap_or(50).clamp(1, MAX_LIST_LIMIT);
    // A negative offset is not a page request: OFFSET refuses it in Postgres,
    // and turning that into a 500 would blame the server for a bad query.
    let offset = params.offset.unwrap_or(0).max(0);

    // One statement for both cases: a NULL bound is not a filter. `from` is
    // inclusive and `to` is exclusive, so the two ranges that meet at midnight
    // partition the ledger — no sale is counted twice or lost between them.
    // With both bounds absent the WHERE clause is vacuously true, which is the
    // unscoped behaviour v1 shipped.
    let rows = sqlx::query!(
        // `payment_method` is the method's name, joined on the FK that 0012 added.
        "SELECT s.id, s.customer_id, round(s.total, 2) AS \"total!\",
                s.payment_method_id, pm.name AS \"payment_method_name!\", s.status,
                s.reversed_at, s.created_at, c.name AS customer_name
         FROM sales s
         JOIN customers c ON c.id = s.customer_id
         JOIN payment_methods pm ON pm.id = s.payment_method_id
         WHERE ($1::timestamptz IS NULL OR s.created_at >= $1)
           AND ($2::timestamptz IS NULL OR s.created_at < $2)
         ORDER BY s.created_at DESC
         LIMIT $3 OFFSET $4",
        params.from,
        params.to,
        limit,
        offset
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
            "SELECT id, sale_id, product_id, product_name, quantity,
                    round(unit_price, 2) AS \"unit_price!\",
                    round(line_total, 2) AS \"line_total!\", unit
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
                payment_method_id: row.payment_method_id,
                payment_method_name: row.payment_method_name,
                status: row.status,
                reversed_at: row.reversed_at,
                created_at: row.created_at,
            },
            customer_name: row.customer_name,
            items,
            credit_notes: credit_notes_for(&state.db, row.id).await?,
        });
    }

    Ok(Json(sales))
}

/// What a scoped period of the ledger is worth, computed by the database.
///
/// The screen used to derive this from the rows it had loaded, which is a page of
/// at most 50 documents — so a period longer than one page showed a total that
/// was missing its oldest documents, and a running balance that started at zero
/// in the middle of the period. The figure the shop reads must be the period's,
/// not the page's, and only the database can answer that.
#[derive(Debug, serde::Serialize)]
pub struct PeriodSummary {
    /// Gross of every sale that counts (not cancelled, not given back in full).
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub gross: Decimal,
    /// What went back to customers on those same sales.
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub refunded: Decimal,
    /// Gross less refunded: what the period put in the till.
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub net: Decimal,
    /// How many documents the period holds, so the screen can say "50 of 312".
    pub documents: i64,
}

#[derive(Debug, Deserialize)]
pub struct SummaryQuery {
    #[serde(default)]
    pub from: Option<DateTime<Utc>>,
    #[serde(default)]
    pub to: Option<DateTime<Utc>>,
}

/// `GET /api/sales/summary` — the period's totals, not the page's.
///
/// A document counts when it is `confirmed` **and** still has money in it: a sale
/// whose credit notes equal its total was given back in full and contributes
/// nothing, exactly as the ledger's own column treats it. The distinction is the
/// review's finding — a partially refunded sale contributes its net, not its
/// gross, and a fully refunded one contributes nothing at all.
async fn sales_summary(
    State(state): State<AppState>,
    Query(params): Query<SummaryQuery>,
) -> Result<Json<PeriodSummary>, StatusCode> {
    // The bounds default to unbounded in SQL, not to sentinel instants. A
    // `DateTime::<Utc>::MIN_UTC` as a bound overflows Postgres's timestamp range
    // ("time zone displacement out of range") and the whole period reads as
    // empty — which is a silent zero, the worst shape for this bug.
    let from: Option<DateTime<Utc>> = params.from;
    let to: Option<DateTime<Utc>> = params.to;

    let row = sqlx::query!(
        // One statement, one shape. `counted` is the same predicate the screen
        // applies per row, expressed once so the total and the column cannot
        // drift: confirmed, and gross greater than what came back.
        r#"
        -- Two different questions, two different sets.
        --
        -- `gross` and `refunded` describe the documents the shop wrote: every
        -- confirmed sale in the period, whether or not money came back. Summing
        -- them over the "still has money in it" set would make a fully refunded
        -- sale vanish from both figures instead of cancelling out.
        --
        -- `net` is what the till kept, and that is the same number whether a row
        -- contributes `total - refunded` or contributes nothing because the
        -- subtraction already reached zero.
        WITH counted AS (
            SELECT s.total,
                   COALESCE((SELECT SUM(cn.total) FROM credit_notes cn WHERE cn.sale_id = s.id), 0) AS refunded
              FROM sales s
             WHERE s.status = 'confirmed'
               -- Absent bound means unbounded, the same contract `list_sales`
               -- has. A sentinel instant cannot be used here: MIN_UTC is outside
               -- Postgres's timestamp range and the whole period reads as empty.
               AND ($1::timestamptz IS NULL OR s.created_at >= $1)
               AND ($2::timestamptz IS NULL OR s.created_at < $2)
        )
        SELECT COALESCE(SUM(total), 0)::numeric           AS "gross!",
               COALESCE(SUM(refunded), 0)::numeric        AS "refunded!",
               COALESCE(SUM(GREATEST(total - refunded, 0)), 0)::numeric AS "net!",
               (SELECT COUNT(*) FROM counted)::bigint      AS "documents!"
          FROM counted
        "#,
        from,
        to
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "sales summary failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(PeriodSummary {
        gross: row.gross,
        refunded: row.refunded,
        net: row.net,
        documents: row.documents,
    }))
}

async fn cash_total(State(state): State<AppState>) -> Result<Json<Cash>, StatusCode> {
    // Net of the corrections. A refund is money leaving the till, and a figure
    // called "cash on hand" that ignored it would be wrong in the one direction
    // that matters. Both sums are `numeric` and both are COALESCEd, so an empty
    // ledger and a ledger with no notes behave identically.
    let total = sqlx::query_scalar!(
        // Money in the till: what came in from sales, minus what was refunded to
        // customers, minus what was spent buying stock, **plus** what suppliers
        // gave back on returns. The shape of the answer never changes — one
        // two-decimal string — but its meaning has changed twice, so the comment
        // says what the figure is rather than what it used to be.
        //
        // `sales` filters `status = 'confirmed'` and the other three filter
        // nothing. That asymmetry is the design, not an oversight: a sale carries
        // a reserved lifecycle whose `cancel` must not count, while a purchase, a
        // credit note and a supplier return have no status at all — if the row
        // exists, it counts.
        "SELECT (COALESCE((SELECT SUM(total) FROM sales WHERE status = 'confirmed'), 0)
               - COALESCE((SELECT SUM(total) FROM credit_notes), 0)
               - COALESCE((SELECT SUM(total) FROM purchases), 0)
               + COALESCE((SELECT SUM(total) FROM supplier_returns), 0))::numeric AS \"total!\""
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "cash total failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(Cash { total }))
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
