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
    /// The payment method this purchase was paid with, or `None` for a purchase on account.
    ///
    /// **`None` is a claim, not a missing value** (`odd/tasks/v4-payables.md` S2): this
    /// purchase moved no money, and `payables` holds what it owes. The constraint trigger in
    /// `0023` keeps the two from disagreeing, so a client reading `null` here knows the debt
    /// exists without asking a second endpoint.
    ///
    /// The id is the identity; the name beside it is **resolved by join**, never stored, so a
    /// rename corrects history instead of freezing a typo. Flat on the wire, the same shape a
    /// sale uses.
    #[serde(rename = "payment_method_id")]
    pub payment_method_id: Option<Uuid>,
    /// `None` exactly when `payment_method_id` is: a purchase on account has no rail to name.
    /// The read paths use `LEFT JOIN` for this reason — an `INNER JOIN` would drop it from
    /// every list, silently.
    #[serde(rename = "payment_method_name")]
    pub payment_method_name: Option<String>,
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
