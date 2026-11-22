//! A parsed receipt: what the mail says an order contained, and the arithmetic that says
//! whether the parser read it right. A parser (an adapter) produces a [`Receipt`]; the domain
//! owns the shape and [`reconcile`] (REAS-ARCH-005).

use jiff::civil::Date;

use super::ids::OrderNumber;
use super::money::{Money, MoneyError, Quantity};
use super::states::SoldBy;

/// One cent: the tolerance for every reconciliation check, because the receipt rounds each
/// line to the cent and the sum of rounded lines can differ from a rounded total by that much.
pub fn tolerance() -> Money {
    Money::from_cents(1)
}

/// Whether a unit price is per piece or per pound, as the receipt prints `($5.79/ea)` or
/// `($2.99/lb)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PricePer {
    /// The price is for one piece: the receipt prints `/ea` or `/each`.
    Each,
    /// The price is for one pound: the receipt prints `/lb` or `/pound`.
    Pound,
}

impl PricePer {
    /// The unit as `grocery_order_lines.unit_price_per` stores it: `ea` or `lb`.
    pub fn as_str(self) -> &'static str {
        match self {
            PricePer::Each => "ea",
            PricePer::Pound => "lb",
        }
    }

    /// How a product with this unit price is sold: by the piece for a price per piece, by
    /// weight for a price per pound.
    pub fn sold_by(self) -> SoldBy {
        match self {
            PricePer::Each => SoldBy::Each,
            PricePer::Pound => SoldBy::Weight,
        }
    }
}

/// The price of one piece or of one pound of an item, from the unit price cell of the receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitPrice {
    /// The dollar amount of the cell: `$5.79` of `($5.79/ea)`.
    pub amount: Money,
    /// Whether `amount` is for one piece or for one pound.
    pub per: PricePer,
}

/// A weight as the receipt prints it, `1.06 lb`. A parser gives the unit in lower case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weight {
    /// The number of the weight cell. When a line has a weight, this is its billed quantity.
    pub value: Quantity,
    /// The unit that follows the number, such as `lb`. The domain does not read it and does
    /// not convert `value`: [`reconcile`] multiplies the unit price by `value` as it is.
    pub unit: String,
}

/// The three flag columns: taxable, special price, bottle deposit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags {
    /// The row has the letter `T`: the item is taxable.
    pub taxable: bool,
    /// The row has the letter `S`: the item has a special price. [`reconcile`] does not
    /// recompute the price of such a line.
    pub special: bool,
    /// The row has the letter `D`: the item carries a bottle deposit.
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineItem {
    /// The position of the item among the item rows of the receipt, from 1. A department row,
    /// a header row and an anomaly take no number. With the order number it identifies a line.
    pub line_no: u32,
    /// The department row that the item stands under, such as `Dairy`. `None` when no
    /// department row comes before the item.
    pub department: Option<String>,
    /// The name cell as the receipt prints it, with the size in parentheses when it has one.
    /// The alias of the line is made from this text.
    pub raw_name: String,
    /// The path of the product link of the row, without host, query string and fragment.
    /// `None` when the row has no link with a path. It is stored on the alias that the line
    /// creates, not on the line.
    pub product_url_path: Option<String>,
    /// The left number of the quantity cell, verbatim: `2` of `2/1.56`.
    pub qty_first: Quantity,
    /// The right number of the quantity cell, verbatim: `1.56` of `2/1.56`. It is the billed
    /// quantity of a line without a weight.
    pub qty_second: Quantity,
    /// The weight cell. `None` when the cell is empty, and also when it does not read as a
    /// positive number below 10,000,000 of at most three decimal places and one of the units
    /// `lb`, `lbs`, `oz`, `kg` and `g`. The billed quantity is then the second number of the quantity cell.
    pub weight: Option<Weight>,
    /// The unit price cell. `None` when the cell is empty or does not read as a price for one
    /// piece or one pound; [`reconcile`] then has nothing to check the line against.
    pub unit_price: Option<UnitPrice>,
    /// The amount of the options price cell. `None` when the cell is empty or is not an
    /// amount. [`reconcile`] does not recompute the price of a line that has one.
    pub options_price: Option<Money>,
    /// The final price cell: what the receipt charges for the line, for its whole billed
    /// quantity. The final prices of the lines sum to the subtotal.
    pub final_price: Money,
    /// The letters of the flag columns of the row.
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

    /// Whether the receipt prices this item by the piece or by weight: what a product gets
    /// when this line creates it. By weight when the line has a weight; else what the unit
    /// price says; by the piece when the line has neither.
    pub fn sold_by(&self) -> SoldBy {
        match (&self.weight, self.unit_price) {
            (Some(_), _) => SoldBy::Weight,
            (None, Some(price)) => price.per.sold_by(),
            (None, None) => SoldBy::Each,
        }
    }
}

