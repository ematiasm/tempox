use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// One line of a sale. The product name, unit, and prices are snapshots taken
/// at sale time, so later catalog changes never rewrite history.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct SaleItem {
    pub id: Uuid,
    pub sale_id: Uuid,
    pub product_id: Uuid,
    pub product_name: String,
    pub quantity: i32,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub unit_price: Decimal,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub line_total: Decimal,
    pub unit: String,
}
