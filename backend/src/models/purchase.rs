use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::models::supplier_return::SupplierReturnWithItems;

/// A purchase: goods that arrived and money that left.
///
/// There is no `status` column, and the absence is the design rather than an
/// omission. A sale carries a reserved lifecycle because v1's reversal needed
/// one; a purchase is a confirmed fact the moment it exists, so "the row exists"
/// and "it counts" stay the same statement — in stock, in cash, and in every
/// derived number. A purchase *order* is a different document, deferred to v2.5,
/// and adding it later needs no change to this table.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Purchase {
    pub id: Uuid,
    pub supplier_id: Uuid,
    /// The method the money left through, and its name **joined** — never stored, so
    /// a rename corrects history instead of freezing a typo. Flat on the wire, the
    /// same shape a sale uses.
    #[serde(rename = "payment_method_id")]
    pub payment_method_id: Uuid,
    #[serde(rename = "payment_method_name")]
    pub payment_method_name: String,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub total: Decimal,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct PurchaseItem {
    pub id: Uuid,
    pub purchase_id: Uuid,
    pub product_id: Uuid,
    /// Snapshot taken at purchase time: renaming the product never rewrites history.
    pub product_name: String,
    pub quantity: i32,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub unit_price: Decimal,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub line_total: Decimal,
    pub unit: String,
}

/// A purchase with its lines, and — in the list — the returns that corrected it.
///
/// The create response leaves `supplier_returns` empty because a purchase cannot
/// have any yet; the list fills it. It is an array and never an `Option`, so a
/// client has one shape to read instead of two code paths for one state.
#[derive(Debug, Clone, Serialize)]
pub struct PurchaseWithItems {
    pub purchase: Purchase,
    pub items: Vec<PurchaseItem>,
    pub supplier_returns: Vec<SupplierReturnWithItems>,
}
