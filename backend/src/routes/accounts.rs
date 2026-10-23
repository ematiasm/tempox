//! `GET`/`POST /api/accounts` and `GET`/`POST /api/payment-methods`, plus the
//! `PATCH` each one needs.
//!
//! This is the write path v3 could not work without. Without it `cash` would be
//! the only method in the system, every sale would name it, and the central claim
//! of the slice — a sale on a bank transfer — would be untestable. The plan had a
//! hole here and it was found by asking why the credit guard could not simply be
//! written; the guard needs the `credit` account to exist, and creating accounts
//! needs endpoints.
//!
//! **Balances come from `money_movements`, never from a column.** `0014` creates the
//! view and owns the sign convention; this module only sums it per account. An
//! account's balance is therefore the same kind of number stock already is — a
//! derived sum over documents — and cannot drift from them.
//!
//! **What is editable, and why.** The rule follows from identity, not convention:
//!
//! | field | editable | reason |
//! |---|---|---|
//! | `name` (both) | yes | a typo must be fixable; documents store the id, so a rename moves no balance |
//! | `is_active` (method) | yes | retires it from the picker without erasing the explanation of a past balance |
//! | `is_default` (account) | yes | moving the till is a real operation; the database enforces exactly one at commit |
//! | `account_id` (method) | only while no document names it | moving a referenced method rewrites yesterday's balance today |
//! | either row, deleted | never | deletion erases the explanation of a balance; `is_active` is the answer |
//!
//! Every refusal here is enforced again in the database (`0012`'s triggers and
//! deferred constraint, `0016`'s seed-identity guards). The API checks are for a
//! clear status code, not for safety: a `psql` session bypasses this file entirely,
//! which is exactly why the invariants live in the schema.
//!
//! **The seeded `credit` rows are refused, never rearranged.** `CREDIT_ACCOUNT_ID`
//! and `CREDIT_PAYMENT_METHOD_ID` are resolved by the sale and purchase guards with
//! no lookup, so the two rows they name must stay what the constants say: the method
//! never changes account, nothing is attached to the account, and the account is
//! never the till. Those three are `odd/tasks/v3-seed-identities.md` S1-S3.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::db::conflict_or_500;
use crate::models::account::{Account, CREDIT_ACCOUNT_ID, CREDIT_PAYMENT_METHOD_ID, PaymentMethod};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/accounts", get(list_accounts).post(create_account))
        .route("/accounts/{id}", patch(update_account))
        .route(
            "/payment-methods",
            get(list_payment_methods).post(create_payment_method),
        )
        .route("/payment-methods/{id}", patch(update_payment_method))
}

/// An account with the methods that point at it, and what it currently holds.
///
/// The methods are nested rather than joined per row because an account with no
/// methods is a legitimate state (it can exist before anyone attaches a method to
/// it), and an empty array says that without an `Option` for the client to branch
/// on.
///
/// `balance` is **derived**, never stored: the sum of `money_movements` for this
/// account, over all time. `0014` owns that definition, so the sign convention lives
/// in one place instead of four. It is serialized through the money helper so it
/// leaves as a two-decimal string like every other amount in the API.
#[derive(Debug, Serialize)]
pub struct AccountWithMethods {
    #[serde(flatten)]
    pub account: Account,
    pub methods: Vec<PaymentMethod>,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub balance: rust_decimal::Decimal,
}

// ── Accounts ────────────────────────────────────────────────────────────────

