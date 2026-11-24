//! `qm import`, first half: read every message of the source, prepare it, store the new ones.

use std::collections::BTreeMap;

use thiserror::Error;

use super::count;
use super::ports::{
    InsertOutcome, MailError, MailReader, MailSource, Prepared, RawMessageStore, ReceivedPages,
    StoreError,
};
use crate::domain::states::MessageKind;
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
    /// The page of each receipt that this run read, for the parse run that follows. It is
    /// not stored and not printed.
    pub pages: ReceivedPages,
}

/// The first half of an import: it takes the raw mail of a source, has the reader prepare
/// each message, and stores the new ones. It does not know which source the mail comes from.
pub struct ImportMail<M: MailSource, R: MailReader, S: RawMessageStore> {
    source: M,
    reader: R,
    store: S,
    /// The platforms whose receipts a parser reads, when the caller says so: the page of a
    /// receipt of another platform is not kept. `None` keeps the page of each receipt.
    pages_of: Option<Vec<Platform>>,
}

impl<M: MailSource, R: MailReader, S: RawMessageStore> ImportMail<M, R, S> {
    /// Builds the use case from the source of the mail, the reader that classifies and
    /// redacts a message, and the store of raw messages.
    pub fn new(source: M, reader: R, store: S) -> Self {
        Self {
            source,
            reader,
            store,
            pages_of: None,
        }
    }

    /// Keeps the pages of the receipts of these platforms only. A page is the whole mail in
    /// memory until the parse run, and a platform without a parser needs none.
    #[must_use]
    pub fn keeping_pages_of(mut self, platforms: &[Platform]) -> Self {
        self.pages_of = Some(platforms.to_vec());
        self
    }

    /// Reads the source message by message and stores the record of each message once; the
    /// page of each receipt is returned for the parse run and is not stored. Idempotent: a
    /// key or a source hash already stored is counted as known and no row is written. A malformed message or a redaction
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
                    let record = message.record;
                    // The page of a receipt waits in memory for the parse run, also when the
                    // message is a known one: a receipt that a parser could not read before
                    // is read again from the mail. No other kind has a parser.
                    if record.kind == MessageKind::Receipt
                        && self
                            .pages_of
                            .as_ref()
                            .is_none_or(|platforms| platforms.contains(&record.platform))
                    {
                        report.pages.insert(record.key.clone(), message.html);
                    }
                    if known.contains(&record.key) {
                        count(&mut report.already_known);
                        continue;
                    }
                    match self.store.insert(&record).await? {
                        InsertOutcome::Inserted(_) => {
                            count(&mut report.inserted);
                            count(
                                report
                                    .inserted_by_platform
                                    .entry(record.platform)
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
