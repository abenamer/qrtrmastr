//! Trust boundary example.
//!
//! One inbound adapter, [`inbound::receive`], takes outside bytes and returns
//! a domain event or a typed error. The steps inside it are the seams that
//! the security rules cover:
//!
//! 1. [`frame::Frame::parse`] reads a length prefix with checked arithmetic
//!    and compares it with a limit before it allocates (REAS-SEC-003,
//!    REAS-SEC-001).
//! 2. [`webhook::Webhook::verify`] turns a `Webhook<Unverified>` into a
//!    `Webhook<Verified>` through the [`webhook::TagVerifier`] port. The
//!    handler accepts only the verified type (REAS-STATE-002).
//! 3. [`event::Event`] is decoded after verification. [`event::EventName`]
//!    validates in `TryFrom`, and serde uses the same path (REAS-SEC-002).
//! 4. [`checksum::fast`] duplicates [`checksum::reference`] with a
//!    differential test (REAS-SEC-005).
//!
//! `Cargo.toml` forbids `unsafe` code, denies unchecked arithmetic and
//! indexing, and enables overflow checks in release (REAS-SEC-004,
//! REAS-SEC-003). The `fuzz/` crate holds one fuzz target (REAS-TEST-003).
//!
//! This example does not implement a message authentication code. Put a
//! reviewed implementation, for example `hmac` with `sha2`, behind the
//! [`webhook::TagVerifier`] port. [`doubles::XorTagVerifier`] is a test
//! double that anyone can forge.
//!
//! A verified webhook can be handled:
//!
//! ```
//! use reas_trust_boundary::doubles::XorTagVerifier;
//! use reas_trust_boundary::frame::Frame;
//! use reas_trust_boundary::inbound::receive;
//!
//! let verifier = XorTagVerifier::new(0x5a);
//! let payload = br#"{"name":"order.paid","attempt":1}"#;
//! let frame = Frame::new(payload.to_vec(), verifier.tag_for(payload)).expect("within the limit");
//! let event = receive(&frame.to_bytes(), &verifier).expect("valid frame");
//! assert_eq!(event.name.as_str(), "order.paid");
//! assert_eq!(event.attempt, 1);
//! ```
//!
//! An unverified webhook cannot be handled. The handler takes
//! `Webhook<Verified>`, so this is error E0308 (mismatched types):
//!
//! ```compile_fail,E0308
//! use reas_trust_boundary::webhook::{handle, Unverified, Webhook};
//!
//! fn skip_verification(webhook: Webhook<Unverified>) {
//!     let _ = handle(webhook);
//! }
//! ```
//!
//! A downstream crate cannot add a verification state. This is error E0277:
//!
//! ```compile_fail,E0277
//! use reas_trust_boundary::webhook::VerificationState;
//!
//! struct Trusted;
//! impl VerificationState for Trusted {}
//! ```

pub mod frame {
    //! Transport frame: a 4-byte big-endian payload length, the payload, and
    //! an 8-byte tag. Every size here comes from outside data.

    use std::fmt;

    /// Length of the big-endian payload-length prefix.
    pub const HEADER_LEN: usize = 4;
    /// Length of the tag that follows the payload.
    pub const TAG_LEN: usize = 8;

    /// Resource limit (REAS-SEC-001). The parser compares the declared length
    /// with this limit before it allocates.
    pub const MAX_PAYLOAD_LEN: usize = 64 * 1024;

