//! The registry's trust metadata: which keys may sign what, and proof that an index is current.
//!
//! Four documents, after The Update Framework's roles (`docs/publishing-design.md` section 6):
//!
//! | document | signed by | says |
//! |---|---|---|
//! | `root` | the root keys (offline), by threshold | which keys hold each role, and how many must sign |
//! | `namespaces` | the namespaces keys (offline) | each publisher's keys: active, retired, or revoked |
//! | `snapshot` | the snapshot key (online) | the digest of every package index, and of `namespaces` |
//! | `timestamp` | the timestamp key (online) | the current snapshot; it expires within a day |
//!
//! A registry that is compromised holds only the online keys. It cannot add a key for a publisher
//! (`namespaces` is signed offline), and it cannot show a client a frozen or rolled-back view for longer than
//! a timestamp lasts.
//!
//! # Retired and revoked
//!
//! - **Retired at T** (a routine rotation): the key signs nothing new; releases published before T stay
//!   trusted.
//! - **Revoked** (a compromise): nothing the key ever signed is trusted, whenever it was published - the
//!   moment a key was stolen is rarely known. The publisher releases a new version under a new key.
//!
//! # The format
//!
//! Every document is a canonical sequence of `name = value` lines - a value is a decimal integer or a quoted
//! string - in a fixed order, with repeated lines strictly sorted, LF endings, and one final newline. It is
//! valid TOML, but it is read by these rules, not by a TOML parser: a Godspeed machine checks it without a
//! heap, and a document has exactly one spelling, which is what its signatures cover. A document's
//! signatures are a separate file (`<document>.sig`).
//!
//! Each signature is Ed25519 over the document type's purpose tag (`godspeed <type> metadata v1` and a
//! newline) followed by the SHA-256 of the document's exact bytes - never valid for any other document type,
//! nor for a manifest.

use crate::digest::Sha256Digest;
use crate::index::is_timestamp;
use crate::name::PackageName;
use crate::sign::{PublicKey, Signature};
use core::fmt;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};

/// The trust metadata format this crate reads and writes.
pub const METADATA_FORMAT: u32 = 1;

/// A role, and the document it signs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// Signs `root`: the keys of every role.
    Root,
    /// Signs `namespaces`: each publisher's keys.
    Namespaces,
    /// Signs `snapshot`: the digest of every index.
    Snapshot,
    /// Signs `timestamp`: the current snapshot.
    Timestamp,
}

impl Role {
    /// Every role, in the order a root document lists them.
    pub const ALL: [Role; 4] = [Role::Root, Role::Namespaces, Role::Snapshot, Role::Timestamp];

    /// The role's name, which is also its document's `type` and file name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Role::Root => "root",
            Role::Namespaces => "namespaces",
            Role::Snapshot => "snapshot",
            Role::Timestamp => "timestamp",
        }
    }

    /// The purpose tag this role's signatures begin with.
    pub const fn context(self) -> &'static [u8] {
        match self {
            Role::Root => b"godspeed root metadata v1\n",
            Role::Namespaces => b"godspeed namespaces metadata v1\n",
            Role::Snapshot => b"godspeed snapshot metadata v1\n",
            Role::Timestamp => b"godspeed timestamp metadata v1\n",
        }
    }

    /// The role with this name.
    pub fn from_name(s: &str) -> Option<Role> {
        Role::ALL.into_iter().find(|r| r.as_str() == s)
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why trust metadata, or a release under it, was not accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrustError {
    /// A document or signature file breaks the canonical format; the text says which rule.
    Malformed(&'static str),
    /// A format this reader does not know.
    UnknownFormat,
    /// The document is not the type expected.
    WrongType,
    /// Fewer valid signatures by the role's keys than its threshold.
    NotEnoughSignatures {
        /// The role whose keys had to sign.
        role: Role,
        /// Valid signatures by distinct keys of that role.
        have: u32,
        /// The role's threshold.
        need: u32,
    },
    /// The document's expiry has passed.
    Expired(Role),
    /// The document is older than one already trusted: a rollback.
    Rollback(Role),
    /// A version is not the one required (a root that is not the next, a snapshot not the one named).
    WrongVersion(Role),
    /// A document's digest is not the one the document above it names.
    DigestMismatch(&'static str),
    /// The snapshot lists no index for this package.
    NotInSnapshot,
    /// The key that signed the release is not one of the publisher's keys.
    UnknownKey,
    /// The key that signed the release is revoked: nothing it signed is trusted.
    RevokedKey,
    /// The release was published after its key was retired.
    RetiredKey,
}

impl fmt::Display for TrustError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrustError::Malformed(why) => write!(f, "malformed trust metadata: {why}"),
            TrustError::UnknownFormat => f.write_str("trust metadata in a format this reader does not know"),
            TrustError::WrongType => f.write_str("trust metadata of the wrong type"),
            TrustError::NotEnoughSignatures { role, have, need } => {
                write!(
                    f,
                    "{role} metadata has {have} valid signature(s) by {role} keys; {need} needed"
                )
            }
            TrustError::Expired(role) => write!(f, "the {role} metadata has expired"),
            TrustError::Rollback(role) => write!(f, "the {role} metadata is older than one already trusted"),
            TrustError::WrongVersion(role) => write!(f, "the {role} metadata is not the version required"),
            TrustError::DigestMismatch(what) => write!(f, "{what} is not the one the trust metadata names"),
            TrustError::NotInSnapshot => f.write_str("the registry's snapshot lists no such package"),
            TrustError::UnknownKey => f.write_str("the release is signed by a key that is not one of the publisher's"),
            TrustError::RevokedKey => {
                f.write_str("the release is signed by a revoked key; nothing it signed is trusted")
            }
            TrustError::RetiredKey => f.write_str("the release was published after the key that signed it was retired"),
        }
    }
}

// ---- reading the canonical line format, without a heap --------------------------------------------------

struct Reader<'a> {
    rest: &'a str,
}

fn malformed(why: &'static str) -> TrustError {
    TrustError::Malformed(why)
}

