//! Product identity from the text a receipt prints (REAS-ARCH-005): a name and a size text
//! become one normalized pair, and that pair is the alias that points at a product. Renamed
//! products are joined later by merging aliases; nothing here guesses.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

/// The normalized identity of one receipt name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct Normalized {
    /// The name without its size suffix: NFKC, lowercase, one space between words, no trailing
    /// punctuation, dash variants unified, `approx.` removed.
    pub name_norm: String,
    /// The size with units in one spelling: `0.5 gal`, `8 oz`, `12 ct`; empty when there is none.
    pub size_norm: String,
    /// What to show for the product the first time it is seen: the raw name without the size.
    pub display_name: String,
    /// The size as printed, without the parentheses, for display.
    pub size_text: Option<String>,
}

static SIZE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    // "Name - (1/2 gallon)", "Name (approx. 8oz)", "Name - 16 oz" at the end of the name.
    Regex::new(r"(?i)\s*(?:[-\u{2013}\u{2014}]\s*)?\((?P<size>[^()]*)\)\s*$").expect("valid regex")
});
static MULTI_SPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").expect("valid regex"));
static APPROX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(approx\.?|approximately|about)\s*").expect("valid regex"));
static DASHES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\u{2010}-\u{2015}\u{2212}]").expect("valid regex"));
static TRAILING_PUNCT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\s,.;:\-]+$").expect("valid regex"));
static HALF_GALLON: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(1/2|half)\s*(gallon|gal)\b").expect("valid regex"));
static QUANTITY_UNIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?P<n>\d+(?:\.\d+)?)\s*(?P<unit>fl\.?\s*oz\.?|fluid ounces?|ounces?|oz\.?|pounds?|lbs?\.?|gallons?|gal\.?|quarts?|qt\.?|pints?|pt\.?|liters?|litres?|l\b|ml\b|milliliters?|grams?|g\b|kg\b|kilograms?|count|ct\.?|pack|pk\.?|pieces?|pcs?\.?|each|ea\.?|dozen|doz\.?)",
    )
    .expect("valid regex")
});

fn canonical_unit(unit: &str) -> &'static str {
    let unit = unit.to_ascii_lowercase().replace('.', "");
    let unit = MULTI_SPACE.replace_all(&unit, " ");
    match unit.as_ref() {
        "fl oz" | "floz" | "fluid ounce" | "fluid ounces" => "fl oz",
        "ounce" | "ounces" | "oz" => "oz",
        "pound" | "pounds" | "lb" | "lbs" => "lb",
        "gallon" | "gallons" | "gal" => "gal",
        "quart" | "quarts" | "qt" => "qt",
        "pint" | "pints" | "pt" => "pt",
        "liter" | "liters" | "litre" | "litres" | "l" => "l",
        "ml" | "milliliter" | "milliliters" => "ml",
        "gram" | "grams" | "g" => "g",
        "kg" | "kilogram" | "kilograms" => "kg",
        "count" | "ct" => "ct",
        "pack" | "pk" => "pk",
        "piece" | "pieces" | "pc" | "pcs" => "pc",
        "each" | "ea" => "ea",
        "dozen" | "doz" => "doz",
        _ => "",
    }
}

fn tidy(text: &str) -> String {
    let nfkc: String = text.nfkc().collect();
    let dashed = DASHES.replace_all(&nfkc, "-");
    let spaced = MULTI_SPACE.replace_all(dashed.trim(), " ");
    TRAILING_PUNCT.replace(&spaced, "").to_lowercase()
}

/// Normalizes a size text: `approx. 8oz` to `8 oz`, `1/2 gallon` to `0.5 gal`, `12 Count` to `12 ct`.
pub fn normalize_size(size: &str) -> String {
    let without_approx = APPROX.replace_all(size, "");
    let halves = HALF_GALLON.replace_all(&without_approx, "0.5 gal");
    let units = QUANTITY_UNIT.replace_all(&halves, |caps: &regex::Captures<'_>| {
        let unit = canonical_unit(&caps["unit"]);
        if unit.is_empty() {
            caps[0].to_owned()
        } else {
            format!("{} {}", &caps["n"], unit)
        }
    });
    tidy(&units)
}

