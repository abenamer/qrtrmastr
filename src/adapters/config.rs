//! Settings: the database URL from the environment, everything else from a TOML file in the
//! user's configuration directory, with `QM_` environment overrides.
//!
//! The database URL is a secret: it is wrapped so `Debug` prints `[REDACTED]` and it never
//! reaches a log (REAS-SEC-001). The redaction terms are personal data; they are read, never
//! printed, and the file that holds them is outside the repository.

use std::collections::BTreeMap;
use std::path::PathBuf;

use figment::Figment;
use figment::providers::{Env, Format, Toml};
use secrecy::SecretString;
use serde::Deserialize;
use thiserror::Error;

use super::mail::gmail::Credentials;
use super::mail::redact::RedactionTerms;
use crate::domain::recommend::Thresholds;
use crate::domain::states::Platform;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("{0} is not set; copy .env.example to .env and fill it in")]
    Missing(&'static str),
    #[error("{name} is not a positive whole number: {value:?}")]
    NotAPositiveNumber { name: &'static str, value: String },
    #[error("cannot read the configuration file {path}: {reason}")]
    File { path: PathBuf, reason: String },
    #[error(
        "no [redaction] section in {0}; the import refuses to run without your redaction terms.\n\
         Through Compose, /config is your configuration directory (QM_CONFIG_DIR, \
         ~/.config/qrtrmastr by default). Add the section with your own values, as \
         docs/reference/configuration.md describes, then import again:\n\n\
         [redaction]\n\
         names = [\"Your Name\", \"Your\"]\n\
         emails = [\"you@example.com\"]\n\
         phones = [\"2125550100\"]\n\
         address_lines = [\"12 Example Street Apt 3\", \"Exampleville, NY 00000\"]\n\
         card_last4 = [\"4242\"]\n\n\
         Put the same terms, one per line, in canary-terms.txt beside it for the local privacy canary."
    )]
    NoRedactionTerms(PathBuf),
    #[error("no configuration directory can be found for this user")]
    NoConfigDir,
}

