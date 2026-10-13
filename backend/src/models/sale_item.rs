use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// One line of a sale. The product name, unit, and prices are snapshots taken
/// at sale time, so later catalog changes never rewrite history.
// Removed when the sale endpoints (work-units 3-4) construct these.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct SaleItem {
    pub id: Uuid,
    pub sale_id: Uuid,
    pub product_id: Uuid,
    pub product_name: String,
    pub quantity: i32,
    pub unit_price: i64,
    pub line_total: i64,
    pub unit: String,
}
