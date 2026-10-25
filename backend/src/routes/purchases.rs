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
use crate::models::account::{CASH_ACCOUNT_ID, CASH_PAYMENT_METHOD_ID};
use crate::models::destination::{self, Destination};
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
    /// Where the money left from, or **`"on_account"` for a purchase that owes the
    /// supplier and moved none** (`odd/tasks/v4-payables.md` S2, S3). The grammar is the
    /// sale's, and it is one definition: `Destination::from_field`.
    ///
    /// | input | meaning |
    /// |---|---|
    /// | omitted | the seeded `cash` method, which is what every purchase meant before v3 |
    /// | a uuid | money left through that method |
    /// | `"on_account"` | money did not leave; the purchase owes, and `payables` records it |
    ///
    /// An explicit `null` is a 400: it cannot mean "the till" without turning a debt into
    /// a cash purchase, nor "on account" without turning every older client's omitted
    /// field into a debt.
    ///
    /// An unknown method id is a 404. A method whose account is the **credit** account is
    /// refused (S9b): a purchase paid with the debt account would subtract from what
    /// customers owe, which no purchase can mean.
    #[serde(default, deserialize_with = "destination::keep_present")]
    pub payment_method_id: Option<serde_json::Value>,
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

    // Resolve the destination before any write, the mirror of a sale: an unknown
    // method is a 404 rather than a foreign-key 500, and the account is read here
    // because the credit rule below needs it.
    //
    // `None` is the purchase on account: money did not leave, so there is no method to
    // resolve and no account to name. It is not "the till" — omitted means the till and
    // is resolved below, which keeps every older client's behaviour intact.
    let destination = match Destination::from_field(body.payment_method_id)
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        Some(Destination::Method(id)) => {
            let row = sqlx::query!("SELECT account_id FROM payment_methods WHERE id = $1", id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| {
                    tracing::error!(?e, "payment method lookup failed");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            let row = row.ok_or(StatusCode::NOT_FOUND)?;
            Some((id, row.account_id))
        }
        Some(Destination::OnAccount) => None,
        None => Some((CASH_PAYMENT_METHOD_ID, CASH_ACCOUNT_ID)),
    };

    // S9b: a purchase may never name the credit method. Its effect would subtract
    // from the debt account — "what customers owe went down by 40,000" — which no
    // purchase means. The debt a purchase creates is the payable, a different document
    // written below; it never names the debt account (`0021`'s rule, mirrored).
    //
    // Written as an equality against the account rather than against the method id so
    // it keeps holding if the shop ever attaches a second method to that account.
    // S3: a purchase on account must name a real supplier. The one-off supplier is
    // anonymous and cannot be owed: anonymous debt is not debt, mirroring the refusal of
    // `walk-in` for a sale on account.
    //
    // `is_active` is deliberately NOT consulted, for the reason `sales.rs` gives.
    if destination.is_none() && supplier_id == ONE_OFF_SUPPLIER_ID {
        return Err(StatusCode::BAD_REQUEST);
    }

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

    // `payment_method_id` is NULL for a purchase on account, and that NULL is the
    // purchase's own statement that it moved no money — `0023`'s constraint trigger holds
    // it against the payable written below, at COMMIT.
    let purchase_id = sqlx::query_scalar!(
        "INSERT INTO purchases (supplier_id, total, payment_method_id) VALUES ($1, $2, $3) RETURNING id",
        supplier_id,
        total,
        destination.map(|(method, _)| method)
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
    //
    // **`LEFT JOIN`, and it is not stylistic.** A purchase on account has no method, and an
    // `INNER JOIN` would drop it from this readback — turning a correct insert into a 500.
    // The name is named nullable explicitly: sqlx infers a joined column as non-null
    // otherwise, and a `!` override would compile and lie.
    let purchase = sqlx::query_as!(
        Purchase,
        "SELECT p.id, p.supplier_id, p.payment_method_id,
                CASE WHEN p.payment_method_id IS NULL THEN NULL ELSE pm.name END
                    AS \"payment_method_name\",
                round(p.total, 2) AS \"total!\", p.created_at
         FROM purchases p
         LEFT JOIN payment_methods pm ON pm.id = p.payment_method_id
         WHERE p.id = $1",
        purchase_id
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "purchase readback failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // **The purchase claims one of two things, and only one path runs** (`S1`). A purchase
    // that moved money writes its book entry (`0017`) and owes nothing. A purchase on
    // account writes a payable and no entry. Writing neither, or both, is what `0023`'s
    // constraint trigger refuses at COMMIT — this is the write path agreeing with it.
    match destination {
        Some((method, _)) => {
            // The book entry (`0017`): a purchase takes money OUT, so the amount is negative.
            let purchase_amount: Decimal = -total;
            sqlx::query!(
                "INSERT INTO money_entries
                    (account_id, payment_method_id, document_kind, document_id, amount, occurred_at, created_by)
                 SELECT pm.account_id, $1, 'purchase', $2, $3, now(), 'counter'
                   FROM payment_methods pm WHERE pm.id = $1",
                method,
                purchase_id,
                purchase_amount
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "purchase money entry failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }
        None => {
            // The debt. `party_id` is the purchase's own supplier, and `0023`'s trigger
            // re-derives it from the purchase rather than trusting this bind — a debt
            // attributed to a supplier who never sold anything is not expressible.
            // `created_by` is a literal until v5, as every other write path's is
            // (`v4-settlement.md` L24).
            sqlx::query!(
                "INSERT INTO payables
                    (source_document_kind, source_document_id, party_kind, party_id,
                     amount, occurred_at, created_by)
                 VALUES ('purchase', $1, 'supplier', $2, $3, $4, 'counter')",
                purchase_id,
                supplier_id,
                total,
                purchase.created_at
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "purchase payable failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }
    }

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
    // **`LEFT JOIN`, for the reason the create readback gives.** An `INNER JOIN` would drop
    // a purchase on account from the ledger with no error — the trap `sales.rs` documents.
    let rows = sqlx::query!(
        "SELECT p.id, p.supplier_id, p.payment_method_id,
                CASE WHEN p.payment_method_id IS NULL THEN NULL ELSE pm.name END
                    AS \"payment_method_name\",
                round(p.total, 2) AS \"total!\", p.created_at
         FROM purchases p
         LEFT JOIN payment_methods pm ON pm.id = p.payment_method_id
         WHERE ($1::timestamptz IS NULL OR p.created_at >= $1)
           AND ($2::timestamptz IS NULL OR p.created_at < $2)
         ORDER BY p.created_at DESC
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
            "SELECT id, purchase_id, payment_method_id,
                    round(total, 2) AS \"total!\", created_at
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
                payment_method_id: row.payment_method_id,
                payment_method_name: row.payment_method_name,
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
    /// Which method the money comes back through. **Omitted means the corrected purchase's
    /// method**, which is what makes a return of a purchase on account move no money: the
    /// return inherits the purchase's NULL, writes no ledger entry and lowers the payable
    /// (`odd/tasks/v4-payables.md` S8).
    ///
    /// An explicit method exists for the case inheritance gets wrong: a purchase collected out
    /// of one rail and refunded through another. `v4-settlement.md` L19 records why the override
    /// arrives together with its guard rather than on its own — `supplier_returns` had no
    /// override, and therefore **no hole**, until this slice gave it both.
    ///
    /// A method may not resolve to the debt account (`400`), and it must be a rail that actually
    /// paid for these goods: `409` otherwise. The client names **the method, never the account** —
    /// the server resolves it, exactly as a purchase never sends an account either.
    #[serde(default)]
    pub payment_method_id: Option<Uuid>,
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

    // The corrected purchase's own method, and **`None` is the whole point since `0023`**: a
    // purchase on account moved no money, so a return of it can move none either.
    let purchase_method: Option<Uuid> = sqlx::query_scalar!(
        "SELECT payment_method_id FROM purchases WHERE id = $1",
        purchase_id
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "purchase payment method read failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // **The return's method: inherited by default, or named by the caller** (`S9`,
    // `v4-settlement.md` L19). Inheritance is right for a purchase that was paid — the supplier
    // gives money back through the same rail that paid them — and it is *wrong* for one that only
    // owed, because then there is no money to give back: the goods coming back lower the debt.
    //
    // The override exists for the case inheritance gets wrong in the other direction: a purchase
    // that was paid from one rail and is being settled back through another. It costs a guard,
    // and the guard is the point.
    let return_method: Option<Uuid> = match body.payment_method_id {
        // `Some(None)` is impossible through serde here — this is `Option<Uuid>`, not the
        // `Destination` grammar, because a return has no `on_account` spelling: it either moves
        // money (a method) or it does not (inherited, and only on an unpaid purchase).
        Some(id) => {
            let account_id: Option<Uuid> =
                sqlx::query_scalar("SELECT account_id FROM payment_methods WHERE id = $1")
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(|e| {
                        tracing::error!(?e, "payment method lookup failed");
                        StatusCode::INTERNAL_SERVER_ERROR
                    })?;
            // The method must exist. Nothing is read off its account any more: the debt
            // account that used to be refused here does not exist (`S7`).
            account_id.ok_or(StatusCode::NOT_FOUND)?;

            // **An explicit method means money comes back through a rail, so that rail must have
            // received that money first** (`v3-accounts.md` L107 mirrored; the credit-note side
            // is `v4-receivables.md` S7). Measured before this guard existed: a purchase paid from
            // the bank, returned through the till, drove the till negative for money it never held.
            //
            // **What a rail is owed here is a budget, and earlier explicit returns spend from it,
            // exactly as on the credit-note side** (`R1-003`/`R3-003`). A return that only
            // inherited moved money too, so both kinds count: the question is what this purchase
            // ever put INTO this rail, less what its returns already took back out of it.
            // **The sign is negated, and getting this wrong is the exact defect `0014` warns
            // about.** A sale's entry is `+total` (money came in), so the credit-note side sums it
            // as it stands. A **purchase's** entry is `-total` (money went out), so the amount this
            // rail ever put into the purchase is its negation. Written as `-SUM(...)` rather than
            // as `SUM(-...)` to keep the aggregate and the convention adjacent.
            //
            // Measured before this fix, and it is why the correct return answered 409: the sum was
            // `-200`, so `arrived - left = -200 < 100` and the rail that HAD paid was refused.
            let arrived_on_rail: Decimal = sqlx::query_scalar(
                "SELECT -COALESCE((
                    SELECT SUM(me.amount) FROM money_entries me
                     WHERE me.document_kind = 'purchase' AND me.document_id = $1
                       AND me.payment_method_id = $2
                 ), 0)
                 + COALESCE((
                    SELECT SUM(ps.amount) FROM payable_settlements ps
                      JOIN payments p ON p.id = ps.payment_id
                     WHERE ps.payable_id = (
                             SELECT id FROM payables
                              WHERE source_document_kind = 'purchase'
                                AND source_document_id = $1)
                       AND p.direction = 'out'
                       AND p.payment_method_id = $2
                 ), 0)",
            )
            .bind(purchase_id)
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "purchase rail arrival failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

            // What earlier returns already took back out through this same rail, as a magnitude.
            let left_on_rail: Decimal = sqlx::query_scalar(
                "SELECT COALESCE(SUM(me.amount), 0)
                   FROM money_entries me
                   JOIN supplier_returns sr ON sr.id = me.document_id
                  WHERE me.document_kind = 'supplier_return' AND sr.purchase_id = $1
                    AND me.payment_method_id = $2",
            )
            .bind(purchase_id)
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "purchase rail refunds failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

            if arrived_on_rail - left_on_rail < return_total {
                return Err(StatusCode::CONFLICT);
            }

            Some(id)
        }
        // **Omitted means inherit, and the inheritance is what carries the two meanings.** On a
        // purchase that was paid, the return inherits the purchase's method and moves money back
        // through the same rail — the behaviour every return had before this slice. On a purchase
        // that only owed, the purchase's method is NULL, so the return inherits NULL: it writes no
        // ledger entry and lowers the payable through a `payables_adjustments` row, which is the
        // honest version of what the old code faked by handing money back for goods nobody paid
        // for.
        //
        // **`S8`'s "a return may be NULL only when its purchase is NULL" is therefore a property
        // of the result, not a refusal to make here.** Omitting the key can never produce a NULL
        // return on a paid purchase, because inheriting a paid purchase's method is not NULL. The
        // rule is reachable only by writing the row directly, and `0023`'s deferred trigger is
        // where it is enforced — `v4_s6b_the_database_refuses_a_null_return_on_a_paid_purchase`
        // measures it there, below the route, because the route cannot express it.
        None => purchase_method,
    };

    // **The combined ceiling on the inherited path, checked before the insert** — the lesson
    // `R1-004`/`R4-001` of `review-336e2618582310f9` records for the credit note, where the same
    // omission answered 500 for an ordinary operation. `payables_adjustments_within_amount`
    // raises at COMMIT; a `RAISE` at COMMIT is a 500, and the caller can act on a 409.
    //
    // **`FOR UPDATE`, in the same order every other writer takes these rows** — the payable, and
    // nothing else here — so a settlement committing between this read and the note's COMMIT
    // cannot turn the pre-check into a lie (`R1-003` of `review-bec...` on the customer side).
    if return_method.is_none() {
        let room: Option<(Decimal, Decimal, Decimal)> = sqlx::query_as(
            "SELECT o.amount,
                    COALESCE((SELECT SUM(a.amount) FROM payables_adjustments a
                               WHERE a.payable_id = o.id), 0),
                    COALESCE((SELECT SUM(s.amount) FROM payable_settlements s
                               WHERE s.payable_id = o.id), 0)
               FROM payables o
              WHERE o.source_document_kind = 'purchase' AND o.source_document_id = $1
              FOR UPDATE",
        )
        .bind(purchase_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(?e, "payable room read failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        match room {
            Some((owed, lowered, settled)) if lowered + settled + return_total > owed => {
                return Err(StatusCode::CONFLICT);
            }
            // A return that moves no money on a purchase that owes nothing is a refund of
            // nothing: `0023` refuses it at COMMIT, and "the purchase did not owe" is the
            // caller's fact to learn.
            None => return Err(StatusCode::CONFLICT),
            _ => {}
        }
    }

    let return_id = sqlx::query_scalar!(
        "INSERT INTO supplier_returns (purchase_id, total, payment_method_id) VALUES ($1, $2, $3) RETURNING id",
        purchase_id,
        return_total,
        return_method
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
        "SELECT id, purchase_id, payment_method_id,
                round(total, 2) AS \"total!\", created_at
         FROM supplier_returns WHERE id = $1",
        return_id
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "supplier return readback failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // **The return claims one of two things, exactly as a credit note does since `0021`.** A
    // return with a method brings money back: it writes the book entry (`0017`) and lowers no
    // debt. A return with no method lowers the debt: it writes a `payables_adjustments` row and
    // no entry. `0023`'s constraint trigger refuses a return that does both or neither, at COMMIT.
    match return_method {
        Some(method) => {
            // The book entry (`0017`): a supplier return brings money BACK, so it is positive.
            sqlx::query!(
                "INSERT INTO money_entries
                    (account_id, payment_method_id, document_kind, document_id, amount, occurred_at, created_by)
                 SELECT pm.account_id, $1, 'supplier_return', $2, $3, now(), 'counter'
                   FROM payment_methods pm WHERE pm.id = $1",
                method,
                return_id,
                return_total
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "supplier return money entry failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }
        None => {
            // The debt drops. `document_id` is the return, so the same return can never lower the
            // debt twice (`0023`'s UNIQUE index) — the guarantee a mutable `payables.amount` would
            // have needed a writer flag to get.
            let adjusted = sqlx::query(
                "INSERT INTO payables_adjustments
                    (payable_id, amount, document_kind, document_id, created_by)
                 SELECT o.id, $2, 'supplier_return', $3, 'counter'
                   FROM payables o
                  WHERE o.source_document_kind = 'purchase' AND o.source_document_id = $1",
            )
            .bind(purchase_id)
            .bind(return_total)
            .bind(return_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                // A ceiling violation is the caller's, not the server's: the pre-check above
                // answers 409 for the case it can see, and this catches a concurrent settlement
                // that committed between the read and the insert.
                if let sqlx::Error::Database(db) = &e
                    && db.code().as_deref() == Some("P0001")
                {
                    return StatusCode::CONFLICT;
                }
                tracing::error!(?e, "supplier return payables adjustment failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            if adjusted.rows_affected() == 0 {
                tracing::error!(
                    purchase_id = %purchase_id,
                    "supplier return with no method on a purchase that owes nothing"
                );
                return Err(StatusCode::CONFLICT);
            }
        }
    }

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
