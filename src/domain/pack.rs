//! How much one pack holds, read from the size text a receipt prints (REAS-ARCH-005). The
//! recommendation rules use it to compare two sizes of one product: a case of 36 is twelve
//! packs of 3. A size text that is not understood gives `None`; nothing here guesses.

use std::str::FromStr;
use std::sync::LazyLock;

use regex::Regex;

use crate::domain::pattern::fixed;
use rust_decimal::Decimal;

/// What a pack amount measures. Two amounts compare only in the same dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PackDimension {
    /// Pieces: `36ct`, `1 dozen`.
    Count,
    /// Ounces: `8oz`, `1.5lb`.
    Weight,
    /// Fluid ounces: `64fl oz`, `375ml`, `1/2 gallon`.
    Volume,
}

/// The amount in one pack, in the base unit of its dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackAmount {
    /// What `amount` measures. Two packs compare only when this is the same.
    pub dimension: PackDimension,
    /// How much one pack holds, always above zero: pieces for a count, ounces for a weight,
    /// fluid ounces for a volume. A range such as `10-12ct` gives its middle, 11.
    pub amount: Decimal,
}

/// At most six digits before the point and three after it: a size text comes from a receipt,
/// and an amount of any length would overflow the arithmetic of the rules that compare packs.
/// A longer number is not an amount this module understands.
const NUMBER: &str = r"(\d{1,6}/\d{1,6}|\d{1,6}(?:\.\d{1,3})?)";

static COUNT: LazyLock<Regex> =
    LazyLock::new(|| fixed(&format!(r"^{NUMBER}(?:\s*-\s*{NUMBER})?\s*(ct|dozen)\b")));
static MEASURE: LazyLock<Regex> = LazyLock::new(|| {
    fixed(&format!(
        r"^{NUMBER}(?:\s*-\s*{NUMBER})?\s*(fl oz|ml|l|gallon|quart|pint|oz|lbs|lb)\b"
    ))
});

fn number(raw: &str) -> Option<Decimal> {
    match raw.split_once('/') {
        Some((top, bottom)) => {
            let bottom = Decimal::from_str(bottom).ok()?;
            Decimal::from_str(top).ok()?.checked_div(bottom)
        }
        None => Decimal::from_str(raw).ok(),
    }
}

/// The first number of a match, or the middle of a range such as `10-12ct`.
fn value(captures: &regex::Captures<'_>) -> Option<Decimal> {
    let low = number(captures.get(1)?.as_str())?;
    match captures.get(2) {
        Some(high) => low
            .checked_add(number(high.as_str())?)?
            .checked_div(Decimal::TWO),
        None => Some(low),
    }
}

/// Reads the leading amount of a size text: `36ct, 0.17oz ea` is 36 pieces, `approx. 1.5lb` is
/// 24 ounces, `1/2 gallon` is 64 fluid ounces. A count comes first because a pack of several
/// states the count before the size of one piece.
pub fn pack_amount(size_text: &str) -> Option<PackAmount> {
    let text = size_text.to_lowercase().replace("approx.", "");
    let text = text.trim();
    let (dimension, amount) = if let Some(c) = COUNT.captures(text) {
        let per = if &c[3] == "dozen" {
            Decimal::from(12)
        } else {
            Decimal::ONE
        };
        (PackDimension::Count, value(&c)?.checked_mul(per)?)
    } else {
        let c = MEASURE.captures(text)?;
        let (dimension, per) = match &c[3] {
            "fl oz" => (PackDimension::Volume, Decimal::ONE),
            "ml" => (PackDimension::Volume, Decimal::new(33_814, 6)),
            "l" => (PackDimension::Volume, Decimal::new(33_814, 3)),
            "gallon" => (PackDimension::Volume, Decimal::from(128)),
            "quart" => (PackDimension::Volume, Decimal::from(32)),
            "pint" => (PackDimension::Volume, Decimal::from(16)),
            "oz" => (PackDimension::Weight, Decimal::ONE),
            _ => (PackDimension::Weight, Decimal::from(16)),
        };
        (dimension, value(&c)?.checked_mul(per)?)
    };
    (amount > Decimal::ZERO).then_some(PackAmount { dimension, amount })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("36ct, 0.17oz ea", PackDimension::Count, "36")]
    #[case("3ct, 0.17oz ea", PackDimension::Count, "3")]
    #[case("10-12ct, approx. 2lb bag", PackDimension::Count, "11")]
    #[case("1 dozen", PackDimension::Count, "12")]
    #[case("approx. 1.5lb", PackDimension::Weight, "24")]
    #[case("8oz", PackDimension::Weight, "8")]
    #[case("0.5 lb, Thin", PackDimension::Weight, "8")]
    #[case("64fl oz", PackDimension::Volume, "64")]
    #[case("375ml", PackDimension::Volume, "12.680250")]
    #[case("1 pint", PackDimension::Volume, "16")]
    #[case("1/2 gallon", PackDimension::Volume, "64")]
    #[case("1.5L", PackDimension::Volume, "50.721")]
    #[case("1 quart", PackDimension::Volume, "32")]
    fn a_size_text_gives_its_amount(
        #[case] text: &str,
        #[case] dimension: PackDimension,
        #[case] amount: &str,
    ) {
        let pack = pack_amount(text).expect("understood");
        assert_eq!(pack.dimension, dimension);
        assert_eq!(pack.amount, Decimal::from_str(amount).expect("decimal"));
    }

    #[rstest]
    #[case("")]
    #[case("Vacuum Pack")]
    #[case("Serves 2")]
    #[case("bunch")]
    #[case("0ct")]
    #[case("1/0 gallon")]
    #[case("Max 3 per customer")]
    // Numbers a receipt has no use for, which would overflow the arithmetic.
    #[case("1234567ct")]
    #[case("9999999999999999999999999999 dozen")]
    #[case("79228162514264337593543950335/1 gallon")]
    #[case("50000000000000000000000000000-50000000000000000000000000000ct")]
    #[case("1.2345 lb")]
    #[case("0/0 gallon")]
    fn a_size_text_that_is_not_an_amount_gives_none(#[case] text: &str) {
        assert_eq!(pack_amount(text), None);
    }
}
