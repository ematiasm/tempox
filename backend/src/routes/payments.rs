//! Money arriving, and the rows that say which debt it paid.
//!
//! `odd/tasks/v4-receivables.md` S4-S6, migration `0022`. `0017` gave money a book and
//! `0021` gave debt a document; this is what connects them.
//!
//! | document | what it is | what it writes |
//! |---|---|---|
//! | a payment | money moving on one rail | one ledger entry, signed by its direction |
//! | a settlement | which debt that money paid, and how much | nothing — the money already moved |
//!
//! **The settlement writing nothing is the distinction this module exists to keep.** A
//! collection moves money into the till; applying it to a debt only says which debt it
//! answered. Writing a ledger entry for the settlement would count the same money twice.
//!
//! **`payments.total` and `settlements.amount` are ceilings, and both are sums.** One
//! payment may cover several debts and one debt may be paid in parts, so neither side is a
//! single row and neither ceiling is a comparison of stored fields. `0022` owns the rule;
//! this module asks first, because a `RAISE` at COMMIT reaches the caller as a 500.
//!
//! **An unapplied payment is the customer's credit balance, and there is no table for
//! it** (`S6`). It is `payments.total - SUM(settlements)`, derived, for the same reason a
//! balance is a sum over `money_entries`: a subtraction of stored rows cannot drift. It is
//! **manual** (`v4-settlement.md` L22, the owner's *"si, vamos por manual"*): applying it to
//! the next sale is a person's decision, and a customer saving up must not lose their
//! balance to a small sale.

use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::db::conflict_or_500;
use crate::models::account::CREDIT_ACCOUNT_ID;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/payments", post(create_payment))
        .route("/settlements", post(create_settlement))
}

// ── A payment ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NewPayment {
    pub party_kind: String,
    pub party_id: Uuid,
    pub payment_method_id: Uuid,
    /// A decimal string, not a JSON number: a float would lose the cent.
    pub total: Decimal,
    /// `in` is a customer paying us. `out` is us paying a supplier, or refunding a
    /// customer's credit balance.
    pub direction: String,
    #[serde(default)]
    pub occurred_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct Payment {
    pub id: Uuid,
    pub party_kind: String,
    pub party_id: Uuid,
    pub payment_method_id: Uuid,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub total: Decimal,
    pub direction: String,
    pub occurred_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    /// What has not been applied to a debt yet: the customer's credit balance, or what is
    /// left of a payment that only covered part of what it was for. **Derived rather than
    /// stored**, so it cannot disagree with the settlements that produce it.
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub unapplied: Decimal,
}

