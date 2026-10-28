use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// An expense: money leaving that touches no stock.
///
/// The everyday member of the family `0018` opened with three one-off documents — a
/// utility bill, a repair, a salary. `odd/tasks/v4-expenses.md` S2.
///
/// `payment_method_id` is `Option<Uuid>` and **the null is a claim, not a free pass**:
/// the expense owes rather than paid, and the constraint trigger on the table holds that
/// claim at COMMIT against the `payments` rows naming this expense.
///
/// **There is no `paid` column and no `outstanding` column**, and their absence is the
/// rule every balance in this schema keeps: an outstanding figure is
/// `amount − SUM(payments)`, a subtraction of stored rows, and a stored figure that is a
/// subtraction can disagree with its own sources. `0015` dropped a whole lifecycle from
/// `sales` for that reason.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Expense {
    pub id: Uuid,
    pub category_id: Uuid,
    /// The name of the category, joined rather than snapshotted: a rename corrects
    /// history instead of freezing a typo, exactly as a payment method's name does.
    pub category_name: String,
    pub payee: String,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub amount: Decimal,
    pub occurred_at: DateTime<Utc>,
    pub payment_method_id: Option<Uuid>,
    /// The method's name, joined for the same reason. `None` means "on account" and
    /// renders differently from a blank name, which is a defect in the row rather than a
    /// statement about the money — the distinction `fix/on-account-method-cell` made for
    /// the two ledgers after a review found them conflated.
    pub payment_method_name: Option<String>,
    pub created_at: DateTime<Utc>,
}
