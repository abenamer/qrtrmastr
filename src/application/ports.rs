//! Ports: what the use cases need from the outside world, as traits (REAS-ARCH-001, 004).
//!
//! Adapters implement them. No SQLx, file or network type appears here (REAS-ARCH-002); an
//! adapter maps its own errors into [`StoreError`] or [`MailError`]. Async ports use native
//! `async fn`. The use cases are statically dispatched and awaited on the composition root's
//! runtime without `tokio::spawn`, so the returned futures are not required to be `Send`
//! (REAS-ARCH-006); the lint that warns about that is allowed for that reason.

use std::collections::HashSet;

use jiff::Timestamp;
use jiff::civil::Date;
use thiserror::Error;

use crate::domain::identity::Normalized;
pub use crate::domain::ids::AliasId;
use crate::domain::ids::{AdditionId, MessageKey, OrderNumber, ProductId, RawMessageId};
use crate::domain::list::{Addition, ItemText};
use crate::domain::receipt::Receipt;
use crate::domain::recommend::Purchase;
use crate::domain::states::{
    AddReason, MessageKind, ParseOutcome, ParseState, Platform, SenderCheck, SoldBy,
};

/// A failure inside a store adapter, with the adapter's error kept as the source.
#[derive(Debug, Error)]
pub enum StoreError {
    /// The connection failed, or the database could not run a statement.
    #[error("database, {operation}: {source}")]
    Database {
        /// What the adapter was doing, as a short fixed text such as `insert order line`. It
        /// holds no stored value.
        operation: &'static str,
        /// The error of the adapter's database library.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    /// A migration could not be applied. The text is the message of the migration runner.
    #[error("migration: {0}")]
    Migration(String),
    /// A stored row did not satisfy a domain invariant (REAS-DATA-001: rows map to the domain
    /// through validation).
    #[error("stored row {table}#{id} is invalid: {reason}")]
    InvalidRow {
        /// The table or the view that the row was read from.
        table: &'static str,
        /// The identity of the row. For a row of the `purchases` view and of
        /// `product_preferences` it is the identity of the product.
        id: i64,
        /// The text of the domain error that refused the value.
        reason: String,
    },
    /// A conditional state update changed no row: the message moved on since it was read
    /// (REAS-STATE-003).
    #[error("message {message} is no longer {expected} at version {version}")]
    Conflict {
        /// The message that the update was for.
        message: RawMessageId,
        /// The parse state that the message had when it was read.
        expected: ParseState,
        /// The value of `raw_messages.version` when the message was read. Each state
        /// transition adds one to it.
        version: i32,
    },
}

/// A failure reading or preparing mail.
#[derive(Debug, Error)]
pub enum MailError {
    /// The source could not be read: a path that cannot be opened, a folder or an archive
    /// without mail, a broken archive, or a request to the mailbox that failed. The text says
    /// which. It ends the run; no message follows it in the stream.
    #[error("cannot read the mail source: {0}")]
    Read(String),
    /// One message cannot be used: its bytes are not an RFC 5322 message, or it has no `Date`
    /// header that reads as a date. The import records it and reads on.
    #[error("message {key} cannot be parsed as mail: {reason}")]
    Malformed {
        /// The message key, written as its kind, a colon and its text. It is the SHA-256 of
        /// the raw bytes when the bytes are not a message.
        key: String,
        /// What is wrong with the message, as a fixed text.
        reason: String,
    },
    /// Personal data survived redaction. The message is not stored.
    #[error("message {key}: {kind} survived redaction; not stored")]
    RedactionFailed {
        /// The message key, written as its kind, a colon and its text.
        key: String,
        /// The kind of the redaction term that survived: `name`, `email`, `address`, `phone`
        /// or `card`. Never the term.
        kind: &'static str,
        /// Where and how, for the report: platform, message kind, part and the assertion that
        /// failed. It never carries the text.
        context: String,
    },
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
#[expect(
    async_fn_in_trait,
    reason = "a native async fn in a port (REAS-ARCH-006); each use case runs on the task of the composition root, so no caller needs the future to be Send"
)]
pub trait SchemaStore {
    /// Applies the pending migrations. Idempotent: a second call applies nothing.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the list of the applied migrations cannot be read.
    /// - [`StoreError::Migration`] when a migration cannot be applied.
    async fn migrate(&self) -> Result<MigrationReport, StoreError>;
}

