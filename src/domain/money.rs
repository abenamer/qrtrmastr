//! Money and quantities as exact decimals. Floating point never touches a price
//! (REAS-RUST-008).
//!
//! An amount and a quantity are bounded by what their columns hold: `numeric(10,2)` and
//! `numeric(10,3)`. Inside those bounds the decimal type holds each sum and each product
//! exactly, so the one rounding of an amount is the one that [`Money::times`] names. Outside
//! them the store would refuse the row, and from about 26 digits the decimal library rounds
//! on its own, to the even digit; a value outside a bound is therefore no value of these
//! types.

use std::fmt;
use std::str::FromStr;

use rust_decimal::{Decimal, RoundingStrategy};
use thiserror::Error;

/// Why a value is not an amount of money, or why arithmetic on two amounts has no answer.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MoneyError {
    /// The text is not an amount, or the value has more than two decimal places or is
    /// outside the range of an amount. Holds the text as it was given, or the number as text.
    #[error(
        "cannot read {0:?} as an amount of money of two decimal places and at most 99,999,999.99"
    )]
    Parse(String),
    /// The result of an addition, a subtraction or a multiplication is outside the range of
    /// an amount: above 99,999,999.99 or below its negative.
    #[error("an amount of money is outside the range of 99,999,999.99")]
    Overflow,
    /// Two amounts in different currencies were added, subtracted or compared. One dollar and
    /// one euro have no sum.
    #[error("an amount in {left} and an amount in {right} have no sum and no order")]
    CurrencyMismatch {
        /// The currency of the amount on the left of the operation.
        left: Currency,
        /// The currency of the amount on the right.
        right: Currency,
    },
    /// The text is not the code of a currency: three capital ASCII letters. Holds the text.
    #[error("{0:?} is not a currency code of three capital letters, such as USD")]
    Currency(String),
}

/// A currency, by its ISO 4217 code of three capital letters. Every receipt that the tool reads
/// today is in US dollars; the type is in [`Money`] so that an amount in another currency
/// cannot be added to one in dollars by mistake (REAS-RUST-008).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Currency([u8; 3]);

impl fmt::Debug for Currency {
    /// The code in a wrapper, `Currency(USD)`, not the three bytes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Currency({})", self.code())
    }
}

impl Currency {
    /// The US dollar: the currency of each amount that a parser or the store gives.
    pub const USD: Currency = Currency(*b"USD");

    /// Reads a currency code. The code is checked for its form only, not against the list of
    /// the currencies that exist.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Currency`] when the text is not exactly three capital ASCII letters.
    pub fn parse(code: &str) -> Result<Self, MoneyError> {
        match <[u8; 3]>::try_from(code.as_bytes()) {
            Ok(letters) if letters.iter().all(u8::is_ascii_uppercase) => Ok(Self(letters)),
            _ => Err(MoneyError::Currency(code.to_owned())),
        }
    }

    /// The three letters of the code.
    pub fn code(&self) -> &str {
        // The letters are ASCII, which the constructor checks, so they are always text.
        std::str::from_utf8(&self.0).unwrap_or("???")
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
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
    /// The number is 10,000,000 or more: more than a `numeric(10,3)` column holds, and more
    /// than a receipt prints. Holds the number.
    #[error("a quantity is below 10,000,000, not {0}")]
    TooLarge(Decimal),
}

/// The largest amount in cents: `99,999,999.99`, the largest number of a `numeric(10,2)`
/// column.
const LARGEST_IN_CENTS: i64 = 9_999_999_999;

/// Whether a number is in the range of an amount, on either side of zero.
fn in_range(amount: Decimal) -> bool {
    amount.abs() <= Decimal::new(LARGEST_IN_CENTS, 2)
}

/// An amount of money in one currency: at most two decimal places, and at most 99,999,999.99
/// on either side of zero, which is what a `numeric(10,2)` column holds. Negative amounts
/// exist: credits and discounts. The type has no serialization of its own: the wire types of
/// the command line write an amount as decimal text, never as a float.
///
/// Two amounts are equal when the number and the currency are equal. The type has no order
/// of its own, because two amounts in different currencies have none: use
/// [`Money::checked_cmp`]. Addition and subtraction are checked, and give an error for a
/// result outside the range and for two currencies (REAS-RUST-008).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Money {
    amount: Decimal,
    currency: Currency,
}