impl<'a> Reader<'a> {
    fn peek_name(&self) -> Option<&'a str> {
        let line = self.rest.split('\n').next()?;
        line.split_once(" = ").map(|(n, _)| n)
    }

    /// The next `name = value` line, by its exact name.
    fn field(&mut self, name: &str) -> Result<&'a str, TrustError> {
        let (line, rest) = self
            .rest
            .split_once('\n')
            .ok_or(malformed("a line is missing, or the final newline"))?;
        let (n, v) = line
            .split_once(" = ")
            .ok_or(malformed("a line is not `name = value`"))?;
        if n != name {
            return Err(malformed("a field is missing or out of order"));
        }
        self.rest = rest;
        Ok(v)
    }

    fn end(&self) -> Result<(), TrustError> {
        if self.rest.is_empty() {
            Ok(())
        } else {
            Err(malformed("unexpected lines at the end"))
        }
    }
}

fn int(v: &str) -> Result<u64, TrustError> {
    if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) || (v.len() > 1 && v.starts_with('0')) {
        return Err(malformed("an integer is not canonical decimal"));
    }
    v.parse().map_err(|_| malformed("an integer is too large"))
}

fn string(v: &str) -> Result<&str, TrustError> {
    let inner = v
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .ok_or(malformed("a string is not quoted"))?;
    if inner.chars().any(|c| c == '"' || c == '\\' || c.is_control()) {
        return Err(malformed("a string holds a quote, backslash or control character"));
    }
    Ok(inner)
}

/// The fields every document begins with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header<'a> {
    /// Which document this is.
    pub role: Role,
    /// Its version: 1 or more, and never lower than one already trusted.
    pub version: u64,
    /// When it stops being valid, `YYYY-MM-DDTHH:MM:SSZ`.
    pub expires: &'a str,
}

impl Header<'_> {
    /// Is it still valid at `now` (`YYYY-MM-DDTHH:MM:SSZ`)?
    pub fn check_fresh(&self, now: &str) -> Result<(), TrustError> {
        if self.expires > now {
            Ok(())
        } else {
            Err(TrustError::Expired(self.role))
        }
    }
}

fn header<'a>(text: &'a str, role: Role) -> Result<(Header<'a>, Reader<'a>), TrustError> {
    let mut r = Reader { rest: text };
    match r.field("format")? {
        "1" => {}
        n if int(n).is_ok() => return Err(TrustError::UnknownFormat),
        _ => return Err(malformed("format is not an integer")),
    }
    if string(r.field("type")?)? != role.as_str() {
        return Err(TrustError::WrongType);
    }
    let version = int(r.field("version")?)?;
    if version == 0 {
        return Err(malformed("a version starts at 1"));
    }
    let expires = string(r.field("expires")?)?;
    if !is_timestamp(expires) {
        return Err(malformed("expires is not YYYY-MM-DDTHH:MM:SSZ"));
    }
    Ok((Header { role, version, expires }, r))
}

/// Split a string value into exactly N space-separated fields.
fn fields<const N: usize>(v: &str) -> Result<[&str; N], TrustError> {
    let mut out = [""; N];
    let mut parts = v.split(' ');
    for slot in out.iter_mut() {
        *slot = parts
            .next()
            .filter(|p| !p.is_empty())
            .ok_or(malformed("a value has too few fields"))?;
    }
    if parts.next().is_some() {
        return Err(malformed("a value has too many fields"));
    }
    Ok(out)
}

fn key(s: &str) -> Result<PublicKey, TrustError> {
    s.parse()
        .map_err(|_| malformed("a public key is not ed25519:<64 hex digits>, or not a valid key"))
}

fn digest(s: &str) -> Result<Sha256Digest, TrustError> {
    s.parse()
        .map_err(|_| malformed("a digest is not sha256:<64 hex digits>"))
}

// ---- root --------------------------------------------------------------------------------------------

/// A root document: for every role, its threshold and its keys.
#[derive(Clone, Copy, Debug)]
pub struct Root<'a> {
    /// Version and expiry.
    pub header: Header<'a>,
    body: &'a str,
}

