//! The FreshDirect receipt layout seen on 2026 mail: an item table whose first cell is the
//! quantity pair `ordered/second`, a product name that may be a link, a weight, a unit price in
//! parentheses, an options price, the final price, and three one-letter flag columns. Department
//! names are rows of their own. Totals sit in a separate two-column table.
//!
//! Every rule reads cell content, never a class name, so a cosmetic change to the template does
//! not break it. A row that fits no rule becomes an anomaly, never a dropped line.

use std::sync::LazyLock;

use jiff::civil::Date;
use regex::Regex;

use crate::domain::pattern::fixed;
use scraper::{ElementRef, Html, Selector};

use super::{Confidence, ParseError, ReceiptParser};
use crate::application::ports::ParserTag;
use crate::domain::identity::{ALIAS_KEY_MAX_BYTES, normalize};
use crate::domain::ids::OrderNumber;
use crate::domain::money::{Money, Quantity};
use crate::domain::receipt::{
    Anomaly, Flags, LineItem, PricePer, Receipt, Totals, UnitPrice, Weight,
};

/// The parser of the FreshDirect receipt layout of this module. It holds no state: its rules
/// are the patterns below.
pub struct FreshDirect2026;

/// The name of this parser, as `parse_attempts.parser_name` and `grocery_orders.parser_name`
/// store it.
pub const NAME: &str = "fd_receipt_2026";
/// The version of this parser's rules, stored beside the name, so that an attempt and an
/// order say which rules read the receipt. Version 2 reads the real layout (ADR 0008).
pub const VERSION: i32 = 2;

static ORDER_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| fixed(r"(?i)order\s*(?:number|#|no\.?)?\s*:?\s*(\d{11,13})\b"));
static DELIVERY_DATE: LazyLock<Regex> = LazyLock::new(|| {
    fixed(
        r"(?i)delivery\s+date\s*:?\s*([A-Za-z]+,?\s+[A-Za-z]+\s+\d{1,2},?\s+\d{4}|[A-Za-z]+\s+\d{1,2},?\s+\d{4}|\d{1,2}/\d{1,2}/\d{4}|\d{4}-\d{2}-\d{2})",
    )
});
/// The real receipt states the delivery in a sentence, not after a label: "It will be delivered
/// between 10 AM and 12 PM on Monday, November 09, 2026." Groups: start, end, date.
static DELIVERY_SENTENCE: LazyLock<Regex> = LazyLock::new(|| {
    fixed(
        r"(?i)delivered\s+between\s+(\d{1,2}(?::\d{2})?\s*[ap]m)\s+and\s+(\d{1,2}(?::\d{2})?\s*[ap]m)\s+on\s+([A-Za-z]+,?\s+[A-Za-z]+\s+\d{1,2},?\s+\d{4})",
    )
});
static DELIVERY_WINDOW: LazyLock<Regex> = LazyLock::new(|| {
    fixed(
        r"(?i)delivery\s+(?:window|time)\s*:?\s*(\d{1,2}(?::\d{2})?\s*(?:am|pm)?\s*[-\u{2013}]\s*\d{1,2}(?::\d{2})?\s*(?:am|pm)?)",
    )
});
static QTY_CELL: LazyLock<Regex> =
    LazyLock::new(|| fixed(r"^\s*(\d+(?:\.\d+)?)\s*/\s*(\d+(?:\.\d+)?)\s*$"));
static WEIGHT_CELL: LazyLock<Regex> =
    LazyLock::new(|| fixed(r"(?i)^\s*(\d+(?:\.\d+)?)\s*(lb|lbs|oz|kg|g)\.?\s*$"));
static UNIT_PRICE_CELL: LazyLock<Regex> = LazyLock::new(|| {
    fixed(r"(?i)^\s*\(?\s*(\$?\s*[\d,]+\.\d{2})\s*/\s*(ea|each|lb|pound)\.?\s*\)?\s*$")
});
static MONEY_CELL: LazyLock<Regex> =
    LazyLock::new(|| fixed(r"^\s*\(?-?\$?\s*[\d,]+\.\d{2}\)?\s*$"));
static FOOTER_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    fixed(
        r"(?i)^\s*(subtotal|sub-total|total tax|tax|sales tax|state bottle deposit|bottle deposit|deposit|delivery fee|delivery|service fee|fuel surcharge|tip|driver tip|order total|total)(?:\s*\([^)]*\))?\s*:?\s*$",
    )
});
static FURNITURE: LazyLock<Regex> =
    LazyLock::new(|| fixed(r"(?i)^\s*(order\s+#?\d{9,14}|order total\b.*|[TSD]\s*=\s*.+)\s*$"));
static WS: LazyLock<Regex> = LazyLock::new(|| fixed(r"\s+"));