impl fmt::Debug for Money {
    /// The number as it is held, then the currency: `Money(17.37 USD)`. A snapshot of a
    /// receipt holds this form.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Money")
            .field(&format_args!("{} {}", self.amount, self.currency))
            .finish()
    }
}

impl Default for Money {
    /// No money, in US dollars.
    fn default() -> Self {
        Self::ZERO
    }
}

impl Money {
    /// No money, in US dollars: the start of a sum of line prices of a receipt in dollars.
    pub const ZERO: Money = Money::zero(Currency::USD);

    /// No money in `currency`: the start of a sum of amounts in that currency.
    pub const fn zero(currency: Currency) -> Self {
        Self {
            amount: Decimal::ZERO,
            currency,
        }
    }

    /// An amount in US dollars from a count of whole cents: `1737` is `$17.37`, `-200` is a
    /// credit of `$2.00`. The count is an `i32`, so each value is in the range of an amount.
    pub fn from_cents(cents: i32) -> Self {
        Self {
            amount: Decimal::new(i64::from(cents), 2),
            currency: Currency::USD,
        }
    }

    /// An amount in `currency`. The number gets two decimal places.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Parse`] when `amount` has more than two decimal places, or is above
    /// 99,999,999.99 or below its negative.
    pub fn new(amount: Decimal, currency: Currency) -> Result<Self, MoneyError> {
        if amount.scale() > 2 || !in_range(amount) {
            return Err(MoneyError::Parse(amount.to_string()));
        }
        let mut amount = amount;
        // The scale is at most 2 here, so this adds zeros and removes no digit.
        amount.rescale(2);
        Ok(Self { amount, currency })
    }

    /// A stored `numeric(10,2)` read back, in US dollars: the store has no column for the
    /// currency, because every stored amount is from a receipt in dollars.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Parse`] when `value` has more than two decimal places or is outside the
    /// range of an amount; either is a corrupt row.
    pub fn from_decimal(value: Decimal) -> Result<Self, MoneyError> {
        Self::new(value, Currency::USD)
    }

    /// The currency of the amount.
    pub fn currency(self) -> Currency {
        self.currency
    }