impl<'a> Root<'a> {
    /// Read a root document. Every role must be present, in order, with at least as many keys as its
    /// threshold, listed in ascending order.
    pub fn parse(text: &'a str) -> Result<Root<'a>, TrustError> {
        let (header, mut r) = header(text, Role::Root)?;
        let body = r.rest;
        for role in Role::ALL {
            let [name, threshold] = fields::<2>(string(r.field("role")?)?)?;
            if name != role.as_str() {
                return Err(malformed(
                    "roles are not root, namespaces, snapshot, timestamp, in order",
                ));
            }
            let threshold = int(threshold)?;
            if threshold == 0 {
                return Err(malformed("a threshold is at least 1"));
            }
            let (mut count, mut previous) = (0u64, None);
            while r.peek_name() == Some("key") {
                let [owner, k] = fields::<2>(string(r.field("key")?)?)?;
                if owner != role.as_str() {
                    return Err(malformed("a key is listed under the wrong role"));
                }
                let k = key(k)?;
                if previous.is_some_and(|p| p >= k) {
                    return Err(malformed("a role's keys are not in ascending order, or repeat"));
                }
                previous = Some(k);
                count += 1;
            }
            if count < threshold {
                return Err(malformed("a role has fewer keys than its threshold"));
            }
        }
        r.end()?;
        Ok(Root { header, body })
    }

    /// How many of `role`'s keys must sign its document.
    pub fn threshold(&self, role: Role) -> u32 {
        let mut r = Reader { rest: self.body };
        while let Some(name) = r.peek_name() {
            let v = r.field(name).unwrap_or_default();
            if name == "role" {
                if let Ok([n, t]) = string(v).and_then(fields::<2>) {
                    if n == role.as_str() {
                        return int(t).ok().and_then(|t| u32::try_from(t).ok()).unwrap_or(u32::MAX);
                    }
                }
            }
        }
        u32::MAX // unreachable for a parsed root: every role is present
    }

    /// Is `k` one of `role`'s keys?
    pub fn has_key(&self, role: Role, k: &PublicKey) -> bool {
        let mut r = Reader { rest: self.body };
        while let Some(name) = r.peek_name() {
            let v = r.field(name).unwrap_or_default();
            if name == "key" {
                if let Ok([owner, ks]) = string(v).and_then(fields::<2>) {
                    if owner == role.as_str() && key(ks).ok().as_ref() == Some(k) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

// ---- namespaces --------------------------------------------------------------------------------------

/// What a publisher's key may do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyStatus<'a> {
    /// It signs releases.
    Active,
    /// Retired at this time: it signs nothing new; releases published before then stay trusted.
    Retired(&'a str),
    /// Revoked at this time: nothing it ever signed is trusted.
    Revoked(&'a str),
}

/// A namespaces document: every publisher's keys and their status.
#[derive(Clone, Copy, Debug)]
pub struct Namespaces<'a> {
    /// Version and expiry.
    pub header: Header<'a>,
    body: &'a str,
}

fn namespace_entry(v: &str) -> Result<(&str, KeyStatus<'_>, PublicKey), TrustError> {
    let v = string(v)?;
    let mut parts = v.splitn(3, ' ');
    let publisher = parts.next().unwrap_or_default();
    let status = parts
        .next()
        .ok_or(malformed("a namespace key line has too few fields"))?;
    let rest = parts
        .next()
        .ok_or(malformed("a namespace key line has too few fields"))?;
    if !PackageName::is_valid_part(publisher) {
        return Err(malformed("a namespace key line names an invalid publisher"));
    }
    let (status, k) = match status {
        "active" => (KeyStatus::Active, fields::<1>(rest)?[0]),
        "retired" | "revoked" => {
            let [when, k] = fields::<2>(rest)?;
            if !is_timestamp(when) {
                return Err(malformed("a retirement or revocation time is not YYYY-MM-DDTHH:MM:SSZ"));
            }
            (
                if status == "retired" {
                    KeyStatus::Retired(when)
                } else {
                    KeyStatus::Revoked(when)
                },
                k,
            )
        }
        _ => return Err(malformed("a key's status is not active, retired or revoked")),
    };
    Ok((publisher, status, key(k)?))
}

impl<'a> Namespaces<'a> {
    /// Read a namespaces document: `key` lines in ascending order of publisher, then key.
    pub fn parse(text: &'a str) -> Result<Namespaces<'a>, TrustError> {
        let (header, mut r) = header(text, Role::Namespaces)?;
        let body = r.rest;
        let mut previous: Option<(&str, PublicKey)> = None;
        while r.peek_name().is_some() {
            let (publisher, _, k) = namespace_entry(r.field("key")?)?;
            if previous.is_some_and(|p| p >= (publisher, k)) {
                return Err(malformed(
                    "namespace keys are not in ascending order of publisher and key, or repeat",
                ));
            }
            previous = Some((publisher, k));
        }
        r.end()?;
        Ok(Namespaces { header, body })
    }

    /// The status of `k` for `publisher`, if it is one of their keys.
    pub fn status(&self, publisher: &str, k: &PublicKey) -> Option<KeyStatus<'a>> {
        self.entries()
            .find(|(p, _, key)| *p == publisher && key == k)
            .map(|(_, s, _)| s)
    }

    /// Every entry, in document order: publisher, status, key.
    pub fn entries(&self) -> impl Iterator<Item = (&'a str, KeyStatus<'a>, PublicKey)> + 'a {
        let mut r = Reader { rest: self.body };
        core::iter::from_fn(move || {
            r.peek_name()?;
            namespace_entry(r.field("key").ok()?).ok()
        })
    }

    /// May `k` sign a NEW release under `publisher`? Only an active key may.
    pub fn may_sign(&self, publisher: &str, k: &PublicKey) -> bool {
        self.status(publisher, k) == Some(KeyStatus::Active)
    }

    /// Is a release under `publisher`, signed by `k` and published at `published`, to be trusted?
    pub fn trusts_release(&self, publisher: &str, k: &PublicKey, published: &str) -> Result<(), TrustError> {
        match self.status(publisher, k) {
            Some(KeyStatus::Active) => Ok(()),
            Some(KeyStatus::Retired(at)) if published < at => Ok(()),
            Some(KeyStatus::Retired(_)) => Err(TrustError::RetiredKey),
            Some(KeyStatus::Revoked(_)) => Err(TrustError::RevokedKey),
            None => Err(TrustError::UnknownKey),
        }
    }
}

// ---- snapshot and timestamp --------------------------------------------------------------------------

/// A snapshot: the version and digest of `namespaces`, and the digest of every package index.
#[derive(Clone, Copy, Debug)]
pub struct Snapshot<'a> {
    /// Version and expiry.
    pub header: Header<'a>,
    /// The `namespaces` version this snapshot was made with.
    pub namespaces_version: u64,
    /// The digest of that `namespaces` document.
    pub namespaces_digest: Sha256Digest,
    body: &'a str,
}

impl<'a> Snapshot<'a> {
    /// Read a snapshot: `namespaces`, then `index` lines in ascending order of package name.
    pub fn parse(text: &'a str) -> Result<Snapshot<'a>, TrustError> {
        let (header, mut r) = header(text, Role::Snapshot)?;
        let [v, d] = fields::<2>(string(r.field("namespaces")?)?)?;
        let (namespaces_version, namespaces_digest) = (int(v)?, digest(d)?);
        let body = r.rest;
        let mut previous: Option<&str> = None;
        while r.peek_name().is_some() {
            let [name, d] = fields::<2>(string(r.field("index")?)?)?;
            PackageName::parse(name).map_err(|_| malformed("an index line names an invalid package"))?;
            digest(d)?;
            if previous.is_some_and(|p| p >= name) {
                return Err(malformed("index lines are not in ascending order, or repeat"));
            }
            previous = Some(name);
        }
        r.end()?;
        Ok(Snapshot {
            header,
            namespaces_version,
            namespaces_digest,
            body,
        })
    }

    /// The digest of `name`'s index, if the snapshot lists it.
    pub fn index(&self, name: &str) -> Option<Sha256Digest> {
        let mut r = Reader { rest: self.body };
        while r.peek_name().is_some() {
            let [n, d] = fields::<2>(string(r.field("index").ok()?).ok()?).ok()?;
            if n == name {
                return digest(d).ok();
            }
        }
        None
    }
}

/// A timestamp: the version and digest of the current snapshot.
#[derive(Clone, Copy, Debug)]
pub struct Timestamp<'a> {
    /// Version and expiry.
    pub header: Header<'a>,
    /// The current snapshot's version.
    pub snapshot_version: u64,
    /// The current snapshot's digest.
    pub snapshot_digest: Sha256Digest,
}

impl<'a> Timestamp<'a> {
    /// Read a timestamp.
    pub fn parse(text: &'a str) -> Result<Timestamp<'a>, TrustError> {
        let (header, mut r) = header(text, Role::Timestamp)?;
        let [v, d] = fields::<2>(string(r.field("snapshot")?)?)?;
        r.end()?;
        Ok(Timestamp {
            header,
            snapshot_version: int(v)?,
            snapshot_digest: digest(d)?,
        })
    }
}

// ---- signatures --------------------------------------------------------------------------------------

/// The exact bytes a metadata signature signs: the role's purpose tag, then the document's SHA-256.
fn message(role: Role, body: &[u8]) -> ([u8; 64], usize) {
    message_of(role, &Sha256Digest::of(body))
}

/// [`message`], from the document's digest: what lets a client verify a document it never held whole.
fn message_of(role: Role, digest: &Sha256Digest) -> ([u8; 64], usize) {
    let tag = role.context();
    let mut msg = [0u8; 64];
    msg[..tag.len()].copy_from_slice(tag);
    msg[tag.len()..tag.len() + 32].copy_from_slice(&digest.0);
    (msg, tag.len() + 32)
}

/// Count the valid signatures in `sigs` over `body` (a `role` document) by keys `accept` admits, and require
/// at least `threshold`. Each key counts once: the file lists keys in strictly ascending order.
pub fn verify_signatures(
    role: Role,
    body: &[u8],
    sigs: &str,
    threshold: u32,
    accept: impl Fn(&PublicKey) -> bool,
) -> Result<(), TrustError> {
    verify_signatures_of(role, &Sha256Digest::of(body), sigs, threshold, accept)
}

/// [`verify_signatures`], given the document's SHA-256 rather than its bytes.
pub fn verify_signatures_of(
    role: Role,
    digest: &Sha256Digest,
    sigs: &str,
    threshold: u32,
    accept: impl Fn(&PublicKey) -> bool,
) -> Result<(), TrustError> {
    let mut r = Reader { rest: sigs };
    match r.field("format")? {
        "1" => {}
        n if int(n).is_ok() => return Err(TrustError::UnknownFormat),
        _ => return Err(malformed("format is not an integer")),
    }
    let (msg, len) = message_of(role, digest);
    let (mut have, mut previous) = (0u32, None);
    while r.peek_name().is_some() {
        let [k, s] = fields::<2>(string(r.field("signature")?)?)?;
        let k = key(k)?;
        let s: Signature = s
            .parse()
            .map_err(|_| malformed("a signature is not ed25519:<128 hex digits>"))?;
        if previous.is_some_and(|p| p >= k) {
            return Err(malformed(
                "signatures are not in ascending order of key, or a key repeats",
            ));
        }
        previous = Some(k);
        if accept(&k) {
            let ok = VerifyingKey::from_bytes(&k.0)
                .map(|vk| {
                    vk.verify_strict(&msg[..len], &ed25519_dalek::Signature::from_bytes(&s.0))
                        .is_ok()
                })
                .unwrap_or(false);
            if ok {
                have += 1;
            }
        }
    }
    r.end()?;
    if have >= threshold {
        Ok(())
    } else {
        Err(TrustError::NotEnoughSignatures {
            role,
            have,
            need: threshold,
        })
    }
}

/// Is `sigs` signed by at least `root`'s threshold of `role` keys, over `body`?
pub fn verify_by(root: &Root<'_>, role: Role, body: &[u8], sigs: &str) -> Result<(), TrustError> {
    verify_signatures(role, body, sigs, root.threshold(role), |k| root.has_key(role, k))
}

// ---- the client's checks, in the order it makes them ------------------------------------------------------

/// Accept root N+1, given trusted root N: it must be signed by N's root threshold AND by its own, and be
/// exactly the next version. (Expiry is checked only on the last root of a chain: an old root may expire.)
pub fn verify_next_root<'a>(trusted: &Root<'_>, text: &'a str, sigs: &str) -> Result<Root<'a>, TrustError> {
    let next = Root::parse(text)?;
    if next.header.version != trusted.header.version + 1 {
        return Err(TrustError::WrongVersion(Role::Root));
    }
    verify_by(trusted, Role::Root, text.as_bytes(), sigs)?;
    verify_by(&next, Role::Root, text.as_bytes(), sigs)?;
    Ok(next)
}

/// Accept a timestamp: signed by the root's timestamp keys, unexpired, and not older than `last_version`.
pub fn verify_timestamp<'a>(
    root: &Root<'_>,
    text: &'a str,
    sigs: &str,
    now: &str,
    last_version: Option<u64>,
) -> Result<Timestamp<'a>, TrustError> {
    let t = Timestamp::parse(text)?;
    verify_by(root, Role::Timestamp, text.as_bytes(), sigs)?;
    if last_version.is_some_and(|v| t.header.version < v) {
        return Err(TrustError::Rollback(Role::Timestamp));
    }
    t.header.check_fresh(now)?;
    Ok(t)
}

/// Accept a snapshot: the one the timestamp names (digest and version), signed by the snapshot keys,
/// unexpired.
pub fn verify_snapshot<'a>(
    root: &Root<'_>,
    timestamp: &Timestamp<'_>,
    text: &'a str,
    sigs: &str,
    now: &str,
) -> Result<Snapshot<'a>, TrustError> {
    if Sha256Digest::of(text.as_bytes()) != timestamp.snapshot_digest {
        return Err(TrustError::DigestMismatch("the snapshot"));
    }
    let s = Snapshot::parse(text)?;
    if s.header.version != timestamp.snapshot_version {
        return Err(TrustError::WrongVersion(Role::Snapshot));
    }
    verify_by(root, Role::Snapshot, text.as_bytes(), sigs)?;
    s.header.check_fresh(now)?;
    Ok(s)
}

/// Accept a namespaces document: the one the snapshot names, signed by the root's namespaces keys (offline),
/// unexpired, and not older than `last_version`.
pub fn verify_namespaces<'a>(
    root: &Root<'_>,
    snapshot: &Snapshot<'_>,
    text: &'a str,
    sigs: &str,
    now: &str,
    last_version: Option<u64>,
) -> Result<Namespaces<'a>, TrustError> {
    if Sha256Digest::of(text.as_bytes()) != snapshot.namespaces_digest {
        return Err(TrustError::DigestMismatch("the namespaces metadata"));
    }
    let n = Namespaces::parse(text)?;
    if n.header.version != snapshot.namespaces_version {
        return Err(TrustError::WrongVersion(Role::Namespaces));
    }
    verify_by(root, Role::Namespaces, text.as_bytes(), sigs)?;
    if last_version.is_some_and(|v| n.header.version < v) {
        return Err(TrustError::Rollback(Role::Namespaces));
    }
    n.header.check_fresh(now)?;
    Ok(n)
}

