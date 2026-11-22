//! Redaction before storage (docs/concepts/privacy.md). Four passes over a message, then an
//! assertion with checks of its own: a message in which it still finds a configured term is
//! refused, never stored.
//!
//! 1. Structural: the smallest HTML block that carries an address, payment or contact heading is
//!    removed whole, as are images and comments; a link whose address carries a configured term
//!    loses that attribute, and each link in an `href`, `src` or `action` attribute loses its
//!    query string and fragment.
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

use crate::domain::pattern::{SIZE_LIMIT, fixed};
use scraper::{Html, Node, Selector};
use serde::Deserialize;
use thiserror::Error;

/// Bumped whenever a pass changes, so stored rows can be re-imported and re-redacted.
/// 2: entity and invisible-character normalization, tag-tolerant terms, independent assertion.
/// 3: the sequence pass for a long term. 4 to 6: card digits after a brand, the word "card",
/// a wallet name. 7: an email address is removed whole when a pass already replaced a term
/// inside it, in the local part or in the domain; and the terms are replaced before the phone
/// numbers as well as after, so the optional country code of a phone number does not take the
/// last digit of an address term. 8: a link loses its address also when the address holds a
/// term between `<` and `>`, the digits of a phone number inside a longer run of digits, or
/// a term that is there only before the percent pairs are decoded; the same check decides
/// whether a `Message-ID` is the message key.
pub const REDACTION_VERSION: i32 = 8;

/// The `[redaction]` section of the configuration file: the redaction terms of the
/// maintainer, by kind. Every value is personal data. A key that the section leaves out is an
/// empty list: no term of that kind is configured, and the patterns for the shapes still run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct RedactionTerms {
    /// Full names and first names as the mail greets you.
    #[serde(default)]
    pub names: Vec<String>,
    /// Email addresses of the maintainer, each replaced as one term in any case. The pattern
    /// for the shape of an address removes other addresses too.
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
    /// A name, an email address or an address line has fewer than three characters, spaces
    /// at its ends not counted.
    #[error("{kind} term #{position} is too short to redact safely; use at least three characters")]
    TermTooShort {
        /// Which list holds the term: `name`, `email` or `address`.
        kind: &'static str,
        /// The position of the term in that list of the configuration; the first is 1.
        position: usize,
    },
    /// The pattern made from a name, an email address or an address line does not compile
    /// within the size limit of a pattern: the term is several hundred characters long.
    #[error(
        "{kind} term #{position} cannot be used as a pattern; a term of several hundred characters is too long"
    )]
    BadTerm {
        /// Which list holds the term: `name`, `email` or `address`.
        kind: &'static str,
        /// The position of the term in that list of the configuration; the first is 1.
        position: usize,
    },
    /// A phone number has fewer than seven digits, or so many that its pattern does not
    /// compile within the size limit. The value is its position in `phones`; the first is 1.
    #[error("phone #{0} needs at least seven digits, and not several hundred")]
    BadPhone(usize),
    /// A card term does not hold exactly four digits; other characters are not counted. The
    /// value is its position in `card_last4`; the first is 1.
    #[error("card_last4 #{0} is not exactly four digits")]
    BadLast4(usize),
    /// `check` says which assertion found the term; it never carries the text.
    #[error("{kind} survived redaction ({check})")]
    Survived {
        /// The kind of the term that is still there: `name`, `email`, `address`, `phone` or
        /// `card`.
        kind: &'static str,
        /// A fixed text for the assertion that found it: in the markup, in the text once the
        /// tags are removed, or as the same letters and digits with other punctuation.
        check: &'static str,
    },
}

impl RedactionError {
    /// The kind of term the error is about: `name`, `email`, `address`, `phone` or `card`.
    /// The import puts it into the error and the report of a refused message, which can name
    /// the kind and must not name the term.
    pub fn kind(&self) -> &'static str {
        match self {
            RedactionError::TermTooShort { kind, .. } | RedactionError::BadTerm { kind, .. } => {
                kind
            }
            RedactionError::BadPhone(_) => "phone",
            RedactionError::BadLast4(_) => "card",
            RedactionError::Survived { kind, .. } => kind,
        }
    }

    /// Which assertion failed, for the import report; empty for a construction failure.
    pub fn check(&self) -> &'static str {
        match self {
            RedactionError::Survived { check, .. } => check,
            RedactionError::TermTooShort { .. }
            | RedactionError::BadTerm { .. }
            | RedactionError::BadPhone(_)
            | RedactionError::BadLast4(_) => "",
        }
    }
}

