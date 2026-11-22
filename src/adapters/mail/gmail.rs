//! The Gmail API as a mail source (ADR 0013): new receipts arrive without a Takeout export.
//!
//! Three parts. The sign-in is the OAuth flow for an installed application with PKCE and the
//! read-only scope; its refresh token is kept in a file only the user can read. A fetch trades
//! that token for an access token, lists the messages of the platform senders, and reads each
//! one raw: the same RFC 5322 bytes a Takeout export holds, so everything after the source is
//! unchanged, redaction included.
//!
//! The HTTP client is blocking, so every call here runs on the blocking pool
//! (REAS-RUST-007); `GmailApi::messages` sends through the bounded channel of the port, which
//! is the limit on work in flight. Limits (REAS-SEC-001): HTTPS only, no redirects, a timeout
//! for each call, a size limit on each response, and a limit on the messages of one run. No
//! token, code or message body is ever part of an error or a log line.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use jiff::Timestamp;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::mpsc;
use url::Url;

use crate::application::ports::{MailError, MailSource, MailStream, RawMail};

/// Read-only access to mail: the narrowest scope that returns a message body.
pub const SCOPE: &str = "https://www.googleapis.com/auth/gmail.readonly";
/// Where Google sends the browser after the consent. Nothing listens there: the user copies
/// the address from the browser and pastes it, so the tool opens no port.
pub const REDIRECT_URI: &str = "http://127.0.0.1:8765";
/// The file in the state directory that holds the refresh token.
pub const TOKEN_FILE_NAME: &str = "gmail-token.json";

const CHANNEL_BOUND: usize = 8;
const PAGE_SIZE: &str = "100";
const CALL_TIMEOUT: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// A token answer and a page of message ids are small.
const SMALL_RESPONSE: u64 = 1024 * 1024;
/// A raw message arrives as base64 inside JSON. Gmail accepts 25 MB of mail; this is above
/// what that becomes and well below what would exhaust memory.
const MESSAGE_RESPONSE: u64 = 64 * 1024 * 1024;
/// What the tool keeps of an error code Google returns: a short identifier, never free text.
const ERROR_CODE_MAX: usize = 48;

/// Why a sign-in or a fetch through the Gmail API did not work. No variant carries a token,
/// an authorization code, a message body or free text of an answer from Google.
#[derive(Debug, Error)]
pub enum GmailError {
    /// `QM_GMAIL_CLIENT_ID` and `QM_GMAIL_CLIENT_SECRET` are not both set, so the settings
    /// hold no OAuth client. The composition root makes this value for `qm gmail login` and
    /// `qm fetch`.
    #[error(
        "Gmail is not set up: QM_GMAIL_CLIENT_ID and QM_GMAIL_CLIENT_SECRET are not set. \
         How to create them: docs/runbooks/gmail-fetch.md"
    )]
    NoCredentials,
    /// The token file does not exist: no sign-in was stored yet. The value is the path where
    /// the file was looked for.
    #[error("no Gmail sign-in is stored at {0}; run `qm gmail login` first")]
    NoToken(PathBuf),
    /// The token file exists and cannot be read, does not hold a refresh token, or cannot be
    /// written.
    #[error("the token file {path}: {reason}")]
    TokenFile {
        /// The path of the token file.
        path: PathBuf,
        /// The kind of the file system error, or a fixed text. Never a part of the file.
        reason: String,
    },
    /// The address that the user pasted at `qm gmail login` is not the redirect of this
    /// sign-in attempt. The value is a fixed text that says what is wrong: not an address,
    /// another host or port, the state of another attempt, or no code.
    #[error("the pasted address cannot be used: {0}")]
    Redirect(&'static str),
    /// The pasted address carries an `error` parameter: the consent was not given. The value
    /// is that error code, cut to letters, digits, underscores and spaces, at most 48.
    #[error("Google did not give access: {0}")]
    Denied(String),
    /// A call answered with the error code `invalid_grant`, which the token endpoint gives.
    /// For a token refresh that means the stored refresh token expired or was revoked. The
    /// exchange of a sign-in code that is wrong or was used before gets this error too.
    #[error(
        "the stored Gmail sign-in is no longer valid (expired or revoked); run `qm gmail login` again"
    )]
    SignInExpired,
    /// Google answered a call with an HTTP status outside 200 to 299, and the error code is
    /// not `invalid_grant`.
    #[error("Google answered {what} with status {status} ({code})")]
    Status {
        /// The call in words: the sign-in, the token refresh, the message list or a message.
        what: &'static str,
        /// The HTTP status of the answer.
        status: u16,
        /// The short error code of the answer, cut to letters, digits, underscores and
        /// spaces, at most 48; it can be empty. `no code` when the answer names none.
        code: String,
    },
    /// A call got no HTTP answer: a timeout, a host name that does not resolve, a connection
    /// that failed, or an address that is not HTTPS. A redirect is not followed and is a
    /// [`GmailError::Status`] with its 3xx status.
    #[error("cannot reach Google for {what}: {reason}")]
    Transport {
        /// The call in words: the sign-in, the token refresh, the message list or a message.
        what: &'static str,
        /// A fixed text for the kind of failure. The text of the HTTP client's error is not
        /// used, because it can name a URL.
        reason: &'static str,
    },
    /// Google answered a call with a success status, and the answer cannot be used: it is
    /// not the expected JSON, it is larger than the size limit of the call, or a value in it
    /// is missing or has the wrong form.
    #[error("Google's answer to {what} cannot be read: {reason}")]
    Answer {
        /// The call in words: the sign-in, the token refresh, the message list or a message.
        what: &'static str,
        /// A fixed text that says what is wrong with the answer. Never a part of the answer.
        reason: &'static str,
    },
    /// The operating system gave no random bytes for the PKCE verifier or the state of a
    /// sign-in attempt.
    #[error("the system gave no random numbers")]
    Random,
}

