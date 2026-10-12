use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// One append-only ledger entry. A product's stock is the SUM of its
/// movements — never a mutable quantity field.
// Removed when the sale endpoints (work-units 3-4) construct these.
#[allow(dead_code)]
#[derive(Debug, Clone, FromRow)]
pub struct StockMovement {
    pub id: Uuid,
    pub product_id: Uuid,
    /// Signed: negative = out, positive = in.
    pub quantity: i32,
    pub reason: String,
    pub sale_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}
