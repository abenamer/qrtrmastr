//! Mail adapters: where messages come from (`TakeoutExport`), how one is read (`mime`) and
//! classified (`classify`), and the `PlatformMail` reader that does both and hands the use
//! case the record of a message and its page (REAS-ARCH-004).

pub mod classify;
pub mod gmail;
pub mod mime;
pub mod takeout;

use sha2::{Digest, Sha256};

use crate::application::ports::{
    MailError, MailReader, NewRawMessage, Prepared, PreparedMessage, RawMail,
};
use crate::domain::ids::MessageKey;
use crate::domain::states::Platform;

/// Reads the mail of the platforms: finds the platform by the exact sender address, classifies
/// the kind, and keys the message. One reader for every platform, because only the sender
/// list and the subject rules differ; both are data (`classify`).
pub struct PlatformMail {
    /// Each exact From address, lowercase, with the platform it belongs to.
    senders: Vec<(String, Platform)>,
}

impl PlatformMail {
    /// A reader for the given sender addresses, each with its platform, as
    /// `ImportConfig::sender_platforms` lists them. An address is trimmed and compared without
    /// regard to ASCII case. Mail from an address that is not in the list is not stored.
    pub fn new(senders: Vec<(String, Platform)>) -> Self {
        let senders = senders
            .into_iter()
            .map(|(address, platform)| (address.trim().to_ascii_lowercase(), platform))
            .collect();
        Self { senders }
    }
}

