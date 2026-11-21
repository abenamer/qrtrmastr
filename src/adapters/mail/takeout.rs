//! `TakeoutExport`: Google Takeout exports as a mail source. The path is one mailbox file, one
//! Takeout archive (`.zip`), or a folder that holds any number of both. Each archive is read in
//! place, entry by entry, and nothing is extracted to disk; each message is sent through a
//! bounded channel as it is read, so an export of any size is never held in memory whole.
//!
//! Reading is blocking I/O and inflating is CPU work, so both run on the blocking pool
//! (REAS-RUST-007). The channel's bound is the limit on work in flight. Cancellation: when the
//! use case drops the receiver, the next send fails and the reader stops. A reader that panics
//! is reported as a read error, never as the end of the export.
//!
//! One archive is open at a time: the files are read in name order by one blocking task, and
//! an archive is closed before the next is opened.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use mail_parser::mailbox::mbox::MessageIterator;
use tokio::sync::mpsc;

use crate::application::ports::{MailError, MailSource, MailStream, RawMail};

/// Messages in flight between the reader and the use case.
const CHANNEL_BOUND: usize = 32;
/// How deep a folder is searched: an unzipped archive holds `Takeout/Mail/<label>.mbox`.
const MAX_DEPTH: usize = 6;
const READ_BUFFER: usize = 1 << 20;
/// What the stream says when the reader task ends in a panic. A fixed text: the text of a
/// panic comes from a library and is not ours to vouch for.
const READER_STOPPED: &str = "the reader of the export stopped unexpectedly; nothing more was read";

const HOW_TO: &str = "qm reads Google Takeout exports of your mail: a folder of the \
    .zip archives as Google delivers them, one archive, or an unzipped Takeout/Mail/<label>.mbox \
    file. Through Compose, your exports directory (QM_DATA_DIR) is /data inside the container. \
    How to request the export: docs/runbooks/takeout-export.md";

pub struct TakeoutExport {
    path: PathBuf,
}

/// One file the export consists of.
#[derive(Debug, PartialEq, Eq)]
enum Container {
    /// A Takeout archive; every `.mbox` entry in it is a mailbox.
    Archive(PathBuf),
    /// A mailbox file on disk.
    Mailbox(PathBuf),
}

impl Container {
    fn path(&self) -> &Path {
        match self {
            Container::Archive(path) | Container::Mailbox(path) => path,
        }
    }
}

/// The receiver is gone: the use case stopped, so the reader stops too.
struct ReceiverGone;

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(extension))
}

fn read_error(path: &Path, error: impl std::fmt::Display) -> MailError {
    MailError::Read(format!("{}: {error}. {HOW_TO}", path.display()))
}

impl TakeoutExport {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The files to read, in name order. A file given directly is an archive when its name ends
    /// in `.zip` and a mailbox otherwise, whatever its name (a mail client's folder file has no
    /// extension). In a folder, only `.zip` and `.mbox` files count.
    fn containers(path: &Path) -> Result<Vec<Container>, MailError> {
        let metadata = std::fs::metadata(path).map_err(|e| read_error(path, e))?;
        if metadata.is_file() {
            return Ok(vec![if has_extension(path, "zip") {
                Container::Archive(path.to_owned())
            } else {
                Container::Mailbox(path.to_owned())
            }]);
        }
        let mut found = Vec::new();
        Self::walk(path, MAX_DEPTH, &mut found)?;
        found.sort_by(|a, b| a.path().cmp(b.path()));
        if found.is_empty() {
            return Err(read_error(
                path,
                "no .zip archive and no .mbox file in this folder",
            ));
        }
        Ok(found)
    }

    /// Collects the archives and mailboxes of a folder. `levels_left` is how many levels of
    /// folders below this one are still read; it counts down, so no step of it can overflow.
    fn walk(dir: &Path, levels_left: usize, found: &mut Vec<Container>) -> Result<(), MailError> {
        for entry in std::fs::read_dir(dir).map_err(|e| read_error(dir, e))? {
            let entry = entry.map_err(|e| read_error(dir, e))?;
            let path = entry.path();
            // `file_type` does not follow a symbolic link, so a link out of the folder is not read.
            let kind = entry.file_type().map_err(|e| read_error(&path, e))?;
            if kind.is_dir() {
                if let Some(deeper) = levels_left.checked_sub(1) {
                    Self::walk(&path, deeper, found)?;
                }
            } else if kind.is_file() {
                if has_extension(&path, "zip") {
                    found.push(Container::Archive(path));
                } else if has_extension(&path, "mbox") {
                    found.push(Container::Mailbox(path));
                }
            }
        }
        Ok(())
    }

