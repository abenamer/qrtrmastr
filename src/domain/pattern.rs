//! Patterns that are written in the source.
//!
//! Every such pattern is compiled through [`fixed`], so one place holds the reason why the
//! compilation cannot fail at run time, and one place sets the limits of the engine. A pattern
//! built from data, such as a configured redaction term, does not come here: its compilation
//! can fail, and the caller returns that as a typed error. The text of the panic below quotes
//! the pattern, which is one more reason: a term must never be in it. The function is private
//! to the crate.

use regex::{Regex, RegexBuilder};

/// The most memory that one compiled pattern may take, in bytes (REAS-SEC-003).
///
/// The engine's own default is ten times this. The largest pattern of the source compiled to
/// about 160 kB when the limit was set, so a pattern that grows past the limit is a pattern
/// that somebody should read again. The redactor uses the same limit for a pattern that it
/// builds from a configured term: there the limit bounds what one term can make the engine
/// allocate, and a term that passes it is refused with its position.
pub(crate) const SIZE_LIMIT: usize = 1 << 20;

/// Compiles a pattern that is a constant of the source.
///
/// The `regex` engine runs in time linear in the text, so a text from a receipt cannot make a
/// match slow, and [`SIZE_LIMIT`] bounds the memory of the compiled pattern (REAS-SEC-003).
///
/// # Panics
///
/// When the pattern is not valid or compiles to more than [`SIZE_LIMIT`]. The pattern is a
/// constant, so that is a defect in the source and not a condition of the input: the first
/// use of the pattern in a test finds it.
#[expect(
    clippy::expect_used,
    reason = "the pattern is a constant of the source; an invalid or oversized one is a bug, found by the first test that uses it"
)]
pub(crate) fn fixed(pattern: &str) -> Regex {
    RegexBuilder::new(pattern)
        .size_limit(SIZE_LIMIT)
        .build()
        .expect("a pattern written in the source is valid and within the size limit")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_size_limit_is_set_on_the_builder_and_refuses_a_pattern_that_passes_it() {
        // A repetition of a large class: a few megabytes when compiled, far below the
        // engine's default limit and above the one of this module.
        let large = r"\w{200}";
        assert!(Regex::new(large).is_ok());
        assert!(matches!(
            RegexBuilder::new(large).size_limit(SIZE_LIMIT).build(),
            Err(regex::Error::CompiledTooBig(limit)) if limit == SIZE_LIMIT
        ));
        // A pattern of the size that the source holds is far inside the limit.
        assert!(fixed(r"(?i)^\s*(\d+(?:\.\d+)?)\s*(lb|lbs|oz|kg|g)\.?\s*$").is_match("1.5 LB"));
    }

    #[test]
    #[should_panic(expected = "within the size limit")]
    fn a_pattern_of_the_source_that_passes_the_limit_stops_the_first_test_that_uses_it() {
        let _ = fixed(r"\w{200}");
    }
}
