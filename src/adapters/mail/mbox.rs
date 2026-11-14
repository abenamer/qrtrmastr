//! `MboxFile`: the Takeout export as a mail source. Reading the file is blocking I/O, so it runs
//! on the blocking pool (REAS-RUST-007); the whole export is read into memory, which a few
//! hundred receipts fit comfortably.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use mail_parser::mailbox::mbox::MessageIterator;

use crate::application::ports::{MailError, MailSource, RawMail};

pub struct MboxFile {
    path: PathBuf,
}

impl MboxFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn read_all(path: &Path) -> Result<Vec<RawMail>, MailError> {
        let file = File::open(path).map_err(|e| {
            MailError::Read(format!(
                "{}: {e}. qm reads a Google Takeout export of your FreshDirect mail: the file \
                 Takeout/Mail/FreshDirect.mbox inside the unzipped archive. Through Compose, your \
                 exports directory (QM_DATA_DIR) is /data inside the container. How to request \
                 the export: docs/runbooks/takeout-export.md",
                path.display()
            ))
        })?;
        let mut messages = Vec::new();
        for message in MessageIterator::new(BufReader::new(file)) {
            let message =
                message.map_err(|e| MailError::Read(format!("{}: {e}", path.display())))?;
            messages.push(RawMail {
                bytes: message.unwrap_contents(),
            });
        }
        Ok(messages)
    }
}

impl MailSource for MboxFile {
    async fn messages(&self) -> Result<Vec<RawMail>, MailError> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || Self::read_all(&path))
            .await
            .map_err(|e| MailError::Read(format!("reader task failed: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_export_with_two_messages_yields_two_raw_mails() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("export");
        std::fs::write(
            &path,
            "From a@mail.example Tue Mar  3 06:15:00 2026\r\nSubject: one\r\n\r\nbody one\r\n\r\nFrom b@mail.example Tue Mar  3 07:15:00 2026\r\nSubject: two\r\n\r\nbody two\r\n",
        )
        .expect("write");
        let mails = MboxFile::new(&path).messages().await.expect("read");
        assert_eq!(mails.len(), 2);
        assert!(
            std::str::from_utf8(&mails[0].bytes)
                .expect("utf8")
                .contains("Subject: one")
        );
        assert!(
            std::str::from_utf8(&mails[1].bytes)
                .expect("utf8")
                .contains("Subject: two")
        );
    }

    #[tokio::test]
    async fn a_missing_file_is_a_read_error_that_says_how_to_get_an_export() {
        let result = MboxFile::new("/nonexistent/export").messages().await;
        let Err(MailError::Read(text)) = result else {
            panic!("expected a read error, got {result:?}");
        };
        assert!(text.contains("/nonexistent/export"), "{text}");
        assert!(text.contains("Google Takeout"), "{text}");
        assert!(text.contains("docs/runbooks/takeout-export.md"), "{text}");
    }
}
