//! Fuzz target for the MIME reader and for the reader of one mail that is built on it
//! (REAS-TEST-003): `mime::parse` and `PlatformMail::prepare`, the inbound adapter that every
//! import path calls with the bytes of a message.

#![no_main]

use std::sync::LazyLock;

use libfuzzer_sys::fuzz_target;
use qrtrmastr::adapters::mail::mime;
use qrtrmastr::adapters::mail::PlatformMail;
use qrtrmastr::application::ports::{MailError, MailReader, Prepared, RawMail};
use qrtrmastr::domain::ids::{MESSAGE_ID_MAX_BYTES, MessageKey};
use qrtrmastr::domain::states::Platform;
use sha2::{Digest, Sha256};

static READER: LazyLock<PlatformMail> = LazyLock::new(|| {
    PlatformMail::new(vec![
        ("service@freshdirect.com".to_owned(), Platform::Freshdirect),
        ("orders@eat.seamless.com".to_owned(), Platform::Seamless),
    ])
});

fuzz_target!(|data: &[u8]| {
    let parsed = mime::parse(data);
    let prepared = READER.prepare(&RawMail {
        bytes: data.to_vec(),
    });
    match prepared {
        // Property: bytes that are no message are refused as malformed, with a typed error.
        Err(MailError::Malformed { .. }) => {}
        Err(error @ MailError::Read(_)) => panic!("the reader of one mail read no source: {error}"),
        Ok(Prepared::OtherSender { .. }) => assert!(parsed.is_some()),
        Ok(Prepared::Message(message)) => {
            assert!(parsed.is_some());
            let record = &message.record;
            // Property: the hash is the hash of the bytes as they came.
            let digest: [u8; 32] = Sha256::digest(data).into();
            assert_eq!(record.source_sha256, digest);
            // Property: the key that is stored comes back from its two columns as the same
            // key. A key that did not would stop every later import.
            assert_eq!(
                MessageKey::from_parts(record.key.kind(), record.key.as_str()),
                Ok(record.key.clone())
            );
            // Property: the key, which is the one text of a mail that the store gets, holds
            // no NUL character and is not longer than its limit. The store refuses such a
            // text, and a mail that the store refuses stops every later import.
            assert!(record.key.as_str().len() <= MESSAGE_ID_MAX_BYTES);
            assert!(!record.key.as_str().contains('\0'), "a NUL character would be stored");
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
