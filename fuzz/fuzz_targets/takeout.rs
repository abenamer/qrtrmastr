//! Fuzz target for the Takeout reader (REAS-TEST-003): `TakeoutExport`, the mail source of
//! `qm import --takeout`.
//!
//! The reader takes a path, so the target writes the bytes to a file and reads it through the
//! public entry point, once as a mailbox file and once as an archive. Each file is removed
//! when its reading ends. Budgets are in `.github/workflows/fuzz.yml`.

#![no_main]

use std::io::Write;
use std::sync::LazyLock;

use libfuzzer_sys::fuzz_target;
use qrtrmastr::adapters::mail::takeout::TakeoutExport;
use qrtrmastr::application::ports::{MailError, MailSource};

static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime for the reader")
});

/// Reads `data` as a file whose name ends in `extension` and gives the sizes of the messages
/// and the error, if one ended the stream.
fn read_as(extension: &str, data: &[u8]) -> (Vec<usize>, Option<MailError>) {
    let mut file = tempfile::Builder::new()
        .prefix("qm-fuzz-")
        .suffix(extension)
        .tempfile()
        .expect("a file for the input");
    file.write_all(data).expect("the input file is written");
    file.flush().expect("the input file is written");
    RUNTIME.block_on(async {
        let mut stream = TakeoutExport::new(file.path()).messages();
        let mut sizes = Vec::new();
        let mut error = None;
        while let Some(item) = stream.recv().await {
            // Property: an error ends the stream; nothing follows it.
            assert!(error.is_none(), "an item after an error");
            match item {
                Ok(mail) => sizes.push(mail.bytes.len()),
                Err(e) => error = Some(e),
            }
        }
        (sizes, error)
    })
}

fuzz_target!(|data: &[u8]| {
    // A mailbox file. Property: the reader gives messages or a typed read error, and the
    // messages together are not larger than the file: the reader adds no byte.
    let (sizes, error) = read_as(".mbox", data);
    assert!(
        matches!(error, None | Some(MailError::Read(_))),
        "{error:?}"
    );
    let total: usize = sizes.iter().sum();
    assert!(
        total <= data.len(),
        "{total} bytes of messages from {} bytes of mailbox",
        data.len()
    );
    // Property: a file that starts as a mailbox starts gives a message or an error, not an
    // import of nothing.
    if data.starts_with(b"From ") && data.len() > 5 {
        assert!(
            !sizes.is_empty() || error.is_some(),
            "a mailbox with no message and no error"
        );
    }

    // An archive. Property: messages, or a typed read error. No lower bound is asserted: an
    // archive whose mailbox holds no message is an export of an empty label, and gives
    // nothing. Bytes that give a message are an archive, which is never smaller than the
    // record that ends it, 22 bytes; fewer bytes are an error.
    let (sizes, error) = read_as(".zip", data);
    assert!(
        matches!(error, None | Some(MailError::Read(_))),
        "{error:?}"
    );
    if data.len() < 22 {
        assert!(
            sizes.is_empty() && error.is_some(),
            "less than an archive gave {} messages and {error:?}",
            sizes.len()
        );
    }
});
