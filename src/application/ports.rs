//! Ports: what the use cases need from the outside world, as traits (REAS-ARCH-001, 004).
//!
//! Adapters implement them. No SQLx, file or network type appears here (REAS-ARCH-002); an
//! adapter maps its own errors into [`StoreError`]. Async ports use native `async fn`. The use
//! cases are statically dispatched and awaited on the composition root's runtime without
//! `tokio::spawn`, so the returned futures are not required to be `Send` (REAS-ARCH-006); the
//! lint that warns about that is allowed for that reason.

use thiserror::Error;

/// A failure inside a store adapter, with the adapter's error kept as the source.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database, {operation}: {source}")]
    Database {
        operation: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("migration: {0}")]
    Migration(String),
}

/// What a migration run did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MigrationReport {
    /// Migrations applied by this run, as `version description`.
    pub applied: Vec<String>,
    /// Migrations that were already applied before this run.
    pub already_applied: usize,
}

/// The schema of the store: the only port the `migrate` command needs.
#[allow(async_fn_in_trait)]
pub trait SchemaStore {
    /// Applies the pending migrations. Idempotent: a second call applies nothing.
    async fn migrate(&self) -> Result<MigrationReport, StoreError>;
}
