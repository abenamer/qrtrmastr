//! Identifier newtypes (REAS-RUST-001). Each has a private field and a smart constructor, so a
//! value that exists is valid, and a bare `i64` or `String` never crosses a port.

use std::fmt;

use thiserror::Error;

use super::states::KeyKind;

/// Why a raw value is not an identifier.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum IdError {
    #[error("an order number is 11 or 13 ASCII digits, not {0:?}")]
    OrderNumber(String),
    #[error("a message key must not be empty")]
    EmptyMessageKey,
    #[error("a {kind} id is positive, not {value}")]
    NonPositive { kind: &'static str, value: i64 },
}

/// A FreshDirect order number: eleven digits until August 2026, thirteen after. Text, never a
/// number, because nobody adds order numbers and a leading zero must survive.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OrderNumber(String);

impl OrderNumber {
    /// Accepts the digits with surrounding whitespace and nothing else.
    pub fn parse(raw: &str) -> Result<Self, IdError> {
        let trimmed = raw.trim();
        let all_digits = !trimmed.is_empty() && trimmed.bytes().all(|b| b.is_ascii_digit());
        if all_digits && matches!(trimmed.len(), 11 | 13) {
            Ok(Self(trimmed.to_owned()))
        } else {
            Err(IdError::OrderNumber(trimmed.to_owned()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OrderNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a raw message is stored under, and the key every import path dedupes on. The RFC 5322
/// `Message-ID` header when the message has one, which a Takeout export and the Gmail API both
/// carry, so the two paths dedupe against each other; else the SHA-256 of the raw bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MessageKey {
    MessageId(String),
    Sha256(String),
}

impl MessageKey {
    /// Normalizes a `Message-ID` header value: whitespace and the surrounding angle brackets go.
    pub fn from_message_id(raw: &str) -> Result<Self, IdError> {
        let inner = raw
            .trim()
            .trim_start_matches('<')
            .trim_end_matches('>')
            .trim();
        if inner.is_empty() {
            return Err(IdError::EmptyMessageKey);
        }
        Ok(Self::MessageId(inner.to_owned()))
    }

    /// The fallback key: the hex SHA-256 of the message bytes as they were read.
    pub fn from_sha256(digest: &[u8; 32]) -> Self {
        Self::Sha256(hex::encode(digest))
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::MessageId(key) | Self::Sha256(key) => key,
        }
    }

    pub fn kind(&self) -> KeyKind {
        match self {
            Self::MessageId(_) => KeyKind::MessageId,
            Self::Sha256(_) => KeyKind::Sha256,
        }
    }
}

impl fmt::Display for MessageKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind(), self.as_str())
    }
}

macro_rules! row_id {
    ($(#[$doc:meta])* $name:ident, $kind:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(i64);

        impl $name {
            /// A database identity column is positive; anything else is a bug upstream.
            pub fn new(value: i64) -> Result<Self, IdError> {
                if value > 0 {
                    Ok(Self(value))
                } else {
                    Err(IdError::NonPositive { kind: $kind, value })
                }
            }

            pub fn get(self) -> i64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
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
    fn a_sha_key_is_lowercase_hex() {
        let key = MessageKey::from_sha256(&[0xab; 32]);
        assert_eq!(key.kind(), KeyKind::Sha256);
        assert_eq!(key.as_str().len(), 64);
        assert!(key.as_str().starts_with("abab"));
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
