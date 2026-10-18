//! `godspeed fetch <publisher>/<name>[@<version>]`: download a published release and verify it, through the
//! registry's whole chain of trust, before writing anything where it will stay.
//!
//!   root (trusted, then every rotation since) -> timestamp -> snapshot -> namespaces -> index
//!     -> manifest (by digest) -> manifest.sig (by a key namespaces vouch for, for this release) -> files
//!
//! The first contact with a registry trusts its root version 1 - on first use, unless `--trust-root` pins its
//! digest, or it is the official registry, whose root 1 digest is built into this CLI - and remembers it;
//! from then on the CLI only moves forward: each new root must be signed by the
//! previous one, and an older timestamp or namespaces document than one already seen is refused as a rollback.
//! This is what a Godspeed machine's package manager will do before installing; `fetch` does it on a
//! workstation, where it can be inspected.

use super::verify;
use crate::diag::{Diagnostic, Kind, Result, Stage};
use crate::registry::{self, Answer, Client, Credentials};
use godspeed_package::trust::{self, Root, TrustError};
use godspeed_package::{self as pkg, ManifestSignature, PackageName, Sha256Digest, Version, SIGNATURE_FILE};
use std::path::{Path, PathBuf};

/// How large a metadata document may be. Releases themselves are bounded by the manifest.
const METADATA_LIMIT: u64 = 4 * 1024 * 1024;

fn untrusted(doing: &str, e: TrustError) -> Diagnostic {
    let next = match e {
        TrustError::Expired(trust::Role::Timestamp) => {
            "The registry has not re-signed its timestamp: it may be down, or be showing a frozen view. Try again \
             later; do not install meanwhile."
        }
        TrustError::NotInSnapshot => {
            "If the release was published moments ago, the registry may not have re-signed its snapshot yet; try \
             again shortly."
        }
        TrustError::RevokedKey | TrustError::RetiredKey | TrustError::UnknownKey => {
            "Do not install it. Ask the publisher for a release signed by one of their current keys."
        }
        _ => "Do not install it. The registry's signed metadata does not hold together; report it if it persists.",
    };
    Diagnostic::new(Kind::Untrusted, Stage::Verification, doing.to_string(), format!("{e}.")).next(next)
}

fn now_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let z = days + 719_468;
    let (era, doe) = (z / 146_097, z % 146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// What this machine trusts of one registry: its latest root, and the newest versions it has seen.
struct TrustState {
    dir: PathBuf,
    root: String,
    timestamp_version: Option<u64>,
    namespaces_version: Option<u64>,
}

fn state_dir(url: &str) -> std::result::Result<PathBuf, Diagnostic> {
    let name: String = url
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    registry::config_dir()
        .map(|d| d.join("trust").join(name))
        .ok_or_else(|| {
            Diagnostic::new(
                Kind::Environment,
                Stage::Setup,
                "remember the registry's trust",
                "No configuration directory.",
            )
        })
}

impl TrustState {
    fn load(url: &str) -> std::result::Result<Option<TrustState>, Diagnostic> {
        let dir = state_dir(url)?;
        let Ok(root) = std::fs::read_to_string(dir.join("root.toml")) else {
            return Ok(None);
        };
        let state: toml::Table = std::fs::read_to_string(dir.join("state.toml"))
            .ok()
            .and_then(|t| t.parse().ok())
            .unwrap_or_default();
        let int = |k: &str| {
            state
                .get(k)
                .and_then(|v| v.as_integer())
                .and_then(|v| u64::try_from(v).ok())
        };
        Ok(Some(TrustState {
            dir,
            root,
            timestamp_version: int("timestamp_version"),
            namespaces_version: int("namespaces_version"),
        }))
    }

    fn save(&self) -> std::result::Result<(), Diagnostic> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| Diagnostic::io("remember the registry's trust", &self.dir, &e))?;
        let mut state = String::new();
        if let Some(v) = self.timestamp_version {
            state.push_str(&format!("timestamp_version = {v}\n"));
        }
        if let Some(v) = self.namespaces_version {
            state.push_str(&format!("namespaces_version = {v}\n"));
        }
        for (file, text) in [("root.toml", &self.root), ("state.toml", &state)] {
            let path = self.dir.join(file);
            std::fs::write(&path, text).map_err(|e| Diagnostic::io("remember the registry's trust", &path, &e))?;
        }
        Ok(())
    }
}

