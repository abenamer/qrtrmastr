//! Mail adapters: where messages come from (`MboxFile`), how one is read (`mime`), classified
//! (`classify`) and redacted (`redact`), and the `FreshDirectMail` reader that does the three in
//! order and hands the use case a message it can store (REAS-ARCH-004).

pub mod classify;
pub mod mbox;
pub mod mime;
pub mod redact;

use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::application::ports::{MailError, MailReader, NewRawMessage, Prepared, RawMail};
use crate::domain::ids::MessageKey;
use crate::domain::states::Platform;
use redact::{REDACTION_VERSION, Redactor};

/// Reads FreshDirect mail: checks the sender, classifies the kind, redacts, and keys the message.
pub struct FreshDirectMail {
    senders: Vec<String>,
    /// The domains of the senders; a DKIM pass must be for one of them.
    domains: Vec<String>,
    redactor: Redactor,
}

impl FreshDirectMail {
    pub fn new(senders: Vec<String>, redactor: Redactor) -> Self {
        let senders: Vec<String> = senders.into_iter().map(|s| s.to_lowercase()).collect();
        let domains = senders
            .iter()
            .filter_map(|s| classify::domain_of(s))
            .collect();
        Self {
            senders,
            domains,
            redactor,
        }
    }
}

impl MailReader for FreshDirectMail {
    fn prepare(&self, raw: &RawMail) -> Result<Prepared, MailError> {
        let digest: [u8; 32] = Sha256::digest(&raw.bytes).into();
        let fallback_key = MessageKey::from_sha256(&digest);
        let parsed = mime::parse(&raw.bytes).ok_or_else(|| MailError::Malformed {
            key: fallback_key.to_string(),
            reason: "not an RFC 5322 message".to_owned(),
        })?;
        // A Message-ID that carries a configured term (some senders embed the recipient) is not
        // stored or printed; the hash stands in for it.
        let key = parsed
            .message_id
            .as_deref()
            .and_then(|id| MessageKey::from_message_id(id).ok())
            .filter(|key| !self.redactor.carries_configured_term(key.as_str()))
            .unwrap_or(fallback_key);

        if !classify::sender_matches(parsed.from_address.as_deref(), &self.senders) {
            return Ok(Prepared::OtherSender {
                from: parsed.from_address.unwrap_or_default(),
            });
        }
        let received_at = parsed.date.ok_or_else(|| MailError::Malformed {
            key: key.to_string(),
            reason: "no Date header".to_owned(),
        })?;
        let kind = classify::kind_from_subject(&parsed.subject);
        let dkim_pass = self
            .domains
            .iter()
            .any(|domain| classify::dkim_passed(&parsed.authentication_results, domain));

        let redaction = |kind| MailError::RedactionFailed {
            key: key.to_string(),
            kind,
        };
        let subject = self
            .redactor
            .redact_text(&parsed.subject)
            .map_err(|e| redaction(e.kind()))?;
        let body_html = parsed
            .html
            .as_deref()
            .map(|html| self.redactor.redact_html(html))
            .transpose()
            .map_err(|e| redaction(e.kind()))?;
        let body_text = parsed
            .text
            .as_deref()
            .map(|text| self.redactor.redact_text(text))
            .transpose()
            .map_err(|e| redaction(e.kind()))?;

        Ok(Prepared::Message(NewRawMessage {
            platform: Platform::Freshdirect,
            key,
            received_at: received_at.with_timezone(&Utc),
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
    use crate::domain::states::MessageKind;
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
        let reader = FreshDirectMail::new(
            vec!["service@freshdirect.com".to_owned()],
            Redactor::new(terms()).expect("terms"),
        );
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
        assert_eq!(m.received_at.to_rfc3339(), "2026-03-03T11:15:00+00:00");
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
        let reader = FreshDirectMail::new(
            vec!["service@freshdirect.com".to_owned()],
            Redactor::new(terms()).expect("terms"),
        );
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
        assert!(matches!(m.key, MessageKey::Sha256(_)), "{:?}", m.key);
    }

    #[test]
    fn mail_from_another_sender_is_not_stored() {
        let reader = FreshDirectMail::new(
            vec!["service@freshdirect.com".to_owned()],
            Redactor::new(terms()).expect("terms"),
        );
        let raw = message("deals@a.freshdirect.com", "20% off this week", "");
        assert!(matches!(
            reader.prepare(&raw),
            Ok(Prepared::OtherSender { from }) if from == "deals@a.freshdirect.com"
        ));
    }

    #[test]
    fn a_message_without_a_dkim_result_is_stored_but_marked() {
        let reader = FreshDirectMail::new(
            vec!["service@freshdirect.com".to_owned()],
            Redactor::new(terms()).expect("terms"),
        );
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
}