/// One message as the source holds it: the raw RFC 5322 bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawMail {
    /// The whole message, headers and body, as the source delivered it. It is not redacted:
    /// it is read once to prepare the message and is never stored.
    pub bytes: Vec<u8>,
}

/// The messages of a source, one at a time, through a bounded channel: the source is never
/// held in memory whole. A failure to read arrives as an item; the stream ends after it.
pub type MailStream = tokio::sync::mpsc::Receiver<Result<RawMail, MailError>>;

/// Where mail comes from: a Takeout export, or the Gmail API.
pub trait MailSource {
    /// Starts to read the source and returns the stream of its messages.
    ///
    /// The source reads ahead of the use case by no more than the channel holds, and it stops
    /// when the receiver is dropped. A [`MailError::Read`] is the last item of the stream.
    /// Both adapters read on a blocking task, so the caller must be on a Tokio runtime.
    fn messages(&self) -> MailStream;
}

/// A message after MIME parsing, classification and redaction, ready to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRawMessage {
    /// The platform whose configured sender address is the exact `From` address of the mail.
    pub platform: Platform,
    /// The message key: the `Message-ID` header, or the SHA-256 of the raw bytes when the
    /// header is missing, is empty, or carries a configured name, mail address, address line
    /// or phone number.
    pub key: MessageKey,
    /// The moment in the `Date` header. The sender wrote it: it is not the moment of the
    /// import, and it can be in the future.
    pub received_at: Timestamp,
    /// The subject after redaction.
    pub subject: String,
    /// Which of the platform's mails this is, classified from the subject before redaction.
    pub kind: MessageKind,
    /// `true` when an `Authentication-Results` header with the server name that Gmail writes,
    /// `mx.google.com`, says a DKIM signature verified for the domain of the sender or one of
    /// its subdomains. The header is read as it stands in the mail; nothing proves who wrote
    /// it. `false` also when the mail has no such header.
    pub dkim_pass: bool,
    /// The HTML body after redaction. For a mail with a text part only, the mail library
    /// gives that text as HTML, so this is `None` only when the mail has neither part.
    pub body_html: Option<String>,
    /// The plain-text body after redaction. For a mail with an HTML part only, the mail
    /// library gives that HTML as text, so this is `None` only when the mail has neither part.
    pub body_text: Option<String>,
    /// The SHA-256 of the raw bytes before redaction. The store keeps it unique, so a mail
    /// that arrives again under another key is stored once.
    pub source_sha256: [u8; 32],
    /// The version of the redactor that made the subject and the bodies. A higher number is a
    /// later redactor.
    pub redaction_version: i32,
}

/// What preparing one raw message produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prepared {
    /// The mail is from a configured sender and no redaction term survived: the message to
    /// store.
    Message(NewRawMessage),
    /// Not from the platform's sender; counted, never stored.
    OtherSender {
        /// The address of the `From` header, not redacted; empty when the mail has none. The
        /// import does not read it.
        from: String,
    },
}

/// Turns raw mail into a redacted, classified message. The one implementation reads the
/// mail of every platform; it holds the sender rules, the subject rules and the redactor.
pub trait MailReader {
    /// Reads the headers and the bodies of one raw mail, finds its key, platform and kind,
    /// and redacts the subject and the bodies.
    ///
    /// # Errors
    ///
    /// An error is about this one message. The import records it and reads on.
    ///
    /// - [`MailError::Malformed`] when the bytes are not an RFC 5322 message, or when a mail
    ///   of a configured sender has no `Date` header that reads as a date.
    /// - [`MailError::RedactionFailed`] when a redaction term survives in the subject, the
    ///   HTML part or the text part. Nothing of the message is returned.
    fn prepare(&self, raw: &RawMail) -> Result<Prepared, MailError>;
}

/// What [`RawMessageStore::insert`] did with a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    /// The message is new. The value is the identity of its row in `raw_messages`.
    Inserted(RawMessageId),
    /// The key or the source hash was stored before. No row was written.
    AlreadyKnown,
}

