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
use crate::db::conflict_or_500;
use crate::models::account::{CASH_ACCOUNT_ID, CASH_PAYMENT_METHOD_ID, CREDIT_ACCOUNT_ID};
use crate::models::destination::{self, Destination};
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
        .route("/sales/{id}/annulments", post(create_annulment))
        .route("/sales/summary", get(sales_summary))
}

#[derive(Debug, Deserialize)]
pub struct NewSale {
    /// Omitted means the seeded walk-in customer.
    #[serde(default)]
    pub customer_id: Option<Uuid>,
    /// Where the money went, or **`"on_account"` for a sale that moved none**
    /// (`odd/tasks/v4-receivables.md` S1, S2).
    ///
    /// Three inputs and three meanings, and they are deliberately distinct:
    ///
    /// | input | meaning |
    /// |---|---|
    /// | omitted | the seeded `cash` method, which is what every sale meant before v3 — an older client keeps its exact behaviour |
    /// | a uuid | money moved, through that method |
    /// | `"on_account"` | **money did not move**; the sale owes, and `obligations` records it |
    ///
    /// **Why a sentinel and not `null`.** `Option<Uuid>` cannot tell `"payment_method_id":
    /// null` from an absent key, so null would have to mean either "the till" — silently
    /// turning a debt into a cash sale — or "on account", silently turning every older
    /// client's omitted field into a debt. A sentinel is the only spelling where all three
    /// inputs stay distinguishable through serde, and `"on_account"` is refused as a method
    /// name by the uuid parse it now fails.
    ///
    /// **Why not `credit`.** The method resolves to an account, and a sale that moved money
    /// into an account is not a debt. Naming it is refused with a status the caller can act
    /// on, instead of the 500 `0021`'s constraint trigger would answer with.
    ///
    /// An unknown id is a 404, and a sale on account must name a real customer (S9):
    /// anonymous debt is not debt.
    ///
    /// Kept raw here and judged by `Destination::from_field`, the one shared definition
    /// (`odd/tasks/v4-payables.md` S3). `keep_present` preserves an explicit null as
    /// distinct from an omitted key, which `Option<Uuid>` cannot do.
    #[serde(default, deserialize_with = "destination::keep_present")]
    pub payment_method_id: Option<serde_json::Value>,
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
    //
    // `None` is the sale on account: money did not move, so there is no method to
    // resolve and no account to name. It is not "the till" — omitted means the till and
    // is resolved below, which is what keeps every older client's behaviour intact.
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