/// A CSS selector that is a constant of the source.
#[expect(
    clippy::expect_used,
    reason = "the selector is a constant of the source; an invalid one is a bug, found by the first test that parses a receipt"
)]
fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("a selector written in the source is valid")
}

fn text_of(element: ElementRef<'_>) -> String {
    let joined: String = element.text().collect::<Vec<_>>().join(" ");
    WS.replace_all(joined.trim(), " ").into_owned()
}

fn parse_date(text: &str) -> Option<Date> {
    let cleaned = WS.replace_all(text.trim(), " ");
    let cleaned = cleaned.trim_end_matches(',');
    let formats = [
        "%A, %B %d, %Y",
        "%A %B %d, %Y",
        "%A, %B %d %Y",
        "%B %d, %Y",
        "%B %d %Y",
        "%m/%d/%Y",
        "%Y-%m-%d",
    ];
    // A weekday or a month is printed in full or in three letters, in any pairing; `%A` and
    // `%B` read the full name only, so each format is tried with each pairing.
    let names = [("%A", "%B"), ("%A", "%b"), ("%a", "%B"), ("%a", "%b")];
    formats.iter().find_map(|format| {
        names.iter().find_map(|(weekday, month)| {
            let format = format.replace("%A", weekday).replace("%B", month);
            Date::strptime(&format, cleaned).ok()
        })
    })
}

/// The cells of a row as text, with the first product link path found in the row.
struct Row {
    cells: Vec<String>,
    link_path: Option<String>,
    header: bool,
}

fn read_row(tr: ElementRef<'_>) -> Row {
    let mut cells = Vec::new();
    let mut header = false;
    let mut link_path = None;
    for child in tr.children().filter_map(ElementRef::wrap) {
        match child.value().name() {
            "td" | "th" => {
                if child.value().name() == "th" {
                    header = true;
                }
                if link_path.is_none()
                    && let Some(a) = child.select(&sel("a[href]")).next()
                {
                    link_path = a.value().attr("href").and_then(|href| {
                        let without_query = href.split(['?', '#']).next().unwrap_or(href);
                        let path = without_query
                            .splitn(4, '/')
                            .nth(3)
                            .map(|p| format!("/{p}"))
                            .or_else(|| {
                                without_query
                                    .starts_with('/')
                                    .then(|| without_query.to_owned())
                            });
                        path.filter(|p| p.len() > 1)
                    });
                }
                cells.push(text_of(child));
            }
            _ => {}
        }
    }
    Row {
        cells,
        link_path,
        header,
    }
}

