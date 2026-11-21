//! Money and quantities as exact decimals. Floating point never touches a price (REAS-RUST-001).

use std::fmt;
use std::str::FromStr;

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;
use thiserror::Error;

/// Why a value is not a dollar amount, or why arithmetic on two amounts has no answer.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MoneyError {
    /// The text is not a dollar amount, or the value has more than two decimal places. Holds
    /// the text as it was given, or the stored number as text.
    #[error("cannot read {0:?} as a dollar amount")]
    Parse(String),
    /// The result of an addition, a subtraction or a multiplication is outside the range of
    /// the decimal type. No amount on a receipt is near that range.
    #[error("a dollar amount overflowed")]
    Overflow,
}

/// Why a value is not a quantity.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum QuantityError {
    /// The text is not a decimal number. Holds the text as it was given.
    #[error("cannot read {0:?} as a quantity")]
    Parse(String),
    /// The number is zero or negative. Holds the number.
    #[error("a quantity is positive, not {0}")]
    NotPositive(Decimal),
    /// The number is written with more than three decimal places; a trailing zero counts as a
    /// place. Holds the number.
    #[error("a quantity has at most three decimals, not {0}")]
    TooPrecise(Decimal),
}

/// A dollar amount with two decimal places. Negative amounts exist: credits and discounts.
/// Serializes as the decimal text, never as a float.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize)]
#[serde(transparent)]
pub struct Money(Decimal);

impl Money {
    /// No money: the start of a sum of line prices.
    pub const ZERO: Money = Money(Decimal::ZERO);

    /// An amount from a count of whole cents: `1737` is `$17.37`, `-200` is a credit of `$2.00`.
    pub fn from_cents(cents: i64) -> Self {
        Self(Decimal::new(cents, 2))
    }

    /// A stored `numeric(10,2)` read back; more than two decimals is a corrupt row.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Parse`] when `value` has more than two decimal places.
    pub fn from_decimal(value: Decimal) -> Result<Self, MoneyError> {
        if value.scale() > 2 {
            return Err(MoneyError::Parse(value.to_string()));
        }
        let mut value = value;
        value.rescale(2);
        Ok(Self(value))
    }

    /// Addition that cannot fail for any sum a receipt reaches; saturates at the type's bound
    /// instead of panicking, so a fold over lines stays total.
    pub fn saturating_add(self, other: Money) -> Money {
        Money(self.0.saturating_add(other.0))
    }

    /// Reads what a receipt prints: `$17.37`, `-$2.00`, `($2.00)`, `$1,234.56`, `5`.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Parse`] when the text, without its one minus sign or pair of parentheses,
    /// its dollar signs and its commas, does not read as a number, has more than two decimal
    /// places, or still carries a minus sign. The decimal library reads more forms than a
    /// receipt prints, such as `.5`, `+5` and `1e2`.
    pub fn parse(raw: &str) -> Result<Self, MoneyError> {
        let text = raw.trim();
        let (negative, text) = if let Some(inner) = text
            .strip_prefix('(')
            .and_then(|rest| rest.strip_suffix(')'))
        {
            (true, inner)
        } else if let Some(rest) = text.strip_prefix('-') {
            (true, rest)
        } else {
            (false, text)
        };
        let digits = text.trim().trim_start_matches('$').trim().replace(',', "");
        let mut value =
            Decimal::from_str(&digits).map_err(|_| MoneyError::Parse(raw.to_owned()))?;
        if value.scale() > 2 || value.is_sign_negative() {
            return Err(MoneyError::Parse(raw.to_owned()));
        }
        value.rescale(2);
        // The value is not negative here, so this sets the sign that the text gave.
        value.set_sign_negative(negative);
        Ok(Self(value))
    }

    /// The sum of two amounts, for a caller that must know when the sum has no answer.
    /// [`Money::saturating_add`] is the form for a fold over the lines of a receipt.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] when the sum is outside the range of the decimal type.
    pub fn checked_add(self, other: Money) -> Result<Money, MoneyError> {
        self.0
            .checked_add(other.0)
            .map(Money)
            .ok_or(MoneyError::Overflow)
    }

