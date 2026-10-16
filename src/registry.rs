//! Talking to a Godspeed registry: which one, with which credentials, over its protocol.
//!
//! The CLI speaks the registry PROTOCOL (`docs/publishing-design.md`), never a storage provider's API. It
//! holds a publisher token and nothing else - no storage credentials, ever.
//!
//! Every answer is one of three things, and they are never merged: the registry did it, the registry
//! refused (with a stable error code), or the registry could not be reached or failed - in which case
//! nobody knows yet whether it happened, and the caller must not say it did or did not.

use crate::diag::{Diagnostic, Kind, Stage};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

/// The official Godspeed registry: used when nothing names another. A default only - a package names no
/// registry, and `--registry`, `GODSPEED_REGISTRY` and `login` choose any other.
///
/// The address is temporary (it moves to a custom domain); when it moves, this changes in a release, and the
/// old address keeps answering meanwhile.
pub const OFFICIAL: &str = "https://godspeed-registry.bankole-422.workers.dev";
/// The digest of the official registry's root 1: on first contact with OFFICIAL, its root is accepted only if
/// it has this digest, as if `--trust-root` had been given. Later roots are each verified by the one before.
pub const OFFICIAL_ROOT: &str = "sha256:d769d1a64c3105c8da0ffcc2f54aba1c3a16ce4ae707a6d206556f08824a146d";

/// The registry to use when `--registry` is not given.
pub const REGISTRY_ENV: &str = "GODSPEED_REGISTRY";
/// A publisher token, for CI: used instead of the stored one, never written anywhere.
pub const TOKEN_ENV: &str = "GODSPEED_TOKEN";
/// Where the CLI keeps its configuration, overriding the platform default (used by tests).
pub const CONFIG_ENV: &str = "GODSPEED_CONFIG_DIR";

// ---- configuration and credentials ------------------------------------------------------------------

/// `~/.config/godspeed` (or `$XDG_CONFIG_HOME/godspeed`), or `%APPDATA%\godspeed` on Windows.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os(CONFIG_ENV).filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d));
    }
    if cfg!(windows) {
        return std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("godspeed"));
    }
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME").filter(|x| !x.is_empty()) {
        return Some(PathBuf::from(x).join("godspeed"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("godspeed"))
}

/// The stored credentials: one token per registry URL, and which registry is the default.
///
/// Kept in `credentials.toml` in the config directory, readable by the user alone. (An operating-system
/// credential store would be better on Windows and macOS; on Linux it needs D-Bus and a C library, which the
/// static builds cannot carry. A plain file is what Cargo does too. This is a candidate for improvement.)
#[derive(Debug, Default, serde::Serialize, Deserialize)]
pub struct Credentials {
    pub default: Option<String>,
    #[serde(default)]
    pub registries: BTreeMap<String, Entry>,
}

#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct Entry {
    pub token: String,
}

fn credentials_path() -> Result<PathBuf, Diagnostic> {
    config_dir().map(|d| d.join("credentials.toml")).ok_or_else(|| {
        Diagnostic::new(
            Kind::Environment,
            Stage::Authentication,
            "find where to keep credentials",
            "Neither HOME nor APPDATA is set, so there is no configuration directory.",
        )
        .next(format!("Set {CONFIG_ENV} to a directory only you can read."))
    })
}

impl Credentials {
    pub fn load() -> Result<Credentials, Diagnostic> {
        let path = credentials_path()?;
        match std::fs::read_to_string(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Credentials::default()),
            Err(e) => Err(Diagnostic::io("read stored credentials", &path, &e)),
            Ok(text) => toml::from_str(&text).map_err(|e| {
                Diagnostic::new(
                    Kind::Environment,
                    Stage::Authentication,
                    "read stored credentials",
                    format!("{} is not valid: {}", path.display(), e.message()),
                )
                .next("Delete the file and run `godspeed login` again.")
            }),
        }
    }

    /// Write the file readable and writable by the user alone, replacing it in one step.
    pub fn save(&self) -> Result<PathBuf, Diagnostic> {
        let path = credentials_path()?;
        let dir = path.parent().expect("a file in a directory");
        std::fs::create_dir_all(dir).map_err(|e| Diagnostic::io("store credentials", dir, &e))?;
        let text = toml::to_string(self).expect("credentials always serialise");
        let tmp = path.with_extension("toml.new");
        let mut open = std::fs::OpenOptions::new();
        open.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            open.mode(0o600);
        }
        use std::io::Write;
        open.open(&tmp)
            .and_then(|mut f| f.write_all(text.as_bytes()))
            .and_then(|()| std::fs::rename(&tmp, &path))
            .map_err(|e| Diagnostic::io("store credentials", &path, &e))?;
        Ok(path)
    }
}

