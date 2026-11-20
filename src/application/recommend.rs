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
    pub include_tentative: bool,
    pub include_lapsed: bool,
    pub include_not_due: bool,
}

/// One row of `qm due`: the recommendation and how to show the product.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DueRow {
    pub product: ProductLabel,
    #[serde(flatten)]
    pub recommendation: Recommendation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DueReport {
    pub as_of: Date,
    pub orders: usize,
    pub order_cadence_days: Option<i64>,
    pub rows: Vec<DueRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SimpleRow {
    pub product: ProductLabel,
    #[serde(flatten)]
    pub recommendation: SimpleRecommendation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SimpleReport {
    pub orders: usize,
    pub rows: Vec<SimpleRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HabitRow {
    pub product: ProductLabel,
    #[serde(flatten)]
    pub recommendation: HabitRecommendation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReminderRow {
    pub product: ProductLabel,
    #[serde(flatten)]
    pub reminder: Reminder,
}

/// How long the lists of `qm due` are, and the seed of the shuffle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HabitOptions {
    pub extras: usize,
    pub remember: usize,
    pub seed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HabitReport {
    pub as_of: Date,
    pub orders: usize,
    /// In at least a quarter of the latest orders: the core of the next order.
    pub essentials: Vec<HabitRow>,
    /// The next most habitual products, cut to the length asked for.
    pub extras: Vec<HabitRow>,
    /// Bought before and gone from the latest orders; a shuffle fixed by `seed`.
    pub remember: Vec<ReminderRow>,
    pub seed: u64,
    /// What the user added by hand since the latest order, each with its reason.
    pub additions: Vec<Addition>,
    /// Habitual products left out of `rows` because the last purchase was a stock that
    /// should last past `as_of`.
    pub stocked: Vec<HabitRow>,
}

pub struct Due<S: GroceryStore + Catalog + ShoppingList> {
    store: S,
    thresholds: Thresholds,
}

impl<S: GroceryStore + Catalog + ShoppingList> Due<S> {
    pub fn new(store: S, thresholds: Thresholds) -> Self {
        Self { store, thresholds }
    }

    /// The gap rule. `as_of` is supplied by the caller; the domain never reads the clock.
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
