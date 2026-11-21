//! The wire types of `qm due --json` (REAS-DATA-001): what the JSON holds, under which names
//! and in which order. The domain and the application layer carry no serialization
//! attribute; a report becomes one of these types here, at the sink, and `json_text` writes
//! it.
//!
//! Each `From` implementation takes the report apart field by field, without `..`. A field
//! that is added to a report or to a recommendation therefore does not compile until this
//! module says whether the JSON carries it.
//!
//! The JSON is an interface: a script may read it. A name, the order of the fields, and the
//! text form of a date, an amount and a quantity do not change without a line in the
//! changelog. The files under `tests/fixtures/synthetic/json/` hold the text for five reports
//! as the code wrote it before these types existed, and the tests below compare with them.

use jiff::civil::Date;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::application::ports::ProductLabel;
use crate::application::recommend::{
    DueReport, DueRow, HabitReport, HabitRow, ReminderRow, SimpleReport, SimpleRow,
};
use crate::domain::list::Addition;
use crate::domain::recommend::{
    HabitRecommendation, Recommendation, Reminder, SimpleRecommendation, Status,
};

/// How a product is shown, under the name `product` in each row.
#[derive(Debug, Serialize)]
pub struct ProductLabelJson<'a> {
    /// The number of the product, `products.id`.
    id: i64,
    /// The stored name, without the size.
    display_name: &'a str,
    /// The size as printed; `null` when the printed name had none.
    size_text: Option<&'a str>,
    /// `each` or `weight`: the stored text of the label.
    sold_by: &'static str,
    /// The department; `null` when the receipt named none.
    department: Option<&'a str>,
    /// The name, with its size, as the latest receipt printed it.
    latest_raw_name: &'a str,
}

impl<'a> From<&'a ProductLabel> for ProductLabelJson<'a> {
    fn from(label: &'a ProductLabel) -> Self {
        let ProductLabel {
            id,
            display_name,
            size_text,
            sold_by,
            department,
            latest_raw_name,
        } = label;
        Self {
            id: id.get(),
            display_name,
            size_text: size_text.as_deref(),
            sold_by: sold_by.as_str(),
            department: department.as_deref(),
            latest_raw_name,
        }
    }
}

/// Where a product stands under the gap rule. The JSON holds it as an object under the name
/// `status` whose own field `status` is the word, so an overdue row reads
/// `"status": { "status": "overdue", "days_overdue": 3 }`. The shape is older than this
/// module and is kept.
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum StatusJson {
    /// Bought on one day only.
    BoughtOnce,
    /// Bought on too few days for a usual gap that the rule trusts.
    Tentative,
    /// Not bought for several usual gaps.
    Lapsed,
    /// The usual gap has passed.
    Overdue {
        /// By how many days.
        days_overdue: i64,
    },
    /// The usual gap passes before the next order is expected.
    DueSoon {
        /// In how many days; 0 is the day of the question.
        days_until_due: i64,
    },
    /// The usual gap does not pass before the next order is expected.
    NotDue,
}

impl From<Status> for StatusJson {
    fn from(status: Status) -> Self {
        match status {
            Status::BoughtOnce => Self::BoughtOnce,
            Status::Tentative => Self::Tentative,
            Status::Lapsed => Self::Lapsed,
            Status::Overdue { days_overdue } => Self::Overdue { days_overdue },
            Status::DueSoon { days_until_due } => Self::DueSoon { days_until_due },
            Status::NotDue => Self::NotDue,
        }
    }
}

/// One row of `qm due --gap --json`: the label, then the numbers of the gap rule beside it.
#[derive(Debug, Serialize)]
pub struct DueRowJson<'a> {
    /// The label of the product.
    product: ProductLabelJson<'a>,
    /// The number of the product that the purchases belong to.
    product_id: i64,
    /// On how many distinct delivery dates the product was bought.
    times_bought: usize,
    /// The usual gap in days; `null` for a product bought on one day only.
    usual_gap_days: Option<i64>,
    /// The latest delivery date that has the product, as `YYYY-MM-DD`.
    last_bought: Date,
    /// The days from `last_bought` to the day of the question.
    days_since: i64,
    /// The typical quantity, as decimal text without trailing zeros.
    typical_quantity: Decimal,
    /// The final price of the line of the last purchase, in dollars, as decimal text: two
    /// decimals for a price that a receipt or the store gave, and `0` for no money. Text and
    /// not a number, so that no reader makes a float of it.
    last_price: Decimal,
    /// Where the product stands.
    status: StatusJson,
}

