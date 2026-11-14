//! Redaction before storage (docs/concepts/privacy.md). Four passes over a message, then an
//! assertion that is stronger than the passes: a message that still carries a configured term is
//! refused, never stored.
//!
//! 1. Structural: the smallest HTML block that carries an address, payment or contact heading is
//!    removed whole, as are images and comments; a link whose address carries a configured term
//!    loses that attribute, and every link loses its query string and fragment.
//! 2. Literal: the serialized HTML is normalized (`&nbsp;` to a space character, invisible
//!    characters removed), then every configured term, longest first, with any whitespace or
//!    tags between its words, becomes `[REDACTED:kind]`.
//! 3. Pattern: anything shaped like a phone number, an email address or a masked card number.
//! 4. Assertion: the text of the result, with tags gone and spacing and case normalized, carries
//!    no configured term, and no term's letters and digits appear in sequence.
//!
//! The terms come from the maintainer's configuration file, outside the repository.

use std::sync::LazyLock;

use regex::{Regex, RegexBuilder};
use scraper::{Html, Node, Selector};
use serde::Deserialize;
use thiserror::Error;

/// Bumped whenever a pass changes, so stored rows can be re-imported and re-redacted.
/// 2: entity and invisible-character normalization, tag-tolerant terms, independent assertion.
pub const REDACTION_VERSION: i32 = 2;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct RedactionTerms {
    /// Full names and first names as the mail greets you.
    #[serde(default)]
    pub names: Vec<String>,
    #[serde(default)]
    pub emails: Vec<String>,
    /// Phone numbers; any formatting, only the digits are used.
    #[serde(default)]
    pub phones: Vec<String>,
    /// Each line of the delivery address as the mail prints it.
    #[serde(default)]
    pub address_lines: Vec<String>,
    /// Last four digits of payment cards; redacted only in payment context.
    #[serde(default)]
    pub card_last4: Vec<String>,
}

/// Construction and assertion failures. None of them carries the term's value: an error message
/// names the kind and the position in the configured list, never the text (REAS-SEC-001).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RedactionError {
    #[error("{kind} term #{position} is too short to redact safely; use at least three characters")]
    TermTooShort { kind: &'static str, position: usize },
    #[error("{kind} term #{position} cannot be used as a pattern")]
    BadTerm { kind: &'static str, position: usize },
    #[error("phone #{0} needs at least seven digits")]
    BadPhone(usize),
    #[error("card_last4 #{0} is not exactly four digits")]
    BadLast4(usize),
    #[error("{kind} survived redaction")]
    Survived { kind: &'static str },
}

impl RedactionError {
    pub fn kind(&self) -> &'static str {
        match self {
            RedactionError::TermTooShort { kind, .. } | RedactionError::BadTerm { kind, .. } => {
                kind
            }
            RedactionError::BadPhone(_) => "phone",
            RedactionError::BadLast4(_) => "card",
            RedactionError::Survived { kind } => kind,
        }
    }
}

static HEADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(deliver(y|ed)?\s+(address|to|information|details)|ship(ping)?\s+(address|to)|payment|billing|contact\s+(info|information)|phone|tel\.?)\b")
        .expect("regex")
});
static PHONE_SHAPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\+?1[\s.-]*)?\(?[2-9][0-9]{2}\)?[\s.-]+[0-9]{3}[\s.-]+[0-9]{4}\b")
        .expect("regex")
});
static EMAIL_SHAPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").expect("regex"));
static CARD_SHAPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(ending\s+(in|with)|[xX*\u{2022}]{2,})\s*[0-9]{4}\b").expect("regex")
});
static WS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").expect("regex"));
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").expect("regex"));

/// Characters that split a word without showing: soft hyphen, zero-width space, joiner,
/// non-joiner, byte-order mark, word joiner.
fn strip_invisible(text: &str) -> String {
    text.chars()
        .filter(|c| {
            !matches!(
                c,
                '\u{ad}' | '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}' | '\u{feff}'
            )
        })
        .collect()
}

/// Serialized HTML as the text passes see it: entities for spaces become space characters, and
/// invisible characters are gone. `&nbsp;` is what html5ever writes for every U+00A0.
fn normalize_markup(html: &str) -> String {
    strip_invisible(html)
        .replace("&nbsp;", " ")
        .replace("&#160;", " ")
        .replace("&#xa0;", " ")
        .replace("&#xA0;", " ")
}

