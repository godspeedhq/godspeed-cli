//! `godspeed check`: validate the project without building or publishing anything.
//!
//! Collects every problem rather than stopping at the first, so one run shows everything to fix.

use crate::diag::{Diagnostic, Kind, Result, Stage};
use crate::platform::Platform;
use crate::project::{Project, ProjectKind};

fn problem(reason: String) -> Diagnostic {
    Diagnostic::new(Kind::InvalidProject, Stage::Validation, "check the project", reason)
}

pub fn run(platform: &Platform, project: &Project) -> Result<()> {
    let mut problems = Vec::new();

    let Some(cargo_name) = project.cargo_name() else {
        return Err(problem("Cargo.toml has no [package] name.".into()).into());
    };

    check_toolchain(platform, project, &mut problems);
    check_stdlib(platform, project, &mut problems);
    if project.kind.runs() {
        check_contract(platform, project, cargo_name, &mut problems);
        check_entry_point(project, &platform.entry, &mut problems);
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// The project must be built with the compiler its OS release is built with.
pub fn check_toolchain(platform: &Platform, project: &Project, problems: &mut Vec<Diagnostic>) {
    match project.toolchain() {
        Some(ch) if ch == platform.toolchain => {}
        Some(ch) => problems.push(
            problem(format!(
                "rust-toolchain.toml pins {ch}, but GodspeedOS {} is built with {}.",
                platform.version, platform.toolchain
            ))
            .expected(format!("channel = \"{}\"", platform.toolchain))
            .next("Set the channel to the OS's pin, or point `--os` at the OS release this project targets."),
        ),
        None => problems.push(
            problem("rust-toolchain.toml is missing or has no channel.".into())
                .expected(format!("[toolchain] channel = \"{}\"", platform.toolchain)),
        ),
    }
}

/// If the project depends on the stdlib, it must be the release of the OS it is built against.
fn check_stdlib(platform: &Platform, project: &Project, problems: &mut Vec<Diagnostic>) {
    let Some(dep) = project.stdlib_dependency() else {
        if project.kind.runs() {
            problems.push(
                problem("Cargo.toml does not depend on `godspeed`, the Godspeed standard library.".into())
                    .expected(format!(
                        "godspeed = {{ git = \"{}\", tag = \"{}\" }}",
                        crate::platform::OS_GIT_URL,
                        platform.tag()
                    ))
                    .next("A service is written on the public stdlib and nothing below it."),
            );
        }
        return;
    };
    match dep.get("tag").and_then(|t| t.as_str()) {
        Some(tag) if tag == platform.tag() => {}
        Some(tag) => problems.push(
            problem(format!(
                "Cargo.toml uses the stdlib at {tag}, but the OS checkout is {}.",
                platform.tag()
            ))
            .expected(format!("tag = \"{}\"", platform.tag()))
            .next("Change the tag to match, or point `--os` at a checkout of that release."),
        ),
        None => problems.push(
            problem(
                "The `godspeed` dependency is not pinned to an OS release tag, so which stdlib it builds against \
             cannot be checked."
                    .into(),
            )
            .expected(format!(
                "godspeed = {{ git = \"{}\", tag = \"{}\" }}",
                crate::platform::OS_GIT_URL,
                platform.tag()
            )),
        ),
    }
}

fn check_contract(platform: &Platform, project: &Project, cargo_name: &str, problems: &mut Vec<Diagnostic>) {
    let rel = project.contract.as_ref().expect("a service always has a contract path");
    let path = project.root.join(rel);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            problems.push(
                problem(format!("The contract {} cannot be read: {e}", rel.display()))
                    .next(format!("Create it, or fix `contract` in {}.", crate::project::MANIFEST)),
            );
            return;
        }
    };
    let contract: toml::Value = match text.parse::<toml::Table>() {
        Ok(t) => toml::Value::Table(t),
        Err(e) => {
            problems.push(problem(format!(
                "The contract {} is not valid TOML: {}",
                rel.display(),
                e.message()
            )));
            return;
        }
    };

    for msg in schema_errors(platform, &contract) {
        problems.push(
            problem(format!(
                "The contract {} does not satisfy the GodspeedOS contract schema: {msg}",
                rel.display()
            ))
            .expected("a contract valid against contracts/schema/service.schema.json in the OS checkout"),
        );
    }

    // `handover` (a hot upgrade) is a SERVICE's: a utility runs for one command, an app is reloaded by its user when
    // a new version is installed, and a driver is always stopped and started - two drivers side by side would
    // program one device. The OS refuses it on a driver at every start; this says so before anything is built.
    let handover = contract
        .get("capabilities")
        .and_then(|c| c.get("handover"))
        .and_then(|h| h.as_bool())
        .unwrap_or(false);
    if handover && project.kind != ProjectKind::Service {
        let kind = project.kind.as_str();
        problems.push(
            problem(format!(
                "The contract {} declares `handover`, and this project is a{} {kind}: only a service is upgraded hot.",
                rel.display(),
                if kind.starts_with('a') { "n" } else { "" },
            ))
            .next("Remove `handover` from [capabilities]. A new version of this project replaces the old one by a stop and start."),
        );
    }

    let cname = contract.get("name").and_then(|n| n.as_str());
    if let Some(cname) = cname {
        if cname != cargo_name {
            problems.push(problem(format!(
                "The contract names the service \"{cname}\" and Cargo.toml names the package \"{cargo_name}\"."
            )));
        }
        let expected_file = format!("{cname}.toml");
        if rel.file_name().and_then(|f| f.to_str()) != Some(expected_file.as_str()) {
            problems.push(
                problem(format!("The contract for \"{cname}\" is in {}.", rel.display())).expected(format!(
                    "contracts/{expected_file}, the file name the OS uses for a service's contract"
                )),
            );
        }
    }
    if let (Some(cv), Some(pv)) = (
        contract.get("version").and_then(|v| v.as_str()),
        project.cargo_version(),
    ) {
        if cv != pv {
            problems.push(
                problem(format!("The contract says version {cv} and Cargo.toml says {pv}."))
                    .next("Keep them equal: they are one release of one service."),
            );
        }
    }
}

