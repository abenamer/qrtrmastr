//! `qm due`: what the history says you are about to run out of.

use chrono::NaiveDate;
use serde::Serialize;

use super::ports::{Catalog, GroceryStore, ProductLabel, StoreError};
use crate::domain::recommend::{
    Recommendation, SimpleRecommendation, Status, Thresholds, evaluate, order_cadence, rank, simple,
};

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
    pub as_of: NaiveDate,
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

pub struct Due<S: GroceryStore + Catalog> {
    store: S,
    thresholds: Thresholds,
}

impl<S: GroceryStore + Catalog> Due<S> {
    pub fn new(store: S, thresholds: Thresholds) -> Self {
        Self { store, thresholds }
    }

    /// The gap rule. `as_of` is supplied by the caller; the domain never reads the clock.
    pub async fn run(
        &self,
        as_of: NaiveDate,
        options: DueOptions,
    ) -> Result<DueReport, StoreError> {
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

fn distinct_orders(dates: &[NaiveDate]) -> usize {
    dates.len()
}
