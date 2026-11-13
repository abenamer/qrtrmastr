//! `qm migrate`: bring the schema up to date.

use super::ports::{MigrationReport, SchemaStore, StoreError};

/// Applies pending migrations through the schema port. It delegates to the port directly,
/// which REAS-ARCH-001 permits for a use case that only establishes the boundary.
pub struct Migrate<S: SchemaStore> {
    store: S,
}

impl<S: SchemaStore> Migrate<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub async fn run(&self) -> Result<MigrationReport, StoreError> {
        self.store.migrate().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A failing double (REAS-TEST-001): the error must reach the caller unchanged.
    struct BrokenSchema;

    impl SchemaStore for BrokenSchema {
        async fn migrate(&self) -> Result<MigrationReport, StoreError> {
            Err(StoreError::Migration("disk full".to_owned()))
        }
    }

    struct FixedSchema(MigrationReport);

    impl SchemaStore for FixedSchema {
        async fn migrate(&self) -> Result<MigrationReport, StoreError> {
            Ok(self.0.clone())
        }
    }

    #[tokio::test]
    async fn a_store_failure_is_returned_not_swallowed() {
        let result = Migrate::new(BrokenSchema).run().await;
        assert!(matches!(result, Err(StoreError::Migration(ref m)) if m == "disk full"));
    }

    #[tokio::test]
    async fn the_report_comes_back_as_the_store_made_it() {
        let report = MigrationReport {
            applied: vec!["0001 raw mail".to_owned()],
            already_applied: 0,
        };
        let result = Migrate::new(FixedSchema(report.clone())).run().await;
        assert_eq!(result.ok(), Some(report));
    }
}
