//! `godspeed build [--arch <isa> | --all]`: validate, fetch dependencies, then compile for each
//! architecture, reporting each stage only once its outcome is established.
//!
//! The output is one ELF per architecture: separate binaries for separate ISAs, one logical project.
//! It is not an OS image, and not installed anywhere.

use super::{cargo, check, installed_targets};
use crate::diag::{Diagnostic, Kind, Result, Stage};
use crate::platform::Platform;
use crate::project::{Project, ProjectKind};
use crate::target::{self, Arch};
use std::path::PathBuf;

/// The architecture built when none is named.
pub const DEFAULT_ARCH: &str = "x86_64";

pub struct Built {
    pub arch: &'static Arch,
    pub artifact: PathBuf,
}

/// Resolve `--arch` / `--all` to architectures, refusing any the OS does not build for.
pub fn select(platform: &Platform, arch: Option<&str>, all: bool) -> Result<Vec<&'static Arch>> {
    if all {
        let supported: Vec<_> = target::ARCHES.iter().filter(|a| platform.supports(a.triple)).collect();
        if supported.is_empty() {
            return Err(Diagnostic::new(
                Kind::UnavailableTarget,
                Stage::Validation,
                "build for every architecture",
                format!(
                    "GodspeedOS {} pins none of the architectures this CLI knows ({}).",
                    platform.version,
                    target::names()
                ),
            )
            .into());
        }
        return Ok(supported);
    }
    let name = arch.unwrap_or(DEFAULT_ARCH);
    let Some(a) = target::by_name(name) else {
        return Err(Diagnostic::new(
            Kind::UnavailableTarget,
            Stage::Validation,
            format!("build for {name}"),
            format!("\"{name}\" is not an architecture Godspeed runs on."),
        )
        .expected(target::names())
        .into());
    };
    if !platform.supports(a.triple) {
        return Err(Diagnostic::new(
            Kind::UnavailableTarget,
            Stage::Validation,
            format!("build for {name}"),
            format!(
                "GodspeedOS {} does not build for {} ({}).",
                platform.version, a.name, a.triple
            ),
        )
        .expected(format!(
            "one of the targets in the OS's rust-toolchain.toml: {}",
            platform.targets.join(", ")
        ))
        .into());
    }
    Ok(vec![a])
}

pub fn run(platform: &Platform, project: &Project, arches: &[&'static Arch]) -> Result<Vec<Built>> {
    eprintln!("Validating project...");
    check::run(platform, project)?;
    let name = project
        .cargo_name()
        .expect("check::run refuses a project with no package name")
        .to_string();

    // Every target must be installed before anything is compiled, so a missing one is reported as an
    // environment problem up front rather than as a compiler error halfway through `--all`.
    let installed = installed_targets(&platform.toolchain)?;
    let missing: Vec<_> = arches
        .iter()
        .filter(|a| !installed.iter().any(|t| t == a.triple))
        .collect();
    if !missing.is_empty() {
        let list = missing.iter().map(|a| a.triple).collect::<Vec<_>>().join(" ");
        let names = missing.iter().map(|a| a.name).collect::<Vec<_>>().join(", ");
        return Err(Diagnostic::new(
            Kind::Environment,
            Stage::Setup,
            format!("build {name} for {names}"),
            format!("The Godspeed {names} target toolchain is not installed."),
        )
        .expected(format!("{list} installed for {}", platform.toolchain))
        .next(format!(
            "Run `rustup target add --toolchain {} {list}`, or `godspeed doctor` for setup guidance.",
            platform.toolchain
        ))
        .into());
    }

    eprintln!("Fetching dependencies...");
    let fetched = cargo(platform, &project.root).arg("fetch").args(super::stdlib_patch(platform)).status();
    match fetched {
        Ok(s) if s.success() => {}
        Ok(_) => {
            return Err(Diagnostic::new(
                Kind::MissingDependency,
                Stage::Dependencies,
                format!("build {name}"),
                "Cargo could not fetch the project's dependencies (output above).",
            )
            .next("Check the network and the dependency tags in Cargo.toml, then retry.")
            .into())
        }
        Err(e) => {
            return Err(Diagnostic::new(
                Kind::Environment,
                Stage::Dependencies,
                format!("build {name}"),
                format!("`cargo` could not be run: {e}"),
            )
            .next("Run `godspeed doctor`.")
            .into())
        }
    }

    let mut built = Vec::new();
    for arch in arches {
        eprintln!("Building {name} for {} ({})...", arch.name, arch.triple);
        let mut flags = platform.rustflags(arch.triple);
        if project.kind.runs() {
            // How a service is linked is the OS's fact, read from its checkout (see `platform`).
            flags.push("-C".into());
            flags.push(format!("link-arg=-T{}", platform.linker_script.display()));
            flags.push("-C".into());
            flags.push(format!("link-arg=--entry={}", platform.entry));
        }
        let flags_toml = toml::Value::Array(flags.into_iter().map(toml::Value::String).collect());
        let status = cargo(platform, &project.root)
            .args(["build", "--release", "--target", arch.triple, "--config"])
            .arg(format!("target.{}.rustflags={}", arch.triple, flags_toml))
            .args(super::stdlib_patch(platform))
            .status();
        match status {
            Ok(s) if s.success() => {}
            Ok(_) => {
                return Err(Diagnostic::new(
                    Kind::CompilationFailure,
                    Stage::Compilation,
                    format!("build {name} for {}", arch.name),
                    "The compiler rejected the code (errors above).",
                )
                .next("Fix the errors and run `godspeed build` again.")
                .into())
            }
            Err(e) => {
                return Err(Diagnostic::new(
                    Kind::Environment,
                    Stage::Compilation,
                    format!("build {name} for {}", arch.name),
                    format!("`cargo` could not be run: {e}"),
                )
                .into())
            }
        }

        let dir = project.root.join("target").join(arch.triple).join("release");
        let artifact = match project.kind {
            ProjectKind::Service | ProjectKind::Utility | ProjectKind::App => dir.join(&name),
            ProjectKind::Library => dir.join(format!("lib{}.rlib", name.replace('-', "_"))),
        };
        if !artifact.is_file() {
            return Err(Diagnostic::new(
                Kind::CompilationFailure,
                Stage::Compilation,
                format!("build {name} for {}", arch.name),
                format!("Cargo reported success but {} does not exist.", artifact.display()),
            )
            .next("Report this: the CLI's idea of where Cargo puts the output is wrong.")
            .into());
        }
        built.push(Built { arch, artifact });
    }
    Ok(built)
}
