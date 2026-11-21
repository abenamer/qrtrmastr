//! `qm add` and `qm drop`: the user's own additions to the next order, each with its reason
//! (ADR 0011).

use jiff::civil::Date;

use super::ports::{Catalog, NewAddition, ProductLabel, ShoppingList, StoreError};
use crate::domain::ids::AdditionId;
use crate::domain::list::{ItemText, the_one_product};
use crate::domain::states::AddReason;

/// What `qm add` stored: the addition and the product its text named, if exactly one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    /// The number of the addition, which `qm drop` takes.
    pub id: AdditionId,
    /// What the user typed for the item: one line, trimmed.
    pub item: ItemText,
    /// Why the user added the item.
    pub reason: AddReason,
    /// The label of the product whose display name holds the whole text, without regard to
    /// case. `None` when no product or more than one matches; the addition is stored all the
    /// same.
    pub product: Option<ProductLabel>,
}

/// Adds to and removes from the user's additions to the next order. It needs the catalog to
/// find the product that the text of an addition names.
pub struct ManageList<S: ShoppingList + Catalog> {
    store: S,
}

impl<S: ShoppingList + Catalog> ManageList<S> {
    /// Builds the use case over the store that holds the additions and the catalog.
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// Stores one addition. `today` is supplied by the caller.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection fails, or the statement that lists the
    ///   product labels or the one that stores the addition fails.
    /// - [`StoreError::InvalidRow`] when a stored product or the identity of the new addition
    ///   is not a value that the domain accepts.
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
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection or the statement fails.
    pub async fn drop(&self, id: AdditionId) -> Result<bool, StoreError> {
        self.store.drop_addition(id).await
    }
}