/// Accept a package index: exactly the bytes the snapshot names for it.
pub fn verify_index(snapshot: &Snapshot<'_>, name: &str, index_text: &str) -> Result<(), TrustError> {
    match snapshot.index(name) {
        None => Err(TrustError::NotInSnapshot),
        Some(d) if d == Sha256Digest::of(index_text.as_bytes()) => Ok(()),
        Some(_) => Err(TrustError::DigestMismatch("the package index")),
    }
}

// ---- the snapshot as a stream: for a client that keeps it on disk instead of in memory ----------------------

/// A snapshot's HEAD: everything before its `index` lines.
///
/// A snapshot holds one line per package, so it grows with the registry; a client with a fixed amount of memory
/// (the native package manager has no heap) cannot hold it whole. Because a signature covers the document's
/// SHA-256 and not its bytes, it need not: it hashes the snapshot as it arrives, keeps it on disk, verifies the
/// signatures from the digest ([`verify_snapshot_stream`]), and reads it back a line at a time ([`IndexLines`]).
/// The head is the small fixed part such a reader parses up front.
#[derive(Clone, Copy, Debug)]
pub struct SnapshotHead<'a> {
    /// Version and expiry.
    pub header: Header<'a>,
    /// The `namespaces` version this snapshot was made with.
    pub namespaces_version: u64,
    /// The digest of that `namespaces` document.
    pub namespaces_digest: Sha256Digest,
    /// How many bytes of the snapshot the head is: its `index` lines start here.
    pub len: usize,
}

