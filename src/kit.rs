//! Platform kits: the few facts the CLI needs from a GodspeedOS release, fetched once and cached, so that
//! installing the CLI is enough - no clone of the OS repository (`docs/no-checkout.md`).
//!
//! A kit is a sparse copy of the OS repository at one release: exactly the files `platform.rs` reads from a
//! checkout, at the same paths, so a kit and a checkout are read by the same code. It is fetched from the
//! release's tag, resolved first to its commit so every file comes from one snapshot, and cached per
//! release with each file's SHA-256 in `kit.toml`. A cached kit that no longer matches its digests is
//! discarded and fetched again.
//!
//! **Option B first, then option A** (`docs/no-checkout.md`, `backlog/01`). A release that publishes
//! `godspeed-platform.toml` (the OS's `docs/developer-services-design.md` section 11) is fetched as that one
//! file, checked against the release's `SHA256SUMS-<tag>` and refused in a format this CLI does not read; the
//! kit is the file and the linker script it carries, written out beside it. A release without one (every
//! release up to 0.28.0) is fetched as the six build files below, at their paths in the OS repository,
//! which is the bridge. The checksum comes from the same release as the file, so it catches a damaged or
//! truncated download, not a tampered release: the trust is what downloading from GitHub gives, no more.

use crate::diag::{Diagnostic, Kind, Stage};
use crate::platform::{PlatformFile, LINKER_SCRIPT_IN_KIT, PLATFORM_FILE};
use godspeed_package::{Sha256Digest, Version};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The files of a kit, as paths in the OS repository, and whether a release must have each.
const FILES: &[(&str, bool)] = &[
    ("Cargo.toml", true),
    ("stdlib/rust/Cargo.toml", true),
    ("rust-toolchain.toml", true),
    (".cargo/config.toml", false),
    ("services/user.ld", true),
    ("contracts/schema/service.schema.json", true),
];
/// What a kit records about itself: which release, from which commit, and each file's digest.
const KIT_FILE: &str = "kit.toml";
/// No kit file is anywhere near this; a larger answer is not one.
const FILE_LIMIT: u64 = 1 << 20;
/// Where the cache lives, overriding the platform default (used by tests).
pub const CACHE_ENV: &str = "GODSPEED_CACHE_DIR";

const REPO: &str = "godspeedhq/godspeed-os";

/// `$XDG_CACHE_HOME/godspeed` or `~/.cache/godspeed`; `~/Library/Caches/godspeed` on macOS;
/// `%LOCALAPPDATA%\godspeed\cache` on Windows.
pub fn cache_dir() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(d) = var(CACHE_ENV) {
        return Some(d);
    }
    if cfg!(windows) {
        return var("LOCALAPPDATA").map(|d| d.join("godspeed").join("cache"));
    }
    if cfg!(target_os = "macos") {
        return var("HOME").map(|h| h.join("Library/Caches/godspeed"));
    }
    var("XDG_CACHE_HOME")
        .map(|x| x.join("godspeed"))
        .or_else(|| var("HOME").map(|h| h.join(".cache/godspeed")))
}

fn kits_dir() -> Result<PathBuf, Diagnostic> {
    cache_dir().map(|d| d.join("platform")).ok_or_else(|| {
        Diagnostic::new(
            Kind::Environment,
            Stage::Setup,
            "find a cache directory",
            "Neither HOME nor a cache directory is set, so there is nowhere to keep the GodspeedOS platform kit.",
        )
        .next(format!("Set {CACHE_ENV}, or name an OS checkout with `--os <path>`."))
    })
}

/// A kit on disk, checked against its own record.
pub struct Kit {
    pub dir: PathBuf,
    pub version: Version,
    /// The commit (or, if it could not be resolved, the tag) its files were fetched from.
    pub reference: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Record {
    release: String,
    reference: String,
    files: BTreeMap<String, String>,
}

/// The cached kit for `version`, if there is one and every file still has its recorded digest. A kit that
/// does not is removed, so the next fetch starts clean.
fn cached(dir: &Path) -> Option<Kit> {
    let record: Record = toml::from_str(&std::fs::read_to_string(dir.join(KIT_FILE)).ok()?).ok()?;
    let intact =
        record.files.iter().all(|(path, digest)| {
            std::fs::read(dir.join(path)).is_ok_and(|b| Sha256Digest::of(&b).to_string() == *digest)
        }) && (record.files.contains_key(PLATFORM_FILE)
            || FILES
                .iter()
                .all(|(path, required)| !required || record.files.contains_key(*path)));
    if !intact {
        let _ = std::fs::remove_dir_all(dir);
        return None;
    }
    Some(Kit {
        dir: dir.to_path_buf(),
        version: record.release.parse().ok()?,
        reference: record.reference,
    })
}

/// The kit for `version`: from the cache, or fetched now.
pub fn ensure(version: &Version) -> Result<Kit, Diagnostic> {
    let kits = kits_dir()?;
    let dir = kits.join(version.to_string());
    if let Some(kit) = cached(&dir) {
        return Ok(kit);
    }
    fetch(version, &kits, &dir)
}

/// Every intact cached kit, newest release first.
pub fn all_cached() -> Vec<Kit> {
    let Ok(kits) = kits_dir() else { return Vec::new() };
    let mut found: Vec<Kit> = std::fs::read_dir(&kits)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(|n| n.parse::<Version>().is_ok()))
        .filter_map(|e| cached(&e.path()))
        .collect();
    found.sort_by_key(|k| std::cmp::Reverse(k.version));
    found
}

