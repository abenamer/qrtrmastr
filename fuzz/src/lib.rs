//! What the fuzz targets share: the redaction terms of a made-up person and the check that a
//! text holds none of them.
//!
//! The terms are the ones of the synthetic receipts under `tests/fixtures/synthetic/input/`:
//! a name of fiction, a reserved mail domain, a phone number of the range reserved for
//! fiction, an invented street. No target and no seed holds a value of a real person.

use qrtrmastr::adapters::mail::redact::{RedactionTerms, Redactor};

/// The full name. As one text of two words it is a term that no other word holds.
pub const NAME: &str = "Pat Example";
/// The mail address.
pub const EMAIL: &str = "pat.example@example.com";
/// The phone number, digits only, as the configuration holds it.
pub const PHONE: &str = "2125550199";
/// The first line of the delivery address.
pub const ADDRESS: &str = "123 Imaginary Street Apt 4B";

/// The terms of the made-up person, as a configuration file would give them.
#[must_use]
pub fn terms() -> RedactionTerms {
    RedactionTerms {
        names: vec![NAME.to_owned(), "Pat".to_owned()],
        emails: vec![EMAIL.to_owned()],
        phones: vec![PHONE.to_owned()],
        address_lines: vec![ADDRESS.to_owned(), "Brooklyn, NY 11201".to_owned()],
        card_last4: vec!["4242".to_owned()],
    }
}

/// A redactor for the terms above.
///
/// # Panics
///
/// When the terms above are not usable, which is a defect of this file.
#[must_use]
pub fn redactor() -> Redactor {
    Redactor::new(terms()).expect("the terms of the fuzz targets are usable")
}

/// The term that a short text, such as a message key, holds as it is, in any case of its
/// letters; `None` when it holds none. This check is not the redactor's own: it knows the
/// four terms that no other word can hold, and it looks for each as plain text.
#[must_use]
pub fn term_in(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();
    [NAME, EMAIL, PHONE, ADDRESS]
        .into_iter()
        .find(|term| lower.contains(&term.to_lowercase()))
}

/// The term that a redacted body still holds. As [`term_in`], with the one rule of the
/// redactor for a body: the digits of the phone number followed by one more digit are
/// another number there, and stay.
#[must_use]
pub fn term_left_in(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();
    let phone_left = lower.match_indices(PHONE).any(|(at, found)| {
        let after = lower.as_bytes().get(at.saturating_add(found.len()));
        !after.is_some_and(u8::is_ascii_digit)
    });
    if phone_left {
        return Some(PHONE);
    }
    [NAME, EMAIL, ADDRESS]
        .into_iter()
        .find(|term| lower.contains(&term.to_lowercase()))
}
