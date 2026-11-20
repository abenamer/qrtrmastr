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

#[derive(Debug, Error)]
pub enum GmailError {
    #[error(
        "Gmail is not set up: QM_GMAIL_CLIENT_ID and QM_GMAIL_CLIENT_SECRET are not set. \
         How to create them: docs/runbooks/gmail-fetch.md"
    )]
    NoCredentials,
    #[error("no Gmail sign-in is stored at {0}; run `qm gmail login` first")]
    NoToken(PathBuf),
    #[error("the token file {path}: {reason}")]
    TokenFile { path: PathBuf, reason: String },
    #[error("the pasted address cannot be used: {0}")]
    Redirect(&'static str),
    #[error("Google did not give access: {0}")]
    Denied(String),
    #[error(
        "the stored Gmail sign-in is no longer valid (expired or revoked); run `qm gmail login` again"
    )]
    SignInExpired,
    #[error("Google answered {what} with status {status} ({code})")]
    Status {
        what: &'static str,
        status: u16,
        code: String,
    },
    #[error("cannot reach Google for {what}: {reason}")]
    Transport {
        what: &'static str,
        reason: &'static str,
    },
    #[error("Google's answer to {what} cannot be read: {reason}")]
    Answer {
        what: &'static str,
        reason: &'static str,
    },
    #[error("the system gave no random numbers")]
    Random,
}

/// The three addresses the adapter talks to. Google's by default; a test names a local server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    pub authorize: String,
    pub token: String,
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
    pub client_id: String,
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

fn read_json<T: serde::de::DeserializeOwned>(
    what: &'static str,
    result: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    limit: u64,
) -> Result<T, GmailError> {
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
        .read_json()
        .map_err(|_| GmailError::Answer {
            what,
            reason: "not the expected JSON, or larger than the limit",
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
    pub fn begin() -> Result<Self, GmailError> {
        Ok(Self {
            verifier: SecretString::from(random_token(32)?),
            state: random_token(16)?,
        })
    }

    /// The address the user opens in a browser to give read-only access.
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
    pub fn in_dir(state_dir: &Path) -> Self {
        Self {
            path: state_dir.join(TOKEN_FILE_NAME),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn failed(&self, reason: &dyn std::fmt::Display) -> GmailError {
        GmailError::TokenFile {
            path: self.path.clone(),
            reason: reason.to_string(),
        }
    }

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
struct RawMessage {
    raw: String,
}

/// Gmail writes the raw message as base64url; padding is accepted either way.
const RAW_ENCODING: GeneralPurpose = GeneralPurpose::new(
    &base64::alphabet::URL_SAFE,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

impl GmailApi {
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

    fn get<T: serde::de::DeserializeOwned>(
        &self,
        what: &'static str,
        path: &str,
        query: &[(&str, &str)],
        limit: u64,
    ) -> Result<T, GmailError> {
        let request = self
            .agent
            .get(format!("{}{path}", self.api))
            .header(
                "Authorization",
                format!("Bearer {}", self.access_token.expose_secret()),
            )
            .query_pairs(query.iter().copied());
        read_json(what, request.call(), limit)
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
            let page: Page = self.get(
                "the message list",
                "/gmail/v1/users/me/messages",
                &query,
                SMALL_RESPONSE,
            )?;
            for message in &page.messages {
                if sent == self.max_messages {
                    tracing::warn!(
                        limit = self.max_messages,
                        "more messages match than one run reads; run `qm fetch` again or raise --max"
                    );
                    return Ok(());
                }
                // The id becomes part of a path; Gmail's ids are hexadecimal.
                let id_ok = !message.id.is_empty()
                    && message.id.len() <= 64
                    && message.id.bytes().all(|b| b.is_ascii_alphanumeric());
                if !id_ok {
                    return Err(GmailError::Answer {
                        what: "the message list",
                        reason: "a message id that is not letters and digits",
                    });
                }
                let raw: RawMessage = self.get(
                    "a message",
                    &format!("/gmail/v1/users/me/messages/{}", message.id),
                    &[("format", "raw")],
                    MESSAGE_RESPONSE,
                )?;
                let bytes =
                    RAW_ENCODING
                        .decode(raw.raw.as_bytes())
                        .map_err(|_| GmailError::Answer {
                            what: "a message",
                            reason: "the raw message is not base64url",
                        })?;
                if tx.blocking_send(Ok(RawMail { bytes })).is_err() {
                    // The use case stopped; there is nobody to read for.
                    return Ok(());
                }
                sent += 1;
            }
            match page.next_page_token {
                Some(token) if !page.messages.is_empty() => page_token = Some(token),
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
