//! `qm import`, first half: read every message of the source, prepare it, store the new ones.

use std::collections::BTreeMap;

use thiserror::Error;

use super::count;
use super::ports::{
    InsertOutcome, MailError, MailReader, MailSource, Prepared, RawMessageStore, StoreError,
};
use crate::domain::states::Platform;

/// Why an import run stopped. A source that cannot be read at all, or a store that fails, ends
/// the run with an error; the command then exits with a failure status (REAS-RUST-005). One
/// message that cannot be parsed or redacted is counted in the report instead, and the run goes on.
#[derive(Debug, Error)]
pub enum ImportError {
    /// The stream of the source delivered an error: the source could not be read. The
    /// messages read before it stay stored.
    #[error(transparent)]
    Source(#[from] MailError),
    /// The store could not list the known keys or insert a message.
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// What one import run did. `redaction_failures` is non-empty when a message was refused
/// because personal data survived; the command then exits with a failure status.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// How many messages the source delivered, of every sender. Each one is counted in
    /// exactly one of `inserted`, `already_known`, `other_sender`, `malformed` and
    /// `redaction_failures`.
    pub read: usize,
    /// How many messages this run stored as new rows.
    pub inserted: usize,
    /// The new messages by platform; the counts sum to `inserted`.
    pub inserted_by_platform: BTreeMap<Platform, usize>,
    /// How many messages of a configured sender were skipped because the store already held
    /// their message key or their source hash.
    pub already_known: usize,
    /// How many messages came from an address that is not a configured sender. They are not
    /// stored.
    pub other_sender: usize,
    /// One text for each message that the reader could not prepare for a cause other than
    /// redaction: the error with the message key and what is wrong. The message is not
    /// stored.
    pub malformed: Vec<String>,
    /// One text for each message in which personal data survived redaction: the message key
    /// and the kind of the redaction term, never the term. The message is not stored.
    pub redaction_failures: Vec<String>,
    /// The refused messages counted by where and how the term survived; never the text.
    pub refused_by_cause: BTreeMap<String, usize>,
}

/// The first half of an import: it takes the raw mail of a source, has the reader prepare
/// each message, and stores the new ones. It does not know which source the mail comes from.
pub struct ImportMail<M: MailSource, R: MailReader, S: RawMessageStore> {
    source: M,
    reader: R,
    store: S,
}

impl<M: MailSource, R: MailReader, S: RawMessageStore> ImportMail<M, R, S> {
    /// Builds the use case from the source of the mail, the reader that classifies and
    /// redacts a message, and the store of raw messages.
    pub fn new(source: M, reader: R, store: S) -> Self {
        Self {
            source,
            reader,
            store,
        }
    }

