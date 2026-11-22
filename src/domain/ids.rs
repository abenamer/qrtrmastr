//! Identifier newtypes (REAS-RUST-001). Each has a private field and a smart constructor, so a
//! value that exists is valid, and a bare `i64` or `String` never crosses a port.

use std::fmt;

use thiserror::Error;

use super::states::KeyKind;

/// Why a raw value is not an identifier.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum IdError {
    /// The text is not 11 or 13 ASCII digits. Holds the text that was refused, without its
    /// surrounding whitespace.
    #[error("an order number is 11 or 13 ASCII digits, not {0:?}")]
    OrderNumber(String),
    /// A `Message-ID` header value holds nothing between its whitespace and angle brackets,
    /// or the text of a stored or typed `Message-ID` key is empty.
    #[error("a message key must not be empty")]
    EmptyMessageKey,
    /// The text of a stored or typed `Message-ID` key starts or ends with whitespace. The
    /// text is not in the error: it comes from a mail.
    #[error("a message key has no whitespace at its start or its end")]
    PaddedMessageKey,
    /// A `Message-ID` header value is longer than [`MESSAGE_ID_MAX_BYTES`] and is no key. The
    /// text is not in the error.
    #[error("a message key is at most 998 bytes")]
    LongMessageKey,
    /// The text of a `Message-ID` key holds a NUL character, which no store of text holds.
    /// The text is not in the error.
    #[error("a message key holds no NUL character")]
    NulInMessageKey,
    /// The text of a stored or typed SHA-256 key is not 64 lowercase hexadecimal digits. The
    /// text is not in the error.
    #[error("a sha256 message key is 64 lowercase hexadecimal digits")]
    NotSha256,
    /// A row identifier is zero or negative. A database identity column starts at 1.
    #[error("a {kind} id is positive, not {value}")]
    NonPositive {
        /// Which identifier was refused, as the error text names it: `raw message`, `product`,
        /// `addition` or `alias`.
        kind: &'static str,
        /// The number that was refused.
        value: i64,
    },
}

/// A FreshDirect order number: eleven digits until August 2026, thirteen after. Text, never a
/// number, because nobody adds order numbers and a leading zero must survive.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OrderNumber(String);

impl OrderNumber {
    /// Accepts the digits with surrounding whitespace and nothing else.
    ///
    /// # Errors
    ///
    /// [`IdError::OrderNumber`] when the text without its surrounding whitespace is not 11 or
    /// 13 ASCII digits.
    pub fn parse(raw: &str) -> Result<Self, IdError> {
        let trimmed = raw.trim();
        let all_digits = !trimmed.is_empty() && trimmed.bytes().all(|b| b.is_ascii_digit());
        if all_digits && matches!(trimmed.len(), 11 | 13) {
            Ok(Self(trimmed.to_owned()))
        } else {
            Err(IdError::OrderNumber(trimmed.to_owned()))
        }
    }

    /// The digits as the receipt prints them, leading zeros kept. This text is the primary key
    /// `grocery_orders.order_number`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OrderNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The most bytes that the text of a `Message-ID` key holds: the longest line that RFC 5322
/// permits in a mail. A real `Message-ID` is far shorter. The store keeps a unique index on
/// the key, and an index entry has a size limit of its own, about 2,700 bytes in PostgreSQL;
/// a header from a mail is outside data, so the limit is checked where the key is made
/// (REAS-SEC-001).
pub const MESSAGE_ID_MAX_BYTES: usize = 998;

/// What a raw message is stored under, and the key every import path dedupes on. The RFC 5322
/// `Message-ID` header when the message has one, which a Takeout export and the Gmail API both
/// carry, so the two paths dedupe against each other; else the SHA-256 of the raw bytes.
///
/// The fields are private. A key that exists has a text that fits its kind: a `Message-ID`
/// text is not empty, has no whitespace at either end and holds no NUL character, and a
/// SHA-256 text is 64 lowercase hexadecimal digits. A key made from a mail is also at most
/// [`MESSAGE_ID_MAX_BYTES`] long. A key read back from the store can be longer: a version
/// before 2026-11-21 had no limit, and a key that the store holds is one that it can hold. A key read back from the store or typed on the command line goes
/// through [`MessageKey::from_parts`], so it is checked like a key made from a mail
/// (REAS-SEC-002).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MessageKey {
    /// How the key was made.
    kind: KeyKind,
    /// The key text, without its kind.
    text: String,
}

