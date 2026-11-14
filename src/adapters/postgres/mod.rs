//! `PostgresStore`: the PostgreSQL adapter behind every store port (REAS-DATA-001).
//!
//! Each port operation is one specific SQL statement in this module. Writes that must succeed
//! or fail together run inside one transaction that this adapter owns (REAS-DATA-002).

use std::time::Duration;

use sqlx::migrate::Migrator;
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::application::ports::{MigrationReport, SchemaStore, StoreError};

/// The migrations under `migrations/`, embedded at compile time.
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self, StoreError> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(30))
            .connect(database_url)
            .await
            .map_err(|source| database_error("connect", source))?;
        Ok(Self { pool })
    }

    /// Wraps a pool a test harness created.
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

fn database_error(operation: &'static str, source: sqlx::Error) -> StoreError {
    StoreError::Database {
        operation,
        source: Box::new(source),
    }
}

/// The versions SQLx has recorded as applied, or none when the table does not exist yet.
async fn applied_migrations(pool: &PgPool) -> Result<Vec<(i64, String)>, StoreError> {
    let (table,): (Option<String>,) =
        sqlx::query_as("select to_regclass('_sqlx_migrations')::text")
            .fetch_one(pool)
            .await
            .map_err(|source| database_error("check the migrations table", source))?;
    if table.is_none() {
        return Ok(Vec::new());
    }
    sqlx::query_as::<_, (i64, String)>(
        "select version, description from _sqlx_migrations where success order by version",
    )
    .fetch_all(pool)
    .await
    .map_err(|source| database_error("list applied migrations", source))
}

impl SchemaStore for PostgresStore {
    async fn migrate(&self) -> Result<MigrationReport, StoreError> {
        let before = applied_migrations(&self.pool).await?;
        MIGRATOR
            .run(&self.pool)
            .await
            .map_err(|error| StoreError::Migration(error.to_string()))?;
        let after = applied_migrations(&self.pool).await?;
        let applied = after
            .iter()
            .filter(|(version, _)| !before.iter().any(|(known, _)| known == version))
            .map(|(version, description)| format!("{version} {description}"))
            .collect();
        Ok(MigrationReport {
            applied,
            already_applied: before.len(),
        })
    }
}
