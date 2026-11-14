//! Money and quantities as exact decimals. Floating point never touches a price (REAS-RUST-001).

use std::fmt;
use std::str::FromStr;

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MoneyError {
    #[error("cannot read {0:?} as a dollar amount")]
    Parse(String),
    #[error("a dollar amount overflowed")]
    Overflow,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum QuantityError {
    #[error("cannot read {0:?} as a quantity")]
    Parse(String),
    #[error("a quantity is positive, not {0}")]
    NotPositive(Decimal),
    #[error("a quantity has at most three decimals, not {0}")]
    TooPrecise(Decimal),
}

/// A dollar amount with two decimal places. Negative amounts exist: credits and discounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Money(Decimal);

impl Money {
    pub const ZERO: Money = Money(Decimal::ZERO);

    pub fn from_cents(cents: i64) -> Self {
        Self(Decimal::new(cents, 2))
    }

    /// Reads what a receipt prints: `$17.37`, `-$2.00`, `($2.00)`, `$1,234.56`, `5`.
    pub fn parse(raw: &str) -> Result<Self, MoneyError> {
        let text = raw.trim();
        let (negative, text) = if text.starts_with('(') && text.ends_with(')') {
            (true, &text[1..text.len() - 1])
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
        Ok(Self(if negative { -value } else { value }))
    }

    pub fn checked_add(self, other: Money) -> Result<Money, MoneyError> {
        self.0
            .checked_add(other.0)
            .map(Money)
            .ok_or(MoneyError::Overflow)
    }

    pub fn checked_sub(self, other: Money) -> Result<Money, MoneyError> {
        self.0
            .checked_sub(other.0)
            .map(Money)
            .ok_or(MoneyError::Overflow)
    }

    /// Unit price times quantity, rounded half away from zero to the cent, as a receipt does.
    pub fn times(self, quantity: Quantity) -> Result<Money, MoneyError> {
        self.0
            .checked_mul(quantity.0)
            .map(|product| {
                Money(product.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero))
            })
            .ok_or(MoneyError::Overflow)
    }

    /// The size of the gap between two amounts, for reconciliation tolerances.
    pub fn abs_diff(self, other: Money) -> Money {
        Money((self.0 - other.0).abs())
    }

    pub fn is_negative(self) -> bool {
        self.0.is_sign_negative() && !self.0.is_zero()
    }

    pub fn as_decimal(self) -> Decimal {
        self.0
    }

    /// Whole cents; `None` only past the range of an `i64`, which no receipt reaches.
    pub fn cents(self) -> Option<i64> {
        (self.0 * Decimal::ONE_HUNDRED).to_i64()
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Quantity(Decimal);

impl Quantity {
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
    pub fn parse(raw: &str) -> Result<Self, QuantityError> {
        let value =
            Decimal::from_str(raw.trim()).map_err(|_| QuantityError::Parse(raw.to_owned()))?;
        Self::new(value)
    }

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
}
