use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// A sellable product. The list price is stored in integer cents and is the
/// default a sale line prefills with; each line snapshots its own price.
// Removed when the sale endpoints (work-units 3-4) construct these.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Product {
    pub id: Uuid,
    pub name: String,
    pub list_price: i64,
    pub created_at: DateTime<Utc>,
}
