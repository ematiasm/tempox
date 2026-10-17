use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// A credit note — the compensating document that reverses part or all of a
/// sale.
///
/// The sale keeps every byte it was recorded with: `status` stays `confirmed`,
/// `reversed_at` stays NULL, and its total is never rewritten. The correction is
/// this second document, and the refunded quantity lives in its lines, which is
/// what makes a partial refund representable at all.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CreditNote {
    pub id: Uuid,
    pub sale_id: Uuid,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub total: Decimal,
    pub created_at: DateTime<Utc>,
}