    /// Reads what a receipt prints, as an amount in US dollars: `$17.37`, `-$2.00`,
    /// `($2.00)`, `$1,234.56`, `5`.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Parse`] when the text, without its one minus sign or pair of parentheses,
    /// its dollar signs and its commas, does not read as a number, has more than two decimal
    /// places, still carries a minus sign, or is above 99,999,999.99. The decimal library
    /// reads more forms than a receipt prints, such as `.5`, `+5` and `1e2`. The sign of
    /// another currency is no part of a number, so `€5` is an error too.
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
        if value.scale() > 2 || value.is_sign_negative() || !in_range(value) {
            return Err(MoneyError::Parse(raw.to_owned()));
        }
        // The scale is at most 2 here, so this adds zeros and removes no digit.
        value.rescale(2);
        // The value is not negative here, so this sets the sign that the text gave.
        value.set_sign_negative(negative);
        Ok(Self {
            amount: value,
            currency: Currency::USD,
        })
    }

    /// The currency of both amounts, or the error for two currencies.
    fn same_currency(self, other: Money) -> Result<Currency, MoneyError> {
        if self.currency == other.currency {
            Ok(self.currency)
        } else {
            Err(MoneyError::CurrencyMismatch {
                left: self.currency,
                right: other.currency,
            })
        }
    }

    /// An amount from the result of an operation, when the result is in the range.
    fn in_range_or_overflow(
        amount: Option<Decimal>,
        currency: Currency,
    ) -> Result<Money, MoneyError> {
        match amount {
            Some(amount) if in_range(amount) => Ok(Money { amount, currency }),
            _ => Err(MoneyError::Overflow),
        }
    }

    /// The sum of two amounts. Both are in the range of an amount, so the decimal type holds
    /// the sum exactly; nothing is rounded.
    ///
    /// # Errors
    ///
    /// - [`MoneyError::CurrencyMismatch`] when the two amounts are in different currencies.
    /// - [`MoneyError::Overflow`] when the sum is outside the range of an amount.
    pub fn checked_add(self, other: Money) -> Result<Money, MoneyError> {
        let currency = self.same_currency(other)?;
        Self::in_range_or_overflow(self.amount.checked_add(other.amount), currency)
    }

    /// This amount minus `other`, exactly. The result is negative when `other` is the larger
    /// amount.
    ///
    /// # Errors
    ///
    /// - [`MoneyError::CurrencyMismatch`] when the two amounts are in different currencies.
    /// - [`MoneyError::Overflow`] when the difference is outside the range of an amount.
    pub fn checked_sub(self, other: Money) -> Result<Money, MoneyError> {
        let currency = self.same_currency(other)?;
        Self::in_range_or_overflow(self.amount.checked_sub(other.amount), currency)
    }

    /// Unit price times quantity, in the currency of the price. This is the one place where
    /// an amount is rounded: to the cent, half away from zero
    /// ([`RoundingStrategy::MidpointAwayFromZero`]), as a receipt does. Half a cent above
    /// zero becomes one cent, and half a cent below zero becomes one cent below. The product
    /// of an amount and a quantity has at most five decimal places and 20 digits, which the
    /// decimal type holds exactly, so no other rounding comes before this one.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] when the rounded product is outside the range of an amount.
    pub fn times(self, quantity: Quantity) -> Result<Money, MoneyError> {
        let product = self.amount.checked_mul(quantity.0).map(|product| {
            product.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
        });
        Self::in_range_or_overflow(product, self.currency)
    }

    /// The size of the gap between two amounts, never negative, for the tolerance of the
    /// reconciliation.
    ///
    /// # Errors
    ///
    /// - [`MoneyError::CurrencyMismatch`] when the two amounts are in different currencies.
    /// - [`MoneyError::Overflow`] when the gap is outside the range of an amount, as for the
    ///   largest amount and a credit.
    pub fn distance(self, other: Money) -> Result<Money, MoneyError> {
        let difference = self.checked_sub(other)?;
        Ok(Money {
            amount: difference.amount.abs(),
            currency: difference.currency,
        })
    }

    /// Which of two amounts is the larger.
    ///
    /// # Errors
    ///
    /// [`MoneyError::CurrencyMismatch`] when the two amounts are in different currencies.
    pub fn checked_cmp(self, other: Money) -> Result<std::cmp::Ordering, MoneyError> {
        self.same_currency(other)?;
        Ok(self.amount.cmp(&other.amount))
    }

    /// Whether the amount is below zero: a credit or a discount. Zero is never negative, also
    /// when the text it was read from had a minus sign or parentheses.
    pub fn is_negative(self) -> bool {
        self.amount.is_sign_negative() && !self.amount.is_zero()
    }

    /// The number of the amount as an exact decimal, without its currency, to bind to a
    /// `numeric(10,2)` column or to write as decimal text.
    pub fn as_decimal(self) -> Decimal {
        self.amount
    }
}

impl fmt::Display for Money {
    /// A dollar amount as a receipt prints it, `$17.37` and `-$2.00`. An amount in another
    /// currency with its code before the number, `EUR 17.37`. Zero has no sign, also when
    /// the text that it was read from had one.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.is_negative() { "-" } else { "" };
        let number = self.amount.abs();
        if self.currency == Currency::USD {
            write!(f, "{sign}${number:.2}")
        } else {
            write!(f, "{} {sign}{number:.2}", self.currency)
        }
    }
}

/// A count or a weight: positive, at most three decimals, below 10,000,000, as the receipt
/// prints them and as a `numeric(10,3)` column holds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Quantity(Decimal);

