use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// A sale header — an immutable event. Corrections are reversals, never edits.
/// `status` moves toward the lifecycle draft -> confirmed -> cancel -> done.
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
