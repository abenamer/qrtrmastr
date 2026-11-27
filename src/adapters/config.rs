//! Settings: the database URL from the environment, everything else from a TOML file in the
//! user's configuration directory, with `QM_` environment overrides.
//!
//! The database URL is a secret: it is wrapped so `Debug` prints `[REDACTED]` and it never
//! reaches a log (REAS-SEC-001). The file is outside the repository.

use std::collections::BTreeMap;
use std::path::PathBuf;

use figment::Figment;
use figment::providers::{Env, Format, Toml};
use secrecy::SecretString;
use serde::Deserialize;
use thiserror::Error;

use super::mail::gmail::Credentials;
use crate::domain::recommend::Thresholds;
use crate::domain::states::{Platform, UnknownText};

/// Why the settings cannot be loaded, or why a command cannot get a section of them. No
/// variant carries a value of the configuration file, because the file holds personal data.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// A required environment variable is not set. The value is the name of the variable;
    /// today only `DATABASE_URL` is required.
    #[error("{0} is not set; copy .env.example to .env and fill it in")]
    Missing(&'static str),
    /// An environment variable that holds a count is set to something that is not a whole
    /// number above zero; today only `DB_MAX_CONNECTIONS`.
    #[error("{name} is not a positive whole number: {value:?}")]
    NotAPositiveNumber {
        /// The name of the variable.
        name: &'static str,
        /// The text of the variable as it was set. It is not a secret.
        value: String,
    },
    /// The configuration file, or a `QM_` variable that overrides a key of it, cannot be read
    /// into the sections. A file that does not exist is not this error.
    #[error("cannot read the configuration file {path}: {reason}")]
    File {
        /// The configuration file that was looked for.
        path: PathBuf,
        /// The key and the kind of the problem. It never holds the value of the key.
        reason: String,
    },
    /// `QM_CONFIG` is not set, and the operating system names no home directory from which
    /// the default place of the configuration file follows.
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

/// The `[recommend]` section of the configuration file: the two numbers of the gap rule
/// (`qm due --gap`). The habit rule and the simple rule do not read them.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RecommendConfig {
    /// On how many different delivery dates a product must be bought before the gap rule
    /// says that it is overdue, due soon or not due. A product bought on fewer dates is
    /// tentative, or lapsed, or bought once. The default is 3.
    pub min_purchases: usize,
    /// A product not bought for more than this many usual gaps is lapsed. A count of gaps,
    /// not of days. The default is 3.
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
    /// The two numbers as the domain type that the gap rule takes.
    pub fn thresholds(&self) -> Thresholds {
        Thresholds {
            min_purchases: self.min_purchases,
            lapsed_after_gaps: self.lapsed_after_gaps,
        }
    }
}

/// The `[import]` section of the configuration file: which sender addresses belong to which
/// platform. `qm import` stores the mail of these addresses, and `qm fetch` searches Gmail
/// for them.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ImportConfig {
    /// The exact From addresses that are FreshDirect's.
    pub senders: Vec<String>,
    /// The exact From addresses of each restaurant platform's order mail (ADR 0007). A table in
    /// the file replaces the whole default. Mail from any other address is counted, not stored.
    #[serde(deserialize_with = "restaurant_senders")]
    pub restaurant_senders: BTreeMap<Platform, Vec<String>>,
}

/// A key of the `[import.restaurant_senders]` table as the file writes it: the stored text
/// of a platform, such as `seamless`. The domain type has no deserializer of its own
/// (REAS-DATA-001), so the key is read as text here and goes through `Platform::from_str`,
/// the one place that knows the platforms (REAS-SEC-002).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(try_from = "String")]
struct PlatformKey(Platform);

impl TryFrom<String> for PlatformKey {
    type Error = UnknownText;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse().map(Self)
    }
}

/// Reads the `[import.restaurant_senders]` table. A key that is not a platform is an error
/// of the file, at that key.
fn restaurant_senders<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<Platform, Vec<String>>, D::Error> {
    let table = BTreeMap::<PlatformKey, Vec<String>>::deserialize(deserializer)?;
    Ok(table
        .into_iter()
        .map(|(PlatformKey(platform), addresses)| (platform, addresses))
        .collect())
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

/// The TOML file. A section that the program does not know is not read: a `[redaction]`
/// section of a file of before 2026-11-24 is such a section now.
#[derive(Clone, Deserialize, Default)]
#[serde(default)]
pub struct FileConfig {
    /// The `[recommend]` section. The defaults when the file has none.
    pub recommend: RecommendConfig,
    /// The `[import]` section. The default sender addresses when the file has none.
    pub import: ImportConfig,
}

impl std::fmt::Debug for FileConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileConfig")
            .field("recommend", &self.recommend)
            .field("import", &self.import)
            .finish()
    }
}

