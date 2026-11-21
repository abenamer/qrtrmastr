//! What the user adds to the next order by hand (ADR 0011). The lists of `qm due` come from
//! the history; an addition comes from the user, with the reason it was added.

use std::fmt;

use jiff::civil::Date;
use serde::Serialize;
use thiserror::Error;

use super::ids::{AdditionId, ProductId};
use super::states::AddReason;

/// The longest item text; the check constraint `list_additions_item_text_check` states the same.
pub const ITEM_TEXT_MAX: usize = 120;

/// Why a text that the user typed is not the item of an addition.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ItemTextError {
    /// The text holds no word: it is empty or only whitespace.
    #[error("an item needs a name")]
    Empty,
    /// The text is longer than [`ITEM_TEXT_MAX`] characters after its words are joined with one
    /// space. Holds its length in characters, not in bytes.
    #[error("an item name is at most {ITEM_TEXT_MAX} characters, not {0}")]
    TooLong(usize),
    /// The text holds a line break, or a control character that is not whitespace. A terminal
    /// obeys such a character, so the text is refused and not repaired.
    #[error("an item name is one line of text without control characters")]
    ControlCharacter,
}

/// What the user typed for an item: one line, trimmed, one space between words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ItemText(String);

impl ItemText {
    /// Reads the item of `qm add`, and a stored `list_additions.item_text` read back. The words
    /// are joined with one space, so a tab between two words becomes a space.
    ///
    /// # Errors
    ///
    /// - [`ItemTextError::ControlCharacter`] when the text holds a line feed, a carriage return,
    ///   or a control character that is not whitespace.
    /// - [`ItemTextError::Empty`] when the text holds no word.
    /// - [`ItemTextError::TooLong`] when the joined text is longer than [`ITEM_TEXT_MAX`]
    ///   characters.
    pub fn parse(raw: &str) -> Result<Self, ItemTextError> {
        if raw.chars().any(|c| c.is_control() && !c.is_whitespace()) || raw.contains(['\n', '\r']) {
            return Err(ItemTextError::ControlCharacter);
        }
        let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        match text.chars().count() {
            0 => Err(ItemTextError::Empty),
            n if n > ITEM_TEXT_MAX => Err(ItemTextError::TooLong(n)),
            _ => Ok(Self(text)),
        }
    }

    /// The item as one line: what `list_additions.item_text` stores and the lists show.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ItemText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One addition as it is stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Addition {
    /// The number of the addition: what `qm add` prints and `qm drop` takes.
    pub id: AdditionId,
    /// What the user typed for the item.
    pub item: ItemText,
    /// The product of the history that the item text named on the day it was added, when it
    /// named exactly one ([`the_one_product`]). `None` when no product or several matched.
    #[serde(rename = "product_id")]
    pub product: Option<ProductId>,
    /// Why the user added the item.
    pub reason: AddReason,
    /// The day the user added the item, as the caller supplied it. The addition is listed
    /// until an order is delivered after this day.
    pub added_on: Date,
}

/// The product an item text names, when it names exactly one: a case-insensitive match of the
/// whole text inside one product name. Several sizes of one name are several products, so the
/// text is then left without a product; nothing here guesses.
pub fn the_one_product<'a>(
    item: &ItemText,
    products: impl IntoIterator<Item = (ProductId, &'a str)>,
) -> Option<ProductId> {
    let wanted = item.as_str().to_lowercase();
    let mut matches = products
        .into_iter()
        .filter(|(_, name)| name.to_lowercase().contains(&wanted))
        .map(|(product, _)| product);
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_text_is_one_trimmed_line_of_limited_length() {
        assert_eq!(
            ItemText::parse("  miso   paste ").map(|t| t.to_string()),
            Ok("miso paste".to_owned())
        );
        assert_eq!(ItemText::parse("   "), Err(ItemTextError::Empty));
        assert_eq!(
            ItemText::parse("a\nb"),
            Err(ItemTextError::ControlCharacter)
        );
        assert_eq!(
            ItemText::parse("a\u{1b}[31m"),
            Err(ItemTextError::ControlCharacter)
        );
        assert!(ItemText::parse(&"x".repeat(ITEM_TEXT_MAX)).is_ok());
        assert_eq!(
            ItemText::parse(&"x".repeat(ITEM_TEXT_MAX + 1)),
            Err(ItemTextError::TooLong(ITEM_TEXT_MAX + 1))
        );
    }

    #[test]
    fn an_item_names_a_product_only_when_exactly_one_matches() {
        let id = |n| ProductId::new(n).expect("id");
        let catalog = [
            (id(1), "Lactaid Whole Milk"),
            (id(2), "Organic Valley Half & Half"),
            (id(3), "Organic Valley Whole Milk"),
        ];
        let find = |text: &str| {
            the_one_product(
                &ItemText::parse(text).expect("text"),
                catalog.iter().copied(),
            )
        };
        assert_eq!(find("half & half"), Some(id(2)));
        assert_eq!(find("LACTAID"), Some(id(1)));
        assert_eq!(find("whole milk"), None);
        assert_eq!(find("miso paste"), None);
    }
}