/// The three addresses the adapter talks to. Google's by default; a test names a local server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    /// The consent page: the address, with the parameters of a sign-in attempt, that the
    /// user opens in a browser.
    pub authorize: String,
    /// The token endpoint: it trades an authorization code for a refresh token, and a
    /// refresh token for an access token.
    pub token: String,
    /// The base address of the Gmail API: scheme and host, with no path and no slash at the
    /// end. The path of each call is appended to it.
    pub api: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            authorize: "https://accounts.google.com/o/oauth2/v2/auth".to_owned(),
            token: "https://oauth2.googleapis.com/token".to_owned(),
            api: "https://gmail.googleapis.com".to_owned(),
        }
    }
}

/// The OAuth client of the user's own Google Cloud project. The secret of an installed
/// application is not confidential in Google's sense, but it is kept out of logs all the same.
#[derive(Debug, Clone)]
pub struct Credentials {
    /// `QM_GMAIL_CLIENT_ID`, without spaces at its ends. Not a secret: it is part of the
    /// address that the user opens in a browser.
    pub client_id: String,
    /// `QM_GMAIL_CLIENT_SECRET`, without spaces at its ends. It goes to the token endpoint
    /// only, in the body of the request.
    pub client_secret: SecretString,
}

/// The HTTP client with the limits of this module. `https_only` is false in tests only.
pub fn agent(https_only: bool) -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(https_only)
        .http_status_as_error(false)
        .max_redirects(0)
        .timeout_global(Some(CALL_TIMEOUT))
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .build()
        .into()
}

fn transport(what: &'static str, error: &ureq::Error) -> GmailError {
    // The text of the error is not used: it can name a URL. The kind is enough to act on.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "ureq::Error is an external non-exhaustive enum; every kind that is not named above gets the same text"
    )]
    let reason = match error {
        ureq::Error::Timeout(_) => "the call timed out",
        ureq::Error::HostNotFound => "the host name cannot be resolved; is the network up?",
        ureq::Error::Io(_) => "the connection failed",
        ureq::Error::RedirectFailed | ureq::Error::TooManyRedirects => {
            "a redirect, which is not followed"
        }
        _ => "the request failed",
    };
    GmailError::Transport { what, reason }
}

/// The short code of an error answer: `error.status` of the API or `error` of the token
/// endpoint, cut to identifier characters. Google's free text is not kept.
fn error_code(body: &serde_json::Value) -> String {
    let raw = body
        .get("error")
        .and_then(|e| e.get("status").or(Some(e)))
        .and_then(|v| v.as_str())
        .unwrap_or("no code");
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == ' ')
        .take(ERROR_CODE_MAX)
        .collect()
}

/// What an answer that cannot be read is called in an error, whatever the cause.
const NOT_THE_JSON: &str = "not the expected JSON, or larger than the limit";

/// The body of an answer with a status of success, at most `limit` bytes of it. An answer
/// with another status becomes the error that names its code.
fn read_body(
    what: &'static str,
    result: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    limit: u64,
) -> Result<Vec<u8>, GmailError> {
    let mut response = result.map_err(|e| transport(what, &e))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let body: serde_json::Value = response
            .body_mut()
            .with_config()
            .limit(SMALL_RESPONSE)
            .read_json()
            .unwrap_or(serde_json::Value::Null);
        let code = error_code(&body);
        return Err(if code == "invalid_grant" {
            GmailError::SignInExpired
        } else {
            GmailError::Status { what, status, code }
        });
    }
    response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(|_| GmailError::Answer {
            what,
            reason: NOT_THE_JSON,
        })
}

fn read_json<T: serde::de::DeserializeOwned>(
    what: &'static str,
    result: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    limit: u64,
) -> Result<T, GmailError> {
    let body = read_body(what, result, limit)?;
    serde_json::from_slice(&body).map_err(|_| GmailError::Answer {
        what,
        reason: NOT_THE_JSON,
    })
}

fn random_token(bytes: usize) -> Result<String, GmailError> {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).map_err(|_| GmailError::Random)?;
    Ok(URL_SAFE_NO_PAD.encode(buffer))
}

/// One sign-in attempt: the PKCE verifier that proves the code exchange comes from the
/// process that asked, and the state that ties the pasted address to this attempt.
pub struct SignIn {
    verifier: SecretString,
    state: String,
}

/// The PKCE challenge of a verifier: the base64url of its SHA-256 (RFC 7636, S256).
fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

impl SignIn {
    /// Starts a sign-in attempt with a new verifier of 32 random bytes and a new state of 16
    /// random bytes, each as base64url text.
    ///
    /// # Errors
    ///
    /// [`GmailError::Random`] when the operating system gives no random bytes.
    pub fn begin() -> Result<Self, GmailError> {
        Ok(Self {
            verifier: SecretString::from(random_token(32)?),
            state: random_token(16)?,
        })
    }