/// Documents read from the registry, each under the path it was read at.
type Kept = Vec<(String, Vec<u8>)>;

/// The registry, and - when a `.gsp` is being made - every document read from it, under the path it was read at.
struct Source<'a> {
    client: &'a Client,
    kept: Option<std::cell::RefCell<Kept>>,
    /// `inspect <file>.gsp`: the documents the file holds, answered in place of the registry's.
    file: Option<&'a std::collections::HashMap<String, Vec<u8>>>,
}

impl Source<'_> {
    /// GET `path` from the registry, or from the file when there is one: a path the file does not hold is
    /// absent, as a document the registry does not have is.
    fn get(&self, path: &str, limit: u64) -> Answer<Option<Vec<u8>>> {
        match self.file {
            None => self.client.get(path, limit),
            Some(f) => match f.get(path) {
                None => Answer::Done(None),
                Some(b) if b.len() as u64 > limit => {
                    Answer::Unknown(format!("{path} in the file is larger than allowed"))
                }
                Some(b) => Answer::Done(Some(b.clone())),
            },
        }
    }

    fn keep(&self, path: &str, bytes: &[u8]) {
        if let Some(kept) = &self.kept {
            let mut kept = kept.borrow_mut();
            if !kept.iter().any(|(p, _)| p == path) {
                kept.push((path.to_string(), bytes.to_vec()));
            }
        }
    }
}

fn text(src: &Source, doing: &str, path: &str, limit: u64) -> std::result::Result<Option<String>, Diagnostic> {
    match src.get(path, limit) {
        Answer::Done(None) => Ok(None),
        Answer::Done(Some(b)) => String::from_utf8(b)
            .inspect(|t| src.keep(path, t.as_bytes()))
            .map(Some)
            .map_err(|_| untrusted(doing, TrustError::Malformed("a document is not UTF-8"))),
        Answer::Refused(r) => Err(registry::unreachable(doing, Stage::Verification, &r.message)),
        Answer::Unknown(why) => Err(registry::unreachable(doing, Stage::Verification, &why)),
    }
}

fn required(src: &Source, doing: &str, path: &str, limit: u64) -> std::result::Result<String, Diagnostic> {
    text(src, doing, path, limit)?.ok_or_else(|| {
        Diagnostic::new(
            Kind::Untrusted,
            Stage::Verification,
            doing.to_string(),
            format!("The registry has no {path}."),
        )
        .next("The registry is not publishing signed trust metadata; nothing from it can be verified.")
    })
}

/// `godspeed inspect`: every check a fetch makes, and nothing kept.
pub enum Inspect {
    /// A release on the registry.
    Registry,
    /// A `.gsp` file, checked with no network as of the day its chain was valid.
    File(PathBuf),
}

/// A `.gsp` read whole: its documents by the path the registry serves them at. Every rule of the layout is the
/// shared crate's, so this and the machine that installs it read the same file the same way.
fn read_gsp(path: &Path, doing: &str) -> std::result::Result<std::collections::HashMap<String, Vec<u8>>, Diagnostic> {
    let bad = |why: &str| {
        Diagnostic::new(
            Kind::InvalidPackage,
            Stage::Verification,
            doing.to_string(),
            format!("{}: {why}.", path.display()),
        )
    };
    let bytes = std::fs::read(path).map_err(|e| Diagnostic::io(doing, path, &e))?;
    let line_at = |at: usize| -> std::result::Result<(&str, usize), Diagnostic> {
        let rest = bytes.get(at..).unwrap_or_default();
        let n = rest
            .iter()
            .take(pkg::gsp::LINE_MAX + 1)
            .position(|&b| b == b'\n')
            .ok_or_else(|| bad("a line is too long or the file is cut short"))?;
        let text = std::str::from_utf8(&rest[..n]).map_err(|_| bad("a line is not text"))?;
        Ok((text, at + n + 1))
    };
    let (head, mut at) = line_at(0)?;
    pkg::gsp::header(head).map_err(|e| bad(e.message()))?;
    let mut docs = std::collections::HashMap::new();
    loop {
        let (text, next) = line_at(at)?;
        match pkg::gsp::line(text).map_err(|e| bad(e.message()))? {
            pkg::gsp::Line::End => {
                if next != bytes.len() {
                    return Err(bad("there are bytes after its end"));
                }
                return Ok(docs);
            }
            pkg::gsp::Line::Entry { size, path: p } => {
                let end = usize::try_from(size)
                    .ok()
                    .and_then(|s| next.checked_add(s))
                    .filter(|&e| e < bytes.len())
                    .ok_or_else(|| bad("an entry runs past its end"))?;
                if bytes[end] != b'\n' {
                    return Err(bad("an entry is not followed by its newline"));
                }
                if docs.len() == pkg::gsp::ENTRIES_MAX {
                    return Err(bad(pkg::gsp::GspError::TooMany.message()));
                }
                if docs.insert(p.to_string(), bytes[next..end].to_vec()).is_some() {
                    return Err(bad(pkg::gsp::GspError::Repeated.message()));
                }
                at = end + 1;
            }
        }
    }
}