    /// A decoded frame: payload and tag. The payload is within `MAX_PAYLOAD_LEN`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Frame {
        payload: Vec<u8>,
        tag: [u8; TAG_LEN],
    }

    /// Why a byte sequence is not a frame.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FrameError {
        /// Fewer than `HEADER_LEN` bytes.
        TruncatedHeader {
            /// Bytes received.
            len: usize,
        },
        /// The declared length is above `MAX_PAYLOAD_LEN`.
        PayloadTooLong {
            /// Declared payload length.
            len: usize,
            /// The limit.
            max: usize,
        },
        /// The declared length does not fit `usize`, or an offset overflowed.
        LengthOverflow,
        /// The buffer ends before the declared payload and tag.
        Truncated {
            /// Bytes the declared frame needs.
            expected: usize,
            /// Bytes received.
            actual: usize,
        },
        /// The buffer continues after the tag.
        TrailingBytes {
            /// Bytes the declared frame occupies.
            expected: usize,
            /// Bytes received.
            actual: usize,
        },
    }

    impl fmt::Display for FrameError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                FrameError::TruncatedHeader { len } => {
                    write!(f, "frame header needs {HEADER_LEN} bytes, got {len}")
                }
                FrameError::PayloadTooLong { len, max } => {
                    write!(f, "payload length {len} is above the limit {max}")
                }
                FrameError::LengthOverflow => f.write_str("frame length overflows usize"),
                FrameError::Truncated { expected, actual } => {
                    write!(f, "frame needs {expected} bytes, got {actual}")
                }
                FrameError::TrailingBytes { expected, actual } => {
                    write!(f, "frame is {expected} bytes, got {actual}")
                }
            }
        }
    }

    impl std::error::Error for FrameError {}

    impl Frame {
        /// Builds a frame from trusted parts. The one invariant, the payload
        /// limit, is checked here too, so `to_bytes` cannot produce a frame
        /// that `parse` rejects.
        ///
        /// # Errors
        /// Returns [`FrameError::PayloadTooLong`] above `MAX_PAYLOAD_LEN`.
        pub fn new(payload: Vec<u8>, tag: [u8; TAG_LEN]) -> Result<Self, FrameError> {
            if payload.len() > MAX_PAYLOAD_LEN {
                return Err(FrameError::PayloadTooLong {
                    len: payload.len(),
                    max: MAX_PAYLOAD_LEN,
                });
            }
            Ok(Self { payload, tag })
        }

        /// Parses outside bytes. The declared length is converted once,
        /// compared with the limit once, and each offset is computed once
        /// with checked arithmetic (REAS-SEC-003).
        ///
        /// # Errors
        /// Returns a [`FrameError`] that names the first check that failed.
        pub fn parse(bytes: &[u8]) -> Result<Self, FrameError> {
            let header = bytes
                .get(..HEADER_LEN)
                .ok_or(FrameError::TruncatedHeader { len: bytes.len() })?;
            let header: [u8; HEADER_LEN] = header
                .try_into()
                .expect("get(..HEADER_LEN) returned exactly HEADER_LEN bytes");
            let declared = u32::from_be_bytes(header);

            // One conversion. On a 32-bit target this cannot fail; on a
            // 16-bit target it can.
            let len = usize::try_from(declared).map_err(|_| FrameError::LengthOverflow)?;
            // The limit check comes before any allocation that `len` bounds.
            if len > MAX_PAYLOAD_LEN {
                return Err(FrameError::PayloadTooLong {
                    len,
                    max: MAX_PAYLOAD_LEN,
                });
            }

            // Each offset is computed once and checked.
            let payload_end = HEADER_LEN
                .checked_add(len)
                .ok_or(FrameError::LengthOverflow)?;
            let frame_end = payload_end
                .checked_add(TAG_LEN)
                .ok_or(FrameError::LengthOverflow)?;

            // `get` validates the range against the real buffer. Nothing
            // below recomputes these offsets.
            let truncated = FrameError::Truncated {
                expected: frame_end,
                actual: bytes.len(),
            };
            let payload = bytes.get(HEADER_LEN..payload_end).ok_or(truncated)?;
            let tag = bytes.get(payload_end..frame_end).ok_or(truncated)?;
            if bytes.len() > frame_end {
                return Err(FrameError::TrailingBytes {
                    expected: frame_end,
                    actual: bytes.len(),
                });
            }

            let tag: [u8; TAG_LEN] = tag
                .try_into()
                .expect("get(payload_end..frame_end) returned exactly TAG_LEN bytes");
            Ok(Self {
                payload: payload.to_vec(),
                tag,
            })
        }

        /// Encodes the frame. `parse(frame.to_bytes())` returns an equal frame.
        pub fn to_bytes(&self) -> Vec<u8> {
            let len = u32::try_from(self.payload.len())
                .expect("Frame::new and Frame::parse keep the payload within MAX_PAYLOAD_LEN");
            let mut out = Vec::new();
            out.extend_from_slice(&len.to_be_bytes());
            out.extend_from_slice(&self.payload);
            out.extend_from_slice(&self.tag);
            out
        }

        /// The payload bytes.
        pub fn payload(&self) -> &[u8] {
            &self.payload
        }

        /// The tag bytes.
        pub fn tag(&self) -> &[u8; TAG_LEN] {
            &self.tag
        }
    }
}

