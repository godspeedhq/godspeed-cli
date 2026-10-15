//! The publisher's signing key: made here, kept here, never sent anywhere.
//!
//! Only the public half leaves this machine: the publisher gives it to the registry maintainer, who
//! registers it for their namespace. The secret half signs manifests and is stored in the configuration
//! directory, readable by the user alone - never in a project, a package or a log line. In CI, the secret
//! comes from `GODSPEED_SIGNING_KEY` instead and is never written to disk.

use crate::diag::{Diagnostic, Kind, Stage};
use crate::registry;
use godspeed_package::{ManifestSignature, PublicKey};
use std::path::PathBuf;

/// The signing key's secret seed, for CI: 64 lowercase hex digits. Used instead of the stored key.
pub const SIGNING_KEY_ENV: &str = "GODSPEED_SIGNING_KEY";

pub struct SigningKey {
    seed: [u8; 32],
    pub public: PublicKey,
    pub from: String,
}

impl SigningKey {
    pub fn sign(&self, manifest: &[u8]) -> ManifestSignature {
        ManifestSignature::sign(&self.seed, manifest)
    }
}

impl Drop for SigningKey {
    fn drop(&mut self) {
        self.seed = [0; 32]; // best effort: do not leave the secret in freed memory
    }
}

pub fn key_path() -> Result<PathBuf, Diagnostic> {
    registry::config_dir()
        .map(|d| d.join("signing-key.toml"))
        .ok_or_else(|| {
            Diagnostic::new(
                Kind::Environment,
                Stage::Setup,
                "find the signing key",
                "Neither HOME nor APPDATA is set, so there is no configuration directory.",
            )
            .next(format!(
                "Set {} to a directory only you can read.",
                registry::CONFIG_ENV
            ))
        })
}

fn decode_seed(hex: &str) -> Option<[u8; 32]> {
    let hex = hex.trim().as_bytes();
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, pair) in hex.chunks_exact(2).enumerate() {
        out[i] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(out)
}

fn malformed(where_: &str) -> Diagnostic {
    Diagnostic::new(
        Kind::Environment,
        Stage::Setup,
        "read the signing key",
        format!("{where_} does not hold a valid signing key."),
    )
    .next("Restore it from your backup. A lost key cannot be recovered: generate a new one and have the registry maintainer register it.")
}

/// The signing key: `GODSPEED_SIGNING_KEY` if set, else the stored one, else a diagnostic saying how to make one.
pub fn load() -> Result<SigningKey, Diagnostic> {
    if let Ok(hex) = std::env::var(SIGNING_KEY_ENV) {
        if !hex.is_empty() {
            let seed = decode_seed(&hex).ok_or_else(|| malformed(SIGNING_KEY_ENV))?;
            return Ok(SigningKey {
                seed,
                public: ManifestSignature::public_key(&seed),
                from: SIGNING_KEY_ENV.into(),
            });
        }
    }
    let path = key_path()?;
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Diagnostic::new(
                Kind::Environment,
                Stage::Setup,
                "find your signing key",
                "You have no signing key. Every published release is signed by its publisher.",
            )
            .next(
                "Run `godspeed key generate`, and send the public key it prints to the registry maintainer to \
                 register for your namespace.",
            ));
        }
        Err(e) => return Err(Diagnostic::io("read the signing key", &path, &e)),
    };
    let table: toml::Table = text.parse().map_err(|_| malformed(&path.display().to_string()))?;
    let seed = table
        .get("secret")
        .and_then(|v| v.as_str())
        .and_then(decode_seed)
        .ok_or_else(|| malformed(&path.display().to_string()))?;
    let public = ManifestSignature::public_key(&seed);
    // The stored public key must be the secret's: a mismatch means the file was edited or mixed up.
    if table.get("public").and_then(|v| v.as_str()) != Some(public.to_string().as_str()) {
        return Err(malformed(&path.display().to_string()));
    }
    Ok(SigningKey {
        seed,
        public,
        from: path.display().to_string(),
    })
}

/// Make a new key and store it. Never replaces an existing one: replacing a registered key silently would
/// leave every release signed by a key nobody can match.
pub fn generate() -> Result<(PublicKey, PathBuf), Diagnostic> {
    let path = key_path()?;
    if path.exists() {
        let existing = load()
            .map(|k| k.public.to_string())
            .unwrap_or_else(|_| "(unreadable)".into());
        return Err(Diagnostic::new(
            Kind::Conflict,
            Stage::Setup,
            "generate a signing key",
            format!("A signing key already exists: {existing}, in {}.", path.display()),
        )
        .next(
            "Keep using it (`godspeed key show`). To replace it, first have the registry maintainer register the new \
             key, then move the old file away yourself - this command never deletes a key.",
        ));
    }
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| {
        Diagnostic::new(
            Kind::Environment,
            Stage::Setup,
            "generate a signing key",
            format!("The operating system's random number source failed: {e}"),
        )
    })?;
    let public = ManifestSignature::public_key(&seed);
    let mut secret = String::with_capacity(64);
    for b in seed {
        secret.push_str(&format!("{b:02x}"));
    }
    let text = format!(
        "# Your Godspeed signing key. PRIVATE: anyone holding this file can sign packages as you.\n\
         # Back it up somewhere safe. Never put it in a repository, a package or a CI log.\n\
         format = 1\n\
         public = \"{public}\"\n\
         secret = \"{secret}\"\n"
    );
    let dir = path.parent().expect("a file in a directory");
    std::fs::create_dir_all(dir).map_err(|e| Diagnostic::io("store the signing key", dir, &e))?;
    let mut open = std::fs::OpenOptions::new();
    open.write(true).create_new(true); // never overwrite, even in a race with another `generate`
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open.mode(0o600);
    }
    use std::io::Write;
    open.open(&path)
        .and_then(|mut f| f.write_all(text.as_bytes()))
        .map_err(|e| Diagnostic::io("store the signing key", &path, &e))?;
    Ok((public, path))
}
