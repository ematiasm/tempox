use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// One line of a credit note: how much of one sale line came back.
///
/// `sale_item_id` is what the cumulative guard sums, and the name, unit and
/// price are snapshots of the corrected sale line, so the note reads as a
/// self-contained receipt even if the catalog changes later.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CreditNoteItem {
    pub id: Uuid,
    pub credit_note_id: Uuid,
    pub sale_item_id: Uuid,
    pub product_id: Uuid,
    pub product_name: String,
    /// Integer quantity returned; never greater than the quantity sold on the
    /// line it corrects.
    pub quantity: i32,
    /// Snapshot of the sale line's price. A bonus line's price is `0.00`, and a
    /// refund of a bonus line returns nothing while its stock still comes back.
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub unit_price: Decimal,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub line_total: Decimal,
    pub unit: String,
}
