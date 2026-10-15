//! The manifest's text form, `manifest.toml`. Host-only (feature `toml`).
//!
//! Reading is strict: an unknown key, a missing field or a value of the wrong form is an error, never
//! ignored, because a reader that skips what it does not understand would install a package on the
//! strength of half its manifest. Writing is canonical: one manifest has exactly one text form.

use crate::arch::Arch;
use crate::digest::Sha256Digest;
use crate::manifest::{Artifact, Kind, Manifest, Problem, ProblemKind, FORMAT};
use crate::name::PackageName;
use crate::version::Version;
use serde::Deserialize;
use std::fmt::Write as _;
use std::string::{String, ToString};
use std::vec::Vec;

/// A manifest read from text, owning its strings. [`OwnedManifest::manifest`] views it as a
/// [`Manifest`], the type everything else in this crate works on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedManifest {
    name: String,
    version: Version,
    kind: Kind,
    description: Option<String>,
    license: Option<String>,
    os_release: Version,
    contract: Sha256Digest,
    artifacts: Vec<Artifact>,
}

impl OwnedManifest {
    /// The manifest, borrowed.
    pub fn manifest(&self) -> Manifest<'_> {
        Manifest {
            format: FORMAT,
            name: PackageName::parse(&self.name).expect("validated when the manifest was read"),
            version: self.version,
            kind: self.kind,
            description: self.description.as_deref(),
            license: self.license.as_deref(),
            os_release: self.os_release,
            contract: self.contract,
            artifacts: &self.artifacts,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    format: Option<u32>,
    package: Option<RawPackage>,
    os: Option<RawOs>,
    contract: Option<RawContract>,
    #[serde(default)]
    artifact: Vec<RawArtifact>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPackage {
    name: Option<String>,
    version: Option<String>,
    kind: Option<String>,
    description: Option<String>,
    license: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOs {
    release: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawContract {
    digest: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawArtifact {
    arch: Option<String>,
    size: Option<u64>,
    digest: Option<String>,
}

/// Why text is not a valid manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// Not TOML, or a key this format does not have, or a value of the wrong TOML type.
    Syntax(String),
    /// Well-formed TOML that breaks the manifest's rules: every problem found.
    Invalid(Vec<Problem>),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Syntax(e) => write!(f, "not a valid manifest: {e}"),
            ParseError::Invalid(ps) => {
                for (i, p) in ps.iter().enumerate() {
                    if i > 0 {
                        f.write_str("\n")?;
                    }
                    write!(f, "{p}")?;
                }
                Ok(())
            }
        }
    }
}

struct Collect(Vec<Problem>);

impl Collect {
    fn bad(&mut self, field: &'static str, kind: ProblemKind) {
        self.0.push(Problem { field, kind });
    }

    fn required<'s>(&mut self, field: &'static str, v: &'s Option<String>) -> Option<&'s str> {
        if v.is_none() {
            self.bad(field, ProblemKind::Missing);
        }
        v.as_deref()
    }

    fn version(&mut self, field: &'static str, v: &Option<String>) -> Option<Version> {
        let s = self.required(field, v)?;
        s.parse()
            .map_err(|e: crate::version::VersionError| self.bad(field, ProblemKind::Malformed(e.message())))
            .ok()
    }

    fn digest(&mut self, field: &'static str, v: &Option<String>) -> Option<Sha256Digest> {
        let s = self.required(field, v)?;
        s.parse()
            .map_err(|e: crate::digest::DigestError| self.bad(field, ProblemKind::Malformed(e.message())))
            .ok()
    }
}

/// Read a manifest from its text form. Every rule is checked, the cross-field ones included, so an
/// `Ok` is a manifest that passes [`Manifest::validate`].
pub fn parse(text: &str) -> Result<OwnedManifest, ParseError> {
    let raw: Raw = toml::from_str(text).map_err(|e| ParseError::Syntax(e.message().to_string()))?;
    let mut c = Collect(Vec::new());

    // The format decides how everything else is read, so an unknown one stops here.
    match raw.format {
        None => {
            return Err(ParseError::Invalid(std::vec![Problem {
                field: "format",
                kind: ProblemKind::Missing
            }]))
        }
        Some(n) if n != FORMAT => {
            return Err(ParseError::Invalid(std::vec![Problem {
                field: "format",
                kind: ProblemKind::UnknownFormat(n)
            }]))
        }
        Some(_) => {}
    }

    let empty_package = RawPackage {
        name: None,
        version: None,
        kind: None,
        description: None,
        license: None,
    };
    let package = raw.package.as_ref().unwrap_or(&empty_package);
    let name = c
        .required("package.name", &package.name)
        .and_then(|n| match PackageName::parse(n) {
            Ok(_) => Some(n.to_string()),
            Err(e) => {
                c.bad("package.name", ProblemKind::Malformed(e.message()));
                None
            }
        });
    let version = c.version("package.version", &package.version);
    let kind = match c.required("package.kind", &package.kind) {
        Some("service") => Some(Kind::Service),
        Some("library") => {
            c.bad(
                "package.kind",
                ProblemKind::KindNotPackageable(
                    "a library is depended on, not installed, and how libraries are distributed is not defined yet",
                ),
            );
            None
        }
        Some(_) => {
            c.bad("package.kind", ProblemKind::Malformed("the kind is \"service\""));
            None
        }
        None => None,
    };

    let os_release = c.version("os.release", &raw.os.as_ref().and_then(|o| o.release.clone()));
    let contract = c.digest("contract.digest", &raw.contract.as_ref().and_then(|o| o.digest.clone()));

    let mut artifacts = Vec::new();
    for a in &raw.artifact {
        let arch = c.required("artifact.arch", &a.arch).and_then(|s| {
            s.parse::<Arch>()
                .map_err(|()| {
                    c.bad(
                        "artifact.arch",
                        ProblemKind::Malformed("one of x86_64, aarch64, armv7, riscv64"),
                    )
                })
                .ok()
        });
        if a.size.is_none() {
            c.bad("artifact.size", ProblemKind::Missing);
        }
        let digest = c.digest("artifact.digest", &a.digest);
        if let (Some(arch), Some(size), Some(digest)) = (arch, a.size, digest) {
            artifacts.push(Artifact { arch, size, digest });
        }
    }

    let (Some(name), Some(version), Some(kind), Some(os_release), Some(contract)) =
        (name, version, kind, os_release, contract)
    else {
        return Err(ParseError::Invalid(c.0));
    };
    let owned = OwnedManifest {
        name,
        version,
        kind,
        description: package.description.clone(),
        license: package.license.clone(),
        os_release,
        contract,
        artifacts,
    };
    owned.manifest().validate(|p| c.0.push(p));
    if c.0.is_empty() {
        Ok(owned)
    } else {
        Err(ParseError::Invalid(c.0))
    }
}

fn quoted(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

/// Write a manifest in its one canonical text form.
pub fn to_toml(m: &Manifest<'_>) -> String {
    let mut s = String::new();
    // Infallible: writing to a String cannot fail.
    let _ = writeln!(s, "format = {}", m.format);
    let _ = writeln!(s, "\n[package]");
    let _ = writeln!(s, "name = {}", quoted(&m.name.to_string()));
    let _ = writeln!(s, "version = \"{}\"", m.version);
    let _ = writeln!(s, "kind = \"{}\"", m.kind.as_str());
    if let Some(d) = m.description {
        let _ = writeln!(s, "description = {}", quoted(d));
    }
    if let Some(l) = m.license {
        let _ = writeln!(s, "license = {}", quoted(l));
    }
    let _ = writeln!(s, "\n[os]\nrelease = \"{}\"", m.os_release);
    let _ = writeln!(s, "\n[contract]\ndigest = \"{}\"", m.contract);
    for a in m.artifacts {
        let _ = writeln!(
            s,
            "\n[[artifact]]\narch = \"{}\"\nsize = {}\ndigest = \"{}\"",
            a.arch, a.size, a.digest
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::tests::{artifacts, manifest};

    #[test]
    fn round_trips_canonically() {
        let a = artifacts();
        let m = manifest(&a);
        let text = to_toml(&m);
        let back = parse(&text).unwrap();
        assert_eq!(back.manifest(), m);
        assert_eq!(to_toml(&back.manifest()), text, "one manifest, one text form");
        assert!(
            text.starts_with("format = 1\n\n[package]\nname = \"alice/hello\"\n"),
            "{text}"
        );
    }

    #[test]
    fn text_with_quotes_and_backslashes_survives() {
        let a = artifacts();
        let mut m = manifest(&a);
        m.description = Some("says \"hello\" \\ waves");
        assert_eq!(parse(&to_toml(&m)).unwrap().manifest(), m);
    }

    #[test]
    fn unknown_keys_are_refused_not_skipped() {
        let a = artifacts();
        let text = to_toml(&manifest(&a)).replace("[os]\n", "[os]\nlatest = true\n");
        assert!(matches!(parse(&text), Err(ParseError::Syntax(e)) if e.contains("latest")));
        let text =
            to_toml(&manifest(&a)).replace("[contract]\n", "[capabilities]\nservice_control = true\n\n[contract]\n");
        assert!(
            matches!(parse(&text), Err(ParseError::Syntax(_))),
            "a manifest cannot carry capabilities"
        );
    }

    #[test]
    fn an_unknown_format_stops_reading() {
        let a = artifacts();
        let text = to_toml(&manifest(&a))
            .replace("format = 1", "format = 2")
            .replace("1.0.0", "one");
        let Err(ParseError::Invalid(ps)) = parse(&text) else {
            panic!()
        };
        assert_eq!(
            ps,
            [Problem {
                field: "format",
                kind: ProblemKind::UnknownFormat(2)
            }]
        );
    }

    #[test]
    fn every_problem_is_reported() {
        let text = "format = 1\n[package]\nname = \"hello\"\nversion = \"1.02.0\"\nkind = \"library\"\n\
                    [[artifact]]\narch = \"mips\"\nsize = 10\ndigest = \"sha256:00\"\n";
        let Err(ParseError::Invalid(ps)) = parse(text) else {
            panic!()
        };
        let fields: Vec<_> = ps.iter().map(|p| p.field).collect();
        assert_eq!(
            fields,
            [
                "package.name",
                "package.version",
                "package.kind",
                "os.release",
                "contract.digest",
                "artifact.arch",
                "artifact.digest"
            ]
        );
        assert!(ps.iter().any(|p| matches!(p.kind, ProblemKind::KindNotPackageable(_))));
    }

    #[test]
    fn cross_field_rules_apply_when_reading() {
        let a = artifacts();
        let text = to_toml(&manifest(&a)).replace("arch = \"riscv64\"", "arch = \"x86_64\"");
        let Err(ParseError::Invalid(ps)) = parse(&text) else {
            panic!()
        };
        assert_eq!(
            ps,
            [Problem {
                field: "artifact.arch",
                kind: ProblemKind::DuplicateArch(Arch::X86_64)
            }]
        );
    }
}