/// `publisher/name@version`, from the one index and the one release a `.gsp` holds.
fn gsp_spec(
    docs: &std::collections::HashMap<String, Vec<u8>>,
    path: &Path,
    doing: &str,
) -> std::result::Result<String, Diagnostic> {
    let names: Vec<&str> = docs
        .keys()
        .filter_map(|k| k.strip_prefix("index/")?.strip_suffix(".toml"))
        .collect();
    let releases: Vec<&str> = docs
        .keys()
        .filter_map(|k| k.strip_prefix("packages/")?.strip_suffix("/manifest.toml"))
        .collect();
    match (names.as_slice(), releases.as_slice()) {
        ([n], [r]) => Ok(format!("{n}@{}", r.rsplit('/').next().unwrap_or(""))),
        // Unpublished: no index, so the release's own path names it.
        ([], [r]) if !docs.contains_key("trust/timestamp.toml") => match r.rsplit_once('/') {
            Some((n, v)) => Ok(format!("{n}@{v}")),
            None => Err(Diagnostic::new(
                Kind::InvalidPackage,
                Stage::Verification,
                doing.to_string(),
                format!("{} names its release by a path with no version.", path.display()),
            )),
        },
        _ => Err(Diagnostic::new(
            Kind::InvalidPackage,
            Stage::Verification,
            doing.to_string(),
            format!(
                "{} holds {} package(s) and {} release(s); a package file holds one of each.",
                path.display(),
                names.len(),
                releases.len()
            ),
        )),
    }
}