pub mod webhook {
    //! Verification as a state. `Webhook<Verified>` has exactly one
    //! constructor, `Webhook::<Unverified>::verify`, and the handler accepts
    //! nothing else (REAS-STATE-002).

    use std::fmt;
    use std::marker::PhantomData;

    use crate::event::{Event, PayloadError};
    use crate::frame::{Frame, TAG_LEN};

    mod sealed {
        pub trait Sealed {}
    }

    /// Marker trait for verification states. Only this crate can implement it.
    pub trait VerificationState: sealed::Sealed {}

    /// The origin has not been verified.
    pub struct Unverified;
    /// The origin has been verified.
    pub struct Verified;

    impl sealed::Sealed for Unverified {}
    impl sealed::Sealed for Verified {}
    impl VerificationState for Unverified {}
    impl VerificationState for Verified {}

    /// Outbound port for origin verification. The application defines it; a
    /// reviewed MAC implementation should sit behind it in production.
    pub trait TagVerifier {
        /// Checks that `tag` authenticates `payload`.
        ///
        /// # Errors
        /// Returns [`TagMismatch`] when it does not.
        fn verify(&self, payload: &[u8], tag: &[u8; TAG_LEN]) -> Result<(), TagMismatch>;
    }

    /// The tag does not authenticate the payload.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TagMismatch;

    impl fmt::Display for TagMismatch {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("webhook tag does not match the payload")
        }
    }

    impl std::error::Error for TagMismatch {}

    /// A received webhook whose verification state is its type parameter.
    pub struct Webhook<State: VerificationState> {
        frame: Frame,
        state: PhantomData<State>,
    }

    impl Webhook<Unverified> {
        /// Wraps a decoded frame. Nothing about its origin is known yet.
        pub fn from_frame(frame: Frame) -> Self {
            Self {
                frame,
                state: PhantomData,
            }
        }

        /// The only constructor of `Webhook<Verified>`.
        ///
        /// # Errors
        /// Returns [`TagMismatch`] when the verifier rejects the tag.
        pub fn verify<V: TagVerifier>(
            self,
            verifier: &V,
        ) -> Result<Webhook<Verified>, TagMismatch> {
            verifier.verify(self.frame.payload(), self.frame.tag())?;
            Ok(Webhook {
                frame: self.frame,
                state: PhantomData,
            })
        }
    }

    impl Webhook<Verified> {
        /// The payload is parsed only after verification (REAS-SEC-002).
        ///
        /// # Errors
        /// Returns [`PayloadError`] when the payload is not a valid event.
        pub fn into_event(self) -> Result<Event, PayloadError> {
            Event::from_json(self.frame.payload())
        }
    }

    /// Handler. It takes only the verified type, so a caller cannot skip
    /// `verify`.
    ///
    /// # Errors
    /// Returns [`PayloadError`] when the payload is not a valid event.
    pub fn handle(webhook: Webhook<Verified>) -> Result<Event, PayloadError> {
        webhook.into_event()
    }
}

pub mod event {
    //! Domain event. `EventName` cannot be constructed with an invalid value,
    //! through the constructor or through serde (REAS-SEC-002).

    use std::fmt;

    use serde::Deserialize;

    /// Maximum length of an event name in bytes.
    pub const MAX_EVENT_NAME_LEN: usize = 64;

    /// Lowercase ASCII letters, digits, `_`, and `.`; 1 to 64 bytes.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    #[serde(try_from = "String")]
    pub struct EventName(String);

