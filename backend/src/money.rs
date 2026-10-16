//! Money arithmetic in decimal currency amounts.
//!
//! Money is never a float in Tempox: the type is [`Decimal`], the wire format is
//! a decimal string, and no binary float touches an amount on this path. Since
//! `0006` the amount is a currency value (`25.00` is $25.00) rather than an
//! integer count of cents, and the unit is two decimal places — a whole number
//! of cents.
//!
//! Why the guard exists even though the arithmetic cannot break it: an integer
//! quantity times a 2-decimal price is exactly 2 decimals, and a sum of
//! 2-decimal values is exactly 2 decimals, so a correct caller never produces a
//! third decimal. [`is_whole_cents`] is not there for the correct caller. It is
//! there so that a *future* change which does produce one fails loudly in Rust
//! instead of being silently rounded away by PostgreSQL, which is what
//! `NUMERIC(14,2)` would have done:
//!
//! ```text
//! If the scale of a value to be stored is greater than the declared scale of
//! the column, the system will round the value to the specified number of
//! fractional digits.
//! ```
//!
//! The column declares 6 and the guard demands 2 for exactly that reason: the
//! column stores what was sent, and the guard refuses what is not money.

use rust_decimal::Decimal;
use rust_decimal::RoundingStrategy;

/// A currency amount, with two decimal places (a whole number of cents).
pub type Amount = Decimal;

/// The number of decimal places money carries.
pub const MONEY_SCALE: u32 = 2;

/// One line of a sale before it is written: quantity and unit price.
pub struct LineInput {
    pub quantity: i32,
    pub unit_price: Amount,
}

/// Whether `amount` is a whole number of cents.
///
/// Deliberately a comparison of *values*, not of the textual scale: `25.000000`
/// and `25.00` are the same amount and both pass, while `1.235` does not. A
/// `scale(amount) <= 2` test would reject the first for the crime of having been
/// written with trailing zeroes.
pub fn is_whole_cents(amount: Amount) -> bool {
    amount.round_dp_with_strategy(MONEY_SCALE, RoundingStrategy::MidpointAwayFromZero) == amount
}

/// Serialize an amount as a string with exactly [`MONEY_SCALE`] decimals.
///
/// The wire contract is one shape for an amount, and that includes zero — but a
/// plain `Decimal` field does not deliver it on its own. `sqlx` decodes
/// PostgreSQL's zero as `Decimal::ZERO`, whose scale is 0, because the server
/// sends an empty digit array for `0.00`:
///
/// ```text
/// if digits.is_empty() {
///     // Postgres returns an empty digit array for 0
///     return Ok(Decimal::ZERO);
/// }
/// ```
///
/// so a zero would leave as `"0"` while every other amount left as `"25.00"`.
/// Rescaling here is what makes the shape uniform, in one place, for every
/// amount the API returns.
///
/// The rescale is safe rather than sloppy: it can only shorten an amount that
/// already has at most two decimals, because `is_whole_cents` asserts that on
/// the way in and the column's `CHECK (v = round(v, 2))` enforces it on the way
/// out. A value finer than a cent cannot reach this function.
pub fn serialize_amount<S>(amount: &Amount, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut value = *amount;
    value.rescale(MONEY_SCALE);
    serializer.serialize_str(&value.to_string())
}

/// `line_total = quantity * unit_price`, in exact decimal arithmetic.
///
/// Returns `None` when the line violates an invariant the database enforces
/// (non-positive quantity, negative price), when the multiplication would
/// overflow, or when the result is not a whole number of cents.
///
/// A zero price is valid — a bonus line (regalo) is a real line whose price
/// happens to be nothing.
pub fn line_total(quantity: i32, unit_price: Amount) -> Option<Amount> {
    if quantity <= 0 || unit_price.is_sign_negative() {
        return None;
    }
    let mut total = Decimal::from(quantity).checked_mul(unit_price)?;
    if !is_whole_cents(total) {
        return None;
    }
    // Normalize to the money scale so the wire carries `5.00`, not `5.000000`.
    // The value is unchanged; only the number of digits written changes.
    total.rescale(MONEY_SCALE);
    Some(total)
}

