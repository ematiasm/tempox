//! A party's account, read as one document: who they are, what is owed in each
//! direction, and which of their documents are still unpaid.
//!
//! `odd/tasks/v5-customer-account.md` S3-S6. The panel that opens when a row is clicked has
//! three boxes, and this module answers all three in **one** response. Three round trips to
//! paint one panel is three chances to show a half-loaded panel whose boxes contradict each
//! other — a balance from one instant and a document list from another.
//!
//! **The whole payload is read under one snapshot** — `REPEATABLE READ READ ONLY`, one
//! transaction for the four queries. Without it the boxes can describe different instants: a
//! collection landing between the balance and the unpaid list would render "owes 0.00" above a
//! sale with 40 outstanding. That is `R3-001` of review `review-b1f1ea20a0e4419b`, corroborated by
//! the refuter, and the reason the reads take a transaction instead of the pool.
//!
//! **Nothing here is stored and nothing here writes.** Both figures are subtractions over rows
//! that already exist, and the module contains no `INSERT`, no `UPDATE` and no balance column:
//!
//! ```text
//! owed            = SUM(obligations.amount) - SUM(adjustments) - SUM(settlements)
//! in their favour = SUM(payments in) - SUM(settlements of those)
//! ```
//!
//! The two figures are reported **separately and never netted**, which is the owner's
//! decision (`L4` of the feature document) and also what the schema says: they are two
//! subtractions over two different sets of rows. A client who owes 1000 on one sale and holds
//! 500 from an overpayment is two facts, and a single figure of 500 would destroy both.
//!
//! **The mirror is written once, parameterised by the party kind**, rather than twice. The
//! sales and purchase tables in this repository are the cautionary tale: two copies of one
//! idea, 2700 lines, drifting apart.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/customers/{id}/account", get(customer_account))
        .route("/suppliers/{id}/account", get(supplier_account))
}

/// One unpaid document of a party: the sale it came from, when it was recorded, what it was
/// worth and what is still owed on it.
///
/// `outstanding` is derived per row, so the panel can show "40 of 100" without a second
/// request and without the client re-deriving the subtraction.
#[derive(Debug, Serialize)]
pub struct UnpaidDocument {
    pub id: Uuid,
    /// The document that created the debt — a sale for a customer, a purchase for a supplier.
    /// The panel links to it, so it is the id it needs, not the obligation's.
    pub source_document_id: Uuid,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub total: Decimal,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub outstanding: Decimal,
    pub occurred_at: DateTime<Utc>,
}

/// The difference between the two mirrors, and it is only the field names: a customer's
/// `owed` is money we are owed, a supplier's `owed_to_them` is money we owe. One struct per
/// direction keeps the wire honest — a client reading `owed` on a supplier would be reading
/// the wrong thing, and a shared name is how that mistake becomes invisible.
#[derive(Debug, Serialize)]
pub struct CustomerAccount {
    pub id: Uuid,
    pub name: String,
    /// What this customer owes, across every debt that is not fully resolved.
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub owed: Decimal,
    /// What we hold of theirs: money that arrived and was not applied to a debt.
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub in_favour: Decimal,
    /// Their sales with `outstanding > 0`, newest first. A settled sale is **absent**, and
    /// the absence is the state — there is no `paid` column to read.
    pub unpaid_sales: Vec<UnpaidDocument>,
}

#[derive(Debug, Serialize)]
pub struct SupplierAccount {
    pub id: Uuid,
    pub name: String,
    /// What we owe this supplier.
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub owed_to_them: Decimal,
    /// What they hold of ours: money we sent that was not applied to a payable.
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub in_our_favour: Decimal,
    /// Their purchases with `outstanding > 0`, newest first.
    pub unpaid_purchases: Vec<UnpaidDocument>,
}

/// Which side of the book this account is on.
///
/// A closed enum rather than a `&str`: the two branches differ in the table names they read
/// and in the direction of the payment, and a typo in a string would become a query against
/// the wrong table rather than a compile error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Party {
    Customer,
    Supplier,
}

impl Party {
    /// The party kind as the `party_kind` columns spell it.
    fn kind(self) -> &'static str {
        match self {
            Party::Customer => "customer",
            Party::Supplier => "supplier",
        }
    }
}

