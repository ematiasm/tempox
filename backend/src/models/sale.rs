use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// A sale header — an immutable event. Corrections are reversals, never edits.
/// `status` moves toward the lifecycle draft -> confirmed -> cancel -> done.
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
    pub status: String,
    pub reversed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