impl Quantity {
    /// A quantity from a number, such as a stored `numeric(10,3)` read back. Trailing zeros
    /// are dropped, so `1.560` and `1.56` are the same quantity and print the same.
    ///
    /// # Errors
    ///
    /// - [`QuantityError::NotPositive`] when `value` is zero or negative.
    /// - [`QuantityError::TooPrecise`] when `value` has more than three decimal places.
    /// - [`QuantityError::TooLarge`] when `value` is 10,000,000 or more.
    pub fn new(value: Decimal) -> Result<Self, QuantityError> {
        if value <= Decimal::ZERO {
            return Err(QuantityError::NotPositive(value));
        }
        if value.scale() > 3 {
            return Err(QuantityError::TooPrecise(value));
        }
        if value >= Decimal::from(10_000_000) {
            return Err(QuantityError::TooLarge(value));
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
    /// - [`QuantityError::TooLarge`] when the number is 10,000,000 or more.
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
    /// The number. A width and an alignment of the caller apply, so that a table can put the
    /// number under its column title.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
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
            Money::from_cents(1737).distance(Money::from_cents(1736)),
            Ok(Money::from_cents(1))
        );
        assert_eq!(
            Money::from_cents(1736).distance(Money::from_cents(1737)),
            Ok(Money::from_cents(1))
        );
    }

    #[test]
    fn half_a_cent_rounds_away_from_zero_and_less_than_half_rounds_down() {
        let quantity = |text: &str| Quantity::parse(text).expect("a quantity");
        let dollars = |text: &str| Money::parse(text).expect("an amount");
        // One cent times a half is half a cent: the boundary. It becomes one cent.
        assert_eq!(dollars("0.01").times(quantity("0.5")), Ok(dollars("0.01")));
        // 0.03 times 0.5 is 0.015: up to two cents, not down to the even cent.
        assert_eq!(dollars("0.03").times(quantity("0.5")), Ok(dollars("0.02")));
        // 0.05 times 0.5 is 0.025: three cents. A rounding to the even cent would give two.
        assert_eq!(dollars("0.05").times(quantity("0.5")), Ok(dollars("0.03")));
        // Just below and just above the half.
        assert_eq!(
            dollars("0.01").times(quantity("0.499")),
            Ok(dollars("0.00"))
        );
        assert_eq!(
            dollars("0.01").times(quantity("0.501")),
            Ok(dollars("0.01"))
        );
        assert_eq!(
            dollars("1.00").times(quantity("0.004")),
            Ok(dollars("0.00"))
        );
        assert_eq!(
            dollars("1.00").times(quantity("0.005")),
            Ok(dollars("0.01"))
        );
        assert_eq!(
            dollars("1.00").times(quantity("0.006")),
            Ok(dollars("0.01"))
        );
        // A credit rounds away from zero too: half a cent below zero is one cent below.
        assert_eq!(
            dollars("-0.01").times(quantity("0.5")),
            Ok(dollars("-0.01"))
        );
        assert_eq!(
            dollars("-0.05").times(quantity("0.5")),
            Ok(dollars("-0.03"))
        );
        assert_eq!(dollars("-0.01").times(quantity("0.499")), Ok(Money::ZERO));
    }

