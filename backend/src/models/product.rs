use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// A sellable product. The list price is a decimal currency amount and is the
/// default a sale line prefills with; each line snapshots its own price.
///
/// Not constructed anywhere yet: the product endpoints answer with
/// `ProductWithStock`, which carries the derived stock this struct has no room
/// for. Kept because it is the shape of the `products` row and v1a's catalog
/// work is expected to use it.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Product {
    pub id: Uuid,
    pub name: String,
    pub list_price: Decimal,
    pub unit: String,
    pub created_at: DateTime<Utc>,
}
