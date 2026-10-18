use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// A supplier. Minimal by design, and a deliberate mirror of `Customer`: a name
/// and nothing else. Contact details, tax id and payment terms grow the same way
/// customers will grow, and none of it is in v2.
///
/// There is no edit and no delete endpoint. A purchase that named a supplier is
/// history, and a deletable supplier would be a hole in it — the same reasoning
/// that put the walk-in customer beyond reach.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Supplier {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
}