    /// Sends every message of one mailbox. `Ok(Err(ReceiverGone))` means stop without an error.
    fn send_mailbox(
        reader: impl BufRead,
        origin: &Path,
        tx: &mpsc::Sender<Result<RawMail, MailError>>,
    ) -> Result<Result<(), ReceiverGone>, MailError> {
        for message in MessageIterator::new(reader) {
            let message =
                message.map_err(|e| MailError::Read(format!("{}: {e}", origin.display())))?;
            let mail = RawMail {
                bytes: message.unwrap_contents(),
            };
            if tx.blocking_send(Ok(mail)).is_err() {
                return Ok(Err(ReceiverGone));
            }
        }
        Ok(Ok(()))
    }

    fn send_archive(
        path: &Path,
        tx: &mpsc::Sender<Result<RawMail, MailError>>,
    ) -> Result<Result<usize, ReceiverGone>, MailError> {
        let file = File::open(path).map_err(|e| read_error(path, e))?;
        let mut archive = zip::ZipArchive::new(BufReader::new(file)).map_err(|e| {
            MailError::Read(format!("{}: not a readable archive: {e}", path.display()))
        })?;
        let mut mailboxes: usize = 0;
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|e| MailError::Read(format!("{}: entry {index}: {e}", path.display())))?;
            let name = entry
                .name()
                .map_err(|e| MailError::Read(format!("{}: entry {index}: {e}", path.display())))?
                .into_owned();
            if !entry.is_file() || !has_extension(Path::new(&name), "mbox") {
                continue;
            }
            mailboxes = mailboxes.saturating_add(1);
            // The entry is named by its index: its name comes from inside the archive.
            tracing::info!(archive = ?path, entry = index, "reading a mailbox");
            let reader = BufReader::with_capacity(READ_BUFFER, entry);
            if Self::send_mailbox(reader, path, tx)?.is_err() {
                return Ok(Err(ReceiverGone));
            }
        }
        Ok(Ok(mailboxes))
    }

    // A log line is an output context (REAS-SEC-006): a path goes into it with `?`, the debug
    // form, which writes a control character or a line break of a file name as an escape. With
    // `%` the name is written as it is, and a line break in it starts a line of its own.
    fn send_all(
        path: &Path,
        tx: &mpsc::Sender<Result<RawMail, MailError>>,
    ) -> Result<(), MailError> {
        // Only zero or not zero is read from this count, so it saturates.
        let mut mailboxes: usize = 0;
        for container in Self::containers(path)? {
            match container {
                Container::Archive(archive) => {
                    tracing::info!(archive = ?archive, "opening an archive");
                    match Self::send_archive(&archive, tx)? {
                        Ok(count) => {
                            tracing::info!(
                                archive = ?archive,
                                mailboxes = count,
                                "archive done"
                            );
                            mailboxes = mailboxes.saturating_add(count);
                        }
                        Err(ReceiverGone) => return Ok(()),
                    }
                }
                Container::Mailbox(file) => {
                    tracing::info!(file = ?file, "reading a mailbox");
                    let reader = BufReader::with_capacity(
                        READ_BUFFER,
                        File::open(&file).map_err(|e| read_error(&file, e))?,
                    );
                    mailboxes = mailboxes.saturating_add(1);
                    if Self::send_mailbox(reader, &file, tx)?.is_err() {
                        return Ok(());
                    }
                }
            }
        }
        if mailboxes == 0 {
            return Err(read_error(
                path,
                "the archives hold no .mbox entry; select Mail when you request the export",
            ));
        }
        Ok(())
    }
}

