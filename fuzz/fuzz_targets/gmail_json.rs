//! Fuzz target for the JSON of the Gmail adapter (REAS-TEST-003): `list_page_from_json` and
//! `raw_message_from_json`, which read the two answers of the Gmail API that carry data of
//! the mailbox.

#![no_main]

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use libfuzzer_sys::fuzz_target;
use qrtrmastr::adapters::mail::gmail::{GmailError, list_page_from_json, raw_message_from_json};

fuzz_target!(|data: &[u8]| {
    // Property: a page, or the typed error of an answer. Each id of a page can be a part of
    // a request path: 1 to 64 ASCII letters and digits.
    match list_page_from_json(data) {
        Ok(page) => {
            for id in &page.ids {
                assert!(
                    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric()),
                    "{id:?}"
                );
            }
        }
        Err(GmailError::Answer { .. }) => {}
        Err(error) => panic!("the list gave another error: {error}"),
    }

    // Property: the bytes of a message, or the typed error of an answer. The bytes encode
    // back to the text of the answer, without its padding.
    match raw_message_from_json(data) {
        Ok(bytes) => {
            // The reader skips a field that it does not use without a look at its bytes, so
            // a body that it reads is not always JSON for a stricter reader. The property is
            // checked when the stricter reader agrees that the body is an object with the
            // text.
            let raw = serde_json::from_slice::<serde_json::Value>(data)
                .ok()
                .and_then(|answer| answer.get("raw").and_then(|raw| raw.as_str().map(str::to_owned)));
            if let Some(raw) = raw {
                assert_eq!(URL_SAFE_NO_PAD.encode(&bytes), raw.trim_end_matches('='));
            }
        }
        Err(GmailError::Answer { .. }) => {}
        Err(error) => panic!("the message gave another error: {error}"),
    }
});
