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
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch},
};
use uuid::Uuid;

use crate::AppState;
use crate::db::conflict_or_500;
use crate::models::destination::Destination;
use crate::models::expense::Expense;
use crate::models::expense_category::ExpenseCategory;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/expense-categories",
            get(list_expense_categories).post(create_expense_category),
        )
        .route("/expense-categories/{id}", patch(update_expense_category))
        .merge(routes_with_expenses())
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

// ── The expense document ───────────────────────────────────────────────────

pub fn routes_with_expenses() -> Router<AppState> {
    Router::new()
        .route(
            "/expenses",
            axum::routing::get(list_expenses).post(create_expense),
        )
        .route("/expenses/summary", axum::routing::get(expenses_summary))
        .route(
            "/expenses/{id}/advances",
            axum::routing::post(create_expense_advance),
        )
}

#[derive(Debug, serde::Deserialize)]
pub struct NewExpense {
    pub category_id: Uuid,
    pub payee: String,
    /// A decimal string, not a JSON number: a float would lose the cent.
    pub amount: rust_decimal::Decimal,
    #[serde(default)]
    pub occurred_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Read raw, so the field's own contract decides the status code. Kept as
    /// `Option<serde_json::Value>` because `Option<Uuid>` cannot tell an absent key from
    /// an explicit `null`, and the difference is the whole point of the grammar.
    #[serde(default, deserialize_with = "crate::models::destination::keep_present")]
    pub payment_method_id: Option<serde_json::Value>,
}

