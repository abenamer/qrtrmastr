//! The recommendation rules (REAS-ARCH-005): counts and observed gaps, nothing more. The
//! application loads the purchases and passes `as_of`; nothing here reads a clock.

use std::collections::{BTreeMap, BTreeSet};

use jiff::civil::Date;
use rust_decimal::Decimal;
use serde::Serialize;

use super::ids::{OrderNumber, ProductId};
use super::money::{Money, Quantity};

/// One line of one order, read through its alias to a product: the `purchases` view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Purchase {
    pub product: ProductId,
    pub order_number: OrderNumber,
    pub delivery_date: Date,
    pub quantity: Quantity,
    pub price: Money,
}

/// The knobs of the gap rule. They live in configuration so they can be tuned without a build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    /// Purchases needed before an item is recommended at all.
    pub min_purchases: usize,
    /// An item not bought for more than this many usual gaps is lapsed.
    pub lapsed_after_gaps: u32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            min_purchases: 3,
            lapsed_after_gaps: 3,
        }
    }
}

/// Where an item stands against its own history on the day of the question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Status {
    /// Bought once; nothing to say about a gap.
    BoughtOnce,
    /// Bought twice: one gap, a guess at best.
    Tentative,
    /// Not bought for more than `lapsed_after_gaps` usual gaps: probably dropped.
    Lapsed,
    /// The usual gap has passed.
    Overdue {
        days_overdue: i64,
    },
    /// The usual gap passes before the next order would arrive.
    DueSoon {
        days_until_due: i64,
    },
    NotDue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Recommendation {
    #[serde(rename = "product_id")]
    pub product: ProductId,
    pub times_bought: usize,
    pub usual_gap_days: Option<i64>,
    pub last_bought: Date,
    pub days_since: i64,
    pub typical_quantity: Quantity,
    pub last_price: Money,
    pub status: Status,
}

/// The median of integer days; the mean of the two middle values when the count is even.
pub fn median_days(values: &mut [i64]) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let mid = values.len() / 2;
    Some(if values.len() % 2 == 1 {
        values[mid]
    } else {
        (values[mid - 1] + values[mid]) / 2
    })
}

fn median_quantity(values: &mut [Quantity]) -> Option<Quantity> {
    if values.is_empty() {
        return None;
    }
    values.sort();
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        Some(values[mid])
    } else {
        let sum = values[mid - 1].as_decimal() + values[mid].as_decimal();
        Quantity::new((sum / Decimal::TWO).round_dp(3)).ok()
    }
}

/// Whole days from `from` to `to`; negative when `to` is the earlier day.
pub fn days_between(from: Date, to: Date) -> i64 {
    i64::from((to - from).get_days())
}

/// The usual gap between orders: the median of the gaps between distinct delivery dates.
pub fn order_cadence(delivery_dates: &[Date]) -> Option<i64> {
    let distinct: BTreeSet<Date> = delivery_dates.iter().copied().collect();
    let mut gaps: Vec<i64> = distinct
        .iter()
        .zip(distinct.iter().skip(1))
        .map(|(a, b)| days_between(*a, *b))
        .collect();
    median_days(&mut gaps)
}

/// The gap rule, for every product in `purchases`, as of `as_of`, with `cadence` days until the
/// next order is expected.
pub fn evaluate(
    purchases: &[Purchase],
    as_of: Date,
    cadence: i64,
    thresholds: Thresholds,
) -> Vec<Recommendation> {
    let mut by_product: BTreeMap<ProductId, Vec<&Purchase>> = BTreeMap::new();
    for purchase in purchases {
        by_product
            .entry(purchase.product)
            .or_default()
            .push(purchase);
    }

    let mut out = Vec::with_capacity(by_product.len());
    for (product, mut history) in by_product {
        history.sort_by_key(|p| p.delivery_date);
        let dates: BTreeSet<Date> = history.iter().map(|p| p.delivery_date).collect();
        let times_bought = dates.len();
        let last_bought = *dates
            .iter()
            .next_back()
            .expect("a product has at least one purchase");
        let days_since = days_between(last_bought, as_of);
        let mut gaps: Vec<i64> = dates
            .iter()
            .zip(dates.iter().skip(1))
            .map(|(a, b)| days_between(*a, *b))
            .collect();
        let usual_gap_days = median_days(&mut gaps);
        let mut quantities: Vec<Quantity> = history.iter().map(|p| p.quantity).collect();
        let typical_quantity = median_quantity(&mut quantities).expect("at least one quantity");
        let last_price = history.last().expect("at least one purchase").price;

        let status = match (times_bought, usual_gap_days) {
            (1, _) | (_, None) => Status::BoughtOnce,
            (n, Some(gap)) if n < thresholds.min_purchases => {
                if days_since > gap * i64::from(thresholds.lapsed_after_gaps) {
                    Status::Lapsed
                } else {
                    Status::Tentative
                }
            }
            (_, Some(gap)) => {
                if days_since > gap * i64::from(thresholds.lapsed_after_gaps) {
                    Status::Lapsed
                } else if days_since > gap {
                    Status::Overdue {
                        days_overdue: days_since - gap,
                    }
                } else if days_since + cadence > gap {
                    Status::DueSoon {
                        days_until_due: gap - days_since,
                    }
                } else {
                    Status::NotDue
                }
            }
        };

        out.push(Recommendation {
            product,
            times_bought,
            usual_gap_days,
            last_bought,
            days_since,
            typical_quantity,
            last_price,
            status,
        });
    }
    out
}