fn fetch(version: &Version, kits: &Path, dir: &Path) -> Result<Kit, Diagnostic> {
    let doing = format!("get GodspeedOS {version}");
    let tag = format!("v{version}");
    eprintln!("Fetching the GodspeedOS {version} platform kit (once for each release)...");
    if let Some(kit) = fetch_platform_file(version, &tag, kits, dir, &doing)? {
        return Ok(kit);
    }
    // One snapshot: if the tag resolves to a commit, every file comes from that commit, even if the tag
    // moves meanwhile. If it cannot be resolved (GitHub's API limits anonymous callers), the tag is used.
    let reference = commit_of(&tag).unwrap_or_else(|| tag.clone());

    let staging = kits.join(format!(".{version}.{}.partial", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    let result = (|| {
        let mut files = BTreeMap::new();
        for (path, required) in FILES {
            let url = format!("{}/{reference}/{path}", raw_base());
            let Some(bytes) = get(&url, &doing)? else {
                if *required {
                    return Err(Diagnostic::new(
                        Kind::NotFound,
                        Stage::Setup,
                        doing.clone(),
                        format!("GodspeedOS {tag} has no {path}."),
                    )
                    .expected(format!("a GodspeedOS release ({REPO}) that this CLI can build against"))
                    .next("Check the version, or name an OS checkout with `--os <path>`."));
                }
                continue;
            };
            let target = staging.join(path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| Diagnostic::io(&doing, parent, &e))?;
            }
            std::fs::write(&target, &bytes).map_err(|e| Diagnostic::io(&doing, &target, &e))?;
            files.insert(path.to_string(), Sha256Digest::of(&bytes).to_string());
        }
        let record = Record {
            release: version.to_string(),
            reference: reference.clone(),
            files,
        };
        let text = toml::to_string(&record).expect("a record serialises");
        let path = staging.join(KIT_FILE);
        std::fs::write(&path, text).map_err(|e| Diagnostic::io(&doing, &path, &e))?;
        Ok(())
    })();
    if let Err(d) = result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(d);
    }
    // Into place in one step. If another run got there first, its kit is as good as this one.
    if std::fs::rename(&staging, dir).is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    cached(dir).ok_or_else(|| {
        Diagnostic::new(
            Kind::Io,
            Stage::Setup,
            doing,
            format!("The kit was fetched but could not be kept in {}.", dir.display()),
        )
    })
}

/// The kit for a release that publishes `godspeed-platform.toml`, or `None` for one that does not.
fn fetch_platform_file(
    version: &Version,
    tag: &str,
    kits: &Path,
    dir: &Path,
    doing: &str,
) -> Result<Option<Kit>, Diagnostic> {
    let base = format!("{}/download/{tag}", releases_base());
    let Some(bytes) = get(&format!("{base}/{PLATFORM_FILE}"), doing)? else {
        return Ok(None);
    };
    let integrity = |why: String| {
        Diagnostic::new(Kind::IntegrityFailure, Stage::Setup, doing.to_string(), why)
            .next("Run the command again; nothing was kept. If it persists, name an OS checkout with `--os <path>`.")
    };
    let sums_name = format!("SHA256SUMS-{tag}");
    let Some(sums) = get(&format!("{base}/{sums_name}"), doing)? else {
        return Err(integrity(format!(
            "GodspeedOS {tag} publishes {PLATFORM_FILE} but no {sums_name} to check it against."
        )));
    };
    let listed = String::from_utf8_lossy(&sums).lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let digest = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        (name == PLATFORM_FILE).then(|| format!("sha256:{}", digest.to_ascii_lowercase()))
    });
    let got = Sha256Digest::of(&bytes).to_string();
    match listed {
        None => return Err(integrity(format!("{sums_name} does not list {PLATFORM_FILE}."))),
        Some(want) if want != got => {
            return Err(integrity(format!(
                "{PLATFORM_FILE} from GodspeedOS {tag} is not the file {sums_name} lists: it is {got}, and the list says {want}."
            )))
        }
        Some(_) => {}
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| integrity(format!("{PLATFORM_FILE} from GodspeedOS {tag} is not text.")))?;
    let file = PlatformFile::parse(text).map_err(|e| e.diagnostic(Path::new(&format!("GodspeedOS {tag}"))))?;
    if file.os != version.to_string() {
        return Err(integrity(format!(
            "{PLATFORM_FILE} from GodspeedOS {tag} says it describes GodspeedOS {}.",
            file.os
        )));
    }

    let staging = kits.join(format!(".{version}.{}.partial", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    let result = (|| {
        let mut files = BTreeMap::new();
        for (path, content) in [
            (PLATFORM_FILE, &bytes[..]),
            (LINKER_SCRIPT_IN_KIT, file.linker_script.as_bytes()),
        ] {
            let target = staging.join(path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| Diagnostic::io(doing, parent, &e))?;
            }
            std::fs::write(&target, content).map_err(|e| Diagnostic::io(doing, &target, &e))?;
            files.insert(path.to_string(), Sha256Digest::of(content).to_string());
        }
        let record = Record {
            release: version.to_string(),
            reference: file.commit.clone().unwrap_or_else(|| tag.to_string()),
            files,
        };
        let text = toml::to_string(&record).expect("a record serialises");
        let path = staging.join(KIT_FILE);
        std::fs::write(&path, text).map_err(|e| Diagnostic::io(doing, &path, &e))?;
        Ok(())
    })();
    if let Err(d) = result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(d);
    }
    if std::fs::rename(&staging, dir).is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    cached(dir).map(Some).ok_or_else(|| {
        Diagnostic::new(
            Kind::Io,
            Stage::Setup,
            doing.to_string(),
            format!("The kit was fetched but could not be kept in {}.", dir.display()),
        )
    })
}

