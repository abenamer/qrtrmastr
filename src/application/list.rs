//! `qm add` and `qm drop`: the user's own additions to the next order, each with its reason
//! (ADR 0011).

use jiff::civil::Date;
use serde::Serialize;

use super::ports::{Catalog, NewAddition, ProductLabel, ShoppingList, StoreError};
use crate::domain::ids::AdditionId;
use crate::domain::list::{ItemText, the_one_product};
use crate::domain::states::AddReason;

/// What `qm add` stored: the addition and the product its text named, if exactly one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Added {
    pub id: AdditionId,
    pub item: ItemText,
    pub reason: AddReason,
    pub product: Option<ProductLabel>,
}

pub struct ManageList<S: ShoppingList + Catalog> {
    store: S,
}

impl<S: ShoppingList + Catalog> ManageList<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// Stores one addition. `today` is supplied by the caller.
    pub async fn add(
        &self,
        item: ItemText,
        reason: AddReason,
        today: Date,
    ) -> Result<Added, StoreError> {
        let labels = self.store.product_labels().await?;
        let product = the_one_product(
            &item,
            labels.iter().map(|l| (l.id, l.display_name.as_str())),
        );
        let id = self
            .store
            .add_addition(&NewAddition {
                item: &item,
                product,
                reason,
                added_on: today,
            })
            .await?;
        Ok(Added {
            id,
            item,
            reason,
            product: labels.into_iter().find(|l| Some(l.id) == product),
        })
    }

    /// Removes one addition; `false` when there is none with this id.
    pub async fn drop(&self, id: AdditionId) -> Result<bool, StoreError> {
        self.store.drop_addition(id).await
    }
}
