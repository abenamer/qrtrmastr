//! Ports: what the use cases need from the outside world, as traits (REAS-ARCH-001, 004).
//!
//! Adapters implement them. No SQLx, file or network type appears here (REAS-ARCH-002); an
//! adapter maps its own errors into [`StoreError`] or [`MailError`]. Async ports use native
//! `async fn`. The use cases are statically dispatched and awaited on the composition root's
//! runtime without `tokio::spawn`, so the returned futures are not required to be `Send`
//! (REAS-ARCH-006); the lint that warns about that is allowed for that reason.

use std::collections::HashSet;

use chrono::{DateTime, NaiveDate, Utc};
use thiserror::Error;

use crate::domain::identity::Normalized;
pub use crate::domain::ids::AliasId;
use crate::domain::ids::{MessageKey, OrderNumber, ProductId, RawMessageId};
use crate::domain::receipt::Receipt;
use crate::domain::recommend::Purchase;
use crate::domain::states::{MessageKind, ParseOutcome, ParseState, Platform, SoldBy};

/// A failure inside a store adapter, with the adapter's error kept as the source.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database, {operation}: {source}")]
    Database {
        operation: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("migration: {0}")]
    Migration(String),
    /// A stored row did not satisfy a domain invariant (REAS-DATA-001: rows map to the domain
    /// through validation).
    #[error("stored row {table}#{id} is invalid: {reason}")]
    InvalidRow {
        table: &'static str,
        id: i64,
        reason: String,
    },
    /// A conditional state update changed no row: the message moved on since it was read
    /// (REAS-STATE-003).
    #[error("message {message} is no longer {expected} at version {version}")]
    Conflict {
        message: RawMessageId,
        expected: ParseState,
        version: i32,
    },
}

/// A failure reading or preparing mail.
#[derive(Debug, Error)]
pub enum MailError {
    #[error("cannot read the mail source: {0}")]
    Read(String),
    #[error("message {key} cannot be parsed as mail: {reason}")]
    Malformed { key: String, reason: String },
    /// Personal data survived redaction. The message is not stored.
    #[error("message {key}: {kind} survived redaction; not stored")]
    RedactionFailed { key: String, kind: &'static str },
}

/// What a migration run did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MigrationReport {
    /// Migrations applied by this run, as `version description`.
    pub applied: Vec<String>,
    /// Migrations that were already applied before this run.
    pub already_applied: usize,
}

/// The schema of the store: the only port the `migrate` command needs.
#[allow(async_fn_in_trait)]
pub trait SchemaStore {
    /// Applies the pending migrations. Idempotent: a second call applies nothing.
    async fn migrate(&self) -> Result<MigrationReport, StoreError>;
}

/// One message as the source holds it: the raw RFC 5322 bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawMail {
    pub bytes: Vec<u8>,
}

/// Where mail comes from: a Takeout export today, the Gmail API later.
#[allow(async_fn_in_trait)]
pub trait MailSource {
    async fn messages(&self) -> Result<Vec<RawMail>, MailError>;
}

/// A message after MIME parsing, classification and redaction, ready to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRawMessage {
    pub platform: Platform,
    pub key: MessageKey,
    pub received_at: DateTime<Utc>,
    pub subject: String,
    pub kind: MessageKind,
    pub dkim_pass: bool,
    pub body_html: Option<String>,
    pub body_text: Option<String>,
    pub source_sha256: [u8; 32],
    pub redaction_version: i32,
}

/// What preparing one raw message produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prepared {
    Message(NewRawMessage),
    /// Not from the platform's sender; counted, never stored.
    OtherSender {
        from: String,
    },
}

/// Turns raw mail into a redacted, classified message. One implementation per platform; it
/// holds the sender rule, the subject rules and the redactor.
pub trait MailReader {
    fn prepare(&self, raw: &RawMail) -> Result<Prepared, MailError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    Inserted(RawMessageId),
    AlreadyKnown,
}

/// A stored message in state `fetched`, as the parser needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingMessage {
    pub id: RawMessageId,
    pub key: MessageKey,
    pub kind: MessageKind,
    pub dkim_pass: bool,
    pub received_at: DateTime<Utc>,
    pub body_html: Option<String>,
    pub parse_state: ParseState,
    pub version: i32,
}