/// Which registry, from `--registry`, then `GODSPEED_REGISTRY`, then the one `login` made the default, then
/// the official one.
pub fn resolve(flag: Option<&str>, creds: &Credentials) -> Result<String, Diagnostic> {
    let url = flag
        .map(String::from)
        .or_else(|| std::env::var(REGISTRY_ENV).ok().filter(|v| !v.is_empty()))
        .or_else(|| creds.default.clone())
        .unwrap_or_else(|| OFFICIAL.to_string());
    let url = url.trim_end_matches('/').to_string();
    if !(url.starts_with("https://") || url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost")) {
        return Err(Diagnostic::new(
            Kind::Environment,
            Stage::Authentication,
            format!("use the registry {url}"),
            "A registry is reached over HTTPS. Plain http is accepted only for a registry on this machine.",
        )
        .expected("https://... (or http://127.0.0.1:<port> for local development)"));
    }
    Ok(url)
}

/// The token for `registry`: `GODSPEED_TOKEN` if set, else the stored one. Says which it used.
pub fn token(registry: &str, creds: &Credentials) -> Result<(String, &'static str), Diagnostic> {
    if let Some(t) = std::env::var(TOKEN_ENV).ok().filter(|t| !t.is_empty()) {
        return Ok((t, TOKEN_ENV));
    }
    match creds.registries.get(registry) {
        Some(e) => Ok((e.token.clone(), "stored credentials")),
        None => Err(Diagnostic::new(
            Kind::AuthenticationFailure,
            Stage::Authentication,
            format!("authenticate with {registry}"),
            format!("You are not logged in to {registry}."),
        )
        .next(format!(
            "Run `godspeed login --registry {registry}`, or set {TOKEN_ENV} in CI."
        ))),
    }
}

// ---- the protocol ------------------------------------------------------------------------------------

/// A refusal: the registry decided, and said why.
#[derive(Debug, Clone, Deserialize)]
pub struct Refusal {
    #[serde(skip)]
    pub status: u16,
    pub error: String,
    pub message: String,
}

/// What came back.
pub enum Answer<T> {
    Done(T),
    Refused(Refusal),
    /// No decision is known: the request did not complete, or the registry failed. The text says how.
    Unknown(String),
}

pub struct Client {
    pub base: String,
    token: Option<String>,
    agent: ureq::Agent,
}

#[derive(Debug, Deserialize)]
pub struct WhoAmI {
    pub account: String,
    pub publishers: Vec<String>,
    pub token_expires: String,
}

#[derive(Debug, Deserialize)]
pub struct Reservation {
    pub upload: String,
    pub expires: String,
    pub files: Vec<String>,
    pub staged: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Published {
    pub manifest: String,
    pub published: String,
    /// The signed snapshot that includes the release; `None` if the registry could not re-sign yet.
    #[serde(default)]
    pub snapshot: Option<u64>,
}

impl Client {
    pub fn new(base: &str, token: Option<String>) -> Client {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(120))
            .timeout_write(Duration::from_secs(300))
            .user_agent(concat!("godspeed/", env!("CARGO_PKG_VERSION")))
            .build();
        Client {
            base: base.trim_end_matches('/').to_string(),
            token,
            agent,
        }
    }

