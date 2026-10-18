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
}

impl Source<'_> {
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
    match src.client.get(path, limit) {
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
    gsp: Option<(&Path, Arches)>,
    json: bool,
) -> Result<()> {
    let (name_text, wanted) = match spec.split_once('@') {
        Some((n, v)) => (n, Some(v)),
        None => (spec, None),
    };
    let doing = format!("fetch {spec}");
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
        kept: gsp.is_some().then(Default::default),
    };
    let now = now_utc();
    let fail = |e: TrustError| untrusted(&doing, e);

    // 1. The root: what this machine already trusts, or root 1 on first use.
    eprintln!("Checking {url}'s trust metadata...");
    let mut state = match TrustState::load(&url)? {
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
            if pin.is_none() && built_in.is_some() {
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
    state.save()?;

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
    if let Some((_, Arches::One(want))) = &gsp {
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
    let dest = match &gsp {
        Some((file, _)) => file.to_path_buf(),
        None => out
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
        eprintln!("Downloading {file}...");
        let bytes = match client.get(&format!("{base}/{file}"), *limit) {
            Answer::Done(Some(b)) => b,
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
        let wanted = match &gsp {
            Some((_, Arches::One(want))) => file == pkg::CONTRACT_FILE || *file == format!("{want}.elf"),
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
    let verified = match verify::run(&staging) {
        Ok(v) => v,
        Err(problems) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(problems);
        }
    };
    if let Some(kept) = &src.kept {
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
            "gsp": gsp.is_some(),
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
    if gsp.is_some() {
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