    /// Why a string is not an event name.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum InvalidEventName {
        /// The name is empty.
        Empty,
        /// The name is longer than `MAX_EVENT_NAME_LEN`.
        TooLong {
            /// Length in bytes.
            len: usize,
            /// The limit.
            max: usize,
        },
        /// The name contains a byte outside the allowed set.
        InvalidByte {
            /// Position of the byte.
            index: usize,
            /// The byte.
            byte: u8,
        },
    }

    impl fmt::Display for InvalidEventName {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                InvalidEventName::Empty => f.write_str("event name is empty"),
                InvalidEventName::TooLong { len, max } => {
                    write!(f, "event name is {len} bytes, limit {max}")
                }
                InvalidEventName::InvalidByte { index, byte } => {
                    write!(
                        f,
                        "event name has invalid byte 0x{byte:02x} at index {index}"
                    )
                }
            }
        }
    }

    impl std::error::Error for InvalidEventName {}

    impl TryFrom<String> for EventName {
        type Error = InvalidEventName;

        /// Smart constructor. serde calls it through `try_from`.
        fn try_from(raw: String) -> Result<Self, Self::Error> {
            if raw.is_empty() {
                return Err(InvalidEventName::Empty);
            }
            if raw.len() > MAX_EVENT_NAME_LEN {
                return Err(InvalidEventName::TooLong {
                    len: raw.len(),
                    max: MAX_EVENT_NAME_LEN,
                });
            }
            for (index, byte) in raw.bytes().enumerate() {
                let allowed = byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || byte == b'_'
                    || byte == b'.';
                if !allowed {
                    return Err(InvalidEventName::InvalidByte { index, byte });
                }
            }
            Ok(Self(raw))
        }
    }

    impl EventName {
        /// The validated name.
        pub fn as_str(&self) -> &str {
            &self.0
        }
    }

    /// A domain event decoded from a verified payload.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Event {
        /// The validated event name.
        pub name: EventName,
        /// Delivery attempt, starting at 1.
        pub attempt: u32,
    }

    /// Typed error for payload decoding. The serde error is the source.
    #[derive(Debug)]
    pub struct PayloadError(serde_json::Error);

    impl fmt::Display for PayloadError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "invalid event payload: {}", self.0)
        }
    }

    impl std::error::Error for PayloadError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    impl Event {
        /// Decodes an event from JSON. `EventName` validates through serde.
        ///
        /// # Errors
        /// Returns [`PayloadError`] for invalid JSON, an unknown field, or an
        /// invalid event name.
        pub fn from_json(payload: &[u8]) -> Result<Self, PayloadError> {
            serde_json::from_slice(payload).map_err(PayloadError)
        }
    }
}

pub mod checksum {
    //! A reference implementation and a duplicate. The duplicate processes
    //! eight bytes at a time with SWAR, which is SIMD within a register. The
    //! differential test and the fuzz target compare the two (REAS-SEC-005).

    const BLOCK: usize = 8;

    /// Reference: the wrapping sum of all bytes.
    pub fn reference(bytes: &[u8]) -> u32 {
        bytes
            .iter()
            .fold(0u32, |sum, &byte| sum.wrapping_add(u32::from(byte)))
    }

    /// Duplicate. `as_chunks` proves the bound before each full-width read:
    /// each block is a `[u8; BLOCK]`, so the type carries the proof. The
    /// scalar tail goes to the reference.
    pub fn fast(bytes: &[u8]) -> u32 {
        let (blocks, tail) = bytes.as_chunks::<BLOCK>();
        let sum = blocks.iter().fold(0u32, |sum, block| {
            sum.wrapping_add(block_sum(u64::from_le_bytes(*block)))
        });
        sum.wrapping_add(reference(tail))
    }

    /// Sum of the eight bytes in `word`. Lane widths double at each step. The
    /// bound of each step is stated, and checked arithmetic enforces it.
    fn block_sum(word: u64) -> u32 {
        const ODD_BYTES: u64 = 0x00FF_00FF_00FF_00FF;
        const ODD_HALVES: u64 = 0x0000_FFFF_0000_FFFF;
        const LOW_WORD: u64 = 0x0000_0000_FFFF_FFFF;

        // Four u16 lanes, each at most 255 + 255 = 510.
        let pairs = (word & ODD_BYTES)
            .checked_add((word >> 8) & ODD_BYTES)
            .expect("each u16 lane holds at most 510");
        // Two u32 lanes, each at most 510 + 510 = 1020.
        let quads = (pairs & ODD_HALVES)
            .checked_add((pairs >> 16) & ODD_HALVES)
            .expect("each u32 lane holds at most 1020");
        // One sum, at most 1020 + 1020 = 2040.
        let total = (quads & LOW_WORD)
            .checked_add(quads >> 32)
            .expect("the sum of eight bytes is at most 2040");
        u32::try_from(total).expect("2040 fits in u32")
    }
}

pub mod doubles {
    //! Test doubles. Not for production.

    use crate::frame::TAG_LEN;
    use crate::webhook::{TagMismatch, TagVerifier};