/// Rows of a table that belong to it directly (not to a nested table).
fn own_rows(table: ElementRef<'_>) -> Vec<ElementRef<'_>> {
    let mut rows = Vec::new();
    for child in table.children().filter_map(ElementRef::wrap) {
        match child.value().name() {
            "tr" => rows.push(child),
            "tbody" | "thead" | "tfoot" => {
                rows.extend(
                    child
                        .children()
                        .filter_map(ElementRef::wrap)
                        .filter(|e| e.value().name() == "tr"),
                );
            }
            _ => {}
        }
    }
    rows
}

fn money(cell: &str) -> Option<Money> {
    Money::parse(cell.trim()).ok()
}

/// The totals rows of a receipt as they are read. An order from two stores (groceries and wine)
/// prints a subtotal, a tax and a deposit for each store, then one `Total Tax`, the fees and one
/// order total; a deposit row after `Total Tax` restates the sum.
#[derive(Default)]
struct FooterSums {
    subtotal: Option<Money>,
    tax: Option<Money>,
    total_tax: Option<Money>,
    bottle_deposit: Option<Money>,
    delivery_fee: Option<Money>,
    service_fee: Option<Money>,
    tip: Option<Money>,
    total: Option<Money>,
}

/// Adds a row to the rows of the same label before it: a receipt of two stores prints the
/// subtotal and the tax of each.
fn add(sum: &mut Option<Money>, amount: Money) -> Result<(), ParseError> {
    *sum = Some(match *sum {
        Some(so_far) => so_far
            .checked_add(amount)
            .map_err(|_| ParseError::TotalsOutOfRange)?,
        None => amount,
    });
    Ok(())
}

impl FooterSums {
    fn apply(&mut self, label: &str, amount: Money) -> Result<(), ParseError> {
        match label {
            "subtotal" | "sub-total" => add(&mut self.subtotal, amount)?,
            "total tax" => self.total_tax = Some(amount),
            "tax" | "sales tax" => add(&mut self.tax, amount)?,
            "state bottle deposit" | "bottle deposit" | "deposit" => {
                if self.total_tax.is_some() {
                    self.bottle_deposit = Some(amount);
                } else {
                    add(&mut self.bottle_deposit, amount)?;
                }
            }
            "delivery fee" | "delivery" => self.delivery_fee = Some(amount),
            "service fee" | "fuel surcharge" => add(&mut self.service_fee, amount)?,
            "tip" | "driver tip" => self.tip = Some(amount),
            "order total" | "total" => self.total = Some(amount),
            _ => {}
        }
        Ok(())
    }
}

enum Classified {
    Department(String),
    Item(Box<LineItem>),
    Footer(String, Money),
    Header,
    Blank,
    Unknown(String),
}

fn classify(row: &Row, line_no: u32, department: Option<&str>) -> Classified {
    if row.header {
        return Classified::Header;
    }
    let non_empty: Vec<&str> = row
        .cells
        .iter()
        .map(String::as_str)
        .filter(|c| !c.is_empty())
        .collect();
    // The cells of a row come from the mail. Each rule below names the cells that it reads
    // in a pattern, so a row with fewer cells does not match the rule (REAS-SEC-003).
    let non_empty = non_empty.as_slice();
    if non_empty.is_empty() {
        return Classified::Blank;
    }
    // Rows of the real layout that carry no item and no total: the column titles in plain
    // cells, the order number, the summary box with the payment, and the legend of the flags.
    let titles =
        row.cells.iter().any(|c| c == "Final Price") && row.cells.iter().any(|c| c == "Unit Price");
    if titles || matches!(non_empty, [only] if FURNITURE.is_match(only)) {
        return Classified::Header;
    }
    if let Some(caps) = row.cells.first().and_then(|cell| QTY_CELL.captures(cell)) {
        let [
            _,
            name_cell,
            weight_cell,
            unit_price_cell,
            options_cell,
            final_price_cell,
            flag_cells @ ..,
        ] = row.cells.as_slice()
        else {
            return Classified::Unknown(format!("quantity row with {} cells", row.cells.len()));
        };
        let (Ok(qty_first), Ok(qty_second)) =
            (Quantity::parse(&caps[1]), Quantity::parse(&caps[2]))
        else {
            return Classified::Unknown(
                "quantity cell does not hold two positive numbers".to_owned(),
            );
        };
        let raw_name = name_cell.clone();
        if raw_name.is_empty() {
            return Classified::Unknown("quantity row without a product name".to_owned());
        }
        // A name is the key of an alias after it is normalized, and a key has a limit. A row
        // with a longer name is a row that this parser cannot read, like any other; the
        // text of the name does not go into the reason.
        if !normalize(&raw_name).fits_an_alias() {
            return Classified::Unknown(format!(
                "product name longer than {ALIAS_KEY_MAX_BYTES} bytes"
            ));
        }
        let weight = WEIGHT_CELL.captures(weight_cell).and_then(|w| {
            Quantity::parse(&w[1]).ok().map(|value| Weight {
                value,
                unit: w[2].to_lowercase(),
            })
        });
        let unit_price = UNIT_PRICE_CELL.captures(unit_price_cell).and_then(|u| {
            let amount = Money::parse(&u[1].replace(' ', "")).ok()?;
            let per = match u[2].to_lowercase().as_str() {
                "ea" | "each" => PricePer::Each,
                _ => PricePer::Pound,
            };
            Some(UnitPrice { amount, per })
        });
        let options_price = (!options_cell.is_empty())
            .then(|| money(options_cell))
            .flatten();
        let Some(final_price) = money(final_price_cell) else {
            return Classified::Unknown(format!(
                "price cell {final_price_cell:?} is not an amount"
            ));
        };
        let mut flags = Flags::default();
        for cell in flag_cells {
            match cell.trim() {
                "T" => flags.taxable = true,
                "S" => flags.special = true,
                "D" => flags.deposit = true,
                _ => {}
            }
        }
        return Classified::Item(Box::new(LineItem {
            line_no,
            department: department.map(str::to_owned),
            raw_name,
            product_url_path: row.link_path.clone(),
            qty_first,
            qty_second,
            weight,
            unit_price,
            options_price,
            final_price,
            flags,
        }));
    }
    // A totals row is a label and an amount; a taxable fee carries a flag letter after them.
    let label_and_amount = match non_empty {
        [label, amount] => Some((label, amount)),
        [label, amount, flag] if matches!(flag.trim(), "T" | "S" | "D") => Some((label, amount)),
        _ => None,
    };
    if let Some((label, amount)) = label_and_amount
        && let (Some(label), Some(amount)) = (FOOTER_LABEL.captures(label), money(amount))
    {
        return Classified::Footer(label[1].to_lowercase(), amount);
    }
    if let [only] = non_empty
        && !only.contains('$')
        && only.len() <= 40
        && !MONEY_CELL.is_match(only)
    {
        return Classified::Department((*only).to_owned());
    }
    Classified::Unknown("no rule matched".to_owned())
}

impl ReceiptParser for FreshDirect2026 {
    fn tag(&self) -> ParserTag {
        ParserTag {
            name: NAME,
            version: VERSION,
        }
    }

    fn supports(&self, document: &Html) -> Option<Confidence> {
        let text = text_of(document.root_element());
        let has_order = ORDER_NUMBER.is_match(&text);
        let has_qty = document
            .select(&sel("td"))
            .any(|td| QTY_CELL.is_match(&text_of(td)));
        let mentions = text.to_lowercase().contains("freshdirect");
        match (has_order, has_qty, mentions) {
            (true, true, true) => Some(Confidence(90)),
            (true, true, false) => Some(Confidence(60)),
            (true, false, true) => Some(Confidence(30)),
            _ => None,
        }
    }

    fn parse(&self, document: &Html) -> Result<Receipt, ParseError> {
        let text = text_of(document.root_element());
        let order_number = ORDER_NUMBER
            .captures(&text)
            .map(|c| c[1].to_owned())
            .ok_or(ParseError::NoOrderNumber)?;
        let order_number = OrderNumber::parse(&order_number)
            .map_err(|e| ParseError::BadOrderNumber(order_number.clone(), e.to_string()))?;
        let sentence = DELIVERY_SENTENCE.captures(&text);
        let date_text = DELIVERY_DATE
            .captures(&text)
            .map(|c| c[1].to_owned())
            .or_else(|| sentence.as_ref().map(|c| c[3].to_owned()))
            .ok_or(ParseError::NoDeliveryDate)?;
        let delivery_date = parse_date(&date_text).ok_or(ParseError::BadDeliveryDate(date_text))?;
        let delivery_window = DELIVERY_WINDOW
            .captures(&text)
            .map(|c| c[1].trim().to_owned())
            .or_else(|| {
                sentence
                    .as_ref()
                    .map(|c| format!("{} - {}", c[1].trim(), c[2].trim()))
            });

        // The item table is the innermost table with a quantity cell in a row of its own.
        let item_table = document.select(&sel("table")).find(|table| {
            own_rows(*table).iter().any(|tr| {
                let row = read_row(*tr);
                row.cells.first().is_some_and(|c| QTY_CELL.is_match(c))
            })
        });
        let item_table = item_table.ok_or(ParseError::NoItemTable)?;

        let mut lines = Vec::new();
        let mut anomalies = Vec::new();
        let mut footers = FooterSums::default();
        let mut department: Option<String> = None;
        // The number that the next item line gets. It is computed in one place, below.
        let mut next_line_no: u32 = 1;

        for (index, tr) in own_rows(item_table).into_iter().enumerate() {
            let row = read_row(tr);
            match classify(&row, next_line_no, department.as_deref()) {
                Classified::Header | Classified::Blank => {}
                Classified::Department(name) => department = Some(name),
                Classified::Item(item) => {
                    next_line_no = next_line_no
                        .checked_add(1)
                        .ok_or(ParseError::TooManyLines)?;
                    lines.push(*item);
                }
                Classified::Footer(label, amount) => footers.apply(&label, amount)?,
                Classified::Unknown(reason) => anomalies.push(Anomaly {
                    row_index: index,
                    cells: row.cells.clone(),
                    reason,
                }),
            }
        }

        // Totals usually sit in a table of their own: read every two-cell row of every table.
        for table in document.select(&sel("table")) {
            if table.id() == item_table.id() {
                continue;
            }
            for tr in own_rows(table) {
                let row = read_row(tr);
                if let Classified::Footer(label, amount) = classify(&row, 0, None) {
                    footers.apply(&label, amount)?;
                }
            }
        }

        if lines.is_empty() {
            return Err(ParseError::NoItems);
        }
        let totals = Totals {
            subtotal: footers.subtotal.ok_or(ParseError::NoSubtotal)?,
            tax: footers.total_tax.or(footers.tax),
            bottle_deposit: footers.bottle_deposit,
            delivery_fee: footers.delivery_fee,
            service_fee: footers.service_fee,
            tip: footers.tip,
            total: footers.total.ok_or(ParseError::NoTotal)?,
        };

        Ok(Receipt {
            order_number,
            delivery_date,
            delivery_window,
            lines,
            anomalies,
            totals,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::receipt::reconcile;

    const FIXTURE: &str =
        include_str!("../../../tests/fixtures/synthetic/input/freshdirect/2026/receipt-basic.html");

    #[test]
    fn a_delivery_date_is_read_in_each_format_a_receipt_prints() {
        let march_3 = Some(jiff::civil::date(2026, 3, 3));
        for text in [
            "Tuesday, March 3, 2026",
            "Tuesday, March 03, 2026",
            "Tuesday March 3, 2026",
            "Tuesday, March 3 2026",
            "March 3, 2026",
            "March 3 2026",
            "Tue, Mar 3, 2026",
            "Mar 3, 2026",
            // A short weekday with a full month, and the reverse.
            "Tue, March 3, 2026",
            "Tuesday, Mar 3, 2026",
            "Tue March 3, 2026",
            "Tue, Mar 3 2026",
            "Mar 3 2026",
            "03/03/2026",
            "3/3/2026",
            "2026-03-03",
            "  Tuesday,   March 3, 2026, ",
        ] {
            assert_eq!(parse_date(text), march_3, "{text}");
        }
        for text in [
            "",
            "soon",
            "March 32, 2026",
            "February 30, 2026",
            "2026-13-01",
            // The weekday must be the weekday of the date.
            "Monday, March 3, 2026",
        ] {
            assert_eq!(parse_date(text), None, "{text}");
        }
    }

    fn parsed() -> Receipt {
        let document = Html::parse_document(FIXTURE);
        assert_eq!(FreshDirect2026.supports(&document), Some(Confidence(90)));
        FreshDirect2026
            .parse(&document)
            .expect("the synthetic receipt parses")
    }

    #[test]
    fn the_synthetic_receipt_parses_into_six_lines_that_reconcile() {
        let receipt = parsed();
        assert_eq!(receipt.order_number.as_str(), "12345678901");
        assert_eq!(receipt.delivery_date, jiff::civil::date(2026, 3, 3));
        assert_eq!(receipt.delivery_window.as_deref(), Some("8:00am - 10:00am"));
        assert_eq!(receipt.lines.len(), 6);
        assert_eq!(receipt.totals.subtotal, Money::from_cents(4777));
        assert_eq!(receipt.totals.total, Money::from_cents(6250));
        assert_eq!(receipt.totals.tip, Some(Money::from_cents(600)));
        let r = reconcile(&receipt).expect("the amounts of the receipt add up");
        assert!(r.reconciled(), "{r:?}");
    }

    #[test]
    fn the_quantity_cell_weight_flags_and_link_path_are_read_verbatim() {
        let receipt = parsed();
        let tomatoes = &receipt.lines[2];
        assert_eq!(tomatoes.raw_name, "Beefsteak Tomatoes, Large");
        assert_eq!(tomatoes.department.as_deref(), Some("Vegetables"));
        assert_eq!(tomatoes.qty_first, Quantity::parse("2").expect("q"));
        assert_eq!(tomatoes.qty_second, Quantity::parse("1.56").expect("q"));
        assert_eq!(
            tomatoes.weight.as_ref().map(|w| w.unit.as_str()),
            Some("lb")
        );
        assert_eq!(tomatoes.billed_qty(), Quantity::parse("1.06").expect("q"));
        assert_eq!(tomatoes.unit_price.map(|u| u.per), Some(PricePer::Pound));
        assert_eq!(
            tomatoes.product_url_path.as_deref(),
            Some("/product/veg_tomato_beef_lg")
        );

        let peppers = &receipt.lines[3];
        assert!(peppers.flags.special && !peppers.flags.taxable);
        let seltzer = &receipt.lines[4];
        assert!(seltzer.flags.taxable && seltzer.flags.deposit);
        let turkey = &receipt.lines[5];
        assert_eq!(turkey.options_price, Some(Money::from_cents(100)));
        assert_eq!(turkey.final_price, Money::from_cents(775));
    }

    #[test]
    fn the_thank_you_row_is_an_anomaly_not_a_dropped_line() {
        let receipt = parsed();
        assert_eq!(receipt.anomalies.len(), 1, "{:?}", receipt.anomalies);
        assert!(receipt.anomalies[0].cells[0].starts_with("Thank you"));
    }

    #[test]
    fn a_receipt_without_an_order_number_fails_hard() {
        let html = FIXTURE.replace("12345678901", "");
        let document = Html::parse_document(&html);
        assert_eq!(
            FreshDirect2026.parse(&document),
            Err(ParseError::NoOrderNumber)
        );
    }

    /// The committed snapshot is derived from redacted HTML, so it is provably what the store
    /// holds; redaction must not change what the parser reads.
    ///
    /// The snapshot is a derived file (REAS-DOC-002): do not edit it. When the parser is meant
    /// to give another value, `INSTA_UPDATE=always cargo test --lib the_parsed_receipt_matches`
    /// writes the file again. With that setting the test passes whatever the parser gives, so
    /// read the difference of the file before you commit it.
    #[test]
    fn the_parsed_receipt_matches_its_snapshot_and_redaction_does_not_change_it() {
        use crate::adapters::mail::redact::{RedactionTerms, Redactor};
        let redactor = Redactor::new(RedactionTerms {
            names: vec!["Pat Example".to_owned(), "Pat".to_owned()],
            emails: vec!["pat.example@example.com".to_owned()],
            phones: vec!["2125550199".to_owned()],
            address_lines: vec![
                "123 Imaginary Street Apt 4B".to_owned(),
                "Brooklyn, NY 11201".to_owned(),
            ],
            card_last4: vec!["4242".to_owned()],
        })
        .expect("terms");
        let redacted = redactor.redact_html(FIXTURE).expect("redacts");
        let document = Html::parse_document(&redacted);
        let from_redacted = FreshDirect2026
            .parse(&document)
            .expect("the redacted receipt parses");
        assert_eq!(from_redacted, parsed());
        insta::assert_debug_snapshot!(from_redacted);
    }

    #[test]
    fn a_receipt_in_the_real_layout_with_two_stores_parses_and_reconciles() {
        let document = Html::parse_document(include_str!(
            "../../../tests/fixtures/synthetic/input/freshdirect/2024/receipt-two-stores.html"
        ));
        assert_eq!(FreshDirect2026.supports(&document), Some(Confidence(90)));
        let receipt = FreshDirect2026.parse(&document).expect("parses");
        assert_eq!(receipt.order_number.as_str(), "12345678902");
        assert_eq!(receipt.delivery_date, jiff::civil::date(2024, 12, 23));
        assert_eq!(
            receipt.delivery_window.as_deref(),
            Some("10:30 AM - 11:30 AM")
        );
        let names: Vec<&str> = receipt.lines.iter().map(|l| l.raw_name.as_str()).collect();
        assert_eq!(names.len(), 4, "{names:?}");
        assert_eq!(receipt.lines[1].department.as_deref(), Some("Vegetables"));
        assert_eq!(receipt.lines[3].department.as_deref(), Some("FD Wines"));
        // Two subtotals are one sum; `Total Tax` stands for the two tax rows; the fuel
        // surcharge, with its flag letter, is a fee.
        assert_eq!(receipt.totals.subtotal, Money::from_cents(3418));
        assert_eq!(receipt.totals.tax, Some(Money::from_cents(216)));
        assert_eq!(receipt.totals.service_fee, Some(Money::from_cents(100)));
        assert_eq!(receipt.totals.tip, Some(Money::from_cents(1000)));
        assert_eq!(receipt.totals.total, Money::from_cents(4734));
        assert!(receipt.anomalies.is_empty(), "{:?}", receipt.anomalies);
        let r = reconcile(&receipt).expect("the amounts of the receipt add up");
        assert!(r.reconciled(), "{r:?}");
    }

    /// A backtest over stored, redacted receipts, outside the repository. `QM_BACKTEST_FILE` is
    /// a file of JSON lines, each `{"year": 2026, "html": "..."}`, as a query over
    /// `raw_messages` writes them. Run: `QM_BACKTEST_FILE=... cargo test --lib backtest -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs QM_BACKTEST_FILE, a local export of redacted receipts"]
    fn backtest_over_stored_receipts() {
        use std::collections::BTreeMap;
        let path = std::env::var("QM_BACKTEST_FILE").expect("QM_BACKTEST_FILE is set");
        let content = std::fs::read_to_string(path).expect("the file is readable");
        let mut outcomes: BTreeMap<(i64, String), usize> = BTreeMap::new();
        let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
        let mut lines = 0;
        for line in content.lines().filter(|l| !l.trim().is_empty()) {
            let value: serde_json::Value = serde_json::from_str(line).expect("a JSON line");
            let year = value["year"].as_i64().expect("a year");
            let document = Html::parse_document(value["html"].as_str().expect("html"));
            let outcome = match FreshDirect2026.supports(&document) {
                None => "unsupported".to_owned(),
                Some(_) => match FreshDirect2026.parse(&document) {
                    Err(error) => format!("failed: {error}"),
                    Ok(receipt) => {
                        lines += receipt.lines.len();
                        for anomaly in &receipt.anomalies {
                            *reasons.entry(anomaly.reason.clone()).or_default() += 1;
                        }
                        let r = reconcile(&receipt).expect("the amounts of the receipt add up");
                        if r.reconciled() {
                            "ok".to_owned()
                        } else {
                            format!(
                                "partial: subtotal {} total {} line issues {}",
                                r.subtotal_matches,
                                r.total_matches,
                                r.line_issues.len().min(1)
                            )
                        }
                    }
                },
            };
            *outcomes.entry((year, outcome)).or_default() += 1;
        }
        let mut by_outcome: BTreeMap<String, usize> = BTreeMap::new();
        for ((year, outcome), count) in &outcomes {
            println!("{year} {count:4} {outcome}");
            *by_outcome.entry(outcome.clone()).or_default() += count;
        }
        println!("--- all years, {lines} lines");
        for (outcome, count) in &by_outcome {
            println!("{count:4} {outcome}");
        }
        for (reason, count) in &reasons {
            println!("anomaly {count:4} {reason}");
        }
    }

    fn row(cells: &[&str]) -> Row {
        Row {
            cells: cells.iter().map(|cell| (*cell).to_owned()).collect(),
            link_path: None,
            header: false,
        }
    }

    #[test]
    fn a_row_whose_name_is_too_long_for_an_alias_is_a_row_that_is_not_read() {
        // The name becomes the key of an alias, and the store has a size limit for a key.
        // Before 2026-11-21 such a line was read, the store refused the alias, and each later
        // run of the parse step stopped at that receipt.
        let at_the_limit = "a".repeat(ALIAS_KEY_MAX_BYTES);
        let above = "a".repeat(ALIAS_KEY_MAX_BYTES + 1);
        let item =
            |name: &str| classify(&row(&["1/1", name, "", "$1.00/ea", "", "$1.00"]), 1, None);
        assert!(matches!(item(&at_the_limit), Classified::Item(_)));
        let Classified::Unknown(reason) = item(&above) else {
            panic!("a name above the limit is no item");
        };
        // The reason says the limit and not the name: the name is outside data.
        assert_eq!(reason, "product name longer than 1024 bytes");
        // The limit is on the normalized name and size together, in bytes: a size in
        // parentheses counts, and a letter of two bytes counts twice.
        let with_size = format!("{} - (12 oz)", "a".repeat(ALIAS_KEY_MAX_BYTES - 5));
        assert!(matches!(item(&with_size), Classified::Item(_)));
        let with_longer_size = format!("{} - (12 oz)", "a".repeat(ALIAS_KEY_MAX_BYTES - 4));
        assert!(matches!(item(&with_longer_size), Classified::Unknown(_)));
        // The limit is on the name after it is normalized, which can be longer than the name
        // on the receipt: one letter of three bytes becomes 33 bytes, and a capital I with a
        // dot, two bytes, becomes three.
        assert!(matches!(item(&"\u{fdfa}".repeat(31)), Classified::Item(_)));
        assert!(matches!(
            item(&"\u{fdfa}".repeat(32)),
            Classified::Unknown(_)
        ));
        assert!(matches!(item(&"\u{130}".repeat(341)), Classified::Item(_)));
        assert!(matches!(
            item(&"\u{130}".repeat(342)),
            Classified::Unknown(_)
        ));
        let wide = "\u{e9}".repeat(ALIAS_KEY_MAX_BYTES / 2 + 1);
        assert!(matches!(item(&wide), Classified::Unknown(_)));
        assert!(matches!(
            item(&"\u{e9}".repeat(ALIAS_KEY_MAX_BYTES / 2)),
            Classified::Item(_)
        ));
    }

    #[test]
    fn a_row_with_fewer_cells_than_a_rule_reads_matches_no_rule() {
        // No cell, and only empty cells.
        assert!(matches!(classify(&row(&[]), 1, None), Classified::Blank));
        assert!(matches!(
            classify(&row(&["", ""]), 1, None),
            Classified::Blank
        ));
        // A quantity cell with no cell to four cells after it: an item row needs five.
        for count in 1..=5 {
            let mut cells = vec!["1/1"];
            cells.resize(count, "x");
            let Classified::Unknown(reason) = classify(&row(&cells), 1, None) else {
                panic!("a quantity row with {count} cells is not an item");
            };
            assert_eq!(reason, format!("quantity row with {count} cells"));
        }
        // Six cells: an item with no flag cell. Seven and more: each later cell can be a flag.
        let six = ["2/2", "Milk", "", "$1.00/ea", "", "$2.00"];
        let Classified::Item(item) = classify(&row(&six), 4, Some("Dairy")) else {
            panic!("six cells are an item row");
        };
        assert_eq!(item.line_no, 4);
        assert_eq!(item.raw_name, "Milk");
        assert_eq!(item.department.as_deref(), Some("Dairy"));
        assert_eq!(item.final_price, Money::from_cents(200));
        assert_eq!(item.options_price, None);
        assert_eq!(item.flags, Flags::default());
        let nine = ["2/2", "Milk", "", "", "$0.50", "$2.00", "T", "", "D"];
        let Classified::Item(item) = classify(&row(&nine), 1, None) else {
            panic!("nine cells are an item row");
        };
        assert_eq!(item.options_price, Some(Money::from_cents(50)));
        assert!(item.flags.taxable && item.flags.deposit && !item.flags.special);
        // A price cell that is no amount.
        let bad_price = ["2/2", "Milk", "", "", "", "two dollars"];
        assert!(matches!(
            classify(&row(&bad_price), 1, None),
            Classified::Unknown(reason) if reason == "price cell \"two dollars\" is not an amount"
        ));
        // A quantity cell that is not the first cell starts no item row.
        assert!(matches!(
            classify(&row(&["", "2/2", "Milk", "", "", "", "$2.00"]), 1, None),
            Classified::Unknown(_)
        ));
    }

    #[test]
    fn two_totals_rows_of_one_label_are_added_and_a_sum_outside_the_range_is_an_error() {
        // A receipt of two stores prints a subtotal and a tax for each.
        let mut sums = FooterSums::default();
        sums.apply("subtotal", Money::from_cents(1000))
            .expect("first");
        sums.apply("subtotal", Money::from_cents(250))
            .expect("second");
        sums.apply("tax", Money::from_cents(30)).expect("first");
        sums.apply("tax", Money::from_cents(12)).expect("second");
        assert_eq!(sums.subtotal, Some(Money::from_cents(1250)));
        assert_eq!(sums.tax, Some(Money::from_cents(42)));
        // A label that is not added keeps the last row.
        sums.apply("tip", Money::from_cents(500)).expect("first");
        sums.apply("tip", Money::from_cents(600)).expect("second");
        assert_eq!(sums.tip, Some(Money::from_cents(600)));

        let most = Money::parse("$99,999,999.99").expect("the largest amount");
        for label in ["subtotal", "tax", "bottle deposit", "service fee"] {
            let mut sums = FooterSums::default();
            sums.apply(label, most).expect("one row is an amount");
            assert_eq!(
                sums.apply(label, most),
                Err(ParseError::TotalsOutOfRange),
                "{label}"
            );
        }
    }

    #[test]
    fn a_totals_row_is_a_label_and_an_amount_with_at_most_one_flag_cell() {
        let footer = |cells: &[&str]| {
            if let Classified::Footer(label, amount) = classify(&row(cells), 0, None) {
                Some((label, amount))
            } else {
                None
            }
        };
        assert_eq!(
            footer(&["Subtotal", "$5.00"]),
            Some(("subtotal".to_owned(), Money::from_cents(500)))
        );
        assert_eq!(
            footer(&["", "Delivery Fee", "", "$5.99", "T"]),
            Some(("delivery fee".to_owned(), Money::from_cents(599)))
        );
        // A third cell that is no flag, four cells, and a label with no amount.
        assert_eq!(footer(&["Tip", "$1.00", "x"]), None);
        assert_eq!(footer(&["Tip", "$1.00", "T", "T"]), None);
        assert_eq!(footer(&["Tip", "one"]), None);
        // One cell alone is a department, unless it is long or an amount.
        assert!(matches!(
            classify(&row(&["", "Dairy"]), 0, None),
            Classified::Department(name) if name == "Dairy"
        ));
        assert!(matches!(
            classify(&row(&["$5.00"]), 0, None),
            Classified::Unknown(_)
        ));
        let long = "x".repeat(41);
        assert!(matches!(
            classify(&row(&[long.as_str()]), 0, None),
            Classified::Unknown(_)
        ));
    }

    #[test]
    fn the_item_lines_of_a_receipt_are_numbered_from_one_without_a_gap() {
        let receipt = FreshDirect2026
            .parse(&Html::parse_document(FIXTURE))
            .expect("the fixture is a receipt");
        assert!(receipt.lines.len() > 1);
        let numbers: Vec<u32> = receipt.lines.iter().map(|line| line.line_no).collect();
        let expected: Vec<u32> = (1..).take(receipt.lines.len()).collect();
        assert_eq!(numbers, expected);
    }

    #[test]
    fn every_group_that_the_code_indexes_takes_part_in_every_match() {
        // `caps[1]` stops the program when group 1 took no part in the match, and no lint
        // sees it. For these patterns the engine can say that each group is in every match.
        for (name, pattern) in [
            ("ORDER_NUMBER", &*ORDER_NUMBER),
            ("DELIVERY_DATE", &*DELIVERY_DATE),
            ("DELIVERY_SENTENCE", &*DELIVERY_SENTENCE),
            ("DELIVERY_WINDOW", &*DELIVERY_WINDOW),
            ("QTY_CELL", &*QTY_CELL),
            ("WEIGHT_CELL", &*WEIGHT_CELL),
            ("UNIT_PRICE_CELL", &*UNIT_PRICE_CELL),
            ("FOOTER_LABEL", &*FOOTER_LABEL),
        ] {
            assert_eq!(
                pattern.static_captures_len(),
                Some(pattern.captures_len()),
                "{name}"
            );
        }
    }
}
