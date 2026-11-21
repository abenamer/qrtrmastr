//! `qm fixture add`: a stored, redacted receipt becomes a local parser fixture.

use super::ports::{RawMessageStore, StoreError, StoredBody};

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

    /// The redacted body under `key`, or `None` when no such message is stored.
    ///
    /// `key` is the text of the message key, without its kind.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Database`] when the connection or the statement fails.
    /// - [`StoreError::InvalidRow`] when the stored platform or the stored kind of the key is
    ///   not a value that the domain knows.
    pub async fn run(&self, key: &str) -> Result<Option<StoredBody>, StoreError> {
        self.store.body_by_key(key).await
    }
}