/// The text a reader would see: tags gone, entities for spaces as spaces, lowercase, one space
/// between words. The assertion runs over this, so no markup can hide a term.
fn text_view(html: &str) -> String {
    let normalized = normalize_markup(html);
    let no_tags = TAG.replace_all(&normalized, " ");
    let unescaped = no_tags
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    WS.replace_all(unescaped.trim(), " ").to_lowercase()
}

fn alphanumerics(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// A configured term as a pattern. The `regex` crate has no look-around, so the word boundary
/// is a captured prefix and suffix that the replacement puts back.
struct Literal {
    kind: &'static str,
    /// Over markup: words may be separated by whitespace or tags.
    regex: Regex,
    /// Over the text view: words separated by one space, with word boundaries.
    plain: Regex,
    /// Letters and digits only, for the sequence check; empty when too short to be specific.
    alnum: String,
    /// Characters in the term, for longest-first ordering.
    length: usize,
}

pub struct Redactor {
    literals: Vec<Literal>,
    phones: Vec<Regex>,
    last4: Vec<Regex>,
}

fn flexible_literal(term: &str) -> String {
    // Each whitespace run in the term matches any run of whitespace and tags in the markup.
    term.split_whitespace()
        .map(regex::escape)
        .collect::<Vec<_>>()
        .join(r"(?:\s|<[^>]*>)+")
}

fn bounded(core: &str) -> String {
    format!(r"(^|[^A-Za-z0-9])({core})([^A-Za-z0-9]|$)")
}

/// Replaces every bounded match of `regex`, whose groups are (prefix, term, suffix), keeping the
/// boundary characters.
fn replace_bounded(regex: &Regex, text: &str, replacement: &str) -> String {
    regex
        .replace_all(text, |caps: &regex::Captures<'_>| {
            format!("{}{replacement}{}", &caps[1], &caps[3])
        })
        .into_owned()
}

fn case_insensitive(pattern: &str) -> Option<Regex> {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .ok()
}

impl Redactor {
    pub fn new(terms: RedactionTerms) -> Result<Self, RedactionError> {
        let mut literals = Vec::new();
        for (kind, list) in [
            ("name", &terms.names),
            ("email", &terms.emails),
            ("address", &terms.address_lines),
        ] {
            for (index, term) in list.iter().enumerate() {
                let position = index + 1;
                let length = term.trim().chars().count();
                if length < 3 {
                    return Err(RedactionError::TermTooShort { kind, position });
                }
                let regex = case_insensitive(&bounded(&flexible_literal(term)))
                    .ok_or(RedactionError::BadTerm { kind, position })?;
                let plain_core = term
                    .split_whitespace()
                    .map(regex::escape)
                    .collect::<Vec<_>>()
                    .join(" ");
                let plain = case_insensitive(&bounded(&plain_core))
                    .ok_or(RedactionError::BadTerm { kind, position })?;
                let alnum = alphanumerics(term);
                literals.push(Literal {
                    kind,
                    regex,
                    plain,
                    alnum: if alnum.len() >= 8 {
                        alnum
                    } else {
                        String::new()
                    },
                    length,
                });
            }
        }
        // Longest first, so "pat.example@example.com" and "Pat Example" go before "Pat".
        literals.sort_by_key(|l| std::cmp::Reverse(l.length));

        let mut phones = Vec::new();
        for (index, phone) in terms.phones.iter().enumerate() {
            let all_digits: String = phone.chars().filter(char::is_ascii_digit).collect();
            if all_digits.len() < 7 {
                return Err(RedactionError::BadPhone(index + 1));
            }
            // A leading country code 1 is optional in the text; the national digits are required.
            let national = match all_digits.strip_prefix('1') {
                Some(rest) if rest.len() >= 10 => rest.to_owned(),
                _ => all_digits,
            };
            let spaced = national
                .chars()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join(r"[^0-9]{0,3}");
            let regex = Regex::new(&format!(r"(?:\+?1[^0-9]{{0,3}})?\(?{spaced}([^0-9]|$)"))
                .map_err(|_| RedactionError::BadPhone(index + 1))?;
            phones.push(regex);
        }
        let mut last4 = Vec::new();
        for (index, four) in terms.card_last4.iter().enumerate() {
            let digits: String = four.chars().filter(char::is_ascii_digit).collect();
            if digits.len() != 4 {
                return Err(RedactionError::BadLast4(index + 1));
            }
            let regex = Regex::new(&format!(
                r"(?i)(ending\s+(in|with)|[xX*\u{{2022}}]{{2,}})\s*{digits}([^0-9]|$)"
            ))
            .map_err(|_| RedactionError::BadLast4(index + 1))?;
            last4.push(regex);
        }
        Ok(Self {
            literals,
            phones,
            last4,
        })
    }