/// `godspeed inspect <file>.gsp` for an UNPUBLISHED file (`godspeed package --local`): its one release written to a
/// scratch folder and verified as any package directory is - every file the manifest names, by digest, and the
/// signature over the manifest - then removed. What it cannot say, and says: whose key that is.
fn inspect_unpublished(docs: &std::collections::HashMap<String, Vec<u8>>, path: &Path, json: bool) -> Result<()> {
    let doing = format!("inspect {}", path.display());
    let spec = gsp_spec(docs, path, &doing)?;
    let (name, version) = spec.split_once('@').unwrap_or((spec.as_str(), ""));
    let base = format!("packages/{name}/{version}/");
    let staging = std::env::temp_dir().join(format!("godspeed-inspect-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| Diagnostic::io(&doing, &staging, &e))?;
    for (p, bytes) in docs {
        let Some(leaf) = p.strip_prefix(&base) else {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(Diagnostic::new(
                Kind::InvalidPackage,
                Stage::Verification,
                doing.clone(),
                format!(
                    "it holds {p}, which is not part of {spec} - an unpublished package file holds its release alone."
                ),
            )
            .into());
        };
        let out = staging.join(leaf);
        std::fs::write(&out, bytes).map_err(|e| Diagnostic::io(&doing, &out, &e))?;
    }
    let verified = verify::run_with(&staging, true);
    let carried: Vec<String> = pkg::Arch::ALL
        .iter()
        .filter(|a| staging.join(a.artifact_file()).exists())
        .map(|a| a.to_string())
        .collect();
    let _ = std::fs::remove_dir_all(&staging);
    let verified = verified.map_err(|mut problems| {
        for p in &mut problems {
            p.headline = doing.clone();
        }
        problems
    })?;
    let Some(key) = verified.signed_by else {
        return Err(Diagnostic::new(Kind::SignatureFailure, Stage::Verification, doing, "It is not signed.".to_string())
            .next("A machine installs an unpublished package only if a key its operator trusts signed it: `godspeed package --local` signs.")
            .into());
    };
    let m = verified.manifest.manifest();
    if json {
        let doc = serde_json::json!({
            "name": m.name.to_string(),
            "version": m.version.to_string(),
            "inspected": true,
            "published": false,
            "carries": carried,
            "signed_by": key.to_string(),
            "requests": verified.requests,
        });
        println!("{doc:#}");
        return Ok(());
    }
    println!(
        "Inspected {}@{} in {}: nothing was kept",
        m.name,
        m.version,
        path.display()
    );
    println!("  UNPUBLISHED: no registry vouches for it, only its signature");
    println!("  carries: {}", carried.join(", "));
    println!("  signed by {key}");
    println!(
        "  requests: {}",
        if verified.requests.is_empty() {
            "nothing".to_string()
        } else {
            verified.requests.join("; ")
        }
    );
    println!(
        "A machine installs it only if its operator trusts that key for {}/:",
        m.name.publisher()
    );
    println!("  godspeed trust add {} {key}", m.name.publisher());
    Ok(())
}

/// Days since 1970-01-01 of `YYYY-MM-DD...`.
fn epoch_days(iso: &str) -> Option<i64> {
    let y: i64 = iso.get(0..4)?.parse().ok()?;
    let m: i64 = iso.get(5..7)?.parse().ok()?;
    let d: i64 = iso.get(8..10)?.parse().ok()?;
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    Some(era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468)
}

/// Which binaries a `.gsp` carries.
#[derive(Clone)]
pub enum Arches {
    /// Every architecture the release was built for.
    All,
    /// One, named as the manifest names it (`x86_64`, `aarch64`, ...).
    One(String),
}

/// Fetch a release and verify it, into a directory - or, with `gsp`, into one `.gsp` file holding every document
/// the fetch read, which a Godspeed machine can install from with no network.
pub fn run(
    spec: &str,
    registry_flag: Option<&str>,
    out: Option<&Path>,
    pin: Option<&str>,
    offline: Option<Arches>,
    inspect: Option<&Inspect>,
    json: bool,
) -> Result<()> {
    // A FILE to inspect names its own package, and is read before anything else.
    let file = match inspect {
        Some(Inspect::File(p)) => Some(read_gsp(p, &format!("inspect {}", p.display()))?),
        _ => None,
    };
    // UNPUBLISHED: no registry documents in it, so nothing but its signature can be checked here, and a machine
    // installs it only if its operator trusts that key.
    if let (Some(docs), Some(Inspect::File(p))) = (&file, inspect) {
        if !docs.contains_key("trust/timestamp.toml") {
            return inspect_unpublished(docs, p, json);
        }
    }
    let derived;
    let spec = match (&file, inspect) {
        (Some(docs), Some(Inspect::File(p))) => {
            derived = gsp_spec(docs, p, &format!("inspect {}", p.display()))?;
            derived.as_str()
        }
        _ => spec,
    };
    let (name_text, wanted) = match spec.split_once('@') {
        Some((n, v)) => (n, Some(v)),
        None => (spec, None),
    };
    let doing = match inspect {
        Some(Inspect::File(p)) => format!("inspect {}", p.display()),
        Some(Inspect::Registry) => format!("inspect {spec}"),
        None => format!("fetch {spec}"),
    };
    let name = PackageName::parse(name_text).map_err(|e| {
        Diagnostic::new(
            Kind::InvalidProject,
            Stage::Validation,
            doing.clone(),
            format!("\"{name_text}\" is not a package name: {e}."),
        )
        .expected("publisher/name, optionally @version: alice/hello or alice/hello@1.0.0")
    })?;
    let wanted: Option<Version> = match wanted {
        None => None,
        Some(v) => Some(v.parse().map_err(|e: pkg::VersionError| {
            Diagnostic::new(
                Kind::InvalidProject,
                Stage::Validation,
                doing.clone(),
                format!("\"{v}\" is not a version: {e}."),
            )
        })?),
    };
    let creds = Credentials::load()?;
    let url = registry::resolve(registry_flag, &creds)?;
    let client = Client::new(&url, None);
    let src = Source {
        client: &client,
        kept: offline.is_some().then(Default::default),
        file: file.as_ref(),
    };
    // A FILE IS CHECKED AS OF ITS OWN DAY: the last moment its timestamp was valid - the same digits without the
    // `Z`, which sorts just below it - and nothing this workstation has seen of the registry since is consulted
    // or changed by it.
    let file_expires: Option<String> = match &file {
        None => None,
        Some(docs) => {
            let ts = docs
                .get("trust/timestamp.toml")
                .and_then(|b| std::str::from_utf8(b).ok())
                .ok_or_else(|| untrusted(&doing, TrustError::Malformed("the file holds no timestamp")))?;
            Some(
                trust::Timestamp::parse(ts)
                    .map_err(|e| untrusted(&doing, e))?
                    .header
                    .expires
                    .to_string(),
            )
        }
    };
    let now = match &file_expires {
        Some(e) => e.trim_end_matches('Z').to_string(),
        None => now_utc(),
    };
    let fail = |e: TrustError| untrusted(&doing, e);

    // 1. The root: what this machine already trusts, or root 1 on first use.
    match &inspect {
        Some(Inspect::File(p)) => eprintln!("Checking {} offline, against the root of {url}...", p.display()),
        _ => eprintln!("Checking {url}'s trust metadata..."),
    };
    let loaded = if file.is_some() { None } else { TrustState::load(&url)? };
    let mut state = match loaded {
        Some(s) => s,
        None => {
            let first = required(&src, &doing, "trust/root/1.toml", METADATA_LIMIT)?;
            let sig = required(&src, &doing, "trust/root/1.sig", METADATA_LIMIT)?;
            let digest = Sha256Digest::of(first.as_bytes());
            // The official registry's root is pinned in this binary; any other is trusted on first use.
            let built_in = (url == registry::OFFICIAL).then_some(registry::OFFICIAL_ROOT);
            if let Some(want) = pin.or(built_in) {
                if want != digest.to_string() {
                    return Err(fail(TrustError::DigestMismatch(if pin.is_some() {
                        "the registry's root (by the digest pinned with --trust-root)"
                    } else {
                        "the official registry's root (by the digest built into this CLI)"
                    }))
                    .into());
                }
            }
            let root = Root::parse(&first).map_err(fail)?;
            trust::verify_by(&root, trust::Role::Root, first.as_bytes(), &sig).map_err(fail)?;
            if file.is_some() {
                eprintln!("Its root 1 matches the official registry's, built into this CLI.");
            } else if pin.is_none() && built_in.is_some() {
                eprintln!("First contact with the official registry: its root 1 matches the one built into this CLI.");
            } else if pin.is_none() {
                eprintln!("First contact with {url}: trusting its root 1, {digest}.");
                eprintln!("  Compare it with the digest the registry publishes; pin it with --trust-root next time.");
            }
            TrustState {
                dir: state_dir(&url)?,
                root: first,
                timestamp_version: None,
                namespaces_version: None,
            }
        }
    };
    // Every rotation since: root N+1 must be signed by root N's keys and its own.
    loop {
        let current = Root::parse(&state.root).map_err(fail)?;
        let next_version = current.header.version + 1;
        let Some(next) = text(&src, &doing, &format!("trust/root/{next_version}.toml"), METADATA_LIMIT)? else {
            break;
        };
        let sig = required(&src, &doing, &format!("trust/root/{next_version}.sig"), METADATA_LIMIT)?;
        trust::verify_next_root(&current, &next, &sig).map_err(fail)?;
        state.root = next;
    }
    let root = Root::parse(&state.root).map_err(fail)?;
    root.header.check_fresh(&now).map_err(fail)?;
    if src.kept.is_some() {
        // A machine that has never been online trusts only root 1: the file carries every root from there.
        for v in 1..=root.header.version {
            for ext in ["toml", "sig"] {
                required(&src, &doing, &format!("trust/root/{v}.{ext}"), METADATA_LIMIT)?;
            }
        }
    }

    // 2. Timestamp -> snapshot -> namespaces -> the package's index.
    let ts = required(&src, &doing, "trust/timestamp.toml", METADATA_LIMIT)?;
    let ts_sig = required(&src, &doing, "trust/timestamp.sig", METADATA_LIMIT)?;
    let timestamp = trust::verify_timestamp(&root, &ts, &ts_sig, &now, state.timestamp_version).map_err(fail)?;
    let snap = required(&src, &doing, "trust/snapshot.toml", METADATA_LIMIT)?;
    let snap_sig = required(&src, &doing, "trust/snapshot.sig", METADATA_LIMIT)?;
    let snapshot = trust::verify_snapshot(&root, &timestamp, &snap, &snap_sig, &now).map_err(fail)?;
    let ns = required(&src, &doing, "trust/namespaces.toml", METADATA_LIMIT)?;
    let ns_sig = required(&src, &doing, "trust/namespaces.sig", METADATA_LIMIT)?;
    let namespaces =
        trust::verify_namespaces(&root, &snapshot, &ns, &ns_sig, &now, state.namespaces_version).map_err(fail)?;
    let index_path = format!("index/{}/{}.toml", name.publisher(), name.name());
    let Some(index_text) = text(&src, &doing, &index_path, METADATA_LIMIT)? else {
        return Err(Diagnostic::new(
            Kind::NotFound,
            Stage::Verification,
            doing.clone(),
            format!("{name} has never been published here."),
        )
        .next("Check the name, and the registry (--registry).")
        .into());
    };
    trust::verify_index(&snapshot, &name.to_string(), &index_text).map_err(fail)?;

    // The metadata held together: remember how far this machine has seen, so it never goes back.
    state.timestamp_version = Some(timestamp.header.version);
    state.namespaces_version = Some(namespaces.header.version);
    if file.is_none() {
        state.save()?;
    }

    // 3. The release: chosen from the verified index, its manifest by digest, its signature by a key the
    //    signed namespaces vouch for, for THIS release.
    let owned_index = pkg::parse_index(&index_text).map_err(|e| {
        Diagnostic::new(
            Kind::Untrusted,
            Stage::Verification,
            doing.clone(),
            format!("The verified index does not parse: {e}."),
        )
    })?;
    let releases = owned_index.releases();
    let index = owned_index.index(&releases);
    let release = match wanted {
        Some(v) => index.release(v).copied().ok_or_else(|| {
            Diagnostic::new(
                Kind::NotFound,
                Stage::Verification,
                doing.clone(),
                format!("{name} has no version {v}."),
            )
            .next(format!(
                "Published versions: {}.",
                releases
                    .iter()
                    .map(|r| r.version.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?,
        None => *index.latest().expect("an index lists at least one release"),
    };
    let version = release.version;
    let base = format!("packages/{}/{}/{version}", name.publisher(), name.name());
    let manifest_text = required(&src, &doing, &format!("{base}/manifest.toml"), METADATA_LIMIT)?;
    if Sha256Digest::of(manifest_text.as_bytes()) != release.manifest {
        return Err(fail(TrustError::DigestMismatch("the manifest")).into());
    }
    let sig_text = required(&src, &doing, &format!("{base}/{SIGNATURE_FILE}"), METADATA_LIMIT)?;
    let signature = ManifestSignature::parse(&sig_text)
        .and_then(|s| s.verify(manifest_text.as_bytes()).map(|()| s))
        .map_err(|e| {
            Diagnostic::new(
                Kind::SignatureFailure,
                Stage::Verification,
                doing.clone(),
                format!("{SIGNATURE_FILE}: {e}."),
            )
            .next("Do not install it.")
        })?;
    namespaces
        .trusts_release(name.publisher(), &signature.key, release.published)
        .map_err(fail)?;

    // 4. The files the manifest names, into a staging directory, verified there, then moved into place.
    let owned = pkg::parse(&manifest_text).map_err(|e| {
        Diagnostic::new(
            Kind::InvalidPackage,
            Stage::Verification,
            doing.clone(),
            format!("The signed manifest does not parse: {e}."),
        )
    })?;
    let manifest = owned.manifest();
    if let Some(Arches::One(want)) = &offline {
        if !manifest.artifacts.iter().any(|a| a.arch.as_str() == want) {
            return Err(Diagnostic::new(
                Kind::NotFound,
                Stage::Verification,
                doing.clone(),
                format!("{name}@{version} has no binary for {want}."),
            )
            .next(format!(
                "It was built for: {}. Name one of them with --arch, or all.",
                manifest
                    .artifacts
                    .iter()
                    .map(|a| a.arch.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .into());
        }
    }
    let dest = match (&offline, inspect) {
        // Inspected: checked in a scratch folder that is removed afterwards.
        (_, Some(_)) => {
            let d = std::env::temp_dir().join(format!("godspeed-inspect-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            d
        }
        // For a machine with no network: one file, named from the release, in `--out` or here.
        (Some(_), None) => {
            let leaf = format!("{}-{version}.gsp", name.name());
            out.map(|o| o.join(&leaf)).unwrap_or_else(|| PathBuf::from(leaf))
        }
        (None, None) => out
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(format!("{}-{}-{version}", name.publisher(), name.name()))),
    };
    if dest.exists() {
        return Err(Diagnostic::new(
            Kind::Io,
            Stage::Verification,
            doing.clone(),
            format!("{} already exists.", dest.display()),
        )
        .next("Choose another --out, or remove it.")
        .into());
    }
    let staging = dest.with_file_name(format!(
        ".{}.partial",
        dest.file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| Diagnostic::io(&doing, &staging, &e))?;
    let mut files = vec![(pkg::CONTRACT_FILE.to_string(), METADATA_LIMIT)];
    files.extend(
        manifest
            .artifacts
            .iter()
            .map(|a| (a.arch.artifact_file().to_string(), a.size)),
    );
    for (file, limit) in &files {
        if src.file.is_none() {
            eprintln!("Downloading {file}...");
        }
        let bytes = match src.get(&format!("{base}/{file}"), *limit) {
            Answer::Done(Some(b)) => b,
            // A package file may carry only some architectures' binaries.
            Answer::Done(None) if src.file.is_some() && file.as_str() != pkg::CONTRACT_FILE => continue,
            Answer::Done(None) => {
                return Err(Diagnostic::new(
                    Kind::InvalidPackage,
                    Stage::Verification,
                    doing.clone(),
                    format!("The registry has no {file} for {name}@{version}."),
                )
                .into())
            }
            Answer::Refused(r) => return Err(registry::unreachable(&doing, Stage::Verification, &r.message).into()),
            Answer::Unknown(why) => return Err(registry::unreachable(&doing, Stage::Verification, &why).into()),
        };
        let path = staging.join(file);
        std::fs::write(&path, &bytes).map_err(|e| Diagnostic::io(&doing, &path, &e))?;
        // A binary goes into a `.gsp` only for the architectures asked for; the contract always does.
        let wanted = match &offline {
            Some(Arches::One(want)) => file == pkg::CONTRACT_FILE || *file == format!("{want}.elf"),
            _ => true,
        };
        if wanted {
            src.keep(&format!("{base}/{file}"), &bytes);
        }
    }
    for (file, text) in [(verify::MANIFEST_FILE, &manifest_text), (SIGNATURE_FILE, &sig_text)] {
        let path = staging.join(file);
        std::fs::write(&path, text).map_err(|e| Diagnostic::io(&doing, &path, &e))?;
    }
    let carried: Vec<String> = manifest
        .artifacts
        .iter()
        .filter(|a| staging.join(a.arch.artifact_file()).exists())
        .map(|a| a.arch.to_string())
        .collect();
    if carried.is_empty() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(Diagnostic::new(
            Kind::InvalidPackage,
            Stage::Verification,
            doing.clone(),
            format!("{name}@{version} carries no binary at all."),
        )
        .into());
    }
    let verified = match verify::run_with(&staging, src.file.is_some()) {
        Ok(v) => v,
        Err(mut problems) => {
            let _ = std::fs::remove_dir_all(&staging);
            // Inspected, the folder was scratch: the problem is with what was asked about.
            if inspect.is_some() {
                for p in &mut problems {
                    p.headline = doing.clone();
                }
            }
            return Err(problems);
        }
    };
    if inspect.is_some() {
        let _ = std::fs::remove_dir_all(&staging);
    } else if let Some(kept) = &src.kept {
        // Every document the fetch read and checked, under the path the registry serves it at.
        let kept = kept.borrow();
        let entries: Vec<(&str, &[u8])> = kept.iter().map(|(p, b)| (p.as_str(), b.as_slice())).collect();
        let partial = dest.with_extension("gsp.partial");
        let written = std::fs::File::create(&partial).and_then(|f| {
            let mut w = std::io::BufWriter::new(f);
            pkg::gsp::write(&mut w, &entries)?;
            std::io::Write::flush(&mut w)
        });
        let _ = std::fs::remove_dir_all(&staging);
        written.map_err(|e| Diagnostic::io(&doing, &partial, &e))?;
        std::fs::rename(&partial, &dest).map_err(|e| Diagnostic::io(&doing, &dest, &e))?;
    } else {
        std::fs::rename(&staging, &dest).map_err(|e| Diagnostic::io(&doing, &dest, &e))?;
    }

    let retired = match namespaces.status(name.publisher(), &signature.key) {
        Some(trust::KeyStatus::Retired(at)) => Some(at.to_string()),
        _ => None,
    };
    if json {
        let doc = serde_json::json!({
            "name": name.to_string(),
            "version": version.to_string(),
            "dir": dest.display().to_string(),
            "gsp": offline.is_some(),
            "inspected": inspect.is_some(),
            "carries": carried,
            "checked_as_of": file_expires,
            "registry": url,
            "verified": {
                "root": root.header.version,
                "timestamp": timestamp.header.version,
                "snapshot": snapshot.header.version,
                "namespaces": namespaces.header.version,
            },
            "signed_by": signature.key.to_string(),
            "key_retired_since": retired,
            "requests": verified.requests,
        });
        println!("{doc:#}");
        return Ok(());
    }
    let status = match retired {
        Some(at) => format!(
            "a key of {}/ retired since {at}, published before then",
            name.publisher()
        ),
        None => format!("an active key of {}/", name.publisher()),
    };
    if let Some(how) = inspect {
        match how {
            Inspect::File(p) => {
                println!("Inspected {name}@{version} in {}: nothing was kept", p.display());
                let expires = file_expires.as_deref().unwrap_or("");
                let today = epoch_days(&now_utc());
                let age = match (today, epoch_days(expires)) {
                    (Some(t), Some(e)) => format!(", {} days ago", (t - e).max(0)),
                    _ => String::new(),
                };
                println!("  checked offline, as of {}{age}", expires.get(..10).unwrap_or(expires));
            }
            Inspect::Registry => println!("Inspected {name}@{version} on {url}: nothing was kept"),
        }
        println!("  carries: {}", carried.join(", "));
    } else if offline.is_some() {
        println!("Wrote {name}@{version} to {}", dest.display());
        println!(
            "  copy it to a Godspeed machine and run: godspeed install {}",
            dest.file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default()
        );
        println!("  checked against the registry as of {now}; a machine trusts it for as long as its settings allow");
    } else {
        println!("Fetched {name}@{version} into {}", dest.display());
    }
    println!(
        "  verified: root {} -> timestamp {} -> snapshot {} -> namespaces {} -> index -> manifest -> files",
        root.header.version, timestamp.header.version, snapshot.header.version, namespaces.header.version
    );
    println!("  signed by {} ({status})", signature.key);
    println!(
        "  requests: {}",
        if verified.requests.is_empty() {
            "nothing".to_string()
        } else {
            verified.requests.join("; ")
        }
    );
    println!("A verified package is one its publisher's key signed and the registry's metadata vouches for. It is");
    println!("not a judgement of safety, and installing it would grant nothing its contract requests.");
    Ok(())
}