    /// The address the user opens in a browser to give read-only access.
    ///
    /// # Panics
    ///
    /// When `endpoints.authorize` is not a valid URL. That cannot occur in `qm`: the endpoint
    /// is a constant of `Endpoints::default`, and in a test it is the address of a local
    /// server. Neither comes from outside data.
    pub fn authorization_url(&self, endpoints: &Endpoints, credentials: &Credentials) -> String {
        #[expect(
            clippy::expect_used,
            reason = "the authorization endpoint is in the `qm` binary a constant of `Endpoints::default`, and in a test the address of a local server; neither comes from outside data"
        )]
        let url = Url::parse_with_params(
            &endpoints.authorize,
            [
                ("client_id", credentials.client_id.as_str()),
                ("redirect_uri", REDIRECT_URI),
                ("response_type", "code"),
                ("scope", SCOPE),
                ("code_challenge", &challenge(self.verifier.expose_secret())),
                ("code_challenge_method", "S256"),
                ("state", &self.state),
                // A refresh token is given only with offline access and a fresh consent.
                ("access_type", "offline"),
                ("prompt", "consent"),
            ],
        )
        .expect("the authorization endpoint is a valid URL");
        url.into()
    }

    /// The authorization code in the address Google sent the browser to. The address must be
    /// the redirect of this attempt: same host and port, same state.
    ///
    /// # Errors
    ///
    /// - [`GmailError::Redirect`] when the pasted text is not an address, when its scheme,
    ///   host or port is not that of [`REDIRECT_URI`], when its `state` is not the state of
    ///   this attempt, or when it has no `code` or an empty one.
    /// - [`GmailError::Denied`] when the address carries an `error` parameter. This is
    ///   checked before the state.
    ///
    /// # Panics
    ///
    /// When [`REDIRECT_URI`] is not a valid URL. That cannot occur: it is a constant of the
    /// source.
    pub fn code_from_redirect(&self, pasted: &str) -> Result<SecretString, GmailError> {
        let url = Url::parse(pasted.trim())
            .map_err(|_| GmailError::Redirect("it is not an address; copy the whole line"))?;
        #[expect(
            clippy::expect_used,
            reason = "REDIRECT_URI is a constant of the source"
        )]
        let expected = Url::parse(REDIRECT_URI).expect("the redirect is a valid URL");
        if url.scheme() != expected.scheme()
            || url.host_str() != expected.host_str()
            || url.port() != expected.port()
        {
            return Err(GmailError::Redirect(
                "it does not start with http://127.0.0.1:8765",
            ));
        }
        let value = |name: &str| {
            url.query_pairs()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.into_owned())
        };
        if let Some(error) = value("error") {
            return Err(GmailError::Denied(error_code(&serde_json::json!({
                "error": error
            }))));
        }
        if value("state").as_deref() != Some(self.state.as_str()) {
            return Err(GmailError::Redirect(
                "it belongs to another sign-in attempt; start `qm gmail login` again",
            ));
        }
        value("code")
            .filter(|code| !code.is_empty())
            .map(SecretString::from)
            .ok_or(GmailError::Redirect("it carries no code"))
    }

    /// Trades the code for the refresh token. Blocking.
    ///
    /// # Errors
    ///
    /// - [`GmailError::Transport`] when the token endpoint gives no HTTP answer.
    /// - [`GmailError::SignInExpired`] when it answers with the error code `invalid_grant`.
    /// - [`GmailError::Status`] when it answers with another error.
    /// - [`GmailError::Answer`] when its answer is not the expected JSON, is larger than
    ///   1 MiB, or holds no refresh token.
    pub fn exchange(
        &self,
        agent: &ureq::Agent,
        endpoints: &Endpoints,
        credentials: &Credentials,
        code: &SecretString,
    ) -> Result<SecretString, GmailError> {
        #[derive(Deserialize)]
        struct Answer {
            refresh_token: Option<String>,
        }
        let what = "the sign-in";
        let answer: Answer = read_json(
            what,
            agent.post(&endpoints.token).send_form([
                ("grant_type", "authorization_code"),
                ("code", code.expose_secret()),
                ("code_verifier", self.verifier.expose_secret()),
                ("client_id", credentials.client_id.as_str()),
                ("client_secret", credentials.client_secret.expose_secret()),
                ("redirect_uri", REDIRECT_URI),
            ]),
            SMALL_RESPONSE,
        )?;
        answer
            .refresh_token
            .filter(|token| !token.is_empty())
            .map(SecretString::from)
            .ok_or(GmailError::Answer {
                what,
                reason: "no refresh token in it",
            })
    }
}

/// A fresh access token for the stored sign-in. Blocking.
///
/// # Errors
///
/// - [`GmailError::Transport`] when the token endpoint gives no HTTP answer.
/// - [`GmailError::SignInExpired`] when it answers with the error code `invalid_grant`: the
///   refresh token expired or was revoked.
/// - [`GmailError::Status`] when it answers with another error.
/// - [`GmailError::Answer`] when its answer is not JSON with an access token, or is larger
///   than 1 MiB.
pub fn access_token(
    agent: &ureq::Agent,
    endpoints: &Endpoints,
    credentials: &Credentials,
    refresh_token: &SecretString,
) -> Result<SecretString, GmailError> {
    #[derive(Deserialize)]
    struct Answer {
        access_token: String,
    }
    let answer: Answer = read_json(
        "the token refresh",
        agent.post(&endpoints.token).send_form([
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.expose_secret()),
            ("client_id", credentials.client_id.as_str()),
            ("client_secret", credentials.client_secret.expose_secret()),
        ]),
        SMALL_RESPONSE,
    )?;
    Ok(SecretString::from(answer.access_token))
}

/// The refresh token on disk: one JSON file that only its owner can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenFile {
    path: PathBuf,
}

#[derive(serde::Serialize, Deserialize)]
struct StoredToken {
    refresh_token: String,
}

impl TokenFile {
    /// The token file of a state directory: the file named [`TOKEN_FILE_NAME`] in it. Nothing
    /// is read or created here.
    pub fn in_dir(state_dir: &Path) -> Self {
        Self {
            path: state_dir.join(TOKEN_FILE_NAME),
        }
    }

    /// The path of the token file, whether or not the file exists.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn failed(&self, reason: &dyn std::fmt::Display) -> GmailError {
        GmailError::TokenFile {
            path: self.path.clone(),
            reason: reason.to_string(),
        }
    }

    /// Reads the refresh token that `qm gmail login` stored.
    ///
    /// # Errors
    ///
    /// - [`GmailError::NoToken`] when the file does not exist.
    /// - [`GmailError::TokenFile`] when the file cannot be read, with the kind of the file
    ///   system error, or when its content is not JSON with a refresh token. The error does
    ///   not quote the file.
    pub fn load(&self) -> Result<SecretString, GmailError> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(GmailError::NoToken(self.path.clone()));
            }
            Err(e) => return Err(self.failed(&e.kind())),
        };
        // A parse error would quote the file; say only that it is not readable.
        let stored: StoredToken = serde_json::from_str(&text)
            .map_err(|_| self.failed(&"not a token file; run `qm gmail login` again"))?;
        Ok(SecretString::from(stored.refresh_token))
    }

    /// Writes the token to a new file with mode 0600 and moves it into place, so the token is
    /// never in a file that others can read and a failed write leaves the old file.
    ///
    /// # Errors
    ///
    /// [`GmailError::TokenFile`], with the kind of the file system error, when the new file
    /// cannot be created (the state directory does not exist, for example), written, flushed
    /// to disk or moved into place.
    pub fn save(&self, refresh_token: &SecretString) -> Result<(), GmailError> {
        use std::os::unix::fs::OpenOptionsExt;

        let text = serde_json::to_string(&StoredToken {
            refresh_token: refresh_token.expose_secret().to_owned(),
        })
        .map_err(|_| self.failed(&"cannot be written"))?;
        let partial = self.path.with_extension("json.partial");
        // A partial file from an interrupted run is replaced.
        let _ = std::fs::remove_file(&partial);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&partial)
            .map_err(|e| self.failed(&format!("{}; does the state directory exist?", e.kind())))?;
        file.write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|e| self.failed(&e.kind()))?;
        std::fs::rename(&partial, &self.path).map_err(|e| self.failed(&e.kind()))
    }
}