/// A row of the item table that no rule classified, or a check of the amounts that did not
/// hold. The raw cells of a row are kept so the parser can be taught; the order is stored
/// regardless.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anomaly {
    /// The position of the row among the rows of the item table, from 0; every row counts,
    /// also a header row and a department row. The use case that stores the order also
    /// records each reconciliation mismatch as an anomaly; such an anomaly is no row and has 0.
    pub row_index: usize,
    /// The text of each cell of the row, in order. Stored as `raw_cells`. Empty for a
    /// reconciliation mismatch.
    pub cells: Vec<String>,
    /// Why the row is an anomaly, as a sentence for the maintainer: the rule that the row
    /// almost matched, that no rule matched, or the amounts that differ.
    pub reason: String,
}

/// The totals rows of a receipt, in dollars. [`reconcile`] checks that the subtotal and the
/// five amounts after it sum to the total. One of the five is `None` when the receipt prints
/// no such row, and then adds nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Totals {
    /// The subtotal as the receipt prints it: the sum of the final prices of the lines. When a
    /// receipt prints one subtotal for each store, a parser gives their sum.
    pub subtotal: Money,
    /// The tax of the whole order.
    pub tax: Option<Money>,
    /// The bottle deposit of the whole order.
    pub bottle_deposit: Option<Money>,
    /// The delivery fee.
    pub delivery_fee: Option<Money>,
    /// The service fee. The 2026 parser adds a fuel surcharge row to it.
    pub service_fee: Option<Money>,
    /// The tip.
    pub tip: Option<Money>,
    /// The order total as the receipt prints it: what the order cost.
    pub total: Money,
}

/// One receipt as a parser read it: the order it is for, its item lines, the rows that no rule
/// classified, and its totals. A parser does not check the arithmetic; [`reconcile`] does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    /// The order that the receipt is for. A later receipt with the same number replaces the
    /// stored order.
    pub order_number: OrderNumber,
    /// The delivery day that the receipt states, not the date of the mail. The recommendation
    /// rules measure every gap between these days.
    pub delivery_date: Date,
    /// The delivery time window as text, such as `8:00am - 10:00am`. `None` when the receipt
    /// states none. Nothing reads it as times.
    pub delivery_window: Option<String>,
    /// The item rows, in the order of the receipt.
    pub lines: Vec<LineItem>,
    /// The rows of the item table that no rule classified. Empty when every row was
    /// classified. The use case that stores the order adds one for each reconciliation
    /// mismatch.
    pub anomalies: Vec<Anomaly>,
    /// The totals rows.
    pub totals: Totals,
}

/// A line whose own arithmetic does not hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineIssue {
    /// The `line_no` of the line.
    pub line_no: u32,
    /// The unit price times the billed quantity, rounded to the cent.
    pub expected: Money,
    /// The final price that the receipt prints for the line. It is more than one cent from
    /// `expected`.
    pub printed: Money,
}

/// What [`reconcile`] found. `reconciled` is what the order row stores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reconciliation {
    /// The sum of the final prices of all lines.
    pub line_sum: Money,
    /// Whether `line_sum` is within one cent of the subtotal.
    pub subtotal_matches: bool,
    /// Whether the subtotal plus tax, bottle deposit, delivery fee, service fee and tip is
    /// within one cent of the total.
    pub total_matches: bool,
    /// The lines whose final price is more than one cent from unit price times billed
    /// quantity. Empty when every checked line holds. A line without a unit price, with an
    /// options price or with the special-price flag is not checked.
    pub line_issues: Vec<LineIssue>,
}

impl Reconciliation {
    /// Whether all three checks hold. This is the value of `grocery_orders.reconciled`, and
    /// the difference between the parse outcomes `ok` and `partial`.
    pub fn reconciled(&self) -> bool {
        self.subtotal_matches && self.total_matches && self.line_issues.is_empty()
    }
}

/// Whether two amounts are at most the tolerance apart.
fn within_tolerance(one: Money, other: Money) -> Result<bool, MoneyError> {
    let gap = one.distance(other)?;
    Ok(gap.checked_cmp(tolerance())? != std::cmp::Ordering::Greater)
}