/// Splits a receipt name into its normalized identity.
pub fn normalize(raw_name: &str) -> Normalized {
    let raw = MULTI_SPACE.replace_all(raw_name.trim(), " ").into_owned();
    let (name_part, size_text) = match SIZE_SUFFIX.captures(&raw) {
        Some(caps) => {
            let size = caps["size"].trim().to_owned();
            let name = raw[..caps.get(0).map_or(raw.len(), |m| m.start())].to_owned();
            (name, (!size.is_empty()).then_some(size))
        }
        None => (raw.clone(), None),
    };
    let display_name = TRAILING_PUNCT.replace(name_part.trim(), "").into_owned();
    let name_norm = tidy(&APPROX.replace_all(&display_name, ""));
    let size_norm = size_text.as_deref().map(normalize_size).unwrap_or_default();
    Normalized {
        name_norm,
        size_norm,
        display_name,
        size_text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(
        "Lactaid 100% Lactose-Free Whole Milk, Carton - (1/2 gallon)",
        "lactaid 100% lactose-free whole milk, carton",
        "0.5 gal",
        Some("1/2 gallon")
    )]
    #[case(
        "Ark Foods Green Shishito Peppers - (approx. 8oz)",
        "ark foods green shishito peppers",
        "8 oz",
        Some("approx. 8oz")
    )]
    #[case("Beefsteak Tomatoes, Large", "beefsteak tomatoes, large", "", None)]
    #[case(
        "  Organic   Eggs \u{2013} (12 Count) ",
        "organic eggs",
        "12 ct",
        Some("12 Count")
    )]
    #[case(
        "Sparkling Water (6 pk, 12 fl. oz.)",
        "sparkling water",
        "6 pk, 12 fl oz",
        Some("6 pk, 12 fl. oz.")
    )]
    #[case(
        "Ground Beef 85% Lean (approx. 1 lb)",
        "ground beef 85% lean",
        "1 lb",
        Some("approx. 1 lb")
    )]
    fn names_and_sizes_normalize(
        #[case] raw: &str,
        #[case] name: &str,
        #[case] size: &str,
        #[case] size_text: Option<&str>,
    ) {
        let n = normalize(raw);
        assert_eq!(n.name_norm, name);
        assert_eq!(n.size_norm, size);
        assert_eq!(n.size_text.as_deref(), size_text);
    }

    #[test]
    fn the_display_name_keeps_the_printed_case_without_the_size() {
        let n = normalize("Ark Foods Green Shishito Peppers - (approx. 8oz)");
        assert_eq!(n.display_name, "Ark Foods Green Shishito Peppers");
    }

    #[test]
    fn two_spellings_of_one_product_share_an_identity() {
        let a = normalize("Lactaid 100% Lactose-Free Whole Milk, Carton - (1/2 gallon)");
        let b = normalize("Lactaid 100% Lactose\u{2011}Free Whole Milk, Carton (Half Gallon)");
        assert_eq!((a.name_norm, a.size_norm), (b.name_norm, b.size_norm));
    }

    #[test]
    fn a_normalized_name_is_lowercase_single_spaced_and_not_empty() {
        proptest::proptest!(|(raw in "[A-Za-z0-9 ,.()%'/-]{1,60}")| {
            let n = normalize(&raw);
            proptest::prop_assert_eq!(n.name_norm.clone(), n.name_norm.to_lowercase());
            proptest::prop_assert!(!n.name_norm.contains("  "));
            proptest::prop_assert_eq!(n.name_norm.trim(), n.name_norm.as_str());
            proptest::prop_assert_eq!(n.size_norm.trim(), n.size_norm.as_str());
            // Normalizing is a pure function of its input: the same text gives the same identity.
            proptest::prop_assert_eq!(normalize(&raw), n.clone());
            let has_letters_outside_parens = raw
                .split(['(', ')'])
                .step_by(2)
                .any(|part| part.chars().any(|c| c.is_alphanumeric()));
            if has_letters_outside_parens {
                proptest::prop_assert!(!n.name_norm.is_empty(), "{:?}", raw);
            }
        });
    }
}