/// The Gmail search for the mail of the platform senders, received after a moment. Gmail
/// reads a number after `after:` as seconds since 1970, so no time zone is involved.
pub fn search_query(senders: &[String], after: Option<Timestamp>) -> String {
    // A sender address is configuration; anything but the characters of an address is left
    // out so that it cannot add an operator to the search.
    let addresses: Vec<String> = senders
        .iter()
        .map(|sender| {
            sender
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || "@.-_+".contains(*c))
                .collect::<String>()
        })
        .filter(|address| !address.is_empty())
        .collect();
    let mut query = format!("from:({})", addresses.join(" OR "));
    if let Some(after) = after {
        query.push_str(&format!(" after:{}", after.as_second().max(0)));
    }
    query
}

/// The mailbox behind the Gmail API, for one search.
#[derive(Clone)]
pub struct GmailApi {
    agent: ureq::Agent,
    api: String,
    access_token: SecretString,
    query: String,
    max_messages: usize,
}

#[derive(Deserialize)]
struct MessageRef {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    #[serde(default)]
    messages: Vec<MessageRef>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct RawMessage<'a> {
    // Borrowed from the body when the text has no escape, so a large message is not copied.
    #[serde(borrow)]
    raw: std::borrow::Cow<'a, str>,
}

/// Gmail writes the raw message as base64url; padding is accepted either way.
const RAW_ENCODING: GeneralPurpose = GeneralPurpose::new(
    &base64::alphabet::URL_SAFE,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// One page of the message list, as Gmail answered it and after the check of each id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListPage {
    /// The ids of the messages of this page, in the order of the answer. Each is 1 to 64
    /// ASCII letters and digits, so it can be a part of a request path.
    pub ids: Vec<String>,
    /// The token that asks for the next page; `None` on the last page.
    pub next_page_token: Option<String>,
}

/// Reads the body of one answer of the message list. The body is outside data: Gmail wrote
/// it, and what it holds comes from the mailbox.
///
/// # Errors
///
/// [`GmailError::Answer`] when the body is not the JSON of a page, or when an id is empty,
/// longer than 64 characters, or holds a character that is not an ASCII letter or digit.
pub fn list_page_from_json(body: &[u8]) -> Result<ListPage, GmailError> {
    let what = "the message list";
    let page: Page = serde_json::from_slice(body).map_err(|_| GmailError::Answer {
        what,
        reason: NOT_THE_JSON,
    })?;
    let mut ids = Vec::with_capacity(page.messages.len());
    for message in page.messages {
        // The id becomes part of a path; Gmail's ids are hexadecimal.
        let id_ok = !message.id.is_empty()
            && message.id.len() <= 64
            && message.id.bytes().all(|b| b.is_ascii_alphanumeric());
        if !id_ok {
            return Err(GmailError::Answer {
                what,
                reason: "a message id that is not letters and digits",
            });
        }
        ids.push(message.id);
    }
    Ok(ListPage {
        ids,
        next_page_token: page.next_page_token,
    })
}

/// Reads the body of one answer for a message in raw form and gives the bytes of the mail.
///
/// # Errors
///
/// [`GmailError::Answer`] when the body is not JSON with a `raw` text, or when that text is
/// not base64url.
pub fn raw_message_from_json(body: &[u8]) -> Result<Vec<u8>, GmailError> {
    let what = "a message";
    let message: RawMessage<'_> = serde_json::from_slice(body).map_err(|_| GmailError::Answer {
        what,
        reason: NOT_THE_JSON,
    })?;
    RAW_ENCODING
        .decode(message.raw.as_bytes())
        .map_err(|_| GmailError::Answer {
            what,
            reason: "the raw message is not base64url",
        })
}

impl GmailApi {
    /// A source for one fetch. `access_token` comes from [`access_token`] and `query` from
    /// [`search_query`]. Of `endpoints` only the API address is used. `max_messages` is the
    /// most messages that one run reads (`qm fetch --max`): when more match the search, the
    /// run stops there and the log has a warning. Nothing is sent to Google until the use
    /// case asks for the messages.
    pub fn new(
        agent: ureq::Agent,
        endpoints: &Endpoints,
        access_token: SecretString,
        query: String,
        max_messages: usize,
    ) -> Self {
        Self {
            agent,
            api: endpoints.api.clone(),
            access_token,
            query,
            max_messages,
        }
    }

    /// The body of one answer of the API, at most `limit` bytes.
    fn get(
        &self,
        what: &'static str,
        path: &str,
        query: &[(&str, &str)],
        limit: u64,
    ) -> Result<Vec<u8>, GmailError> {
        let request = self
            .agent
            .get(format!("{}{path}", self.api))
            .header(
                "Authorization",
                format!("Bearer {}", self.access_token.expose_secret()),
            )
            .query_pairs(query.iter().copied());
        read_body(what, request.call(), limit)
    }

