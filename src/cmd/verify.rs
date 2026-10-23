//! `godspeed verify <dir>`: check a package directory against its own manifest.
//!
//! Needs no OS checkout, no network and no project: a package is checkable by anyone holding its files.
//! It establishes INTEGRITY - these files are exactly the ones the manifest names - and, when the package
//! carries `manifest.sig`, which KEY signed the manifest. Whether that key belongs to the publisher is the
//! registry's to establish, not a file's, and the output says so.

use crate::diag::{Diagnostic, Kind, Result, Stage};
use crate::target;
use godspeed_package::{
    self as pkg, ArtifactVerifier, ManifestSignature, OwnedManifest, ParseError, PublicKey, Sha256Digest,
    CONTRACT_FILE, SIGNATURE_FILE,
};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

pub const MANIFEST_FILE: &str = "manifest.toml";

/// A package that passed: its manifest, and what its contract requests (for showing, not granting).
pub struct Verified {
    pub manifest: OwnedManifest,
    pub requests: Vec<String>,
    /// The key that signed the manifest, if the package is signed.
    pub signed_by: Option<PublicKey>,
}

fn invalid(dir: &Path, reason: String) -> Diagnostic {
    Diagnostic::new(
        Kind::InvalidPackage,
        Stage::Verification,
        format!("verify the package in {}", dir.display()),
        reason,
    )
}

fn integrity(dir: &Path, reason: String) -> Diagnostic {
    Diagnostic::new(
        Kind::IntegrityFailure,
        Stage::Verification,
        format!("verify the package in {}", dir.display()),
        reason,
    )
    .next("Do not install or publish this package. Rebuild it with `godspeed package`, or obtain it again.")
}

/// Stream a file through a verifier, 64 KiB at a time.
fn stream(path: &Path, expected: pkg::Artifact) -> std::io::Result<std::result::Result<(), pkg::VerifyError>> {
    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut v = ArtifactVerifier::new(expected);
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(v.finish());
        }
        if let Err(e) = v.update(&buf[..n]) {
            return Ok(Err(e));
        }
    }
}

/// What a release requires installed, e.g. `alice/store ^1.2.0`: shown, never granted (GodspeedOS
/// `docs/developer-services-design.md` 16).
pub fn requires(m: &pkg::Manifest<'_>) -> Vec<String> {
    m.requires.iter().map(|r| format!("{} {}", r.name(), r.range)).collect()
}

/// Summarise what a contract requests, e.g. `log_write`, `ipc_send = fs`.
fn requests(contract: &toml::Table) -> Vec<String> {
    let Some(caps) = contract.get("capabilities").and_then(|c| c.as_table()) else {
        return Vec::new();
    };
    caps.iter()
        .filter_map(|(k, v)| match v {
            toml::Value::Boolean(true) => Some(k.clone()),
            toml::Value::Array(a) if !a.is_empty() => {
                let items: Vec<String> = a
                    .iter()
                    .map(|i| i.as_str().map(String::from).unwrap_or(i.to_string()))
                    .collect();
                Some(format!("{k} = {}", items.join(", ")))
            }
            toml::Value::String(s) => Some(format!("{k} = {s}")),
            toml::Value::Integer(i) => Some(format!("{k} = {i}")),
            _ => None,
        })
        .collect()
}

pub fn run(dir: &Path) -> Result<Verified> {
    run_with(dir, false)
}