    // **A sale claims one of two things, and the debt account is neither.** Naming
    // `credit` would say "money moved into an account that holds nothing", which is the
    // defect this slice removes (`v3-accounts.md` L4-L5). The constraint trigger in
    // `0021` refuses it; this is the status code, not the rule. A boolean refusal is the
    // honest answer for an incoherent claim, and it is the one this path already
    // answered before the column became nullable.
    if let Some((_, account_id)) = destination
        && account_id == CREDIT_ACCOUNT_ID
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    // S9: a sale on account must name a real customer. The check is written
    // **positively first** in the tests — a sale on account to a named customer is
    // accepted — because a refusal on its own could pass for the wrong reason.
    //
    // `is_active` is deliberately NOT consulted: it is a picker concern, and gating
    // writes on it would make this refusal fire because the seeded `credit` method is
    // inactive rather than because the customer is anonymous.
    if destination.is_none() && customer_id == WALK_IN_CUSTOMER_ID {
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

    // The sale header. `payment_method_id` is NULL for a sale on account, and that NULL
    // is the sale's own statement that it moved no money — `0021`'s constraint trigger
    // holds it against the obligation written below, at COMMIT.
    let sale_id = sqlx::query_scalar!(
        "INSERT INTO sales (customer_id, total, payment_method_id) VALUES ($1, $2, $3) RETURNING id",
        customer_id,
        total,
        destination.map(|(method, _)| method)
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
    // (created_at) instead of echoing the request.
    //
    // **`LEFT JOIN`, and it is not a stylistic choice.** A sale on account has no method,
    // and an `INNER JOIN` would drop it from this readback entirely — turning a correct
    // insert into a 500. `payment_method_name` is therefore nullable in the response, which
    // is the honest shape: a sale that moved no money has no rail to name.
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
                s.payment_method_id,
                CASE WHEN s.payment_method_id IS NULL THEN NULL ELSE pm.name END
                    AS \"payment_method_name\",
                s.created_at
         FROM sales s
         LEFT JOIN payment_methods pm ON pm.id = s.payment_method_id
         WHERE s.id = $1",
        sale_id
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "sale readback failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // **The sale claims one of two things, and only one path runs.** A sale that moved
    // money writes its book entry (`0017`) and owes nothing; a sale on account writes an
    // obligation and no entry at all (`S1`). Writing neither, or both, is what `0021`'s
    // constraint trigger refuses at COMMIT — this is the write path agreeing with it.
    match destination {
        Some((method, _)) => {
            // One row per effect of money, written in the same transaction as the
            // document that causes it. `account_id` comes from the method, and `0017`'s
            // trigger re-checks that the two agree — an account the method does not
            // resolve to is not expressible, rather than merely discouraged.
            sqlx::query!(
                "INSERT INTO money_entries
                    (account_id, payment_method_id, document_kind, document_id, amount, occurred_at, created_by)
                 SELECT pm.account_id, $1, 'sale', $2, $3, $4, 'counter'
                   FROM payment_methods pm WHERE pm.id = $1",
                method,
                sale_id,
                total,
                sale.created_at
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "sale money entry failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }
        None => {
            // The debt. `party_id` is the sale's own customer, and `0021`'s trigger
            // re-derives it from the sale rather than trusting this bind — a debt
            // attributed to someone who never bought anything is not expressible.
            sqlx::query!(
                "INSERT INTO obligations
                    (source_document_kind, source_document_id, party_kind, party_id,
                     amount, occurred_at, created_by)
                 VALUES ('sale', $1, 'customer', $2, $3, $4, 'counter')",
                sale_id,
                customer_id,
                total,
                sale.created_at
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                // A sale that already owes cannot owe twice, and this path cannot reach
                // that state through HTTP — the sale is inserted in this same
                // transaction. A 500 here is the honest answer to a case the route
                // cannot produce, rather than a status pretending it is reachable.
                tracing::error!(?e, "sale obligation failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }
    }

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
        // **The budget comes from the line, not from a sum over the notes** (`0019`, S9).
        // It used to be `SUM(credit_note_items.quantity)`, which meant a second document
        // that reverses stock had no way to consume it — and a `psql` session could
        // exceed it entirely. The column is the one place both documents write.
        let line = sqlx::query!(
            "SELECT si.product_id, si.product_name, si.quantity, si.unit_price, si.unit,
                    si.reversed_quantity
             FROM sale_items si
             WHERE si.id = $1 AND si.sale_id = $2",
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
            .or_insert(line.reversed_quantity as i64);
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
    // **Inheritance is the default, and since `0021` it means two different things.**
    // On a sale that moved money, the note returns money through the same rail. On a sale
    // that only owed, the sale's method is NULL, so the note inherits NULL — writes no
    // ledger entry, and lowers the debt through an `obligations_adjustments` row. That is
    // the honest version of what the old code faked by routing the note through `credit`,
    // and it is why `v3-accounts.md` L107's refund of a sale nobody paid no longer empties
    // the till.
    //
    // The override exists for the sale that was already collected — and since `S7` it is
    // conditional on that having actually happened.
    let sale_method: Option<Uuid> =
        sqlx::query_scalar!("SELECT payment_method_id FROM sales WHERE id = $1", sale_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "sale payment method read failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let note_method: Option<Uuid> = match body.payment_method_id {
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
            // A note may not target the debt account, which is now redundant with
            // `0021`'s refusal on every ledger row — and is kept anyway. The schema is the
            // rule; this is the 400 that tells the caller, and a removed pre-check would
            // turn a caller's mistake into a 500 the first time the trigger fired.
            if account_id == CREDIT_ACCOUNT_ID {
                return Err(StatusCode::BAD_REQUEST);
            }

            // **`v3-accounts.md` L107, closed here.** An explicit method means money goes
            // back through a rail — so that rail must have received that money first. The
            // measured defect was a refund of a sale nobody paid taking 100 out of the
            // till and leaving the debt standing: the shop paid a customer who had not
            // paid it. `SUM(settlements)` is the answer the question always needed, and it
            // only exists from this slice on.
            //
            // **Only the explicit path is checked, and that asymmetry is the design**
            // (`v4-settlement.md` S14). The inherited path lowers the obligation and moves
            // nothing, so it has no precondition to check.
            //
            // **What arrived is a budget, and earlier notes spend from it** (`R1-003` /
            // `R3-003`). The first version compared each note against the settled total and
            // never subtracted the explicit notes already issued, so several notes each
            // passed alone while together paying out more than ever arrived. Measured: a
            // 200.00 on-account sale settled for 100.00 admitted two 100.00 cash notes, and
            // the till ended lower than it had ever been higher.
            //
            // Both sums are taken with the sale's row already locked (`FOR UPDATE` above),
            // so a concurrent collection or refund cannot be counted after the read.
            let settled: Decimal = sqlx::query_scalar(
                "SELECT COALESCE(SUM(s.amount), 0)
                   FROM settlements s
                   JOIN obligations o ON o.id = s.obligation_id
                  WHERE o.source_document_kind = 'sale' AND o.source_document_id = $1",
            )
            .bind(sale_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "sale settled sum failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

            // A note on a sale that was paid outright has no obligation at all, and its
            // `SUM(settlements)` is zero — but its money *did* arrive, at the sale. So the
            // question is not "was it settled" but "did this sale ever take in at least
            // this much": a paid sale took in its total, and an owed one took in its
            // settlements.
            let arrived = if sale_method.is_some() {
                sqlx::query_scalar::<_, Decimal>("SELECT total FROM sales WHERE id = $1")
                    .bind(sale_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|e| {
                        tracing::error!(?e, "sale total read failed");
                        StatusCode::INTERNAL_SERVER_ERROR
                    })?
            } else {
                settled
            };

            // What earlier explicit notes already took out of a real rail. A note with a
            // method writes a `credit_note` entry; one that inherited the sale's NULL
            // writes none, so this sum counts exactly the refunds that moved money — which
            // is the budget being spent.
            let already_refunded: Decimal = sqlx::query_scalar(
                "SELECT COALESCE(SUM(me.amount), 0)
                   FROM money_entries me
                   JOIN credit_notes cn ON cn.id = me.document_id
                  WHERE me.document_kind = 'credit_note' AND cn.sale_id = $1",
            )
            .bind(sale_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "sale refunded sum failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            // Entries are signed: a refund is negative, so its magnitude is what left.
            let spent = -already_refunded;

            if arrived - spent < note_total {
                return Err(StatusCode::CONFLICT);
            }

            // **And it must have arrived on the rail the note names** (`R1-refund-guard-rail-blind`).
            // The check above is a budget over the sale, and a budget is rail-blind: the money
            // came in somewhere, so *some* rail received it. But a refund leaves through the
            // account the note names, and money that arrived in the till did not arrive in the
            // bank. Measured before this guard: a sale collected in cash, refunded by transfer,
            // answered 201 and drove the bank to -100.00 — the harm this whole guard exists to
            // close, through a side door.
            //
            // **A rail's arrival is a sum over this sale's own documents**, not over the party:
            // what this sale put into that account.
            //   - paid outright: the sale's single entry, on the sale's own method
            //   - on account:     the settlements whose payment used that method
            //
            // and what earlier explicit notes already took back out through that same account.
            let arrived_on_rail: Decimal = sqlx::query_scalar(
                "SELECT COALESCE((
                    SELECT SUM(me.amount) FROM money_entries me
                     WHERE me.document_kind = 'sale' AND me.document_id = $1
                       AND me.payment_method_id = $2
                 ), 0)
                 + COALESCE((
                    SELECT SUM(s.amount) FROM settlements s
                      JOIN obligations o ON o.id = s.obligation_id
                      JOIN payments p ON p.id = s.payment_id
                     WHERE o.source_document_kind = 'sale' AND o.source_document_id = $1
                       AND p.direction = 'in'
                       AND p.payment_method_id = $2
                 ), 0)",
            )
            .bind(sale_id)
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "sale rail arrival failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

            // What earlier notes already paid out through this same rail, as a magnitude.
            let left_on_rail: Decimal = sqlx::query_scalar(
                "SELECT COALESCE(SUM(me.amount), 0)
                   FROM money_entries me
                   JOIN credit_notes cn ON cn.id = me.document_id
                  WHERE me.document_kind = 'credit_note' AND cn.sale_id = $1
                    AND me.payment_method_id = $2",
            )
            .bind(sale_id)
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "sale rail refunds failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

            if arrived_on_rail + left_on_rail < note_total {
                return Err(StatusCode::CONFLICT);
            }

            Some(id)
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

    // **The note claims one of two things, exactly as the sale did.** A note with a method
    // returns money: it writes the book entry (`0017`) and lowers no debt. A note with no
    // method lowers the debt: it writes an `obligations_adjustments` row and no entry.
    // `0021`'s constraint trigger refuses a note that does both or neither, at COMMIT.
    match note_method {
        Some(method) => {
            // Money leaves the account the note's method resolves to. `-total` because
            // positive is money entering.
            let note_amount: Decimal = -note_total;
            sqlx::query!(
                "INSERT INTO money_entries
                    (account_id, payment_method_id, document_kind, document_id, amount, occurred_at, created_by)
                 SELECT pm.account_id, $1, 'credit_note', $2, $3, $4, 'counter'
                   FROM payment_methods pm WHERE pm.id = $1",
                method,
                note_id,
                note_amount,
                note.created_at
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "credit note money entry failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }
        None => {
            // The debt drops. `document_id` is the note, so the same note can never lower
            // the debt twice (`0021`'s UNIQUE index) — the guarantee a mutable
            // `obligations.amount` would have needed a writer flag to get.
            //
            // **The combined ceiling is checked here, before the insert** (`R1-004`,
            // `R4-001`). `0022` re-cut `obligation_adjustments_ceiling` to include
            // settlements, so a note larger than the debt's remaining room raises at COMMIT
            // — and a `RAISE` at COMMIT reaches the caller as a 500. The settlement handler
            // pre-checks the same ceiling and answers 409; this path did not, which left a
            // reachable ordinary operation with no actionable status. Measured before the
            // check: returning the goods on an already-settled on-account sale answered 500.
            // **`FOR UPDATE`, and it is the lock the settlement writer also takes** (`R1-003`).
            // Without it a settlement could commit between this read and the note's COMMIT:
            // the pre-check would pass, `0022`'s deferred ceiling would then raise, and the
            // caller would get the 500 this pre-check exists to remove. The database still
            // refuses correctly — no over-lowering survives — but the status code lied.
            //
            // Ordering matters and matches `create_settlement`: obligation first. Two writers
            // taking the same rows in the same order cannot deadlock.
            let room: Option<(Decimal, Decimal, Decimal)> = sqlx::query_as(
                "SELECT o.amount,
                        COALESCE((SELECT SUM(a.amount) FROM obligations_adjustments a
                                   WHERE a.obligation_id = o.id), 0),
                        COALESCE((SELECT SUM(s.amount) FROM settlements s
                                   WHERE s.obligation_id = o.id), 0)
                   FROM obligations o
                  WHERE o.source_document_kind = 'sale' AND o.source_document_id = $1
                  FOR UPDATE",
            )
            .bind(sale_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "obligation room read failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

            if let Some((owed, lowered, settled)) = room
                && lowered + settled + note_total > owed
            {
                return Err(StatusCode::CONFLICT);
            }

            let adjusted = sqlx::query!(
                "INSERT INTO obligations_adjustments
                    (obligation_id, amount, document_kind, document_id, created_by)
                 SELECT o.id, $2, 'credit_note', $3, 'counter'
                   FROM obligations o
                  WHERE o.source_document_kind = 'sale' AND o.source_document_id = $1",
                sale_id,
                note_total,
                note_id
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                // **A ceiling violation is the caller's, not the server's** (review
                // `R4-ceiling-trigger-returns-500`). The pre-check above answers 409 for the
                // case it can see; this catches what it cannot — a concurrent settlement
                // that committed between the read and the insert. The database refuses
                // correctly either way; only the status code differed, and 500 blamed the
                // server for a race a caller can retry.
                if let sqlx::Error::Database(db) = &e
                    && db.code().as_deref() == Some("P0001")
                {
                    return StatusCode::CONFLICT;
                }
                tracing::error!(?e, "credit note obligation adjustment failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

            // A note with no method on a sale with no obligation would move no money and
            // lower no debt: a refund of nothing. `0021` refuses it at COMMIT; this names
            // it, because "the sale did not owe" is the caller's fact to learn.
            if adjusted.rows_affected() == 0 {
                tracing::error!(
                    sale_id = %sale_id,
                    "credit note with no method on a sale that owes nothing"
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
        //
        // **`LEFT JOIN`, and this is the difference between a sale on account appearing
        // in the ledger or vanishing from it.** A sale that moved no money has no method,
        // so an `INNER JOIN` would silently drop every such sale from the day's list while
        // the money book stayed consistent — the worst shape of defect: a missing document
        // nobody is told about. `payment_method_name` is `Option` in the model for the
        // same reason.
        "SELECT s.id, s.customer_id, round(s.total, 2) AS \"total!\",
                s.payment_method_id,
                CASE WHEN s.payment_method_id IS NULL THEN NULL ELSE pm.name END
                    AS \"payment_method_name\",
                s.created_at, c.name AS customer_name
         FROM sales s
         JOIN customers c ON c.id = s.customer_id
         LEFT JOIN payment_methods pm ON pm.id = s.payment_method_id
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
             WHERE -- Absent bound means unbounded, the same contract `list_sales`
               -- has. A sentinel instant cannot be used here: MIN_UTC is outside
               -- Postgres's timestamp range and the whole period reads as empty.
               ($1::timestamptz IS NULL OR s.created_at >= $1)
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

// ── Annulment ───────────────────────────────────────────────────────────────
//
// `odd/tasks/v4-settlement.md` S10, S11. A sale reported wrongly — the amount, the
// method, the customer, or the document itself — is corrected by **a new document**, and
// never by editing or deleting the sale. Before `0020` the only exit was a full credit
// note, which means "the customer returned the goods": the stock moved as if merchandise
// came back, and it never did.
//
// **Stock is always returned** (`S10`, the owner's decision): the goods never left, so the
// annulment puts them back. The stock movement is `initial`'s mirror with a reason naming
// the annulment, and it is the one place in the system where a returning movement is not a
// refund.

#[derive(Debug, Deserialize)]
pub struct NewAnnulment {
    /// Why the sale is wrong. A closed set, like `stock_movements.reason`.
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct Annulment {
    pub id: Uuid,
    pub sale_id: Uuid,
    pub reason: String,
    pub created_at: DateTime<Utc>,
}

async fn create_annulment(
    State(state): State<AppState>,
    Path(sale_id): Path<Uuid>,
    Json(body): Json<NewAnnulment>,
) -> Result<(StatusCode, Json<Annulment>), StatusCode> {
    if !matches!(
        body.reason.as_str(),
        "wrong_amount" | "wrong_method" | "wrong_customer" | "wrong_document"
    ) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!(?e, "tx begin failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // The sale's row lock, for the same reason the refund path takes it: the reversal
    // budget is judged against the committed state.
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

    // What the sale moved, so the annulment reverses exactly that.
    let sale: Option<(Uuid, Decimal, DateTime<Utc>)> =
        sqlx::query_as("SELECT payment_method_id, total, created_at FROM sales WHERE id = $1")
            .bind(sale_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "sale read failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    let (method, _total, created) = sale.ok_or(StatusCode::NOT_FOUND)?;

    // **Read what is left BEFORE inserting the document.** The annulment's trigger consumes
    // the budget on insert (`0020`), so a read placed after it sees `reversed_quantity =
    // quantity` everywhere and computes zero — which is exactly the defect measured while
    // fixing `R3-001`: the annulment became a no-op, the till stayed at 200 and the stock
    // at 8. Order here is not style; it is the difference between reversing the remainder
    // and reversing nothing.
    let remaining: Vec<(Uuid, String, String, i32, Decimal, String)> = sqlx::query_as(
        "SELECT product_id, product_name, unit, quantity - reversed_quantity,
                unit_price, unit
           FROM sale_items
          WHERE sale_id = $1 AND quantity > reversed_quantity",
    )
    .bind(sale_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "remaining sale lines read failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if remaining.is_empty() {
        // Nothing left to annul: the whole sale was already reversed by credit notes. The
        // schema refuses this too (`R3-002`); asking first is what makes it a status the
        // caller can act on instead of a 500 from a trigger.
        return Err(StatusCode::CONFLICT);
    }

    // The document. Its trigger consumes the reversal budget for every line with budget
    // left, which is what stops an annulment and a credit note reversing the same unit.
    let annulment = sqlx::query_as::<_, (Uuid, Uuid, String, DateTime<Utc>)>(
        "INSERT INTO annulments (sale_id, reason, created_by)
         VALUES ($1, $2, 'counter')
         RETURNING id, sale_id, reason, created_at",
    )
    .bind(sale_id)
    .bind(&body.reason)
    .fetch_one(&mut *tx)
    .await
    // A second annulment of the same sale is a 409, not a 500: the unique index is the
    // rule and the caller can act on it.
    .map_err(conflict_or_500)?;

    // The money to give back is the sum of those lines at their snapshotted price, which
    // is what the sale charged — not the sale total, which still includes what a credit
    // note already returned.
    let amount: Decimal = -remaining
        .iter()
        .map(|(_, _, _, qty, price, _)| *price * Decimal::from(*qty))
        .fold(Decimal::ZERO, |acc, line| acc + line);
    sqlx::query(
        "INSERT INTO money_entries
            (account_id, payment_method_id, document_kind, document_id, amount, occurred_at, created_by)
         SELECT pm.account_id, $1, 'annulment', $2, $3, $4, 'counter'
           FROM payment_methods pm WHERE pm.id = $1",
    )
    .bind(method)
    .bind(annulment.0)
    .bind(amount)
    .bind(created)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "annulment money entry failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Stock back in for **the units still on the line**, not the whole line: the two
    // columns differ once a credit note has taken part of it (`R3-001`).
    for (product_id, product_name, _unit, quantity, unit_price, unit) in &remaining {
        let line_total: Decimal = *unit_price * Decimal::from(*quantity);
        sqlx::query(
            "INSERT INTO stock_movements (product_id, quantity, reason, sale_id, annulment_id)
             VALUES ($1, $2, 'sale_annulled', $3, $4)",
        )
        .bind(product_id)
        .bind(quantity)
        .bind(sale_id)
        .bind(annulment.0)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(
                ?e,
                "annulment stock movement failed for {product_name} / {unit} / {line_total}"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "tx commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::CREATED,
        Json(Annulment {
            id: annulment.0,
            sale_id: annulment.1,
            reason: annulment.2,
            created_at: annulment.3,
        }),
    ))
}