    /// XORs each payload byte into an 8-byte tag seeded with `key`. It is
    /// not a message authentication code: anyone who can read a frame can
    /// forge its tag. It exists so that tests and the fuzz target can reach
    /// both the verified and the unverified path.
    pub struct XorTagVerifier {
        key: u8,
    }

    impl XorTagVerifier {
        /// Creates a verifier with the given key byte.
        pub fn new(key: u8) -> Self {
            Self { key }
        }

        /// Computes the tag this verifier accepts for `payload`.
        pub fn tag_for(&self, payload: &[u8]) -> [u8; TAG_LEN] {
            let mut tag = [self.key; TAG_LEN];
            for (slot, &byte) in (0..TAG_LEN).cycle().zip(payload) {
                if let Some(lane) = tag.get_mut(slot) {
                    *lane ^= byte;
                }
            }
            tag
        }
    }

    impl TagVerifier for XorTagVerifier {
        fn verify(&self, payload: &[u8], tag: &[u8; TAG_LEN]) -> Result<(), TagMismatch> {
            if self.tag_for(payload) == *tag {
                Ok(())
            } else {
                Err(TagMismatch)
            }
        }
    }
}

pub mod inbound {
    //! The inbound adapter: the trust boundary. Outside bytes go in; a domain
    //! event or a typed error comes out.

    use std::fmt;

    use crate::event::{Event, PayloadError};
    use crate::frame::{Frame, FrameError};
    use crate::webhook::{handle, TagMismatch, TagVerifier, Webhook};

    /// Why outside bytes did not become an event. Each variant names the
    /// step that refused them.
    #[derive(Debug)]
    pub enum ReceiveError {
        /// The bytes are not a frame.
        Frame(FrameError),
        /// The tag does not authenticate the payload.
        Tag(TagMismatch),
        /// The payload is not a valid event.
        Payload(PayloadError),
    }

    impl fmt::Display for ReceiveError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                ReceiveError::Frame(err) => write!(f, "{err}"),
                ReceiveError::Tag(err) => write!(f, "{err}"),
                ReceiveError::Payload(err) => write!(f, "{err}"),
            }
        }
    }

    impl std::error::Error for ReceiveError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            match self {
                ReceiveError::Frame(err) => Some(err),
                ReceiveError::Tag(err) => Some(err),
                ReceiveError::Payload(err) => Some(err),
            }
        }
    }

    impl From<FrameError> for ReceiveError {
        fn from(err: FrameError) -> Self {
            ReceiveError::Frame(err)
        }
    }

    impl From<TagMismatch> for ReceiveError {
        fn from(err: TagMismatch) -> Self {
            ReceiveError::Tag(err)
        }
    }

    impl From<PayloadError> for ReceiveError {
        fn from(err: PayloadError) -> Self {
            ReceiveError::Payload(err)
        }
    }

    /// Decode the frame, verify the origin, then parse the payload. The
    /// order is fixed by the types: `handle` needs a `Webhook<Verified>`.
    ///
    /// # Errors
    /// Returns the [`ReceiveError`] of the first step that refused the bytes.
    pub fn receive<V: TagVerifier>(bytes: &[u8], verifier: &V) -> Result<Event, ReceiveError> {
        let frame = Frame::parse(bytes)?;
        let verified = Webhook::from_frame(frame).verify(verifier)?;
        Ok(handle(verified)?)
    }
}

#[cfg(test)]
mod tests {
    // Test inputs are constants; the crate lints stay on the code under test.
    #![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]

    use crate::checksum;
    use crate::doubles::XorTagVerifier;
    use crate::event::{Event, EventName, InvalidEventName, MAX_EVENT_NAME_LEN};
    use crate::frame::{Frame, FrameError, HEADER_LEN, MAX_PAYLOAD_LEN, TAG_LEN};
    use crate::inbound::{receive, ReceiveError};
    use crate::webhook::{TagMismatch, Webhook};

    const KEY: u8 = 0x5a;
    const VALID_PAYLOAD: &[u8] = br#"{"name":"order.paid","attempt":1}"#;

    fn verifier() -> XorTagVerifier {
        XorTagVerifier::new(KEY)
    }

    fn encode(payload: &[u8]) -> Vec<u8> {
        Frame::new(payload.to_vec(), verifier().tag_for(payload))
            .expect("test payloads are within the limit")
            .to_bytes()
    }

