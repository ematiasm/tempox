use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// One append-only ledger entry. A product's stock is the SUM of its
/// movements — never a mutable quantity field.
///
/// Not constructed anywhere yet: `products.rs` reads the balance with an
/// aggregate instead of materializing rows, and nothing reads individual
/// movements. Kept as the shape of a `stock_movements` row for the v1.5
/// reversal work, which will write and show them.
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
