//! Receipt parsers: pure format adapters that turn one platform's HTML into a domain
//! [`Receipt`]. Each parser has a name and a version; the
//! registry picks the one that recognizes the layout best.

pub mod freshdirect_2026;
pub mod registry;

use scraper::Html;

use crate::application::ports::ParserTag;
use crate::domain::receipt::Receipt;

/// How sure a parser is that a document is in its layout, 0 to 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Confidence(pub u8);

/// Why a parser that recognized a layout could not read the receipt. The registry returns the
/// text of the error, the parse use case stores it with the parse attempt, and the message
/// gets the parse state `failed`.
/// A row that no rule classifies is not an error: it becomes an anomaly of the order.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// The text of the document holds no "order" label with 11 to 13 digits after it.
    #[error("no order number found")]
    NoOrderNumber,
    /// The digits after the "order" label are not an order number of 11 or 13 digits. The
    /// first value is the digits as found, the second is the reason the domain type gave.
    #[error("order number {0:?} is not valid: {1}")]
    BadOrderNumber(String, String),
    /// The document holds no "delivery date" label with a date after it, and no sentence that
    /// states the delivery window and the day.
    #[error("no delivery date found")]
    NoDeliveryDate,
    /// The text that stands where the delivery date is expected is in none of the date formats
    /// that the parser reads. The value is that text.
    #[error("delivery date {0:?} is not in a known format")]
    BadDeliveryDate(String),
    /// No table of the document has a row whose first cell is a quantity pair.
    #[error("no item table found")]
    NoItemTable,
    /// The item table has no row that the parser read as an item.
    #[error("no item rows found")]
    NoItems,
    /// No totals row has the label of the subtotal. Without it the order cannot be reconciled.
    #[error("no subtotal found")]
    NoSubtotal,
    /// No totals row has the label of the order total.
    #[error("no order total found")]
    NoTotal,
    /// One row of the item table cannot be read. No parser makes this value today: the 2026
    /// parser keeps such a row as an anomaly and stores the order.
    #[error("row {row}: {reason}")]
    Row {
        /// The position of the row in the item table.
        row: usize,
        /// What is wrong with the row.
        reason: String,
    },
    /// The receipt has more item lines than the line number, a `u32`, can count.
    #[error("more item lines than a line number can count")]
    TooManyLines,
    /// Two rows of the totals with one label have amounts whose sum is outside the range of
    /// an amount of money. No amount that a receipt prints is near that range.
    #[error("the amounts of two totals rows cannot be added")]
    TotalsOutOfRange,
}

/// One layout. `supports` is cheap and never fails; `parse` is the full read.
pub trait ReceiptParser {
    /// The name and the version of this parser, stored with each parse attempt, so that a
    /// stored order says which parser version read it.
    fn tag(&self) -> ParserTag;
    /// Whether the document looks like this parser's layout, and how sure the parser is.
    /// `None` when it does not; the registry lets the most confident parser read the document.
    fn supports(&self, document: &Html) -> Option<Confidence>;
    /// Reads the receipt: the order number, the delivery date, the lines, the totals and the
    /// anomalies. The result is not reconciled; the use case does that.
    ///
    /// # Errors
    ///
    /// A [`ParseError`] when a part that every receipt has is missing or cannot be read:
    ///
    /// - [`ParseError::NoOrderNumber`] or [`ParseError::BadOrderNumber`] for the order number;
    /// - [`ParseError::NoDeliveryDate`] or [`ParseError::BadDeliveryDate`] for the delivery
    ///   date;
    /// - [`ParseError::NoItemTable`] or [`ParseError::NoItems`] for the item table and its
    ///   item rows;
    /// - [`ParseError::NoSubtotal`] or [`ParseError::NoTotal`] for the two totals;
    /// - [`ParseError::TooManyLines`] for more item lines than a line number can count;
    /// - [`ParseError::TotalsOutOfRange`] for two totals rows of one label whose amounts
    ///   cannot be added.
    ///
    /// No parser returns [`ParseError::Row`] today.
    fn parse(&self, document: &Html) -> Result<Receipt, ParseError>;
}
