//! `qm fixture add`: a stored, redacted receipt becomes a local parser fixture.

use super::ports::{RawMessageStore, StoreError, StoredBody};
use crate::domain::ids::MessageKey;

/// Finds the stored message that `qm fixture add` names. The use case only reads; the fixtures
/// adapter writes the file.
pub struct ExportFixture<S: RawMessageStore> {
    store: S,
}

impl<S: RawMessageStore> ExportFixture<S> {
    /// Builds the use case over the store of raw messages.
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// The redacted body under `key`, or `None` when no message is stored under that kind
    /// and that text.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection or the statement fails.
    /// - [`StoreError::InvalidRow`] when the stored platform is not a value that the domain
    ///   knows.
    pub async fn run(&self, key: &MessageKey) -> Result<Option<StoredBody>, StoreError> {
        self.store.body_by_key(key).await
    }
}