impl MailSource for TakeoutExport {
    fn messages(&self) -> MailStream {
        let (tx, rx) = mpsc::channel(CHANNEL_BOUND);
        let path = self.path.clone();
        let reader_tx = tx.clone();
        let reader = tokio::task::spawn_blocking(move || {
            if let Err(error) = Self::send_all(&path, &reader_tx) {
                // A send that fails here means the use case already stopped.
                let _ = reader_tx.blocking_send(Err(error));
            }
        });
        tokio::spawn(async move {
            if reader.await.is_err() {
                let _ = tx
                    .send(Err(MailError::Read(READER_STOPPED.to_owned())))
                    .await;
            }
        });
        rx
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    const ONE: &str =
        "From a@mail.example Tue Mar  3 06:15:00 2026\r\nSubject: one\r\n\r\nbody one\r\n";
    const TWO_AND_THREE: &str = "From b@mail.example Tue Mar  3 07:15:00 2026\r\nSubject: two\r\n\r\nbody two\r\n\r\nFrom c@mail.example Tue Mar  3 08:15:00 2026\r\nSubject: three\r\n\r\nbody three\r\n";

    async fn subjects(source: &TakeoutExport) -> Result<Vec<String>, MailError> {
        let mut stream = source.messages();
        let mut subjects = Vec::new();
        while let Some(mail) = stream.recv().await {
            let text = String::from_utf8(mail?.bytes).expect("utf8");
            let subject = text
                .lines()
                .find_map(|line| line.strip_prefix("Subject: "))
                .expect("a subject")
                .to_owned();
            subjects.push(subject);
        }
        Ok(subjects)
    }

    fn write_archive(path: &Path, entries: &[(&str, &str)]) {
        let mut writer = zip::ZipWriter::new(File::create(path).expect("create"));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, content) in entries {
            writer.start_file(*name, options).expect("entry");
            writer.write_all(content.as_bytes()).expect("write");
        }
        writer.finish().expect("finish");
    }

    /// Collects what the log layer writes, for a test that reads the log lines.
    #[derive(Clone, Default)]
    struct Captured(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Captured {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("log").extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
        type Writer = Captured;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[test]
    fn a_file_name_with_control_characters_is_one_log_line_without_them() {
        let dir = tempfile::tempdir().expect("tempdir");
        // The name clears the screen and then starts a second line that looks like a log line.
        let path = dir.path().join("a\u{1b}[2J\nWARN forged line\r\u{9b}31m");
        std::fs::write(&path, ONE).expect("write");
        let captured = Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(captured.clone())
            .with_ansi(false)
            .finish();
        let (tx, mut rx) = mpsc::channel(8);
        tracing::subscriber::with_default(subscriber, || {
            TakeoutExport::send_all(&path, &tx).expect("read");
        });
        assert!(rx.try_recv().expect("one message").is_ok());

        let log = String::from_utf8(captured.0.lock().expect("log").clone()).expect("utf-8");
        // The positive control: the line of this file is in the log, with the name in it.
        assert!(log.contains("reading a mailbox"), "{log:?}");
        assert!(log.contains("forged line"), "{log:?}");
        // One event, one line: the name added no line of its own.
        assert_eq!(log.lines().count(), 1, "{log:?}");
        assert!(
            !log.trim_end_matches('\n')
                .contains(|c: char| c.is_control()),
            "{log:?}"
        );
    }

    #[test]
    fn an_archive_name_with_control_characters_adds_no_log_line_either() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("t\u{1b}[2J\nWARN forged line\r.zip");
        write_archive(&path, &[("Takeout/Mail/FreshDirect.mbox", ONE)]);
        let captured = Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(captured.clone())
            .with_ansi(false)
            .finish();
        let (tx, mut rx) = mpsc::channel(8);
        tracing::subscriber::with_default(subscriber, || {
            TakeoutExport::send_all(&path, &tx).expect("read");
        });
        assert!(rx.try_recv().expect("one message").is_ok());

        let log = String::from_utf8(captured.0.lock().expect("log").clone()).expect("utf-8");
        // Three events name the archive: it is opened, one mailbox in it is read, it is done.
        for event in ["opening an archive", "reading a mailbox", "archive done"] {
            assert!(log.contains(event), "{log:?}");
        }
        assert_eq!(log.matches("forged line").count(), 3, "{log:?}");
        assert_eq!(log.lines().count(), 3, "{log:?}");
        assert!(
            !log.contains(|c: char| c.is_control() && c != '\n'),
            "{log:?}"
        );
    }

    #[tokio::test]
    async fn a_mailbox_file_yields_its_messages_whatever_its_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("FreshDirect");
        std::fs::write(&path, TWO_AND_THREE).expect("write");
        let subjects = subjects(&TakeoutExport::new(&path)).await.expect("read");
        assert_eq!(subjects, ["two", "three"]);
    }