/// A stored message's redacted body, for writing a local fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredBody {
    pub platform: Platform,
    pub key: MessageKey,
    pub received_at: DateTime<Utc>,
    pub body_html: Option<String>,
}

#[allow(async_fn_in_trait)]
pub trait RawMessageStore {
    async fn known_keys(&self) -> Result<HashSet<String>, StoreError>;
    /// The redacted body stored under `key`, if any.
    async fn body_by_key(&self, key: &str) -> Result<Option<StoredBody>, StoreError>;
    /// Inserts unless the key or the source hash is already stored.
    async fn insert(&self, message: &NewRawMessage) -> Result<InsertOutcome, StoreError>;
    /// Messages of the platform in state `fetched`, oldest first.
    async fn pending(&self, platform: Platform) -> Result<Vec<PendingMessage>, StoreError>;
}

/// Which parser produced a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParserTag {
    pub name: &'static str,
    pub version: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptRecord {
    pub parser: ParserTag,
    pub outcome: ParseOutcome,
    pub error: Option<String>,
    pub anomaly_count: i32,
}

/// An order to write, with the alias each line resolved to, in line order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderWrite<'a> {
    pub receipt: &'a Receipt,
    pub reconciled: bool,
    pub alias_ids: Vec<AliasId>,
}

/// Everything one parse attempt changes, written in one transaction (REAS-DATA-002): the
/// attempt row, the conditional state transition, and the order with its lines and anomalies
/// when the attempt produced one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseFinish<'a> {
    pub message: RawMessageId,
    pub expected_state: ParseState,
    pub expected_version: i32,
    pub next_state: ParseState,
    pub attempt: AttemptRecord,
    pub order: Option<OrderWrite<'a>>,
}

#[allow(async_fn_in_trait)]
pub trait GroceryStore {
    /// Fails with [`StoreError::Conflict`] when the message is no longer in the expected state
    /// and version; nothing is written then.
    async fn finish_parse(&self, finish: &ParseFinish<'_>) -> Result<(), StoreError>;
    async fn purchases(&self) -> Result<Vec<Purchase>, StoreError>;
    async fn delivery_dates(&self) -> Result<Vec<NaiveDate>, StoreError>;
    async fn muted_products(&self) -> Result<HashSet<ProductId>, StoreError>;
}

/// What a line offers the catalog to find or create its alias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasRequest<'a> {
    pub normalized: &'a Normalized,
    pub raw_name: &'a str,
    pub sold_by: SoldBy,
    pub department: Option<&'a str>,
    pub product_url_path: Option<&'a str>,
}

/// How a product is shown: its name, size and department, and the name the most recent receipt
/// printed, which is what the FreshDirect search box wants.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProductLabel {
    pub id: ProductId,
    pub display_name: String,
    pub size_text: Option<String>,
    pub department: Option<String>,
    pub latest_raw_name: String,
}

#[allow(async_fn_in_trait)]
pub trait Catalog {
    /// The alias for a normalized name and size, created with a new product when unseen.
    async fn resolve_alias(&self, request: &AliasRequest<'_>) -> Result<AliasId, StoreError>;
    async fn product_labels(&self) -> Result<Vec<ProductLabel>, StoreError>;
}

/// Runs the receipt parsers over one HTML body. The registry of format adapters implements it;
/// it is a trait so a use-case test can use a fixed double.
pub trait ReceiptParsing {
    fn parse(&self, html: &str) -> ParseAttempt;
}

/// What the parsers made of one body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseAttempt {
    /// No parser recognized the layout.
    Unsupported,
    /// A parser recognized the layout and could not read it.
    Failed { parser: ParserTag, error: String },
    Parsed {
        parser: ParserTag,
        receipt: Box<Receipt>,
    },
}

impl ParseAttempt {
    pub fn order_number(&self) -> Option<&OrderNumber> {
        match self {
            ParseAttempt::Parsed { receipt, .. } => Some(&receipt.order_number),
            ParseAttempt::Unsupported | ParseAttempt::Failed { .. } => None,
        }
    }
}
