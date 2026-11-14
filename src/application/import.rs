//! `qm import`, first half: read every message of the source, prepare it, store the new ones.

use serde::Serialize;

use super::ports::{InsertOutcome, MailError, MailReader, MailSource, Prepared, RawMessageStore};

/// What one import run did. `redaction_failures` is non-empty when a message was refused
/// because personal data survived; the command then exits with a failure status.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    pub read: usize,
    pub inserted: usize,
    pub already_known: usize,
    pub other_sender: usize,
    pub malformed: Vec<String>,
    pub redaction_failures: Vec<String>,
}

pub struct ImportMail<M: MailSource, R: MailReader, S: RawMessageStore> {
    source: M,
    reader: R,
    store: S,
}

impl<M: MailSource, R: MailReader, S: RawMessageStore> ImportMail<M, R, S> {
    pub fn new(source: M, reader: R, store: S) -> Self {
        Self {
            source,
            reader,
            store,
        }
    }

    /// Reads the whole source and stores each message once. Idempotent: a key or a source hash
    /// already stored is counted as known and skipped. A malformed message or a redaction
    /// failure is recorded and the run continues; a store failure stops it.
    pub async fn run(&self) -> Result<ImportReport, super::ports::StoreError> {
        let mut report = ImportReport::default();
        let raws = match self.source.messages().await {
            Ok(raws) => raws,
            Err(error) => {
                report.malformed.push(error.to_string());
                return Ok(report);
            }
        };
        let known = self.store.known_keys().await?;
        for raw in &raws {
            report.read += 1;
            match self.reader.prepare(raw) {
                Ok(Prepared::OtherSender { .. }) => report.other_sender += 1,
                Ok(Prepared::Message(message)) => {
                    if known.contains(message.key.as_str()) {
                        report.already_known += 1;
                        continue;
                    }
                    match self.store.insert(&message).await? {
                        InsertOutcome::Inserted(_) => report.inserted += 1,
                        InsertOutcome::AlreadyKnown => report.already_known += 1,
                    }
                }
                Err(MailError::RedactionFailed { key, kind }) => {
                    report.redaction_failures.push(format!("{key}: {kind}"));
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

    use chrono::Utc;

    use super::*;
    use crate::application::ports::{NewRawMessage, PendingMessage, StoreError, StoredBody};
    use crate::domain::ids::{MessageKey, RawMessageId};
    use crate::domain::states::{MessageKind, Platform};

    struct TwoMails;

    impl MailSource for TwoMails {
        async fn messages(&self) -> Result<Vec<RawMail>, MailError> {
            Ok(vec![
                RawMail {
                    bytes: b"a".to_vec(),
                },
                RawMail {
                    bytes: b"b".to_vec(),
                },
                RawMail {
                    bytes: b"other".to_vec(),
                },
                RawMail {
                    bytes: b"leaky".to_vec(),
                },
            ])
        }
    }

    use crate::application::ports::RawMail;

    struct ByteReader;

    fn message(key: &str) -> NewRawMessage {
        NewRawMessage {
            platform: Platform::Freshdirect,
            key: MessageKey::from_message_id(key).expect("key"),
            received_at: Utc::now(),
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
                }),
                bytes => Ok(Prepared::Message(message(
                    std::str::from_utf8(bytes).expect("ascii"),
                ))),
            }
        }
    }

    #[derive(Default)]
    struct MemoryStore {
        known: HashSet<String>,
        inserted: RefCell<Vec<String>>,
    }

    impl RawMessageStore for MemoryStore {
        async fn known_keys(&self) -> Result<HashSet<String>, StoreError> {
            Ok(self.known.clone())
        }
        async fn body_by_key(&self, _: &str) -> Result<Option<StoredBody>, StoreError> {
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
            known: HashSet::from(["a".to_owned()]),
            inserted: RefCell::new(Vec::new()),
        };
        let import = ImportMail::new(TwoMails, ByteReader, store);
        let report = import.run().await.expect("import");
        assert_eq!(report.read, 4);
        assert_eq!(report.inserted, 1);
        assert_eq!(report.already_known, 1);
        assert_eq!(report.other_sender, 1);
        assert_eq!(report.redaction_failures, vec!["leaky: phone".to_owned()]);
        assert_eq!(*import.store.inserted.borrow(), vec!["b".to_owned()]);
    }

    struct BrokenStore;

    impl RawMessageStore for BrokenStore {
        async fn known_keys(&self) -> Result<HashSet<String>, StoreError> {
            Err(StoreError::Migration("down".to_owned()))
        }
        async fn body_by_key(&self, _: &str) -> Result<Option<StoredBody>, StoreError> {
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
        assert!(matches!(result, Err(StoreError::Migration(_))));
    }
}