    #[tokio::test]
    async fn an_archive_is_read_in_place_and_only_its_mailbox_entries_count() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("takeout-001.zip");
        write_archive(
            &path,
            &[
                ("Takeout/archive_browser.html", "<p>not mail</p>"),
                ("Takeout/Mail/FreshDirect.mbox", TWO_AND_THREE),
            ],
        );
        let subjects = subjects(&TakeoutExport::new(&path)).await.expect("read");
        assert_eq!(subjects, ["two", "three"]);
        assert_eq!(
            std::fs::read_dir(dir.path()).expect("list").count(),
            1,
            "nothing extracted"
        );
    }

    #[tokio::test]
    async fn a_folder_yields_every_archive_and_mailbox_in_name_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_archive(
            &dir.path().join("takeout-002.zip"),
            &[("Takeout/Mail/FreshDirect.mbox", TWO_AND_THREE)],
        );
        let unzipped = dir.path().join("earlier/Takeout/Mail");
        std::fs::create_dir_all(&unzipped).expect("dirs");
        std::fs::write(unzipped.join("FreshDirect.mbox"), ONE).expect("write");
        std::fs::write(dir.path().join("notes.txt"), "ignored").expect("write");
        let subjects = subjects(&TakeoutExport::new(dir.path()))
            .await
            .expect("read");
        assert_eq!(subjects, ["one", "two", "three"]);
    }

    #[tokio::test]
    async fn a_missing_path_is_a_read_error_that_says_how_to_get_an_export() {
        let result = subjects(&TakeoutExport::new("/nonexistent/export")).await;
        let Err(MailError::Read(text)) = result else {
            panic!("expected a read error, got {result:?}");
        };
        assert!(text.contains("/nonexistent/export"), "{text}");
        assert!(text.contains("Google Takeout"), "{text}");
        assert!(text.contains("docs/runbooks/takeout-export.md"), "{text}");
    }

    #[tokio::test]
    async fn a_folder_without_exports_and_an_archive_without_mail_are_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let empty = subjects(&TakeoutExport::new(dir.path())).await;
        assert!(
            matches!(empty, Err(MailError::Read(ref t)) if t.contains("no .zip archive")),
            "{empty:?}"
        );
        write_archive(
            &dir.path().join("takeout-drive.zip"),
            &[("Takeout/Drive/a.txt", "x")],
        );
        let no_mail = subjects(&TakeoutExport::new(dir.path())).await;
        assert!(
            matches!(no_mail, Err(MailError::Read(ref t)) if t.contains("no .mbox entry")),
            "{no_mail:?}"
        );
    }

    #[tokio::test]
    async fn a_broken_archive_is_an_error_after_the_messages_before_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.mbox"), ONE).expect("write");
        std::fs::write(dir.path().join("broken.zip"), "not a zip").expect("write");
        let mut stream = TakeoutExport::new(dir.path()).messages();
        let mut good = 0;
        let mut failed = false;
        while let Some(item) = stream.recv().await {
            match item {
                Ok(_) => good += 1,
                Err(MailError::Read(text)) => {
                    assert!(text.contains("broken.zip"), "{text}");
                    failed = true;
                }
                Err(other) => panic!("unexpected {other:?}"),
            }
        }
        assert!(failed, "the broken archive must be reported");
        assert_eq!(
            good, 1,
            "the mailbox before the broken archive is delivered"
        );
    }

    #[tokio::test]
    async fn a_mailbox_six_folders_down_is_read_and_one_seven_folders_down_is_not() {
        let dir = tempfile::tempdir().expect("tempdir");
        let six = dir.path().join("1/2/3/4/5/6");
        let seven = six.join("z");
        std::fs::create_dir_all(&seven).expect("dirs");
        std::fs::write(six.join("a.mbox"), ONE).expect("write");
        std::fs::write(seven.join("b.mbox"), TWO_AND_THREE).expect("write");
        let from_the_top = subjects(&TakeoutExport::new(dir.path()))
            .await
            .expect("read");
        assert_eq!(from_the_top, ["one"]);
        // From one folder down, the second file is six folders down and is read too.
        let from_one_down = subjects(&TakeoutExport::new(dir.path().join("1")))
            .await
            .expect("read");
        assert_eq!(from_one_down, ["one", "two", "three"]);
    }
}
