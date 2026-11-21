//! `qm due`: what the history says you are about to run out of.

use jiff::civil::Date;
use serde::Serialize;

use super::ports::{Catalog, GroceryStore, ProductLabel, ShoppingList, StoreError};
use crate::domain::list::Addition;
use crate::domain::pack::pack_amount;
use crate::domain::recommend::{
    HabitRecommendation, Recommendation, Reminder, SimpleRecommendation, Status, Thresholds,
    Variant, evaluate, habit, merge_variants, order_cadence, rank, remember, simple,
};
use crate::domain::states::SoldBy;

/// Which items the list shows beyond the ones that are due.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DueOptions {
    /// Also list a product bought on more than one day and on fewer days than
    /// `min_purchases`: its usual gap comes from too few gaps to trust.
    pub include_tentative: bool,
    /// Also list a product not bought for more than `lapsed_after_gaps` times its usual gap.
    pub include_lapsed: bool,
    /// Also list a product whose usual gap does not pass before the next order is expected.
    pub include_not_due: bool,
}

/// One row of `qm due`: the recommendation and how to show the product.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DueRow {
    /// The label of the product that the recommendation is about.
    pub product: ProductLabel,
    /// What the gap rule says about the product on the day of the question. In JSON its
    /// fields stand beside `product`, not under a name of their own.
    #[serde(flatten)]
    pub recommendation: Recommendation,
}

/// The report of the gap rule, `qm due --gap`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DueReport {
    /// The day the rule judges from: `--as-of`, or today.
    pub as_of: Date,
    /// How many orders are stored.
    pub orders: usize,
    /// The order cadence: the median gap, in days, between the distinct delivery dates.
    /// `None` when fewer than two distinct delivery dates are stored; the rule then takes 7
    /// days as the time until the next order.
    pub order_cadence_days: Option<i64>,
    /// The products to list: the overdue ones first, the most days overdue at the top, then
    /// the ones due soon, the soonest first, then the ones that the options add. Muted
    /// products and products bought on one day only are left out. Empty when nothing is due.
    pub rows: Vec<DueRow>,
}

/// One row of `qm due --simple`: the recommendation and how to show the product.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SimpleRow {
    /// The label of the product that the recommendation is about.
    pub product: ProductLabel,
    /// What the simple rule counted for the product. In JSON its fields stand beside
    /// `product`.
    #[serde(flatten)]
    pub recommendation: SimpleRecommendation,
}

/// The report of the simple rule, `qm due --simple`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SimpleReport {
    /// How many orders are stored.
    pub orders: usize,
    /// The products in at least half of all orders and absent from the most recent one, the
    /// product in the most orders first. Muted products are left out.
    pub rows: Vec<SimpleRow>,
}

/// One row of the essentials, the extras or the stocked products: the recommendation of the
/// habit rule and how to show the product.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HabitRow {
    /// The label of the product that the recommendation is about. When several products have
    /// exactly the same name, it is the one bought last, and it stands for all of them.
    pub product: ProductLabel,
    /// What the habit rule counted for the product. In JSON its fields stand beside
    /// `product`.
    #[serde(flatten)]
    pub recommendation: HabitRecommendation,
}

/// One row of "remember this?": the reminder and how to show the product.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReminderRow {
    /// The label of the product that the reminder is about.
    pub product: ProductLabel,
    /// In how many orders the product is and when it was last bought. In JSON its fields
    /// stand beside `product`.
    #[serde(flatten)]
    pub reminder: Reminder,
}

/// How long the lists of `qm due` are, and the seed of the shuffle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HabitOptions {
    /// The most extras to list: `--top`. The essentials are all listed, whatever this is.
    pub extras: usize,
    /// The most reminders to list: `--remember`.
    pub remember: usize,
    /// The seed of the shuffle that picks the reminders: `--seed`. One seed and one history
    /// give the same reminders each time.
    pub seed: u64,
}

/// The report of the habit rule, the default of `qm due`: the essentials, the extras, the
/// reminders, the additions and the stocked products.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HabitReport {
    /// The day a stock is judged from: `--as-of`, or today. The counts of the habit rule use
    /// no dates.
    pub as_of: Date,
    /// How many orders are stored.
    pub orders: usize,
    /// In at least a quarter of the latest orders: the core of the next order.
    pub essentials: Vec<HabitRow>,
    /// The next most habitual products, cut to the length asked for.
    pub extras: Vec<HabitRow>,
    /// Bought before and gone from the latest orders; a shuffle fixed by `seed`.
    pub remember: Vec<ReminderRow>,
    /// The seed that picked `remember`, so that the same reminders can be asked for again.
    pub seed: u64,
    /// What the user added by hand since the latest order, each with its reason.
    pub additions: Vec<Addition>,
    /// Habitual products left out of `essentials` and `extras` because the last purchase was
    /// a stock that should last past `as_of`.
    pub stocked: Vec<HabitRow>,
}

/// The use case of `qm due`: it loads the purchases, the delivery dates, the muted products
/// and the product labels, applies one of the three rules of the domain, and gives each
/// result its label.
pub struct Due<S: GroceryStore + Catalog + ShoppingList> {
    store: S,
    thresholds: Thresholds,
}