    /// This amount minus `other`. The result is negative when `other` is the larger amount.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] when the difference is outside the range of the decimal type.
    pub fn checked_sub(self, other: Money) -> Result<Money, MoneyError> {
        self.0
            .checked_sub(other.0)
            .map(Money)
            .ok_or(MoneyError::Overflow)
    }

    /// Unit price times quantity, rounded half away from zero to the cent, as a receipt does.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] when the product is outside the range of the decimal type.
    pub fn times(self, quantity: Quantity) -> Result<Money, MoneyError> {
        self.0
            .checked_mul(quantity.0)
            .map(|product| {
                Money(product.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero))
            })
            .ok_or(MoneyError::Overflow)
    }

    /// The size of the gap between two amounts, for reconciliation tolerances. Two amounts
    /// at opposite ends of the type's range give the largest amount, not a panic: such a gap
    /// is outside every tolerance, which is the right answer.
    pub fn abs_diff(self, other: Money) -> Money {
        Money(self.0.saturating_sub(other.0).abs())
    }

    /// Whether the amount is below zero: a credit or a discount. Zero is never negative, also
    /// when the text it was read from had a minus sign or parentheses.
    pub fn is_negative(self) -> bool {
        self.0.is_sign_negative() && !self.0.is_zero()
    }

    /// The amount in dollars as an exact decimal, to bind to a `numeric(10,2)` column.
    pub fn as_decimal(self) -> Decimal {
        self.0
    }

    /// Whole cents; `None` only past the range of an `i64`, which no receipt reaches.
    pub fn cents(self) -> Option<i64> {
        self.0.checked_mul(Decimal::ONE_HUNDRED)?.to_i64()
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_negative() {
            write!(f, "-${:.2}", self.0.abs())
        } else {
            write!(f, "${:.2}", self.0)
        }
    }
}

/// A count or a weight: positive, at most three decimals, as the receipt prints them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Quantity(Decimal);

impl Quantity {
    /// A quantity from a number, such as a stored `numeric(10,3)` read back. Trailing zeros
    /// are dropped, so `1.560` and `1.56` are the same quantity and print the same.
    ///
    /// # Errors
    ///
    /// - [`QuantityError::NotPositive`] when `value` is zero or negative.
    /// - [`QuantityError::TooPrecise`] when `value` has more than three decimal places.
    pub fn new(value: Decimal) -> Result<Self, QuantityError> {
        if value <= Decimal::ZERO {
            return Err(QuantityError::NotPositive(value));
        }
        if value.scale() > 3 {
            return Err(QuantityError::TooPrecise(value));
        }
        Ok(Self(value.normalize()))
    }

    /// Reads `3`, `1.56` or `0.5`.
    ///
    /// # Errors
    ///
    /// - [`QuantityError::Parse`] when the text without its surrounding whitespace does not
    ///   read as a number. The decimal library also reads forms such as `.5` and `1e2`.
    /// - [`QuantityError::NotPositive`] when the number is zero or negative.
    /// - [`QuantityError::TooPrecise`] when the number has more than three decimal places.
    pub fn parse(raw: &str) -> Result<Self, QuantityError> {
        let value =
            Decimal::from_str(raw.trim()).map_err(|_| QuantityError::Parse(raw.to_owned()))?;
        Self::new(value)
    }

