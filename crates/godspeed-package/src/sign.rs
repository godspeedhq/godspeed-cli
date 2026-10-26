// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Publisher signatures over manifests: `manifest.sig`, beside `manifest.toml`.
//!
//! A signature establishes which KEY signed a manifest - and through the manifest's digests, every file of
//! the package. It does not establish who holds that key: binding a key to a publisher is the registry's
//! trust model (`docs/publishing-design.md`, section 6). And it says nothing about whether the package is
//! safe.
//!
//! # What is signed
//!
//! Ed25519 over a fixed 63-byte message: the purpose tag [`MANIFEST_CONTEXT`] followed by the SHA-256 of the
//! manifest's exact bytes. The tag means a signature made for a manifest can never be presented as a
//! signature over anything else the registry signs later (an index, root metadata), and signing a digest
//! keeps the message on the stack, so verification needs no heap on a Godspeed machine.
//!
//! # The file
//!
//! Exactly three lines, in exactly this form, and nothing else:
//!
//! ```text
//! format = 1
//! key = "ed25519:<64 lowercase hex digits: the public key>"
//! signature = "ed25519:<128 lowercase hex digits>"
//! ```
//!
//! It is valid TOML, but it is read by comparing its canonical form, not by a TOML parser - so the native
//! package manager reads it without one, and a file has one spelling.

use crate::digest::Sha256Digest;
use crate::hex;
use core::fmt;
use core::str::FromStr;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};

/// The signature's file name in a package.
pub const SIGNATURE_FILE: &str = "manifest.sig";

/// The `manifest.sig` format this crate reads and writes.
pub const SIGNATURE_FORMAT: u32 = 1;

/// The purpose tag every manifest signature begins with. Never reused for any other kind of signature.
pub const MANIFEST_CONTEXT: &[u8; 31] = b"godspeed manifest signature v1\n";

/// An Ed25519 public key, written `ed25519:<64 hex digits>`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PublicKey(pub [u8; 32]);

/// An Ed25519 signature, written `ed25519:<128 hex digits>`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Signature(pub [u8; 64]);

/// A manifest's signature: which key, and the signature it made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManifestSignature {
    /// The public key that made the signature.
    pub key: PublicKey,
    /// The signature over [`signed_message`] of the manifest.
    pub signature: Signature,
}

/// Why a signature was not accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignatureError {
    /// The file is not exactly the canonical three lines.
    Malformed,
    /// The file is a format this reader does not know.
    UnknownFormat,
    /// The public key is not a valid Ed25519 point.
    BadKey,
    /// The signature does not verify: this key did not sign this manifest.
    Invalid,
}

impl SignatureError {
    /// What went wrong, in one line.
    pub const fn message(self) -> &'static str {
        match self {
            SignatureError::Malformed => "the signature file is not in its exact canonical form",
            SignatureError::UnknownFormat => "the signature file is a format this reader does not know",
            SignatureError::BadKey => "the signature names a public key that is not a valid Ed25519 key",
            SignatureError::Invalid => "the signature does not verify: this key did not sign this manifest",
        }
    }
}

impl fmt::Display for SignatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

fn parse_tagged<const N: usize>(s: &str) -> Option<[u8; N]> {
    let hex = s.strip_prefix("ed25519:")?;
    let mut out = [0u8; N];
    hex::decode(hex.as_bytes(), &mut out).then_some(out)
}

