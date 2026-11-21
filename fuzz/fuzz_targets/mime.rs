//! Fuzz target for the MIME reader and for the reader of one mail that is built on it
//! (REAS-TEST-003): `mime::parse` and `PlatformMail::prepare`, the inbound adapter that every
//! import path calls with the bytes of a message.

#![no_main]

use std::sync::LazyLock;

use libfuzzer_sys::fuzz_target;
use qrtrmastr::adapters::mail::mime;
use qrtrmastr::adapters::mail::PlatformMail;
use qrtrmastr::application::ports::{MailError, MailReader, Prepared, RawMail};
use qrtrmastr::domain::ids::MessageKey;
use qrtrmastr::domain::states::Platform;
use qrtrmastr_fuzz::{redactor, term_in, term_left_in};
use sha2::{Digest, Sha256};

static READER: LazyLock<PlatformMail> = LazyLock::new(|| {
    PlatformMail::new(
        vec![
            ("service@freshdirect.com".to_owned(), Platform::Freshdirect),
            ("orders@eat.seamless.com".to_owned(), Platform::Seamless),
        ],
        redactor(),
    )
});

fuzz_target!(|data: &[u8]| {
    let parsed = mime::parse(data);
    let prepared = READER.prepare(&RawMail {
        bytes: data.to_vec(),
    });
    match prepared {
        // Property: bytes that are no message are refused as malformed, with a typed error.
        Err(MailError::Malformed { .. }) => {}
        Err(MailError::RedactionFailed { .. }) => assert!(parsed.is_some()),
        Err(error @ MailError::Read(_)) => panic!("the reader of one mail read no source: {error}"),
        Ok(Prepared::OtherSender { .. }) => assert!(parsed.is_some()),
        Ok(Prepared::Message(message)) => {
            assert!(parsed.is_some());
            // Property: the hash is the hash of the bytes as they came.
            let digest: [u8; 32] = Sha256::digest(data).into();
            assert_eq!(message.source_sha256, digest);
            // Property: the key that is stored comes back from its two columns as the same
            // key. A key that did not would stop every later import.
            assert_eq!(
                MessageKey::from_parts(message.key.kind(), message.key.as_str()),
                Ok(message.key.clone())
            );
            // Property: what is stored holds no configured term as it is.
            assert_eq!(term_in(message.key.as_str()), None, "in the key");
            assert_eq!(term_left_in(&message.subject), None, "in the subject");
            if let Some(html) = &message.body_html {
                assert_eq!(term_left_in(html), None, "in the HTML body");
            }
            if let Some(text) = &message.body_text {
                assert_eq!(term_left_in(text), None, "in the text body");
            }
        }
    }
    // Property: without a message there is no stored message and no other sender.
    if parsed.is_none() {
        assert!(matches!(
            READER.prepare(&RawMail {
                bytes: data.to_vec()
            }),
            Err(MailError::Malformed { .. })
        ));
    }
});
