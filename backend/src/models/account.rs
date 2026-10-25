use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// The seeded `cash` account (`00000000-0000-0000-0000-000000000003`).
///
/// A fixed id, exactly as `WALK_IN_CUSTOMER_ID` and `ONE_OFF_SUPPLIER_ID` are.
/// It is what lets the server name the till without a lookup and — after v3
/// retired `/api/cash` — without a classification column.
pub const CASH_ACCOUNT_ID: Uuid = Uuid::from_u128(3);

/// The seeded `cash` payment method (`…0004`).
pub const CASH_PAYMENT_METHOD_ID: Uuid = Uuid::from_u128(4);

/// An account: where money lives. `is_default` marks the one the shop means by
/// "the till", and exactly one row holds it.
///
/// No `kind`. A classification was argued for and withdrawn: in v3 its values
/// would be indistinguishable, and locating the till is already solved by the
/// fixed id above. Adding it later is an `ALTER TABLE` with a default over a
/// handful of rows.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Account {
    pub id: Uuid,
    pub name: String,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
}

/// A payment method: the concrete thing a cashier picks, tied to exactly one
/// account. Documents name the method and the method resolves the account.
///
/// `is_active` is a **picker** concern only. The write paths accept an inactive
/// method, which is what keeps the `walk-in`+credit refusal from passing merely
/// because the `credit` method is seeded inactive.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct PaymentMethod {
    pub id: Uuid,
    pub account_id: Uuid,
    pub name: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}
