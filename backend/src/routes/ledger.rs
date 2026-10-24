//! Money that did not come from a sale: the opening balance, a transfer between the
//! shop's own accounts, and a count adjustment.
//!
//! `odd/tasks/v4-settlement.md` S12, migration `0018`. Before this module there were
//! exactly two ways for money to move — a document that sold something and one that
//! reversed it — so three ordinary events had no representation:
//!
//! | event | what was missing | what the alternative was |
//! |---|---|---|
//! | the till's first figure | no `reason = 'initial'` for money, unlike stock | the book started at the first sale |
//! | till → bank | no transfer document | a fake sale, polluting the sales ledger |
//! | "I counted 2,850, the book says 3,000" | no adjustment document | a rewritten balance, which cannot be audited |
//!
//! **Each writes its entries through the same book as everything else.** The account is
//! derived from the method by `0017`'s trigger, so a document and its entries cannot
//! disagree about where the money is — that derivation is what made the old `INNER JOIN`
//! impossible to lose a row through, and it is why `payment_method_id` stays `NOT NULL`
//! in the book even for a document that names an account directly.
//!
//! **A transfer is two effects with one net of zero.** It is the only document in the
//! system that writes two entries that cancel: money leaves one account and arrives at
//! another. If it wrote one, money would appear or disappear.

use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::db::conflict_or_500;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/opening-balances", post(create_opening_balance))
        .route("/transfers", post(create_transfer))
        .route("/count-adjustments", post(create_count_adjustment))
}

// ── Opening balance ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NewOpeningBalance {
    pub account_id: Uuid,
    /// The figure that was already in the account. A decimal string, not a JSON number:
    /// a float would lose the cent.
    pub amount: Decimal,
    /// When the account started. Absent means now, which is what an operator entering
    /// the figure today means.
    #[serde(default)]
    pub opened_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct OpeningBalance {
    pub id: Uuid,
    pub account_id: Uuid,
    pub amount: Decimal,
    pub opened_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

async fn create_opening_balance(
    State(state): State<AppState>,
    Json(body): Json<NewOpeningBalance>,
) -> Result<(StatusCode, Json<OpeningBalance>), StatusCode> {
    if body.amount <= Decimal::ZERO {
        return Err(StatusCode::BAD_REQUEST);
    }

    // The account has to exist, and it has to have a method, because the book derives the
    // account from a method and an account with no method cannot be written to. That is
    // not an obstacle: it is the same shape as "the default account needs an active
    // method" (`0012`).
    let method: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM payment_methods WHERE account_id = $1 ORDER BY is_active DESC, created_at LIMIT 1",
    )
    .bind(body.account_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "opening balance method lookup failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let method = method.ok_or(StatusCode::NOT_FOUND)?;

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!(?e, "tx begin failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let opened = sqlx::query_as::<_, (Uuid, Uuid, Decimal, DateTime<Utc>, DateTime<Utc>)>(
        "INSERT INTO opening_balances (account_id, amount, opened_at, created_by)
         VALUES ($1, $2, COALESCE($3, NOW()), 'counter')
         RETURNING id, account_id, amount, opened_at, created_at",
    )
    .bind(body.account_id)
    .bind(body.amount)
    .bind(body.opened_at)
    .fetch_one(&mut *tx)
    .await
    // A second beginning for the same account is a 409, not a 500: the unique index is
    // the rule, and the caller can act on it.
    .map_err(conflict_or_500)?;

    sqlx::query(
        "INSERT INTO money_entries
            (account_id, payment_method_id, document_kind, document_id, amount, occurred_at, created_by)
         SELECT pm.account_id, $1, 'opening_balance', $2, $3, $4, 'counter'
           FROM payment_methods pm WHERE pm.id = $1",
    )
    .bind(method)
    .bind(opened.0)
    .bind(opened.2)
    .bind(opened.3)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "opening balance entry failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "tx commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::CREATED,
        Json(OpeningBalance {
            id: opened.0,
            account_id: opened.1,
            amount: opened.2,
            opened_at: opened.3,
            created_at: opened.4,
        }),
    ))
}

