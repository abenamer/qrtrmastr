//! Patterns that are written in the source.
//!
//! Every such pattern is compiled through [`fixed`], so one place holds the reason why the
//! compilation cannot fail at run time, and one place sets the limits of the engine. A pattern
//! built from data, such as a configured redaction term, does not come here: its compilation
//! can fail, and the caller returns that as a typed error. The text of the panic below quotes
//! the pattern, which is one more reason: a term must never be in it. The function is private
//! to the crate.

use regex::Regex;

/// Compiles a pattern that is a constant of the source.
///
/// The `regex` engine runs in time linear in the text, so a text from a receipt cannot make a
/// match slow (REAS-SEC-003).
///
/// # Panics
///
/// When the pattern is not valid. The pattern is a constant, so that is a defect in the
/// source and not a condition of the input: the first use of the pattern in a test finds it.
#[expect(
    clippy::expect_used,
    reason = "the pattern is a constant of the source; an invalid one is a bug, found by the first test that uses it"
)]
pub(crate) fn fixed(pattern: &str) -> Regex {
    Regex::new(pattern).expect("a pattern written in the source is valid")
}