/// The two figures and the unpaid documents, for either side.
///
/// Returned as `None` when the party does not exist, which the handlers turn into a `404`: an
/// unknown party must not produce zeros, because "owes nothing" and "is not there" are
/// different statements and the panel renders them differently (`S2`).
async fn account_of(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    party: Party,
    id: Uuid,
) -> Result<Option<(String, Decimal, Decimal, Vec<UnpaidDocument>)>, sqlx::Error> {
    // Who they are. A single lookup, and the `name` doubles as the existence check — the two
    // queries below would both return zero for an unknown id, which is exactly the false
    // "owes nothing" this avoids.
    let name: Option<String> = match party {
        Party::Customer => sqlx::query_scalar("SELECT name FROM customers WHERE id = $1"),
        Party::Supplier => sqlx::query_scalar("SELECT name FROM suppliers WHERE id = $1"),
    }
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(name) = name else {
        return Ok(None);
    };

    // **The debt, in one statement.** `SUM(amount)` over the party's documents, less what the
    // documents lowered, less what money settled — a subtraction of stored rows, so it cannot
    // drift from them the way a cached balance would.
    let owed: Decimal = match party {
        Party::Customer => sqlx::query_scalar(
            "SELECT COALESCE(SUM(o.amount), 0)
                    - COALESCE((SELECT SUM(a.amount) FROM obligations_adjustments a
                                  JOIN obligations ao ON ao.id = a.obligation_id
                                 WHERE ao.party_kind = $1 AND ao.party_id = $2), 0)
                    - COALESCE((SELECT SUM(s.amount) FROM settlements s
                                  JOIN obligations so ON so.id = s.obligation_id
                                 WHERE so.party_kind = $1 AND so.party_id = $2), 0)
               FROM obligations o
              WHERE o.party_kind = $1 AND o.party_id = $2",
        ),
        Party::Supplier => sqlx::query_scalar(
            "SELECT COALESCE(SUM(o.amount), 0)
                    - COALESCE((SELECT SUM(a.amount) FROM payables_adjustments a
                                  JOIN payables ao ON ao.id = a.payable_id
                                 WHERE ao.party_kind = $1 AND ao.party_id = $2), 0)
                    - COALESCE((SELECT SUM(s.amount) FROM payable_settlements s
                                  JOIN payables so ON so.id = s.payable_id
                                 WHERE so.party_kind = $1 AND so.party_id = $2), 0)
               FROM payables o
              WHERE o.party_kind = $1 AND o.party_id = $2",
        ),
    }
    .bind(party.kind())
    .bind(id)
    .fetch_one(&mut **tx)
    .await?;

    // **What the party holds of ours, or we of theirs.** Money that moved in this party's
    // direction, less the part of it already applied to a debt. This is the same subtraction
    // `lock_party_balance` and `lock_party_sent` perform, and it is stated here rather than
    // guessed: a customer's `in` minus its settlements, a supplier's `out` minus its
    // payable settlements.
    let in_favour: Decimal = match party {
        Party::Customer => sqlx::query_scalar(
            "SELECT COALESCE((SELECT SUM(p.total) FROM payments p
                               WHERE p.party_kind = $1 AND p.party_id = $2
                                 AND p.direction = 'in'), 0)
                    - COALESCE((SELECT SUM(s.amount) FROM settlements s
                                  JOIN payments sp ON sp.id = s.payment_id
                                 WHERE sp.party_kind = $1 AND sp.party_id = $2
                                   AND sp.direction = 'in'), 0)",
        ),
        Party::Supplier => sqlx::query_scalar(
            "SELECT COALESCE((SELECT SUM(p.total) FROM payments p
                               WHERE p.party_kind = $1 AND p.party_id = $2
                                 AND p.direction = 'out'), 0)
                    - COALESCE((SELECT SUM(s.amount) FROM payable_settlements s
                                  JOIN payments sp ON sp.id = s.payment_id
                                 WHERE sp.party_kind = $1 AND sp.party_id = $2
                                   AND sp.direction = 'out'), 0)",
        ),
    }
    .bind(party.kind())
    .bind(id)
    .fetch_one(&mut **tx)
    .await?;

    // **The documents still owed on**, each with its own outstanding, in one query with a
    // `HAVING` rather than a filter after the fact: a settled document must not be fetched at
    // all, so `outstanding > 0` belongs in the query and not in the client.
    let unpaid: Vec<(Uuid, Uuid, Decimal, Decimal, DateTime<Utc>)> = match party {
        Party::Customer => sqlx::query_as(
            "SELECT s.id, o.source_document_id, s.total,
                    o.amount
                      - COALESCE((SELECT SUM(a.amount) FROM obligations_adjustments a
                                   WHERE a.obligation_id = o.id), 0)
                      - COALESCE((SELECT SUM(st.amount) FROM settlements st
                                   WHERE st.obligation_id = o.id), 0) AS outstanding,
                    s.created_at
               FROM obligations o
               JOIN sales s ON s.id = o.source_document_id
              WHERE o.party_kind = $1 AND o.party_id = $2
                AND o.source_document_kind = 'sale'
                AND o.amount
                      - COALESCE((SELECT SUM(a.amount) FROM obligations_adjustments a
                                   WHERE a.obligation_id = o.id), 0)
                      - COALESCE((SELECT SUM(st.amount) FROM settlements st
                                   WHERE st.obligation_id = o.id), 0) > 0
              ORDER BY s.created_at DESC, s.id",
        ),
        Party::Supplier => sqlx::query_as(
            "SELECT p.id, o.source_document_id, p.total,
                    o.amount
                      - COALESCE((SELECT SUM(a.amount) FROM payables_adjustments a
                                   WHERE a.payable_id = o.id), 0)
                      - COALESCE((SELECT SUM(st.amount) FROM payable_settlements st
                                   WHERE st.payable_id = o.id), 0) AS outstanding,
                    p.created_at
               FROM payables o
               JOIN purchases p ON p.id = o.source_document_id
              WHERE o.party_kind = $1 AND o.party_id = $2
                AND o.source_document_kind = 'purchase'
                AND o.amount
                      - COALESCE((SELECT SUM(a.amount) FROM payables_adjustments a
                                   WHERE a.payable_id = o.id), 0)
                      - COALESCE((SELECT SUM(st.amount) FROM payable_settlements st
                                   WHERE st.payable_id = o.id), 0) > 0
              ORDER BY p.created_at DESC, p.id",
        ),
    }
    .bind(party.kind())
    .bind(id)
    .fetch_all(&mut **tx)
    .await?;

    let documents = unpaid
        .into_iter()
        .map(
            |(id, source_document_id, total, outstanding, occurred_at)| UnpaidDocument {
                id,
                source_document_id,
                total,
                outstanding,
                occurred_at,
            },
        )
        .collect();

    Ok(Some((name, owed, in_favour, documents)))
}