    fn call<T: serde::de::DeserializeOwned>(&self, method: &str, path: &str, body: Option<&[u8]>) -> Answer<T> {
        let mut req = self.agent.request(method, &format!("{}{path}", self.base));
        if let Some(t) = &self.token {
            req = req.set("Authorization", &format!("Bearer {t}"));
        }
        let result = match body {
            Some(b) => req.send_bytes(b),
            None => req.call(),
        };
        match result {
            Ok(resp) => match resp.into_json::<T>() {
                Ok(v) => Answer::Done(v),
                // It answered 200 and we cannot read it: what happened is not known.
                Err(e) => Answer::Unknown(format!("the registry's answer could not be read: {e}")),
            },
            Err(ureq::Error::Status(status, resp)) if (400..500).contains(&status) => {
                match resp.into_json::<Refusal>() {
                    Ok(mut r) => {
                        r.status = status;
                        Answer::Refused(r)
                    }
                    Err(_) => Answer::Refused(Refusal {
                        status,
                        error: format!("http-{status}"),
                        message: format!("the registry refused with HTTP {status} and no explanation"),
                    }),
                }
            }
            Err(ureq::Error::Status(status, resp)) => {
                let detail = resp.into_json::<Refusal>().map(|r| r.message).unwrap_or_default();
                Answer::Unknown(
                    format!("the registry failed (HTTP {status}) {detail}")
                        .trim_end()
                        .to_string(),
                )
            }
            Err(ureq::Error::Transport(t)) => Answer::Unknown(format!("the registry could not be reached: {t}")),
        }
    }

    pub fn whoami(&self) -> Answer<WhoAmI> {
        self.call("GET", "/v1/whoami", None)
    }

    pub fn logout(&self) -> Answer<serde_json::Value> {
        self.call("POST", "/v1/logout", Some(b""))
    }

    pub fn reserve(&self, name: &str, manifest: &[u8]) -> Answer<Reservation> {
        self.call("POST", &format!("/v1/packages/{name}/reservations"), Some(manifest))
    }

    pub fn upload(&self, name: &str, version: &str, upload: &str, file: &str, bytes: &[u8]) -> Answer<Reservation> {
        self.call(
            "PUT",
            &format!("/v1/packages/{name}/{version}/uploads/{upload}/{file}"),
            Some(bytes),
        )
    }

    pub fn finalise(&self, name: &str, version: &str, upload: &str) -> Answer<Published> {
        self.call(
            "POST",
            &format!("/v1/packages/{name}/{version}/uploads/{upload}/finalise"),
            Some(b""),
        )
    }

    /// A public file (`trust/...`, `index/...`, `packages/...`), or `None` if it does not exist. Read without a
    /// token, and never more than `limit` bytes: a hostile server cannot make the CLI read forever.
    pub fn get(&self, path: &str, limit: u64) -> Answer<Option<Vec<u8>>> {
        use std::io::Read;
        let url = format!("{}/{path}", self.base);
        match self.agent.get(&url).call() {
            Ok(resp) => {
                let mut bytes = Vec::new();
                match resp.into_reader().take(limit + 1).read_to_end(&mut bytes) {
                    Ok(_) if bytes.len() as u64 > limit => {
                        Answer::Unknown(format!("{path} is larger than the {limit} bytes it may be"))
                    }
                    Ok(_) => Answer::Done(Some(bytes)),
                    Err(e) => Answer::Unknown(format!("{path} could not be read: {e}")),
                }
            }
            Err(ureq::Error::Status(404, _)) => Answer::Done(None),
            Err(ureq::Error::Status(status, _)) => Answer::Unknown(format!("{path} could not be read (HTTP {status})")),
            Err(ureq::Error::Transport(t)) => Answer::Unknown(format!("{path} could not be read: {t}")),
        }
    }

    /// The package's public index, or `None` if nothing of it has been published. Read without a token.
    pub fn index(&self, name: &str) -> Answer<Option<String>> {
        let (publisher, service) = name.split_once('/').unwrap_or((name, ""));
        let url = format!("{}/index/{publisher}/{service}.toml", self.base);
        match self.agent.get(&url).call() {
            Ok(resp) => match resp.into_string() {
                Ok(t) => Answer::Done(Some(t)),
                Err(e) => Answer::Unknown(format!("the index could not be read: {e}")),
            },
            Err(ureq::Error::Status(404, _)) => Answer::Done(None),
            Err(ureq::Error::Status(status, _)) => {
                Answer::Unknown(format!("the index could not be read (HTTP {status})"))
            }
            Err(ureq::Error::Transport(t)) => Answer::Unknown(format!("the index could not be read: {t}")),
        }
    }
}

