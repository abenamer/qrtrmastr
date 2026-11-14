//! Receipt parsers: pure format adapters that turn one platform's HTML into a domain
//! [`Receipt`](crate::domain::receipt::Receipt). Each parser has a name and a version; the
//! registry picks the one that recognizes the layout best.

pub mod freshdirect_2026;
pub mod registry;

use scraper::Html;

use crate::application::ports::ParserTag;
use crate::domain::receipt::Receipt;

/// How sure a parser is that a document is in its layout, 0 to 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Confidence(pub u8);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("no order number found")]
    NoOrderNumber,
    #[error("order number {0:?} is not valid: {1}")]
    BadOrderNumber(String, String),
    #[error("no delivery date found")]
    NoDeliveryDate,
    #[error("delivery date {0:?} is not in a known format")]
    BadDeliveryDate(String),
    #[error("no item table found")]
    NoItemTable,
    #[error("no item rows found")]
    NoItems,
    #[error("no subtotal found")]
    NoSubtotal,
    #[error("no order total found")]
    NoTotal,
    #[error("row {row}: {reason}")]
    Row { row: usize, reason: String },
}

/// One layout. `supports` is cheap and never fails; `parse` is the full read.
pub trait ReceiptParser {
    fn tag(&self) -> ParserTag;
    fn supports(&self, document: &Html) -> Option<Confidence>;
    fn parse(&self, document: &Html) -> Result<Receipt, ParseError>;
}