    /// Lists the search page by page and sends each message raw, newest first as Gmail lists
    /// them. Stops at `max_messages`, and when the receiver is gone.
    fn send_all(&self, tx: &mpsc::Sender<Result<RawMail, MailError>>) -> Result<(), GmailError> {
        let mut page_token: Option<String> = None;
        let mut sent = 0usize;
        loop {
            let mut query = vec![("q", self.query.as_str()), ("maxResults", PAGE_SIZE)];
            if let Some(token) = &page_token {
                query.push(("pageToken", token.as_str()));
            }
            let page = list_page_from_json(&self.get(
                "the message list",
                "/gmail/v1/users/me/messages",
                &query,
                SMALL_RESPONSE,
            )?)?;
            for id in &page.ids {
                if sent == self.max_messages {
                    tracing::warn!(
                        limit = self.max_messages,
                        "more messages match than one run reads; run `qm fetch` again or raise --max"
                    );
                    return Ok(());
                }
                let bytes = raw_message_from_json(&self.get(
                    "a message",
                    &format!("/gmail/v1/users/me/messages/{id}"),
                    &[("format", "raw")],
                    MESSAGE_RESPONSE,
                )?)?;
                if tx.blocking_send(Ok(RawMail { bytes })).is_err() {
                    // The use case stopped; there is nobody to read for.
                    return Ok(());
                }
                // `sent` is below `max_messages` here: the loop stops when the two are equal.
                sent = sent.saturating_add(1);
            }
            match page.next_page_token {
                Some(token) if !page.ids.is_empty() => page_token = Some(token),
                _ => return Ok(()),
            }
        }
    }
}