// ── Transfer ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NewTransfer {
    pub from_account_id: Uuid,
    pub to_account_id: Uuid,
    pub from_payment_method_id: Uuid,
    pub to_payment_method_id: Uuid,
    pub amount: Decimal,
    #[serde(default)]
    pub occurred_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct Transfer {
    pub id: Uuid,
    pub from_account_id: Uuid,
    pub to_account_id: Uuid,
    pub amount: Decimal,
    pub occurred_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

async fn create_transfer(
    State(state): State<AppState>,
    Json(body): Json<NewTransfer>,
) -> Result<(StatusCode, Json<Transfer>), StatusCode> {
    if body.amount <= Decimal::ZERO {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!(?e, "tx begin failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // The database refuses a same-account transfer with a CHECK, and refuses a method that
    // does not resolve to the account it names with a trigger. Both would surface as a 500
    // at commit; asking first is what gives the caller a status it can act on.
    let moved = sqlx::query_as::<_, (Uuid, Uuid, Uuid, Decimal, DateTime<Utc>, DateTime<Utc>)>(
        "INSERT INTO transfers
            (from_account_id, to_account_id, from_payment_method_id, to_payment_method_id,
             amount, occurred_at, created_by)
         VALUES ($1, $2, $3, $4, $5, COALESCE($6, NOW()), 'counter')
         RETURNING id, from_account_id, to_account_id, amount, occurred_at, created_at",
    )
    .bind(body.from_account_id)
    .bind(body.to_account_id)
    .bind(body.from_payment_method_id)
    .bind(body.to_payment_method_id)
    .bind(body.amount)
    .bind(body.occurred_at)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        // A CHECK violation on `transfers_between_two_accounts` is the caller's mistake,
        // not a server fault.
        if let sqlx::Error::Database(db) = &e
            && db.code().as_deref() == Some("23514")
        {
            return StatusCode::BAD_REQUEST;
        }
        conflict_or_500(e)
    })?;

    // Two entries, one net of zero: the only document in the system that cancels itself.
    for (method, signed) in [
        (body.from_payment_method_id, -body.amount),
        (body.to_payment_method_id, body.amount),
    ] {
        sqlx::query(
            "INSERT INTO money_entries
                (account_id, payment_method_id, document_kind, document_id, amount, occurred_at, created_by)
             SELECT pm.account_id, $1, 'transfer', $2, $3, $4, 'counter'
               FROM payment_methods pm WHERE pm.id = $1",
        )
        .bind(method)
        .bind(moved.0)
        .bind(signed)
        .bind(moved.4)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(?e, "transfer entry failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "tx commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::CREATED,
        Json(Transfer {
            id: moved.0,
            from_account_id: moved.1,
            to_account_id: moved.2,
            amount: moved.3,
            occurred_at: moved.4,
            created_at: moved.5,
        }),
    ))
}

// ── Count adjustment ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NewCountAdjustment {
    pub account_id: Uuid,
    pub payment_method_id: Uuid,
    /// Signed: positive is money the count found that the book did not have, negative is
    /// money the book claims and the drawer does not have.
    pub difference: Decimal,
    #[serde(default)]
    pub counted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct CountAdjustment {
    pub id: Uuid,
    pub account_id: Uuid,
    pub difference: Decimal,
    pub counted_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

async fn create_count_adjustment(
    State(state): State<AppState>,
    Json(body): Json<NewCountAdjustment>,
) -> Result<(StatusCode, Json<CountAdjustment>), StatusCode> {
    // Counting nothing different is not a document. The CHECK says the same; asking first
    // gives a status instead of a constraint violation.
    if body.difference == Decimal::ZERO {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!(?e, "tx begin failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let adjusted = sqlx::query_as::<_, (Uuid, Uuid, Decimal, DateTime<Utc>, DateTime<Utc>)>(
        "INSERT INTO count_adjustments
                (account_id, payment_method_id, difference, counted_at, created_by)
             VALUES ($1, $2, $3, COALESCE($4, NOW()), 'counter')
             RETURNING id, account_id, difference, counted_at, created_at",
    )
    .bind(body.account_id)
    .bind(body.payment_method_id)
    .bind(body.difference)
    .bind(body.counted_at)
    .fetch_one(&mut *tx)
    .await
    .map_err(conflict_or_500)?;

    sqlx::query(
        "INSERT INTO money_entries
            (account_id, payment_method_id, document_kind, document_id, amount, occurred_at, created_by)
         SELECT pm.account_id, $1, 'count_adjustment', $2, $3, $4, 'counter'
           FROM payment_methods pm WHERE pm.id = $1",
    )
    .bind(body.payment_method_id)
    .bind(adjusted.0)
    .bind(adjusted.2)
    .bind(adjusted.3)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "count adjustment entry failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "tx commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::CREATED,
        Json(CountAdjustment {
            id: adjusted.0,
            account_id: adjusted.1,
            difference: adjusted.2,
            counted_at: adjusted.3,
            created_at: adjusted.4,
        }),
    ))
}
