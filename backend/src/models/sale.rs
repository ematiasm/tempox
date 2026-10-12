use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// A sale header — an immutable event. Corrections are reversals, never edits.
/// `status` moves toward the lifecycle draft -> confirmed -> cancel -> done.
// Removed when the sale endpoints (work-units 3-4) construct these.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Sale {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub total: i64,
    pub payment_method: String,
    pub status: String,
    pub reversed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