    /// Reads the source message by message and stores each message once. Idempotent: a key or a source hash
    /// already stored is counted as known and skipped. A malformed message or a redaction
    /// failure is recorded and the run continues; a source that cannot be read, or a store
    /// failure, stops it.
    ///
    /// # Errors
    ///
    /// - [`ImportError::Store`] when the store cannot list the known keys or insert a
    ///   message. It carries the [`StoreError`].
    /// - [`ImportError::Source`] when the stream delivers an error instead of a message. It
    ///   carries the [`MailError`], which for both sources is [`MailError::Read`].
    ///
    /// The messages stored before the error stay stored; the next run counts them as known.
    pub async fn run(&self) -> Result<ImportReport, ImportError> {
        let mut report = ImportReport::default();
        let known = self.store.known_keys().await?;
        let mut stream = self.source.messages();
        while let Some(raw) = stream.recv().await {
            let raw = raw?;
            count(&mut report.read);
            match self.reader.prepare(&raw) {
                Ok(Prepared::OtherSender { .. }) => count(&mut report.other_sender),
                Ok(Prepared::Message(message)) => {
                    if known.contains(&message.key) {
                        count(&mut report.already_known);
                        continue;
                    }
                    match self.store.insert(&message).await? {
                        InsertOutcome::Inserted(_) => {
                            count(&mut report.inserted);
                            count(
                                report
                                    .inserted_by_platform
                                    .entry(message.platform)
                                    .or_default(),
                            );
                        }
                        InsertOutcome::AlreadyKnown => count(&mut report.already_known),
                    }
                }
                Err(MailError::RedactionFailed { key, kind, context }) => {
                    report.redaction_failures.push(format!("{key}: {kind}"));
                    count(report.refused_by_cause.entry(context).or_default());
                }
                Err(error) => report.malformed.push(error.to_string()),
            }
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashSet;

    use jiff::Timestamp;

    use super::*;
    use crate::application::ports::{NewRawMessage, PendingMessage, StoredBody};
    use crate::domain::ids::{MessageKey, RawMessageId};
    use crate::domain::states::MessageKind;

    struct TwoMails;

    fn stream_of(items: Vec<Result<RawMail, MailError>>) -> MailStream {
        let (tx, rx) = tokio::sync::mpsc::channel(items.len().max(1));
        for item in items {
            tx.try_send(item).expect("the channel holds every item");
        }
        rx
    }

    impl MailSource for TwoMails {
        fn messages(&self) -> MailStream {
            stream_of(vec![
                Ok(RawMail {
                    bytes: b"a".to_vec(),
                }),
                Ok(RawMail {
                    bytes: b"b".to_vec(),
                }),
                Ok(RawMail {
                    bytes: b"other".to_vec(),
                }),
                Ok(RawMail {
                    bytes: b"leaky".to_vec(),
                }),
            ])
        }
    }

    use crate::application::ports::{MailStream, RawMail};

    struct ByteReader;

    fn message(key: &str) -> NewRawMessage {
        NewRawMessage {
            platform: Platform::Freshdirect,
            key: MessageKey::from_message_id(key).expect("key"),
            received_at: Timestamp::now(),
            subject: "s".to_owned(),
            kind: MessageKind::Receipt,
            dkim_pass: true,
            body_html: Some("<p>x</p>".to_owned()),
            body_text: None,
            source_sha256: [0; 32],
            redaction_version: 1,
        }
    }

    impl MailReader for ByteReader {
        fn prepare(&self, raw: &RawMail) -> Result<Prepared, MailError> {
            match raw.bytes.as_slice() {
                b"other" => Ok(Prepared::OtherSender {
                    from: "x@y.example".to_owned(),
                }),
                b"leaky" => Err(MailError::RedactionFailed {
                    key: "leaky".to_owned(),
                    kind: "phone",
                    context: "freshdirect receipt, html body: phone".to_owned(),
                }),
                bytes => Ok(Prepared::Message(message(
                    std::str::from_utf8(bytes).expect("ascii"),
                ))),
            }
        }
    }

    #[derive(Default)]
    struct MemoryStore {
        known: HashSet<MessageKey>,
        inserted: RefCell<Vec<String>>,
    }

    impl RawMessageStore for MemoryStore {
        async fn known_keys(&self) -> Result<HashSet<MessageKey>, StoreError> {
            Ok(self.known.clone())
        }
        async fn newest_received_at(&self) -> Result<Option<jiff::Timestamp>, StoreError> {
            Ok(None)
        }
        async fn body_by_key(&self, _: &MessageKey) -> Result<Option<StoredBody>, StoreError> {
            Ok(None)
        }
        async fn insert(&self, message: &NewRawMessage) -> Result<InsertOutcome, StoreError> {
            self.inserted
                .borrow_mut()
                .push(message.key.as_str().to_owned());
            Ok(InsertOutcome::Inserted(RawMessageId::new(1).expect("id")))
        }
        async fn pending(&self, _: Platform) -> Result<Vec<PendingMessage>, StoreError> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn known_messages_are_skipped_and_a_redaction_failure_is_reported_not_stored() {
        let store = MemoryStore {
            known: HashSet::from([MessageKey::from_message_id("a").expect("key")]),
            inserted: RefCell::new(Vec::new()),
        };
        let import = ImportMail::new(TwoMails, ByteReader, store);
        let report = import.run().await.expect("import");
        assert_eq!(report.read, 4);
        assert_eq!(report.inserted, 1);
        assert_eq!(
            report.inserted_by_platform,
            BTreeMap::from([(Platform::Freshdirect, 1)])
        );
        assert_eq!(report.already_known, 1);
        assert_eq!(report.other_sender, 1);
        assert_eq!(report.redaction_failures, vec!["leaky: phone".to_owned()]);
        assert_eq!(
            report.refused_by_cause,
            BTreeMap::from([("freshdirect receipt, html body: phone".to_owned(), 1)])
        );
        assert_eq!(*import.store.inserted.borrow(), vec!["b".to_owned()]);
    }

    struct BrokenStore;

    impl RawMessageStore for BrokenStore {
        async fn known_keys(&self) -> Result<HashSet<MessageKey>, StoreError> {
            Err(StoreError::Migration("down".to_owned()))
        }
        async fn newest_received_at(&self) -> Result<Option<jiff::Timestamp>, StoreError> {
            unreachable!()
        }
        async fn body_by_key(&self, _: &MessageKey) -> Result<Option<StoredBody>, StoreError> {
            unreachable!()
        }
        async fn insert(&self, _: &NewRawMessage) -> Result<InsertOutcome, StoreError> {
            unreachable!("never reached after known_keys fails")
        }
        async fn pending(&self, _: Platform) -> Result<Vec<PendingMessage>, StoreError> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn a_store_failure_stops_the_run() {
        let result = ImportMail::new(TwoMails, ByteReader, BrokenStore)
            .run()
            .await;
        assert!(matches!(
            result,
            Err(ImportError::Store(StoreError::Migration(_)))
        ));
    }

    struct MissingExport;

    impl MailSource for MissingExport {
        fn messages(&self) -> MailStream {
            stream_of(vec![
                Ok(RawMail {
                    bytes: b"b".to_vec(),
                }),
                Err(MailError::Read("/data/missing: no such file".to_owned())),
            ])
        }
    }

    #[tokio::test]
    async fn a_source_that_cannot_be_read_is_an_error_not_an_empty_report() {
        let import = ImportMail::new(MissingExport, ByteReader, MemoryStore::default());
        let result = import.run().await;
        assert!(
            matches!(result, Err(ImportError::Source(MailError::Read(ref text))) if text.contains("/data/missing")),
            "{result:?}"
        );
        // The message read before the failure is stored; a second run skips it.
        assert_eq!(*import.store.inserted.borrow(), vec!["b".to_owned()]);
    }
}