impl<'a> SnapshotHead<'a> {
    /// Read the head from the start of a snapshot. `text` may go on past it (into the `index` lines); only the
    /// head is read, and it must be whole.
    pub fn parse(text: &'a str) -> Result<SnapshotHead<'a>, TrustError> {
        let (header, mut r) = header(text, Role::Snapshot)?;
        let [v, d] = fields::<2>(string(r.field("namespaces")?)?)?;
        Ok(SnapshotHead {
            header,
            namespaces_version: int(v)?,
            namespaces_digest: digest(d)?,
            len: text.len() - r.rest.len(),
        })
    }
}

/// A snapshot's `index` lines, one at a time, checked exactly as [`Snapshot::parse`] checks them: each names a
/// valid package and a digest, and the names are in strictly ascending order. Feed it every line after the head,
/// in order, without its newline.
pub struct IndexLines {
    previous: [u8; PREVIOUS_MAX],
    plen: usize,
    started: bool,
}

/// The longest package name an index line may hold (`PackageName`'s own bound).
const PREVIOUS_MAX: usize = 128;

impl Default for IndexLines {
    fn default() -> Self { IndexLines::new() }
}

impl IndexLines {
    /// A reader at the first `index` line.
    pub const fn new() -> IndexLines {
        IndexLines { previous: [0; PREVIOUS_MAX], plen: 0, started: false }
    }

    /// The next line: its package name and index digest.
    pub fn next<'l>(&mut self, line: &'l str) -> Result<(&'l str, Sha256Digest), TrustError> {
        let (n, v) = line.split_once(" = ").ok_or(malformed("a line is not `name = value`"))?;
        if n != "index" {
            return Err(malformed("a field is missing or out of order"));
        }
        let [name, d] = fields::<2>(string(v)?)?;
        PackageName::parse(name).map_err(|_| malformed("an index line names an invalid package"))?;
        let d = digest(d)?;
        if name.len() > PREVIOUS_MAX {
            return Err(malformed("an index line names an invalid package"));
        }
        if self.started && name.as_bytes() <= &self.previous[..self.plen] {
            return Err(malformed("index lines are not in ascending order, or repeat"));
        }
        self.previous[..name.len()].copy_from_slice(name.as_bytes());
        self.plen = name.len();
        self.started = true;
        Ok((name, d))
    }
}

/// Accept a snapshot held on disk, from its SHA-256, its byte length and its head: the one the timestamp names,
/// signed by the snapshot keys, unexpired. The caller checks every `index` line with [`IndexLines`] as it reads
/// them; together that is [`verify_snapshot`], without the document in memory.
pub fn verify_snapshot_stream(
    root: &Root<'_>,
    timestamp: &Timestamp<'_>,
    digest: &Sha256Digest,
    head: &SnapshotHead<'_>,
    sigs: &str,
    now: &str,
) -> Result<(), TrustError> {
    if *digest != timestamp.snapshot_digest {
        return Err(TrustError::DigestMismatch("the snapshot"));
    }
    if head.header.version != timestamp.snapshot_version {
        return Err(TrustError::WrongVersion(Role::Snapshot));
    }
    verify_signatures_of(Role::Snapshot, digest, sigs, root.threshold(Role::Snapshot), |k| root.has_key(Role::Snapshot, k))?;
    head.header.check_fresh(now)
}

/// Accept a namespaces document against a snapshot read as a stream: [`verify_namespaces`], given the head.
pub fn verify_namespaces_by_head<'a>(
    root: &Root<'_>,
    head: &SnapshotHead<'_>,
    text: &'a str,
    sigs: &str,
    now: &str,
    last_version: Option<u64>,
) -> Result<Namespaces<'a>, TrustError> {
    if Sha256Digest::of(text.as_bytes()) != head.namespaces_digest {
        return Err(TrustError::DigestMismatch("the namespaces metadata"));
    }
    let n = Namespaces::parse(text)?;
    if n.header.version != head.namespaces_version {
        return Err(TrustError::WrongVersion(Role::Namespaces));
    }
    verify_by(root, Role::Namespaces, text.as_bytes(), sigs)?;
    if last_version.is_some_and(|v| n.header.version < v) {
        return Err(TrustError::Rollback(Role::Namespaces));
    }
    n.header.check_fresh(now)?;
    Ok(n)
}

/// Accept a package index against the digest an [`IndexLines`] reader found for it.
pub fn verify_index_digest(listed: &Sha256Digest, index_text: &str) -> Result<(), TrustError> {
    if *listed == Sha256Digest::of(index_text.as_bytes()) { Ok(()) } else { Err(TrustError::DigestMismatch("the package index")) }
}

// ---- signing and writing (the maintainer's tool, the registry) -------------------------------------------