    /// Whether a short text, such as a message key or a link address, carries a configured term
    /// or phone number. Shapes are not checked: a message key always looks like an address.
    pub fn carries_configured_term(&self, text: &str) -> bool {
        let decoded = percent_decode(text);
        let view = text_view(&decoded);
        self.literals.iter().any(|l| {
            l.plain.is_match(&view)
                || (!l.alnum.is_empty() && alphanumerics(&view).contains(&l.alnum))
        }) || self.phones.iter().any(|p| p.is_match(&view))
    }

    /// Redacts HTML: structure first, then text passes over the normalized serialized result.
    pub fn redact_html(&self, html: &str) -> Result<String, RedactionError> {
        let mut document = Html::parse_document(html);
        let element = Selector::parse("td, div, p, li, section, address, span").expect("selector");

        // The smallest containers whose text carries a heading: no child container carries it too.
        let mut to_detach: Vec<ego_tree::NodeId> = Vec::new();
        for container in document.select(&element) {
            let own_text: String = container.text().collect::<Vec<_>>().join(" ");
            if !HEADING.is_match(&own_text) {
                continue;
            }
            let child_carries = container.select(&element).any(|child| {
                child.id() != container.id()
                    && HEADING.is_match(&child.text().collect::<Vec<_>>().join(" "))
            });
            if !child_carries {
                to_detach.push(container.id());
            }
        }
        let images = Selector::parse("img").expect("selector");
        to_detach.extend(document.select(&images).map(|img| img.id()));
        // Comments can carry anything a template author left there; nothing of ours reads them.
        to_detach.extend(
            document
                .tree
                .nodes()
                .filter(|n| matches!(n.value(), Node::Comment(_)))
                .map(|n| n.id()),
        );
        for id in to_detach {
            if let Some(mut node) = document.tree.get_mut(id) {
                node.detach();
            }
        }

        // Links keep their path and lose their query string and fragment; a path that carries a
        // configured term loses the whole attribute.
        let ids: Vec<ego_tree::NodeId> = document.tree.nodes().map(|n| n.id()).collect();
        for id in ids {
            let Some(mut node) = document.tree.get_mut(id) else {
                continue;
            };
            if let Node::Element(element) = node.value() {
                let mut drop_attrs = Vec::new();
                for (name, value) in element.attrs.iter_mut() {
                    if matches!(&*name.local, "href" | "src" | "action") {
                        let stripped = value.split(['?', '#']).next().unwrap_or("").to_owned();
                        if self.carries_configured_term(&stripped) {
                            drop_attrs.push(name.clone());
                        } else if stripped.len() != value.len() {
                            *value = stripped.as_str().into();
                        }
                    }
                }
                element.attrs.retain(|(name, _)| !drop_attrs.contains(name));
            }
        }

        let serialized = document.html();
        self.redact_text(&serialized)
    }