/// The latest published GodspeedOS release, from the address `releases/latest` redirects to.
pub fn latest() -> Result<Version, String> {
    let response = agent(Duration::from_secs(10))
        .redirects(0)
        .build()
        .head(&format!("{}/latest", releases_base()))
        .call()
        .map_err(|e| format!("GitHub could not be asked for the latest GodspeedOS release: {e}"))?;
    let location = response.header("location").unwrap_or_default();
    let tag = location
        .split_once("/releases/tag/v")
        .map(|(_, t)| t)
        .ok_or_else(|| "GitHub lists no GodspeedOS release".to_string())?;
    let end = tag.find(['/', '?', '#']).unwrap_or(tag.len());
    tag[..end].parse().map_err(|_| {
        format!(
            "the latest GodspeedOS release, {}, has no version this CLI can read",
            &tag[..end]
        )
    })
}

/// The commit a tag names, or `None` if GitHub could not say.
fn commit_of(tag: &str) -> Option<String> {
    let sha = agent(Duration::from_secs(10))
        .build()
        .get(&format!("{}/{tag}", commits_base()))
        .set("Accept", "application/vnd.github.sha")
        .call()
        .ok()?
        .into_string()
        .ok()?;
    let sha = sha.trim();
    (sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())).then(|| sha.to_ascii_lowercase())
}

/// A file, or `None` if there is no such file.
fn get(url: &str, doing: &str) -> Result<Option<Vec<u8>>, Diagnostic> {
    let failed = |why: String| {
        Diagnostic::new(Kind::MissingDependency, Stage::Setup, doing.to_string(), why).next(
            "Check the network and run the command again; nothing was kept. Or name an OS checkout with \
             `--os <path>`, which needs no network.",
        )
    };
    match agent(Duration::from_secs(30)).build().get(url).call() {
        Ok(response) => {
            let mut bytes = Vec::new();
            response
                .into_reader()
                .take(FILE_LIMIT + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| failed(format!("{url} could not be read: {e}.")))?;
            if bytes.len() as u64 > FILE_LIMIT {
                return Err(failed(format!("{url} is larger than any platform file should be.")));
            }
            Ok(Some(bytes))
        }
        Err(ureq::Error::Status(404, _)) => Ok(None),
        Err(ureq::Error::Status(code, _)) => Err(failed(format!("{url} answered HTTP {code}."))),
        Err(ureq::Error::Transport(t)) => Err(failed(format!("{url} could not be downloaded: {t}."))),
    }
}

fn agent(timeout: Duration) -> ureq::AgentBuilder {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(timeout)
        .user_agent(concat!("godspeed/", env!("CARGO_PKG_VERSION")))
}

// Where each thing is asked for. Debug builds can serve all three from one local address, for the tests.
#[cfg(debug_assertions)]
const TEST_URL_ENV: &str = "GODSPEED_TEST_OS_URL";

fn test_base() -> Option<String> {
    #[cfg(debug_assertions)]
    if let Some(u) = std::env::var_os(TEST_URL_ENV).filter(|u| !u.is_empty()) {
        return Some(u.to_string_lossy().trim_end_matches('/').to_string());
    }
    None
}

fn releases_base() -> String {
    test_base()
        .map(|b| format!("{b}/releases"))
        .unwrap_or_else(|| format!("https://github.com/{REPO}/releases"))
}

fn commits_base() -> String {
    test_base()
        .map(|b| format!("{b}/commits"))
        .unwrap_or_else(|| format!("https://api.github.com/repos/{REPO}/commits"))
}

fn raw_base() -> String {
    test_base()
        .map(|b| format!("{b}/raw"))
        .unwrap_or_else(|| format!("https://raw.githubusercontent.com/{REPO}"))
}
