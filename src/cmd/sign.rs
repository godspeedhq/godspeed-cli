//! `godspeed key generate | show` and `godspeed sign <dir>`.
//!
//! `sign` turns a package directory that verifies into a signed one: it writes `manifest.sig`, the
//! publisher's signature over the exact manifest bytes, and verifies the result. `publish` does the same
//! before uploading; `sign` exists so a package can be built, inspected, and signed as separate steps.

use super::verify;
use crate::diag::{Diagnostic, Kind, Result, Stage};
use crate::signing::{self, SigningKey};
use godspeed_package::{PublicKey, SIGNATURE_FILE};
use std::path::Path;

pub fn key_generate() -> Result<()> {
    let (public, path) = signing::generate()?;
    println!("Generated a signing key.");
    println!("  public key: {public}");
    println!("  stored in:  {} (readable by you alone)", path.display());
    println!("Next:");
    println!("  1. Back up that file. A lost key cannot be recovered.");
    println!("  2. Send the PUBLIC key above - never the file - to the registry maintainer, to register for your");
    println!("     namespace. Releases signed by an unregistered key are refused.");
    Ok(())
}

pub fn key_show() -> Result<()> {
    let key = signing::load()?;
    println!("{}", key.public);
    eprintln!("(from {})", key.from);
    Ok(())
}

/// Sign the package in `dir` with `key`, then verify it. Returns the key that signed.
pub fn sign_dir(dir: &Path, key: &SigningKey) -> Result<PublicKey> {
    // Only a package that verifies is signed: a signature vouches for every file, through the manifest.
    let before = verify::run(dir)?;
    let manifest = std::fs::read(dir.join(verify::MANIFEST_FILE))
        .map_err(|e| Diagnostic::io("sign the package", &dir.join(verify::MANIFEST_FILE), &e))?;
    let mut text = String::new();
    key.sign(&manifest)
        .write(&mut text)
        .expect("writing to a String cannot fail");

    let path = dir.join(SIGNATURE_FILE);
    let tmp = dir.join(format!(".{SIGNATURE_FILE}.new"));
    std::fs::write(&tmp, &text)
        .and_then(|()| std::fs::rename(&tmp, &path))
        .map_err(|e| Diagnostic::io("sign the package", &path, &e))?;

    let after = verify::run(dir)?;
    if after.signed_by != Some(key.public) {
        return Err(Diagnostic::new(
            Kind::SignatureFailure,
            Stage::Packaging,
            "sign the package",
            "The signature just written does not verify with the signing key.",
        )
        .next("Report this: a signature this command wrote should always verify.")
        .into());
    }
    if let Some(previous) = before.signed_by.filter(|p| *p != key.public) {
        eprintln!("Replaced the previous signature, by {previous}.");
    }
    Ok(key.public)
}

pub fn run(dir: &Path) -> Result<()> {
    let key = signing::load()?;
    let public = sign_dir(dir, &key)?;
    let verified = verify::run(dir)?;
    let m = verified.manifest.manifest();
    println!("Signed {}@{} with {public}", m.name, m.version);
    Ok(())
}