impl MessageKey {
    /// Normalizes a `Message-ID` header value: whitespace and the surrounding angle brackets go.
    ///
    /// # Errors
    ///
    /// - [`IdError::EmptyMessageKey`] when nothing is left after the whitespace and the angle
    ///   brackets are removed.
    /// - [`IdError::LongMessageKey`] when what is left is longer than
    ///   [`MESSAGE_ID_MAX_BYTES`].
    /// - [`IdError::NulInMessageKey`] when what is left holds a NUL character.
    pub fn from_message_id(raw: &str) -> Result<Self, IdError> {
        let inner = raw
            .trim()
            .trim_start_matches('<')
            .trim_end_matches('>')
            .trim();
        Self::check_message_id_text(inner)?;
        if inner.len() > MESSAGE_ID_MAX_BYTES {
            return Err(IdError::LongMessageKey);
        }
        Ok(Self {
            kind: KeyKind::MessageId,
            text: inner.to_owned(),
        })
    }

    /// The checks of a `Message-ID` text that the two constructors share. The whitespace at
    /// an end and the length are the business of each constructor: one removes the
    /// whitespace and has the limit, the other refuses the whitespace and has no limit.
    fn check_message_id_text(text: &str) -> Result<(), IdError> {
        if text.is_empty() {
            return Err(IdError::EmptyMessageKey);
        }
        if text.contains('\0') {
            return Err(IdError::NulInMessageKey);
        }
        Ok(())
    }

    /// The fallback key: the hex SHA-256 of the message bytes as they were read.
    pub fn from_sha256(digest: &[u8; 32]) -> Self {
        Self {
            kind: KeyKind::Sha256,
            text: hex::encode(digest),
        }
    }

    /// A key from its kind and its text as they are stored: `raw_messages.key_kind` and
    /// `raw_messages.message_key`. The text is taken as it is and nothing is removed from it,
    /// so a key that [`MessageKey::from_message_id`] or [`MessageKey::from_sha256`] made comes
    /// back equal to itself. The text of a `Message-ID` key has no limit of length here: a
    /// row that an older version wrote with a longer key is read, and does not stop a run.
    ///
    /// # Errors
    ///
    /// - [`IdError::EmptyMessageKey`] when `kind` is [`KeyKind::MessageId`] and the text is
    ///   empty.
    /// - [`IdError::PaddedMessageKey`] when `kind` is [`KeyKind::MessageId`] and the text
    ///   starts or ends with whitespace.
    /// - [`IdError::NulInMessageKey`] when `kind` is [`KeyKind::MessageId`] and the text holds
    ///   a NUL character.
    /// - [`IdError::NotSha256`] when `kind` is [`KeyKind::Sha256`] and the text is not 64
    ///   characters, each a digit or a lowercase letter from `a` to `f`.
    pub fn from_parts(kind: KeyKind, text: &str) -> Result<Self, IdError> {
        match kind {
            KeyKind::MessageId => {
                if text.is_empty() {
                    return Err(IdError::EmptyMessageKey);
                }
                if text.trim() != text {
                    return Err(IdError::PaddedMessageKey);
                }
                Self::check_message_id_text(text)?;
            }
            KeyKind::Sha256 => {
                let hex_digit = |b: u8| b.is_ascii_digit() || (b'a'..=b'f').contains(&b);
                if text.len() != 64 || !text.bytes().all(hex_digit) {
                    return Err(IdError::NotSha256);
                }
            }
        }
        Ok(Self {
            kind,
            text: text.to_owned(),
        })
    }

    /// The key text alone, without its kind: what `raw_messages.message_key` stores. The
    /// `Display` form puts the kind before it.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// How the key was made, as the label that is stored beside the key text. A store needs
    /// both to read the key back as the same key.
    pub fn kind(&self) -> KeyKind {
        self.kind
    }
}

impl fmt::Display for MessageKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind, self.text)
    }
}

