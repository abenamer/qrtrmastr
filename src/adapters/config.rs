//! Settings read from the environment and an optional `.env` file.
//!
//! The database URL is a secret: it is wrapped so `Debug` prints `[REDACTED]` and it never
//! reaches a log (REAS-SEC-001).

use secrecy::SecretString;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("{0} is not set; copy .env.example to .env and fill it in")]
    Missing(&'static str),
    #[error("{name} is not a positive whole number: {value:?}")]
    NotAPositiveNumber { name: &'static str, value: String },
}

#[derive(Debug)]
pub struct Settings {
    pub database_url: SecretString,
    pub db_max_connections: u32,
}

impl Settings {
    /// Loads `.env` if present, then reads the variables. A missing required variable is an
    /// error, never a default (REAS-RUST-005).
    pub fn from_env() -> Result<Self, ConfigError> {
        // A missing .env is the normal case outside development; a present one is loaded.
        let _ = dotenvy::dotenv();
        let database_url =
            std::env::var("DATABASE_URL").map_err(|_| ConfigError::Missing("DATABASE_URL"))?;
        let db_max_connections = match std::env::var("DB_MAX_CONNECTIONS") {
            Ok(value) => value.trim().parse::<u32>().ok().filter(|n| *n > 0).ok_or(
                ConfigError::NotAPositiveNumber {
                    name: "DB_MAX_CONNECTIONS",
                    value,
                },
            )?,
            Err(_) => 4,
        };
        Ok(Self {
            database_url: SecretString::from(database_url),
            db_max_connections,
        })
    }
}