    #[test]
    fn a_sum_a_difference_and_a_product_outside_the_range_are_errors() {
        // The largest amount is what a `numeric(10,2)` column holds.
        let most = Money::parse("$99,999,999.99").expect("the largest amount");
        let least = Money::parse("-$99,999,999.99").expect("the largest credit");
        let cent = Money::from_cents(1);
        assert_eq!(most.checked_add(cent), Err(MoneyError::Overflow));
        assert_eq!(most.checked_add(most), Err(MoneyError::Overflow));
        assert_eq!(least.checked_sub(cent), Err(MoneyError::Overflow));
        assert_eq!(least.checked_sub(most), Err(MoneyError::Overflow));
        assert_eq!(most.distance(least), Err(MoneyError::Overflow));
        assert_eq!(least.distance(most), Err(MoneyError::Overflow));
        let quantity = |text: &str| Quantity::parse(text).expect("a quantity");
        assert_eq!(most.times(quantity("2")), Err(MoneyError::Overflow));
        assert_eq!(most.times(quantity("1.001")), Err(MoneyError::Overflow));
        // The product of the largest amount and the largest quantity is held by the decimal
        // type and is outside the range of an amount: an error, and no rounding on the way.
        assert_eq!(
            most.times(quantity("9999999.999")),
            Err(MoneyError::Overflow)
        );
        // The product is rounded first and compared with the range after it: 99,999,999.9919
        // is above the largest amount, and it rounds to the largest amount.
        assert_eq!(
            Money::parse("99700897.30")
                .expect("an amount")
                .times(quantity("1.003")),
            Ok(most)
        );
        // The control: at the bound, and one step inside it, the arithmetic has its answer.
        assert_eq!(most.checked_add(Money::ZERO), Ok(most));
        assert_eq!(most.checked_sub(cent), Money::parse("99999999.98"));
        assert_eq!(most.checked_sub(most), Ok(Money::ZERO));
        assert_eq!(most.checked_add(least), Ok(Money::ZERO));
        assert_eq!(most.distance(most), Ok(Money::ZERO));
        assert_eq!(most.distance(Money::ZERO), Ok(most));
        assert_eq!(least.distance(Money::ZERO), Ok(most));
        assert_eq!(most.times(quantity("1")), Ok(most));
        assert_eq!(cent.checked_add(cent), Ok(Money::from_cents(2)));
        assert_eq!(
            Money::parse("49999999.99")
                .expect("half")
                .times(quantity("2")),
            Money::parse("99999999.98")
        );
    }

    #[test]
    fn an_amount_or_a_quantity_outside_what_its_column_holds_is_no_value() {
        // One cent above the largest amount, on each way into the type.
        for text in [
            "100000000.00",
            "$100,000,000",
            "-100000000.00",
            "79228162514264337593543950335",
        ] {
            assert_eq!(
                Money::parse(text),
                Err(MoneyError::Parse(text.to_owned())),
                "{text}"
            );
        }
        let above = Decimal::new(10_000_000_000, 2);
        assert_eq!(
            Money::new(above, Currency::USD),
            Err(MoneyError::Parse("100000000.00".to_owned()))
        );
        assert_eq!(
            Money::from_decimal(-above),
            Err(MoneyError::Parse("-100000000.00".to_owned()))
        );
        // The control: the largest amount is an amount.
        let most = Decimal::new(9_999_999_999, 2);
        assert_eq!(Money::from_decimal(most).map(Money::as_decimal), Ok(most));
        assert_eq!(Money::from_decimal(-most).map(Money::as_decimal), Ok(-most));
        assert_eq!(
            Money::from_cents(i32::MAX).as_decimal(),
            Decimal::new(2_147_483_647, 2)
        );
        assert_eq!(
            Money::from_cents(i32::MIN).as_decimal(),
            Decimal::new(-2_147_483_648, 2)
        );
        // A quantity: below ten million.
        assert!(Quantity::parse("9999999.999").is_ok());
        let ten_million = Decimal::from(10_000_000);
        assert_eq!(
            Quantity::parse("10000000"),
            Err(QuantityError::TooLarge(ten_million))
        );
        assert_eq!(
            Quantity::new(ten_million),
            Err(QuantityError::TooLarge(ten_million))
        );
    }

    #[test]
    fn a_quantity_takes_the_width_of_its_column() {
        // Before 2026-11-21 the width was lost, and the quantity column of the tables of
        // `qm due` was not under its title.
        let two = Quantity::parse("2").expect("a quantity");
        assert_eq!(format!("{two}"), "2");
        assert_eq!(format!("{two:>5}"), "    2");
        assert_eq!(format!("{two:<5}|"), "2    |");
        assert_eq!(
            format!("{:>7}", Quantity::parse("1.56").expect("a quantity")),
            "   1.56"
        );
    }

