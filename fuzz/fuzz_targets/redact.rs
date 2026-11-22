//! Fuzz target for the redactor (REAS-TEST-003): `Redactor::redact_html`,
//! `Redactor::redact_text` and `Redactor::carries_configured_term`, which read the subject
//! and the bodies of a mail before anything is stored.

#![no_main]

use std::sync::LazyLock;

use libfuzzer_sys::fuzz_target;
use qrtrmastr::adapters::mail::redact::{RedactionError, Redactor};
use qrtrmastr_fuzz::{redactor, term_in, term_left_in};

static REDACTOR: LazyLock<Redactor> = LazyLock::new(redactor);

/// Property: the redactor gives a text that holds no configured term as it is, or the one
/// error that says that a term survived. No other error can come from a text.
fn check(what: &str, result: Result<String, RedactionError>) {
    match result {
        Ok(redacted) => assert_eq!(term_left_in(&redacted), None, "in the redacted {what}"),
        Err(RedactionError::Survived { .. }) => {}
        Err(error) => panic!("the {what} gave an error of the terms: {error}"),
    }
}

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    check("text", REDACTOR.redact_text(&text));
    check("HTML", REDACTOR.redact_html(&text));
    // Property: a text with nothing to redact is not refused. It has no digit and no `@`, so
    // it is no phone number, card number or address, and it carries no configured term; the
    // one error of a text is for a term that survived, and none is there.
    let nothing_to_redact = !text.contains(|c: char| c.is_ascii_digit() || c == '@' || c == '%')
        && !REDACTOR.carries_configured_term(&text);
    if nothing_to_redact {
        assert!(REDACTOR.redact_text(&text).is_ok(), "a refusal of {text:?}");
    }
    // Property: the check of a short text gives an answer for any text, and a term as it is
    // is found.
    if term_in(&text).is_some() {
        assert!(REDACTOR.carries_configured_term(&text));
    } else {
        let _ = REDACTOR.carries_configured_term(&text);
    }
});