impl<S: GroceryStore + Catalog + ShoppingList> Due<S> {
    /// Builds the use case. `thresholds` are the settings of the gap rule, from the
    /// `[recommend]` section of the configuration; the habit rule and the simple rule do not
    /// read them.
    pub fn new(store: S, thresholds: Thresholds) -> Self {
        Self { store, thresholds }
    }

    /// The gap rule. `as_of` is supplied by the caller; the domain never reads the clock.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection fails, or the statement that reads the
    ///   purchases, the delivery dates, the muted products or the product labels fails.
    /// - [`StoreError::InvalidRow`] when a stored purchase, muted product or product label is
    ///   not a value that the domain accepts.
    pub async fn run(&self, as_of: Date, options: DueOptions) -> Result<DueReport, StoreError> {
        let purchases = self.store.purchases().await?;
        let dates = self.store.delivery_dates().await?;
        let muted = self.store.muted_products().await?;
        let labels = self.store.product_labels().await?;
        let cadence = order_cadence(&dates);
        let mut recommendations =
            evaluate(&purchases, as_of, cadence.unwrap_or(7), self.thresholds);
        rank(&mut recommendations);
        let rows = recommendations
            .into_iter()
            .filter(|r| !muted.contains(&r.product))
            .filter(|r| match r.status {
                Status::Overdue { .. } | Status::DueSoon { .. } => true,
                Status::Tentative => options.include_tentative,
                Status::Lapsed => options.include_lapsed,
                Status::NotDue => options.include_not_due,
                Status::BoughtOnce => false,
            })
            .filter_map(|recommendation| {
                labels
                    .iter()
                    .find(|label| label.id == recommendation.product)
                    .cloned()
                    .map(|product| DueRow {
                        product,
                        recommendation,
                    })
            })
            .collect();
        Ok(DueReport {
            as_of,
            orders: distinct_orders(&dates),
            order_cadence_days: cadence,
            rows,
        })
    }

    /// The habit rule, the default: the essentials, the extras and the reminders (ADR 0011).
    /// The sizes of one product count as one (ADR 0010), and a product whose last purchase
    /// was a stock that lasts past `as_of` is reported apart.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection fails, or the statement that reads the
    ///   purchases, the delivery dates, the muted products, the product labels or the open
    ///   additions fails.
    /// - [`StoreError::InvalidRow`] when a stored purchase, muted product, product label or
    ///   addition is not a value that the domain accepts.
    pub async fn run_habit(
        &self,
        options: HabitOptions,
        as_of: Date,
    ) -> Result<HabitReport, StoreError> {
        let purchases = self.store.purchases().await?;
        let dates = self.store.delivery_dates().await?;
        let muted = self.store.muted_products().await?;
        let labels = self.store.product_labels().await?;
        let variants: Vec<Variant<'_>> = labels
            .iter()
            .map(|label| Variant {
                product: label.id,
                name: &label.display_name,
                pack: match label.sold_by {
                    SoldBy::Each => label.size_text.as_deref().and_then(pack_amount),
                    SoldBy::Weight => None,
                },
            })
            .collect();
        let merged = merge_variants(&purchases, &variants);
        let label_of = |product| labels.iter().find(|label| label.id == product).cloned();
        let (stocked, listed): (Vec<HabitRow>, Vec<HabitRow>) = habit(&merged, as_of)
            .into_iter()
            .filter(|r| !muted.contains(&r.product))
            .filter_map(|recommendation| {
                label_of(recommendation.product).map(|product| HabitRow {
                    product,
                    recommendation,
                })
            })
            .partition(|row| row.recommendation.stocked_until.is_some());
        let (essentials, extras): (Vec<HabitRow>, Vec<HabitRow>) = listed
            .into_iter()
            .partition(|row| row.recommendation.is_essential());
        // Every candidate is asked for, so that a muted one does not shorten the list.
        let remember = remember(&merged, options.seed, usize::MAX)
            .into_iter()
            .filter(|r| !muted.contains(&r.product))
            .filter_map(|reminder| {
                label_of(reminder.product).map(|product| ReminderRow { product, reminder })
            })
            .take(options.remember)
            .collect();
        Ok(HabitReport {
            as_of,
            orders: distinct_orders(&dates),
            essentials,
            extras: extras.into_iter().take(options.extras).collect(),
            remember,
            seed: options.seed,
            additions: self.store.open_additions().await?,
            stocked,
        })
    }

    /// The simple rule: in at least half of all orders and missing from the last one.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection fails, or the statement that reads the
    ///   purchases, the delivery dates, the muted products or the product labels fails.
    /// - [`StoreError::InvalidRow`] when a stored purchase, muted product or product label is
    ///   not a value that the domain accepts.
    pub async fn run_simple(&self) -> Result<SimpleReport, StoreError> {
        let purchases = self.store.purchases().await?;
        let dates = self.store.delivery_dates().await?;
        let muted = self.store.muted_products().await?;
        let labels = self.store.product_labels().await?;
        let rows = simple(&purchases)
            .into_iter()
            .filter(|r| !muted.contains(&r.product))
            .filter_map(|recommendation| {
                labels
                    .iter()
                    .find(|label| label.id == recommendation.product)
                    .cloned()
                    .map(|product| SimpleRow {
                        product,
                        recommendation,
                    })
            })
            .collect();
        Ok(SimpleReport {
            orders: distinct_orders(&dates),
            rows,
        })
    }
}

fn distinct_orders(dates: &[Date]) -> usize {
    dates.len()
}