    #[test]
    fn zero_has_no_sign_however_it_was_written() {
        for text in ["-0", "-$0.00", "($0.00)", "0", "$0.00"] {
            let zero = Money::parse(text).expect("zero");
            assert_eq!(zero.to_string(), "$0.00", "{text}");
            assert!(!zero.is_negative(), "{text}");
            assert_eq!(zero, Money::ZERO, "{text}");
        }
    }

    #[test]
    fn two_currencies_have_no_sum_no_difference_and_no_order() {
        let euro = Currency::parse("EUR").expect("a code");
        let dollars = Money::from_cents(500);
        let euros = Money::new(Decimal::new(500, 2), euro).expect("an amount");
        let mismatch = Err(MoneyError::CurrencyMismatch {
            left: Currency::USD,
            right: euro,
        });
        assert_eq!(dollars.checked_add(euros), mismatch);
        assert_eq!(dollars.checked_sub(euros), mismatch);
        assert_eq!(dollars.distance(euros), mismatch);
        assert_eq!(
            dollars.checked_cmp(euros),
            Err(MoneyError::CurrencyMismatch {
                left: Currency::USD,
                right: euro,
            })
        );
        assert_eq!(
            euros.checked_add(dollars),
            Err(MoneyError::CurrencyMismatch {
                left: euro,
                right: Currency::USD
            })
        );
        // The same number in two currencies is two amounts.
        assert_ne!(dollars, euros);
        // The control: in one currency each operation has its answer, also in euros.
        assert_eq!(
            euros.checked_add(euros).map(Money::as_decimal),
            Ok(Decimal::new(1000, 2))
        );
        assert_eq!(euros.checked_add(euros).map(Money::currency), Ok(euro));
        assert_eq!(
            dollars.checked_cmp(Money::from_cents(499)),
            Ok(std::cmp::Ordering::Greater)
        );
        assert_eq!(dollars.checked_cmp(dollars), Ok(std::cmp::Ordering::Equal));
        assert_eq!(
            Money::ZERO.checked_cmp(dollars),
            Ok(std::cmp::Ordering::Less)
        );
        assert_eq!(Money::zero(euro).checked_add(euros), Ok(euros));
        // A product keeps the currency of the price.
        let twice = euros
            .times(Quantity::parse("2").expect("a quantity"))
            .expect("a product");
        assert_eq!(
            (twice.as_decimal(), twice.currency()),
            (Decimal::new(1000, 2), euro)
        );
        assert_eq!(euros.to_string(), "EUR 5.00");
        assert_eq!(
            Money::new(Decimal::new(-250, 2), euro)
                .expect("an amount")
                .to_string(),
            "EUR -2.50"
        );
    }

    #[test]
    fn a_currency_code_is_three_capital_letters() {
        // The debug form is the code in a wrapper, as the snapshot of a parsed receipt has it.
        assert_eq!(format!("{:?}", Currency::USD), "Currency(USD)");
        assert_eq!(Currency::parse("USD"), Ok(Currency::USD));
        assert_eq!(
            Currency::parse("EUR").map(|c| c.to_string()),
            Ok("EUR".to_owned())
        );
        for bad in [
            "", "US", "USDX", "usd", "Usd", "U$D", "US1", "ÜSD", " USD", "US ",
        ] {
            assert_eq!(
                Currency::parse(bad),
                Err(MoneyError::Currency(bad.to_owned())),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn an_amount_with_more_than_two_decimals_is_no_amount_in_any_currency() {
        let third = Decimal::new(1001, 3);
        assert_eq!(
            Money::new(third, Currency::USD),
            Err(MoneyError::Parse("1.001".to_owned()))
        );
        assert_eq!(
            Money::from_decimal(third),
            Err(MoneyError::Parse("1.001".to_owned()))
        );
        // The control: one and two decimals, and none, are amounts with two decimals.
        assert_eq!(
            Money::new(Decimal::new(15, 1), Currency::USD),
            Ok(Money::from_cents(150))
        );
        assert_eq!(
            Money::from_decimal(Decimal::new(2, 0)),
            Ok(Money::from_cents(200))
        );
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
            Money::from_cents(-200).distance(Money::from_cents(300)),
            Ok(Money::from_cents(500))
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
}
