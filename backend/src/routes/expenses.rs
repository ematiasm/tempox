//! `GET`/`POST /api/expense-categories` and the `PATCH` that retires one.
//!
//! `odd/tasks/v4-expenses.md` S1 and S10, and the migration section that closes
//! `0001_schema.sql`. An expense is money leaving that touches no stock, and this is
//! how the shop says what it spent it on.
//!
//! **A category is a row because the alternative has already been paid for three
//! times.** `stock_movements.reason` was re-cut in `0007`, `0009` and `0010` until
//! `0011` replaced six enumerating constraints with one shape-derived trigger;
//! `money_entries.document_kind` was re-cut in `0018`, `0020` and `0022`, and `0020`
//! copied the members by hand and **silently dropped `annulment`**, turning every
//! annulment into a 500; `obligations.source_document_kind` is closed and `0023` paid
//! for an entire duplicated table rather than re-cut it. A classification the shop
//! invents on a Tuesday is data, and data belongs in a row.
//!
//! **What is editable, and why**, in the same shape `accounts.rs` states it:
//!
//! | field | editable | reason |
//! |---|---|---|
//! | `name` | yes | a typo must be fixable; expenses store the id, so a rename moves no money |
//! | `is_active` | yes | retires it from the picker without erasing the explanation of a past expense |
//! | the row, deleted | never | deletion erases that explanation; `is_active` is the answer |
//!
//! Every refusal here is enforced again in the database — the `UNIQUE`, the
//! `name_not_blank` CHECK and the two delete/truncate triggers — because a `psql`
//! session bypasses this file entirely. The checks here exist for the status code.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
};
use uuid::Uuid;

use crate::AppState;
use crate::db::conflict_or_500;
use crate::models::expense_category::ExpenseCategory;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/expense-categories",
            get(list_expense_categories).post(create_expense_category),
        )
        .route("/expense-categories/{id}", patch(update_expense_category))
}

// ── Reading ────────────────────────────────────────────────────────────────

/// Every category, **retired ones included**, active first and then by name.
///
/// Including retired rows is a decision rather than an oversight, and `GET
/// /api/payment-methods` made it first for the same reason: a ledger rendering a
/// historical expense has to render the name of a category that was retired after
/// the expense was recorded. Filtering them out here would turn a past row's
/// category into a blank the client cannot explain.
///
/// The order is `is_active DESC, name`, which is what puts a usable picker in front
/// without the client sorting: retired rows sink to the bottom and the rest read
/// alphabetically in the shop's own vocabulary.
async fn list_expense_categories(
    State(state): State<AppState>,
) -> Result<Json<Vec<ExpenseCategory>>, StatusCode> {
    let categories = sqlx::query_as!(
        ExpenseCategory,
        "SELECT id, name, is_active, created_at
           FROM expense_categories
          ORDER BY is_active DESC, name"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "list_expense_categories failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(categories))
}

// ── Creating ───────────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct NewExpenseCategory {
    pub name: String,
}

async fn create_expense_category(
    State(state): State<AppState>,
    Json(body): Json<NewExpenseCategory>,
) -> Result<(StatusCode, Json<ExpenseCategory>), StatusCode> {
    // Trimmed before the emptiness check, for the reason `create_account` gives: a
    // name of spaces is empty, and storing it would put an unreadable row in every
    // picker. The CHECK in the schema guards the same value, so this is for the
    // status code rather than for safety.
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // A duplicate is a `409`, not a `500`: the unique index is the rule and the
    // caller can act on it — which is the distinction `conflict_or_500` exists for.
    let created = sqlx::query_as!(
        ExpenseCategory,
        "INSERT INTO expense_categories (name) VALUES ($1)
         RETURNING id, name, is_active, created_at",
        name
    )
    .fetch_one(&state.db)
    .await
    .map_err(conflict_or_500)?;

    Ok((StatusCode::CREATED, Json(created)))
}

// ── Retiring, and correcting a typo ────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct UpdateExpenseCategory {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub is_active: Option<bool>,
}

async fn update_expense_category(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateExpenseCategory>,
) -> Result<Json<ExpenseCategory>, StatusCode> {
    // **An empty body is refused, not answered with a `200` that changed nothing.**
    // `v3_t0_patch_with_nothing_to_change_is_refused` pinned this for accounts and the
    // same argument holds here: a request that reports success while doing nothing is
    // a lie the client cannot detect.
    if body.name.is_none() && body.is_active.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // The name is trimmed and validated only when it was sent. `None` means "leave it
    // alone", which is what `COALESCE` below reads; `Some("  ")` is a request to set a
    // name, and that request is refused.
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

    // One statement, so a name change and a retirement cannot half-apply. `COALESCE`
    // keeps the other column untouched, and the `RETURNING` clause means the response
    // is the row as it now stands rather than as the caller imagined it.
    let updated = sqlx::query_as!(
        ExpenseCategory,
        "UPDATE expense_categories
            SET name = COALESCE($2, name),
                is_active = COALESCE($3, is_active)
          WHERE id = $1
         RETURNING id, name, is_active, created_at",
        id,
        name,
        body.is_active
    )
    .fetch_optional(&state.db)
    .await
    // A rename to a name another category already holds is a `409`, exactly as a
    // duplicate creation is: the unique index is the same rule reached from the other
    // side.
    .map_err(conflict_or_500)?
    // An unknown id is a `404`, never a `200` with nothing in it: "there is no such
    // category" and "the category is unchanged" are different statements, and the
    // second one would need a row to be true.
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(updated))
}