static HEADING: LazyLock<Regex> = LazyLock::new(|| {
    fixed(
        r"(?i)\b(deliver(y|ed)?\s+(address|to|information|details)|ship(ping)?\s+(address|to)|payment|billing|contact\s+(info|information)|phone|tel\.?)\b",
    )
});
/// A card brand, or the word "card", before four digits is payment context too: a
/// point-of-sale receipt prints "MasterCard 4242" and a delivery platform "Credit Card - 4242",
/// with no mask; a wallet ("Android Pay - 4242") counts the same. No boundary before the word: a text body runs cells together.
const CARD_BRAND: &str = r"(?:visa|card|amex|american\s+express|discover|diners\s+club|jcb|pay|paypal|venmo)[\s#:\-\x{2013}]*";

static PHONE_SHAPE: LazyLock<Regex> =
    LazyLock::new(|| fixed(r"(?:\+?1[\s.-]*)?\(?[2-9][0-9]{2}\)?[\s.-]+[0-9]{3}[\s.-]+[0-9]{4}\b"));
/// What a pass leaves in place of a term.
const TOKEN: &str = r"\[REDACTED:[a-z]+\]";
/// An address may hold a replacement on either side of the `@`: a pass that ran earlier can
/// have replaced a name inside an address that is not itself a configured term, and the rest
/// of the address must still go. The domain ends in a top-level name or in a replacement.
static EMAIL_SHAPE: LazyLock<Regex> = LazyLock::new(|| {
    fixed(&format!(
        r"(?:[A-Za-z0-9._%+-]|{TOKEN})+@(?:(?:[A-Za-z0-9.-]|{TOKEN})+\.[A-Za-z]{{2,}}|(?:[A-Za-z0-9.-]|{TOKEN})*{TOKEN})"
    ))
});
static CARD_SHAPE: LazyLock<Regex> = LazyLock::new(|| {
    fixed(&format!(
        r"(?i)(ending\s+(in|with)|[xX*\u{{2022}}]{{2,}}|{CARD_BRAND})\s*[0-9]{{4}}\b"
    ))
});
static WS: LazyLock<Regex> = LazyLock::new(|| fixed(r"\s+"));
static TAG: LazyLock<Regex> = LazyLock::new(|| fixed(r"<[^>]*>"));

/// A CSS selector that is a constant of the source.
#[expect(
    clippy::expect_used,
    reason = "the selector is a constant of the source; an invalid one is a bug, found by the first test that redacts HTML"
)]
fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("a selector written in the source is valid")
}

/// The term matches the redacted markup itself: a pass left an occurrence behind.
const CHECK_MARKUP: &str = "still in the markup";
/// The term matches only once the tags are gone: markup the passes do not bridge splits it.
const CHECK_TEXT: &str = "in the text once tags are removed";
/// The term's letters and digits appear in sequence: other punctuation or spacing, or inside a
/// longer run such as a link.
const CHECK_SEQUENCE: &str = "same letters and digits, other punctuation";

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
    /// The same letters and digits with any punctuation or spacing between them and no word
    /// boundary, inside one run of text; `None` when `alnum` is empty.
    sequence: Option<Regex>,
    /// Characters in the term, for longest-first ordering.
    length: usize,
}

/// The redaction terms as compiled patterns, with the passes and the assertion that use them.
/// A command builds one from the configuration and uses it for every message. It holds the
/// terms inside its patterns and does not implement `Debug`.
pub struct Redactor {
    literals: Vec<Literal>,
    phones: Vec<Regex>,
    /// The national digits of each phone number, for the check of a short text.
    phone_digits: Vec<String>,
    last4: Vec<Regex>,
}

fn flexible_literal(term: &str) -> String {
    // Each whitespace run in the term matches any run of whitespace and tags in the markup.
    term.split_whitespace()
        .map(regex::escape)
        .collect::<Vec<_>>()
        .join(r"(?:\s|<[^>]*>)+")
}

/// What may stand between two characters of a term in the sequence pass: anything that is not
/// a letter or digit, and an entity the text view turns into punctuation. A tag is not allowed,
/// so the pass never removes markup; a term that a tag splits this way is left to the assertion.
const SEQUENCE_GAP: &str = r"(?:&(?:amp|lt|gt|quot|#39);|[^A-Za-z0-9<>])*";

