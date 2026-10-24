use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// A sale header — an immutable event. Corrections are credit notes, never edits.
///
/// **No `status`, no `reversed_at`.** v1 reserved them for a lifecycle that never
/// happened: nothing but the column default ever wrote either, and `0015` dropped
/// them. An annulment is derived from the credit notes and from nothing else —
/// exactly as it already was for `purchases`, `credit_notes` and `supplier_returns`,
/// where "the row exists" and "it counts" are one statement.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Sale {
    pub id: Uuid,
    pub customer_id: Uuid,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub total: Decimal,
    /// The payment method this sale was paid with, or `None` for a sale on account.
    ///
    /// **`None` is a claim, not a missing value** (`odd/tasks/v4-receivables.md` S2):
    /// this sale moved no money, and `obligations` holds what it owes. The constraint
    /// trigger in `0021` is what keeps the two from disagreeing, so a client reading
    /// `null` here knows the debt exists without asking a second endpoint.
    ///
    /// The id is the identity; the name beside it is **resolved by join**, never stored.
    ///
    /// Not a stored column because a method's name is editable and the document's
    /// identity is its id, so joining by id is what lets a rename *correct* history
    /// instead of freezing a typo into rows that are immutable by trigger.
    ///
    /// The wire shape is **flat** (`payment_method_id` + `payment_method_name`), not
    /// nested and without `account_id`: a document with one effect does not need the
    /// account, and `money_movements` is the single place that translates method to
    /// account.
    #[serde(rename = "payment_method_id")]
    pub payment_method_id: Option<Uuid>,
    /// `None` exactly when `payment_method_id` is: a sale that moved no money has no rail
    /// to name. The read paths use `LEFT JOIN` for this reason — an `INNER JOIN` would
    /// drop a sale on account from every list, silently.
    #[serde(rename = "payment_method_name")]
    pub payment_method_name: Option<String>,
    pub created_at: DateTime<Utc>,
}