impl<'a> From<&'a DueRow> for DueRowJson<'a> {
    fn from(row: &'a DueRow) -> Self {
        let DueRow {
            product: label,
            recommendation,
        } = row;
        let Recommendation {
            product,
            times_bought,
            usual_gap_days,
            last_bought,
            days_since,
            typical_quantity,
            last_price,
            status,
        } = recommendation;
        Self {
            product: label.into(),
            product_id: product.get(),
            times_bought: *times_bought,
            usual_gap_days: *usual_gap_days,
            last_bought: *last_bought,
            days_since: *days_since,
            typical_quantity: typical_quantity.as_decimal(),
            last_price: last_price.as_decimal(),
            status: (*status).into(),
        }
    }
}

/// The report of the gap rule, `qm due --gap --json`.
#[derive(Debug, Serialize)]
pub struct DueReportJson<'a> {
    /// The day the rule judges from, as `YYYY-MM-DD`.
    as_of: Date,
    /// How many orders are stored.
    orders: usize,
    /// The order cadence in days; `null` when fewer than two delivery dates are stored.
    order_cadence_days: Option<i64>,
    /// The products to list, in the order of the table.
    rows: Vec<DueRowJson<'a>>,
}

impl<'a> From<&'a DueReport> for DueReportJson<'a> {
    fn from(report: &'a DueReport) -> Self {
        let DueReport {
            as_of,
            orders,
            order_cadence_days,
            rows,
        } = report;
        Self {
            as_of: *as_of,
            orders: *orders,
            order_cadence_days: *order_cadence_days,
            rows: rows.iter().map(DueRowJson::from).collect(),
        }
    }
}

/// One row of `qm due --simple --json`: the label, then the counts of the simple rule.
#[derive(Debug, Serialize)]
pub struct SimpleRowJson<'a> {
    /// The label of the product.
    product: ProductLabelJson<'a>,
    /// The number of the product.
    product_id: i64,
    /// How many orders hold the product.
    orders_with_product: usize,
    /// How many orders the history holds.
    orders_total: usize,
    /// The typical quantity, as decimal text without trailing zeros.
    typical_quantity: Decimal,
    /// The final price of the line of the last purchase, as decimal text; see
    /// [`DueRowJson`] for its form.
    last_price: Decimal,
}

impl<'a> From<&'a SimpleRow> for SimpleRowJson<'a> {
    fn from(row: &'a SimpleRow) -> Self {
        let SimpleRow {
            product: label,
            recommendation,
        } = row;
        let SimpleRecommendation {
            product,
            orders_with_product,
            orders_total,
            typical_quantity,
            last_price,
        } = recommendation;
        Self {
            product: label.into(),
            product_id: product.get(),
            orders_with_product: *orders_with_product,
            orders_total: *orders_total,
            typical_quantity: typical_quantity.as_decimal(),
            last_price: last_price.as_decimal(),
        }
    }
}

/// The report of the simple rule, `qm due --simple --json`.
#[derive(Debug, Serialize)]
pub struct SimpleReportJson<'a> {
    /// How many orders are stored.
    orders: usize,
    /// The products to list, the product in the most orders first.
    rows: Vec<SimpleRowJson<'a>>,
}

impl<'a> From<&'a SimpleReport> for SimpleReportJson<'a> {
    fn from(report: &'a SimpleReport) -> Self {
        let SimpleReport { orders, rows } = report;
        Self {
            orders: *orders,
            rows: rows.iter().map(SimpleRowJson::from).collect(),
        }
    }
}

/// One row of the essentials, the extras or the stocked products in `qm due --json`: the
/// label, then the counts of the habit rule.
#[derive(Debug, Serialize)]
pub struct HabitRowJson<'a> {
    /// The label of the product.
    product: ProductLabelJson<'a>,
    /// The number of the product; it stands for every size of its name.
    product_id: i64,
    /// Of the latest `recent_orders` orders, how many hold the product.
    in_recent: usize,
    /// How many orders `in_recent` is counted over.
    recent_orders: usize,
    /// Of the latest `long_orders` orders, how many hold the product.
    in_long: usize,
    /// How many orders `in_long` is counted over.
    long_orders: usize,
    /// The delivery date of the last purchase, as `YYYY-MM-DD`.
    last_bought: Date,
    /// The typical quantity, as decimal text without trailing zeros.
    typical_quantity: Decimal,
    /// The final price of the line of the last purchase, as decimal text; see
    /// [`DueRowJson`] for its form.
    last_price: Decimal,
    /// The day a stock is expected to run out; `null` when the last purchase was no stock.
    stocked_until: Option<Date>,
}

