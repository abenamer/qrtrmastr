//! The recommendation rules (REAS-ARCH-005): counts and observed gaps, nothing more. The
//! application loads the purchases and passes `as_of`; nothing here reads a clock.

use std::collections::{BTreeMap, BTreeSet};

use jiff::civil::Date;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use super::ids::{OrderNumber, ProductId};
use super::money::{Money, Quantity};
use super::pack::PackAmount;

/// One line of one order, read through its alias to a product: the `purchases` view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Purchase {
    /// The product that the alias of the line points at. After [`merge_variants`], the product
    /// that stands for every size of its name.
    pub product: ProductId,
    /// The order that the line is on. The rules count orders by it.
    pub order_number: OrderNumber,
    /// The delivery date of that order, not the date of the mail. The rules measure every gap
    /// in days between these dates.
    pub delivery_date: Date,
    /// The billed quantity of the line: the weight when the receipt gives the line one, else
    /// the second number of the quantity cell. [`merge_variants`] does not change it.
    pub quantity: Quantity,
    /// The final price of the line: what the receipt charged for the whole billed quantity,
    /// not a unit price.
    pub price: Money,
    /// How much the line brought, to compare one purchase with another: the billed quantity,
    /// or after [`merge_variants`] the quantity times the pack amount.
    pub units: Decimal,
}

/// One product as [`merge_variants`] sees it: its name and what one pack of it holds. `pack`
/// is `None` for a product sold by weight, whose quantity is the amount already, and for a
/// size text that is not understood.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Variant<'a> {
    /// The product that this size is stored as.
    pub product: ProductId,
    /// The display name of the product, without its size. Two products are sizes of one
    /// product only when their names are exactly the same.
    pub name: &'a str,
    /// What one pack of this size holds. `None` for a product sold by weight and for a size
    /// text that is not understood; the purchases of the name then keep their quantity as
    /// their units.
    pub pack: Option<PackAmount>,
}

/// Joins the sizes of one product (ADR 0010). Products with exactly the same name become the
/// one of them bought last, so a case and a small pack share one history. When every size of
/// a name is understood in one dimension, `units` is the quantity times the pack amount;
/// otherwise it stays the quantity. A product that no variant names is left as it is.
pub fn merge_variants(purchases: &[Purchase], variants: &[Variant<'_>]) -> Vec<Purchase> {
    let mut by_name: BTreeMap<&str, Vec<&Variant<'_>>> = BTreeMap::new();
    for variant in variants {
        by_name.entry(variant.name).or_default().push(variant);
    }
    let mut last_bought: BTreeMap<ProductId, Date> = BTreeMap::new();
    for purchase in purchases {
        let date = last_bought
            .entry(purchase.product)
            .or_insert(purchase.delivery_date);
        *date = (*date).max(purchase.delivery_date);
    }
    // product -> (the product that stands for its name, the amount of one pack)
    let mut merged: BTreeMap<ProductId, (ProductId, Decimal)> = BTreeMap::new();
    for group in by_name.values().filter(|group| group.len() > 1) {
        let Some(canonical) = group
            .iter()
            .filter_map(|v| last_bought.get(&v.product).map(|date| (*date, v.product)))
            .max()
            .map(|(_, product)| product)
        else {
            continue;
        };
        let first_dimension = group.first().and_then(|v| v.pack).map(|p| p.dimension);
        let comparable = group
            .iter()
            .all(|v| v.pack.map(|p| p.dimension) == first_dimension && v.pack.is_some());
        for variant in group {
            let amount = match variant.pack {
                Some(pack) if comparable => pack.amount,
                _ => Decimal::ONE,
            };
            merged.insert(variant.product, (canonical, amount));
        }
    }
    purchases
        .iter()
        .map(|purchase| match merged.get(&purchase.product) {
            Some((canonical, amount)) => Purchase {
                product: *canonical,
                // A pack amount has at most six digits and a quantity ten, so the product
                // fits; the quantity alone stands in if it ever did not.
                units: purchase
                    .quantity
                    .as_decimal()
                    .checked_mul(*amount)
                    .unwrap_or_else(|| purchase.quantity.as_decimal()),
                ..purchase.clone()
            },
            None => purchase.clone(),
        })
        .collect()
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Bought once; nothing to say about a gap.
    BoughtOnce,
    /// Bought on more than one day and on fewer days than `min_purchases`, three by default,
    /// and not lapsed: one gap or a few, a guess at best.
    Tentative,
    /// Not bought for more than `lapsed_after_gaps` usual gaps: probably dropped.
    Lapsed,
    /// The usual gap has passed.
    Overdue {
        /// The days since the last purchase minus the usual gap, in days. At least 1.
        days_overdue: i64,
    },
    /// The usual gap passes before the next order would arrive.
    DueSoon {
        /// The usual gap minus the days since the last purchase, in days. 0 means that the
        /// gap passes on the day of the question.
        days_until_due: i64,
    },
    /// The product has enough purchases, and its usual gap does not pass before the next order
    /// would arrive.
    NotDue,
}

/// One product under the gap rule: its history in numbers, and where it stands on the day of
/// the question. [`evaluate`] makes one for each product, and it is one row of `qm due --gap`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recommendation {
    /// The product that the purchases belong to.
    pub product: ProductId,
    /// On how many distinct delivery dates the product was bought. Two lines of one day count
    /// once. At least 1.
    pub times_bought: usize,
    /// The usual gap: the median number of days between consecutive purchase dates. `None`
    /// when the product was bought on one day only.
    pub usual_gap_days: Option<i64>,
    /// The latest delivery date that has the product.
    pub last_bought: Date,
    /// The days from `last_bought` to the day of the question. Negative when the day of the
    /// question is before the last purchase.
    pub days_since: i64,
    /// The typical quantity: the median billed quantity over all lines of the product. For
    /// an item sold by weight it is a weight in the unit of the receipt.
    pub typical_quantity: Quantity,
    /// The final price of the line of the last purchase: for its whole billed quantity, not a
    /// unit price.
    pub last_price: Money,
    /// Where the product stands on the day of the question.
    pub status: Status,
}