async fn list_accounts(
    State(state): State<AppState>,
) -> Result<Json<Vec<AccountWithMethods>>, StatusCode> {
    // Ordered with the default first, then by name: the till is what the sales
    // footer reads, and putting it at the top means the common case does not
    // depend on the dictionary ordering of the shop's language.
    let accounts = sqlx::query_as!(
        Account,
        "SELECT id, name, is_default, created_at FROM accounts
          ORDER BY is_default DESC, name"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "list_accounts failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // One statement for every balance, grouped by account, rather than one query per
    // account. The view is small at this scale (a neighbourhood shop reaches ~215k
    // rows in a decade) and `COALESCE` gives an account with no movements a clean zero
    // instead of a null the client has to branch on.
    let balances = sqlx::query!(
        r#"SELECT a.id AS "account_id!", COALESCE(SUM(m.amount), 0)::numeric AS "balance!"
             FROM accounts a
             LEFT JOIN payment_methods pm ON pm.account_id = a.id
             LEFT JOIN money_movements m  ON m.payment_method_id = pm.id
            GROUP BY a.id"#
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "account balances failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let balances: std::collections::HashMap<Uuid, rust_decimal::Decimal> = balances
        .into_iter()
        .map(|row| (row.account_id, row.balance))
        .collect();

    let mut with_methods = Vec::with_capacity(accounts.len());
    for account in accounts {
        // One extra query per account, bounded by the number of accounts. The same
        // simplification `list_purchases` documents for its lines: an aggregated
        // single statement is the optimization, later.
        let methods = sqlx::query_as!(
            PaymentMethod,
            "SELECT id, account_id, name, is_active, created_at
               FROM payment_methods WHERE account_id = $1 ORDER BY name",
            account.id
        )
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(?e, "account methods read failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        let balance = balances
            .get(&account.id)
            .copied()
            .unwrap_or(rust_decimal::Decimal::ZERO);
        with_methods.push(AccountWithMethods {
            account,
            methods,
            balance,
        });
    }

    Ok(Json(with_methods))
}

#[derive(Debug, Deserialize)]
pub struct NewAccount {
    pub name: String,
    /// Whether this account becomes the till. Omitted means "not the default":
    /// creating an account is not the operation that moves the shop's till unless
    /// the caller says so.
    #[serde(default)]
    pub is_default: bool,
}

async fn create_account(
    State(state): State<AppState>,
    Json(body): Json<NewAccount>,
) -> Result<(StatusCode, Json<Account>), StatusCode> {
    // Trimmed before the emptiness check: a name of spaces is empty, and storing it
    // would put an unreadable row in every picker. `0012` guards the same value, so
    // this is for the status code rather than for safety.
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // **A new account cannot be the default.** It has no payment methods yet, and
    // `guard_new_default_account_has_active_method` refuses a default account with
    // nothing active — so the option this route used to accept could never succeed,
    // and it surfaced as a 500 from the trigger. Found by native review (`R3-001`)
    // after the fact, with the status code measured before the fix.
    //
    // Refused here with the reason a caller can act on: create the account, attach a
    // method, then promote it through `PATCH`. That sequence is the supported one and
    // is covered by `v3_t0_moving_the_till_is_a_single_operation…`.
    if body.is_default {
        return Err(StatusCode::BAD_REQUEST);
    }

    // One transaction, because promoting the new account to default is a statement
    // about two rows: the new one becomes true and the old one must become false
    // **in the same transaction**, or the deferred "exactly one default" constraint
    // fails at commit. Doing it in two autocommitted statements would be the
    // deadlock `T1` had to fix — except that this path can always satisfy the
    // constraint, so it takes the lock and does it properly.
    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!(?e, "tx begin failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let account = sqlx::query_as!(
        Account,
        "INSERT INTO accounts (name, is_default) VALUES ($1, $2)
         RETURNING id, name, is_default, created_at",
        name,
        body.is_default
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(conflict_or_500)?;

    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "tx commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(account)))
}

/// The editable fields of an account. Every field is optional, so a `PATCH` can
/// rename without touching the default and vice versa.
///
/// `Option<bool>` rather than `bool` for `is_default`: an absent field means "leave
/// it as it is", and a plain `bool` would read absent as `false` and silently
/// demote the till on a rename.
#[derive(Debug, Deserialize)]
pub struct PatchAccount {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub is_default: Option<bool>,
}

async fn update_account(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<PatchAccount>,
) -> Result<Json<Account>, StatusCode> {
    // An empty body is not a rename: it would be a 200 that changed nothing while
    // looking like it did.
    if body.name.is_none() && body.is_default.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let name = match body.name {
        Some(raw) => {
            let trimmed = raw.trim().to_string();
            if trimmed.is_empty() {
                return Err(StatusCode::BAD_REQUEST);
            }
            Some(trimmed)
        }
        None => None,
    };

    // S3: the debt account is never the shop's till. Checked before the transaction
    // because it needs no row state, and because the schema's own refusal arrives at
    // commit time where it can only be reported as a server fault.
    //
    // Decided before the "clear the old default" statement below on purpose:
    // refusing after it would abandon a half-done move for the wrong reason.
    if body.is_default == Some(true) && id == CREDIT_ACCOUNT_ID {
        return Err(StatusCode::CONFLICT);
    }

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!(?e, "tx begin failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Lock the row first: promoting this account is a two-row statement (this one
    // true, the current default false), and a concurrent promote would otherwise
    // interleave between the two updates.
    let exists: Option<Uuid> =
        sqlx::query_scalar!("SELECT id FROM accounts WHERE id = $1 FOR UPDATE", id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "account lock failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    if exists.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    // Promoting to default clears the old one in the same transaction. Demoting is
    // refused by the deferred constraint at commit unless another row becomes
    // default, which is what "exactly one till" means — the API does not special
    // case it, it lets the database say no.
    if body.is_default == Some(true) {
        sqlx::query("UPDATE accounts SET is_default = false WHERE is_default AND id <> $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "clear default failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    }

    let account = sqlx::query_as!(
        Account,
        "UPDATE accounts SET
            name = COALESCE($2, name),
            is_default = COALESCE($3, is_default)
         WHERE id = $1
         RETURNING id, name, is_default, created_at",
        id,
        name,
        body.is_default
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(conflict_or_500)?;

    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "tx commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(account))
}

// ── Payment methods ─────────────────────────────────────────────────────────

async fn list_payment_methods(
    State(state): State<AppState>,
) -> Result<Json<Vec<PaymentMethod>>, StatusCode> {
    // Inactive methods are listed, not hidden. A picker filters on `is_active`
    // itself, and a ledger rendering a historical document needs the name of a
    // method that was retired after the sale — hiding it here would make an old row
    // unrenderable.
    let methods = sqlx::query_as!(
        PaymentMethod,
        "SELECT id, account_id, name, is_active, created_at
           FROM payment_methods ORDER BY name"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "list_payment_methods failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(methods))
}

#[derive(Debug, Deserialize)]
pub struct NewPaymentMethod {
    pub name: String,
    pub account_id: Uuid,
}

async fn create_payment_method(
    State(state): State<AppState>,
    Json(body): Json<NewPaymentMethod>,
) -> Result<(StatusCode, Json<PaymentMethod>), StatusCode> {
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // S2: nothing may be attached to the debt account. Decided before the existence
    // lookup, because the seeded account does exist and the refusal is about the
    // rule rather than about whether the row is there.
    if body.account_id == CREDIT_ACCOUNT_ID {
        return Err(StatusCode::CONFLICT);
    }

    // An unknown account is a 404 rather than a 500 from the foreign key, the same
    // distinction `create_purchase` makes for an unknown supplier.
    let account_exists: i64 = sqlx::query_scalar!(
        "SELECT count(*) AS \"count!\" FROM accounts WHERE id = $1",
        body.account_id
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "account lookup failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if account_exists == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    // A new method is active by default: creating one is how the shop starts using
    // it. Deactivating is a separate, deliberate PATCH.
    let method = sqlx::query_as!(
        PaymentMethod,
        "INSERT INTO payment_methods (account_id, name) VALUES ($1, $2)
         RETURNING id, account_id, name, is_active, created_at",
        body.account_id,
        name
    )
    .fetch_one(&state.db)
    .await
    .map_err(conflict_or_500)?;
    Ok((StatusCode::CREATED, Json(method)))
}

#[derive(Debug, Deserialize)]
pub struct PatchPaymentMethod {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub is_active: Option<bool>,
    /// Moving a method to another account. Allowed only while no document names it,
    /// which the database enforces (S2); here it is passed through so the caller gets
    /// a 500-free error rather than a constraint violation.
    #[serde(default)]
    pub account_id: Option<Uuid>,
}

async fn update_payment_method(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<PatchPaymentMethod>,
) -> Result<Json<PaymentMethod>, StatusCode> {
    if body.name.is_none() && body.is_active.is_none() && body.account_id.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let name = match body.name {
        Some(raw) => {
            let trimmed = raw.trim().to_string();
            if trimmed.is_empty() {
                return Err(StatusCode::BAD_REQUEST);
            }
            Some(trimmed)
        }
        None => None,
    };

    if let Some(account_id) = body.account_id {
        // S1 and S2, each answering a different question: the seeded method's account
        // is not editable at all, and no method may be moved onto the debt account.
        // Without these the refusals would arrive from `0016`'s triggers at commit
        // time, where they can only surface as a 500.
        if id == CREDIT_PAYMENT_METHOD_ID {
            return Err(StatusCode::CONFLICT);
        }
        if account_id == CREDIT_ACCOUNT_ID {
            return Err(StatusCode::CONFLICT);
        }

        let account_exists: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM accounts WHERE id = $1",
            account_id
        )
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(?e, "account lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        if account_exists == 0 {
            return Err(StatusCode::NOT_FOUND);
        }

        // Is any document naming this method? If so the move is history-rewriting
        // and refused here with a 409, so the caller gets a reason instead of a
        // trigger exception surfacing as a 500.
        //
        // The database trigger is still the guard: this check can race with a
        // document being written concurrently, and the trigger is what closes that.
        let referenced: i64 = sqlx::query_scalar!(
            r#"SELECT (
                 (SELECT count(*) FROM sales            WHERE payment_method_id = $1)
               + (SELECT count(*) FROM credit_notes     WHERE payment_method_id = $1)
               + (SELECT count(*) FROM purchases        WHERE payment_method_id = $1)
               + (SELECT count(*) FROM supplier_returns WHERE payment_method_id = $1)
               )::bigint AS "count!""#,
            id
        )
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(?e, "method reference count failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        if referenced > 0 {
            return Err(StatusCode::CONFLICT);
        }
    }

    let method = sqlx::query_as!(
        PaymentMethod,
        "UPDATE payment_methods SET
            name = COALESCE($2, name),
            is_active = COALESCE($3, is_active),
            account_id = COALESCE($4, account_id)
         WHERE id = $1
         RETURNING id, account_id, name, is_active, created_at",
        id,
        name,
        body.is_active,
        body.account_id
    )
    .fetch_optional(&state.db)
    .await
    .map_err(conflict_or_500)?
    // No row means the id does not exist. `map_err` above turns a duplicate name into
    // 409, and a missing row into 404 here rather than an empty 200.
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(method))
}