/// Everything the tool reads from outside at the start of a command: the environment
/// variables and the configuration file. The composition root loads it once.
#[derive(Debug)]
pub struct Settings {
    /// `DATABASE_URL`: the connection URL of the PostgreSQL database. A secret: `Debug` does
    /// not print it, and only the code that opens the connection pool exposes it.
    pub database_url: SecretString,
    /// `DB_MAX_CONNECTIONS`: the most connections the pool opens. At least 1; 4 when the
    /// variable is not set.
    pub db_max_connections: u32,
    /// The configuration file that was looked for: `QM_CONFIG`, else `config.toml` in the
    /// folder `qrtrmastr` of the configuration directory of the user. The file need not exist.
    pub config_path: PathBuf,
    /// The sections of the configuration file, with a `QM_` variable in place of each key
    /// that one overrides.
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
    ///
    /// # Errors
    ///
    /// - [`ConfigError::Missing`] when `DATABASE_URL` is not set.
    /// - [`ConfigError::NotAPositiveNumber`] when `DB_MAX_CONNECTIONS` is set and is not a
    ///   whole number above zero.
    /// - [`ConfigError::NoConfigDir`] when `QM_CONFIG` is not set and the user has no home
    ///   directory to find the configuration directory from.
    /// - [`ConfigError::File`] when the configuration file exists and cannot be read, is not
    ///   valid TOML, or when a key of it or of a `QM_` variable has the wrong type, names an
    ///   unknown platform.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_config_has_working_defaults_and_does_not_read_a_section_of_before() {
        // A file of before 2026-11-24 has a `[redaction]` section with the terms of its
        // user. The program has no such section now: the file is read, and the section is not.
        let parsed: FileConfig = toml::from_str(
            "[redaction]\nnames = [\"Pat Example\"]\naddress_line = [\"x\"]\n\
             [recommend]\nmin_purchases = 4\n",
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
        // Nothing of the section is kept, so nothing of it can be printed.
        let debug = format!("{parsed:?}");
        assert!(!debug.contains("Pat Example"), "{debug}");
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

    fn settings(text: &str) -> Settings {
        Settings {
            database_url: SecretString::from("postgres://unused".to_owned()),
            db_max_connections: 1,
            config_path: PathBuf::from("/config/config.toml"),
            file: toml::from_str(text).expect("parses"),
            gmail: None,
            state_dir: None,
        }
    }

    #[test]
    fn the_numbers_of_the_gap_rule_come_from_the_file() {
        let file = settings("[recommend]\nmin_purchases = 4\nlapsed_after_gaps = 5\n").file;
        assert_eq!(
            file.recommend.thresholds(),
            Thresholds {
                min_purchases: 4,
                lapsed_after_gaps: 5
            }
        );
    }

    #[test]
    fn an_unknown_platform_in_the_file_is_an_error_at_its_key() {
        let error: figment::Error = Figment::new()
            .merge(Toml::string(
                "[import.restaurant_senders]\nubereats = [\"orders@uber.example\"]\n",
            ))
            .extract::<FileConfig>()
            .expect_err("ubereats is not a platform");
        assert_eq!(
            describe_without_values(&error),
            "cannot be read at import.restaurant_senders.ubereats"
        );
        // The key is the stored text of the platform as it is: another case and a space at
        // an end are not repaired.
        for key in ["Toast", "TOAST", "\" toast\"", "\"toast \"", "\"\""] {
            let file = format!("[import.restaurant_senders]\n{key} = []\n");
            assert!(
                Figment::new()
                    .merge(Toml::string(&file))
                    .extract::<FileConfig>()
                    .is_err(),
                "{key}"
            );
        }
        // The control: a platform at the same place is read, through the same path.
        let file: FileConfig = Figment::new()
            .merge(Toml::string(
                "[import.restaurant_senders]\ntoast = [\"orders@toast.example\"]\n",
            ))
            .extract()
            .expect("toast is a platform");
        assert_eq!(
            file.import.restaurant_senders,
            BTreeMap::from([(Platform::Toast, vec!["orders@toast.example".to_owned()])])
        );
    }

    #[test]
    fn a_configuration_error_names_the_key_and_never_the_value() {
        let error: figment::Error = Figment::new()
            .merge(Toml::string("[import]\nsenders = \"pat@example.com\"\n"))
            .extract::<FileConfig>()
            .expect_err("a scalar where a list is expected fails");
        let text = describe_without_values(&error);
        assert!(text.contains("import.senders"), "{text}");
        assert!(!text.contains("pat@"), "{text}");
    }
}