/// Checks the receipt against itself: the lines sum to the subtotal, the subtotal and its
/// additions sum to the total, and each priced line is unit price times billed quantity. A line
/// with an options price or a special-price flag is exempt from the last check, because the
/// receipt does not print enough to recompute it.
///
/// # Errors
///
/// A receipt whose amounts cannot be added has no reconciliation; the caller records it as
/// a receipt that could not be read.
///
/// - [`MoneyError::Overflow`] when a sum, a product or a gap of the amounts is outside the
///   range of an amount, 99,999,999.99.
/// - [`MoneyError::CurrencyMismatch`] when an amount of the receipt is not in the currency of
///   its subtotal, or the receipt is not in US dollars, the currency of the tolerance. No
///   parser gives such a receipt today.
pub fn reconcile(receipt: &Receipt) -> Result<Reconciliation, MoneyError> {
    // Every amount of the receipt is in the currency of its subtotal, also the amounts that
    // no check below reads: the options price of a line, and the unit price of a line that
    // is exempt.
    let zero = Money::zero(receipt.totals.subtotal.currency());
    let of_the_lines = receipt.lines.iter().flat_map(|line| {
        [
            Some(line.final_price),
            line.unit_price.map(|unit| unit.amount),
            line.options_price,
        ]
    });
    let of_the_totals = [
        Some(receipt.totals.total),
        receipt.totals.tax,
        receipt.totals.bottle_deposit,
        receipt.totals.delivery_fee,
        receipt.totals.service_fee,
        receipt.totals.tip,
    ];
    for amount in of_the_lines.chain(of_the_totals).flatten() {
        zero.checked_add(amount)?;
    }

    let line_sum = receipt
        .lines
        .iter()
        .try_fold(zero, |sum, line| sum.checked_add(line.final_price))?;
    let subtotal_matches = within_tolerance(line_sum, receipt.totals.subtotal)?;

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
        .try_fold(receipt.totals.subtotal, |sum, add| sum.checked_add(*add))?;
    let total_matches = within_tolerance(expected_total, receipt.totals.total)?;

    let mut line_issues = Vec::new();
    for line in &receipt.lines {
        let Some(unit) = line.unit_price else {
            continue;
        };
        if line.options_price.is_some() || line.flags.special {
            continue;
        }
        let expected = unit.amount.times(line.billed_qty())?;
        if !within_tolerance(expected, line.final_price)? {
            line_issues.push(LineIssue {
                line_no: line.line_no,
                expected,
                printed: line.final_price,
            });
        }
    }

    Ok(Reconciliation {
        line_sum,
        subtotal_matches,
        total_matches,
        line_issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(
        no: u32,
        qty: &str,
        unit: Option<(i32, PricePer)>,
        weight: Option<&str>,
        final_cents: i32,
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

    fn receipt(lines: Vec<LineItem>, subtotal: i32, total: i32) -> Receipt {
        Receipt {
            order_number: OrderNumber::parse("12345678901").expect("order number"),
            delivery_date: jiff::civil::date(2026, 3, 3),
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
        let r = reconcile(&receipt(lines, 2854, 2854 + 100 + 599 + 500)).expect("sums");
        assert_eq!(r.line_sum, Money::from_cents(2854));
        assert!(r.subtotal_matches);
        assert!(r.total_matches);
        assert!(r.line_issues.is_empty());
        assert!(r.reconciled());
    }

    #[test]
    fn a_wrong_line_price_and_a_wrong_subtotal_are_both_reported() {
        let lines = vec![line(1, "3", Some((579, PricePer::Each)), None, 1700)];
        let r = reconcile(&receipt(lines, 1737, 1737 + 100 + 599 + 500)).expect("sums");
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
        let r = reconcile(&receipt(vec![special], 600, 600 + 100 + 599 + 500)).expect("sums");
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
        assert!(
            reconcile(&receipt(lines.clone(), 1000, 1000 + 1199))
                .expect("sums")
                .subtotal_matches
        );
        assert!(
            !reconcile(&receipt(lines, 1001, 1001 + 1199))
                .expect("sums")
                .subtotal_matches
        );
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

    #[test]
    fn a_gap_of_one_cent_is_within_the_tolerance_and_two_cents_are_not() {
        let lines = vec![line(1, "1", None, None, 1000)];
        // Below, at and above the tolerance, on each side of the subtotal and of the total.
        for (subtotal, matches) in [
            (998, false),
            (999, true),
            (1000, true),
            (1001, true),
            (1002, false),
        ] {
            let r = reconcile(&receipt(lines.clone(), subtotal, subtotal + 1199)).expect("sums");
            assert_eq!(r.subtotal_matches, matches, "subtotal {subtotal}");
            assert!(r.total_matches, "subtotal {subtotal}");
        }
        for (total, matches) in [
            (2197, false),
            (2198, true),
            (2199, true),
            (2200, true),
            (2201, false),
        ] {
            let r = reconcile(&receipt(lines.clone(), 1000, total)).expect("sums");
            assert_eq!(r.total_matches, matches, "total {total}");
        }
        // A line: 2.99 a pound times 1.06 pounds is 3.17.
        for (printed, issue) in [
            (315, true),
            (316, false),
            (317, false),
            (318, false),
            (319, true),
        ] {
            let priced = vec![line(
                1,
                "1",
                Some((299, PricePer::Pound)),
                Some("1.06"),
                printed,
            )];
            let r = reconcile(&receipt(priced, printed, printed + 1199)).expect("sums");
            assert_eq!(!r.line_issues.is_empty(), issue, "printed {printed}");
        }
    }

    #[test]
    fn amounts_that_cannot_be_added_give_an_error_and_no_reconciliation() {
        let most = Money::parse("$99,999,999.99").expect("the largest amount");
        // Two lines whose sum is outside the range of an amount. Before 2026-11-21 such a sum
        // was an amount, and the receipt was stored as one that does not reconcile.
        let mut two = vec![line(1, "1", None, None, 1), line(2, "1", None, None, 1)];
        for item in &mut two {
            item.final_price = most;
        }
        assert_eq!(
            reconcile(&receipt(two, 1000, 2199)),
            Err(MoneyError::Overflow)
        );
        // A subtotal at the end of the range and additions that pass it.
        let mut far = receipt(vec![line(1, "1", None, None, 1000)], 1000, 2199);
        far.totals.subtotal = most;
        assert_eq!(reconcile(&far), Err(MoneyError::Overflow));
        // A unit price whose product with the quantity is outside the range.
        let mut priced = vec![line(1, "2", Some((1, PricePer::Each)), None, 1000)];
        if let Some(unit) = priced.first_mut().and_then(|item| item.unit_price.as_mut()) {
            unit.amount = most;
        }
        assert_eq!(
            reconcile(&receipt(priced, 1000, 2199)),
            Err(MoneyError::Overflow)
        );
        // The control: one line of the largest amount, alone, is a receipt with its sums.
        let mut one = receipt(vec![line(1, "1", None, None, 1)], 1, 1);
        one.totals.subtotal = most;
        one.totals.total = most;
        one.totals.tax = None;
        one.totals.delivery_fee = None;
        one.totals.tip = None;
        if let Some(item) = one.lines.first_mut() {
            item.final_price = most;
        }
        assert!(reconcile(&one).expect("sums").reconciled());
    }

    #[test]
    fn a_receipt_in_two_currencies_gives_an_error_and_no_reconciliation() {
        use crate::domain::money::Currency;
        use rust_decimal::Decimal;

        let euro = Currency::parse("EUR").expect("a code");
        let euros = |cents: i64| Money::new(Decimal::new(cents, 2), euro).expect("an amount");
        let mismatch = |left, right| Err(MoneyError::CurrencyMismatch { left, right });
        // A line in euros on a receipt in dollars.
        let mut mixed = receipt(vec![line(1, "1", None, None, 1000)], 1000, 2199);
        if let Some(item) = mixed.lines.first_mut() {
            item.final_price = euros(1000);
        }
        assert_eq!(reconcile(&mixed), mismatch(Currency::USD, euro));
        // A tip in euros.
        let mut tipped = receipt(vec![line(1, "1", None, None, 1000)], 1000, 2199);
        tipped.totals.tip = Some(euros(500));
        assert_eq!(reconcile(&tipped), mismatch(Currency::USD, euro));
        // An options price in euros, and the unit price of a line with the special-price
        // flag: no check of the sums reads them, and the currency check does.
        let mut option = receipt(vec![line(1, "1", None, None, 1000)], 1000, 2199);
        if let Some(item) = option.lines.first_mut() {
            item.options_price = Some(euros(100));
        }
        assert_eq!(reconcile(&option), mismatch(Currency::USD, euro));
        let mut exempt = receipt(
            vec![line(1, "1", Some((1000, PricePer::Each)), None, 1000)],
            1000,
            2199,
        );
        if let Some(item) = exempt.lines.first_mut() {
            item.flags.special = true;
            if let Some(unit) = item.unit_price.as_mut() {
                unit.amount = euros(1000);
            }
        }
        assert_eq!(reconcile(&exempt), mismatch(Currency::USD, euro));
        // A receipt that is in euros throughout has its sums, and then meets the tolerance,
        // which is one cent of a dollar.
        let mut whole = receipt(vec![line(1, "1", None, None, 1000)], 1000, 2199);
        whole.totals.subtotal = euros(1000);
        whole.totals.total = euros(1000);
        whole.totals.tax = None;
        whole.totals.delivery_fee = None;
        whole.totals.tip = None;
        if let Some(item) = whole.lines.first_mut() {
            item.final_price = euros(1000);
        }
        assert_eq!(reconcile(&whole), mismatch(euro, Currency::USD));
    }
}
