// Planted violations of the lint levels that `Cargo.toml` declares (REAS-DOC-006).
//
// The module below breaks each rule of the lint table at least once, with no attribute that
// suppresses one of those lints. It is compiled only with `--cfg lint_canary`, which no build
// of the program sets. `scripts/check.sh planted` runs Clippy on this file with that flag and
// fails unless every lint of its list fires here at the level that the manifest declares. So
// a lint that somebody takes out of `Cargo.toml`, or lowers, fails the `rust` job. The one
// `unsafe` block of the repository is here, and the compiler refuses it also with the flag:
// Clippy reports it and builds nothing.
//
// An `#[expect]` on each violation would not do this: `#[expect(lint)]` switches the lint on
// for its own scope, so the expectation holds whatever the manifest says.
//
// This is an example target, so it is code outside the tests: the allowances of `clippy.toml`
// for test code do not apply to it. Without the flag it is an empty program. It is no part
// of the `qm` binary or of the image, and nothing runs it.
//
// To add a lint to `Cargo.toml`, add its violation here and its name to the list in
// `scripts/check.sh`, in the same change.
//
// The crate itself is the planted violation of `missing_docs`: with the flag it has no doc
// comment. The lints `clippy::missing_errors_doc` and `clippy::missing_panics_doc` read
// public functions that are not inside a private module, so their two planted violations
// are at the root of the file and not in the module.
#![cfg_attr(
    not(lint_canary),
    doc = "Planted violations of the lint levels of `Cargo.toml`; see the comment at the head of the file."
)]

#[cfg(lint_canary)]
mod planted {
    pub enum Kind {
        First,
        Second,
        Third,
    }

    pub struct Named;

    impl Named {
        // clippy::same_name_method
        pub fn name(&self) -> &'static str {
            "inherent"
        }
    }

    pub trait HasName {
        fn name(&self) -> &'static str;
    }

    impl HasName for Named {
        fn name(&self) -> &'static str {
            "trait"
        }
    }

    // clippy::wildcard_enum_match_arm
    pub fn wildcard(kind: &Kind) -> u8 {
        match kind {
            Kind::First => 1,
            _ => 0,
        }
    }

    // clippy::needless_pass_by_value
    pub fn by_value(text: String) -> usize {
        text.len()
    }

    // clippy::redundant_clone
    pub fn cloned() -> usize {
        let text = String::from("b");
        let copy = text.clone();
        copy.len()
    }

    // clippy::unwrap_used
    pub fn unwrapped(value: Option<u8>) -> u8 {
        value.unwrap()
    }

    // clippy::expect_used
    pub fn expected(value: Option<u8>) -> u8 {
        value.expect("planted")
    }

    // clippy::allow_attributes
    #[allow(
        clippy::missing_const_for_fn,
        reason = "the attribute is the planted violation"
    )]
    pub fn allowed() -> u8 {
        1
    }

    // clippy::allow_attributes_without_reason
    #[allow(clippy::missing_const_for_fn)]
    pub fn allowed_without_reason() -> u8 {
        2
    }

    // clippy::arithmetic_side_effects
    pub fn added(left: u8, right: u8) -> u8 {
        left + right
    }

    // clippy::indexing_slicing
    pub fn indexed(values: &[u8]) -> u8 {
        values[0]
    }

    // clippy::string_slice
    pub fn cut(text: &str) -> &str {
        &text[1..]
    }

    // unsafe_code, clippy::undocumented_unsafe_blocks, clippy::multiple_unsafe_ops_per_block
    pub fn unsafe_block(values: &[u8]) -> u8 {
        unsafe {
            values
                .get_unchecked(0)
                .unchecked_add(*values.get_unchecked(1))
        }
    }
}

/// Documented, and without the section that says when it fails: `clippy::missing_errors_doc`.
#[cfg(lint_canary)]
pub fn fallible(value: u8) -> Result<u8, u8> {
    if value == 0 { Err(value) } else { Ok(value) }
}

/// Documented, and without the section that says when it panics: `clippy::missing_panics_doc`.
#[cfg(lint_canary)]
pub fn panicking(value: u8) -> u8 {
    assert!(value != 0, "planted");
    value
}

fn main() {
    // Every planted function is called, so that no lint for unused code stands in for the
    // lint that the function is there for.
    #[cfg(lint_canary)]
    {
        use planted::{HasName, Kind, Named};
        let named = Named;
        let values = [1, 2];
        let total = [
            usize::from(planted::wildcard(&Kind::First)),
            usize::from(planted::wildcard(&Kind::Second)),
            usize::from(planted::wildcard(&Kind::Third)),
            Named::name(&named).len(),
            HasName::name(&named).len(),
            planted::by_value(String::from("a")),
            planted::cloned(),
            usize::from(planted::unwrapped(Some(1))),
            usize::from(planted::expected(Some(1))),
            usize::from(planted::allowed()),
            usize::from(planted::allowed_without_reason()),
            usize::from(planted::added(1, 2)),
            usize::from(planted::indexed(&values)),
            planted::cut("ab").len(),
            usize::from(planted::unsafe_block(&values)),
            usize::from(fallible(1).unwrap_or(0)),
            usize::from(panicking(1)),
        ];
        std::hint::black_box(total);
    }
}