/// The pattern of the sequence pass for a term's letters and digits.
fn sequence_pattern(alnum: &str) -> String {
    alnum
        .chars()
        .map(|c| regex::escape(&c.to_string()))
        .collect::<Vec<_>>()
        .join(SEQUENCE_GAP)
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

/// Compiles a pattern that was built from a configured term, without regard to case.
///
/// `None` when the pattern is not valid or compiles to more than [`SIZE_LIMIT`]
/// (REAS-SEC-003). A term of a name, an address or a mail address is far below that; a term
/// of several hundred characters is not one of these, and the caller refuses it by position.
fn case_insensitive(pattern: &str) -> Option<Regex> {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .size_limit(SIZE_LIMIT)
        .build()
        .ok()
}

/// Compiles a pattern that was built from configured digits, with the same limit.
fn from_digits(pattern: &str) -> Option<Regex> {
    RegexBuilder::new(pattern)
        .size_limit(SIZE_LIMIT)
        .build()
        .ok()
}

impl Redactor {
    #[expect(
        clippy::needless_pass_by_value,
        reason = "the terms are personal data: the constructor takes the value that the caller hands over, so the caller keeps none of it; the settings still hold the copy that they read from the file"
    )]
    /// Compiles the terms into patterns. The lists are checked in the order names, email
    /// addresses, address lines, phone numbers, cards, and the first term that cannot be used
    /// ends the construction. No error carries the text of a term.
    ///
    /// # Errors
    ///
    /// - [`RedactionError::TermTooShort`] when a name, an email address or an address line has
    ///   fewer than three characters.
    /// - [`RedactionError::BadTerm`] when the pattern of such a term does not compile within
    ///   the size limit, which a term of several hundred characters causes.
    /// - [`RedactionError::BadPhone`] when a phone number has fewer than seven digits, or its
    ///   pattern does not compile within the size limit.
    /// - [`RedactionError::BadLast4`] when a card term does not hold exactly four digits.
    pub fn new(terms: RedactionTerms) -> Result<Self, RedactionError> {
        let mut literals = Vec::new();
        for (kind, list) in [
            ("name", &terms.names),
            ("email", &terms.emails),
            ("address", &terms.address_lines),
        ] {
            for (position, term) in (1_usize..).zip(list) {
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
                let alnum = if alnum.len() >= 8 {
                    alnum
                } else {
                    String::new()
                };
                let sequence = if alnum.is_empty() {
                    None
                } else {
                    Some(
                        case_insensitive(&sequence_pattern(&alnum))
                            .ok_or(RedactionError::BadTerm { kind, position })?,
                    )
                };
                literals.push(Literal {
                    kind,
                    regex,
                    plain,
                    alnum,
                    sequence,
                    length,
                });
            }
        }
        // Longest first, so "pat.example@example.com" and "Pat Example" go before "Pat".
        literals.sort_by_key(|l| std::cmp::Reverse(l.length));

        let mut phones = Vec::new();
        let mut phone_digits = Vec::new();
        for (position, phone) in (1_usize..).zip(&terms.phones) {
            let all_digits: String = phone.chars().filter(char::is_ascii_digit).collect();
            if all_digits.len() < 7 {
                return Err(RedactionError::BadPhone(position));
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
            let regex = from_digits(&format!(r"(?:\+?1[^0-9]{{0,3}})?\(?{spaced}([^0-9]|$)"))
                .ok_or(RedactionError::BadPhone(position))?;
            phones.push(regex);
            phone_digits.push(national);
        }
        let mut last4 = Vec::new();
        for (position, four) in (1_usize..).zip(&terms.card_last4) {
            let digits: String = four.chars().filter(char::is_ascii_digit).collect();
            if digits.len() != 4 {
                return Err(RedactionError::BadLast4(position));
            }
            let regex = from_digits(&format!(
                r"(?i)(ending\s+(in|with)|[xX*\u{{2022}}]{{2,}}|{CARD_BRAND})\s*{digits}([^0-9]|$)"
            ))
            .ok_or(RedactionError::BadLast4(position))?;
            last4.push(regex);
        }
        Ok(Self {
            literals,
            phones,
            phone_digits,
            last4,
        })
    }

    /// Whether a short text, such as a message key or a link address, carries a configured term
    /// or phone number. Shapes are not checked: a message key always looks like an address.
    ///
    /// The check is wider than the passes over a body, because a text that it refuses costs
    /// nothing: the key becomes the hash of the message, and the link loses its address.
    ///
    /// - The text is read as it is and with its percent pairs decoded. A `%` before a term
    ///   that starts with two hexadecimal digits would otherwise take them away as a pair:
    ///   `x%2125550199` holds the phone number, and its decoded form does not.
    /// - Each of the two is read as markup and as plain text. A key is no markup, so a term
    ///   between `<` and `>` is still in it.
    /// - The digits of a phone number count also inside a longer run of digits and with
    ///   other signs between them. A body keeps such a run: there it can be another number.
    ///
    /// The fuzz targets and their review found the three cases on 2026-11-21.
    pub fn carries_configured_term(&self, text: &str) -> bool {
        let decoded = percent_decode(text);
        let plain = |reading: &str| {
            WS.replace_all(normalize_markup(reading).trim(), " ")
                .to_lowercase()
        };
        [
            text_view(&decoded),
            plain(&decoded),
            text_view(text),
            plain(text),
        ]
        .iter()
        .any(|view| {
            let letters_and_digits = alphanumerics(view);
            let digits: String = view.chars().filter(char::is_ascii_digit).collect();
            self.literals.iter().any(|l| {
                l.plain.is_match(view)
                    || (!l.alnum.is_empty() && letters_and_digits.contains(&l.alnum))
            }) || self.phones.iter().any(|p| p.is_match(view))
                || self
                    .phone_digits
                    .iter()
                    .any(|phone| digits.contains(phone.as_str()))
        })
    }

    /// Redacts HTML: structure first, then text passes over the normalized serialized result.
    ///
    /// # Errors
    ///
    /// [`RedactionError::Survived`] when the assertion still finds a configured name, email
    /// address, address line, phone number or card ending in the result. The caller must not
    /// store the message then.
    ///
    /// # Panics
    ///
    /// Does not panic for any input. The function it calls for a CSS selector has an `expect`
    /// on a selector that is a constant of the source.
    pub fn redact_html(&self, html: &str) -> Result<String, RedactionError> {
        let mut document = Html::parse_document(html);
        let element = selector("td, div, p, li, section, address, span");

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
        let images = selector("img");
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
    ///
    /// # Errors
    ///
    /// [`RedactionError::Survived`] when the assertion still finds a configured name, email
    /// address, address line, phone number or card ending in the result. The caller must not
    /// store the message then.
    pub fn redact_text(&self, text: &str) -> Result<String, RedactionError> {
        // The terms first, then the phone numbers, then the terms again. A phone pattern takes
        // an optional country code 1 before the number, and that 1 can be the last digit of
        // what stands before it. Before the phone pass, an address term claims its own ZIP
        // code; after it, a term that the country code was glued to has its boundary back.
        let mut out = self.replace_literals(normalize_markup(text));
        for regex in &self.phones {
            out = regex
                .replace_all(&out, |caps: &regex::Captures<'_>| {
                    format!("[REDACTED:phone]{}", &caps[1])
                })
                .into_owned();
        }
        out = self.replace_literals(out);
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
        // Until nothing changes: "a@b.test@c.test" is an address after its first part is replaced.
        // Each round removes at least one `@`, so the loop ends.
        loop {
            let next = EMAIL_SHAPE.replace_all(&out, "[REDACTED:email]");
            if next == out {
                break;
            }
            out = next.into_owned();
        }
        out = CARD_SHAPE.replace_all(&out, "[REDACTED:card]").into_owned();
        self.verify(&out)?;
        Ok(out)
    }

    /// Replaces every configured term, longest first.
    fn replace_literals(&self, text: String) -> String {
        let mut out = text;
        for literal in &self.literals {
            let replacement = format!("[REDACTED:{}]", literal.kind);
            // The sequence pass first: a long term with other punctuation, or with no word
            // boundary because a text body ran two table cells together. It removes exactly
            // what the assertion would refuse the message for.
            if let Some(sequence) = &literal.sequence {
                out = sequence
                    .replace_all(&out, regex::NoExpand(&replacement))
                    .into_owned();
            }
            // Twice: adjacent occurrences share a boundary character, which one pass consumes.
            out = replace_bounded(&literal.regex, &out, &replacement);
            out = replace_bounded(&literal.regex, &out, &replacement);
        }
        out
    }

    /// The assertion. For each configured term it has one check of the markup with the pattern
    /// of the pass, and two checks of its own over the text a reader would see, with tags gone
    /// and spacing and case normalized: the term does not match with word boundaries, and the
    /// letters and digits of a long term do not appear in sequence. For a phone number and a
    /// card ending it uses the patterns of the passes, on the markup and on that text.
    fn verify(&self, out: &str) -> Result<(), RedactionError> {
        let view = text_view(out);
        let alnum = alphanumerics(&view);
        let survived = |kind, check| Err(RedactionError::Survived { kind, check });
        for literal in &self.literals {
            if literal.regex.is_match(out) {
                return survived(literal.kind, CHECK_MARKUP);
            }
            if literal.plain.is_match(&view) {
                return survived(literal.kind, CHECK_TEXT);
            }
            if !literal.alnum.is_empty() && alnum.contains(&literal.alnum) {
                return survived(literal.kind, CHECK_SEQUENCE);
            }
        }
        for regex in &self.phones {
            if regex.is_match(out) {
                return survived("phone", CHECK_MARKUP);
            }
            if regex.is_match(&view) {
                return survived("phone", CHECK_TEXT);
            }
        }
        for regex in &self.last4 {
            if regex.is_match(out) {
                return survived("card", CHECK_MARKUP);
            }
            if regex.is_match(&view) {
                return survived("card", CHECK_TEXT);
            }
        }
        Ok(())
    }
}