impl<'a> From<&'a HabitRow> for HabitRowJson<'a> {
    fn from(row: &'a HabitRow) -> Self {
        let HabitRow {
            product: label,
            recommendation,
        } = row;
        let HabitRecommendation {
            product,
            in_recent,
            recent_orders,
            in_long,
            long_orders,
            last_bought,
            typical_quantity,
            last_price,
            stocked_until,
        } = recommendation;
        Self {
            product: label.into(),
            product_id: product.get(),
            in_recent: *in_recent,
            recent_orders: *recent_orders,
            in_long: *in_long,
            long_orders: *long_orders,
            last_bought: *last_bought,
            typical_quantity: typical_quantity.as_decimal(),
            last_price: last_price.as_decimal(),
            stocked_until: *stocked_until,
        }
    }
}

/// One row of "remember this?" in `qm due --json`: the label, then the reminder.
#[derive(Debug, Serialize)]
pub struct ReminderRowJson<'a> {
    /// The label of the product.
    product: ProductLabelJson<'a>,
    /// The number of the product.
    product_id: i64,
    /// How many orders of the whole history hold the product.
    orders_with_product: usize,
    /// The delivery date of the last purchase, as `YYYY-MM-DD`.
    last_bought: Date,
    /// The final price of the line of the last purchase, as decimal text; see
    /// [`DueRowJson`] for its form.
    last_price: Decimal,
}

impl<'a> From<&'a ReminderRow> for ReminderRowJson<'a> {
    fn from(row: &'a ReminderRow) -> Self {
        let ReminderRow {
            product: label,
            reminder,
        } = row;
        let Reminder {
            product,
            orders_with_product,
            last_bought,
            last_price,
        } = reminder;
        Self {
            product: label.into(),
            product_id: product.get(),
            orders_with_product: *orders_with_product,
            last_bought: *last_bought,
            last_price: last_price.as_decimal(),
        }
    }
}

/// One addition in `qm due --json`.
#[derive(Debug, Serialize)]
pub struct AdditionJson<'a> {
    /// The number of the addition: what `qm drop` takes.
    id: i64,
    /// What the user typed for the item.
    item: &'a str,
    /// The number of the product that the item named; `null` when it named none or several.
    product_id: Option<i64>,
    /// Why the user added the item: the stored text of the reason, such as `running-low`.
    reason: &'static str,
    /// The day the user added the item, as `YYYY-MM-DD`.
    added_on: Date,
}

impl<'a> From<&'a Addition> for AdditionJson<'a> {
    fn from(addition: &'a Addition) -> Self {
        let Addition {
            id,
            item,
            product,
            reason,
            added_on,
        } = addition;
        Self {
            id: id.get(),
            item: item.as_str(),
            product_id: product.map(|product| product.get()),
            reason: reason.as_str(),
            added_on: *added_on,
        }
    }
}

/// The report of the habit rule, `qm due --json`.
#[derive(Debug, Serialize)]
pub struct HabitReportJson<'a> {
    /// The day a stock is judged from, as `YYYY-MM-DD`.
    as_of: Date,
    /// How many orders are stored.
    orders: usize,
    /// The core of the next order.
    essentials: Vec<HabitRowJson<'a>>,
    /// The next most habitual products.
    extras: Vec<HabitRowJson<'a>>,
    /// Bought before and gone from the latest orders.
    remember: Vec<ReminderRowJson<'a>>,
    /// The seed that picked `remember`.
    seed: u64,
    /// What the user added by hand since the latest order.
    additions: Vec<AdditionJson<'a>>,
    /// Habitual products left out because the last purchase was a stock.
    stocked: Vec<HabitRowJson<'a>>,
}

