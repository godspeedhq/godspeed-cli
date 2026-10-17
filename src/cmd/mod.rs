pub mod account;
pub mod build;
pub mod check;
pub mod doctor;
pub mod fetch;
pub mod new;
pub mod package;
pub mod publish;
pub mod self_manage;
pub mod sign;
pub mod test;
pub mod unavailable;
pub mod verify;

use crate::diag::{Diagnostic, Kind, Stage};
use crate::platform::{Platform, Source, OS_GIT_URL};
use std::path::Path;
use std::process::Command;

/// A `cargo` invocation in `dir`, pinned to the OS's toolchain.
///
/// The pin is set explicitly rather than left to the project's `rust-toolchain.toml`, because a rustup
/// directory override would otherwise win over the file and build with a compiler nobody chose.
pub fn cargo(platform: &Platform, dir: &Path) -> Command {
    let mut c = Command::new("cargo");
    c.current_dir(dir).env("RUSTUP_TOOLCHAIN", &platform.toolchain);
    c
}

/// `--config` arguments that build against a CHECKOUT's own standard library, for after the subcommand.
///
/// Built against a checkout (`--os`), the project gets that checkout's stdlib, not the release its Cargo.toml
/// names: "built against this OS" is then literally true, and a checkout ahead of its last release can be
/// developed against before that release exists. A cached kit has no stdlib of its own, so the tag is used as
/// written. Put AFTER the subcommand, beside any other `--config`: given before the subcommand, it was replaced
/// by the rustflags `--config` that `build` gives after it, and the build silently used the tag.
pub fn stdlib_patch(platform: &Platform) -> Vec<String> {
    if !matches!(platform.source, Source::Checkout) {
        return Vec::new();
    }
    let stdlib = platform.root.join("stdlib").join("rust");
    if !stdlib.join("Cargo.toml").exists() {
        return Vec::new();
    }
    let path = toml::Value::String(stdlib.to_string_lossy().replace('\\', "/"));
    vec![
        "--config".into(),
        format!("patch.\"{OS_GIT_URL}\".godspeed.path={path}"),
    ]
}

/// The Rust targets installed for `toolchain`, or a diagnostic saying the toolchain itself is missing.
pub fn installed_targets(toolchain: &str) -> Result<Vec<String>, Diagnostic> {
    let out = Command::new("rustup")
        .args(["target", "list", "--installed", "--toolchain", toolchain])
        .output();
    match out {
        Err(e) => Err(Diagnostic::new(
            Kind::Environment,
            Stage::Setup,
            "find the Rust toolchain",
            format!("`rustup` could not be run: {e}"),
        )
        .expected("rustup, which installs and selects the toolchain GodspeedOS pins")
        .next("Install rustup from https://rustup.rs, then run `godspeed doctor`.")),
        Ok(o) if !o.status.success() => Err(Diagnostic::new(
            Kind::Environment,
            Stage::Setup,
            "find the Rust toolchain",
            format!("The toolchain {toolchain} is not installed."),
        )
        .expected(format!(
            "{toolchain}, the compiler this GodspeedOS release is built with"
        ))
        .next(format!(
            "Run `rustup toolchain install {toolchain}`, then `godspeed doctor`."
        ))),
        Ok(o) => Ok(String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()),
    }
}