/// Turn a refusal into a diagnostic a developer can act on. `doing` completes "Cannot ...".
pub fn refused(doing: &str, registry: &str, r: &Refusal) -> Diagnostic {
    let (kind, stage, next): (Kind, Stage, String) = match r.error.as_str() {
        "token-not-approved" => (
            Kind::AuthenticationFailure,
            Stage::Authentication,
            "Send the registry maintainer that fingerprint and your account name, and wait for them to approve it. \
             (`godspeed login` prints it again.)"
                .into(),
        ),
        "unauthenticated" | "token-expired" => (
            Kind::AuthenticationFailure,
            Stage::Authentication,
            format!("Run `godspeed login --registry {registry}`: it makes a new token for the maintainer to approve."),
        ),
        "namespace-not-yours" => (
            Kind::PermissionDenied,
            Stage::Authentication,
            "Publish under a namespace your account may use (`godspeed whoami` lists them), or ask the registry \
             maintainer to add you to this one."
                .into(),
        ),
        "version-exists" => (
            Kind::Conflict,
            Stage::Upload,
            "Increment the version (in the contract and Cargo.toml) and publish again.".into(),
        ),
        "version-being-published" => (
            Kind::Conflict,
            Stage::Upload,
            "Another upload holds this version for up to 15 minutes. Wait and check, or publish a different version."
                .into(),
        ),
        "reservation-expired" | "upload-incomplete" => (
            Kind::Conflict,
            Stage::Upload,
            "Run `godspeed publish` again: it resumes or restarts the upload, and is safe to repeat.".into(),
        ),
        "manifest-invalid" | "file-not-in-manifest" => (
            Kind::InvalidPackage,
            Stage::Upload,
            "Run `godspeed package` and `godspeed verify`, and report it if they pass: the CLI and the registry \
             disagree about the package format."
                .into(),
        ),
        "signature-invalid" => (
            Kind::SignatureFailure,
            Stage::Upload,
            "Sign the package again (`godspeed publish` does) and retry; if it persists, report it.".into(),
        ),
        "key-not-registered" => (
            Kind::PermissionDenied,
            Stage::Authentication,
            "Send your public key (`godspeed key show`) to the registry maintainer to register for this namespace, \
             then publish again."
                .into(),
        ),
        "trust-not-initialised" | "trust-expired" => (
            Kind::RegistryUnavailable,
            Stage::Authentication,
            "The registry's signed trust metadata is missing or expired; its maintainer must sign and upload it. \
             Nothing can be published until then."
                .into(),
        ),
        "size-mismatch" | "digest-mismatch" => (
            Kind::IntegrityFailure,
            Stage::Upload,
            "The bytes changed on the way. Run `godspeed publish` again; if it persists, report it.".into(),
        ),
        "published-differently" => (
            Kind::Conflict,
            Stage::Finalisation,
            "Nothing was changed. Report this to the registry maintainer.".into(),
        ),
        _ => (
            Kind::Conflict,
            Stage::Upload,
            format!(
                "The registry answered with `{}`, which this CLI does not know. Update the CLI: `godspeed self update`.",
                r.error
            ),
        ),
    };
    Diagnostic::new(kind, stage, doing.to_string(), r.message.clone()).next(next)
}

/// A request that got no decision, at a stage where nothing can have been committed: authentication,
/// reservation, or an upload (only finalisation publishes).
pub fn unreachable(doing: &str, stage: Stage, why: &str) -> Diagnostic {
    let next = if stage == Stage::Authentication {
        "Check the network and the registry URL, then run the command again."
    } else {
        "Check the network and the registry URL, then run `godspeed publish` again: nothing was published, and \
         the upload resumes where it stopped."
    };
    Diagnostic::new(Kind::RegistryUnavailable, stage, doing.to_string(), format!("{why}.")).next(next)
}