impl<'a> From<&'a HabitReport> for HabitReportJson<'a> {
    fn from(report: &'a HabitReport) -> Self {
        let HabitReport {
            as_of,
            orders,
            essentials,
            extras,
            remember,
            seed,
            additions,
            stocked,
        } = report;
        let rows = |rows: &'a [HabitRow]| rows.iter().map(HabitRowJson::from).collect();
        Self {
            as_of: *as_of,
            orders: *orders,
            essentials: rows(essentials),
            extras: rows(extras),
            remember: remember.iter().map(ReminderRowJson::from).collect(),
            seed: *seed,
            additions: additions.iter().map(AdditionJson::from).collect(),
            stocked: rows(stocked),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use jiff::civil::date;
    use rust_decimal::Decimal;

    use super::{DueReportJson, HabitReportJson, SimpleReportJson};
    use crate::adapters::cli::json_text;
    use crate::application::ports::ProductLabel;
    use crate::application::recommend::{
        DueReport, DueRow, HabitReport, HabitRow, ReminderRow, SimpleReport, SimpleRow,
    };
    use crate::domain::ids::{AdditionId, ProductId};
    use crate::domain::list::{Addition, ItemText};
    use crate::domain::money::{Money, Quantity};
    use crate::domain::recommend::{
        HabitRecommendation, Recommendation, Reminder, SimpleRecommendation, Status,
    };
    use crate::domain::states::{AddReason, SoldBy};

    fn product(id: i64) -> ProductId {
        ProductId::new(id).expect("a positive id")
    }

    fn quantity(text: &str) -> Quantity {
        Quantity::new(Decimal::from_str(text).expect("a decimal")).expect("a quantity")
    }

    /// A label with every optional field set.
    fn full_label(id: i64) -> ProductLabel {
        ProductLabel {
            id: product(id),
            display_name: "Organic Whole Milk".to_owned(),
            size_text: Some("1/2 gallon".to_owned()),
            sold_by: SoldBy::Each,
            department: Some("Dairy".to_owned()),
            latest_raw_name: "Organic Whole Milk - (1/2 gallon)".to_owned(),
        }
    }

    /// A label with no optional field set, for a product sold by weight.
    fn bare_label(id: i64) -> ProductLabel {
        ProductLabel {
            id: product(id),
            display_name: "Bananas \"ripe\" & <green>".to_owned(),
            size_text: None,
            sold_by: SoldBy::Weight,
            department: None,
            latest_raw_name: "Bananas \"ripe\" & <green>".to_owned(),
        }
    }

    /// The number of the product in the numbers of a row. In the samples it is never the
    /// number in the label of the row, so that a wire type that takes one for the other
    /// writes a different text.
    fn counted(label: &ProductLabel) -> ProductId {
        product(label.id.get().saturating_add(100))
    }

    fn habit_row(label: ProductLabel, stocked_until: Option<jiff::civil::Date>) -> HabitRow {
        HabitRow {
            recommendation: HabitRecommendation {
                product: counted(&label),
                in_recent: 17,
                recent_orders: 20,
                in_long: 41,
                long_orders: 50,
                last_bought: date(2026, 2, 24),
                typical_quantity: quantity("1.560"),
                last_price: Money::from_cents(1737),
                stocked_until,
            },
            product: label,
        }
    }

    fn habit_report() -> HabitReport {
        let reasons = [
            (AddReason::OutOfStock, Some(product(7))),
            (AddReason::RunningLow, None),
            (AddReason::Recipe, Some(product(i64::MAX))),
            (AddReason::NewFood, None),
            (AddReason::RepeatFavorite, None),
        ];
        HabitReport {
            as_of: date(2026, 3, 3),
            orders: 510,
            essentials: vec![
                habit_row(full_label(7), None),
                habit_row(bare_label(8), None),
            ],
            extras: vec![habit_row(bare_label(9), None)],
            remember: vec![ReminderRow {
                product: full_label(10),
                reminder: Reminder {
                    product: product(110),
                    orders_with_product: 4,
                    last_bought: date(2025, 12, 31),
                    last_price: Money::from_cents(-200),
                },
            }],
            seed: u64::MAX,
            additions: (1_i64..)
                .zip(reasons)
                .map(|(number, (reason, product))| Addition {
                    id: AdditionId::new(number).expect("a positive id"),
                    item: ItemText::parse("miso  paste").expect("an item"),
                    product,
                    reason,
                    added_on: date(2026, 3, 1),
                })
                .collect(),
            stocked: vec![habit_row(full_label(11), Some(date(2026, 4, 15)))],
        }
    }

    fn empty_habit_report() -> HabitReport {
        HabitReport {
            as_of: date(1, 1, 1),
            orders: 0,
            essentials: Vec::new(),
            extras: Vec::new(),
            remember: Vec::new(),
            seed: 0,
            additions: Vec::new(),
            stocked: Vec::new(),
        }
    }

    fn simple_report() -> SimpleReport {
        SimpleReport {
            orders: 12,
            rows: vec![
                SimpleRow {
                    product: full_label(7),
                    recommendation: SimpleRecommendation {
                        product: product(107),
                        orders_with_product: 9,
                        orders_total: 12,
                        typical_quantity: quantity("2"),
                        last_price: Money::ZERO,
                    },
                },
                SimpleRow {
                    product: bare_label(8),
                    recommendation: SimpleRecommendation {
                        product: product(108),
                        orders_with_product: 6,
                        orders_total: 12,
                        typical_quantity: quantity("0.125"),
                        last_price: Money::from_cents(99_999_999),
                    },
                },
            ],
        }
    }

    fn due_report() -> DueReport {
        let statuses = [
            Status::BoughtOnce,
            Status::Tentative,
            Status::Lapsed,
            Status::Overdue { days_overdue: 3 },
            Status::DueSoon { days_until_due: 0 },
            Status::NotDue,
        ];
        DueReport {
            as_of: date(2026, 3, 3),
            orders: 510,
            order_cadence_days: Some(7),
            rows: (1_i64..)
                .zip(statuses)
                .map(|(id, status)| DueRow {
                    product: if id % 2 == 0 {
                        full_label(id)
                    } else {
                        bare_label(id)
                    },
                    recommendation: Recommendation {
                        product: product(id.saturating_add(100)),
                        times_bought: 12,
                        usual_gap_days: if id == 1 { None } else { Some(14) },
                        last_bought: date(2026, 2, 10),
                        days_since: if id == 2 { -4 } else { 21 },
                        typical_quantity: quantity("1.5"),
                        last_price: Money::from_cents(450),
                        status,
                    },
                })
                .collect(),
        }
    }

    fn empty_due_report() -> DueReport {
        DueReport {
            as_of: date(2026, 3, 3),
            orders: 1,
            order_cadence_days: None,
            rows: Vec::new(),
        }
    }

    /// The text that the code before the wire types wrote for the same report. The file ends
    /// with one line feed, as `println!` ends the output.
    macro_rules! before {
        ($name:literal) => {
            include_str!(concat!(
                "../../../tests/fixtures/synthetic/json/due-",
                $name,
                ".json"
            ))
        };
    }

    #[test]
    fn the_habit_report_is_the_json_it_was_before_the_wire_types() {
        let report = habit_report();
        let json = json_text(&HabitReportJson::from(&report)).expect("json");
        assert_eq!(json + "\n", before!("habit"));
        let report = empty_habit_report();
        let json = json_text(&HabitReportJson::from(&report)).expect("json");
        assert_eq!(json + "\n", before!("habit-empty"));
    }

    #[test]
    fn the_simple_report_is_the_json_it_was_before_the_wire_types() {
        let report = simple_report();
        let json = json_text(&SimpleReportJson::from(&report)).expect("json");
        assert_eq!(json + "\n", before!("simple"));
    }

    #[test]
    fn the_gap_report_is_the_json_it_was_before_the_wire_types() {
        let report = due_report();
        let json = json_text(&DueReportJson::from(&report)).expect("json");
        assert_eq!(json + "\n", before!("gap"));
        let report = empty_due_report();
        let json = json_text(&DueReportJson::from(&report)).expect("json");
        assert_eq!(json + "\n", before!("gap-empty"));
    }

    #[test]
    fn the_sample_reports_hold_every_value_of_each_label_and_status() {
        // A positive control for the three tests above: a variant that no sample holds would
        // be compared with nothing.
        let habit = before!("habit");
        for reason in AddReason::ALL {
            assert!(
                habit.contains(&format!("\"reason\": \"{reason}\"")),
                "{reason}"
            );
        }
        for sold_by in SoldBy::ALL {
            assert!(
                habit.contains(&format!("\"sold_by\": \"{sold_by}\"")),
                "{sold_by}"
            );
        }
        let due = before!("gap");
        for status in [
            "bought_once",
            "tentative",
            "lapsed",
            "overdue",
            "due_soon",
            "not_due",
        ] {
            assert!(
                due.contains(&format!("\"status\": \"{status}\"")),
                "{status}"
            );
        }
        assert_eq!(due.matches("\"status\": {").count(), 6);
    }
}
