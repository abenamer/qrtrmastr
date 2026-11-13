//! The parser registry: the Serializer-normalizer pattern. Every parser is asked whether it
//! recognizes the document; the most confident one parses it.

use scraper::Html;

use super::ReceiptParser;
use super::freshdirect_2026::FreshDirect2026;
use crate::application::ports::{ParseAttempt, ReceiptParsing};

pub struct Registry {
    parsers: Vec<Box<dyn ReceiptParser>>,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            parsers: vec![Box::new(FreshDirect2026)],
        }
    }
}

impl ReceiptParsing for Registry {
    fn parse(&self, html: &str) -> ParseAttempt {
        let document = Html::parse_document(html);
        let best = self
            .parsers
            .iter()
            .filter_map(|parser| parser.supports(&document).map(|c| (c, parser)))
            .max_by_key(|(confidence, _)| *confidence);
        match best {
            None => ParseAttempt::Unsupported,
            Some((_, parser)) => match parser.parse(&document) {
                Ok(receipt) => ParseAttempt::Parsed {
                    parser: parser.tag(),
                    receipt: Box::new(receipt),
                },
                Err(error) => ParseAttempt::Failed {
                    parser: parser.tag(),
                    error: error.to_string(),
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unrelated_page_is_unsupported() {
        let attempt = Registry::default().parse("<html><body><p>Hello</p></body></html>");
        assert_eq!(attempt, ParseAttempt::Unsupported);
    }
}
