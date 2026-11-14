//! MIME reading: the few header fields and the two body parts the importer needs, through
//! `mail-parser`. Nothing here decides anything; it extracts.

use chrono::{DateTime, TimeZone, Utc};
use mail_parser::MessageParser;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMail {
    pub message_id: Option<String>,
    pub date: Option<DateTime<Utc>>,
    pub subject: String,
    pub from_address: Option<String>,
    /// Every `Authentication-Results` header, as text.
    pub authentication_results: Vec<String>,
    pub html: Option<String>,
    pub text: Option<String>,
}

/// `None` when the bytes are not a message at all.
pub fn parse(bytes: &[u8]) -> Option<ParsedMail> {
    let message = MessageParser::default().parse(bytes)?;
    let headers = message.headers();
    let authentication_results = headers
        .all("Authentication-Results")
        .filter_map(|header| header.value().as_text().map(str::to_owned))
        .collect();
    let date = message
        .date()
        .and_then(|d| Utc.timestamp_opt(d.to_timestamp(), 0).single());
    Some(ParsedMail {
        message_id: message.message_id().map(str::to_owned),
        date,
        subject: message.subject().unwrap_or_default().to_owned(),
        from_address: message
            .from()
            .and_then(|list| list.first())
            .and_then(|mailbox| mailbox.address().map(str::to_owned)),
        authentication_results,
        html: message.body_html(0).map(|c| c.into_owned()),
        text: message.body_text(0).map(|c| c.into_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fields_the_importer_needs_come_out_of_a_multipart_message() {
        let raw = concat!(
            "From: FreshDirect <service@freshdirect.com>\r\n",
            "To: someone@example.com\r\n",
            "Subject: Your order for Tuesday\r\n",
            "Date: Tue, 3 Mar 2026 06:15:00 -0500\r\n",
            "Message-ID: <abc@mail.example>\r\n",
            "Authentication-Results: mx.google.com; dkim=pass header.i=@freshdirect.com; spf=pass\r\n",
            "MIME-Version: 1.0\r\n",
            "Content-Type: multipart/alternative; boundary=\"b\"\r\n\r\n",
            "--b\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nplain body\r\n",
            "--b\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>html body</p>\r\n",
            "--b--\r\n"
        );
        let parsed = parse(raw.as_bytes()).expect("parses");
        assert_eq!(parsed.message_id.as_deref(), Some("abc@mail.example"));
        assert_eq!(parsed.subject, "Your order for Tuesday");
        assert_eq!(
            parsed.from_address.as_deref(),
            Some("service@freshdirect.com")
        );
        assert_eq!(
            parsed.date.map(|d| d.to_rfc3339()),
            Some("2026-03-03T11:15:00+00:00".to_owned())
        );
        assert_eq!(parsed.authentication_results.len(), 1);
        assert!(parsed.authentication_results[0].contains("dkim=pass"));
        assert_eq!(parsed.html.as_deref(), Some("<p>html body</p>"));
        assert_eq!(parsed.text.as_deref(), Some("plain body"));
    }

    #[test]
    fn garbage_is_none_or_an_empty_shell_never_a_panic() {
        let parsed = parse(b"\x00\x01\x02 not mail");
        if let Some(parsed) = parsed {
            assert!(parsed.message_id.is_none());
        }
    }
}