    fn raw_frame(declared_len: u32, body: &[u8]) -> Vec<u8> {
        let mut bytes = declared_len.to_be_bytes().to_vec();
        bytes.extend_from_slice(body);
        bytes
    }

    // Frame: sizes from outside data (REAS-SEC-003).

    #[test]
    fn frame_round_trips() {
        let bytes = encode(VALID_PAYLOAD);
        let frame = Frame::parse(&bytes).expect("valid frame");
        assert_eq!(frame.payload(), VALID_PAYLOAD);
        assert_eq!(frame.to_bytes(), bytes);
    }

    #[test]
    fn frame_rejects_a_short_header() {
        assert_eq!(
            Frame::parse(&[0, 0, 0]),
            Err(FrameError::TruncatedHeader { len: 3 })
        );
    }

    #[test]
    fn frame_rejects_a_declared_length_beyond_the_buffer() {
        // Declares 100 payload bytes; supplies 2 bytes and no tag.
        let bytes = raw_frame(100, &[1, 2]);
        assert_eq!(
            Frame::parse(&bytes),
            Err(FrameError::Truncated {
                expected: HEADER_LEN + 100 + TAG_LEN,
                actual: bytes.len(),
            })
        );
    }

    #[test]
    fn frame_rejects_a_missing_tag() {
        let bytes = raw_frame(2, &[1, 2]);
        assert_eq!(
            Frame::parse(&bytes),
            Err(FrameError::Truncated {
                expected: HEADER_LEN + 2 + TAG_LEN,
                actual: bytes.len(),
            })
        );
    }

    #[test]
    fn frame_rejects_trailing_bytes() {
        let mut bytes = encode(VALID_PAYLOAD);
        bytes.push(0);
        assert_eq!(
            Frame::parse(&bytes),
            Err(FrameError::TrailingBytes {
                expected: bytes.len() - 1,
                actual: bytes.len(),
            })
        );
    }

    #[test]
    fn frame_accepts_an_empty_payload() {
        let frame = Frame::parse(&encode(&[])).expect("empty payload is legal");
        assert_eq!(frame.payload(), &[] as &[u8]);
    }

    #[test]
    fn frame_accepts_a_payload_at_the_limit() {
        let payload = vec![0xAB; MAX_PAYLOAD_LEN];
        let frame = Frame::parse(&encode(&payload)).expect("payload at the limit is legal");
        assert_eq!(frame.payload().len(), MAX_PAYLOAD_LEN);
    }

    #[test]
    fn frame_rejects_a_payload_above_the_limit_before_reading_it() {
        // Declares one byte above the limit, supplies nothing. The limit
        // check must fire, not the truncation check.
        let declared = u32::try_from(MAX_PAYLOAD_LEN + 1).expect("fits u32");
        assert_eq!(
            Frame::parse(&raw_frame(declared, &[])),
            Err(FrameError::PayloadTooLong {
                len: MAX_PAYLOAD_LEN + 1,
                max: MAX_PAYLOAD_LEN,
            })
        );
    }

    #[test]
    fn frame_rejects_the_maximum_length_field() {
        assert_eq!(
            Frame::parse(&raw_frame(u32::MAX, &[])),
            Err(FrameError::PayloadTooLong {
                len: u32::MAX as usize,
                max: MAX_PAYLOAD_LEN,
            })
        );
    }

    #[test]
    fn frame_new_rejects_a_payload_above_the_limit() {
        let payload = vec![0; MAX_PAYLOAD_LEN + 1];
        assert_eq!(
            Frame::new(payload, [0; TAG_LEN]),
            Err(FrameError::PayloadTooLong {
                len: MAX_PAYLOAD_LEN + 1,
                max: MAX_PAYLOAD_LEN,
            })
        );
    }

    // Verification as a state (REAS-STATE-002).

    #[test]
    fn verify_rejects_a_wrong_tag() {
        let frame = Frame::new(VALID_PAYLOAD.to_vec(), [0; TAG_LEN]).expect("within the limit");
        let result = Webhook::from_frame(frame).verify(&verifier());
        assert!(matches!(result, Err(TagMismatch)));
    }

