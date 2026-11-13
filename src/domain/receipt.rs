//! A parsed receipt: what the mail says an order contained, and the arithmetic that says
//! whether the parser read it right. A parser (an adapter) produces a [`Receipt`]; the domain
//! owns the shape and [`reconcile`] (REAS-ARCH-005).

use chrono::NaiveDate;
use serde::Serialize;

use super::ids::OrderNumber;
use super::money::{Money, Quantity};
use super::states::SoldBy;

/// One cent: the tolerance for every reconciliation check, because the receipt rounds each
/// line to the cent and the sum of rounded lines can differ from a rounded total by that much.
pub fn tolerance() -> Money {
    Money::from_cents(1)
}

/// Whether a unit price is per piece or per pound, as the receipt prints `($5.79/ea)` or
/// `($2.99/lb)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PricePer {
    Each,
    Pound,
}

impl PricePer {
    pub fn as_str(self) -> &'static str {
        match self {
            PricePer::Each => "ea",
            PricePer::Pound => "lb",
        }
    }

    pub fn sold_by(self) -> SoldBy {
        match self {
            PricePer::Each => SoldBy::Each,
            PricePer::Pound => SoldBy::Weight,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct UnitPrice {
    pub amount: Money,
    pub per: PricePer,
}

/// A weight as the receipt prints it, `1.06 lb`. The unit is kept as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Weight {
    pub value: Quantity,
    pub unit: String,
}

/// The three flag columns: taxable, special price, bottle deposit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct Flags {
    pub taxable: bool,
    pub special: bool,
    pub deposit: bool,
}

impl Flags {
    /// The flags as the receipt prints them and the database stores them.
    pub fn as_letters(self) -> Vec<&'static str> {
        let mut letters = Vec::new();
        if self.taxable {
            letters.push("T");
        }
        if self.special {
            letters.push("S");
        }
        if self.deposit {
            letters.push("D");
        }
        letters
    }
}

/// One item row of the receipt, with the quantity cell kept verbatim. `qty_first` and
/// `qty_second` are the two numbers of `2/1.56`; which one is "delivered" is confirmed against a
/// real receipt before either is given a better name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineItem {
    pub line_no: u32,
    pub department: Option<String>,
    pub raw_name: String,
    pub product_url_path: Option<String>,
    pub qty_first: Quantity,
    pub qty_second: Quantity,
    pub weight: Option<Weight>,
    pub unit_price: Option<UnitPrice>,
    pub options_price: Option<Money>,
    pub final_price: Money,
    pub flags: Flags,
}

impl LineItem {
    /// The quantity the receipt charged for: the weight when the item was weighed, else the
    /// second number of the quantity cell.
    pub fn billed_qty(&self) -> Quantity {
        match &self.weight {
            Some(weight) => weight.value,
            None => self.qty_second,
        }
    }

    pub fn sold_by(&self) -> SoldBy {
        match (&self.weight, self.unit_price) {
            (Some(_), _) => SoldBy::Weight,
            (None, Some(price)) => price.per.sold_by(),
            (None, None) => SoldBy::Each,
        }
    }
}

/// A row of the item table that no rule classified. The raw cells are kept so the parser can
/// be taught; the order is stored regardless.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Anomaly {
    pub row_index: usize,
    pub cells: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Totals {
    pub subtotal: Money,
    pub tax: Option<Money>,
    pub bottle_deposit: Option<Money>,
    pub delivery_fee: Option<Money>,
    pub service_fee: Option<Money>,
    pub tip: Option<Money>,
    pub total: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Receipt {
    pub order_number: OrderNumber,
    pub delivery_date: NaiveDate,
    pub delivery_window: Option<String>,
    pub lines: Vec<LineItem>,
    pub anomalies: Vec<Anomaly>,
    pub totals: Totals,
}

/// A line whose own arithmetic does not hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineIssue {
    pub line_no: u32,
    pub expected: Money,
    pub printed: Money,
}

/// What [`reconcile`] found. `reconciled` is what the order row stores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Reconciliation {
    pub line_sum: Money,
    pub subtotal_matches: bool,
    pub total_matches: bool,
    pub line_issues: Vec<LineIssue>,
}

impl Reconciliation {
    pub fn reconciled(&self) -> bool {
        self.subtotal_matches && self.total_matches && self.line_issues.is_empty()
    }
}

