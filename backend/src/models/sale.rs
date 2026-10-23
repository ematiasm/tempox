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
    /// The payment method's **name, resolved by join** on `payment_method_id`.
    ///
    /// This is deliberately not a stored column: a method's name is editable and
    /// the document's identity is its id, so joining by id is what keeps history
    /// correct. Snapshotting the string here would freeze a typo into rows that
    /// are immutable by trigger. The wire keeps this same field and value in v3's
    /// migration; the id and the wire rename arrive with the sales write path.
    pub payment_method: String,
    pub status: String,
    pub reversed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