/// The middle of a sorted list: one value when the count is odd, the two values around the
/// middle when it is even, the lower one first.
enum Middle<T> {
    One(T),
    Two(T, T),
}

/// Finds the middle of a sorted list; `None` for an empty list.
///
/// The three medians of this module take their positions from here, so the position is
/// computed in one place and read with `get` (REAS-SEC-003).
fn middle<T: Copy>(sorted: &[T]) -> Option<Middle<T>> {
    let upper = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted.get(upper).copied().map(Middle::One)
    } else {
        let lower = upper.checked_sub(1)?;
        Some(Middle::Two(*sorted.get(lower)?, *sorted.get(upper)?))
    }
}

/// The median of integer days; the mean of the two middle values when the count is even.
///
/// `None` for an empty list, and for two middle values whose sum is outside an `i64`. Days
/// between two dates never reach that, so a caller reads `None` as "no gap is known".
pub fn median_days(values: &mut [i64]) -> Option<i64> {
    values.sort_unstable();
    match middle(values)? {
        Middle::One(value) => Some(value),
        Middle::Two(lower, upper) => Some(lower.checked_add(upper)? / 2),
    }
}

/// The median of quantities; the mean of the two middle values when the count is even,
/// rounded to three decimals. The mean is the lower value plus half the difference, which is
/// the same number as half the sum for every quantity. A quantity is below 10,000,000, so no
/// step of this can pass the range of the decimal type; the checked steps are there because
/// the lint for arithmetic asks for them.
fn median_quantity(values: &mut [Quantity]) -> Option<Quantity> {
    values.sort();
    match middle(values)? {
        Middle::One(value) => Some(value),
        Middle::Two(lower, upper) => {
            let half = upper
                .as_decimal()
                .checked_sub(lower.as_decimal())?
                .checked_div(Decimal::TWO)?;
            let mean = lower.as_decimal().checked_add(half)?;
            Quantity::new(mean.round_dp(3)).ok()
        }
    }
}