async fn create_payment(
    State(state): State<AppState>,
    Json(body): Json<NewPayment>,
) -> Result<(StatusCode, Json<Payment>), StatusCode> {
    // Shape first, with nothing opened and nothing locked. The database refuses each of
    // these with a CHECK; asking first is what gives the caller a status it can act on.
    if body.total <= Decimal::ZERO {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !matches!(body.direction.as_str(), "in" | "out") {
        return Err(StatusCode::BAD_REQUEST);
    }
    // A customer pays us, we pay a supplier, and we may refund a customer's balance. The
    // crossed pair — `in` from a supplier — is not a document this schema describes.
    if !matches!(body.party_kind.as_str(), "customer" | "supplier")
        || (body.direction == "in" && body.party_kind != "customer")
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!(?e, "tx begin failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // The method has to exist, and it has to name a real rail. A collection through the
    // debt account would say "money arrived in an account that holds nothing", which is
    // the defect `0021` removed from the sale side and does not reopen here. `0021`'s
    // trigger on the book would refuse this too; this is the 400 that tells the caller.
    let account_id: Option<Uuid> =
        sqlx::query_scalar("SELECT account_id FROM payment_methods WHERE id = $1")
            .bind(body.payment_method_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "payment method lookup failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    let account_id = account_id.ok_or(StatusCode::NOT_FOUND)?;
    if account_id == CREDIT_ACCOUNT_ID {
        return Err(StatusCode::BAD_REQUEST);
    }

    // The party has to exist. A payment from a customer nobody has recorded is a document
    // whose counterparty cannot be named, and the FK-less `party_id` cannot say so — this
    // is where it is said.
    //
    // **Two statements, not one with a table name interpolated.** `party_kind` is validated
    // above, so the interpolation would be safe — but sqlx refuses a dynamic SQL string on
    // principle, and it is right to: the next person to add a kind would inherit a pattern
    // whose safety depends on a check three branches away.
    let found: i64 = if body.party_kind == "customer" {
        sqlx::query_scalar("SELECT count(*) FROM customers WHERE id = $1")
            .bind(body.party_id)
            .fetch_one(&mut *tx)
            .await
    } else {
        sqlx::query_scalar("SELECT count(*) FROM suppliers WHERE id = $1")
            .bind(body.party_id)
            .fetch_one(&mut *tx)
            .await
    }
    .map_err(|e| {
        tracing::error!(?e, "party lookup failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if found == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    let payment = sqlx::query_as::<_, (Uuid, DateTime<Utc>, DateTime<Utc>)>(
        "INSERT INTO payments
            (party_kind, party_id, payment_method_id, total, direction, occurred_at, created_by)
         VALUES ($1, $2, $3, $4, $5, COALESCE($6, NOW()), 'counter')
         RETURNING id, occurred_at, created_at",
    )
    .bind(&body.party_kind)
    .bind(body.party_id)
    .bind(body.payment_method_id)
    .bind(body.total)
    .bind(&body.direction)
    .bind(body.occurred_at)
    .fetch_one(&mut *tx)
    .await
    .map_err(conflict_or_500)?;

    // **One entry, signed by the direction.** Money in is positive, money out negative —
    // `0014`'s convention, declared there and never re-derived. `0022`'s constraint trigger
    // refuses a payment that wrote none, so this is the write path agreeing with the rule
    // rather than being the rule.
    let signed = if body.direction == "in" {
        body.total
    } else {
        -body.total
    };
    sqlx::query(
        "INSERT INTO money_entries
            (account_id, payment_method_id, document_kind, document_id, amount, occurred_at, created_by)
         SELECT pm.account_id, $1, 'payment', $2, $3, $4, 'counter'
           FROM payment_methods pm WHERE pm.id = $1",
    )
    .bind(body.payment_method_id)
    .bind(payment.0)
    .bind(signed)
    .bind(payment.1)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "payment money entry failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "tx commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::CREATED,
        Json(Payment {
            id: payment.0,
            party_kind: body.party_kind,
            party_id: body.party_id,
            payment_method_id: body.payment_method_id,
            total: body.total,
            direction: body.direction,
            occurred_at: payment.1,
            created_at: payment.2,
            // Nothing has been applied yet: a payment is created unapplied, and applying it
            // is a separate document a person writes.
            unapplied: body.total,
        }),
    ))
}

// ── A settlement ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NewSettlement {
    pub obligation_id: Uuid,
    pub payment_id: Uuid,
    pub amount: Decimal,
}

#[derive(Debug, Serialize)]
pub struct Settlement {
    pub id: Uuid,
    pub obligation_id: Uuid,
    pub payment_id: Uuid,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub amount: Decimal,
    pub created_at: DateTime<Utc>,
    /// What the debt still owes afterwards, and what the payment still has free. Both
    /// derived, and both returned because a caller applying a balance needs to know where
    /// it stands without asking again.
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub obligation_outstanding: Decimal,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub payment_unapplied: Decimal,
}

async fn create_settlement(
    State(state): State<AppState>,
    Json(body): Json<NewSettlement>,
) -> Result<(StatusCode, Json<Settlement>), StatusCode> {
    if body.amount <= Decimal::ZERO {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!(?e, "tx begin failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // **Both sides are locked before either ceiling is read.** Without this, two concurrent
    // settlements of the same payment would each read the same `SUM(applied)`, each find
    // room, and each write — over-applying in exactly the shape `create_credit_note`
    // records for over-refunding a line. The lock order is obligation then payment, and it
    // is fixed here so two callers cannot deadlock by taking them in opposite orders.
    let owed: Option<Decimal> =
        sqlx::query_scalar("SELECT amount FROM obligations WHERE id = $1 FOR UPDATE")
            .bind(body.obligation_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "obligation lock failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    let owed = owed.ok_or(StatusCode::NOT_FOUND)?;

    let paid: Option<Decimal> =
        sqlx::query_scalar("SELECT total FROM payments WHERE id = $1 FOR UPDATE")
            .bind(body.payment_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "payment lock failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    let paid = paid.ok_or(StatusCode::NOT_FOUND)?;

    // The two ceilings, each asking the question the database will ask again. The
    // obligation's is net of what a *document* already lowered — a credit note on an unpaid
    // sale reduces what a payment may still cover, and judging the two apart would let a
    // debt be resolved twice (`0022`'s `settlement_within_both_ceilings`).
    let already_settled: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount), 0) FROM settlements WHERE obligation_id = $1",
    )
    .bind(body.obligation_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "obligation settled sum failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let already_lowered: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount), 0) FROM obligations_adjustments WHERE obligation_id = $1",
    )
    .bind(body.obligation_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "obligation lowered sum failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if already_settled + already_lowered + body.amount > owed {
        return Err(StatusCode::CONFLICT);
    }

    let already_applied: Decimal =
        sqlx::query_scalar("SELECT COALESCE(SUM(amount), 0) FROM settlements WHERE payment_id = $1")
            .bind(body.payment_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "payment applied sum failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    if already_applied + body.amount > paid {
        return Err(StatusCode::CONFLICT);
    }

    let created: (Uuid, DateTime<Utc>) = sqlx::query_as(
        "INSERT INTO settlements (obligation_id, payment_id, amount, created_by)
         VALUES ($1, $2, $3, 'counter') RETURNING id, created_at",
    )
    .bind(body.obligation_id)
    .bind(body.payment_id)
    .bind(body.amount)
    .fetch_one(&mut *tx)
    .await
    // A second settlement of the same pair is the unique index: one row written twice is
    // not a partial payment, and the caller can act on a 409.
    .map_err(conflict_or_500)?;

    // **No ledger entry, and this is the line that must never be added.** The money moved
    // when the payment was recorded; the settlement only says which debt it answered.
    // Writing an entry here would count the same coin twice.

    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "tx commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::CREATED,
        Json(Settlement {
            id: created.0,
            obligation_id: body.obligation_id,
            payment_id: body.payment_id,
            amount: body.amount,
            created_at: created.1,
            obligation_outstanding: owed - already_settled - already_lowered - body.amount,
            payment_unapplied: paid - already_applied - body.amount,
        }),
    ))
}