async fn customer_account(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CustomerAccount>, StatusCode> {
    // **One transaction, so the three boxes describe the same instant** (`R3-001` of review
    // `review-b1f1ea20a0e4419b`, corroborated by the refuter). Four separate pool statements
    // let a concurrent collection land between the balance and the unpaid list: the panel would
    // then show "owes 0.00" above a sale with 40 outstanding, or the reverse. The endpoint exists
    // to be one consistent document, so it reads under one snapshot.
    //
    // `REPEATABLE READ` is what makes that a property rather than a timing coincidence: the
    // default `READ COMMITTED` gives each statement its own snapshot, which is exactly the defect.
    let mut tx = state
        .db
        .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .await
        .map_err(|e| {
            tracing::error!(?e, "account transaction failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let (name, owed, in_favour, unpaid_sales) = account_of(&mut tx, Party::Customer, id)
        .await
        .map_err(|e| {
            tracing::error!(?e, "customer account failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "account commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(CustomerAccount {
        id,
        name,
        owed,
        in_favour,
        unpaid_sales,
    }))
}

async fn supplier_account(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SupplierAccount>, StatusCode> {
    // The same single snapshot as the customer side, for the same reason.
    let mut tx = state
        .db
        .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .await
        .map_err(|e| {
            tracing::error!(?e, "account transaction failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let (name, owed_to_them, in_our_favour, unpaid_purchases) =
        account_of(&mut tx, Party::Supplier, id)
            .await
            .map_err(|e| {
                tracing::error!(?e, "supplier account failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or(StatusCode::NOT_FOUND)?;
    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "account commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(SupplierAccount {
        id,
        name,
        owed_to_them,
        in_our_favour,
        unpaid_purchases,
    }))
}
