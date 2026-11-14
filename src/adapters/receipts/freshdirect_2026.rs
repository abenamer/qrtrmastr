//! The FreshDirect receipt layout seen on 2026 mail: an item table whose first cell is the
//! quantity pair `ordered/second`, a product name that may be a link, a weight, a unit price in
//! parentheses, an options price, the final price, and three one-letter flag columns. Department
//! names are rows of their own. Totals sit in a separate two-column table.
//!
//! Every rule reads cell content, never a class name, so a cosmetic change to the template does
//! not break it. A row that fits no rule becomes an anomaly, never a dropped line.

use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};

use super::{Confidence, ParseError, ReceiptParser};
use crate::application::ports::ParserTag;
use crate::domain::ids::OrderNumber;
use crate::domain::money::{Money, Quantity};
use crate::domain::receipt::{
    Anomaly, Flags, LineItem, PricePer, Receipt, Totals, UnitPrice, Weight,
};

pub struct FreshDirect2026;

pub const NAME: &str = "fd_receipt_2026";
pub const VERSION: i32 = 1;

static ORDER_NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)order\s*(?:number|#|no\.?)?\s*:?\s*(\d{11,13})\b").expect("regex")
});
static DELIVERY_DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)delivery\s+date\s*:?\s*([A-Za-z]+,?\s+[A-Za-z]+\s+\d{1,2},?\s+\d{4}|[A-Za-z]+\s+\d{1,2},?\s+\d{4}|\d{1,2}/\d{1,2}/\d{4}|\d{4}-\d{2}-\d{2})").expect("regex")
});
static DELIVERY_WINDOW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)delivery\s+(?:window|time)\s*:?\s*(\d{1,2}(?::\d{2})?\s*(?:am|pm)?\s*[-\u{2013}]\s*\d{1,2}(?::\d{2})?\s*(?:am|pm)?)")
        .expect("regex")
});
static QTY_CELL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(\d+(?:\.\d+)?)\s*/\s*(\d+(?:\.\d+)?)\s*$").expect("regex"));
static WEIGHT_CELL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*(\d+(?:\.\d+)?)\s*(lb|lbs|oz|kg|g)\.?\s*$").expect("regex")
});
static UNIT_PRICE_CELL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*\(?\s*(\$?\s*[\d,]+\.\d{2})\s*/\s*(ea|each|lb|pound)\.?\s*\)?\s*$")
        .expect("regex")
});
static MONEY_CELL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\(?-?\$?\s*[\d,]+\.\d{2}\)?\s*$").expect("regex"));
static FOOTER_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*(subtotal|sub-total|tax|sales tax|bottle deposit|deposit|delivery fee|delivery|service fee|tip|driver tip|order total|total)\s*:?\s*$")
        .expect("regex")
});
static WS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").expect("regex"));

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("valid selector")
}

fn text_of(element: ElementRef<'_>) -> String {
    let joined: String = element.text().collect::<Vec<_>>().join(" ");
    WS.replace_all(joined.trim(), " ").into_owned()
}

