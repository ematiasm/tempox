use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// A return of goods to their supplier: the compensating document for a purchase.
///
/// It exists because a purchase is immutable — `0009` gave `purchases` triggers
/// that reject UPDATE, DELETE and TRUNCATE — so a correction cannot be an edit,
/// and deleting the purchase would erase a real movement of money and stock.
///
/// There is no `status`: a return is a fact the moment it exists, exactly as a
/// purchase is. Whether a purchase is fully returned is derived from its returns,
/// never written onto it.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct SupplierReturn {
    pub id: Uuid,
    pub purchase_id: Uuid,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub total: Decimal,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct SupplierReturnItem {
    pub id: Uuid,
    pub supplier_return_id: Uuid,
    /// The purchase line this return corrects. The document reports what came
    /// from where, so a reader never has to infer it from product ids.
    pub purchase_item_id: Uuid,
    pub product_id: Uuid,
    /// Snapshot of the purchase line: renaming the product never rewrites history.
    pub product_name: String,
    pub quantity: i32,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub unit_price: Decimal,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub line_total: Decimal,
    pub unit: String,
}

/// A return with its lines, which is how both the create response and the
/// nested reads answer.
#[derive(Debug, Clone, Serialize)]
pub struct SupplierReturnWithItems {
    #[serde(rename = "return")]
    pub supplier_return: SupplierReturn,
    pub items: Vec<SupplierReturnItem>,
}
