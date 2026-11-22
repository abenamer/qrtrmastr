//! Mail adapters: where messages come from (`TakeoutExport`), how one is read (`mime`), classified
//! (`classify`) and redacted (`redact`), and the `PlatformMail` reader that does the three in
//! order and hands the use case a message it can store (REAS-ARCH-004).

pub mod classify;
pub mod gmail;
pub mod mime;
pub mod redact;
pub mod takeout;

use sha2::{Digest, Sha256};

use crate::application::ports::{MailError, MailReader, NewRawMessage, Prepared, RawMail};
use crate::domain::ids::MessageKey;
use crate::domain::states::Platform;
use redact::{REDACTION_VERSION, Redactor};

/// Reads the mail of the platforms: finds the platform by the exact sender address, classifies
/// the kind, redacts, and keys the message. One reader for every platform, because only the
/// sender list and the subject rules differ; both are data (`classify`).
pub struct PlatformMail {
    /// Each exact From address, lowercase, with the platform it belongs to.
    senders: Vec<(String, Platform)>,
    redactor: Redactor,
}

impl PlatformMail {
    /// A reader for the given sender addresses, each with its platform, as
    /// `ImportConfig::sender_platforms` lists them. An address is trimmed and compared without
    /// regard to ASCII case. Mail from an address that is not in the list is not stored.
    pub fn new(senders: Vec<(String, Platform)>, redactor: Redactor) -> Self {
        let senders = senders
            .into_iter()
            .map(|(address, platform)| (address.trim().to_ascii_lowercase(), platform))
            .collect();
        Self { senders, redactor }
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
        // A Message-ID that carries a configured term (some senders embed the recipient) is not
        // stored or printed; the hash stands in for it. The hash also stands in for a header
        // that is no key: empty, longer than the limit of a key, or with a NUL character.
        let key = parsed
            .message_id
            .as_deref()
            .and_then(|id| MessageKey::from_message_id(id).ok())
            .filter(|key| !self.redactor.carries_configured_term(key.as_str()))
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

        let redaction = |part: &str, error: redact::RedactionError| MailError::RedactionFailed {
            key: key.to_string(),
            kind: error.kind(),
            context: format!(
                "{platform} {kind}, {part}: {} {}",
                error.kind(),
                error.check()
            ),
        };
        let subject = self
            .redactor
            .redact_text(&parsed.subject)
            .map_err(|e| redaction("subject", e))?;
        let body_html = parsed
            .html
            .as_deref()
            .map(|html| self.redactor.redact_html(html))
            .transpose()
            .map_err(|e| redaction("html body", e))?;
        let body_text = parsed
            .text
            .as_deref()
            .map(|text| self.redactor.redact_text(text))
            .transpose()
            .map_err(|e| redaction("text body", e))?;

        // PostgreSQL holds no NUL character in a text. A mail with one in a part that would be
        // stored is refused here, as one message; without this the insert fails, and the
        // import stops at this message on each run.
        for (part, text) in [
            ("subject", Some(subject.as_str())),
            ("HTML body", body_html.as_deref()),
            ("text body", body_text.as_deref()),
        ] {
            if text.is_some_and(|text| text.contains('\0')) {
                return Err(MailError::Malformed {
                    key: key.to_string(),
                    reason: format!("a NUL character in the {part}"),
                });
            }
        }

        Ok(Prepared::Message(NewRawMessage {
            platform,
            key,
            received_at,
            subject,
            kind,
            dkim_pass,
            body_html,
            body_text,
            source_sha256: digest,
            redaction_version: REDACTION_VERSION,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::states::{KeyKind, MessageKind};
    use redact::RedactionTerms;

    const FIXTURE_HTML: &str =
        include_str!("../../../tests/fixtures/synthetic/input/freshdirect/2026/receipt-basic.html");

    fn terms() -> RedactionTerms {
        RedactionTerms {
            names: vec!["Pat Example".to_owned(), "Pat".to_owned()],
            emails: vec!["pat.example@example.com".to_owned()],
            phones: vec!["2125550199".to_owned()],
            address_lines: vec![
                "123 Imaginary Street Apt 4B".to_owned(),
                "Brooklyn, NY 11201".to_owned(),
            ],
            card_last4: vec!["4242".to_owned()],
        }
    }

    fn reader() -> PlatformMail {
        PlatformMail::new(
            vec![
                ("service@freshdirect.com".to_owned(), Platform::Freshdirect),
                ("orders@eat.seamless.com".to_owned(), Platform::Seamless),
            ],
            Redactor::new(terms()).expect("terms"),
        )
    }

    fn message(from: &str, subject: &str, auth: &str) -> RawMail {
        let bytes = format!(
            "From: FreshDirect <{from}>\r\nTo: pat.example@example.com\r\nSubject: {subject}\r\nDate: Tue, 3 Mar 2026 06:15:00 -0500\r\nMessage-ID: <synthetic-1@mail.example>\r\n{auth}Content-Type: text/html; charset=utf-8\r\n\r\n{FIXTURE_HTML}"
        );
        RawMail {
            bytes: bytes.into_bytes(),
        }
    }

    #[test]
    fn a_receipt_from_the_sender_is_keyed_classified_and_redacted() {
        let reader = reader();
        let raw = message(
            "service@freshdirect.com",
            "Your FreshDirect order is in the works! Here's your receipt",
            "Authentication-Results: mx.google.com; dkim=pass header.i=@freshdirect.com\r\n",
        );
        let Prepared::Message(m) = reader.prepare(&raw).expect("prepared") else {
            panic!("expected a message");
        };
        assert_eq!(m.key.as_str(), "synthetic-1@mail.example");
        assert_eq!(m.kind, MessageKind::Receipt);
        assert!(m.dkim_pass);
        assert_eq!(m.received_at.to_string(), "2026-03-03T11:15:00Z");
        let html = m.body_html.expect("html");
        for leaked in [
            "Imaginary Street",
            "555-0199",
            "4242",
            "pat.example@",
            "Pat Example",
            "Hi Pat",
        ] {
            assert!(!html.contains(leaked), "{leaked} survived");
        }
        assert!(html.contains("Beefsteak Tomatoes"));
        assert!(html.contains("$17.37"));
    }

    #[test]
    fn a_message_id_that_carries_a_term_is_replaced_by_the_hash() {
        let reader = reader();
        let raw = message("service@freshdirect.com", "Your order for Tuesday", "");
        let bytes = String::from_utf8(raw.bytes).expect("utf8").replace(
            "Message-ID: <synthetic-1@mail.example>",
            "Message-ID: <pat.example@example.com.9@mail.example>",
        );
        let Prepared::Message(m) = reader
            .prepare(&RawMail {
                bytes: bytes.into_bytes(),
            })
            .expect("prepared")
        else {
            panic!("expected a message");
        };
        assert_eq!(m.key.kind(), KeyKind::Sha256, "{:?}", m.key);
    }

    /// A mail of the grocer with the given header lines after the subject, and the given body.
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
    fn a_mail_with_a_nul_character_in_a_stored_part_is_refused_as_one_message() {
        // The store holds no NUL character in a text. Before 2026-11-21 such a mail failed at
        // the insert, and the import stopped at it on each run.
        let reader = reader();
        for (what, mail) in [
            (
                "HTML body",
                mail_with("Your receipt", "nul-1@mail.example", "<p>Order\0 1</p>"),
            ),
            (
                "HTML body",
                mail_with("Your receipt", "nul-2@mail.example", "<p>Order&#0; 1</p>"),
            ),
            (
                "HTML body",
                mail_with(
                    "Your receipt",
                    "nul-3@mail.example",
                    "<p title=\"a\0b\">Order</p>",
                ),
            ),
        ] {
            match reader.prepare(&mail) {
                // The page reader gives another character for a NUL in the HTML, as a browser
                // does; the text form of the same body keeps it. So the part that the
                // reason names is the text body for these mails.
                Err(MailError::Malformed { key, reason }) => {
                    assert_eq!(reason, "a NUL character in the text body", "{what}");
                    assert!(!key.contains('\0'));
                }
                Ok(Prepared::Message(message)) => {
                    // The library that reads the page may give another character for the NUL,
                    // as a browser does. What matters is that nothing with a NUL is stored.
                    for text in [
                        Some(message.subject.as_str()),
                        message.body_html.as_deref(),
                        message.body_text.as_deref(),
                        Some(message.key.as_str()),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        assert!(!text.contains('\0'), "{what}: a NUL would be stored");
                    }
                }
                other => panic!("{what}: {other:?}"),
            }
        }
        // The control: the same mail without the character is a message.
        assert!(matches!(
            reader.prepare(&mail_with(
                "Your receipt",
                "nul-0@mail.example",
                "<p>Order 1</p>"
            )),
            Ok(Prepared::Message(_))
        ));
    }

    #[test]
    fn a_message_id_that_is_no_key_is_replaced_by_the_hash() {
        // A header longer than the limit of a key, and one with a NUL character. Before
        // 2026-11-21 the first was stored as the key when the unique index of the keys could
        // hold it, and stopped the import when it could not.
        let reader = reader();
        let long = "a".repeat(crate::domain::ids::MESSAGE_ID_MAX_BYTES);
        for message_id in [format!("{long}b"), "nul\0-5@mail.example".to_owned()] {
            let mail = mail_with("Your receipt", &message_id, "<p>Order 1</p>");
            let Ok(Prepared::Message(message)) = reader.prepare(&mail) else {
                panic!("a mail with such a header is a message");
            };
            assert_eq!(message.key.kind(), KeyKind::Sha256);
            assert_eq!(message.key.as_str(), hex::encode(message.source_sha256));
        }
        // The control: a header at the limit is the key.
        let mail = mail_with("Your receipt", &long, "<p>Order 1</p>");
        let Ok(Prepared::Message(message)) = reader.prepare(&mail) else {
            panic!("a message");
        };
        assert_eq!(message.key.kind(), KeyKind::MessageId);
        assert_eq!(message.key.as_str(), long);
    }

    #[test]
    fn a_nul_character_in_the_subject_or_the_message_id_never_reaches_the_store() {
        let reader = reader();
        for mail in [
            mail_with("Your\0 receipt", "nul-4@mail.example", "<p>Order 1</p>"),
            mail_with("Your receipt", "nul\0-5@mail.example", "<p>Order 1</p>"),
        ] {
            match reader.prepare(&mail) {
                Err(MailError::Malformed { key, reason }) => {
                    assert!(!key.contains('\0'), "the key of the error holds a NUL");
                    assert!(reason.starts_with("a NUL character in the "), "{reason}");
                }
                Ok(Prepared::Message(message)) => {
                    assert!(!message.subject.contains('\0'));
                    assert!(!message.key.as_str().contains('\0'));
                }
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn mail_from_another_sender_is_not_stored() {
        let reader = reader();
        let raw = message("deals@a.freshdirect.com", "20% off this week", "");
        assert!(matches!(
            reader.prepare(&raw),
            Ok(Prepared::OtherSender { from }) if from == "deals@a.freshdirect.com"
        ));
    }

    #[test]
    fn a_message_without_a_dkim_result_is_stored_but_marked() {
        let reader = reader();
        let raw = message(
            "service@freshdirect.com",
            "Your order for Tuesday, March 3",
            "",
        );
        let Prepared::Message(m) = reader.prepare(&raw).expect("prepared") else {
            panic!("expected a message");
        };
        assert_eq!(m.kind, MessageKind::Placed);
        assert!(!m.dkim_pass);
    }

    fn seamless(auth: &str) -> RawMail {
        let bytes = format!(
            "From: Seamless <orders@eat.seamless.com>\r\nTo: pat.example@example.com\r\nSubject: Thanks for your Example Noodle Bar order\r\nDate: Fri, 6 Mar 2026 19:05:00 -0500\r\nMessage-ID: <synthetic-s1@mail.example>\r\n{auth}Content-Type: text/html; charset=utf-8\r\n\r\n<html><body><p>Hi Pat, your order is confirmed.</p><p>Deliver to 123 Imaginary Street Apt 4B, Brooklyn, NY 11201, (212) 555-0199</p><table><tr><td>1</td><td>Dan Dan Noodles</td><td>$14.50</td></tr></table></body></html>"
        );
        RawMail {
            bytes: bytes.into_bytes(),
        }
    }

    #[test]
    fn a_restaurant_receipt_is_stored_under_its_own_platform_and_redacted() {
        let raw = seamless(
            "Authentication-Results: mx.google.com; dkim=pass header.i=@eat.seamless.com\r\n",
        );
        let Prepared::Message(m) = reader().prepare(&raw).expect("prepared") else {
            panic!("expected a message");
        };
        assert_eq!(m.platform, Platform::Seamless);
        assert_eq!(m.kind, MessageKind::Receipt);
        assert!(m.dkim_pass);
        let html = m.body_html.expect("html");
        for leaked in ["Imaginary Street", "555-0199", "Hi Pat"] {
            assert!(!html.contains(leaked), "{leaked} survived");
        }
        assert!(html.contains("Dan Dan Noodles"));
        assert!(html.contains("$14.50"));
    }

    #[test]
    fn a_dkim_pass_for_another_platform_does_not_count_for_this_sender() {
        let raw = seamless(
            "Authentication-Results: mx.google.com; dkim=pass header.i=@freshdirect.com\r\n",
        );
        let Prepared::Message(m) = reader().prepare(&raw).expect("prepared") else {
            panic!("expected a message");
        };
        assert!(!m.dkim_pass);
    }
}