impl MailReader for PlatformMail {
    fn prepare(&self, raw: &RawMail) -> Result<Prepared, MailError> {
        let digest: [u8; 32] = Sha256::digest(&raw.bytes).into();
        let fallback_key = MessageKey::from_sha256(&digest);
        let parsed = mime::parse(&raw.bytes).ok_or_else(|| MailError::Malformed {
            key: fallback_key.to_string(),
            reason: "not an RFC 5322 message".to_owned(),
        })?;
        // The hash stands in for a header that is no key: empty, longer than the limit of a
        // key, or with a NUL character.
        let key = parsed
            .message_id
            .as_deref()
            .and_then(|id| MessageKey::from_message_id(id).ok())
            .unwrap_or(fallback_key);

        let Some(platform) = classify::platform_of(parsed.from_address.as_deref(), &self.senders)
        else {
            return Ok(Prepared::OtherSender {
                from: parsed.from_address.unwrap_or_default(),
            });
        };
        let received_at = parsed.date.ok_or_else(|| MailError::Malformed {
            key: key.to_string(),
            reason: "no Date header".to_owned(),
        })?;
        let kind = classify::kind_from_subject(platform, &parsed.subject);
        // The pass must be for the domain of this message's own sender, not for any platform's.
        let dkim_pass = parsed
            .from_address
            .as_deref()
            .and_then(classify::domain_of)
            .is_some_and(|domain| classify::dkim_passed(&parsed.authentication_results, &domain));

        // Nothing that the mail says is stored. The page goes to the parser of the same run.
        Ok(Prepared::Message(PreparedMessage {
            record: NewRawMessage {
                platform,
                key,
                received_at,
                kind,
                dkim_pass,
                source_sha256: digest,
            },
            html: parsed.html,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::states::{KeyKind, MessageKind};

    const FIXTURE_HTML: &str =
        include_str!("../../../tests/fixtures/synthetic/input/freshdirect/2026/receipt-basic.html");

    fn reader() -> PlatformMail {
        PlatformMail::new(vec![
            ("service@freshdirect.com".to_owned(), Platform::Freshdirect),
            ("orders@eat.seamless.com".to_owned(), Platform::Seamless),
        ])
    }

    fn message(from: &str, subject: &str, auth: &str) -> RawMail {
        let bytes = format!(
            "From: FreshDirect <{from}>\r\nTo: pat.example@example.com\r\nSubject: {subject}\r\nDate: Tue, 3 Mar 2026 06:15:00 -0500\r\nMessage-ID: <synthetic-1@mail.example>\r\n{auth}Content-Type: text/html; charset=utf-8\r\n\r\n{FIXTURE_HTML}"
        );
        RawMail {
            bytes: bytes.into_bytes(),
        }
    }

    fn prepared(raw: &RawMail) -> PreparedMessage {
        match reader().prepare(raw) {
            Ok(Prepared::Message(message)) => message,
            other => panic!("expected a message: {other:?}"),
        }
    }

    #[test]
    fn a_receipt_from_the_sender_is_keyed_and_classified_and_its_page_is_beside_the_record() {
        let raw = message(
            "service@freshdirect.com",
            "Your FreshDirect order is in the works! Here's your receipt",
            "Authentication-Results: mx.google.com; dkim=pass header.i=@freshdirect.com\r\n",
        );
        let m = prepared(&raw);
        // The record is all that the store gets. It has no field for a subject or a body.
        assert_eq!(m.record.platform, Platform::Freshdirect);
        assert_eq!(m.record.key.as_str(), "synthetic-1@mail.example");
        assert_eq!(m.record.kind, MessageKind::Receipt);
        assert!(m.record.dkim_pass);
        assert_eq!(m.record.received_at.to_string(), "2026-03-03T11:15:00Z");
        let digest: [u8; 32] = Sha256::digest(&raw.bytes).into();
        assert_eq!(m.record.source_sha256, digest);
        // The page is the mail as its sender wrote it: the items for the parser, and the
        // address of the made-up person too. That is why it stays in memory.
        let page = m.html.expect("the page");
        assert!(page.contains("Beefsteak Tomatoes"));
        assert!(page.contains("Imaginary Street"));
    }

    /// A mail of the grocer with the given subject, `Message-ID` and body.
    fn mail_with(subject: &str, message_id: &str, body: &str) -> RawMail {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"From: FreshDirect <service@freshdirect.com>\r\nSubject: ");
        bytes.extend_from_slice(subject.as_bytes());
        bytes.extend_from_slice(b"\r\nDate: Tue, 3 Mar 2026 06:15:00 -0500\r\nMessage-ID: <");
        bytes.extend_from_slice(message_id.as_bytes());
        bytes.extend_from_slice(b">\r\nContent-Type: text/html; charset=utf-8\r\n\r\n");
        bytes.extend_from_slice(body.as_bytes());
        RawMail { bytes }
    }

    #[test]
    fn a_message_id_that_is_no_key_is_replaced_by_the_hash() {
        // A header longer than the limit of a key, and one with a NUL character, which the
        // store cannot hold in a text.
        let long = "a".repeat(crate::domain::ids::MESSAGE_ID_MAX_BYTES);
        for message_id in [format!("{long}b"), "nul\0-5@mail.example".to_owned()] {
            let m = prepared(&mail_with("Your receipt", &message_id, "<p>Order 1</p>"));
            assert_eq!(m.record.key.kind(), KeyKind::Sha256);
            assert_eq!(m.record.key.as_str(), hex::encode(m.record.source_sha256));
        }
        // The control: a header at the limit is the key.
        let m = prepared(&mail_with("Your receipt", &long, "<p>Order 1</p>"));
        assert_eq!(m.record.key.kind(), KeyKind::MessageId);
        assert_eq!(m.record.key.as_str(), long);
    }

    #[test]
    fn a_mail_without_a_date_is_malformed_and_its_error_names_the_key() {
        let raw = message("service@freshdirect.com", "Your receipt", "");
        let bytes = String::from_utf8(raw.bytes)
            .expect("utf8")
            .replace("Date: Tue, 3 Mar 2026 06:15:00 -0500\r\n", "");
        assert!(matches!(
            reader().prepare(&RawMail { bytes: bytes.into_bytes() }),
            Err(MailError::Malformed { key, reason })
                if key.contains("synthetic-1@mail.example") && reason == "no Date header"
        ));
    }

    #[test]
    fn mail_from_another_sender_is_not_stored() {
        let raw = message("deals@a.freshdirect.com", "20% off this week", "");
        assert!(matches!(
            reader().prepare(&raw),
            Ok(Prepared::OtherSender { from }) if from == "deals@a.freshdirect.com"
        ));
    }

    #[test]
    fn a_message_without_a_dkim_result_is_marked() {
        let raw = message(
            "service@freshdirect.com",
            "Your order for Tuesday, March 3",
            "",
        );
        let m = prepared(&raw);
        assert_eq!(m.record.kind, MessageKind::Placed);
        assert!(!m.record.dkim_pass);
    }

    fn seamless(auth: &str) -> RawMail {
        let bytes = format!(
            "From: Seamless <orders@eat.seamless.com>\r\nTo: pat.example@example.com\r\nSubject: Thanks for your Example Noodle Bar order\r\nDate: Fri, 6 Mar 2026 19:05:00 -0500\r\nMessage-ID: <synthetic-s1@mail.example>\r\n{auth}Content-Type: text/html; charset=utf-8\r\n\r\n<html><body><p>Hi Pat, your order is confirmed.</p><table><tr><td>1</td><td>Dan Dan Noodles</td><td>$14.50</td></tr></table></body></html>"
        );
        RawMail {
            bytes: bytes.into_bytes(),
        }
    }

    #[test]
    fn a_restaurant_receipt_is_read_under_its_own_platform() {
        let raw = seamless(
            "Authentication-Results: mx.google.com; dkim=pass header.i=@eat.seamless.com\r\n",
        );
        let m = prepared(&raw);
        assert_eq!(m.record.platform, Platform::Seamless);
        assert_eq!(m.record.kind, MessageKind::Receipt);
        assert!(m.record.dkim_pass);
    }

    #[test]
    fn a_dkim_pass_for_another_platform_does_not_count_for_this_sender() {
        let raw = seamless(
            "Authentication-Results: mx.google.com; dkim=pass header.i=@freshdirect.com\r\n",
        );
        assert!(!prepared(&raw).record.dkim_pass);
    }
}