    /// Redacts plain text, or already-serialized HTML: normalization, literal terms, shapes, then
    /// the assertion.
    pub fn redact_text(&self, text: &str) -> Result<String, RedactionError> {
        let mut out = normalize_markup(text);
        for regex in &self.phones {
            out = regex
                .replace_all(&out, |caps: &regex::Captures<'_>| {
                    format!("[REDACTED:phone]{}", &caps[1])
                })
                .into_owned();
        }
        for literal in &self.literals {
            let replacement = format!("[REDACTED:{}]", literal.kind);
            // Twice: adjacent occurrences share a boundary character, which one pass consumes.
            out = replace_bounded(&literal.regex, &out, &replacement);
            out = replace_bounded(&literal.regex, &out, &replacement);
        }
        for regex in &self.last4 {
            out = regex
                .replace_all(&out, |caps: &regex::Captures<'_>| {
                    format!("[REDACTED:card]{}", &caps[3])
                })
                .into_owned();
        }
        out = PHONE_SHAPE
            .replace_all(&out, "[REDACTED:phone]")
            .into_owned();
        out = EMAIL_SHAPE
            .replace_all(&out, "[REDACTED:email]")
            .into_owned();
        out = CARD_SHAPE.replace_all(&out, "[REDACTED:card]").into_owned();
        self.verify(&out)?;
        Ok(out)
    }

    /// The assertion, independent of the passes: over the text a reader would see, with tags
    /// gone and spacing and case normalized, no term matches with word boundaries, no long term's
    /// letters and digits appear in sequence, and no phone or card pattern matches.
    fn verify(&self, out: &str) -> Result<(), RedactionError> {
        let view = text_view(out);
        let alnum = alphanumerics(&view);
        for literal in &self.literals {
            if literal.plain.is_match(&view) || literal.regex.is_match(out) {
                return Err(RedactionError::Survived { kind: literal.kind });
            }
            if !literal.alnum.is_empty() && alnum.contains(&literal.alnum) {
                return Err(RedactionError::Survived { kind: literal.kind });
            }
        }
        for regex in &self.phones {
            if regex.is_match(&view) || regex.is_match(out) {
                return Err(RedactionError::Survived { kind: "phone" });
            }
        }
        for regex in &self.last4 {
            if regex.is_match(&view) || regex.is_match(out) {
                return Err(RedactionError::Survived { kind: "card" });
            }
        }
        Ok(())
    }
}

