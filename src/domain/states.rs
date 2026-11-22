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
    /// The name of the Rust enum that the text was read into, such as `ParseState`.
    pub name: &'static str,
    /// The text that was read, whole. It matches no stored text of that enum.
    pub text: String,
}

macro_rules! text_enum {
    (
        $(#[$doc:meta])*
        $name:ident { $($(#[$variant_doc:meta])* $variant:ident => $text:literal),+ $(,)? }
    ) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $($(#[$variant_doc])* $variant),+
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
    };
}

text_enum!(
    /// Where a message came from: the grocer, or one of the restaurant platforms whose receipts
    /// the import stores (ADR 0007). A new source adds a value, not a table.
    Platform {
        /// FreshDirect, the grocer. The only platform whose receipts a parser reads.
        Freshdirect => "freshdirect",
        /// Seamless, a restaurant platform. Its mail is stored and not parsed yet.
        Seamless => "seamless",
        /// Grubhub, a restaurant platform. Its mail is stored and not parsed yet.
        Grubhub => "grubhub",
        /// Toast, a restaurant platform. Its mail is stored and not parsed yet.
        Toast => "toast",
        /// Square, a restaurant platform. Its mail is stored and not parsed yet.
        Square => "square",
        /// Clover, a restaurant platform. Its mail is stored and not parsed yet.
        Clover => "clover",
        /// DoorDash, a restaurant platform. Its mail is stored and not parsed yet.
        Doordash => "doordash",
    }
);

text_enum!(
    /// Which of a platform's mails this is. `Unknown` is stored, never skipped.
    MessageKind {
        /// The mail that confirms that an order was placed.
        Placed => "placed",
        /// The mail that says that a placed order changed.
        Updated => "updated",
        /// The receipt of an order. The parse step reads this kind only.
        Receipt => "receipt",
        /// Any other mail of the platform, such as an offer: the subject matched no rule.
        Unknown => "unknown",
    }
);

text_enum!(
    /// How a raw message's key was made.
    KeyKind {
        /// The key is the `Message-ID` header of the mail.
        MessageId => "message_id",
        /// The key is the SHA-256 of the raw bytes: the `Message-ID` header is missing, is
        /// empty, or carries a configured name, mail address, address line or phone number.
        Sha256 => "sha256",
    }
);

text_enum!(
    /// The lifecycle of a raw message. `Fetched` is the only start state; the four others are
    /// outcomes of a parse attempt and can be reached again by a reparse.
    ParseState {
        /// The message is stored, and no parser ran over it yet.
        Fetched => "fetched",
        /// The last attempt stored an order, and the order reconciled.
        Parsed => "parsed",
        /// The last attempt stored an order that did not reconcile.
        Partial => "partial",
        /// The last attempt stored no order: the message has no stored HTML body, or the
        /// parser that recognized the layout could not read the receipt, or the amounts of the
        /// receipt cannot be added.
        Failed => "failed",
        /// The last attempt stored no order: no parser recognized the layout.
        Unsupported => "unsupported",
    }
);

text_enum!(
    /// The result of one parser run over one message, as `parse_attempts.outcome` stores it.
    ParseOutcome {
        /// The parser read the receipt, and its amounts reconciled.
        Ok => "ok",
        /// The parser read the receipt, and at least one check of the amounts did not hold.
        Partial => "partial",
        /// No receipt was read: the message has no stored HTML body, or the parser that
        /// recognized the layout returned an error, or the amounts of the receipt that it read
        /// cannot be added.
        Failed => "failed",
        /// No receipt was read: no parser recognized the layout.
        Unsupported => "unsupported",
    }
);

text_enum!(
    /// Whether Gmail recorded a DKIM pass for the sender of the receipt an order came from.
    /// A receipt without one is parsed all the same; its order carries the tag (ADR 0008).
    SenderCheck {
        /// The receipt has a DKIM pass.
        Verified => "verified",
        /// No DKIM pass was recorded for the receipt.
        NonVerified => "non-verified",
    }
);

impl SenderCheck {
    /// The tag of an order, from the `dkim_pass` of the receipt it came from. `false` gives
    /// `NonVerified`: it says that no pass was recorded, not why.
    pub fn from_dkim_pass(passed: bool) -> Self {
        if passed {
            SenderCheck::Verified
        } else {
            SenderCheck::NonVerified
        }
    }
}

text_enum!(
    /// How an alias came to point at its product.
    AliasSource {
        /// The parse step made the alias from a line of a receipt.
        Auto => "auto",
        /// A person pointed the alias at its product. No command writes this value yet.
        Manual => "manual",
    }
);

text_enum!(
    /// Whether a product is priced by the piece or by weight.
    SoldBy {
        /// By the piece: the line that made the product has no weight, and its unit price, when
        /// it has one, is for one piece.
        Each => "each",
        /// By weight: the line that made the product has a weight, or its unit price is for one
        /// pound.
        Weight => "weight",
    }
);

text_enum!(
    /// Why the user adds an item to the next order by hand (ADR 0011). `OutOfStock` is the
    /// user's own stock: none is left at home. `RunningLow`: some is left, not for long.
    AddReason {
        /// None is left at home.
        OutOfStock => "out-of-stock",
        /// Some is left at home, not for long.
        RunningLow => "running-low",
        /// A recipe needs the item.
        Recipe => "recipe",
        /// A food that the user wants to try.
        NewFood => "new-food",
        /// A favorite that the user wants again.
        RepeatFavorite => "repeat-favorite",
    }
);

impl AddReason {
    /// The reason as a person says it, for a prompt and a list.
    pub fn phrase(self) -> &'static str {
        match self {
            AddReason::OutOfStock => "ran out, need to replace",
            AddReason::RunningLow => "almost running out",
            AddReason::Recipe => "making a recipe",
            AddReason::NewFood => "new food to try",
            AddReason::RepeatFavorite => "repeat favorite",
        }
    }
}

/// A transition the current state does not permit (REAS-STATE-001).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TransitionError {
    /// A first parse was asked of a message that already has the outcome of an attempt. Holds
    /// the state the message is in.
    #[error("a first parse needs a fetched message; this one is {0}")]
    NotFetched(ParseState),
    /// A reparse was asked of a message that no parser ran over yet. Holds the state the
    /// message is in, which is `Fetched`.
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
    ///
    /// # Errors
    ///
    /// [`TransitionError::NotFetched`] when the state is `Parsed`, `Partial`, `Failed` or
    /// `Unsupported`.
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
    ///
    /// # Errors
    ///
    /// [`TransitionError::NeverParsed`] when the state is `Fetched`.
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
        assert_round_trip!(SenderCheck);
        assert_eq!(SenderCheck::from_dkim_pass(false).as_str(), "non-verified");
        assert_round_trip!(SoldBy);
        assert_round_trip!(AddReason);
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
