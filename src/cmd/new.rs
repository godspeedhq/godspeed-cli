//! `godspeed new <kind> <name>`: create the smallest valid project of a kind the OS can build.
//!
//! A template ships only for a kind GodspeedOS can actually run or build today. Every other kind gets a
//! refusal naming what the OS is missing and where that is tracked, never a template that pretends.

use crate::diag::{Diagnostic, Kind, Result, Stage};
use crate::platform::{Platform, OS_GIT_URL};
use crate::project::ProjectKind;
use std::path::{Path, PathBuf};

struct File {
    path: &'static str,
    body: &'static str,
}

const TOOLCHAIN: &str = include_str!("../../templates/common/rust-toolchain.toml");
const GITIGNORE: &str = include_str!("../../templates/common/gitignore");

const SERVICE: &[File] = &[
    File {
        path: "godspeed.toml",
        body: include_str!("../../templates/service/godspeed.toml"),
    },
    File {
        path: "Cargo.toml",
        body: include_str!("../../templates/service/Cargo.toml"),
    },
    File {
        path: "contracts/{{name}}.toml",
        body: include_str!("../../templates/service/contracts/contract.toml"),
    },
    File {
        path: "src/main.rs",
        body: include_str!("../../templates/service/src/main.rs"),
    },
    File {
        path: "README.md",
        body: include_str!("../../templates/service/README.md"),
    },
    File {
        path: "rust-toolchain.toml",
        body: TOOLCHAIN,
    },
    File {
        path: ".gitignore",
        body: GITIGNORE,
    },
];

const LIBRARY: &[File] = &[
    File {
        path: "godspeed.toml",
        body: include_str!("../../templates/library/godspeed.toml"),
    },
    File {
        path: "Cargo.toml",
        body: include_str!("../../templates/library/Cargo.toml"),
    },
    File {
        path: "src/lib.rs",
        body: include_str!("../../templates/library/src/lib.rs"),
    },
    File {
        path: "README.md",
        body: include_str!("../../templates/library/README.md"),
    },
    File {
        path: "rust-toolchain.toml",
        body: TOOLCHAIN,
    },
    File {
        path: ".gitignore",
        body: GITIGNORE,
    },
];

/// Kinds the CLI knows and the OS cannot run yet: why, and where the OS tracks it.
const NOT_YET: &[(&str, &str, &str)] = &[
    (
        "utility",
        "A utility is a program that does one job and ends. GodspeedOS has no terminating task yet: every \
      program is a service that never returns, and there is no way for a program to end itself.",
        "GodspeedOS docs/stdlib-design.md, section 1 (options A and B)",
    ),
    (
        "app",
        "An app is a user-facing program on public application interfaces. GodspeedOS has none yet, and no \
      terminating task either.",
        "GodspeedOS docs/stdlib-design.md, section 1",
    ),
    (
        "driver",
        "A driver needs a device granted to it. GodspeedOS grants devices only from the supervisor's own \
      compiled spawn rows, so a driver outside the OS tree cannot be given one.",
        "the supervisor's spawn rows, by device kind (GodspeedOS services/supervisor)",
    ),
];

const KINDS: &str = "service, library (and later: utility, app, driver)";

pub fn run(platform: &Platform, kind: &str, name: &str, dir: Option<&Path>) -> Result<PathBuf> {
    let kind = match kind {
        "service" => ProjectKind::Service,
        "library" => ProjectKind::Library,
        other => {
            if let Some((k, why, tracked)) = NOT_YET.iter().find(|(k, ..)| *k == other) {
                return Err(Diagnostic::new(
                    Kind::NotProvidedByOs,
                    Stage::Validation,
                    format!("create a {k} project"),
                    *why,
                )
                .expected(format!("tracked in {tracked}"))
                .next("Create a `service` instead - every runnable program on Godspeed is a service today.")
                .into());
            }
            return Err(Diagnostic::new(
                Kind::InvalidProject,
                Stage::Validation,
                format!("create a \"{other}\" project"),
                "There is no such project kind.",
            )
            .expected(KINDS)
            .into());
        }
    };

    check_name(platform, name)?;

    let root = dir.map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(name));
    if root.exists() && std::fs::read_dir(&root).map(|mut d| d.next().is_some()).unwrap_or(true) {
        return Err(Diagnostic::new(
            Kind::InvalidProject,
            Stage::Setup,
            format!("create {}", root.display()),
            "The directory already exists and is not empty. Nothing was written.",
        )
        .next("Choose another name, or pass an empty directory with `--dir`.")
        .into());
    }

    let files = match kind {
        ProjectKind::Service => SERVICE,
        ProjectKind::Library => LIBRARY,
    };
    let targets = platform
        .targets
        .iter()
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let fill = |s: &str| {
        s.replace("{{name}}", name)
            .replace("{{tag}}", &platform.tag())
            .replace("{{git}}", OS_GIT_URL)
            .replace("{{toolchain}}", &platform.toolchain)
            .replace("{{targets}}", &targets)
    };

    for f in files {
        let path = root.join(fill(f.path));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Diagnostic::io("create the project", parent, &e))?;
        }
        std::fs::write(&path, fill(f.body)).map_err(|e| Diagnostic::io("create the project", &path, &e))?;
    }
    Ok(root)
}

/// The name must be one the OS contract schema accepts, checked against the schema's own pattern.
fn check_name(platform: &Platform, name: &str) -> Result<()> {
    let Some(pattern) = platform.name_pattern() else {
        return Ok(()); // the schema states no pattern, so there is nothing to hold the name to
    };
    let schema = serde_json::json!({ "type": "string", "pattern": pattern });
    let valid = jsonschema::JSONSchema::compile(&schema)
        .map(|s| s.is_valid(&serde_json::Value::String(name.to_string())))
        .unwrap_or(true);
    if valid {
        Ok(())
    } else {
        Err(Diagnostic::new(
            Kind::InvalidProject,
            Stage::Validation,
            format!("create a project named \"{name}\""),
            "The name does not match what the GodspeedOS contract schema accepts.",
        )
        .expected(format!(
            "a name matching {pattern}: lowercase letters, digits and hyphens, starting with a letter"
        ))
        .into())
    }
}