/// A stored message in state `fetched`, as the parser needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingMessage {
    /// The identity of the row in `raw_messages`.
    pub id: RawMessageId,
    /// The message key that the message is stored under.
    pub key: MessageKey,
    /// The kind that the import classified from the subject. Only a `receipt` is parsed; a
    /// message of another kind stays in `fetched`.
    pub kind: MessageKind,
    /// Whether the mail had a DKIM pass when it was imported. The order of a receipt without
    /// one is tagged `non-verified`.
    pub dkim_pass: bool,
    /// The moment in the `Date` header of the mail. The pending messages are ordered by it.
    pub received_at: Timestamp,
    /// The redacted HTML body, which the parsers read. `None` when the mail had neither an
    /// HTML part nor a text part; a receipt with `None` gets the outcome `failed`.
    pub body_html: Option<String>,
    /// The parse state when the row was read. The transition changes the row only if this
    /// and `version` still hold.
    pub parse_state: ParseState,
    /// The value of `raw_messages.version` when the row was read: the number of state
    /// transitions so far, zero for a message that was never parsed.
    pub version: i32,
}

/// A stored message's redacted body, for writing a local fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredBody {
    /// The platform of the message. The fixture goes into a folder of this name.
    pub platform: Platform,
    /// The message key. The file name of the fixture is made from its text.
    pub key: MessageKey,
    /// The moment in the `Date` header of the mail. Its year in UTC names the folder of the
    /// fixture.
    pub received_at: Timestamp,
    /// The redacted HTML body. `None` when the mail had neither an HTML part nor a text
    /// part; no fixture is written then.
    pub body_html: Option<String>,
}

#[expect(
    async_fn_in_trait,
    reason = "a native async fn in a port (REAS-ARCH-006); each use case runs on the task of the composition root, so no caller needs the future to be Send"
)]
/// The raw messages: what the import writes, and what the parse step, a fetch and
/// `qm fixture add` read.
pub trait RawMessageStore {
    /// The text of every stored message key, without its kind. The import reads the set once
    /// when it starts, and skips a message whose key is in it.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection or the statement fails.
    async fn known_keys(&self) -> Result<HashSet<String>, StoreError>;
    /// When the newest stored message was received; `None` for an empty store. A fetch asks
    /// the mailbox for what came after it.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection or the statement fails.
    async fn newest_received_at(&self) -> Result<Option<Timestamp>, StoreError>;
    /// The redacted body stored under `key`, if any.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection or the statement fails.
    /// - [`StoreError::InvalidRow`] when the stored platform or the stored kind of the key is
    ///   not a value that the domain knows.
    async fn body_by_key(&self, key: &str) -> Result<Option<StoredBody>, StoreError>;
    /// Inserts unless the key or the source hash is already stored.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection or the statement fails.
    /// - [`StoreError::InvalidRow`] when the identity of the new row is not positive.
    async fn insert(&self, message: &NewRawMessage) -> Result<InsertOutcome, StoreError>;
    /// Messages of the platform in state `fetched`, oldest first.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection or the statement fails.
    /// - [`StoreError::InvalidRow`] when a stored identity, kind of key, kind of message or
    ///   parse state is not a value that the domain accepts.
    async fn pending(&self, platform: Platform) -> Result<Vec<PendingMessage>, StoreError>;
}

/// Which parser produced a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParserTag {
    /// The fixed name of the parser, such as `fd_receipt_2026`. It is `refused` when no
    /// parser ran. Stored with each parse attempt and with the order.
    pub name: &'static str,
    /// The version of that parser. Stored beside the name.
    pub version: i32,
}

/// One parser run over one message, as a row of `parse_attempts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptRecord {
    /// The parser that ran. It is the tag `refused` when the receipt has no stored HTML body
    /// or no parser recognizes the layout.
    pub parser: ParserTag,
    /// How the run ended. The next parse state of the message follows from it.
    pub outcome: ParseOutcome,
    /// Why the run made no order: the error of the parser, the text `no HTML part` for a
    /// receipt with no stored HTML body, or that no parser recognizes the layout. `None` when
    /// the run made an order.
    pub error: Option<String>,
    /// How many anomalies the order carries: the rows that no parser rule classified and the
    /// mismatches of the reconciliation. Zero when the run made no order.
    pub anomaly_count: i32,
}