/// Sign a `role` document's exact bytes with the key whose secret seed is `seed`.
pub fn sign_metadata(role: Role, body: &[u8], seed: &[u8; 32]) -> (PublicKey, Signature) {
    let key = SigningKey::from_bytes(seed);
    let (msg, len) = message(role, body);
    (
        PublicKey(key.verifying_key().to_bytes()),
        Signature(key.sign(&msg[..len]).to_bytes()),
    )
}

#[cfg(feature = "std")]
pub use write::*;

#[cfg(feature = "std")]
mod write {
    use super::*;
    use std::fmt::Write as _;
    use std::string::{String, ToString};
    use std::vec::Vec;

    fn header(out: &mut String, role: Role, version: u64, expires: &str) {
        let _ = write!(
            out,
            "format = {METADATA_FORMAT}\ntype = \"{role}\"\nversion = {version}\nexpires = \"{expires}\"\n"
        );
    }

    /// Write a root document. Keys are sorted; each role's threshold must not exceed its key count.
    pub fn write_root(version: u64, expires: &str, roles: &[(Role, u32, Vec<PublicKey>)]) -> String {
        let mut out = String::new();
        header(&mut out, Role::Root, version, expires);
        for role in Role::ALL {
            let (_, threshold, keys) = roles.iter().find(|(r, ..)| *r == role).expect("every role is given");
            let _ = writeln!(out, "role = \"{role} {threshold}\"");
            let mut keys = keys.clone();
            keys.sort();
            keys.dedup();
            for k in keys {
                let _ = writeln!(out, "key = \"{role} {k}\"");
            }
        }
        out
    }

    /// One publisher key in a namespaces document, owned.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct NamespaceKey {
        /// The publisher.
        pub publisher: String,
        /// The key.
        pub key: PublicKey,
        /// `None` if active; else `("retired" | "revoked", when)`.
        pub ended: Option<(&'static str, String)>,
    }

    /// Write a namespaces document, entries sorted by publisher then key.
    pub fn write_namespaces(version: u64, expires: &str, keys: &[NamespaceKey]) -> String {
        let mut out = String::new();
        header(&mut out, Role::Namespaces, version, expires);
        let mut keys = keys.to_vec();
        keys.sort_by(|a, b| (&a.publisher, a.key).cmp(&(&b.publisher, b.key)));
        for k in keys {
            match &k.ended {
                None => {
                    let _ = writeln!(out, "key = \"{} active {}\"", k.publisher, k.key);
                }
                Some((how, when)) => {
                    let _ = writeln!(out, "key = \"{} {how} {when} {}\"", k.publisher, k.key);
                }
            }
        }
        out
    }

