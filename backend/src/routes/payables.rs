//! Paying a supplier, and the rows that say which payable the money settled.
//!
//! `odd/tasks/v4-payables.md` S6, S7, S10, migration `0023`. The mirror of
//! [`crate::routes::payments`], with the direction reversed: there, money arrives and the debt
//! is what a customer owes us; here, money leaves and the debt is what we owe a supplier.
//!
//! | document | what it is | what it writes |
//! |---|---|---|
//! | a payment, `direction = out` | money we sent a supplier | one negative ledger entry |
//! | a payable settlement | which payable that money answered, and how much | nothing |
//!
//! **The settlement writing nothing is the distinction this module exists to keep.** The money
//! moved when the payment was recorded; applying it to a payable only says which debt it
//! answered. A ledger entry here would count the same payment twice — the sentence
//! `payments.rs` writes for the customer side, and the reason it is repeated rather than
//! referenced is that the two modules must not drift apart.
//!
//! **`payables.amount` and `payments.total` are ceilings, and both are sums.** A supplier payment
//! may cover several payables and a payable may be paid in parts, so neither side is a single
//! row and neither ceiling is a comparison of stored fields. `0023` owns the rule; this module
//! asks first, because a `RAISE` reaches the caller as a 500.

use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::db::conflict_or_500;

pub fn routes() -> Router<AppState> {
    Router::new().route("/payable-settlements", post(create_payable_settlement))
}

// ── The one definition of "how much money this supplier still holds of ours" ──
//
/// How much we have sent a party and **not yet applied to a debt**, with the party's payment
/// rows locked for the rest of the transaction.
///
/// ```text
///   + SUM(payments out)          money we sent
///   - SUM(payable_settlements)   money already applied to a payable
/// ```
///
/// **Two terms, not three, and the asymmetry is structural rather than an omission.** The
/// customer-side `lock_party_balance` in `routes::payments` has a third term for money
/// that arrived, because a customer pays us; a supplier cannot,
/// `0023`'s `payments_direction_matches_party` (carried from `0022`) makes `in` from a supplier
/// inexpressible. This is stated in `v4-payables.md` L5 and repeated here so a reader comparing
/// the two functions does not read a missing term as a forgotten one.
///
/// **The lock is the point of the `FOR UPDATE`.** Reading a balance and then inserting is a race
/// with no symptom until two requests overlap; taking the party's payment rows first serialises
/// them. `payable_settlements` is read without its own lock because a settlement can only be
/// written by a caller that already holds the payment row — the lock chain closes it.
async fn lock_party_sent(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    party_kind: &str,
    party_id: Uuid,
) -> Result<Decimal, StatusCode> {
    let _rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM payments
          WHERE party_kind = $1 AND party_id = $2
          FOR UPDATE",
    )
    .bind(party_kind)
    .bind(party_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "party payment lock failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let sent: Decimal = sqlx::query_scalar(
        "SELECT COALESCE((SELECT SUM(p.total) FROM payments p
                           WHERE p.party_kind = $1 AND p.party_id = $2 AND p.direction = 'out'), 0)
                - COALESCE((SELECT SUM(ps.amount) FROM payable_settlements ps
                              JOIN payments sp ON sp.id = ps.payment_id
                             WHERE sp.party_kind = $1 AND sp.party_id = $2
                               AND sp.direction = 'out'), 0)",
    )
    .bind(party_kind)
    .bind(party_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "party sent lookup failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(sent)
}

// ── A payable settlement ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NewPayableSettlement {
    pub payable_id: Uuid,
    pub payment_id: Uuid,
    /// A decimal string, not a JSON number: a float would lose the cent.
    pub amount: Decimal,
}

#[derive(Debug, Serialize)]
pub struct PayableSettlement {
    pub id: Uuid,
    pub payable_id: Uuid,
    pub payment_id: Uuid,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub amount: Decimal,
    pub created_at: DateTime<Utc>,
    /// What the payable still owes afterwards, and what the payment still has unapplied. Both
    /// derived, and both returned because a caller paying a supplier needs to know where it
    /// stands without asking again — the same contract `payments.rs` gives its settlement.
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub payable_outstanding: Decimal,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub payment_unapplied: Decimal,
}

