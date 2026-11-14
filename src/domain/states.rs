//! Stored labels and runtime states as enums (REAS-RUST-001, REAS-STATE-001).
//!
//! `text_enum!` gives a label its stored text, `FromStr`, `Display` and `ALL`, so a check
//! constraint in a migration and the Rust enum list the same values. A lifecycle is one enum
//! with a function per legal transition, never a set of Boolean flags.

use thiserror::Error;

/// A stored text that matches no variant of the enum it was read into.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("{text:?} is not a {name}")]
pub struct UnknownText {
    pub name: &'static str,
    pub text: String,
}

macro_rules! text_enum {
    ($(#[$doc:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            /// Every variant, in declaration order; the list a check constraint must admit.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The text stored in PostgreSQL.
            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $text),+
                }
            }
        }

        impl std::str::FromStr for $name {
            type Err = UnknownText;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                match text {
                    $($text => Ok($name::$variant),)+
                    other => Err(UnknownText { name: stringify!($name), text: other.to_owned() }),
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }
    };
}

text_enum!(
    /// Where a message came from. One value today; the column exists so the restaurant
    /// platforms of the deferred design add a value, not a table.
    Platform { Freshdirect => "freshdirect" }
);

text_enum!(
    /// Which of a platform's mails this is. `Unknown` is stored, never skipped.
    MessageKind {
        Placed => "placed",
        Updated => "updated",
        Receipt => "receipt",
        Unknown => "unknown",
    }
);

text_enum!(
    /// How a raw message's key was made.
    KeyKind {
        MessageId => "message_id",
        Sha256 => "sha256",
    }
);

text_enum!(
    /// The lifecycle of a raw message. `Fetched` is the only start state; the four others are
    /// outcomes of a parse attempt and can be reached again by a reparse.
    ParseState {
        Fetched => "fetched",
        Parsed => "parsed",
        Partial => "partial",
        Failed => "failed",
        Unsupported => "unsupported",
    }
);

text_enum!(
    /// The result of one parser run over one message, as `parse_attempts.outcome` stores it.
    ParseOutcome {
        Ok => "ok",
        Partial => "partial",
        Failed => "failed",
        Unsupported => "unsupported",
    }
);

text_enum!(
    /// How an alias came to point at its product.
    AliasSource {
        Auto => "auto",
        Manual => "manual",
    }
);

text_enum!(
    /// Whether a product is priced by the piece or by weight.
    SoldBy {
        Each => "each",
        Weight => "weight",
    }
);

/// A transition the current state does not permit (REAS-STATE-001).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TransitionError {
    #[error("a first parse needs a fetched message; this one is {0}")]
    NotFetched(ParseState),
    #[error("a reparse needs a message that was parsed before; this one is {0}")]
    NeverParsed(ParseState),
}

impl ParseOutcome {
    /// The state a message is in after an attempt with this outcome.
    pub fn into_state(self) -> ParseState {
        match self {
            ParseOutcome::Ok => ParseState::Parsed,
            ParseOutcome::Partial => ParseState::Partial,
            ParseOutcome::Failed => ParseState::Failed,
            ParseOutcome::Unsupported => ParseState::Unsupported,
        }
    }
}

impl ParseState {
    /// The first attempt over a message. Legal from `Fetched` only.
    pub fn first_parse(self, outcome: ParseOutcome) -> Result<ParseState, TransitionError> {
        match self {
            ParseState::Fetched => Ok(outcome.into_state()),
            ParseState::Parsed
            | ParseState::Partial
            | ParseState::Failed
            | ParseState::Unsupported => Err(TransitionError::NotFetched(self)),
        }
    }

    /// A later attempt with a newer parser. Legal from every outcome state, never from `Fetched`.
    pub fn reparse(self, outcome: ParseOutcome) -> Result<ParseState, TransitionError> {
        match self {
            ParseState::Fetched => Err(TransitionError::NeverParsed(self)),
            ParseState::Parsed
            | ParseState::Partial
            | ParseState::Failed
            | ParseState::Unsupported => Ok(outcome.into_state()),
        }
    }

    /// Whether an order may exist for a message in this state.
    pub fn has_order(self) -> bool {
        match self {
            ParseState::Parsed | ParseState::Partial => true,
            ParseState::Fetched | ParseState::Failed | ParseState::Unsupported => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! assert_round_trip {
        ($name:ident) => {
            for variant in $name::ALL {
                let text = variant.as_str();
                assert_eq!(text.parse::<$name>(), Ok(*variant), "{text}");
                assert_eq!(variant.to_string(), text);
            }
            assert_eq!(
                "bogus".parse::<$name>(),
                Err(UnknownText {
                    name: stringify!($name),
                    text: "bogus".to_owned()
                })
            );
        };
    }

    #[test]
    fn every_label_round_trips_through_its_text() {
        assert_round_trip!(Platform);
        assert_round_trip!(MessageKind);
        assert_round_trip!(KeyKind);
        assert_round_trip!(ParseState);
        assert_round_trip!(ParseOutcome);
        assert_round_trip!(AliasSource);
        assert_round_trip!(SoldBy);
    }

    #[test]
    fn a_first_parse_leaves_fetched_and_only_fetched() {
        assert_eq!(
            ParseState::Fetched.first_parse(ParseOutcome::Ok),
            Ok(ParseState::Parsed)
        );
        assert_eq!(
            ParseState::Fetched.first_parse(ParseOutcome::Partial),
            Ok(ParseState::Partial)
        );
        assert_eq!(
            ParseState::Parsed.first_parse(ParseOutcome::Ok),
            Err(TransitionError::NotFetched(ParseState::Parsed))
        );
    }

    #[test]
    fn a_reparse_needs_an_earlier_attempt() {
        assert_eq!(
            ParseState::Failed.reparse(ParseOutcome::Ok),
            Ok(ParseState::Parsed)
        );
        assert_eq!(
            ParseState::Parsed.reparse(ParseOutcome::Unsupported),
            Ok(ParseState::Unsupported)
        );
        assert_eq!(
            ParseState::Fetched.reparse(ParseOutcome::Ok),
            Err(TransitionError::NeverParsed(ParseState::Fetched))
        );
    }

    #[test]
    fn only_parsed_and_partial_messages_carry_an_order() {
        assert!(ParseState::Parsed.has_order());
        assert!(ParseState::Partial.has_order());
        assert!(!ParseState::Fetched.has_order());
        assert!(!ParseState::Failed.has_order());
        assert!(!ParseState::Unsupported.has_order());
    }
}