/// Checks the receipt against itself: the lines sum to the subtotal, the subtotal and its
/// additions sum to the total, and each priced line is unit price times billed quantity. A line
/// with an options price or a special-price flag is exempt from the last check, because the
/// receipt does not print enough to recompute it.
pub fn reconcile(receipt: &Receipt) -> Reconciliation {
    let line_sum = receipt.lines.iter().fold(Money::ZERO, |sum, line| {
        sum.saturating_add(line.final_price)
    });
    let subtotal_matches = line_sum.abs_diff(receipt.totals.subtotal) <= tolerance();

    let additions = [
        receipt.totals.tax,
        receipt.totals.bottle_deposit,
        receipt.totals.delivery_fee,
        receipt.totals.service_fee,
        receipt.totals.tip,
    ];
    let expected_total = additions
        .iter()
        .flatten()
        .fold(receipt.totals.subtotal, |sum, add| sum.saturating_add(*add));
    let total_matches = expected_total.abs_diff(receipt.totals.total) <= tolerance();

    let line_issues = receipt
        .lines
        .iter()
        .filter(|line| line.options_price.is_none() && !line.flags.special)
        .filter_map(|line| {
            let unit = line.unit_price?;
            let expected = unit.amount.times(line.billed_qty()).ok()?;
            (expected.abs_diff(line.final_price) > tolerance()).then_some(LineIssue {
                line_no: line.line_no,
                expected,
                printed: line.final_price,
            })
        })
        .collect();

    Reconciliation {
        line_sum,
        subtotal_matches,
        total_matches,
        line_issues,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(
        no: u32,
        qty: &str,
        unit: Option<(i64, PricePer)>,
        weight: Option<&str>,
        final_cents: i64,
    ) -> LineItem {
        LineItem {
            line_no: no,
            department: Some("Dairy".to_owned()),
            raw_name: format!("Item {no}"),
            product_url_path: None,
            qty_first: Quantity::parse(qty).expect("qty"),
            qty_second: Quantity::parse(qty).expect("qty"),
            weight: weight.map(|w| Weight {
                value: Quantity::parse(w).expect("weight"),
                unit: "lb".to_owned(),
            }),
            unit_price: unit.map(|(cents, per)| UnitPrice {
                amount: Money::from_cents(cents),
                per,
            }),
            options_price: None,
            final_price: Money::from_cents(final_cents),
            flags: Flags::default(),
        }
    }

    fn receipt(lines: Vec<LineItem>, subtotal: i64, total: i64) -> Receipt {
        Receipt {
            order_number: OrderNumber::parse("12345678901").expect("order number"),
            delivery_date: NaiveDate::from_ymd_opt(2026, 3, 3).expect("date"),
            delivery_window: None,
            lines,
            anomalies: Vec::new(),
            totals: Totals {
                subtotal: Money::from_cents(subtotal),
                tax: Some(Money::from_cents(100)),
                bottle_deposit: None,
                delivery_fee: Some(Money::from_cents(599)),
                service_fee: None,
                tip: Some(Money::from_cents(500)),
                total: Money::from_cents(total),
            },
        }
    }

    #[test]
    fn the_sample_rows_of_the_brief_reconcile() {
        // 3 milk at $5.79 = $17.37; 1.06 lb tomatoes at $2.99/lb = $3.17; 2 peppers at $4.00 = $8.00.
        let lines = vec![
            line(1, "3", Some((579, PricePer::Each)), None, 1737),
            line(2, "2", Some((299, PricePer::Pound)), Some("1.06"), 317),
            line(3, "2", Some((400, PricePer::Each)), None, 800),
        ];
        let r = reconcile(&receipt(lines, 2854, 2854 + 100 + 599 + 500));
        assert_eq!(r.line_sum, Money::from_cents(2854));
        assert!(r.subtotal_matches);
        assert!(r.total_matches);
        assert!(r.line_issues.is_empty());
        assert!(r.reconciled());
    }

    #[test]
    fn a_wrong_line_price_and_a_wrong_subtotal_are_both_reported() {
        let lines = vec![line(1, "3", Some((579, PricePer::Each)), None, 1700)];
        let r = reconcile(&receipt(lines, 1737, 1737 + 100 + 599 + 500));
        assert!(!r.subtotal_matches, "{r:?}");
        assert!(r.total_matches);
        assert_eq!(
            r.line_issues,
            vec![LineIssue {
                line_no: 1,
                expected: Money::from_cents(1737),
                printed: Money::from_cents(1700),
            }]
        );
        assert!(!r.reconciled());
    }

    #[test]
    fn a_special_price_line_is_exempt_from_the_unit_check() {
        let mut special = line(1, "2", Some((400, PricePer::Each)), None, 600);
        special.flags.special = true;
        let r = reconcile(&receipt(vec![special], 600, 600 + 100 + 599 + 500));
        assert!(r.line_issues.is_empty());
        assert!(r.reconciled());
    }

    #[test]
    fn one_cent_of_rounding_is_tolerated_and_two_are_not() {
        let lines = vec![
            line(1, "1", Some((333, PricePer::Each)), None, 333),
            line(2, "1", Some((333, PricePer::Each)), None, 333),
            line(3, "1", Some((333, PricePer::Each)), None, 333),
        ];
        assert!(reconcile(&receipt(lines.clone(), 1000, 1000 + 1199)).subtotal_matches);
        assert!(!reconcile(&receipt(lines, 1001, 1001 + 1199)).subtotal_matches);
    }

    #[test]
    fn billed_quantity_is_the_weight_when_there_is_one() {
        let weighed = line(1, "2", Some((299, PricePer::Pound)), Some("1.06"), 317);
        assert_eq!(weighed.billed_qty(), Quantity::parse("1.06").expect("qty"));
        assert_eq!(weighed.sold_by(), SoldBy::Weight);
        let counted = line(2, "3", Some((579, PricePer::Each)), None, 1737);
        assert_eq!(counted.billed_qty(), Quantity::parse("3").expect("qty"));
        assert_eq!(counted.sold_by(), SoldBy::Each);
        assert_eq!(
            Flags {
                taxable: true,
                special: false,
                deposit: true
            }
            .as_letters(),
            vec!["T", "D"]
        );
    }
}
