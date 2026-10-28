use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// An expense category: how the shop classifies what it spent.
///
/// A row rather than a closed set, which is the whole decision `D2` of
/// `odd/tasks/v4-expenses.md` records: adding a category is an `INSERT`, never a
/// migration. This schema has paid three times for the alternative —
/// `stock_movements.reason`, `money_entries.document_kind` (which lost `annulment`
/// once) and `obligations.source_document_kind`.
///
/// `is_active` mirrors `payment_methods.is_active`, and for the same reason: an
/// expense that named a category is history, so the row cannot be deleted and a
/// picker must still be able to stop offering it. `GET` returns retired rows on
/// purpose — a ledger rendering a historical expense needs the name of a category
/// retired afterwards.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ExpenseCategory {
    pub id: Uuid,
    pub name: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}