impl MailSource for GmailApi {
    fn messages(&self) -> MailStream {
        let (tx, rx) = mpsc::channel(CHANNEL_BOUND);
        let source = self.clone();
        let reader_tx = tx.clone();
        let reader = tokio::task::spawn_blocking(move || {
            if let Err(error) = source.send_all(&reader_tx) {
                // A send that fails here means the use case already stopped.
                let _ = reader_tx.blocking_send(Err(MailError::Read(error.to_string())));
            }
        });
        tokio::spawn(async move {
            // A fixed text: the text of a panic comes from a library and is not ours to vouch for.
            if reader.await.is_err() {
                let _ = tx
                    .send(Err(MailError::Read(
                        "the reader of the mailbox stopped unexpectedly; nothing more was read"
                            .to_owned(),
                    )))
                    .await;
            }
        });
        rx
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    use super::*;

    fn credentials() -> Credentials {
        Credentials {
            client_id: "client-1.apps.example".to_owned(),
            client_secret: SecretString::from("client-secret-value"),
        }
    }

    /// One request as the local server saw it.
    #[derive(Debug, Clone)]
    struct Seen {
        target: String,
        authorization: Option<String>,
        body: String,
    }

    /// A local HTTP server for the adapter to call: answers each request with what `answer`
    /// returns for it (status, JSON), and records what it saw.
    fn serve(
        answer: impl Fn(&Seen) -> (u16, String) + Send + 'static,
    ) -> (Endpoints, Arc<Mutex<Vec<Seen>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("address"));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                    continue;
                }
                let target = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_owned();
                let (mut authorization, mut length) = (None, 0usize);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':') {
                        match name.to_ascii_lowercase().as_str() {
                            "authorization" => authorization = Some(value.trim().to_owned()),
                            "content-length" => length = value.trim().parse().unwrap_or(0),
                            _ => {}
                        }
                    }
                }
                let mut body = vec![0u8; length];
                reader.read_exact(&mut body).expect("body");
                let request = Seen {
                    target,
                    authorization,
                    body: String::from_utf8_lossy(&body).into_owned(),
                };
                let (status, json) = answer(&request);
                log.lock().expect("log").push(request);
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
                    json.len()
                );
            }
        });
        let endpoints = Endpoints {
            authorize: format!("{base}/authorize"),
            token: format!("{base}/token"),
            api: base,
        };
        (endpoints, seen)
    }

    #[test]
    fn the_challenge_is_the_one_of_rfc_7636() {
        // RFC 7636, appendix B.
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn the_authorization_address_asks_for_read_only_offline_access_with_pkce() {
        let sign_in = SignIn::begin().expect("random");
        let url = Url::parse(&sign_in.authorization_url(&Endpoints::default(), &credentials()))
            .expect("url");
        assert_eq!(url.host_str(), Some("accounts.google.com"));
        let get = |name: &str| {
            url.query_pairs()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.into_owned())
                .expect("present")
        };
        assert_eq!(get("scope"), SCOPE);
        assert_eq!(get("redirect_uri"), REDIRECT_URI);
        assert_eq!(get("client_id"), "client-1.apps.example");
        assert_eq!(get("response_type"), "code");
        assert_eq!(get("access_type"), "offline");
        assert_eq!(get("code_challenge_method"), "S256");
        assert_eq!(
            get("code_challenge"),
            challenge(sign_in.verifier.expose_secret())
        );
        assert_eq!(get("state"), sign_in.state);
        // The verifier and the secret are not in the address.
        assert!(!url.as_str().contains(sign_in.verifier.expose_secret()));
        assert!(!url.as_str().contains("client-secret-value"));
        // Two attempts do not share a state or a verifier.
        let other = SignIn::begin().expect("random");
        assert_ne!(other.state, sign_in.state);
        assert_ne!(
            other.verifier.expose_secret(),
            sign_in.verifier.expose_secret()
        );
        assert_eq!(sign_in.verifier.expose_secret().len(), 43);
    }

    #[test]
    fn the_pasted_address_must_be_the_redirect_of_this_attempt() {
        let sign_in = SignIn::begin().expect("random");
        let state = &sign_in.state;
        let code = sign_in
            .code_from_redirect(&format!(
                "  http://127.0.0.1:8765/?state={state}&code=4%2FabcDEF&scope=x \n"
            ))
            .expect("code");
        assert_eq!(code.expose_secret(), "4/abcDEF");

        let refused = |pasted: &str| {
            sign_in
                .code_from_redirect(pasted)
                .map(|_| ())
                .expect_err("refused")
                .to_string()
        };
        assert!(refused("4/abcDEF").contains("not an address"));
        assert!(
            refused(&format!("https://evil.example/?state={state}&code=x"))
                .contains("does not start with")
        );
        assert!(
            refused(&format!("http://127.0.0.1:9999/?state={state}&code=x"))
                .contains("does not start with")
        );
        assert!(refused("http://127.0.0.1:8765/?state=other&code=x").contains("another sign-in"));
        assert!(refused(&format!("http://127.0.0.1:8765/?state={state}")).contains("no code"));
        // A refusal on the consent page arrives as an error code; only the code is kept.
        assert_eq!(
            refused("http://127.0.0.1:8765/?error=access_denied<script>&state=x"),
            "Google did not give access: access_deniedscript"
        );
    }

    #[test]
    fn the_pasted_address_is_refused_when_one_of_its_three_parts_is_another() {
        // The scheme, the host and the port are each checked: an address that differs in one
        // of them only is not the redirect.
        let sign_in = SignIn::begin().expect("random");
        let state = &sign_in.state;
        for start in [
            "https://127.0.0.1:8765",
            "http://localhost:8765",
            "http://127.0.0.2:8765",
            "http://127.0.0.1:8766",
            "http://127.0.0.1",
        ] {
            let result = sign_in.code_from_redirect(&format!("{start}/?state={state}&code=x"));
            assert!(
                matches!(
                    result,
                    Err(GmailError::Redirect(
                        "it does not start with http://127.0.0.1:8765"
                    ))
                ),
                "{start}"
            );
        }
    }

    #[test]
    fn a_failed_call_gets_a_fixed_text_for_its_kind_and_never_the_text_of_the_client() {
        let reason = |error: ureq::Error| {
            let GmailError::Transport { what, reason } = transport("the message list", &error)
            else {
                panic!("a failed call is a transport error");
            };
            assert_eq!(what, "the message list");
            reason
        };
        assert_eq!(
            reason(ureq::Error::RedirectFailed),
            "a redirect, which is not followed"
        );
        assert_eq!(
            reason(ureq::Error::TooManyRedirects),
            "a redirect, which is not followed"
        );
        assert_eq!(
            reason(ureq::Error::HostNotFound),
            "the host name cannot be resolved; is the network up?"
        );
        assert_eq!(
            reason(ureq::Error::Timeout(ureq::Timeout::Global)),
            "the call timed out"
        );
        assert_eq!(
            reason(ureq::Error::Io(std::io::Error::other(
                "https://secret.example/?token=1"
            ))),
            "the connection failed"
        );
        assert_eq!(reason(ureq::Error::ConnectionFailed), "the request failed");
    }

    #[test]
    fn the_size_limits_of_an_answer_are_one_and_sixty_four_mebibytes() {
        // Written here a second time: a limit that changes fails this test.
        assert_eq!(SMALL_RESPONSE, 1_048_576);
        assert_eq!(MESSAGE_RESPONSE, 67_108_864);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_page_without_a_message_ends_the_list_whatever_token_it_has() {
        // A page that names a next page and holds no message would be asked for again and
        // again. The second answer is there so that a reader that asks for it ends too.
        let (endpoints, seen) = serve(|request| {
            let t = &request.target;
            let json = if t.starts_with("/gmail/v1/users/me/messages?") && !t.contains("pageToken")
            {
                r#"{"nextPageToken":"p2","resultSizeEstimate":0}"#.to_owned()
            } else if t.starts_with("/gmail/v1/users/me/messages?") {
                r#"{"messages":[{"id":"18z"}]}"#.to_owned()
            } else {
                format!(
                    r#"{{"raw":"{}"}}"#,
                    URL_SAFE_NO_PAD.encode("Subject: never\r\n\r\n")
                )
            };
            (200, json)
        });
        let source = GmailApi::new(
            agent(false),
            &endpoints,
            SecretString::from("access-1"),
            search_query(&["service@freshdirect.com".to_owned()], None),
            10,
        );
        assert!(collect(&source).await.is_empty());
        assert_eq!(seen.lock().expect("log").len(), 1);
    }

    #[test]
    fn the_search_names_the_senders_and_the_moment_and_nothing_else() {
        let senders = [
            "service@freshdirect.com".to_owned(),
            "orders@eat.seamless.com) +OR in:anywhere (".to_owned(),
        ];
        let after = Timestamp::from_second(1_795_000_000).ok();
        assert_eq!(
            search_query(&senders, after),
            "from:(service@freshdirect.com OR orders@eat.seamless.com+ORinanywhere) after:1795000000"
        );
        assert_eq!(
            search_query(&senders[..1], None),
            "from:(service@freshdirect.com)"
        );
    }

    #[test]
    fn a_token_file_that_is_there_and_cannot_be_read_is_not_a_missing_token() {
        // A directory in the place of the file: the answer names the kind of the error of
        // the file system, and does not tell the user to sign in again.
        let dir = tempfile::tempdir().expect("dir");
        let file = TokenFile::in_dir(dir.path());
        std::fs::create_dir(dir.path().join(TOKEN_FILE_NAME)).expect("a directory");
        let result = file.load();
        assert!(
            matches!(result, Err(GmailError::TokenFile { .. })),
            "{:?}",
            result.map(|_| ())
        );
        // The control: with nothing there, the token is missing.
        let empty = tempfile::tempdir().expect("dir");
        assert!(matches!(
            TokenFile::in_dir(empty.path()).load(),
            Err(GmailError::NoToken(_))
        ));
    }

    #[test]
    fn the_token_file_is_private_and_round_trips() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("dir");
        let file = TokenFile::in_dir(dir.path());
        assert!(matches!(file.load(), Err(GmailError::NoToken(_))));
        file.save(&SecretString::from("refresh-1")).expect("save");
        file.save(&SecretString::from("refresh-2"))
            .expect("save again");
        assert_eq!(file.load().expect("load").expose_secret(), "refresh-2");
        let mode = std::fs::metadata(file.path())
            .expect("meta")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(std::fs::read_dir(dir.path()).expect("list").count(), 1);

        // A file that is not a token file is refused without quoting it.
        std::fs::write(
            file.path(),
            "{\"refresh_token\": 5, \"note\": \"private text\"}",
        )
        .expect("write");
        let error = file.load().map(|_| ()).expect_err("refused").to_string();
        assert!(error.contains("not a token file") && !error.contains("private text"));

        // A missing state directory is said, not a panic.
        let missing = TokenFile::in_dir(&dir.path().join("absent"));
        let error = missing
            .save(&SecretString::from("r"))
            .expect_err("refused")
            .to_string();
        assert!(error.contains("does the state directory exist"), "{error}");
    }

    #[test]
    fn the_sign_in_and_the_refresh_send_the_right_form_and_keep_secrets_out_of_errors() {
        let (endpoints, seen) = serve(|request| {
            if request.body.contains("grant_type=authorization_code") {
                (
                    200,
                    r#"{"access_token":"a1","refresh_token":"r1"}"#.to_owned(),
                )
            } else if request.body.contains("refresh_token=r1") {
                (200, r#"{"access_token":"a2","expires_in":3599}"#.to_owned())
            } else {
                (
                    400,
                    r#"{"error":"invalid_grant","error_description":"Token r-old was revoked"}"#
                        .to_owned(),
                )
            }
        });
        let agent = agent(false);
        let sign_in = SignIn::begin().expect("random");
        let refresh = sign_in
            .exchange(
                &agent,
                &endpoints,
                &credentials(),
                &SecretString::from("the-code"),
            )
            .expect("exchange");
        assert_eq!(refresh.expose_secret(), "r1");
        let access = access_token(&agent, &endpoints, &credentials(), &refresh).expect("refresh");
        assert_eq!(access.expose_secret(), "a2");

        let seen_now = seen.lock().expect("log").clone();
        assert_eq!(seen_now[0].target, "/token");
        for part in [
            "code=the-code",
            "client_id=client-1.apps.example",
            "client_secret=client-secret-value",
            "redirect_uri=http%3A%2F%2F127.0.0.1%3A8765",
        ] {
            assert!(seen_now[0].body.contains(part), "{part}");
        }
        assert!(seen_now[0].body.contains(&format!(
            "code_verifier={}",
            sign_in.verifier.expose_secret()
        )));
        assert!(seen_now[1].body.contains("grant_type=refresh_token"));

        // A revoked sign-in says what to do, and nothing of Google's text or the token.
        let error = access_token(
            &agent,
            &endpoints,
            &credentials(),
            &SecretString::from("r-old"),
        )
        .map(|_| ())
        .expect_err("revoked")
        .to_string();
        assert!(error.contains("qm gmail login"), "{error}");
        assert!(!error.contains("r-old") && !error.contains("revoked\""));
    }

    async fn collect(source: &GmailApi) -> Vec<Result<Vec<u8>, String>> {
        let mut stream = source.messages();
        let mut out = Vec::new();
        while let Some(item) = stream.recv().await {
            out.push(item.map(|m| m.bytes).map_err(|e| e.to_string()));
        }
        out
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_token_with_a_line_break_sends_no_request() {
        // An HTTP header is an output context (REAS-SEC-006): a line break in a value would
        // start a header of its own. The token is outside data, an answer of the token
        // endpoint. The HTTP library refuses the value, so nothing goes to the network.
        let (endpoints, seen) = serve(|_| (200, r#"{"messages":[]}"#.to_owned()));
        let query = search_query(&["service@freshdirect.com".to_owned()], None);
        let source = |token: &str| {
            GmailApi::new(
                agent(false),
                &endpoints,
                SecretString::from(token.to_owned()),
                query.clone(),
                10,
            )
        };
        let got = collect(&source("access-1\r\nX-Injected: 1")).await;
        assert_eq!(got.len(), 1, "{got:?}");
        assert!(got[0].is_err(), "{got:?}");
        assert!(seen.lock().expect("log").is_empty());

        // The control: the same call with a plain token reaches the server.
        assert!(collect(&source("access-1")).await.is_empty());
        let seen = seen.lock().expect("log");
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].authorization.as_deref(), Some("Bearer access-1"));
    }

    #[test]
    fn a_page_of_the_list_is_read_and_an_id_that_is_no_id_is_refused() {
        assert_eq!(
            list_page_from_json(
                br#"{"messages":[{"id":"18c2a","threadId":"t"},{"id":"Zz09"}],"nextPageToken":"n1"}"#
            )
            .expect("a page"),
            ListPage {
                ids: vec!["18c2a".to_owned(), "Zz09".to_owned()],
                next_page_token: Some("n1".to_owned()),
            }
        );
        // The last page has no list and no token.
        assert_eq!(
            list_page_from_json(br#"{"resultSizeEstimate":0}"#).expect("a page"),
            ListPage {
                ids: Vec::new(),
                next_page_token: None,
            }
        );
        let too_long = format!(r#"{{"messages":[{{"id":"{}"}}]}}"#, "a".repeat(65));
        let longest = format!(r#"{{"messages":[{{"id":"{}"}}]}}"#, "a".repeat(64));
        assert!(list_page_from_json(longest.as_bytes()).is_ok());
        for body in [
            r#"{"messages":[{"id":""}]}"#,
            r#"{"messages":[{"id":"../settings"}]}"#,
            r#"{"messages":[{"id":"18c2a?x"}]}"#,
            r#"{"messages":[{"id":"18c 2a"}]}"#,
            r#"{"messages":[{"id":"18c2\u00e9"}]}"#,
            too_long.as_str(),
        ] {
            let error = list_page_from_json(body.as_bytes()).expect_err(body);
            assert_eq!(
                error.to_string(),
                "Google's answer to the message list cannot be read: a message id that is not letters and digits",
                "{body}"
            );
        }
        for body in [
            "",
            "[]",
            "null",
            r#"{"messages":{}}"#,
            r#"{"messages":[{}]}"#,
            "{",
        ] {
            let error = list_page_from_json(body.as_bytes()).expect_err(body);
            assert!(
                error
                    .to_string()
                    .ends_with("not the expected JSON, or larger than the limit"),
                "{body}: {error}"
            );
        }
    }

    #[test]
    fn a_raw_message_is_decoded_with_and_without_padding_and_other_text_is_refused() {
        for body in [
            r#"{"raw":"aGk="}"#,
            r#"{"id":"1","raw":"aGk"}"#,
            r#"{"raw":"a\u0047k"}"#,
        ] {
            assert_eq!(
                raw_message_from_json(body.as_bytes()).expect(body),
                b"hi",
                "{body}"
            );
        }
        // The alphabet is the one for addresses: `-` and `_`, not `+` and `/`.
        assert_eq!(
            raw_message_from_json(br#"{"raw":"-_8"}"#).expect("url alphabet"),
            [0xfb, 0xff]
        );
        for body in [
            r#"{"raw":"+/8"}"#,
            r#"{"raw":"aG k"}"#,
            r#"{"raw":"a"}"#,
            r#"{"raw":"aGl="}"#,
        ] {
            let error = raw_message_from_json(body.as_bytes()).expect_err(body);
            assert!(
                error
                    .to_string()
                    .ends_with("the raw message is not base64url"),
                "{body}: {error}"
            );
        }
        for body in ["", "{}", r#"{"raw":5}"#, r#"{"raw":null}"#, "[]"] {
            let error = raw_message_from_json(body.as_bytes()).expect_err(body);
            assert!(
                error
                    .to_string()
                    .ends_with("not the expected JSON, or larger than the limit"),
                "{body}: {error}"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_fetch_pages_through_the_list_and_reads_each_message_raw() {
        let raw = |text: &str| URL_SAFE_NO_PAD.encode(text);
        let (padded, plain) = (
            base64::engine::general_purpose::URL_SAFE.encode("Subject: two\r\n\r\n?>"),
            raw("Subject: one\r\n\r\nbody"),
        );
        let (endpoints, seen) = serve(move |request| {
            let t = &request.target;
            let json = if t.starts_with("/gmail/v1/users/me/messages?") && !t.contains("pageToken")
            {
                r#"{"messages":[{"id":"18a","threadId":"t"},{"id":"18b"}],"nextPageToken":"p2"}"#
                    .to_owned()
            } else if t.starts_with("/gmail/v1/users/me/messages?") {
                r#"{"messages":[{"id":"18c"}],"resultSizeEstimate":3}"#.to_owned()
            } else if t.starts_with("/gmail/v1/users/me/messages/18a?") {
                format!(r#"{{"id":"18a","raw":"{plain}"}}"#)
            } else if t.starts_with("/gmail/v1/users/me/messages/18b?") {
                format!(r#"{{"id":"18b","raw":"{padded}"}}"#)
            } else {
                format!(r#"{{"raw":"{}"}}"#, raw("Subject: three\r\n\r\n"))
            };
            (200, json)
        });
        let query = search_query(&["service@freshdirect.com".to_owned()], None);
        let source = GmailApi::new(
            agent(false),
            &endpoints,
            SecretString::from("access-1"),
            query,
            10,
        );
        let got = collect(&source).await;
        let texts: Vec<String> = got
            .into_iter()
            .map(|m| String::from_utf8(m.expect("message")).expect("utf-8"))
            .collect();
        assert_eq!(
            texts,
            [
                "Subject: one\r\n\r\nbody",
                "Subject: two\r\n\r\n?>",
                "Subject: three\r\n\r\n"
            ]
        );
        let seen_now = seen.lock().expect("log").clone();
        assert_eq!(seen_now.len(), 5);
        assert!(
            seen_now
                .iter()
                .all(|r| r.authorization.as_deref() == Some("Bearer access-1"))
        );
        // The search is the `q` parameter, whatever the client escapes in it.
        let list = Url::parse(&format!("http://local{}", seen_now[0].target)).expect("url");
        assert!(
            list.query_pairs()
                .any(|(k, v)| k == "q" && v == "from:(service@freshdirect.com)")
        );
        assert!(seen_now[0].target.contains("maxResults=100"));
        assert!(seen_now[1].target.ends_with("/18a?format=raw"));
        assert!(seen_now[3].target.contains("pageToken=p2"));

        // The limit of a run stops the reading; what was read is delivered.
        let limited = GmailApi {
            max_messages: 1,
            ..source
        };
        assert_eq!(collect(&limited).await.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_refusal_and_a_bad_answer_arrive_as_one_error_without_the_token() {
        let (endpoints, _) = serve(|request| {
            if request.authorization.as_deref() == Some("Bearer expired") {
                (
                    401,
                    r#"{"error":{"code":401,"message":"Request had invalid credentials for Bearer expired","status":"UNAUTHENTICATED"}}"#
                        .to_owned(),
                )
            } else if request.target.contains("messages?") {
                (200, r#"{"messages":[{"id":"../../settings"}]}"#.to_owned())
            } else {
                (200, "{}".to_owned())
            }
        });
        let source = |token: &str| {
            GmailApi::new(
                agent(false),
                &endpoints,
                SecretString::from(token),
                "from:(a@b.example)".to_owned(),
                10,
            )
        };
        let got = collect(&source("expired")).await;
        assert_eq!(
            got,
            [Err(
                "cannot read the mail source: Google answered the message list with status 401 (UNAUTHENTICATED)"
                    .to_owned()
            )]
        );
        // An id that could leave the messages path is refused before it is used.
        let got = collect(&source("good")).await;
        assert_eq!(got.len(), 1);
        assert!(
            got[0]
                .as_ref()
                .expect_err("refused")
                .contains("not letters and digits")
        );

        // The real client refuses plain HTTP before anything is sent.
        let strict = GmailApi {
            agent: agent(true),
            ..source("good")
        };
        let got = collect(&strict).await;
        assert!(
            got[0]
                .as_ref()
                .expect_err("refused")
                .contains("cannot reach Google")
        );
    }
}