/// An order to write, with the alias each line resolved to, in line order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderWrite<'a> {
    /// The receipt as the parser read it, with one anomaly added for each mismatch of the
    /// reconciliation.
    pub receipt: &'a Receipt,
    /// `true` when the line prices sum to the subtotal, the subtotal and its additions sum to
    /// the total, and each priced line is its unit price times its billed quantity, each
    /// within one cent.
    pub reconciled: bool,
    /// `verified` when the mail of the receipt had a DKIM pass, `non-verified` when not.
    pub sender_check: SenderCheck,
    /// The alias of each line: one for each element of `receipt.lines`, in the same order.
    pub alias_ids: Vec<AliasId>,
}

/// Everything one parse attempt changes, written in one transaction (REAS-DATA-002): the
/// attempt row, the conditional state transition, and the order with its lines and anomalies
/// when the attempt produced one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseFinish<'a> {
    /// The message that the attempt ran over.
    pub message: RawMessageId,
    /// The parse state that the message had when it was read.
    pub expected_state: ParseState,
    /// The value of `raw_messages.version` when the message was read. The transition changes
    /// the row only if the state and the version are still these.
    pub expected_version: i32,
    /// The parse state after the attempt: `parsed`, `partial`, `failed` or `unsupported`.
    pub next_state: ParseState,
    /// The row of `parse_attempts` to add.
    pub attempt: AttemptRecord,
    /// The order that the attempt made. It replaces a stored order with the same order
    /// number. `None` when the attempt made no order; no order row changes then.
    pub order: Option<OrderWrite<'a>>,
}

#[expect(
    async_fn_in_trait,
    reason = "a native async fn in a port (REAS-ARCH-006); each use case runs on the task of the composition root, so no caller needs the future to be Send"
)]
/// The orders: what a parse attempt writes, and the history that the recommendation reads.
pub trait GroceryStore {
    /// Fails with [`StoreError::Conflict`] when the message is no longer in the expected state
    /// and version; nothing is written then.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Conflict`] when the state or the version of the message is not the
    ///   expected one.
    /// - [`StoreError::Database`] when the connection or a statement fails. Nothing is
    ///   written then: the attempt, the order and the transition are one transaction.
    async fn finish_parse(&self, finish: &ParseFinish<'_>) -> Result<(), StoreError>;
    /// Every purchase of the history, from the `purchases` view: one for each line of each
    /// stored order, read through its alias to a product. In no promised order.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection or the statement fails.
    /// - [`StoreError::InvalidRow`] when the product, the order number, the billed quantity
    ///   or the price of a row is not a value that the domain accepts.
    async fn purchases(&self) -> Result<Vec<Purchase>, StoreError>;
    /// The delivery date of each stored order, earliest first. Two orders delivered on one
    /// day give that day twice, so the length is the number of orders. Empty when no order
    /// is stored.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection or the statement fails.
    async fn delivery_dates(&self) -> Result<Vec<Date>, StoreError>;
    /// The products that the user muted (`product_preferences.muted`). The recommendation
    /// leaves them out of every list.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection or the statement fails.
    /// - [`StoreError::InvalidRow`] when a stored product identity is not positive.
    async fn muted_products(&self) -> Result<HashSet<ProductId>, StoreError>;
}

/// What a line offers the catalog to find or create its alias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasRequest<'a> {
    /// The normalized name and size of the line's printed name. The alias is found by the
    /// pair of the two; the display name and the size text go to a new product.
    pub normalized: &'a Normalized,
    /// The name as the receipt printed it, with its size. A new alias keeps it as the first
    /// name seen.
    pub raw_name: &'a str,
    /// Whether the line is priced by the piece or by weight. Only a new product takes it.
    pub sold_by: SoldBy,
    /// The department that the receipt lists the line under; `None` when it names none. Only
    /// a new product takes it.
    pub department: Option<&'a str>,
    /// The path of the product link in the row of the line, without host, query string and
    /// fragment; `None` when the row has no link with a path. Only a new alias takes it.
    pub product_url_path: Option<&'a str>,
}