macro_rules! row_id {
    ($(#[$doc:meta])* $name:ident, $kind:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(i64);

        impl $name {
            /// A database identity column is positive; anything else is a bug upstream.
            ///
            /// # Errors
            ///
            /// [`IdError::NonPositive`] when `value` is zero or negative.
            pub fn new(value: i64) -> Result<Self, IdError> {
                if value > 0 {
                    Ok(Self(value))
                } else {
                    Err(IdError::NonPositive { kind: $kind, value })
                }
            }

            /// The number of the row, always positive, to bind to a query or to print.
            pub fn get(self) -> i64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            /// The number. A width and an alignment of the caller apply, so that a table
            /// can put the number in a column.
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

row_id!(
    /// A `raw_messages.id`.
    RawMessageId,
    "raw message"
);
row_id!(
    /// A `products.id`.
    ProductId,
    "product"
);
row_id!(
    /// A `list_additions.id`.
    AdditionId,
    "addition"
);
row_id!(
    /// A `product_aliases.id`.
    AliasId,
    "alias"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_order_number_is_eleven_or_thirteen_digits() {
        assert_eq!(
            OrderNumber::parse(" 12345678901 ").map(|n| n.to_string()),
            Ok("12345678901".to_owned())
        );
        assert!(OrderNumber::parse("1234567890123").is_ok());
        assert_eq!(
            OrderNumber::parse("123456789012"),
            Err(IdError::OrderNumber("123456789012".to_owned()))
        );
        assert!(OrderNumber::parse("1234567890a").is_err());
        assert!(OrderNumber::parse("").is_err());
    }

    #[test]
    fn a_message_id_loses_its_brackets_and_keeps_its_kind() {
        let key = MessageKey::from_message_id(" <abc@mail.example> ").expect("valid header");
        assert_eq!(key.as_str(), "abc@mail.example");
        assert_eq!(key.kind(), KeyKind::MessageId);
        assert_eq!(key.to_string(), "message_id:abc@mail.example");
        assert_eq!(
            MessageKey::from_message_id("<>"),
            Err(IdError::EmptyMessageKey)
        );
    }

    #[test]
    fn a_message_id_key_is_at_most_998_bytes_and_holds_no_nul_character() {
        // The limit, through each of the two constructors: at it, and one byte above it.
        let at = "a".repeat(MESSAGE_ID_MAX_BYTES);
        let above = "a".repeat(MESSAGE_ID_MAX_BYTES + 1);
        assert_eq!(MESSAGE_ID_MAX_BYTES, 998);
        assert_eq!(
            MessageKey::from_message_id(&format!(" <{at}> ")).map(|key| key.as_str().len()),
            Ok(998)
        );
        assert_eq!(
            MessageKey::from_message_id(&format!("<{above}>")),
            Err(IdError::LongMessageKey)
        );
        // A stored key has no limit: a version before 2026-11-21 wrote a longer key when the
        // index could hold it, and such a row is read and does not stop a run.
        assert!(MessageKey::from_parts(KeyKind::MessageId, &at).is_ok());
        assert_eq!(
            MessageKey::from_parts(KeyKind::MessageId, &above).map(|key| key.as_str().len()),
            Ok(999)
        );
        // The limit counts bytes: 499 letters of two bytes each are at it, 500 are above.
        assert!(MessageKey::from_message_id(&"\u{e9}".repeat(499)).is_ok());
        assert_eq!(
            MessageKey::from_message_id(&"\u{e9}".repeat(500)),
            Err(IdError::LongMessageKey)
        );
        // A NUL character, at the start, in the middle and at the end. At an end it is not
        // whitespace, so no constructor removes it.
        for text in ["\0a@mail.example", "a\0b@mail.example", "a@mail.example\0"] {
            assert_eq!(
                MessageKey::from_message_id(text),
                Err(IdError::NulInMessageKey),
                "{text:?}"
            );
            assert_eq!(
                MessageKey::from_parts(KeyKind::MessageId, text),
                Err(IdError::NulInMessageKey),
                "{text:?}"
            );
        }
        // No error holds the text.
        assert_eq!(
            IdError::LongMessageKey.to_string(),
            "a message key is at most 998 bytes"
        );
        assert_eq!(
            IdError::NulInMessageKey.to_string(),
            "a message key holds no NUL character"
        );
    }

    #[test]
    fn a_sha_key_is_lowercase_hex() {
        let key = MessageKey::from_sha256(&[0xab; 32]);
        assert_eq!(key.kind(), KeyKind::Sha256);
        assert_eq!(key.as_str().len(), 64);
        assert!(key.as_str().starts_with("abab"));
    }

    #[test]
    fn a_key_from_its_parts_is_the_key_that_was_stored() {
        // What each constructor makes comes back equal through its stored kind and text.
        let from_header = MessageKey::from_message_id("<abc@mail.example>").expect("a header");
        assert_eq!(
            MessageKey::from_parts(from_header.kind(), from_header.as_str()),
            Ok(from_header.clone())
        );
        let from_bytes = MessageKey::from_sha256(&[0x0f; 32]);
        assert_eq!(
            MessageKey::from_parts(from_bytes.kind(), from_bytes.as_str()),
            Ok(from_bytes.clone())
        );
        // One text under two kinds is two keys.
        let hex = "0f".repeat(32);
        let as_header = MessageKey::from_parts(KeyKind::MessageId, &hex).expect("a key");
        assert_eq!(as_header.as_str(), from_bytes.as_str());
        assert_ne!(as_header, from_bytes);
        // Nothing is removed: a bracket that the header form drops stays.
        assert_eq!(
            MessageKey::from_parts(KeyKind::MessageId, "<a").map(|key| key.as_str().to_owned()),
            Ok("<a".to_owned())
        );
    }

    #[test]
    fn a_key_that_a_mail_gives_is_a_key_that_the_store_gives_back() {
        // A header is outside data: whatever it holds, the key that the import makes of it
        // and stores must come back from its two columns as the same key. A key that did not
        // would stop every later import at the list of the known keys.
        proptest::proptest!(|(raw in "\\PC{0,40}", pad in "[ \t\r\n\u{a0}\u{2028}\u{3000}<>]{0,3}")| {
            for header in [raw.clone(), format!("{pad}{raw}{pad}"), format!("<{pad}{raw}{pad}>")] {
                if let Ok(key) = MessageKey::from_message_id(&header) {
                    proptest::prop_assert_eq!(
                        MessageKey::from_parts(key.kind(), key.as_str()),
                        Ok(key),
                        "{:?}", header
                    );
                }
            }
        });
        proptest::proptest!(|(digest in proptest::array::uniform32(proptest::num::u8::ANY))| {
            let key = MessageKey::from_sha256(&digest);
            proptest::prop_assert_eq!(MessageKey::from_parts(key.kind(), key.as_str()), Ok(key));
        });
    }

    #[test]
    fn a_message_id_key_from_its_parts_is_not_empty_and_not_padded() {
        assert_eq!(
            MessageKey::from_parts(KeyKind::MessageId, ""),
            Err(IdError::EmptyMessageKey)
        );
        for padded in [
            " a@mail.example",
            "a@mail.example ",
            "a@mail.example\n",
            "\u{a0}a",
            " ",
        ] {
            assert_eq!(
                MessageKey::from_parts(KeyKind::MessageId, padded),
                Err(IdError::PaddedMessageKey),
                "{padded:?}"
            );
        }
        // Whitespace inside the text is not at an end.
        assert!(MessageKey::from_parts(KeyKind::MessageId, "a b").is_ok());
    }

    #[test]
    fn a_sha256_key_from_its_parts_is_64_lowercase_hex_digits() {
        let good = "0123456789abcdef".repeat(4);
        assert!(MessageKey::from_parts(KeyKind::Sha256, &good).is_ok());
        let short = "a".repeat(63);
        let long = "a".repeat(65);
        let upper = "A".repeat(64);
        let other_letter = "g".repeat(64);
        // 64 characters and more than 64 bytes: the length is counted in bytes, and a
        // character outside ASCII is refused in either count.
        let not_ascii = "é".repeat(64);
        let wide = "é".repeat(32);
        for bad in ["", &short, &long, &upper, &other_letter, &not_ascii, &wide] {
            assert_eq!(
                MessageKey::from_parts(KeyKind::Sha256, bad),
                Err(IdError::NotSha256),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_row_id_takes_the_width_of_its_column() {
        // Before 2026-11-21 the width was lost, and the numbers of the additions in the
        // table of `qm due` were not in a column.
        let id = AdditionId::new(7).expect("id");
        assert_eq!(format!("{id}"), "7");
        assert_eq!(format!("{id:>4}"), "   7");
    }

    #[test]
    fn a_row_id_is_positive() {
        assert_eq!(ProductId::new(7).map(ProductId::get), Ok(7));
        assert_eq!(
            AliasId::new(0),
            Err(IdError::NonPositive {
                kind: "alias",
                value: 0
            })
        );
        assert!(RawMessageId::new(-1).is_err());
    }
}