/// `%20` to a space and so on; a lone `%` or a bad pair stays as it is.
///
/// The text is outside data: a message key or a link of a mail. The function reads bytes and
/// takes no index into the text, so a `%` before a character of more than one byte cannot stop
/// it (REAS-SEC-003). A pair is two hexadecimal digits, with one leniency that [`hex_pair`]
/// explains.
fn percent_decode(text: &str) -> String {
    let mut out = Vec::with_capacity(text.len());
    let mut rest = text.as_bytes();
    while let Some((&first, tail)) = rest.split_first() {
        if first == b'%'
            && let Some((pair, after)) = tail.split_first_chunk::<2>()
            && let Some(byte) = hex_pair(*pair)
        {
            out.push(byte);
            rest = after;
        } else {
            out.push(first);
            rest = tail;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The byte that two hexadecimal digits name, or `None` when the two are not a pair.
///
/// A `+` and one digit is read as that digit alone, so `%+9` is a tab. No reader of a link
/// decodes that, but the first decoder did, and it stays so that this function gives the same
/// result as the first decoder for every text that did not stop it. The lenient reading is not
/// safer than the strict one, only the same as before: it finds `x%+9Pat`, and it misses a
/// term whose first character follows `%+`, because that character goes into the pair. A
/// check of both readings is a change of the redaction and is not made here.
fn hex_pair([high, low]: [u8; 2]) -> Option<u8> {
    let digit = |byte: u8| {
        char::from(byte)
            .to_digit(16)
            .and_then(|d| u8::try_from(d).ok())
    };
    let low = digit(low)?;
    if high == b'+' {
        return Some(low);
    }
    Some((digit(high)? << 4) | low)
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
    fn a_long_term_is_redacted_without_a_word_boundary_and_with_other_punctuation() {
        let r = redactor();
        // A text body made from a table: the cells run together.
        let cells = r
            .redact_text("Deliver to:Pat ExampleOrder 12345678901 Beefsteak Tomatoes $3.99")
            .expect("redacts");
        assert!(!cells.to_lowercase().contains("example"), "{cells}");
        assert!(cells.contains("Beefsteak Tomatoes $3.99"), "{cells}");
        // Other punctuation and spacing than the configured line, and no boundary after it.
        let address = r
            .redact_text("123 Imaginary Street, Apt. 4B\r\nBrooklyn NY, 11201Leave at door")
            .expect("redacts");
        assert!(!address.contains("Imaginary"), "{address}");
        assert!(!address.contains("11201"), "{address}");
        assert!(address.contains("eave at door"), "{address}");
        // In markup the pass stays inside one run of text and keeps the tags.
        let html = r
            .redact_html("<table><tr><td>Pat-Example</td><td>Milk</td></tr></table>")
            .expect("redacts");
        assert!(!html.contains("Example"), "{html}");
        assert!(html.contains("<td>Milk</td>"), "{html}");
        // A short term has no sequence pass: it would eat parts of ordinary words.
        assert_eq!(
            r.redact_text("Patagonia and Spatula are words")
                .expect("ok"),
            "Patagonia and Spatula are words"
        );
    }

    #[test]
    fn four_digits_after_a_card_brand_are_redacted_and_a_price_is_not() {
        let r = redactor();
        // A configured card and another card, both unmasked, in a text body with no spacing.
        let out = r
            .redact_text("TotalMasterCard 4242 (Chip)Visa 1881 $42.42 for 4242 items")
            .expect("redacts");
        let platform = r
            .redact_html("<p>Payment<br>   Credit Card - 4242</p><p>Gift card: 9004</p><p>Android Pay - 7310</p>")
            .expect("redacts");
        assert!(!platform.contains("4242"), "{platform}");
        assert!(!platform.contains("9004"), "{platform}");
        assert!(!platform.contains("7310"), "{platform}");
        assert!(!out.contains("MasterCard 4242"), "{out}");
        assert!(!out.contains("1881"), "{out}");
        assert!(out.contains("$42.42 for 4242 items"), "{out}");
    }

    #[test]
    fn an_address_that_holds_a_replaced_term_is_removed_whole() {
        let r = redactor();
        // None of these addresses is a configured term; a term is inside each of them, in the
        // local part or in the domain.
        for text in [
            "Contact: pat.example@workplace-domain.test today",
            "Contact: patexample@workplace-domain.test today",
            "mr.pat_example@workplace-domain.test",
            "Pat Example <pat.example@other.test> wrote",
            "pat@workplace-domain.test",
            "jane.roe@patexample.test",
            "orders+freshdirect@patexample.test today",
            "jane.roe@mail.patexample.co.test",
            "jane@pat.test",
            "jane@card4242.test",
            "x@212-555-0199.carrier.test",
            "212-555-0199@txt.carrier.test",
            // One address after another, with no space between them.
            "Pat@a.test@d.test",
            "x@a.test@d.test",
        ] {
            let out = r.redact_text(text).expect("redacts");
            assert!(!out.contains('@'), "{text} -> {out}");
            for part in [
                "domain.test",
                "other.test",
                "jane",
                "roe",
                "carrier",
                "d.test",
            ] {
                assert!(!out.contains(part), "{text} -> {out}");
            }
        }
        let html = r
            .redact_html("<p>Reply to jane.roe@patexample.test</p>")
            .expect("redacts");
        assert!(!html.contains("jane"), "{html}");
    }

    #[test]
    fn a_phone_number_after_a_zip_code_leaves_the_address_term_whole() {
        let r = redactor();
        // The configured phone and another phone, each right after the ZIP code of the
        // configured address, which ends in 1.
        for text in [
            "Brooklyn, NY 11201\n(212) 555-0199",
            "Brooklyn, NY 11201 212-555-0199",
            "Brooklyn, NY 11201\n(718) 555-0123",
            "Brooklyn, NY 11201(718) 555-0123",
        ] {
            let out = r.redact_text(text).expect("redacts");
            assert!(!out.contains("1120"), "{text} -> {out}");
            assert!(!out.contains("555"), "{text} -> {out}");
        }
        assert_eq!(
            r.redact_text("call +1 718 555 0123 now").expect("redacts"),
            "call [REDACTED:phone] now"
        );
    }

    /// A message is safe when it is refused, or when nothing of `gone` is left in it.
    fn refused_or_without(r: &Redactor, text: &str, gone: &[&str]) {
        if let Ok(out) = r.redact_text(text) {
            for part in gone {
                assert!(!out.contains(part), "{text} -> {out}");
            }
        }
    }

    #[test]
    fn a_country_code_glued_to_what_stands_before_it_hides_nothing() {
        let r = redactor();
        // The configured phone, then a number that starts with the country code, with no
        // space: the 1 must not become the end of the configured phone.
        for text in [
            "Phone:(212) 555-01991-800-555-0100 Customer Service",
            "Phone: 212-555-01991 (800) 555-0100",
            "Home (718) 555-01231-212-555-0199 Cell",
            "call +1 212 555 0199 now",
            "call 1-212-555-0199 now",
            "x51 212 555 0199",
        ] {
            refused_or_without(&r, text, &["555-01", "555 01"]);
        }
        // A short term that ends in a digit, then a phone number with its country code.
        let mut with_short_terms = terms();
        with_short_terms.names.push("Unit 12".to_owned());
        with_short_terms.names.push("11201".to_owned());
        let r = Redactor::new(with_short_terms).expect("terms");
        refused_or_without(&r, "Unit 121-212-555-0199", &["Unit 12", "555"]);
        refused_or_without(&r, "ZIP 112011-212-555-0199", &["11201", "555"]);
    }

    #[test]
    fn the_smallest_block_with_a_heading_is_removed_and_the_blocks_around_it_stay() {
        // The outer block carries the heading too, through its child. Only the inner one
        // goes: a receipt is one large table, and its items are beside the address.
        let r = redactor();
        let out = r
            .redact_html(
                "<div><p>Milk 2 $5.79</p><div><p>Eggs 1 $4.29</p>\
                 <p>Delivery Address: somewhere</p></div><p>Bread 1 $3.49</p></div>",
            )
            .expect("redacts");
        assert!(out.contains("Milk 2 $5.79"), "{out}");
        assert!(out.contains("Eggs 1 $4.29"), "{out}");
        assert!(out.contains("Bread 1 $3.49"), "{out}");
        assert!(!out.contains("somewhere"), "{out}");
        assert!(!out.contains("Delivery Address"), "{out}");
        // A block with the heading in its own text goes whole, also when a block inside it
        // has no heading.
        let out = r
            .redact_html("<p>Milk</p><div>Delivery Address: somewhere <span>else</span></div>")
            .expect("redacts");
        assert!(out.contains("Milk"), "{out}");
        assert!(!out.contains("somewhere") && !out.contains("else"), "{out}");
    }

    #[test]
    fn a_form_that_the_passes_leave_is_refused_by_a_check_of_the_assertion_alone() {
        // The letters of a name with a tag inside the first word: no pass replaces this form.
        // The markup still holds the tag, so the pattern of the pass does not match; the two
        // checks that are the assertion's own read the text without its tags.
        let r = redactor();
        assert_eq!(
            r.redact_html("<p>P<b>a</b>t Example</p>"),
            Err(RedactionError::Survived {
                kind: "name",
                check: CHECK_SEQUENCE
            })
        );
        // The control: the same words without the name are a result.
        assert!(r.redact_html("<p>P<b>e</b>t Example</p>").is_ok());
        // The view of the assertion: tags gone, spacing and case made one.
        assert_eq!(
            text_view("<p>Pat <b>EXAMPLE</b>&nbsp;x</p>"),
            text_view("pat example x")
        );
        assert_ne!(text_view("<p>Pat</p>"), text_view("<p>Pet</p>"));
    }

    #[test]
    fn the_assertion_is_independent_of_the_passes() {
        let r = redactor();
        assert_eq!(
            r.verify("delivered to 123 imaginary street apt 4b"),
            Err(RedactionError::Survived {
                kind: "address",
                check: CHECK_MARKUP
            })
        );
        assert_eq!(
            r.verify("123 <b>Imaginary</b> Street <i>Apt</i> 4B"),
            Err(RedactionError::Survived {
                kind: "address",
                check: CHECK_MARKUP
            })
        );
        assert_eq!(
            r.verify("call 212-555-0199 now"),
            Err(RedactionError::Survived {
                kind: "phone",
                check: CHECK_MARKUP
            })
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

    #[test]
    fn a_phone_number_inside_a_longer_run_of_digits_is_found_in_a_short_text() {
        let r = redactor();
        // Found by the fuzz target of the mail reader on 2026-11-21: a `Message-ID` with the
        // digits of the phone number and one more digit was stored as the message key.
        assert!(r.carries_configured_term("r21255501991@mail.example"));
        assert!(r.carries_configured_term("92125550199@mail.example"));
        assert!(r.carries_configured_term("id-2125550199-7@mail.example"));
        // The digits with other signs between them, and with the country code.
        assert!(r.carries_configured_term("u212x555y0199z@mail.example"));
        assert!(r.carries_configured_term("12125550199000@mail.example"));
        // The control: nine of the ten digits, and the ten in another order, are no phone.
        assert!(!r.carries_configured_term("r212555019@mail.example"));
        assert!(!r.carries_configured_term("r2125550919@mail.example"));
        // A body is not read this way: there the longer run can be another number.
        assert_eq!(
            r.redact_text("order 21255501991").expect("redacts"),
            "order 21255501991"
        );
    }

    #[test]
    fn a_percent_sign_before_a_term_does_not_hide_it_in_a_short_text() {
        let r = redactor();
        // Found by the review of the fuzz targets on 2026-11-21: the check read the decoded
        // text only, and `%21` is one character there, so two digits of the number were gone.
        assert!(r.carries_configured_term("x%2125550199@mail.example"));
        assert!(r.carries_configured_term("x%+2125550199"));
        assert!(r.carries_configured_term("x%123 Imaginary Street Apt 4B"));
        // A term that is there only after decoding is still found.
        assert!(r.carries_configured_term("/u/Pat%20Example"));
        assert!(r.carries_configured_term("%32125550199"));
        // The control: a pair before a text that is no term.
        assert!(!r.carries_configured_term("x%21abc@mail.example"));
    }

    #[test]
    fn a_term_between_angle_brackets_is_found_in_a_short_text() {
        let r = redactor();
        // Found by the fuzz target of the redactor on 2026-11-21: the check read the text as
        // markup only, and what stands between `<` and `>` is a tag there.
        assert!(r.carries_configured_term("a<pat.example@example.com>b@mail.example"));
        assert!(r.carries_configured_term("x<Pat Example>@mail.example"));
        assert!(r.carries_configured_term("<p title=\"123 Imaginary Street Apt 4B\">"));
        // The reading as markup still counts: a tag between the two words of a name.
        assert!(r.carries_configured_term("Pat<b>Example"));
        // The control: angle brackets alone change nothing.
        assert!(!r.carries_configured_term("a<abc.def@mail.example>b@mail.example"));
    }

    #[test]
    fn a_percent_sign_before_a_wide_character_does_not_stop_the_check() {
        let r = redactor();
        // Before 2026-11-20 each of the first three texts stopped the program: the decoder cut
        // the text two bytes after the `%`, inside the character.
        assert!(!r.carries_configured_term("<abc%a\u{e9}@mail.example>"));
        assert!(!r.carries_configured_term("/u/100%1\u{20ac}"));
        assert!(!r.carries_configured_term("%0\u{1f600}"));
        // A term after such a sequence is still found, and only through decoding: without it
        // the first text has no word "Pat", and the second no line of the address.
        assert!(r.carries_configured_term("/u/%a\u{e9}/%50at%20Example"));
        assert!(r.carries_configured_term("/u/%a\u{e9}/Brookly%6E%2C%20%4eY%2011201"));
        assert!(!r.carries_configured_term("/u/%a\u{e9}/%51at%20Example"));
    }

    #[test]
    fn a_term_whose_words_are_joined_by_a_lenient_pair_is_still_found() {
        let r = redactor();
        // `%09` is a tab. `%+9` is one too in the lenient reading, which the first decoder had.
        assert!(r.carries_configured_term("x%09Pat"));
        assert!(r.carries_configured_term("x%+9Pat"));
        assert!(r.carries_configured_term("123%+9Imaginary%+9Street%+9Apt%+94B"));
        assert!(r.carries_configured_term("x%+0Pat"));
        // A digit that is not part of a pair joins the word, and the word is then another word.
        assert!(!r.carries_configured_term("x9Pat"));
    }

    #[test]
    fn a_pair_is_decoded_and_everything_else_stays() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("%4a%4A"), "JJ");
        assert_eq!(percent_decode("%"), "%");
        assert_eq!(percent_decode("%2"), "%2");
        assert_eq!(percent_decode("50%"), "50%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("%2g"), "%2g");
        assert_eq!(percent_decode("%g2"), "%g2");
        assert_eq!(percent_decode("%-5"), "%-5");
        // The one leniency: a plus sign and a digit is that digit.
        assert_eq!(percent_decode("%+5"), "\u{5}");
        assert_eq!(percent_decode("%+g"), "%+g");
        assert_eq!(percent_decode("%++"), "%++");
        assert_eq!(percent_decode("%5+"), "%5+");
        // Two pairs that are one character, and one pass only.
        assert_eq!(percent_decode("%c3%a9"), "\u{e9}");
        assert_eq!(percent_decode("%2541"), "%41");
        assert_eq!(percent_decode("%a\u{e9}"), "%a\u{e9}");
        assert_eq!(percent_decode("%%41"), "%A");
        // Bytes that are not UTF-8 after decoding become the replacement character.
        assert_eq!(percent_decode("%ff"), "\u{fffd}");
    }

    #[test]
    fn a_term_of_a_hundred_characters_is_accepted() {
        // Longer than a name, a mail address or an address line.
        let long: String = "Ab9".chars().cycle().take(100).collect();
        let mut t = terms();
        t.names.push(long.clone());
        let redactor = Redactor::new(t).expect("a long term");
        let out = redactor
            .redact_text(&format!("for {long} today"))
            .expect("redacted");
        assert!(!out.contains(&long), "{out}");
    }

    #[test]
    fn a_term_whose_pattern_passes_the_size_limit_is_refused_by_its_position() {
        // A thousand letters and digits: the pattern of the sequence pass passes the limit.
        let huge: String = "Ab9".chars().cycle().take(1000).collect();
        let mut t = terms();
        t.names.push(huge);
        let refused = Redactor::new(t).err();
        assert_eq!(
            refused,
            Some(RedactionError::BadTerm {
                kind: "name",
                position: 3
            })
        );
        // The message names the position and never the term.
        let message = refused.expect("an error").to_string();
        assert!(
            message.starts_with("name term #3 cannot be used"),
            "{message}"
        );
        assert!(!message.contains("Ab9"));

        let mut t = terms();
        t.phones.push("5".repeat(5000));
        assert_eq!(Redactor::new(t).err(), Some(RedactionError::BadPhone(2)));
    }

    #[test]
    fn every_group_that_the_code_indexes_takes_part_in_every_match() {
        // The replacements read `caps[1]` and `caps[3]` of a term's pattern and `caps[1]` of
        // a phone's pattern; no lint sees an index of a capture group. The term here holds
        // every character that is special in a pattern, so it shows that a term adds no group.
        let mut t = terms();
        t.names.push("(a|b)? [c]{2} (?P<x>d)* \\1 $^".to_owned());
        let redactor = Redactor::new(t).expect("terms");
        for literal in &redactor.literals {
            for pattern in [&literal.regex, &literal.plain] {
                assert_eq!(pattern.captures_len(), 4);
                assert_eq!(pattern.static_captures_len(), Some(4));
            }
        }
        for phone in &redactor.phones {
            assert_eq!(phone.captures_len(), 2);
            assert_eq!(phone.static_captures_len(), Some(2));
        }
        // The pattern of the card digits has one group that is not in every match, the
        // second, and the code reads the third, which is the last.
        for card in &redactor.last4 {
            assert_eq!(card.captures_len(), 4);
            assert_eq!(card.static_captures_len(), None);
        }
    }
}
