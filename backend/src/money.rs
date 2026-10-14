//! Money arithmetic in integer cents.
//!
//! Money is never a float in Tempox: everything is an integer count of cents
//! (`1500` means $15.00). These pure functions are the single source of the
//! `line_total` and `sales.total` invariants that the database checks assume.

/// An amount of money, in integer cents.
pub type Cents = i64;

/// One line of a sale before it is written: quantity and unit price in cents.
pub struct LineInput {
    pub quantity: i32,
    pub unit_price: Cents,
}

/// `line_total = quantity * unit_price`, in exact integer arithmetic.
///
/// Returns `None` when the line violates an invariant the database enforces
/// (non-positive quantity, negative price) or when the multiplication would
/// overflow: money math fails loudly instead of wrapping silently.
///
/// A zero price is valid — a bonus line (regalo) is a real line whose price
/// happens to be nothing.
pub fn line_total(quantity: i32, unit_price: Cents) -> Option<Cents> {
    if quantity <= 0 || unit_price < 0 {
        return None;
    }
    i64::from(quantity).checked_mul(unit_price)
}

/// `sales.total = SUM(line_total)` over the sale's lines.
///
/// Returns `None` if any line is invalid. An empty slice yields `0`: the API
/// rejects empty item lists with 400 before this is ever reached. A sale whose
/// lines are all bonus lines totals `0` and is still a real sale.
pub fn sale_total(lines: &[LineInput]) -> Option<Cents> {
    let mut total: Cents = 0;
    for line in lines {
        total = total.checked_add(line_total(line.quantity, line.unit_price)?)?;
    }
    Some(total)
}

// Unit tests for the money module live here first (RED); the functions land after.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_total_multiplies_quantity_by_unit_price() {
        // 3 × $2.50 = $7.50
        assert_eq!(line_total(3, 250), Some(750));
    }

    #[test]
    fn line_total_rejects_non_positive_quantity() {
        assert_eq!(line_total(0, 250), None);
        assert_eq!(line_total(-2, 250), None);
    }

    #[test]
    fn line_total_rejects_negative_price() {
        // Zero is allowed: a bonus line (regalo) is a real sale line whose
        // price happens to be nothing. Only a negative price is nonsense.
        assert_eq!(line_total(2, -100), None);
    }

    #[test]
    fn line_total_allows_a_zero_price() {
        assert_eq!(line_total(3, 0), Some(0));
    }

    #[test]
    fn line_total_reports_overflow_instead_of_wrapping() {
        // 2 × i64::MAX cannot be represented — math fails loudly, never wraps.
        assert_eq!(line_total(2, i64::MAX), None);
    }

    #[test]
    fn sale_total_sums_all_lines() {
        let lines = [
            LineInput {
                quantity: 2,
                unit_price: 250,
            }, // $5.00
            LineInput {
                quantity: 1,
                unit_price: 1000,
            }, // $10.00
            LineInput {
                quantity: 4,
                unit_price: 75,
            }, // $3.00
        ];
        assert_eq!(sale_total(&lines), Some(1800)); // $18.00
    }

    #[test]
    fn sale_total_of_no_lines_is_zero() {
        assert_eq!(sale_total(&[]), Some(0));
    }

    #[test]
    fn sale_total_stays_exact_where_float_would_not() {
        // 3 × $0.10 is 0.30000000000000004 in float; in cents it is exact.
        let lines = [LineInput {
            quantity: 3,
            unit_price: 10,
        }];
        assert_eq!(sale_total(&lines), Some(30));
    }

    #[test]
    fn sale_total_of_bonus_lines_is_zero() {
        // A sale made entirely of bonus lines is worth nothing but still
        // happened: the stock left the shop, so the row must exist at 0.
        let lines = [
            LineInput {
                quantity: 3,
                unit_price: 0,
            },
            LineInput {
                quantity: 1,
                unit_price: 0,
            },
        ];
        assert_eq!(sale_total(&lines), Some(0));
    }

    #[test]
    fn sale_total_propagates_an_invalid_line() {
        let lines = [
            LineInput {
                quantity: 2,
                unit_price: 250,
            },
            LineInput {
                quantity: 0,
                unit_price: 250,
            }, // invalid line poisons the sale
        ];
        assert_eq!(sale_total(&lines), None);
    }
}