/// [`run`], where `some_binaries` lets a binary the manifest names be absent - a package file may carry only the
/// architectures it was made for. Every binary that IS present is checked exactly as `run` checks it.
pub fn run_with(dir: &Path, some_binaries: bool) -> Result<Verified> {
    let mut problems = Vec::new();

    // 1. The manifest: it decides what every other file must be.
    let mpath = dir.join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&mpath).map_err(|e| {
        invalid(dir, format!("{MANIFEST_FILE} cannot be read: {e}"))
            .expected("a package directory: manifest.toml, contract.toml and one <arch>.elf per architecture")
    })?;
    let owned = match pkg::parse(&text) {
        Ok(m) => m,
        Err(ParseError::Syntax(e)) => return Err(invalid(dir, format!("{MANIFEST_FILE}: {e}")).into()),
        Err(ParseError::Invalid(ps)) => {
            return Err(ps
                .iter()
                .map(|p| invalid(dir, format!("{MANIFEST_FILE}: {p}")))
                .collect());
        }
    };
    let manifest = owned.manifest();

    // 2. The signature, if there is one: it must cover these exact manifest bytes.
    let spath = dir.join(SIGNATURE_FILE);
    let mut signed_by = None;
    if spath.exists() {
        let bad = |why: String| {
            Diagnostic::new(
                Kind::SignatureFailure,
                Stage::Verification,
                format!("verify the package in {}", dir.display()),
                why,
            )
            .next("Do not install or publish this package. Sign it again (`godspeed sign`), or obtain it again.")
        };
        match std::fs::read_to_string(&spath) {
            Err(e) => problems.push(bad(format!("{SIGNATURE_FILE} cannot be read: {e}"))),
            Ok(t) => match ManifestSignature::parse(&t).and_then(|s| s.verify(text.as_bytes()).map(|()| s.key)) {
                Ok(key) => signed_by = Some(key),
                Err(e) => problems.push(bad(format!("{SIGNATURE_FILE}: {e}."))),
            },
        }
    }

    // 3. Exactly the files the manifest names: nothing missing, nothing extra.
    let mut expected: BTreeSet<String> = [MANIFEST_FILE, CONTRACT_FILE].iter().map(|s| s.to_string()).collect();
    if spath.exists() {
        expected.insert(SIGNATURE_FILE.to_string());
    }
    for a in manifest.artifacts {
        expected.insert(a.arch.artifact_file().to_string());
    }
    match std::fs::read_dir(dir) {
        Ok(entries) => {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if !expected.contains(&name) {
                    problems.push(invalid(
                        dir,
                        format!("{name} is not part of the package: the manifest names no such file."),
                    ));
                }
            }
        }
        Err(e) => return Err(Diagnostic::io("verify the package", dir, &e).into()),
    }

    // 4. The contract: its bytes, then its name and version against the manifest's.
    let cpath = dir.join(CONTRACT_FILE);
    let mut contract_requests = Vec::new();
    match std::fs::read(&cpath) {
        Err(e) => problems.push(invalid(dir, format!("{CONTRACT_FILE} cannot be read: {e}"))),
        Ok(bytes) => {
            let actual = Sha256Digest::of(&bytes);
            if actual != manifest.contract {
                problems.push(integrity(
                    dir,
                    format!(
                        "{CONTRACT_FILE} is not the contract the manifest names: expected {}, got {actual}.",
                        manifest.contract
                    ),
                ));
            } else {
                match std::str::from_utf8(&bytes)
                    .ok()
                    .and_then(|t| t.parse::<toml::Table>().ok())
                {
                    None => problems.push(invalid(dir, format!("{CONTRACT_FILE} is not valid TOML."))),
                    Some(c) => {
                        let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let version = c.get("version").and_then(|v| v.as_str()).unwrap_or("");
                        manifest.validate_against_contract(name, version, |p| {
                            problems.push(invalid(dir, format!("{MANIFEST_FILE}: {p}")))
                        });
                        contract_requests = requests(&c);
                    }
                }
            }
        }
    }

    // 5. Each artifact: streamed against its declared size and digest, then checked to be an ELF for the
    //    architecture it is filed under.
    for a in manifest.artifacts {
        let file = a.arch.artifact_file();
        let path = dir.join(file);
        if some_binaries && !path.exists() {
            continue;
        }
        match stream(&path, *a) {
            Err(e) => problems.push(invalid(dir, format!("{file} cannot be read: {e}"))),
            Ok(Err(e)) => problems.push(integrity(dir, format!("{file}: {e}."))),
            Ok(Ok(())) => {
                let mut head = [0u8; 20];
                let read = std::fs::File::open(&path).and_then(|mut f| f.read_exact(&mut head));
                let want = target::for_package(a.arch).elf_machine;
                if read.is_err() || &head[..4] != b"\x7fELF" {
                    problems.push(invalid(dir, format!("{file} is not an ELF binary.")));
                } else if head[5] != 1 || u16::from_le_bytes([head[18], head[19]]) != want {
                    problems.push(invalid(
                        dir,
                        format!(
                            "{file} is an ELF for machine {:#x}, not {} ({want:#x}).",
                            u16::from_le_bytes([head[18], head[19]]),
                            a.arch
                        ),
                    ));
                }
            }
        }
    }

    if problems.is_empty() {
        Ok(Verified {
            manifest: owned,
            requests: contract_requests,
            signed_by,
        })
    } else {
        // Tampering is the decision-relevant finding, so it leads and sets the exit code: altered bytes
        // first, then a signature that does not hold, then anything malformed.
        problems.sort_by_key(|d| match d.kind {
            Kind::IntegrityFailure => 0,
            Kind::SignatureFailure => 1,
            _ => 2,
        });
        Err(problems)
    }
}

/// Print what a verified package is.
/// `verify --json`: what `report` says, as data.
pub fn to_json(v: &Verified) -> serde_json::Value {
    let m = v.manifest.manifest();
    let artifacts: Vec<_> = m
        .artifacts
        .iter()
        .map(|a| {
            serde_json::json!({
                "arch": a.arch.as_str(),
                "file": a.arch.artifact_file(),
                "size": a.size,
                "digest": a.digest.to_string(),
            })
        })
        .collect();
    serde_json::json!({
        "name": m.name.to_string(),
        "version": m.version.to_string(),
        "kind": m.kind.as_str(),
        "description": m.description,
        "license": m.license,
        "os_release": m.os_release.to_string(),
        "contract": { "file": CONTRACT_FILE, "digest": m.contract.to_string(), "requests": v.requests },
        "artifacts": artifacts,
        "signed_by": v.signed_by.as_ref().map(|k| k.to_string()),
    })
}

pub fn report(v: &Verified) {
    let m = v.manifest.manifest();
    println!(
        "Verified {} {} ({}), built against GodspeedOS {}",
        m.name,
        m.version,
        m.kind.as_str(),
        m.os_release
    );
    if let Some(d) = m.description {
        println!("  {d}");
    }
    let requests = if v.requests.is_empty() {
        "nothing".to_string()
    } else {
        v.requests.join("; ")
    };
    println!("  {:<14} {:>10}  {}", CONTRACT_FILE, "", m.contract);
    println!("  {:<14} requests: {requests}", "");
    for a in m.artifacts {
        println!(
            "  {:<14} {:>10}  {}",
            a.arch.artifact_file(),
            format!("{} B", a.size),
            a.digest
        );
    }
    match &v.signed_by {
        Some(key) => {
            println!("  {:<14} {:>10}  signed by {key}", SIGNATURE_FILE, "");
            println!("These files are exactly the ones the manifest names, and that key signed it. Whether the key is");
            println!("{}'s is the registry's to establish, not a file's.", m.name.publisher());
        }
        None => {
            println!("  (not signed)");
            println!("These files are exactly the ones the manifest names. Nothing says who made them: the package is");
            println!("not signed.");
        }
    }
    println!("What the contract requests is not granted by installing it.");
}
