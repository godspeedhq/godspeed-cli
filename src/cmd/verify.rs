//! `godspeed verify <dir>`: check a package directory against its own manifest.
//!
//! Needs no OS checkout, no network and no project: a package is checkable by anyone holding its files.
//! It establishes INTEGRITY - these files are exactly the ones the manifest names - and nothing more.
//! Who published the package is not checked, because nothing signs packages yet, and the output says so.

use crate::diag::{Diagnostic, Kind, Result, Stage};
use crate::target;
use godspeed_package::{self as pkg, ArtifactVerifier, OwnedManifest, ParseError, Sha256Digest, CONTRACT_FILE};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

pub const MANIFEST_FILE: &str = "manifest.toml";

/// A package that passed: its manifest, and what its contract requests (for showing, not granting).
pub struct Verified {
    pub manifest: OwnedManifest,
    pub requests: Vec<String>,
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

    // 2. Exactly the files the manifest names: nothing missing, nothing extra.
    let mut expected: BTreeSet<String> = [MANIFEST_FILE, CONTRACT_FILE].iter().map(|s| s.to_string()).collect();
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

    // 3. The contract: its bytes, then its name and version against the manifest's.
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

    // 4. Each artifact: streamed against its declared size and digest, then checked to be an ELF for the
    //    architecture it is filed under.
    for a in manifest.artifacts {
        let file = a.arch.artifact_file();
        let path = dir.join(file);
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
        })
    } else {
        // An integrity failure is the decision-relevant one, so it leads and sets the exit code.
        problems.sort_by_key(|d| d.kind != Kind::IntegrityFailure);
        Err(problems)
    }
}

/// Print what a verified package is.
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
    println!("Integrity only: these files are exactly the ones the manifest names. Who published them is not");
    println!("verified (packages are not signed yet), and what the contract requests is not granted by installing it.");
}