/// Orders recommendations for a list: overdue first by days overdue, then due soon by how soon,
/// then the rest by recency.
pub fn rank(recommendations: &mut [Recommendation]) {
    recommendations.sort_by(|a, b| {
        fn key(s: Status) -> (u8, i64) {
            match s {
                Status::Overdue { days_overdue } => (0, -days_overdue),
                Status::DueSoon { days_until_due } => (1, days_until_due),
                Status::Tentative => (2, 0),
                Status::NotDue => (3, 0),
                Status::Lapsed => (4, 0),
                Status::BoughtOnce => (5, 0),
            }
        }
        key(a.status)
            .cmp(&key(b.status))
            .then_with(|| b.days_since.cmp(&a.days_since))
    });
}

/// How many of the latest orders the habit rule ranks by, and how many break a tie.
pub const HABIT_RECENT_ORDERS: usize = 20;
pub const HABIT_LONG_ORDERS: usize = 50;

/// The habit rule: how often a product is in the latest orders. It is the default of `qm due`
/// because, on held-out orders, it names what the next order holds about twice as often as the
/// gap rule and as often as a fitted model (ADR 0009). No dates and no gaps: an order counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HabitRecommendation {
    #[serde(rename = "product_id")]
    pub product: ProductId,
    /// Of the latest `recent_orders` orders, how many hold the product.
    pub in_recent: usize,
    pub recent_orders: usize,
    /// The same over the latest `long_orders` orders.
    pub in_long: usize,
    pub long_orders: usize,
    pub last_bought: Date,
    pub typical_quantity: Quantity,
    pub last_price: Money,
}

