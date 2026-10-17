//! A Godspeed project on disk: `godspeed.toml`, Cargo's manifest, and (for a service) the OS contract.
//!
//! Three files, three jobs, deliberately not merged:
//! - `Cargo.toml` is how Rust builds the code;
//! - `contracts/<name>.toml` is the OS's own service contract (its schema is the OS's), and is the ONE
//!   place the project declares the authority it requests - the CLI never keeps a second copy;
//! - `godspeed.toml` says what kind of project this is, where its contract is, and what a package of it
//!   says about itself (publisher, description, license) - nothing that one of the other two already
//!   says. Name and version come from the contract.

use crate::diag::{Diagnostic, Kind, Stage};
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const MANIFEST: &str = "godspeed.toml";
/// The `godspeed.toml` format version this CLI reads and writes.
pub const FORMAT: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    Service,
    /// A program that does one job and ends: built and packaged exactly as a service, with a contract, and run by
    /// the shell for one command line instead of kept running.
    Utility,
    /// A utility that has the screen and the keyboard while it runs (`gs::app`).
    App,
    /// A service that drives one device, named by its PCI class in the contract.
    Driver,
    Library,
}

impl ProjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectKind::Service => "service",
            ProjectKind::Utility => "utility",
            ProjectKind::App => "app",
            ProjectKind::Driver => "driver",
            ProjectKind::Library => "library",
        }
    }

    /// Is this a program GodspeedOS runs - built to an image, with a contract - rather than a library?
    pub fn runs(self) -> bool {
        matches!(self, ProjectKind::Service | ProjectKind::Utility | ProjectKind::App | ProjectKind::Driver)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    format: u32,
    kind: String,
    contract: Option<String>,
    publisher: Option<String>,
    description: Option<String>,
    license: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    pub kind: ProjectKind,
    /// Relative to `root`. Present exactly when `kind` is `Service`.
    pub contract: Option<PathBuf>,
    /// The namespace a package of this project is published under: `alice` in `alice/hello`. A claim
    /// until a registry checks it; a package name is not an identity.
    pub publisher: Option<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    pub cargo: toml::Table,
}

fn invalid(headline: &str, reason: String) -> Diagnostic {
    Diagnostic::new(Kind::InvalidProject, Stage::Validation, headline, reason)
}

impl Project {
    /// Find the project containing `start`: the nearest directory at or above it holding `godspeed.toml`.
    pub fn find(start: &Path) -> Result<Project, Diagnostic> {
        let mut dir = Some(start);
        while let Some(d) = dir {
            if d.join(MANIFEST).is_file() {
                return Project::open(d);
            }
            dir = d.parent();
        }
        Err(invalid(
            "find a Godspeed project",
            format!("No {MANIFEST} in {} or any directory above it.", start.display()),
        )
        .next("Run this inside a project, or create one with `godspeed new service <name>`."))
    }

    pub fn open(root: &Path) -> Result<Project, Diagnostic> {
        let mpath = root.join(MANIFEST);
        let text = std::fs::read_to_string(&mpath).map_err(|e| Diagnostic::io("read the project", &mpath, &e))?;
        let raw: RawManifest = toml::from_str(&text).map_err(|e| {
            invalid("read the project", format!("{MANIFEST} is not valid: {}", e.message())).expected(
                "format = 1, kind = \"service\" | \"library\", and for a service contract = \"contracts/<name>.toml\"",
            )
        })?;
        if raw.format != FORMAT {
            return Err(invalid(
                "read the project",
                format!("{MANIFEST} is format {}; this CLI reads format {FORMAT}.", raw.format),
            )
            .next("Use a CLI release that reads that format."));
        }
        let kind = match raw.kind.as_str() {
            "service" => ProjectKind::Service,
            "utility" => ProjectKind::Utility,
            "app" => ProjectKind::App,
            "driver" => ProjectKind::Driver,
            "library" => ProjectKind::Library,
            other => {
                return Err(
                    invalid("read the project", format!("{MANIFEST} names kind \"{other}\"."))
                        .expected("\"service\", \"utility\", \"app\", \"driver\" or \"library\" - the kinds GodspeedOS can build today"),
                );
            }
        };
        let contract = match (kind, raw.contract) {
            (ProjectKind::Service | ProjectKind::Utility | ProjectKind::App | ProjectKind::Driver, Some(c)) => Some(PathBuf::from(c)),
            (ProjectKind::Service | ProjectKind::Utility | ProjectKind::App | ProjectKind::Driver, None) => {
                return Err(
                    invalid("read the project", format!("{MANIFEST} is a service with no contract."))
                        .expected("contract = \"contracts/<name>.toml\": a service declares the authority it requests"),
                );
            }
            (ProjectKind::Library, Some(_)) => {
                return Err(
                    invalid("read the project", format!("{MANIFEST} gives a library a contract."))
                        .expected("no contract: a library is not spawned, so it requests no authority of its own"),
                );
            }
            (ProjectKind::Library, None) => None,
        };

        let cpath = root.join("Cargo.toml");
        let ctext = std::fs::read_to_string(&cpath).map_err(|e| Diagnostic::io("read the project", &cpath, &e))?;
        let cargo = ctext.parse::<toml::Table>().map_err(|e| {
            invalid(
                "read the project",
                format!("Cargo.toml is not valid TOML: {}", e.message()),
            )
        })?;

        if let Some(p) = &raw.publisher {
            if !godspeed_package::PackageName::is_valid_part(p) {
                return Err(
                    invalid("read the project", format!("{MANIFEST} names publisher \"{p}\"."))
                        .expected("a lowercase letter, then lowercase letters, digits and '-', at most 64 bytes"),
                );
            }
        }

        Ok(Project {
            root: root.to_path_buf(),
            kind,
            contract,
            publisher: raw.publisher,
            description: raw.description,
            license: raw.license,
            cargo,
        })
    }

    pub fn cargo_name(&self) -> Option<&str> {
        self.cargo.get("package")?.get("name")?.as_str()
    }

    pub fn cargo_version(&self) -> Option<&str> {
        self.cargo.get("package")?.get("version")?.as_str()
    }

    /// The `godspeed` dependency, if declared.
    pub fn stdlib_dependency(&self) -> Option<&toml::Value> {
        self.cargo.get("dependencies")?.get("godspeed")
    }

    /// The toolchain channel the project pins, if it has a `rust-toolchain.toml`.
    pub fn toolchain(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.root.join("rust-toolchain.toml")).ok()?;
        let t = text.parse::<toml::Table>().ok()?;
        Some(t.get("toolchain")?.get("channel")?.as_str()?.to_string())
    }
}
