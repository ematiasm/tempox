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
    /// The payment method this sale was paid with. The id is the identity; the name
    /// beside it is **resolved by join**, never stored.
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
    pub payment_method_id: Uuid,
    #[serde(rename = "payment_method_name")]
    pub payment_method_name: String,
    pub created_at: DateTime<Utc>,
}