    /// The number without trailing zeros, to bind to a `numeric(10,3)` column or to compute
    /// with. It carries no unit: the receipt says whether it is a count of pieces or a weight.
    pub fn as_decimal(self) -> Decimal {
        self.0
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_reads_the_formats_a_receipt_prints() {
        assert_eq!(Money::parse("$17.37"), Ok(Money::from_cents(1737)));
        assert_eq!(Money::parse("5"), Ok(Money::from_cents(500)));
        assert_eq!(Money::parse("$1,234.56"), Ok(Money::from_cents(123_456)));
        assert_eq!(Money::parse("-$2.00"), Ok(Money::from_cents(-200)));
        assert_eq!(Money::parse("($2.00)"), Ok(Money::from_cents(-200)));
        assert_eq!(Money::parse("$ 3.17"), Ok(Money::from_cents(317)));
        assert_eq!(
            Money::parse("$1.234"),
            Err(MoneyError::Parse("$1.234".to_owned()))
        );
        assert!(Money::parse("abc").is_err());
        assert!(Money::parse("").is_err());
    }

    #[test]
    fn money_displays_with_two_decimals_and_a_leading_sign() {
        assert_eq!(Money::from_cents(500).to_string(), "$5.00");
        assert_eq!(Money::from_cents(-200).to_string(), "-$2.00");
        assert_eq!(Money::ZERO.to_string(), "$0.00");
    }

    #[test]
    fn a_price_times_a_weight_rounds_to_the_cent_like_the_receipt() {
        let per_pound = Money::from_cents(299);
        let weight = Quantity::parse("1.06").expect("valid");
        assert_eq!(per_pound.times(weight), Ok(Money::from_cents(317)));
        let each = Money::from_cents(579);
        assert_eq!(
            each.times(Quantity::parse("3").expect("valid")),
            Ok(Money::from_cents(1737))
        );
        assert_eq!(
            Money::from_cents(1737).abs_diff(Money::from_cents(1736)),
            Money::from_cents(1)
        );
        assert_eq!(Money::from_cents(1737).cents(), Some(1737));
    }

    #[test]
    fn a_quantity_is_positive_with_at_most_three_decimals() {
        assert_eq!(
            Quantity::parse("1.560").map(|q| q.to_string()),
            Ok("1.56".to_owned())
        );
        assert!(Quantity::parse("0").is_err());
        assert!(Quantity::parse("-1").is_err());
        assert!(Quantity::parse("1.2345").is_err());
        assert!(Quantity::parse("two").is_err());
    }

    #[test]
    fn a_bracket_or_a_sign_at_the_edge_of_a_text_is_an_error_or_an_amount() {
        for text in ["()", "(-", "-", "$", "( )", "(($1.00))", "(\u{e9})"] {
            assert!(Money::parse(text).is_err(), "{text}");
        }
        assert_eq!(Money::parse("( $2.00 )"), Ok(Money::from_cents(-200)));
        assert_eq!(Money::parse("($0.00)"), Ok(Money::ZERO));
        assert!(!Money::parse("($0.00)").expect("zero").is_negative());
        assert!(!Money::parse("-0").expect("zero").is_negative());
        assert_eq!(
            Money::from_cents(-200).abs_diff(Money::from_cents(300)),
            Money::from_cents(500)
        );
        assert_eq!(
            Money::parse("92233720368547758.07")
                .expect("amount")
                .cents(),
            Some(i64::MAX)
        );
        assert_eq!(
            Money::parse("92233720368547758.08")
                .expect("amount")
                .cents(),
            None
        );
    }

    #[test]
    fn a_text_of_one_bracket_is_an_error_like_any_text_that_is_no_amount() {
        // The reader takes the brackets off with `strip_prefix` and `strip_suffix`, so no
        // position is computed for a text of one character.
        for text in ["(", ")", ")(", "(\u{e9}", "\u{e9})"] {
            assert!(Money::parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn amounts_at_the_ends_of_the_range_give_an_answer_and_no_panic() {
        let most = Money::parse("79228162514264337593543950335").expect("the largest decimal");
        let least = Money::parse("-79228162514264337593543950335").expect("the smallest decimal");
        // The gap is outside the range of the type: the largest amount stands for it.
        assert_eq!(most.abs_diff(least), most);
        assert_eq!(least.abs_diff(most), most);
        assert_eq!(most.abs_diff(most), Money::ZERO);
        // In cents the amount is outside the range of a decimal.
        assert_eq!(most.cents(), None);
        assert_eq!(least.cents(), None);
    }
}