    /// Write a snapshot, index lines sorted by package name.
    pub fn write_snapshot(
        version: u64,
        expires: &str,
        namespaces: (u64, Sha256Digest),
        indexes: &[(String, Sha256Digest)],
    ) -> String {
        let mut out = String::new();
        header(&mut out, Role::Snapshot, version, expires);
        let _ = writeln!(out, "namespaces = \"{} {}\"", namespaces.0, namespaces.1);
        let mut indexes = indexes.to_vec();
        indexes.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, d) in indexes {
            let _ = writeln!(out, "index = \"{name} {d}\"");
        }
        out
    }

    /// Write a timestamp.
    pub fn write_timestamp(version: u64, expires: &str, snapshot: (u64, Sha256Digest)) -> String {
        let mut out = String::new();
        header(&mut out, Role::Timestamp, version, expires);
        let _ = writeln!(out, "snapshot = \"{} {}\"", snapshot.0, snapshot.1);
        out
    }

    /// Write a signature file from signatures, sorted by key; duplicates keep one.
    pub fn write_signatures(sigs: &[(PublicKey, Signature)]) -> String {
        let mut sigs = sigs.to_vec();
        sigs.sort_by_key(|(k, _)| *k);
        sigs.dedup_by_key(|(k, _)| *k);
        let mut out = String::from("format = 1\n");
        for (k, s) in sigs {
            let _ = writeln!(out, "signature = \"{k} {s}\"");
        }
        out
    }

    /// Sign a document with every given seed and write the signature file.
    pub fn sign_document(role: Role, body: &str, seeds: &[[u8; 32]]) -> String {
        let sigs: Vec<_> = seeds.iter().map(|s| sign_metadata(role, body.as_bytes(), s)).collect();
        write_signatures(&sigs)
    }

    /// The `Retired`/`Revoked` labels as `write_namespaces` expects them.
    pub fn ended(how: &'static str, when: &str) -> Option<(&'static str, String)> {
        Some((how, when.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::string::String;
    use std::vec;
    use std::vec::Vec;

    const NOW: &str = "2026-10-15T12:00:00Z";
    const LATER: &str = "2027-10-15T00:00:00Z";
    const SOON: &str = "2026-10-16T12:00:00Z";

    fn seed(n: u8) -> [u8; 32] {
        [n; 32]
    }
    fn pk(n: u8) -> PublicKey {
        crate::ManifestSignature::public_key(&seed(n))
    }

    /// Root keys 1, 2, 3 (threshold 2); namespaces keys 1, 2 (threshold 2); snapshot 10; timestamp 11.
    fn root_text(version: u64, root_keys: &[u8], threshold: u32) -> String {
        write_root(
            version,
            LATER,
            &[
                (Role::Root, threshold, root_keys.iter().map(|&n| pk(n)).collect()),
                (Role::Namespaces, 2, vec![pk(1), pk(2)]),
                (Role::Snapshot, 1, vec![pk(10)]),
                (Role::Timestamp, 1, vec![pk(11)]),
            ],
        )
    }

    #[test]
    fn root_round_trips_and_answers_questions() {
        let text = root_text(1, &[3, 1, 2], 2);
        let root = Root::parse(&text).unwrap();
        assert_eq!(root.header.version, 1);
        assert_eq!(root.threshold(Role::Root), 2);
        assert_eq!(root.threshold(Role::Snapshot), 1);
        assert!(root.has_key(Role::Root, &pk(3)));
        assert!(!root.has_key(Role::Root, &pk(10)), "a key belongs to its role only");
        assert!(root.has_key(Role::Snapshot, &pk(10)));
    }

    #[test]
    fn root_refuses_non_canonical_text() {
        let text = root_text(1, &[1, 2, 3], 2);
        for (bad, why) in [
            (text.replace("version = 1", "version = 01"), "leading zero"),
            (
                text.replace("role = \"root 2\"", "role = \"root 4\""),
                "threshold above key count",
            ),
            (text.replace("role = \"root 2\"", "role = \"root 0\""), "zero threshold"),
            (std::format!("{text}\n"), "trailing blank line"),
            (text.replace('\n', "\r\n"), "CRLF"),
            (
                text.replacen("key = \"root", "key = \"snapshot", 1),
                "key under wrong role",
            ),
        ] {
            assert!(Root::parse(&bad).is_err(), "{why}");
        }
        // Keys out of order: swap the first two root key lines.
        let lines: Vec<&str> = text.lines().collect();
        let i = lines.iter().position(|l| l.starts_with("key = \"root")).unwrap();
        let mut swapped = lines.clone();
        swapped.swap(i, i + 1);
        let swapped = swapped.join("\n") + "\n";
        assert_eq!(
            Root::parse(&swapped).unwrap_err(),
            TrustError::Malformed("a role's keys are not in ascending order, or repeat")
        );
        assert_eq!(
            Root::parse(&text.replace("type = \"root\"", "type = \"snapshot\"")).unwrap_err(),
            TrustError::WrongType
        );
    }

    #[test]
    fn thresholds_count_distinct_valid_signatures_by_the_role() {
        let text = root_text(1, &[1, 2, 3], 2);
        let root = Root::parse(&text).unwrap();
        let ok = sign_document(Role::Root, &text, &[seed(1), seed(3)]);
        assert_eq!(verify_by(&root, Role::Root, text.as_bytes(), &ok), Ok(()));

        let one = sign_document(Role::Root, &text, &[seed(1)]);
        assert_eq!(
            verify_by(&root, Role::Root, text.as_bytes(), &one),
            Err(TrustError::NotEnoughSignatures {
                role: Role::Root,
                have: 1,
                need: 2
            })
        );
        // Keys of another role do not count, however many.
        let wrong = sign_document(Role::Root, &text, &[seed(1), seed(10), seed(11)]);
        assert!(matches!(
            verify_by(&root, Role::Root, text.as_bytes(), &wrong),
            Err(TrustError::NotEnoughSignatures { have: 1, .. })
        ));
        // A signature made for another document type does not count.
        let as_snapshot = sign_document(Role::Snapshot, &text, &[seed(1), seed(2)]);
        assert!(matches!(
            verify_by(&root, Role::Root, text.as_bytes(), &as_snapshot),
            Err(TrustError::NotEnoughSignatures { have: 0, .. })
        ));
        // The same key twice is refused, not counted twice.
        let line = ok.lines().nth(1).unwrap();
        let doubled = std::format!("format = 1\n{line}\n{line}\n");
        assert!(matches!(
            verify_by(&root, Role::Root, text.as_bytes(), &doubled),
            Err(TrustError::Malformed(_))
        ));
    }

    #[test]
    fn a_root_rotates_only_to_the_next_version_signed_by_old_and_new_keys() {
        let v1 = root_text(1, &[1, 2, 3], 2);
        let r1 = Root::parse(&v1).unwrap();
        // Root keys rotate from {1,2,3} to {2,3,4}.
        let v2 = root_text(2, &[2, 3, 4], 2);
        let by_both = sign_document(Role::Root, &v2, &[seed(2), seed(3)]);
        assert!(verify_next_root(&r1, &v2, &by_both).is_ok());
        // Signed only by the new keys that the old root does not know: not enough old signatures.
        let new_only = sign_document(Role::Root, &v2, &[seed(4), seed(3)]);
        assert!(matches!(
            verify_next_root(&r1, &v2, &new_only),
            Err(TrustError::NotEnoughSignatures { have: 1, .. })
        ));
        // Skipping a version.
        let v3 = root_text(3, &[1, 2, 3], 2);
        let s3 = sign_document(Role::Root, &v3, &[seed(1), seed(2)]);
        assert_eq!(
            verify_next_root(&r1, &v3, &s3).unwrap_err(),
            TrustError::WrongVersion(Role::Root)
        );
    }

    #[test]
    fn a_snapshot_read_as_a_stream_is_checked_as_the_whole_one_is() {
        let root_t = root_text(1, &[1, 2, 3], 2);
        let root = Root::parse(&root_t).unwrap();
        let ns = write_namespaces(4, LATER, &[NamespaceKey { publisher: "alice".into(), key: pk(20), ended: None }]);
        let (ia, ib) = ("format = 1
name = \"alice/a\"
", "format = 1
name = \"alice/b\"
");
        let snap = write_snapshot(
            7,
            SOON,
            (4, Sha256Digest::of(ns.as_bytes())),
            &[
                ("alice/a".into(), Sha256Digest::of(ia.as_bytes())),
                ("alice/b".into(), Sha256Digest::of(ib.as_bytes())),
            ],
        );
        let ts = write_timestamp(9, SOON, (7, Sha256Digest::of(snap.as_bytes())));
        let t = verify_timestamp(&root, &ts, &sign_document(Role::Timestamp, &ts, &[seed(11)]), NOW, None).unwrap();
        let sig = sign_document(Role::Snapshot, &snap, &[seed(10)]);
        let digest = Sha256Digest::of(snap.as_bytes());

        // The head, then every index line through one reader: the same answer as the whole document.
        let head = SnapshotHead::parse(&snap).unwrap();
        assert_eq!(verify_snapshot_stream(&root, &t, &digest, &head, &sig, NOW), Ok(()));
        let whole = verify_snapshot(&root, &t, &snap, &sig, NOW).unwrap();
        assert_eq!(head.namespaces_digest, whole.namespaces_digest);
        let mut lines = IndexLines::new();
        let mut seen = 0;
        for line in snap[head.len..].lines() {
            let (name, d) = lines.next(line).unwrap();
            assert_eq!(Some(d), whole.index(name));
            seen += 1;
        }
        assert_eq!(seen, 2);
        assert_eq!(verify_index_digest(&whole.index("alice/a").unwrap(), ia), Ok(()));
        assert!(verify_index_digest(&whole.index("alice/a").unwrap(), ib).is_err());
        let ns_sig = sign_document(Role::Namespaces, &ns, &[seed(1), seed(2)]);
        assert!(verify_namespaces_by_head(&root, &head, &ns, &ns_sig, NOW, None).is_ok());

        // Refused as the whole one is: another document's digest, a signature by the wrong key, out of order.
        let other = Sha256Digest::of(b"another snapshot");
        assert_eq!(verify_snapshot_stream(&root, &t, &other, &head, &sig, NOW).unwrap_err(), TrustError::DigestMismatch("the snapshot"));
        let wrong = sign_document(Role::Snapshot, &snap, &[seed(11)]);
        assert!(matches!(verify_snapshot_stream(&root, &t, &digest, &head, &wrong, NOW), Err(TrustError::NotEnoughSignatures { .. })));
        let mut again = IndexLines::new();
        let b_line = snap[head.len..].lines().nth(1).unwrap();
        let a_line = snap[head.len..].lines().next().unwrap();
        assert!(again.next(b_line).is_ok());
        assert!(again.next(a_line).is_err());
        assert!(IndexLines::new().next("namespaces = \"4 sha256:00\"").is_err());
    }

    #[test]
    fn the_chain_from_timestamp_to_index() {
        let root_t = root_text(1, &[1, 2, 3], 2);
        let root = Root::parse(&root_t).unwrap();
        let ns = write_namespaces(
            4,
            LATER,
            &[NamespaceKey {
                publisher: "alice".into(),
                key: pk(20),
                ended: None,
            }],
        );
        let index = "format = 1\nname = \"alice/hello\"\n";
        let snap = write_snapshot(
            7,
            SOON,
            (4, Sha256Digest::of(ns.as_bytes())),
            &[("alice/hello".into(), Sha256Digest::of(index.as_bytes()))],
        );
        let ts = write_timestamp(9, SOON, (7, Sha256Digest::of(snap.as_bytes())));
        let ts_sig = sign_document(Role::Timestamp, &ts, &[seed(11)]);
        let snap_sig = sign_document(Role::Snapshot, &snap, &[seed(10)]);
        let ns_sig = sign_document(Role::Namespaces, &ns, &[seed(1), seed(2)]);

        let t = verify_timestamp(&root, &ts, &ts_sig, NOW, Some(9)).unwrap();
        let s = verify_snapshot(&root, &t, &snap, &snap_sig, NOW).unwrap();
        let n = verify_namespaces(&root, &s, &ns, &ns_sig, NOW, None).unwrap();
        assert_eq!(verify_index(&s, "alice/hello", index), Ok(()));
        assert!(n.may_sign("alice", &pk(20)));

        // Rollback, expiry, a swapped snapshot, a tampered index, an unknown package.
        assert_eq!(
            verify_timestamp(&root, &ts, &ts_sig, NOW, Some(10)).unwrap_err(),
            TrustError::Rollback(Role::Timestamp)
        );
        assert_eq!(
            verify_timestamp(&root, &ts, &ts_sig, LATER, None).unwrap_err(),
            TrustError::Expired(Role::Timestamp)
        );
        let other_snap = snap.replace("version = 7", "version = 8");
        assert_eq!(
            verify_snapshot(&root, &t, &other_snap, &snap_sig, NOW).unwrap_err(),
            TrustError::DigestMismatch("the snapshot")
        );
        assert_eq!(
            verify_index(&s, "alice/hello", "format = 1\n").unwrap_err(),
            TrustError::DigestMismatch("the package index")
        );
        assert_eq!(
            verify_index(&s, "bob/hello", index).unwrap_err(),
            TrustError::NotInSnapshot
        );
        // The online keys cannot sign namespaces: a registry holding only them cannot add a key.
        let forged = sign_document(Role::Namespaces, &ns, &[seed(10), seed(11)]);
        assert!(matches!(
            verify_namespaces(&root, &s, &ns, &forged, NOW, None),
            Err(TrustError::NotEnoughSignatures { have: 0, .. })
        ));
    }

    #[test]
    fn retired_keys_keep_their_past_and_revoked_keys_lose_it() {
        let ns_t = write_namespaces(
            1,
            LATER,
            &[
                NamespaceKey {
                    publisher: "alice".into(),
                    key: pk(20),
                    ended: ended("retired", "2026-10-01T00:00:00Z"),
                },
                NamespaceKey {
                    publisher: "alice".into(),
                    key: pk(21),
                    ended: None,
                },
                NamespaceKey {
                    publisher: "alice".into(),
                    key: pk(22),
                    ended: ended("revoked", "2026-10-10T00:00:00Z"),
                },
            ],
        );
        let ns = Namespaces::parse(&ns_t).unwrap();
        // Active: signs new releases, and is trusted.
        assert!(ns.may_sign("alice", &pk(21)));
        assert_eq!(ns.trusts_release("alice", &pk(21), NOW), Ok(()));
        // Retired: signs nothing new; trusted for releases published before its retirement only.
        assert!(!ns.may_sign("alice", &pk(20)));
        assert_eq!(ns.trusts_release("alice", &pk(20), "2026-09-30T00:00:00Z"), Ok(()));
        assert_eq!(
            ns.trusts_release("alice", &pk(20), "2026-10-02T00:00:00Z"),
            Err(TrustError::RetiredKey)
        );
        // Revoked: nothing it ever signed, however early.
        assert!(!ns.may_sign("alice", &pk(22)));
        assert_eq!(
            ns.trusts_release("alice", &pk(22), "2020-01-01T00:00:00Z"),
            Err(TrustError::RevokedKey)
        );
        // Another publisher's key, or a stranger's.
        assert_eq!(ns.trusts_release("bob", &pk(21), NOW), Err(TrustError::UnknownKey));
        assert_eq!(ns.trusts_release("alice", &pk(99), NOW), Err(TrustError::UnknownKey));
    }

    #[test]
    fn every_document_round_trips_exactly() {
        let root = root_text(1, &[1, 2, 3], 2);
        assert!(Root::parse(&root).is_ok());
        let ns = write_namespaces(
            2,
            LATER,
            &[NamespaceKey {
                publisher: "bob".into(),
                key: pk(5),
                ended: None,
            }],
        );
        assert_eq!(Namespaces::parse(&ns).unwrap().header.version, 2);
        let snap = write_snapshot(3, SOON, (2, Sha256Digest::of(ns.as_bytes())), &[]);
        assert_eq!(Snapshot::parse(&snap).unwrap().namespaces_version, 2);
        let ts = write_timestamp(4, SOON, (3, Sha256Digest::of(snap.as_bytes())));
        assert_eq!(Timestamp::parse(&ts).unwrap().snapshot_version, 3);
    }
}