impl FromStr for PublicKey {
    type Err = SignatureError;
    fn from_str(s: &str) -> Result<Self, SignatureError> {
        let bytes = parse_tagged::<32>(s).ok_or(SignatureError::Malformed)?;
        VerifyingKey::from_bytes(&bytes).map_err(|_| SignatureError::BadKey)?;
        Ok(PublicKey(bytes))
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ed25519:")?;
        hex::encode(&self.0, f)
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl FromStr for Signature {
    type Err = SignatureError;
    fn from_str(s: &str) -> Result<Self, SignatureError> {
        parse_tagged::<64>(s).map(Signature).ok_or(SignatureError::Malformed)
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ed25519:")?;
        hex::encode(&self.0, f)
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// The exact bytes a manifest signature signs: the purpose tag, then the manifest's SHA-256.
pub fn signed_message(manifest: &[u8]) -> [u8; 63] {
    let mut msg = [0u8; 63];
    msg[..31].copy_from_slice(MANIFEST_CONTEXT);
    msg[31..].copy_from_slice(&Sha256Digest::of(manifest).0);
    msg
}

impl ManifestSignature {
    /// Sign `manifest` with the Ed25519 key whose 32-byte secret seed is `seed`.
    pub fn sign(seed: &[u8; 32], manifest: &[u8]) -> ManifestSignature {
        let key = SigningKey::from_bytes(seed);
        let signature = key.sign(&signed_message(manifest));
        ManifestSignature {
            key: PublicKey(key.verifying_key().to_bytes()),
            signature: Signature(signature.to_bytes()),
        }
    }

    /// The public key belonging to a secret seed.
    pub fn public_key(seed: &[u8; 32]) -> PublicKey {
        PublicKey(SigningKey::from_bytes(seed).verifying_key().to_bytes())
    }

    /// Does this signature, by its key, cover exactly `manifest`? Strict verification: malleable signatures
    /// and weak keys are refused.
    pub fn verify(&self, manifest: &[u8]) -> Result<(), SignatureError> {
        let key = VerifyingKey::from_bytes(&self.key.0).map_err(|_| SignatureError::BadKey)?;
        let sig = ed25519_dalek::Signature::from_bytes(&self.signature.0);
        key.verify_strict(&signed_message(manifest), &sig)
            .map_err(|_| SignatureError::Invalid)
    }

    /// Read `manifest.sig`. Only the exact canonical form is accepted.
    pub fn parse(text: &str) -> Result<ManifestSignature, SignatureError> {
        let mut lines = text.split('\n');
        let format = lines.next().ok_or(SignatureError::Malformed)?;
        match format.strip_prefix("format = ") {
            Some("1") => {}
            Some(n) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => {
                return Err(SignatureError::UnknownFormat)
            }
            _ => return Err(SignatureError::Malformed),
        }
        fn quoted<'a>(line: Option<&'a str>, key: &str) -> Result<&'a str, SignatureError> {
            line.and_then(|l| l.strip_prefix(key))
                .and_then(|l| l.strip_prefix(" = \""))
                .and_then(|l| l.strip_suffix('"'))
                .ok_or(SignatureError::Malformed)
        }
        let key: PublicKey = quoted(lines.next(), "key")?.parse()?;
        let signature: Signature = quoted(lines.next(), "signature")?.parse()?;
        // Exactly one final newline, and nothing after it.
        if lines.next() != Some("") || lines.next().is_some() {
            return Err(SignatureError::Malformed);
        }
        Ok(ManifestSignature { key, signature })
    }

    /// Write `manifest.sig` in its canonical form.
    pub fn write(&self, out: &mut impl fmt::Write) -> fmt::Result {
        write!(
            out,
            "format = {SIGNATURE_FORMAT}\nkey = \"{}\"\nsignature = \"{}\"\n",
            self.key, self.signature
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::string::{String, ToString};

    const SEED: [u8; 32] = [7; 32];
    const MANIFEST: &[u8] = b"format = 1\n\n[package]\nname = \"alice/hello\"\n";

    fn text(s: &ManifestSignature) -> String {
        let mut t = String::new();
        s.write(&mut t).unwrap();
        t
    }

    #[test]
    fn signs_verifies_and_round_trips() {
        let s = ManifestSignature::sign(&SEED, MANIFEST);
        assert_eq!(s.key, ManifestSignature::public_key(&SEED));
        assert_eq!(s.verify(MANIFEST), Ok(()));
        let t = text(&s);
        assert_eq!(ManifestSignature::parse(&t), Ok(s));
        assert!(t.starts_with("format = 1\nkey = \"ed25519:"), "{t}");
        assert_eq!(t.lines().count(), 3);
    }

    #[test]
    fn a_changed_manifest_or_another_key_does_not_verify() {
        let s = ManifestSignature::sign(&SEED, MANIFEST);
        let mut changed = MANIFEST.to_vec();
        changed[0] ^= 1;
        assert_eq!(s.verify(&changed), Err(SignatureError::Invalid));

        let other = ManifestSignature::sign(&[8; 32], MANIFEST);
        let forged = ManifestSignature {
            key: s.key,
            signature: other.signature,
        };
        assert_eq!(forged.verify(MANIFEST), Err(SignatureError::Invalid));

        let mut flipped = s;
        flipped.signature.0[10] ^= 1;
        assert_eq!(flipped.verify(MANIFEST), Err(SignatureError::Invalid));
    }

    #[test]
    fn the_signed_message_is_tagged_and_fixed_size() {
        let m = signed_message(MANIFEST);
        assert_eq!(&m[..31], MANIFEST_CONTEXT);
        assert_eq!(&m[31..], &Sha256Digest::of(MANIFEST).0);
    }

    #[test]
    fn only_the_canonical_form_is_read() {
        let t = text(&ManifestSignature::sign(&SEED, MANIFEST));
        for bad in [
            t.replace('\n', "\r\n"),
            t.trim_end().to_string(),
            std::format!("{t}\n"),
            t.replace("format = 1", "format=1"),
            t.replace("key = ", "key  = "),
            t.replace("ed25519:", "ED25519:"),
            std::format!("# comment\n{t}"),
        ] {
            assert_eq!(
                ManifestSignature::parse(&bad),
                Err(SignatureError::Malformed),
                "{bad:?}"
            );
        }
        assert_eq!(
            ManifestSignature::parse(&t.replace("format = 1", "format = 2")),
            Err(SignatureError::UnknownFormat)
        );
    }

    #[test]
    fn keys_parse_and_refuse() {
        let k = ManifestSignature::public_key(&SEED);
        assert_eq!(std::format!("{k}").parse::<PublicKey>(), Ok(k));
        assert_eq!("ed25519:00".parse::<PublicKey>(), Err(SignatureError::Malformed));
        assert_eq!("rsa:00".parse::<PublicKey>(), Err(SignatureError::Malformed));
    }
}