/// `sales.total = SUM(line_total)` over the sale's lines.
///
/// Returns `None` if any line is invalid. An empty slice yields `0`: the API
/// rejects empty item lists with 400 before this is ever reached. A sale whose
/// lines are all bonus lines totals `0` and is still a real sale.
pub fn sale_total(lines: &[LineInput]) -> Option<Amount> {
    let mut total = Amount::ZERO;
    for line in lines {
        total = total.checked_add(line_total(line.quantity, line.unit_price)?)?;
    }
    if !is_whole_cents(total) {
        return None;
    }
    total.rescale(MONEY_SCALE);
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).expect("test literal is a decimal")
    }

    #[test]
    fn line_total_multiplies_quantity_by_unit_price() {
        // 3 x $2.50 = $7.50
        assert_eq!(line_total(3, d("2.50")), Some(d("7.50")));
    }

    #[test]
    fn line_total_rejects_non_positive_quantity() {
        assert_eq!(line_total(0, d("2.50")), None);
        assert_eq!(line_total(-2, d("2.50")), None);
    }

    #[test]
    fn line_total_rejects_negative_price() {
        // Zero is allowed: a bonus line (regalo) is a real sale line whose
        // price happens to be nothing. Only a negative price is nonsense.
        assert_eq!(line_total(2, d("-1.00")), None);
    }

    #[test]
    fn line_total_allows_a_zero_price() {
        assert_eq!(line_total(3, d("0")), Some(d("0")));
    }

    #[test]
    fn line_total_refuses_a_result_finer_than_a_cent() {
        // The rule the column enforces: money is a whole number of cents.
        // 3 x $0.012 = $0.036, which is not expressible, and rounding it here
        // would record a price nobody typed.
        assert_eq!(line_total(3, d("0.012")), None);
    }

    #[test]
    fn line_total_accepts_a_price_written_with_extra_zeroes() {
        // 25.000000 is 25.00. The guard is about the value, not the spelling.
        assert_eq!(line_total(1, d("25.000000")), Some(d("25.00")));
    }

    #[test]
    fn line_total_reports_overflow_instead_of_wrapping() {
        assert_eq!(line_total(2, Decimal::MAX), None);
    }

    #[test]
    fn sale_total_sums_all_lines() {
        let lines = [
            LineInput {
                quantity: 2,
                unit_price: d("2.50"),
            }, // $5.00
            LineInput {
                quantity: 1,
                unit_price: d("10.00"),
            }, // $10.00
            LineInput {
                quantity: 4,
                unit_price: d("0.75"),
            }, // $3.00
        ];
        assert_eq!(sale_total(&lines), Some(d("18.00")));
    }

    #[test]
    fn sale_total_of_no_lines_is_zero() {
        assert_eq!(sale_total(&[]), Some(Decimal::ZERO));
    }

    #[test]
    fn sale_total_stays_exact_where_float_would_not() {
        // 3 x $0.10 is 0.30000000000000004 in float; in decimal it is exact.
        let lines = [LineInput {
            quantity: 3,
            unit_price: d("0.10"),
        }];
        assert_eq!(sale_total(&lines), Some(d("0.30")));
    }

    #[test]
    fn sale_total_of_bonus_lines_is_zero() {
        // A sale made entirely of bonus lines is worth nothing but still
        // happened: the stock left the shop, so the row must exist at 0.
        let lines = [
            LineInput {
                quantity: 3,
                unit_price: d("0"),
            },
            LineInput {
                quantity: 1,
                unit_price: d("0"),
            },
        ];
        assert_eq!(sale_total(&lines), Some(Decimal::ZERO));
    }

    #[test]
    fn sale_total_propagates_an_invalid_line() {
        let lines = [
            LineInput {
                quantity: 2,
                unit_price: d("2.50"),
            },
            LineInput {
                quantity: 0,
                unit_price: d("2.50"),
            }, // invalid line poisons the sale
        ];
        assert_eq!(sale_total(&lines), None);
    }

    #[test]
    fn is_whole_cents_is_about_the_value_not_the_spelling() {
        assert!(is_whole_cents(d("25.00")));
        assert!(is_whole_cents(d("25.000000")));
        assert!(is_whole_cents(d("0")));
        assert!(is_whole_cents(d("99999999.99")));
        assert!(!is_whole_cents(d("1.235")));
        assert!(!is_whole_cents(d("0.036")));
    }

    #[test]
    fn the_wire_carries_two_decimals_and_a_string() {
        // The contract: an amount is a JSON string with exactly two decimal
        // places, whatever spelling or scale went in. `rust_decimal` serializes
        // to a string by default, and `rescale` is what fixes the digit count.
        let total = line_total(2, d("2.50")).expect("valid line");
        assert_eq!(
            serde_json::to_string(&total).expect("serialize"),
            "\"5.00\""
        );

        // A price that arrived with a wider scale still leaves as two digits.
        let total = line_total(3, d("25.000000")).expect("valid line");
        assert_eq!(
            serde_json::to_string(&total).expect("serialize"),
            "\"75.00\""
        );

        // Zero is `0.00`, not `0` and not `0.000000`.
        let total = line_total(3, d("0")).expect("valid line");
        assert_eq!(
            serde_json::to_string(&total).expect("serialize"),
            "\"0.00\""
        );
    }

    #[test]
    fn serialized_amounts_are_uniformly_shaped() {
        // The regression this guards: `Decimal::ZERO` has scale 0, so without
        // `serialize_amount` a zero amount leaves the API as `"0"` while every
        // other amount leaves as `"25.00"`. One shape, always.
        #[derive(serde::Serialize)]
        struct Wrapper {
            #[serde(serialize_with = "serialize_amount")]
            amount: Amount,
        }

        let cases = [
            (d("0"), "0.00"),
            (d("0.000000"), "0.00"),
            (d("25"), "25.00"),
            (d("25.5"), "25.50"),
            (d("2.50"), "2.50"),
            (d("-7200"), "-7200.00"),
        ];
        for (amount, expected) in cases {
            let json = serde_json::to_value(Wrapper { amount }).expect("serialize");
            assert_eq!(
                json["amount"], expected,
                "an amount must always leave with two decimals"
            );
        }
    }
}