/// Every message the schema produces for this contract, with where in the contract it applies.
pub fn schema_errors(platform: &Platform, contract: &toml::Value) -> Vec<String> {
    let instance = match serde_json::to_value(contract) {
        Ok(v) => v,
        Err(e) => return vec![format!("it cannot be read as data: {e}")],
    };
    let schema = match jsonschema::JSONSchema::compile(&platform.schema) {
        Ok(s) => s,
        Err(e) => {
            return vec![format!(
                "the OS's schema itself does not compile ({e}), so nothing can be checked"
            )]
        }
    };
    let result = schema.validate(&instance);
    match result {
        Ok(()) => Vec::new(),
        Err(errors) => errors
            .map(|e| {
                let at = e.instance_path.to_string();
                if at.is_empty() {
                    e.to_string()
                } else {
                    format!("at {at}: {e}")
                }
            })
            .collect(),
    }
}

/// The one mistake that compiles, links and then fails silently at boot: an unexported entry point. `entry` is
/// the release's entry symbol (its platform file's, or `service_main` for a release that has none).
fn check_entry_point(project: &Project, entry: &str, problems: &mut Vec<Diagnostic>) {
    let path = project.root.join("src/main.rs");
    let signature = format!("`pub extern \"C\" fn {entry}(ctx: ServiceContext) -> !`");
    let Ok(src) = std::fs::read_to_string(&path) else {
        problems
            .push(problem("src/main.rs is missing.".into()).expected(format!("a service entry point: {signature}")));
        return;
    };
    if !src.contains(&format!("fn {entry}")) {
        problems.push(
            problem(format!("src/main.rs has no `{entry}`."))
                .expected(format!("{signature} - where every service starts")),
        );
    } else if !src.contains("no_mangle") {
        problems.push(
            problem(format!("`{entry}` is not marked `#[no_mangle]`.")).expected(format!(
                "#[no_mangle] on `{entry}`. Without it the loader cannot find the entry point; the \
                       service would start at address 0 and fault before its first line"
            )),
        );
    }
}
