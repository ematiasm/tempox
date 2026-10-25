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
    /// The method the money came back through, or `None` for a return that only lowered a
    /// payable (`odd/tasks/v4-payables.md` S8, migration `0023`).
    ///
    /// **`None` is a claim, not a missing value**, exactly as it is on a sale and a purchase
    /// since `0021`: this return moved no money, because the purchase it corrects never did.
    /// `0023`'s constraint trigger is what keeps the two from disagreeing, so a client reading
    /// `null` here knows the debt dropped without asking a second endpoint.
    ///
    /// The name is not resolved alongside it, and the asymmetry with `Sale` and `Purchase` is
    /// deliberate: those are listed in screens that render a rail per row, and a return is
    /// nested inside the purchase that already carries one. Adding a join here would be a second
    /// copy of the same fact on the wire.
    pub payment_method_id: Option<Uuid>,
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