/// Every product in at least one of the latest [`HABIT_RECENT_ORDERS`] orders, the most
/// habitual first: by its count in those orders, then in the latest [`HABIT_LONG_ORDERS`],
/// then by the later last purchase.
pub fn habit(purchases: &[Purchase]) -> Vec<HabitRecommendation> {
    let orders: BTreeMap<&OrderNumber, Date> = purchases
        .iter()
        .map(|p| (&p.order_number, p.delivery_date))
        .collect();
    let mut latest: Vec<(Date, &OrderNumber)> = orders
        .iter()
        .map(|(number, date)| (*date, *number))
        .collect();
    latest.sort();
    latest.reverse();
    let long: BTreeSet<&OrderNumber> = latest
        .iter()
        .take(HABIT_LONG_ORDERS)
        .map(|(_, number)| *number)
        .collect();
    let recent: BTreeSet<&OrderNumber> = latest
        .iter()
        .take(HABIT_RECENT_ORDERS)
        .map(|(_, number)| *number)
        .collect();

    let mut by_product: BTreeMap<ProductId, Vec<&Purchase>> = BTreeMap::new();
    for purchase in purchases.iter().filter(|p| long.contains(&p.order_number)) {
        by_product
            .entry(purchase.product)
            .or_default()
            .push(purchase);
    }

    let mut out: Vec<HabitRecommendation> = by_product
        .into_iter()
        .filter_map(|(product, mut history)| {
            history.sort_by_key(|p| p.delivery_date);
            let in_long: BTreeSet<&OrderNumber> = history.iter().map(|p| &p.order_number).collect();
            let in_recent = in_long.iter().filter(|n| recent.contains(*n)).count();
            let last = history.last()?;
            let mut quantities: Vec<Quantity> = history.iter().map(|p| p.quantity).collect();
            (in_recent > 0).then(|| HabitRecommendation {
                product,
                in_recent,
                recent_orders: recent.len(),
                in_long: in_long.len(),
                long_orders: long.len(),
                last_bought: last.delivery_date,
                typical_quantity: median_quantity(&mut quantities).expect("at least one"),
                last_price: last.price,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.in_recent
            .cmp(&a.in_recent)
            .then_with(|| b.in_long.cmp(&a.in_long))
            .then_with(|| b.last_bought.cmp(&a.last_bought))
            .then_with(|| a.product.cmp(&b.product))
    });
    out
}

/// The simple rule: products present in at least half of all orders and absent from the most
/// recent one, ranked by the share of orders they appear in. No dates involved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SimpleRecommendation {
    #[serde(rename = "product_id")]
    pub product: ProductId,
    pub orders_with_product: usize,
    pub orders_total: usize,
    pub typical_quantity: Quantity,
    pub last_price: Money,
}

pub fn simple(purchases: &[Purchase]) -> Vec<SimpleRecommendation> {
    let orders: BTreeMap<OrderNumber, Date> = purchases
        .iter()
        .map(|p| (p.order_number.clone(), p.delivery_date))
        .collect();
    let orders_total = orders.len();
    let Some((last_order, _)) = orders
        .iter()
        .max_by_key(|(number, date)| (**date, (*number).clone()))
    else {
        return Vec::new();
    };
    let last_order = last_order.clone();

    let mut by_product: BTreeMap<ProductId, Vec<&Purchase>> = BTreeMap::new();
    for purchase in purchases {
        by_product
            .entry(purchase.product)
            .or_default()
            .push(purchase);
    }

    let mut out: Vec<SimpleRecommendation> = by_product
        .into_iter()
        .filter_map(|(product, mut history)| {
            history.sort_by_key(|p| p.delivery_date);
            let orders_with_product: BTreeSet<&OrderNumber> =
                history.iter().map(|p| &p.order_number).collect();
            let in_last = orders_with_product.contains(&last_order);
            let count = orders_with_product.len();
            (!in_last && count * 2 >= orders_total && orders_total >= 2).then(|| {
                let mut quantities: Vec<Quantity> = history.iter().map(|p| p.quantity).collect();
                SimpleRecommendation {
                    product,
                    orders_with_product: count,
                    orders_total,
                    typical_quantity: median_quantity(&mut quantities).expect("at least one"),
                    last_price: history.last().expect("at least one").price,
                }
            })
        })
        .collect();
    out.sort_by_key(|r| std::cmp::Reverse(r.orders_with_product));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i16, m: i8, day: i8) -> Date {
        jiff::civil::date(y, m, day)
    }

    fn buy(product: i64, order: &str, date: Date, qty: &str) -> Purchase {
        Purchase {
            product: ProductId::new(product).expect("id"),
            order_number: OrderNumber::parse(order).expect("order"),
            delivery_date: date,
            quantity: Quantity::parse(qty).expect("qty"),
            price: Money::from_cents(579),
        }
    }

    #[test]
    fn the_order_cadence_is_the_median_gap_between_distinct_dates() {
        let dates = [
            d(2026, 1, 1),
            d(2026, 1, 8),
            d(2026, 1, 8),
            d(2026, 1, 22),
            d(2026, 1, 29),
        ];
        // gaps: 7, 14, 7 -> median 7
        assert_eq!(order_cadence(&dates), Some(7));
        assert_eq!(order_cadence(&[d(2026, 1, 1)]), None);
        assert_eq!(median_days(&mut [7, 14]), Some(10));
    }

    #[test]
    fn the_habit_rule_ranks_by_the_latest_orders_and_forgets_what_was_dropped() {
        // 60 weekly orders. Product 1 is in every order; product 2 in the first 35 only, so it
        // is outside the latest 20 and not listed; product 3 in the latest 5; product 4 in
        // every other order.
        let mut p = Vec::new();
        for week in 0..60u32 {
            let date = d(2025, 1, 1) + jiff::Span::new().days(i64::from(week) * 7);
            let order = format!("1{:010}", week);
            p.push(buy(1, &order, date, "2"));
            if week < 35 {
                p.push(buy(2, &order, date, "1"));
            }
            if week >= 55 {
                p.push(buy(3, &order, date, "1"));
            }
            if week % 2 == 0 {
                p.push(buy(4, &order, date, "3"));
            }
        }
        let r = habit(&p);
        let listed: Vec<(i64, usize, usize)> = r
            .iter()
            .map(|x| (x.product.get(), x.in_recent, x.in_long))
            .collect();
        assert_eq!(listed, [(1, 20, 50), (4, 10, 25), (3, 5, 5)]);
        assert_eq!((r[0].recent_orders, r[0].long_orders), (20, 50));
        assert_eq!(r[1].typical_quantity, Quantity::parse("3").expect("q"));
        assert!(habit(&[]).is_empty());
    }

    #[test]
    fn weekly_milk_is_overdue_after_its_usual_week() {
        let p = vec![
            buy(1, "10000000001", d(2026, 1, 1), "2"),
            buy(1, "10000000002", d(2026, 1, 8), "2"),
            buy(1, "10000000003", d(2026, 1, 15), "3"),
            buy(1, "10000000004", d(2026, 1, 22), "2"),
        ];
        let r = evaluate(&p, d(2026, 2, 1), 7, Thresholds::default());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].usual_gap_days, Some(7));
        assert_eq!(r[0].days_since, 10);
        assert_eq!(r[0].status, Status::Overdue { days_overdue: 3 });
        assert_eq!(r[0].typical_quantity, Quantity::parse("2").expect("qty"));
        assert_eq!(r[0].times_bought, 4);
    }

    #[test]
    fn biweekly_eggs_are_due_soon_when_the_next_order_would_pass_the_gap() {
        let p = vec![
            buy(2, "10000000001", d(2026, 1, 1), "1"),
            buy(2, "10000000003", d(2026, 1, 15), "1"),
            buy(2, "10000000005", d(2026, 1, 29), "1"),
        ];
        // 9 days since; gap 14; cadence 7: 9 + 7 > 14 -> due soon, 5 days until due.
        let r = evaluate(&p, d(2026, 2, 7), 7, Thresholds::default());
        assert_eq!(r[0].status, Status::DueSoon { days_until_due: 5 });
        // 3 days since: not due.
        let r = evaluate(&p, d(2026, 2, 1), 7, Thresholds::default());
        assert_eq!(r[0].status, Status::NotDue);
    }

    #[test]
    fn a_lapsed_item_and_a_one_off_are_told_apart_from_a_staple() {
        let lapsed = vec![
            buy(3, "10000000001", d(2025, 1, 1), "1"),
            buy(3, "10000000002", d(2025, 1, 8), "1"),
            buy(3, "10000000003", d(2025, 1, 15), "1"),
        ];
        let once = vec![buy(4, "10000000001", d(2025, 1, 1), "1")];
        let twice = vec![
            buy(5, "10000000001", d(2026, 1, 1), "1"),
            buy(5, "10000000002", d(2026, 1, 15), "1"),
        ];
        let all: Vec<Purchase> = lapsed.into_iter().chain(once).chain(twice).collect();
        let r = evaluate(&all, d(2026, 2, 1), 7, Thresholds::default());
        let status = |id: i64| {
            r.iter()
                .find(|x| x.product.get() == id)
                .expect("present")
                .status
        };
        assert_eq!(status(3), Status::Lapsed);
        assert_eq!(status(4), Status::BoughtOnce);
        assert_eq!(status(5), Status::Tentative);
    }

    #[test]
    fn ranking_puts_the_most_overdue_first() {
        let mut r = vec![
            Recommendation {
                product: ProductId::new(1).expect("id"),
                times_bought: 3,
                usual_gap_days: Some(7),
                last_bought: d(2026, 1, 1),
                days_since: 9,
                typical_quantity: Quantity::parse("1").expect("q"),
                last_price: Money::ZERO,
                status: Status::DueSoon { days_until_due: 2 },
            },
            Recommendation {
                product: ProductId::new(2).expect("id"),
                times_bought: 3,
                usual_gap_days: Some(7),
                last_bought: d(2026, 1, 1),
                days_since: 20,
                typical_quantity: Quantity::parse("1").expect("q"),
                last_price: Money::ZERO,
                status: Status::Overdue { days_overdue: 13 },
            },
            Recommendation {
                product: ProductId::new(3).expect("id"),
                times_bought: 3,
                usual_gap_days: Some(7),
                last_bought: d(2026, 1, 1),
                days_since: 10,
                typical_quantity: Quantity::parse("1").expect("q"),
                last_price: Money::ZERO,
                status: Status::Overdue { days_overdue: 3 },
            },
        ];
        rank(&mut r);
        let order: Vec<i64> = r.iter().map(|x| x.product.get()).collect();
        assert_eq!(order, vec![2, 3, 1]);
    }

    #[test]
    fn the_simple_rule_lists_frequent_items_missing_from_the_last_order() {
        let p = vec![
            buy(1, "10000000001", d(2026, 1, 1), "2"),
            buy(1, "10000000002", d(2026, 1, 8), "2"),
            buy(1, "10000000003", d(2026, 1, 15), "2"),
            buy(2, "10000000001", d(2026, 1, 1), "1"),
            buy(2, "10000000003", d(2026, 1, 15), "1"),
            buy(3, "10000000001", d(2026, 1, 1), "1"),
            buy(3, "10000000002", d(2026, 1, 8), "1"),
            buy(4, "10000000002", d(2026, 1, 8), "1"),
        ];
        // Last order is 10000000003. Product 1 is in it. Product 3 is in 2 of 3 orders and absent.
        // Product 2 is in it. Product 4 is in 1 of 3, below half.
        let r = simple(&p);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].product.get(), 3);
        assert_eq!((r[0].orders_with_product, r[0].orders_total), (2, 3));
    }
}