/// Whole days from `from` to `to`; negative when `to` is the earlier day.
pub fn days_between(from: Date, to: Date) -> i64 {
    // A civil date has no time zone, so each day between two dates is 24 hours.
    to.duration_since(from).as_hours() / 24
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
///
/// # Panics
///
/// Does not panic. The body has three `expect` calls. Each reads the purchases of one product,
/// and a product is in the map only because a purchase of it was put there: the list holds at
/// least one purchase, so it has a last date and a last price. It has a median quantity too,
/// because the mean of two positive quantities of at most three decimals is a quantity.
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
        #[expect(
            clippy::expect_used,
            reason = "a group of `by_product` is made by a push, so it holds at least one purchase"
        )]
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
        #[expect(
            clippy::expect_used,
            reason = "the list holds at least one quantity, and the mean of two positive quantities of three decimals is one too"
        )]
        let typical_quantity = median_quantity(&mut quantities).expect("at least one quantity");
        #[expect(
            clippy::expect_used,
            reason = "a group of `by_product` is made by a push, so it holds at least one purchase"
        )]
        let last_price = history.last().expect("at least one purchase").price;

        let status = match (times_bought, usual_gap_days) {
            (1, _) | (_, None) => Status::BoughtOnce,
            (n, Some(gap)) if n < thresholds.min_purchases => {
                if days_since > lapsed_after(gap, thresholds) {
                    Status::Lapsed
                } else {
                    Status::Tentative
                }
            }
            (_, Some(gap)) => {
                // The numbers are days between dates and two settings. A saturating step gives
                // the same comparison as an exact one, also for a setting at the bound of its type.
                if days_since > lapsed_after(gap, thresholds) {
                    Status::Lapsed
                } else if days_since > gap {
                    Status::Overdue {
                        days_overdue: days_since.saturating_sub(gap),
                    }
                } else if days_since.saturating_add(cadence) > gap {
                    Status::DueSoon {
                        days_until_due: gap.saturating_sub(days_since),
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

/// The days after which a product with this usual gap counts as lapsed.
fn lapsed_after(gap: i64, thresholds: Thresholds) -> i64 {
    gap.saturating_mul(i64::from(thresholds.lapsed_after_gaps))
}

/// Orders recommendations for a list: overdue first by days overdue, then due soon by how soon,
/// then the rest by recency.
pub fn rank(recommendations: &mut [Recommendation]) {
    recommendations.sort_by(|a, b| {
        fn key(s: Status) -> (u8, i64) {
            match s {
                Status::Overdue { days_overdue } => (0, days_overdue.saturating_neg()),
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

/// How many of the latest orders the habit rule ranks by.
pub const HABIT_RECENT_ORDERS: usize = 20;
/// How many of the latest orders the habit rule reads at all. The count of a product over
/// them breaks a tie, and its lines in them give its typical quantity.
pub const HABIT_LONG_ORDERS: usize = 50;
/// A last purchase of at least this many usual amounts is a stock: the product is not needed
/// until that many usual gaps have passed.
pub const STOCKED_RATIO: u32 = 3;

/// The habit rule: how often a product is in the latest orders. It is the default of `qm due`
/// because, on held-out orders, it names what the next order holds about twice as often as the
/// gap rule and as often as a fitted model (ADR 0009). No dates and no gaps: an order counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HabitRecommendation {
    /// The product. After [`merge_variants`] it stands for every size of its name.
    pub product: ProductId,
    /// Of the latest `recent_orders` orders, how many hold the product.
    pub in_recent: usize,
    /// How many orders `in_recent` is counted over: [`HABIT_RECENT_ORDERS`], or every order
    /// when the history holds fewer.
    pub recent_orders: usize,
    /// The same over the latest `long_orders` orders.
    pub in_long: usize,
    /// How many orders `in_long` is counted over: [`HABIT_LONG_ORDERS`], or every order when
    /// the history holds fewer.
    pub long_orders: usize,
    /// The delivery date of the last purchase of the product.
    pub last_bought: Date,
    /// The typical quantity: the median billed quantity over the lines of the product in the
    /// latest `long_orders` orders.
    pub typical_quantity: Quantity,
    /// The final price of the line of the last purchase: for its whole billed quantity, not a
    /// unit price.
    pub last_price: Money,
    /// Set when the last purchase was a stock ([`STOCKED_RATIO`]) that should last past
    /// `as_of`: the day it is expected to run out.
    pub stocked_until: Option<Date>,
}

impl HabitRecommendation {
    /// An essential is in at least a quarter of the latest orders; the other habitual
    /// products are extras (ADR 0011).
    pub fn is_essential(&self) -> bool {
        self.in_recent.saturating_mul(ESSENTIAL_ONE_IN) >= self.recent_orders
    }
}

/// A product is an essential when it is in at least one of this many of the latest orders.
pub const ESSENTIAL_ONE_IN: usize = 4;
/// A product can be a reminder when it is in at least this many orders of the whole history.
pub const REMEMBER_MIN_ORDERS: usize = 3;

/// A product bought before that left the latest orders: a question, not a prediction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reminder {
    /// The product to ask about.
    pub product: ProductId,
    /// How many orders of the whole history hold the product. At least
    /// [`REMEMBER_MIN_ORDERS`].
    pub orders_with_product: usize,
    /// The delivery date of the last purchase of the product.
    pub last_bought: Date,
    /// The final price of the line of the last purchase: for its whole billed quantity, not a
    /// unit price.
    pub last_price: Money,
}

/// One step of SplitMix64: spreads a seed and a product over the whole range, so that the
/// order of the reminders is a fixed shuffle for one seed. Not for secrets.
fn mix(seed: u64, product: ProductId) -> u64 {
    let mut z = seed
        .wrapping_add(product.get().unsigned_abs())
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// `count` products to ask "remember this?" about: in at least [`REMEMBER_MIN_ORDERS`] orders
/// of the history and in none of the latest [`HABIT_RECENT_ORDERS`]. Which ones is a shuffle
/// fixed by `seed`; the caller supplies it, so the same seed gives the same list.
pub fn remember(purchases: &[Purchase], seed: u64, count: usize) -> Vec<Reminder> {
    let orders: BTreeMap<&OrderNumber, Date> = purchases
        .iter()
        .map(|p| (&p.order_number, p.delivery_date))
        .collect();
    let mut latest: Vec<(Date, &OrderNumber)> = orders
        .iter()
        .map(|(number, date)| (*date, *number))
        .collect();
    latest.sort();
    let recent: BTreeSet<&OrderNumber> = latest
        .iter()
        .rev()
        .take(HABIT_RECENT_ORDERS)
        .map(|(_, number)| *number)
        .collect();

    let mut by_product: BTreeMap<ProductId, Vec<&Purchase>> = BTreeMap::new();
    for purchase in purchases {
        by_product
            .entry(purchase.product)
            .or_default()
            .push(purchase);
    }
    let mut out: Vec<Reminder> = by_product
        .into_iter()
        .filter_map(|(product, history)| {
            let in_orders: BTreeSet<&OrderNumber> =
                history.iter().map(|p| &p.order_number).collect();
            let last = history.iter().max_by_key(|p| p.delivery_date)?;
            let listed = in_orders.len() >= REMEMBER_MIN_ORDERS && in_orders.is_disjoint(&recent);
            listed.then_some(Reminder {
                product,
                orders_with_product: in_orders.len(),
                last_bought: last.delivery_date,
                last_price: last.price,
            })
        })
        .collect();
    out.sort_by_key(|r| (mix(seed, r.product), r.product));
    out.truncate(count);
    out
}

/// The day a bulk purchase is expected to last until, when the last purchase of `history`
/// (sorted by date) was one. The usual amount is the median over the earlier purchase days and
/// the usual gap the median between all of them; a product with fewer than three purchase days
/// has no usual amount. The amounts and the dates come from receipts: every step that could
/// overflow is checked, and a history that cannot be computed with is not a stock.
fn stocked_until(history: &[&Purchase]) -> Option<Date> {
    let mut by_day: BTreeMap<Date, Decimal> = BTreeMap::new();
    for purchase in history {
        let units = by_day.entry(purchase.delivery_date).or_default();
        *units = units.checked_add(purchase.units)?;
    }
    if by_day.len() < 3 {
        return None;
    }
    let (last_day, last_units) = by_day.iter().next_back().map(|(d, u)| (*d, *u))?;
    let mut earlier: Vec<Decimal> = by_day.values().rev().skip(1).copied().collect();
    earlier.sort();
    let usual = match middle(&earlier)? {
        Middle::One(units) => units,
        Middle::Two(lower, upper) => lower.checked_add(upper)?.checked_div(Decimal::TWO)?,
    };
    if usual <= Decimal::ZERO {
        return None;
    }
    let ratio = last_units.checked_div(usual)?;
    if ratio < Decimal::from(STOCKED_RATIO) {
        return None;
    }
    let mut gaps: Vec<i64> = by_day
        .keys()
        .zip(by_day.keys().skip(1))
        .map(|(a, b)| days_between(*a, *b))
        .collect();
    let gap = median_days(&mut gaps)?;
    let days = Decimal::from(gap).checked_mul(ratio)?.round().to_i64()?;
    last_day
        .checked_add(jiff::Span::new().try_days(days).ok()?)
        .ok()
}

/// Every product in at least one of the latest [`HABIT_RECENT_ORDERS`] orders, the most
/// habitual first: by its count in those orders, then in the latest [`HABIT_LONG_ORDERS`],
/// then by the later last purchase.
///
/// # Panics
///
/// Does not panic. The one `expect` in the body takes the median quantity of the purchases of
/// one product. A product is in the map only because a purchase of it was put there, so the
/// list holds at least one quantity, and the mean of two positive quantities of at most three
/// decimals is a quantity.
pub fn habit(purchases: &[Purchase], as_of: Date) -> Vec<HabitRecommendation> {
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
            #[expect(
                clippy::expect_used,
                reason = "the list holds at least one quantity, and the mean of two positive quantities of three decimals is one too"
            )]
            let typical_quantity = median_quantity(&mut quantities).expect("at least one");
            (in_recent > 0).then(|| HabitRecommendation {
                product,
                in_recent,
                recent_orders: recent.len(),
                in_long: in_long.len(),
                long_orders: long.len(),
                last_bought: last.delivery_date,
                typical_quantity,
                last_price: last.price,
                stocked_until: stocked_until(&history).filter(|day| *day > as_of),
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleRecommendation {
    /// The product: in at least half of the orders, and not in the most recent one.
    pub product: ProductId,
    /// How many orders hold the product. Twice this number is at least `orders_total`.
    pub orders_with_product: usize,
    /// How many orders the history holds. At least 2.
    pub orders_total: usize,
    /// The typical quantity: the median billed quantity over all lines of the product.
    pub typical_quantity: Quantity,
    /// The final price of the line of the last purchase: for its whole billed quantity, not a
    /// unit price.
    pub last_price: Money,
}

/// Applies the simple rule to `purchases`: one [`SimpleRecommendation`] for each product that
/// passes, the product in the most orders first. The most recent order is the one with the
/// latest delivery date; of two orders of that day, the one whose number sorts last as text. A
/// history of fewer than two orders gives an empty list.
///
/// # Panics
///
/// Does not panic. The two `expect` calls in the body read the purchases of one product. A
/// product is in the map only because a purchase of it was put there, so the list holds at
/// least one purchase and has a last price. It has a median quantity too, because the mean of
/// two positive quantities of at most three decimals is a quantity.
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
            (!in_last && count.saturating_mul(2) >= orders_total && orders_total >= 2).then(|| {
                let mut quantities: Vec<Quantity> = history.iter().map(|p| p.quantity).collect();
                #[expect(
                    clippy::expect_used,
                    reason = "the list holds at least one quantity, and the mean of two positive quantities of three decimals is one too"
                )]
                let typical_quantity = median_quantity(&mut quantities).expect("at least one");
                #[expect(
                    clippy::expect_used,
                    reason = "a group of `by_product` is made by a push, so it holds at least one purchase"
                )]
                let last_price = history.last().expect("at least one").price;
                SimpleRecommendation {
                    product,
                    orders_with_product: count,
                    orders_total,
                    typical_quantity,
                    last_price,
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
            units: Decimal::from_str_exact(qty).expect("qty"),
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
        let r = habit(&p, d(2026, 3, 1));
        let listed: Vec<(i64, usize, usize)> = r
            .iter()
            .map(|x| (x.product.get(), x.in_recent, x.in_long))
            .collect();
        assert_eq!(listed, [(1, 20, 50), (4, 10, 25), (3, 5, 5)]);
        assert_eq!((r[0].recent_orders, r[0].long_orders), (20, 50));
        assert_eq!(r[1].typical_quantity, Quantity::parse("3").expect("q"));
        assert!(habit(&[], d(2026, 3, 1)).is_empty());
        assert!(r.iter().all(|x| x.stocked_until.is_none()));
    }

    #[test]
    fn essentials_are_in_a_quarter_of_the_latest_orders_and_reminders_left_them() {
        // 40 weekly orders. Product 1 in all; product 2 in every fourth (5 of the latest 20);
        // product 3 in every fifth (4 of 20); products 10 to 19 in the first three orders
        // only; product 20 in the first two only.
        let mut p = Vec::new();
        for week in 0..40u32 {
            let date = d(2025, 1, 1) + jiff::Span::new().days(i64::from(week) * 7);
            let order = format!("1{:010}", week);
            p.push(buy(1, &order, date, "1"));
            if week % 4 == 0 {
                p.push(buy(2, &order, date, "1"));
            }
            if week % 5 == 0 {
                p.push(buy(3, &order, date, "1"));
            }
            if week < 3 {
                for product in 10..20 {
                    p.push(buy(product, &order, date, "1"));
                }
            }
            if week < 2 {
                p.push(buy(20, &order, date, "1"));
            }
        }
        let r = habit(&p, d(2026, 3, 1));
        let essential: Vec<(i64, usize, bool)> = r
            .iter()
            .map(|x| (x.product.get(), x.in_recent, x.is_essential()))
            .collect();
        assert_eq!(essential, [(1, 20, true), (2, 5, true), (3, 4, false)]);

        // The reminders are among products 10 to 19: bought three times, gone from the
        // latest 20 orders. Product 20 was bought twice only; 1 to 3 are still bought.
        let all = remember(&p, 1, 100);
        let mut ids: Vec<i64> = all.iter().map(|x| x.product.get()).collect();
        ids.sort_unstable();
        assert_eq!(ids, (10..20).collect::<Vec<i64>>());
        assert_eq!(all[0].orders_with_product, 3);
        assert_eq!(all[0].last_bought, d(2025, 1, 15));
        // The same seed gives the same list; another seed gives another order.
        let first = remember(&p, 1, 4);
        assert_eq!(first.len(), 4);
        assert_eq!(first, remember(&p, 1, 4));
        assert_eq!(first[..], all[..4]);
        assert!((2..40).any(|seed| remember(&p, seed, 4) != first));
        assert!(remember(&[], 1, 4).is_empty());
    }

    #[test]
    fn a_case_and_a_small_pack_are_one_product_and_the_case_is_a_stock() {
        use crate::domain::pack::pack_amount;
        // Product 1 is a pack of 3, bought two at a time every 14 days; product 2 is the same
        // name as a case of 36, bought once, last. Product 3 has another name.
        let mut p = Vec::new();
        for n in 0..5i64 {
            let date = d(2026, 1, 1) + jiff::Span::new().days(n * 14);
            p.push(buy(1, &format!("1{n:010}"), date, "2"));
            p.push(buy(3, &format!("1{n:010}"), date, "1"));
        }
        let case_day = d(2026, 3, 12);
        p.push(buy(2, "10000000009", case_day, "1"));
        let variants = [
            Variant {
                product: ProductId::new(1).expect("id"),
                name: "Seaweed Snack",
                pack: pack_amount("3ct, 0.17oz ea"),
            },
            Variant {
                product: ProductId::new(2).expect("id"),
                name: "Seaweed Snack",
                pack: pack_amount("36ct, 0.17oz ea"),
            },
            Variant {
                product: ProductId::new(3).expect("id"),
                name: "Milk",
                pack: pack_amount("1/2 gallon"),
            },
        ];
        let merged = merge_variants(&p, &variants);
        // The name is now the product bought last, the case; the units are pieces.
        assert!(merged.iter().all(|x| x.product.get() != 1));
        assert_eq!(merged[0].units, Decimal::from(6));
        assert_eq!(merged.last().expect("case").units, Decimal::from(36));
        // A product alone under its name keeps its quantity as units.
        assert_eq!(merged[1].units, Decimal::ONE);

        // 36 pieces against the usual 6: six usual gaps of 14 days from the case.
        let r = habit(&merged, d(2026, 3, 20));
        let seaweed = r.iter().find(|x| x.product.get() == 2).expect("listed");
        assert_eq!(seaweed.in_recent, 6);
        assert_eq!(seaweed.last_bought, case_day);
        assert_eq!(
            seaweed.stocked_until,
            Some(case_day + jiff::Span::new().days(84))
        );
        let milk = r.iter().find(|x| x.product.get() == 3).expect("listed");
        assert_eq!(milk.stocked_until, None);
        // After the stock has run out, the product is listed as before.
        let later = habit(&merged, d(2026, 7, 1));
        assert!(later.iter().all(|x| x.stocked_until.is_none()));

        // Sizes that do not compare are still one product, with the quantity as units.
        let odd = [
            Variant {
                pack: None,
                ..variants[0]
            },
            variants[1],
        ];
        let merged = merge_variants(&p, &odd);
        assert_eq!(merged.last().expect("case").units, Decimal::ONE);
        assert_eq!(merged.last().expect("case").product.get(), 2);
    }

    #[test]
    fn amounts_too_large_to_compute_with_are_not_a_stock_and_do_not_stop_the_rule() {
        // Four purchase days, two weeks apart, of one product. `units` is what a hostile
        // size text or quantity could make of it.
        let history = |units: [Decimal; 4]| -> Vec<Purchase> {
            units
                .iter()
                .enumerate()
                .map(|(n, units)| Purchase {
                    units: *units,
                    ..buy(
                        1,
                        &format!("1{n:010}"),
                        d(2026, 1, 1)
                            + jiff::Span::new().days(i64::try_from(n).expect("small") * 14),
                        "1",
                    )
                })
                .collect()
        };
        let tiny = Decimal::new(1, 10);
        for units in [
            // The sum of a day, the mean of two days, the ratio and the days all overflow.
            [Decimal::MAX, Decimal::MAX, Decimal::MAX, Decimal::MAX],
            [tiny, tiny, tiny, Decimal::MAX],
            [Decimal::ONE, Decimal::ONE, Decimal::ONE, Decimal::MAX],
        ] {
            let mut purchases = history(units);
            // A second line on the last day, so that the sum of that day overflows too.
            purchases.push(purchases[3].clone());
            let listed = habit(&purchases, d(2026, 3, 1));
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].stocked_until, None);
        }
        // An amount that is large and still computes is a stock, far ahead or not at all,
        // never a panic: a million usual amounts, two weeks each.
        let far = history([
            Decimal::ONE,
            Decimal::ONE,
            Decimal::ONE,
            Decimal::from(1_000_000),
        ]);
        assert_eq!(habit(&far, d(2026, 3, 1))[0].stocked_until, None);
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
    fn a_lapsed_item_and_a_one_off_are_told_apart_from_an_item_bought_often() {
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

    #[test]
    fn a_median_reads_the_middle_of_a_list_of_any_length() {
        assert_eq!(median_days(&mut []), None);
        assert_eq!(median_days(&mut [5]), Some(5));
        assert_eq!(median_days(&mut [9, 1, 5]), Some(5));
        assert_eq!(median_days(&mut [21, 7, 14, 7]), Some(10));
        // The mean of two negative numbers rounds toward zero, as the division always did.
        assert_eq!(median_days(&mut [-7, -14]), Some(-10));
        assert_eq!(median_days(&mut [-1, -2]), Some(-1));
        assert_eq!(median_days(&mut [i64::MAX, i64::MIN]), Some(0));

        let q = |text: &str| Quantity::parse(text).expect("qty");
        assert_eq!(median_quantity(&mut []), None);
        assert_eq!(median_quantity(&mut [q("2")]), Some(q("2")));
        assert_eq!(median_quantity(&mut [q("3"), q("2")]), Some(q("2.5")));
        assert_eq!(median_quantity(&mut [q("4"), q("1"), q("2")]), Some(q("2")));
        // The mean of two amounts of three decimals can have four: it rounds to the even digit.
        assert_eq!(
            median_quantity(&mut [q("0.001"), q("0.002")]),
            Some(q("0.002"))
        );
        assert_eq!(
            median_quantity(&mut [q("0.002"), q("0.003")]),
            Some(q("0.002"))
        );
    }

    #[test]
    fn the_days_between_two_dates_are_the_same_as_their_difference() {
        assert_eq!(days_between(d(2026, 3, 1), d(2026, 3, 1)), 0);
        assert_eq!(days_between(d(2026, 2, 28), d(2026, 3, 1)), 1);
        assert_eq!(days_between(d(2024, 2, 28), d(2024, 3, 1)), 2);
        assert_eq!(days_between(d(2026, 3, 1), d(2026, 2, 22)), -7);
        // The widest span that the date type holds, in both directions.
        assert_eq!(days_between(d(-9999, 1, 1), d(9999, 12, 31)), 7_304_483);
        assert_eq!(days_between(d(9999, 12, 31), d(-9999, 1, 1)), -7_304_483);
        // The reference is the subtraction of the date library, which the function used before.
        let dates = [
            d(-9999, 1, 1),
            d(1, 1, 1),
            d(1999, 12, 31),
            d(2024, 2, 29),
            d(2026, 3, 8),
            d(2026, 11, 1),
            d(9999, 12, 31),
        ];
        for from in dates {
            for to in dates {
                assert_eq!(
                    days_between(from, to),
                    i64::from((to - from).get_days()),
                    "{from} {to}"
                );
            }
        }
    }

    #[test]
    fn a_sum_outside_the_range_of_the_type_gives_no_median_or_the_mean_and_no_panic() {
        assert_eq!(median_days(&mut [i64::MAX, i64::MAX]), None);
        assert_eq!(median_days(&mut [i64::MIN, i64::MIN, -1, 0]), None);
        // The two largest quantities: their sum is no quantity, and their mean is one. Until
        // 2026-11-21 this test used numbers at the bound of the decimal type, which are no
        // quantities now.
        let q = |text: &str| Quantity::parse(text).expect("qty");
        let large = q("9999999.999");
        let below = q("9999999.997");
        assert_eq!(median_quantity(&mut [large, below]), Some(q("9999999.998")));
        assert_eq!(median_quantity(&mut [large, large]), Some(large));
    }

    #[test]
    fn a_setting_at_the_bound_of_its_type_changes_a_status_and_does_not_stop_the_rule() {
        let p = vec![
            buy(1, "10000000001", d(2026, 1, 1), "1"),
            buy(1, "10000000002", d(2026, 1, 8), "1"),
            buy(1, "10000000003", d(2026, 1, 15), "1"),
        ];
        let never_lapsed = Thresholds {
            lapsed_after_gaps: u32::MAX,
            ..Thresholds::default()
        };
        // The usual gap is 7 days and the last purchase was 3 days ago.
        let soon = evaluate(&p, d(2026, 1, 18), i64::MAX, never_lapsed);
        assert_eq!(soon[0].status, Status::DueSoon { days_until_due: 4 });
        let not_due = evaluate(&p, d(2026, 1, 18), i64::MIN, never_lapsed);
        assert_eq!(not_due[0].status, Status::NotDue);
        // The last day that the date type holds.
        let late = evaluate(&p, d(9999, 12, 31), 7, never_lapsed);
        assert_eq!(
            late[0].status,
            Status::Overdue {
                days_overdue: 2_912_428 - 7
            }
        );
        let mut ranked = late;
        ranked.push(Recommendation {
            status: Status::Overdue {
                days_overdue: i64::MIN,
            },
            ..ranked[0].clone()
        });
        rank(&mut ranked);
        assert_eq!(
            ranked[0].status,
            Status::Overdue {
                days_overdue: 2_912_428 - 7
            }
        );
    }
}
