//! `qm fixture add`: a stored, redacted receipt becomes a local parser fixture.

use super::ports::{RawMessageStore, StoreError, StoredBody};

pub struct ExportFixture<S: RawMessageStore> {
    store: S,
}

impl<S: RawMessageStore> ExportFixture<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// The redacted body under `key`, or `None` when no such message is stored.
    pub async fn run(&self, key: &str) -> Result<Option<StoredBody>, StoreError> {
        self.store.body_by_key(key).await
    }
}