fn parse_date(text: &str) -> Option<NaiveDate> {
    let cleaned = WS.replace_all(text.trim(), " ");
    let cleaned = cleaned.trim_end_matches(',');
    let formats = [
        "%A, %B %d, %Y",
        "%A %B %d, %Y",
        "%A, %B %d %Y",
        "%B %d, %Y",
        "%B %d %Y",
        "%a, %b %d, %Y",
        "%b %d, %Y",
        "%m/%d/%Y",
        "%Y-%m-%d",
    ];
    formats
        .iter()
        .find_map(|format| NaiveDate::parse_from_str(cleaned, format).ok())
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
    let non_empty: Vec<&String> = row.cells.iter().filter(|c| !c.is_empty()).collect();
    if non_empty.is_empty() {
        return Classified::Blank;
    }
    if let Some(caps) = QTY_CELL.captures(&row.cells[0]) {
        if row.cells.len() < 6 {
            return Classified::Unknown(format!("quantity row with {} cells", row.cells.len()));
        }
        let (Ok(qty_first), Ok(qty_second)) =
            (Quantity::parse(&caps[1]), Quantity::parse(&caps[2]))
        else {
            return Classified::Unknown(
                "quantity cell does not hold two positive numbers".to_owned(),
            );
        };
        let raw_name = row.cells[1].clone();
        if raw_name.is_empty() {
            return Classified::Unknown("quantity row without a product name".to_owned());
        }
        let weight = WEIGHT_CELL.captures(&row.cells[2]).and_then(|w| {
            Quantity::parse(&w[1]).ok().map(|value| Weight {
                value,
                unit: w[2].to_lowercase(),
            })
        });
        let unit_price = UNIT_PRICE_CELL.captures(&row.cells[3]).and_then(|u| {
            let amount = Money::parse(&u[1].replace(' ', "")).ok()?;
            let per = match u[2].to_lowercase().as_str() {
                "ea" | "each" => PricePer::Each,
                _ => PricePer::Pound,
            };
            Some(UnitPrice { amount, per })
        });
        let options_price = (!row.cells[4].is_empty())
            .then(|| money(&row.cells[4]))
            .flatten();
        let Some(final_price) = money(&row.cells[5]) else {
            return Classified::Unknown(format!("price cell {:?} is not an amount", row.cells[5]));
        };
        let mut flags = Flags::default();
        for cell in row.cells.iter().skip(6) {
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
    if non_empty.len() == 2
        && let (Some(label), Some(amount)) =
            (FOOTER_LABEL.captures(non_empty[0]), money(non_empty[1]))
    {
        return Classified::Footer(label[1].to_lowercase(), amount);
    }
    if non_empty.len() == 1
        && !non_empty[0].contains('$')
        && non_empty[0].len() <= 40
        && !MONEY_CELL.is_match(non_empty[0])
    {
        return Classified::Department(non_empty[0].clone());
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
        let date_text = DELIVERY_DATE
            .captures(&text)
            .map(|c| c[1].to_owned())
            .ok_or(ParseError::NoDeliveryDate)?;
        let delivery_date = parse_date(&date_text).ok_or(ParseError::BadDeliveryDate(date_text))?;
        let delivery_window = DELIVERY_WINDOW
            .captures(&text)
            .map(|c| c[1].trim().to_owned());

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
        let mut totals = Totals::default();
        let mut found_subtotal = false;
        let mut found_total = false;
        let mut department: Option<String> = None;
        let mut line_no: u32 = 0;

        let mut apply_footer = |label: &str, amount: Money, totals: &mut Totals| match label {
            "subtotal" | "sub-total" => {
                totals.subtotal = amount;
                found_subtotal = true;
            }
            "tax" | "sales tax" => totals.tax = Some(amount),
            "bottle deposit" | "deposit" => totals.bottle_deposit = Some(amount),
            "delivery fee" | "delivery" => totals.delivery_fee = Some(amount),
            "service fee" => totals.service_fee = Some(amount),
            "tip" | "driver tip" => totals.tip = Some(amount),
            "order total" | "total" => {
                totals.total = amount;
                found_total = true;
            }
            _ => {}
        };

        for (index, tr) in own_rows(item_table).into_iter().enumerate() {
            let row = read_row(tr);
            match classify(&row, line_no + 1, department.as_deref()) {
                Classified::Header | Classified::Blank => {}
                Classified::Department(name) => department = Some(name),
                Classified::Item(item) => {
                    line_no += 1;
                    lines.push(*item);
                }
                Classified::Footer(label, amount) => apply_footer(&label, amount, &mut totals),
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
                    apply_footer(&label, amount, &mut totals);
                }
            }
        }

        if lines.is_empty() {
            return Err(ParseError::NoItems);
        }
        if !found_subtotal {
            return Err(ParseError::NoSubtotal);
        }
        if !found_total {
            return Err(ParseError::NoTotal);
        }

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
        assert_eq!(
            receipt.delivery_date,
            NaiveDate::from_ymd_opt(2026, 3, 3).expect("date")
        );
        assert_eq!(receipt.delivery_window.as_deref(), Some("8:00am - 10:00am"));
        assert_eq!(receipt.lines.len(), 6);
        assert_eq!(receipt.totals.subtotal, Money::from_cents(4777));
        assert_eq!(receipt.totals.total, Money::from_cents(6250));
        assert_eq!(receipt.totals.tip, Some(Money::from_cents(600)));
        let r = reconcile(&receipt);
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
        insta::assert_yaml_snapshot!(from_redacted);
    }
}
