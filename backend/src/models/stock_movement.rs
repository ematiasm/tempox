use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// One append-only ledger entry. A product's stock is the SUM of its
/// movements — never a mutable quantity field.
///
/// Not constructed anywhere yet: `products.rs` reads the balance with an
/// aggregate instead of materializing rows, and nothing reads individual
/// movements. Kept as the documented shape of a `stock_movements` row; the
/// v1.5 credit-note path writes its own rows with `sqlx` directly, because what
/// it needs to insert is not what a reader needs to decode.
#[allow(dead_code)]
#[derive(Debug, Clone, FromRow)]
pub struct StockMovement {
    pub id: Uuid,
    pub product_id: Uuid,
    /// Signed: negative = out, positive = in.
    pub quantity: i32,
    pub reason: String,
    pub sale_id: Option<Uuid>,
    /// Set on a `'credit_note'` row: the document that brought this stock back.
    /// The sale is never edited by a refund — the reversing movement is a new
    /// row, which is why the append-only rule above it still holds.
    pub credit_note_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}