    #[test]
    fn receive_verifies_before_it_parses_the_payload() {
        // Invalid JSON with a wrong tag: the tag error must win, which shows
        // that the payload was not parsed first.
        let frame = Frame::new(b"not json".to_vec(), [0; TAG_LEN]).expect("within the limit");
        let err = receive(&frame.to_bytes(), &verifier()).expect_err("wrong tag");
        assert!(matches!(err, ReceiveError::Tag(TagMismatch)));
    }

    #[test]
    fn receive_returns_the_event_for_a_valid_frame() {
        let event = receive(&encode(VALID_PAYLOAD), &verifier()).expect("valid frame");
        assert_eq!(event.name.as_str(), "order.paid");
        assert_eq!(event.attempt, 1);
    }

    #[test]
    fn receive_reports_a_frame_error() {
        let err = receive(&[1, 2], &verifier()).expect_err("short header");
        assert!(matches!(
            err,
            ReceiveError::Frame(FrameError::TruncatedHeader { len: 2 })
        ));
    }

    // Validated types through serde (REAS-SEC-002).

    #[test]
    fn event_name_rejects_empty() {
        assert_eq!(
            EventName::try_from(String::new()),
            Err(InvalidEventName::Empty)
        );
    }

    #[test]
    fn event_name_rejects_uppercase() {
        assert_eq!(
            EventName::try_from("Order".to_owned()),
            Err(InvalidEventName::InvalidByte {
                index: 0,
                byte: b'O'
            })
        );
    }

    #[test]
    fn event_name_accepts_the_maximum_length() {
        let name = "a".repeat(MAX_EVENT_NAME_LEN);
        assert_eq!(
            EventName::try_from(name.clone()).map(|n| n.as_str().to_owned()),
            Ok(name)
        );
    }

    #[test]
    fn event_name_rejects_one_byte_above_the_maximum_length() {
        let name = "a".repeat(MAX_EVENT_NAME_LEN + 1);
        assert_eq!(
            EventName::try_from(name),
            Err(InvalidEventName::TooLong {
                len: MAX_EVENT_NAME_LEN + 1,
                max: MAX_EVENT_NAME_LEN,
            })
        );
    }

    #[test]
    fn deserializer_uses_the_same_validation_as_the_constructor() {
        let err = Event::from_json(br#"{"name":"Order","attempt":1}"#).expect_err("uppercase");
        assert_eq!(
            err.to_string(),
            "invalid event payload: event name has invalid byte 0x4f at index 0 at line 1 column 15"
        );
    }

    #[test]
    fn deserializer_rejects_an_unknown_field() {
        let err = Event::from_json(br#"{"name":"order.paid","attempt":1,"admin":true}"#)
            .expect_err("unknown field");
        assert_eq!(
            err.to_string(),
            "invalid event payload: unknown field `admin`, expected `name` or `attempt` at line 1 column 40"
        );
    }

    #[test]
    fn deserializer_rejects_a_negative_attempt() {
        let err = Event::from_json(br#"{"name":"order.paid","attempt":-1}"#).expect_err("negative");
        assert_eq!(
            err.to_string(),
            "invalid event payload: invalid value: integer `-1`, expected u32 at line 1 column 33"
        );
    }

    // Duplicate implementation against the reference (REAS-SEC-005).

    #[test]
    fn checksum_fast_matches_reference_at_block_boundaries() {
        for len in [0usize, 1, 7, 8, 9, 15, 16, 17, 31, 32, 33] {
            for fill in [0x00u8, 0x01, 0x7F, 0x80, 0xFF] {
                let bytes = vec![fill; len];
                assert_eq!(
                    checksum::fast(&bytes),
                    checksum::reference(&bytes),
                    "len {len} fill {fill:#x}"
                );
            }
        }
    }

    #[test]
    fn checksum_fast_matches_reference_on_generated_inputs() {
        // xorshift64: deterministic, so a failure names its seed.
        let seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut state = seed;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for case in 0..2000u32 {
            let len = usize::try_from(next() % 300).expect("fits usize");
            let bytes: Vec<u8> = (0..len).map(|_| (next() & 0xFF) as u8).collect();
            assert_eq!(
                checksum::fast(&bytes),
                checksum::reference(&bytes),
                "seed {seed:#x} case {case} len {len}"
            );
        }
    }

    #[test]
    fn checksum_reference_sums_bytes() {
        assert_eq!(checksum::reference(&[1, 2, 3]), 6);
        assert_eq!(checksum::reference(&[0xFF; 8]), 8 * 255);
    }
}