async fn create_expense(
    State(state): State<AppState>,
    Json(body): Json<NewExpense>,
) -> Result<(StatusCode, Json<Expense>), StatusCode> {
    // Trimmed before the emptiness check, for the reason the category route gives: a
    // payee of spaces is empty, and storing it puts an unreadable row in every ledger.
    let payee = body.payee.trim().to_string();
    if payee.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // **Money before any write, and the whole-cents rule is asked here so the caller gets
    // a 400 instead of a trigger's 500.** The column declares scale 6 with a
    // `amount = round(amount, 2)` CHECK: a value finer than a cent is *refused* by the
    // database rather than silently rounded, and `10.005` is the measured example.
    if body.amount <= rust_decimal::Decimal::ZERO || body.amount != body.amount.round_dp(2) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // **`Destination::from_field` and not a serde deserializer** (`S3`). A custom
    // deserializer's failure reaches the client through axum's `Json` as **422**, while
    // this field's contract promises 400 — measured in `v4-payables.md` L4, and both
    // pre-existing routes were repaired for it. Reading it raw is what keeps the promise.
    let destination = match Destination::from_field(body.payment_method_id)
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        Some(Destination::Method(id)) => Some(id),
        Some(Destination::OnAccount) => None,
        // Omitted keeps every older client's behaviour: the seeded till.
        None => Some(crate::models::account::CASH_PAYMENT_METHOD_ID),
    };

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!(?e, "tx begin failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // The category has to exist, and this reads before the insert so an unknown one is a
    // 404 with nothing written.
    let known: Option<Uuid> = sqlx::query_scalar("SELECT id FROM expense_categories WHERE id = $1")
        .bind(body.category_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(?e, "category lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    known.ok_or(StatusCode::NOT_FOUND)?;

    // **The method is resolved before the first write, and it is a 404 rather than a
    // foreign-key 500.** The first version checked it *after* inserting the expense and
    // got a 500: the insert already referenced the unknown method, so the FK fired before
    // the lookup that would have answered 404. Measured by the destination-grammar test,
    // which asserts the status rather than merely that something failed — a refusal with
    // the wrong code is a caller who cannot act on it.
    if let Some(method) = destination {
        let known_method: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM payment_methods WHERE id = $1")
                .bind(method)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| {
                    tracing::error!(?e, "payment method lookup failed");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        known_method.ok_or(StatusCode::NOT_FOUND)?;
    }

    let expense_id: Uuid = sqlx::query_scalar(
        "INSERT INTO expenses (category_id, payee, amount, occurred_at, payment_method_id, created_by)
         VALUES ($1, $2, $3, COALESCE($4, NOW()), $5, 'counter') RETURNING id",
    )
    .bind(body.category_id)
    .bind(&payee)
    .bind(body.amount)
    .bind(body.occurred_at)
    .bind(destination)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "expense insert failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // **A paid expense records its money as a `payments` row, and the row owns the ledger
    // entry** (`S5`). Writing an `'expense'` entry as well would count the same coin
    // twice — the defect the first draft of the specification carried.
    if let Some(method) = destination {
        let payment_id: Uuid = sqlx::query_scalar(
            "INSERT INTO payments
                (party_kind, party_id, payment_method_id, total, direction, occurred_at, created_by)
             VALUES ('expense', $1, $2, $3, 'out', COALESCE($4, NOW()), 'counter')
             RETURNING id",
        )
        .bind(expense_id)
        .bind(method)
        .bind(body.amount)
        .bind(body.occurred_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(?e, "expense payment insert failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        // Money out is negative — `0014`'s convention, declared there and never
        // re-derived. `payments_claim_their_entry` refuses a payment that writes none.
        sqlx::query(
            "INSERT INTO money_entries
                (account_id, payment_method_id, document_kind, document_id, amount, occurred_at, created_by)
             SELECT pm.account_id, $1, 'payment', $2, -$3, COALESCE($4, NOW()), 'counter'
               FROM payment_methods pm WHERE pm.id = $1",
        )
        .bind(method)
        .bind(payment_id)
        .bind(body.amount)
        .bind(body.occurred_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(?e, "expense payment entry failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

    // The row as it now stands, read back through the same projection the ledger uses, so
    // the response cannot describe a document differently from how it will be listed.
    let created = sqlx::query_as::<_, Expense>(
        "SELECT e.id, e.category_id, c.name AS category_name, e.payee, e.amount,
                e.occurred_at, e.payment_method_id, pm.name AS payment_method_name,
                e.created_at
           FROM expenses e
           JOIN expense_categories c ON c.id = e.category_id
           LEFT JOIN payment_methods pm ON pm.id = e.payment_method_id
          WHERE e.id = $1",
    )
    .bind(expense_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "expense readback failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "tx commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(created)))
}

// ── An advance against an owed expense ─────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct NewAdvance {
    /// A decimal string, not a JSON number: a float would lose the cent.
    pub amount: rust_decimal::Decimal,
    /// Same grammar as the document's own destination, read raw so the field's contract
    /// decides the status code. `"on_account"` is refused here — an advance *is* money
    /// moving, so there is no third state to name.
    #[serde(default, deserialize_with = "crate::models::destination::keep_present")]
    pub payment_method_id: Option<serde_json::Value>,
}

#[derive(Debug, serde::Serialize)]
pub struct Advance {
    pub id: Uuid,
    pub expense_id: Uuid,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub amount: rust_decimal::Decimal,
    pub payment_method_id: Uuid,
    pub direction: &'static str,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    /// What is still owed after this advance, derived rather than stored.
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub outstanding: rust_decimal::Decimal,
}

/// One advance: a `payments` row naming the expense, and its ledger entry.
///
/// `odd/tasks/v4-expenses.md` S7, and `D3`'s whole reason for existing — an owed salary is
/// paid in pieces over the month. **No new table**: the advance is a payment, so the
/// outstanding figure is `expenses.amount − SUM(payments)`, a subtraction of stored rows.
///
/// **The ceiling is judged twice, on purpose.** Here it becomes a `409` the caller can act
/// on; below, `expense_payment_agrees_with_its_document` holds it at COMMIT, because a
/// ceiling enforced only above the database is a suggestion to everyone who reaches below
/// it (`v4-payables.md` R3-005).
async fn create_expense_advance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<NewAdvance>,
) -> Result<(StatusCode, Json<Advance>), StatusCode> {
    // Money before any write: a value that is not positive whole cents is a `400`, asked
    // here so the database's CHECK becomes a status rather than a 500.
    if body.amount <= rust_decimal::Decimal::ZERO || body.amount != body.amount.round_dp(2) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // `on_account` is not a destination for an advance: money is moving, so the caller has
    // to say through which rail. Omitted keeps every client's default — the till.
    let method = match Destination::from_field(body.payment_method_id)
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        Some(Destination::Method(id)) => id,
        Some(Destination::OnAccount) => return Err(StatusCode::BAD_REQUEST),
        None => crate::models::account::CASH_PAYMENT_METHOD_ID,
    };

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!(?e, "tx begin failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // **The expense row is locked first, and it is the only lock this route takes.** Two
    // concurrent advances would otherwise both read the same committed sum and both find
    // room; the `FOR UPDATE` serialises them. There is no second row to order against, so
    // the lock-order defect `aa95665` cost is not reachable here.
    let expense: Option<(rust_decimal::Decimal, Option<Uuid>)> =
        sqlx::query_as("SELECT amount, payment_method_id FROM expenses WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "expense lookup failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    let (total, document_method) = expense.ok_or(StatusCode::NOT_FOUND)?;

    // The rail is resolved before the payment, for the reason `create_expense` records: an
    // unknown method must be a 404 rather than a foreign-key 500 arriving later.
    let known_method: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM payment_methods WHERE id = $1")
            .bind(method)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "payment method lookup failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    known_method.ok_or(StatusCode::NOT_FOUND)?;

    // **A paid expense owes nothing**, and that is a `409` rather than a `400`: the request
    // is well formed and the state is what refuses it.
    if document_method.is_some() {
        return Err(StatusCode::CONFLICT);
    }

    let paid: rust_decimal::Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(total), 0) FROM payments
          WHERE party_kind = 'expense' AND party_id = $1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "advance sum failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if paid + body.amount > total {
        return Err(StatusCode::CONFLICT);
    }

    let payment: (Uuid, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "INSERT INTO payments
            (party_kind, party_id, payment_method_id, total, direction, occurred_at, created_by)
         VALUES ('expense', $1, $2, $3, 'out', NOW(), 'counter')
         RETURNING id, occurred_at",
    )
    .bind(id)
    .bind(method)
    .bind(body.amount)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "advance payment insert failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Money out is negative — `0014`'s convention, never re-derived.
    sqlx::query(
        "INSERT INTO money_entries
            (account_id, payment_method_id, document_kind, document_id, amount, occurred_at, created_by)
         SELECT pm.account_id, $1, 'payment', $2, -$3, $4, 'counter'
           FROM payment_methods pm WHERE pm.id = $1",
    )
    .bind(method)
    .bind(payment.0)
    .bind(body.amount)
    .bind(payment.1)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "advance entry failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "tx commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::CREATED,
        Json(Advance {
            id: payment.0,
            expense_id: id,
            amount: body.amount,
            payment_method_id: method,
            direction: "out",
            occurred_at: payment.1,
            outstanding: total - paid - body.amount,
        }),
    ))
}

// ── The ledger ─────────────────────────────────────────────────────────────

const MAX_LIST_LIMIT: i64 = 200;

#[derive(Debug, serde::Deserialize)]
pub struct ExpensesQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
    /// Inclusive lower bound on `occurred_at`. Absent means no lower bound.
    #[serde(default)]
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    /// Exclusive upper bound, so adjacent days partition the ledger instead of
    /// double-counting a document recorded at midnight — the contract every other ledger
    /// in this system has.
    #[serde(default)]
    pub to: Option<chrono::DateTime<chrono::Utc>>,
}

/// A row of the expense ledger: the document, its names by join, and the two figures that
/// are **derived** rather than stored.
///
/// `paid` and `outstanding` are computed by the query — `SUM(payments)` and the subtraction
/// — because a stored copy is a number that can disagree with the rows it summarises, which
/// is why `sales` lost its lifecycle column in `0015`.
#[derive(Debug, serde::Serialize)]
pub struct ExpenseRow {
    pub id: Uuid,
    pub category_id: Uuid,
    pub category_name: String,
    pub payee: String,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub amount: rust_decimal::Decimal,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub payment_method_id: Option<Uuid>,
    pub payment_method_name: Option<String>,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub paid: rust_decimal::Decimal,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub outstanding: rust_decimal::Decimal,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

async fn list_expenses(
    State(state): State<AppState>,
    Query(params): Query<ExpensesQuery>,
) -> Result<Json<Vec<ExpenseRow>>, StatusCode> {
    let limit = params.limit.unwrap_or(50).clamp(1, MAX_LIST_LIMIT);
    // A negative offset is not a page request: OFFSET refuses it in Postgres, and turning
    // that into a 500 would blame the server for a bad query. Every other ledger clamps the
    // same way, because a filter that behaves differently on two screens reads as a bug.
    let offset = params.offset.unwrap_or(0).max(0);

    // **One statement for the rows and their figures**, rather than a query per row: `paid`
    // is a sum over a different table, and asking it once per document is the N+1 this
    // ledger can avoid cheaply because expenses are few and payments fewer.
    //
    // `LEFT JOIN` for the method, for the reason the create readback gives: an `INNER JOIN`
    // would drop an owed expense from the ledger with no error — the trap `purchases.rs`
    // documents. The payments join is aggregated in a subquery so a document with two
    // advances yields one row, not two.
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            Uuid,
            String,
            String,
            rust_decimal::Decimal,
            chrono::DateTime<chrono::Utc>,
            Option<Uuid>,
            Option<String>,
            rust_decimal::Decimal,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        "SELECT e.id, e.category_id, c.name, e.payee, e.amount, e.occurred_at,
                e.payment_method_id, pm.name,
                COALESCE(p.paid, 0),
                e.created_at
           FROM expenses e
           JOIN expense_categories c ON c.id = e.category_id
           LEFT JOIN payment_methods pm ON pm.id = e.payment_method_id
           LEFT JOIN (
                SELECT party_id, SUM(total) AS paid
                  FROM payments
                 WHERE party_kind = 'expense'
                 GROUP BY party_id
           ) p ON p.party_id = e.id
          WHERE ($1::timestamptz IS NULL OR e.occurred_at >= $1)
            AND ($2::timestamptz IS NULL OR e.occurred_at < $2)
          ORDER BY e.occurred_at DESC, e.id
          LIMIT $3 OFFSET $4",
    )
    .bind(params.from)
    .bind(params.to)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "list_expenses failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let expenses = rows
        .into_iter()
        .map(
            |(
                id,
                category_id,
                category_name,
                payee,
                amount,
                occurred_at,
                payment_method_id,
                payment_method_name,
                paid,
                created_at,
            )| ExpenseRow {
                id,
                category_id,
                category_name,
                payee,
                amount,
                occurred_at,
                payment_method_id,
                payment_method_name,
                paid,
                outstanding: amount - paid,
                created_at,
            },
        )
        .collect();

    Ok(Json(expenses))
}

// ── The summary ────────────────────────────────────────────────────────────

/// What the shop spent in a period, in two terms plus the correction that nets them.
///
/// `net = expenses + purchases − supplier_returns`. The purchases term is **derived, not
/// copied** (`D1`): a purchase keeps meaning "goods arrived and money left, with a
/// supplier", and copying it into `expenses` would be two records of one fact. `PRODUCT.md`
/// forbids a report justifying a new column at write time, so the purchase gains no
/// category and enters here as one group.
///
/// **Payments appear in no term.** An advance is the settlement of an amount already
/// counted at full in `expenses`; adding it would count the same coin twice, which is the
/// sentence `routes/payments.rs` exists to keep for settlements.
#[derive(Debug, serde::Serialize)]
pub struct SpendingSummary {
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub expenses: rust_decimal::Decimal,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub purchases: rust_decimal::Decimal,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub supplier_returns: rust_decimal::Decimal,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub net: rust_decimal::Decimal,
    pub documents: i64,
}

#[derive(Debug, serde::Deserialize)]
pub struct SummaryQuery {
    #[serde(default)]
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub to: Option<chrono::DateTime<chrono::Utc>>,
}

async fn expenses_summary(
    State(state): State<AppState>,
    Query(params): Query<SummaryQuery>,
) -> Result<Json<SpendingSummary>, StatusCode> {
    // **Each term is scoped by its own document's clock, and that is the decision the
    // comments in the test call out.** An expense carries `occurred_at` — the day it
    // belongs to, which is the whole reason an owed salary can be entered in October and
    // paid in November. A purchase carries only `created_at`: it is a fact about the day
    // the goods arrived. Using one clock for both would silently misdate half the answer.
    let row = sqlx::query_as::<
        _,
        (
            rust_decimal::Decimal,
            rust_decimal::Decimal,
            rust_decimal::Decimal,
            i64,
        ),
    >(
        "WITH recorded AS (
             SELECT COALESCE(SUM(amount), 0) AS total, count(*) AS docs
               FROM expenses
              WHERE ($1::timestamptz IS NULL OR occurred_at >= $1)
                AND ($2::timestamptz IS NULL OR occurred_at < $2)
         ),
         bought AS (
             SELECT COALESCE(SUM(total), 0) AS total
               FROM purchases
              WHERE ($1::timestamptz IS NULL OR created_at >= $1)
                AND ($2::timestamptz IS NULL OR created_at < $2)
         ),
         returned AS (
             SELECT COALESCE(SUM(total), 0) AS total
               FROM supplier_returns
              WHERE ($1::timestamptz IS NULL OR created_at >= $1)
                AND ($2::timestamptz IS NULL OR created_at < $2)
         )
         SELECT recorded.total, bought.total, returned.total, recorded.docs
           FROM recorded, bought, returned",
    )
    .bind(params.from)
    .bind(params.to)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "expenses_summary failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let (expenses, purchases, supplier_returns, documents) = row;
    Ok(Json(SpendingSummary {
        expenses,
        purchases,
        supplier_returns,
        net: expenses + purchases - supplier_returns,
        documents,
    }))
}
