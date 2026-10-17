//! `godspeed package`: build every selected architecture, assemble a package directory, and verify it
//! from disk before saying it exists.
//!
//! The package is written beside its final place and moved into it only after it has verified, so an
//! interrupted or failed run never leaves something that looks like a finished package.

use super::{build, verify};
use crate::diag::{Diagnostic, Kind, Result, Stage};
use crate::platform::Platform;
use crate::project::{Project, ProjectKind};
use crate::target::Arch;
use godspeed_package::{self as pkg, Artifact, Manifest, PackageName, Sha256Digest, Version, CONTRACT_FILE};
use std::path::{Path, PathBuf};

fn refuse(kind: Kind, headline: &str, reason: String) -> Diagnostic {
    Diagnostic::new(kind, Stage::Packaging, headline, reason)
}

/// Is `dir` absent, or a package directory this command may replace? Never deletes anything else.
fn replaceable(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return !dir.exists();
    };
    let allowed: Vec<&str> = [verify::MANIFEST_FILE, CONTRACT_FILE, pkg::SIGNATURE_FILE]
        .into_iter()
        .chain(pkg::Arch::ALL.iter().map(|a| a.artifact_file()))
        .collect();
    entries
        .flatten()
        .all(|e| allowed.contains(&e.file_name().to_string_lossy().as_ref()))
}

pub fn run(platform: &Platform, project: &Project, arches: &[&'static Arch], out: Option<&Path>) -> Result<PathBuf> {
    let headline = "package the project";
    if !project.kind.runs() {
        return Err(refuse(
            Kind::NotImplemented,
            headline,
            "Only services, utilities, apps and drivers are packaged. A library is depended on, not installed, and how libraries are \
             distributed is not defined yet."
                .into(),
        )
        .into());
    }
    let Some(publisher) = project.publisher.as_deref() else {
        return Err(refuse(
            Kind::InvalidProject,
            headline,
            "godspeed.toml names no publisher, so the package has no name: packages are named publisher/name.".into(),
        )
        .expected("publisher = \"<your-name>\" in godspeed.toml, e.g. publisher = \"alice\" for alice/hello")
        .next("Add the line and run `godspeed package` again. A registry checks the namespace when you publish.")
        .into());
    };

    // Build (which validates first). Nothing is packaged from a build that did not complete.
    let built = build::run(platform, project, arches)?;
    eprintln!("Creating package...");

    let contract_rel = project.contract.as_ref().expect("a service has a contract");
    let contract_path = project.root.join(contract_rel);
    let contract_bytes = std::fs::read(&contract_path).map_err(|e| Diagnostic::io(headline, &contract_path, &e))?;
    let contract: toml::Table = std::str::from_utf8(&contract_bytes)
        .ok()
        .and_then(|t| t.parse().ok())
        .expect("`check` parsed this contract moments ago");
    let service = contract
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let version_text = contract.get("version").and_then(|v| v.as_str()).unwrap_or_default();
    let version: Version = version_text.parse().map_err(|e: pkg::VersionError| {
        refuse(
            Kind::InvalidProject,
            headline,
            format!("The contract's version \"{version_text}\" cannot be a package version: {e}."),
        )
    })?;
    let os_release: Version = platform.version.parse().map_err(|e: pkg::VersionError| {
        refuse(
            Kind::Environment,
            headline,
            format!(
                "The OS checkout's version \"{}\" is not MAJOR.MINOR.PATCH: {e}.",
                platform.version
            ),
        )
    })?;
    let full_name = format!("{publisher}/{service}");
    let name = PackageName::parse(&full_name).map_err(|e| {
        refuse(
            Kind::InvalidProject,
            headline,
            format!("\"{full_name}\" is not a package name: {e}."),
        )
    })?;

    let mut artifacts = Vec::new();
    let mut files = Vec::new();
    for b in &built {
        let bytes = std::fs::read(&b.artifact).map_err(|e| Diagnostic::io(headline, &b.artifact, &e))?;
        artifacts.push(Artifact {
            arch: b.arch.package,
            size: bytes.len() as u64,
            digest: Sha256Digest::of(&bytes),
        });
        files.push((b.arch.package.artifact_file(), bytes));
    }
    artifacts.sort_by_key(|a| a.arch);

    let manifest = Manifest {
        format: pkg::FORMAT,
        name,
        version,
        kind: match project.kind {
            ProjectKind::Utility => pkg::Kind::Utility,
            ProjectKind::App => pkg::Kind::App,
            ProjectKind::Driver => pkg::Kind::Driver,
            _ => pkg::Kind::Service,
        },
        description: project.description.as_deref(),
        license: project.license.as_deref(),
        os_release,
        contract: Sha256Digest::of(&contract_bytes),
        artifacts: &artifacts,
    };
    let mut problems = Vec::new();
    manifest.validate(|p| problems.push(refuse(Kind::InvalidProject, headline, format!("{p}"))));
    if !problems.is_empty() {
        return Err(problems);
    }

    let dest = out.map(Path::to_path_buf).unwrap_or_else(|| {
        project
            .root
            .join("target")
            .join("package")
            .join(format!("{publisher}-{service}-{version}"))
    });
    if !replaceable(&dest) {
        return Err(refuse(
            Kind::Io,
            headline,
            format!(
                "{} exists and holds files that are not a package's. Nothing was written.",
                dest.display()
            ),
        )
        .next("Choose another --out, or empty that directory.")
        .into());
    }

    // Assemble beside the destination, verify there, then move into place.
    let staging = dest.with_file_name(format!(
        ".{}.partial",
        dest.file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| Diagnostic::io(headline, &staging, &e))?;
    let write = |file: &str, bytes: &[u8]| {
        let p = staging.join(file);
        std::fs::write(&p, bytes).map_err(|e| Diagnostic::io(headline, &p, &e))
    };
    write(CONTRACT_FILE, &contract_bytes)?;
    for (file, bytes) in &files {
        write(file, bytes)?;
    }
    write(verify::MANIFEST_FILE, pkg::to_toml(&manifest).as_bytes())?;

    eprintln!("Verifying package...");
    if let Err(mut problems) = verify::run(&staging) {
        problems.push(
            refuse(
                Kind::IntegrityFailure,
                headline,
                format!(
                    "The package failed verification as written. It was left in {} for inspection.",
                    staging.display()
                ),
            )
            .next("Report this: a package this command wrote should always verify."),
        );
        return Err(problems);
    }

    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|e| Diagnostic::io(headline, &dest, &e))?;
    }
    std::fs::rename(&staging, &dest).map_err(|e| Diagnostic::io(headline, &dest, &e))?;
    Ok(dest)
}