async fn create_payable_settlement(
    State(state): State<AppState>,
    Json(body): Json<NewPayableSettlement>,
) -> Result<(StatusCode, Json<PayableSettlement>), StatusCode> {
    // Shape first, with nothing opened and nothing locked. The database refuses each of these
    // with a CHECK; asking first is what gives the caller a status it can act on instead of the
    // 500 a `RAISE` surfaces as. `is_whole_cents` is in this list because it was the one the
    // customer-side route missed (`R2-001` of `review-336e2618582310f9`), and a comment claiming
    // every CHECK is pre-checked while one is not is worse than no comment.
    if body.amount <= Decimal::ZERO || !crate::money::is_whole_cents(body.amount) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!(?e, "tx begin failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // **Read the payment's party first, without a lock, only to know whose rows to lock.** This
    // is `R3-003`'s fix on the customer side, reproduced rather than re-derived: the party's
    // payment rows are the resource a refund also takes, so they are acquired before the payable
    // and before the single payment row, in that order, by every writer.
    let payer: Option<(String, Uuid)> =
        sqlx::query_as("SELECT party_kind, party_id FROM payments WHERE id = $1")
            .bind(body.payment_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "payment party read failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    let (party_kind, party_id) = payer.ok_or(StatusCode::NOT_FOUND)?;
    let sent_available = lock_party_sent(&mut tx, &party_kind, party_id).await?;

    // The payable, locked, because both ceilings are judged against the committed state.
    let owed: Option<(Decimal, String, Uuid)> = sqlx::query_as(
        "SELECT amount, party_kind, party_id FROM payables WHERE id = $1 FOR UPDATE",
    )
    .bind(body.payable_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "payable lock failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let (owed, debtor_kind, debtor_id) = owed.ok_or(StatusCode::NOT_FOUND)?;

    // **The payment's direction and party, not just its total.** Three different mistakes, three
    // different answers: the payment must exist (404, above), it must be money that **went out**
    // (400), and it must belong to the **same supplier** the payable belongs to (400).
    //
    // The direction half is the mirror of `R1-002`/`R3-002` of `review-336e2618582310f9`, where a
    // customer's outbound refund settled a debt and resolved the same money twice. Here the
    // failure it prevents is a customer's collection — money that arrived — retiring what we owe
    // a supplier, which would make the shop believe it paid a bill it never paid.
    let paid: Option<(Decimal, String)> =
        sqlx::query_as("SELECT total, direction FROM payments WHERE id = $1 FOR UPDATE")
            .bind(body.payment_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "payment lock failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    let (paid, direction) = paid.ok_or(StatusCode::NOT_FOUND)?;

    if direction != "out" {
        return Err(StatusCode::BAD_REQUEST);
    }
    if party_kind != debtor_kind || party_id != debtor_id {
        return Err(StatusCode::BAD_REQUEST);
    }

    // The two ceilings, each asking the question `0023`'s trigger will ask again. The payable's
    // is net of what a *document* already lowered — a supplier return on an unpaid purchase
    // reduces what a payment may still cover, and judging the two apart would let a debt be
    // resolved twice (`payable_settlement_within_both_ceilings`).
    let already_settled: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount), 0) FROM payable_settlements WHERE payable_id = $1",
    )
    .bind(body.payable_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "payable settled sum failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let already_lowered: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount), 0) FROM payables_adjustments WHERE payable_id = $1",
    )
    .bind(body.payable_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "payable lowered sum failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if already_settled + already_lowered + body.amount > owed {
        return Err(StatusCode::CONFLICT);
    }

    let already_applied: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount), 0) FROM payable_settlements WHERE payment_id = $1",
    )
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

    // **The party's own sent balance, and it is the ceiling the per-payment remainder cannot
    // see.** A payment can be wholly unapplied while the shop has already had that money come
    // back through a supplier return, or while a second payable was settled from a payment this
    // one outlived. Measured on the customer side as `R1-001`/`R3-001` of
    // `review-1becd57b99cb3a1f`, where the same blind spot resolved one collection twice.
    //
    // The helper is the same one the refund path will need, which is why it exists once: three
    // call sites with three hand-written versions is how two of them end up missing a term.
    if body.amount > sent_available {
        return Err(StatusCode::CONFLICT);
    }

    let created: (Uuid, DateTime<Utc>) = sqlx::query_as(
        "INSERT INTO payable_settlements (payable_id, payment_id, amount, created_by)
         VALUES ($1, $2, $3, 'counter') RETURNING id, created_at",
    )
    .bind(body.payable_id)
    .bind(body.payment_id)
    .bind(body.amount)
    .fetch_one(&mut *tx)
    .await
    // Two rows from one payment to one payable is `payable_settlements_one_row_per_pair`: one
    // row written twice is not a partial payment, and the caller can act on a 409.
    .map_err(conflict_or_500)?;

    // **No ledger entry, and this is the line that must never be added.** The money moved when
    // the payment was recorded; the settlement only says which payable it answered.

    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "tx commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::CREATED,
        Json(PayableSettlement {
            id: created.0,
            payable_id: body.payable_id,
            payment_id: body.payment_id,
            amount: body.amount,
            created_at: created.1,
            payable_outstanding: owed - already_settled - already_lowered - body.amount,
            payment_unapplied: paid - already_applied - body.amount,
        }),
    ))
}