/// `%20` to a space and so on; a lone `%` or a bad pair stays as it is.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&text[i + 1..i + 3], 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str =
        include_str!("../../../tests/fixtures/synthetic/input/freshdirect/2026/receipt-basic.html");

    fn terms() -> RedactionTerms {
        RedactionTerms {
            names: vec!["Pat Example".to_owned(), "Pat".to_owned()],
            emails: vec!["pat.example@example.com".to_owned()],
            phones: vec!["(212) 555-0199".to_owned()],
            address_lines: vec![
                "123 Imaginary Street Apt 4B".to_owned(),
                "Brooklyn, NY 11201".to_owned(),
            ],
            card_last4: vec!["4242".to_owned()],
        }
    }

    fn redactor() -> Redactor {
        Redactor::new(terms()).expect("terms")
    }

    #[test]
    fn the_synthetic_receipt_loses_its_personal_data_and_keeps_its_items() {
        let out = redactor().redact_html(FIXTURE).expect("redacts");
        for gone in [
            "Imaginary Street",
            "Brooklyn, NY 11201",
            "555-0199",
            "4242",
            "pat.example@example.com",
            "Pat Example",
            "Hi Pat",
            "SYNTHETIC-TRACKING-TOKEN",
            "SYNTHETIC-CUSTOMER-ID",
            "<img",
            "<!--",
        ] {
            assert!(!out.contains(gone), "{gone} survived:\n{out}");
        }
        for kept in [
            "Beefsteak Tomatoes, Large",
            "2/1.56",
            "$17.37",
            "Order #",
            "12345678901",
            "/product/veg_tomato_beef_lg",
            "Subtotal",
        ] {
            assert!(out.contains(kept), "{kept} was lost:\n{out}");
        }
        assert!(out.contains("[REDACTED:"), "{out}");
    }

    #[test]
    fn redaction_is_idempotent() {
        let once = redactor().redact_html(FIXTURE).expect("first");
        let twice = redactor().redact_text(&once).expect("second");
        assert_eq!(once, twice);
    }

    #[test]
    fn text_passes_catch_formatting_variants() {
        let out = redactor()
            .redact_text("Call 212.555.0199 or +1 (212) 555 0199. Card ****4242. Mail PAT.EXAMPLE@example.com. 123  Imaginary\nStreet Apt 4B")
            .expect("redacts");
        assert_eq!(
            out,
            "Call [REDACTED:phone] or [REDACTED:phone]. Card [REDACTED:card]. Mail [REDACTED:email]. [REDACTED:address]"
        );
    }

    #[test]
    fn a_term_split_by_tags_or_entities_is_still_redacted() {
        let r = redactor();
        // Inline markup inside an address line, in a block without a heading.
        let out = r
            .redact_html("<table><tr><td>123 Imaginary Street <span style=\"white-space:nowrap\">Apt 4B</span></td></tr></table>")
            .expect("redacts");
        assert!(!out.to_lowercase().contains("imaginary"), "{out}");
        assert!(out.contains("[REDACTED:address]"), "{out}");
        // Non-breaking spaces inside a phone number and a card number, no heading nearby.
        let out = r
            .redact_html(
                "<p>Tel. (212)&nbsp;555-0199 and Visa ending in&nbsp;4242 and ****&nbsp;4242</p>",
            )
            .expect("redacts");
        assert!(!out.contains("555"), "{out}");
        assert!(!out.contains("4242"), "{out}");
        // A soft hyphen inside a word of the name.
        let out = r
            .redact_text("Dear Pat Exam\u{ad}ple, welcome")
            .expect("redacts");
        assert_eq!(out, "Dear [REDACTED:name], welcome");
    }

    #[test]
    fn a_link_whose_path_carries_a_term_loses_the_attribute() {
        let out = redactor()
            .redact_html("<p><a href=\"https://x.example/u/Pat%20Example/orders?x=1\">orders</a> <a href=\"https://x.example/product/milk?trk=1\">milk</a></p>")
            .expect("redacts");
        assert!(!out.contains("Pat%20Example"), "{out}");
        assert!(!out.to_lowercase().contains("pat example"), "{out}");
        assert!(
            out.contains("href=\"https://x.example/product/milk\""),
            "{out}"
        );
    }

    #[test]
    fn the_assertion_is_independent_of_the_passes() {
        let r = redactor();
        assert_eq!(
            r.verify("delivered to 123 imaginary street apt 4b"),
            Err(RedactionError::Survived { kind: "address" })
        );
        assert_eq!(
            r.verify("123 <b>Imaginary</b> Street <i>Apt</i> 4B"),
            Err(RedactionError::Survived { kind: "address" })
        );
        assert_eq!(
            r.verify("call 212-555-0199 now"),
            Err(RedactionError::Survived { kind: "phone" })
        );
        assert_eq!(r.verify("Patagonia and Spatula are words"), Ok(()));
        assert_eq!(
            r.verify("order 1212555019901 shipped"),
            Ok(()),
            "digits inside a longer number are not a phone"
        );
    }

    #[test]
    fn short_or_malformed_terms_are_refused_without_echoing_them() {
        let mut t = terms();
        t.names.push("Pa".to_owned());
        let error = Redactor::new(t).err().expect("refused");
        assert_eq!(
            error,
            RedactionError::TermTooShort {
                kind: "name",
                position: 3
            }
        );
        assert!(!error.to_string().contains("Pa\""));
        let mut t = terms();
        t.card_last4.push("4242424242424242".to_owned());
        let error = Redactor::new(t).err().expect("refused");
        assert_eq!(error, RedactionError::BadLast4(2));
        assert!(!error.to_string().contains("4242"));
        let mut t = terms();
        t.phones.push("12345".to_owned());
        assert_eq!(Redactor::new(t).err(), Some(RedactionError::BadPhone(2)));
    }

    #[test]
    fn product_names_that_merely_contain_a_name_are_left_alone() {
        let out = redactor()
            .redact_text("Patagonia Provisions Smoked Salmon; Spatula; Pat's Choice is gone")
            .expect("redacts");
        assert_eq!(
            out,
            "Patagonia Provisions Smoked Salmon; Spatula; [REDACTED:name]'s Choice is gone"
        );
    }

    #[test]
    fn two_adjacent_terms_are_both_redacted() {
        let out = redactor().redact_text("Pat Pat Example").expect("redacts");
        assert_eq!(out, "[REDACTED:name] [REDACTED:name]");
    }

    #[test]
    fn a_message_key_that_carries_a_term_is_recognized() {
        let r = redactor();
        assert!(r.carries_configured_term("pat.example@example.com.1234@mail.example"));
        assert!(r.carries_configured_term("/u/Pat%20Example"));
        assert!(!r.carries_configured_term("abc.def@mail.example"));
    }
}