/// A configuration error as text that names the key and the kind of problem, never the value:
/// the file holds personal data (REAS-SEC-001).
fn describe_without_values(error: &figment::Error) -> String {
    use figment::error::Kind;
    let what = match &error.kind {
        Kind::InvalidType(_, expected) => format!("wrong type, expected {expected}"),
        Kind::InvalidValue(_, expected) => format!("invalid value, expected {expected}"),
        Kind::InvalidLength(_, expected) => format!("wrong length, expected {expected}"),
        Kind::UnknownVariant(_, expected) => {
            format!("unknown variant, expected one of {expected:?}")
        }
        Kind::UnknownField(field, _) => format!("unknown field {field}"),
        Kind::MissingField(field) => format!("missing field {field}"),
        Kind::DuplicateField(field) => format!("duplicate field {field}"),
        Kind::ISizeOutOfRange(_) | Kind::USizeOutOfRange(_) => "number out of range".to_owned(),
        Kind::Unsupported(_) => "unsupported value".to_owned(),
        Kind::UnsupportedKey(_, _) => "unsupported key".to_owned(),
        Kind::Message(_) => "cannot be read".to_owned(),
    };
    if error.path.is_empty() {
        what
    } else {
        format!("{what} at {}", error.path.join("."))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RecommendConfig {
    pub min_purchases: usize,
    pub lapsed_after_gaps: u32,
}

impl Default for RecommendConfig {
    fn default() -> Self {
        Self {
            min_purchases: 3,
            lapsed_after_gaps: 3,
        }
    }
}

impl RecommendConfig {
    pub fn thresholds(&self) -> Thresholds {
        Thresholds {
            min_purchases: self.min_purchases,
            lapsed_after_gaps: self.lapsed_after_gaps,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ImportConfig {
    /// The exact From addresses that are FreshDirect's.
    pub senders: Vec<String>,
    /// The exact From addresses of each restaurant platform's order mail (ADR 0007). A table in
    /// the file replaces the whole default. Mail from any other address is counted, not stored.
    pub restaurant_senders: BTreeMap<Platform, Vec<String>>,
}

impl Default for ImportConfig {
    fn default() -> Self {
        let restaurant = |platform, addresses: &[&str]| {
            (
                platform,
                addresses.iter().map(|a| (*a).to_owned()).collect(),
            )
        };
        Self {
            senders: vec!["service@freshdirect.com".to_owned()],
            restaurant_senders: BTreeMap::from([
                restaurant(Platform::Seamless, &["orders@eat.seamless.com"]),
                restaurant(
                    Platform::Grubhub,
                    &["orders@eat.grubhub.com", "no-reply@grubhub.com"],
                ),
                restaurant(Platform::Toast, &["no-reply@toasttab.com"]),
                restaurant(Platform::Square, &["messenger@messaging.squareup.com"]),
                restaurant(Platform::Clover, &["app@clover.com"]),
                restaurant(Platform::Doordash, &["no-reply@doordash.com"]),
            ]),
        }
    }
}

impl ImportConfig {
    /// Every sender address with its platform, as the mail reader takes them.
    pub fn sender_platforms(&self) -> Vec<(String, Platform)> {
        let grocer = self
            .senders
            .iter()
            .map(|address| (address.clone(), Platform::Freshdirect));
        let restaurants = self
            .restaurant_senders
            .iter()
            .flat_map(|(platform, addresses)| addresses.iter().map(|a| (a.clone(), *platform)));
        grocer.chain(restaurants).collect()
    }
}

/// The TOML file. `redaction` has no default on purpose: the import needs your terms.
#[derive(Clone, Deserialize, Default)]
#[serde(default)]
pub struct FileConfig {
    pub redaction: Option<RedactionTerms>,
    pub recommend: RecommendConfig,
    pub import: ImportConfig,
}

impl std::fmt::Debug for FileConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileConfig")
            .field("redaction", &self.redaction.as_ref().map(|_| "[REDACTED]"))
            .field("recommend", &self.recommend)
            .field("import", &self.import)
            .finish()
    }
}

#[derive(Debug)]
pub struct Settings {
    pub database_url: SecretString,
    pub db_max_connections: u32,
    pub config_path: PathBuf,
    pub file: FileConfig,
    /// The OAuth client for `qm gmail login` and `qm fetch`: `QM_GMAIL_CLIENT_ID` and
    /// `QM_GMAIL_CLIENT_SECRET`. `None` until both are set; only those commands need it.
    pub gmail: Option<Credentials>,
    /// Where the tool keeps what it must remember between runs, today the Gmail refresh
    /// token: `QM_STATE`, else the user's state directory.
    pub state_dir: Option<PathBuf>,
}

impl Settings {
    /// Loads `.env` if present, the environment, and the TOML file. A missing required variable
    /// is an error, never a default (REAS-RUST-005). A missing TOML file is fine until a command
    /// needs something from it.
    pub fn load() -> Result<Self, ConfigError> {
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
        let config_path = match std::env::var_os("QM_CONFIG") {
            Some(path) => PathBuf::from(path),
            None => directories::ProjectDirs::from("", "", "qrtrmastr")
                .ok_or(ConfigError::NoConfigDir)?
                .config_dir()
                .join("config.toml"),
        };
        let file: FileConfig = Figment::new()
            .merge(Toml::file(&config_path))
            .merge(Env::prefixed("QM_").split("__"))
            .extract()
            .map_err(|e| ConfigError::File {
                path: config_path.clone(),
                reason: describe_without_values(&e),
            })?;
        let set = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        let gmail = match (set("QM_GMAIL_CLIENT_ID"), set("QM_GMAIL_CLIENT_SECRET")) {
            (Some(client_id), Some(secret)) => Some(Credentials {
                client_id: client_id.trim().to_owned(),
                client_secret: SecretString::from(secret.trim().to_owned()),
            }),
            _ => None,
        };
        // An empty value is no value: it would put the token file into the working directory.
        let state_dir = match std::env::var_os("QM_STATE").filter(|value| !value.is_empty()) {
            Some(path) => Some(PathBuf::from(path)),
            None => directories::ProjectDirs::from("", "", "qrtrmastr").map(|dirs| {
                dirs.state_dir()
                    .unwrap_or_else(|| dirs.data_local_dir())
                    .to_path_buf()
            }),
        };
        Ok(Self {
            database_url: SecretString::from(database_url),
            db_max_connections,
            config_path,
            file,
            gmail,
            state_dir,
        })
    }

    pub fn redaction_terms(&self) -> Result<RedactionTerms, ConfigError> {
        self.file
            .redaction
            .clone()
            .ok_or_else(|| ConfigError::NoRedactionTerms(self.config_path.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_config_has_working_defaults_and_hides_the_terms_in_debug() {
        let parsed: FileConfig = toml::from_str(
            "[redaction]\nnames = [\"Pat Example\"]\n[recommend]\nmin_purchases = 4\n",
        )
        .expect("parses");
        assert_eq!(parsed.recommend.min_purchases, 4);
        assert_eq!(parsed.recommend.lapsed_after_gaps, 3);
        assert_eq!(
            parsed.import.senders,
            vec!["service@freshdirect.com".to_owned()]
        );
        assert!(
            parsed
                .import
                .sender_platforms()
                .contains(&("orders@eat.seamless.com".to_owned(), Platform::Seamless))
        );
        let debug = format!("{parsed:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("Pat Example"));
    }

    #[test]
    fn a_restaurant_senders_table_replaces_the_default_and_refuses_an_unknown_platform() {
        let parsed: FileConfig = toml::from_str(
            "[import.restaurant_senders]\nsquare = [\"receipts@messaging.squareup.com\"]\n",
        )
        .expect("parses");
        assert_eq!(
            parsed.import.sender_platforms(),
            vec![
                ("service@freshdirect.com".to_owned(), Platform::Freshdirect),
                (
                    "receipts@messaging.squareup.com".to_owned(),
                    Platform::Square
                ),
            ]
        );
        assert!(
            toml::from_str::<FileConfig>("[import.restaurant_senders]\nubereats = []\n").is_err()
        );
    }

    #[test]
    fn the_missing_redaction_error_shows_the_section_to_add_and_where() {
        let text = ConfigError::NoRedactionTerms(PathBuf::from("/config/config.toml")).to_string();
        assert!(
            text.starts_with("no [redaction] section in /config/config.toml"),
            "{text}"
        );
        assert!(text.contains("QM_CONFIG_DIR"), "{text}");
        for key in ["names", "emails", "phones", "address_lines", "card_last4"] {
            assert!(
                text.contains(&format!("\n{key} = [")),
                "{key} missing in {text}"
            );
        }
    }

    #[test]
    fn a_configuration_error_names_the_key_and_never_the_value() {
        let error: figment::Error = Figment::new()
            .merge(Toml::string("[redaction]\nnames = \"Pat Example\"\n"))
            .extract::<FileConfig>()
            .expect_err("a scalar where a list is expected fails");
        let text = describe_without_values(&error);
        assert!(text.contains("redaction.names"), "{text}");
        assert!(!text.contains("Pat"), "{text}");
    }
}