/// How a product is shown: its name, size and department, and the name the most recent receipt
/// printed, which is what the FreshDirect search box wants.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProductLabel {
    /// The product that the label is for.
    pub id: ProductId,
    /// The stored name of the product, without the size. For a product that the import
    /// created it is the printed name of the line that created it.
    pub display_name: String,
    /// The size as that line printed it, without the parentheses; `None` when the printed
    /// name had no size.
    pub size_text: Option<String>,
    /// Whether the product is priced by the piece or by weight. The habit rule reads the
    /// amount of a pack from the size only for a product sold by the piece.
    pub sold_by: SoldBy,
    /// The department that the receipt listed that line under; `None` when it named none.
    pub department: Option<String>,
    /// The name, with its size, as the receipt of the latest delivery date printed it. The
    /// display name when no order line points at the product.
    pub latest_raw_name: String,
}

/// One addition to store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewAddition<'a> {
    /// What the user typed for the item: one line, trimmed.
    pub item: &'a ItemText,
    /// The product that the text names, when it names exactly one product of the catalog.
    /// `None` when it names none or several.
    pub product: Option<ProductId>,
    /// Why the user adds the item.
    pub reason: AddReason,
    /// The day the item was added; the caller supplies it. The addition is open until an
    /// order is delivered after this day.
    pub added_on: Date,
}

/// The user's own additions to the next order (ADR 0011).
#[expect(
    async_fn_in_trait,
    reason = "a native async fn in a port (REAS-ARCH-006); each use case runs on the task of the composition root, so no caller needs the future to be Send"
)]
pub trait ShoppingList {
    /// Stores one addition and returns its number, which `qm drop` takes.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection or the statement fails.
    /// - [`StoreError::InvalidRow`] when the identity of the new row is not positive.
    async fn add_addition(&self, addition: &NewAddition<'_>) -> Result<AdditionId, StoreError>;
    /// The additions no order has been delivered after: added on or after the latest delivery
    /// date, oldest first.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection or the statement fails.
    /// - [`StoreError::InvalidRow`] when a stored identity, item text or reason is not a
    ///   value that the domain accepts.
    async fn open_additions(&self) -> Result<Vec<Addition>, StoreError>;
    /// `false` when no addition has this id.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection or the statement fails.
    async fn drop_addition(&self, id: AdditionId) -> Result<bool, StoreError>;
}

/// The products and the aliases that point at them.
#[expect(
    async_fn_in_trait,
    reason = "a native async fn in a port (REAS-ARCH-006); each use case runs on the task of the composition root, so no caller needs the future to be Send"
)]
pub trait Catalog {
    /// The alias for a normalized name and size, created with a new product when unseen.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection or a statement fails.
    /// - [`StoreError::InvalidRow`] when the identity of the alias is not positive.
    async fn resolve_alias(&self, request: &AliasRequest<'_>) -> Result<AliasId, StoreError>;
    /// The label of every product of the catalog, also of one that no order line points at.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection or the statement fails.
    /// - [`StoreError::InvalidRow`] when a stored identity is not positive, or a stored
    ///   `sold_by` is not a value that the domain knows.
    async fn product_labels(&self) -> Result<Vec<ProductLabel>, StoreError>;
}

/// Runs the receipt parsers over one HTML body. The registry of format adapters implements it;
/// it is a trait so a use-case test can use a fixed double.
pub trait ReceiptParsing {
    /// Parses one redacted HTML body with the parser that recognizes its layout best. The
    /// call does not fail: an unknown layout and an error of the parser are variants of the
    /// result.
    fn parse(&self, html: &str) -> ParseAttempt;
}

/// What the parsers made of one body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseAttempt {
    /// No parser recognized the layout.
    Unsupported,
    /// A parser recognized the layout and could not read it.
    Failed {
        /// The parser that recognized the layout.
        parser: ParserTag,
        /// The text of the parser's error, such as a missing order number or delivery date.
        error: String,
    },
    /// A parser read the layout. The receipt is not reconciled yet.
    Parsed {
        /// The parser that read the layout.
        parser: ParserTag,
        /// What the receipt says: the order number, the delivery date, the lines, the totals
        /// and the rows that no parser rule classified.
        receipt: Box<Receipt>,
    },
}

impl ParseAttempt {
    /// The order number of the parsed receipt; `None` when no receipt was parsed.
    pub fn order_number(&self) -> Option<&OrderNumber> {
        match self {
            ParseAttempt::Parsed { receipt, .. } => Some(&receipt.order_number),
            ParseAttempt::Unsupported | ParseAttempt::Failed { .. } => None,
        }
    }
}
