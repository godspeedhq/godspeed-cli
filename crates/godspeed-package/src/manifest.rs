//! The package manifest: what one version of one package is.

use crate::arch::Arch;
use crate::digest::Sha256Digest;
use crate::name::PackageName;
use crate::version::Version;
use core::fmt;

/// The manifest format this crate reads and writes. A reader refuses any other value: a format change
/// is a new number, never a silent reinterpretation (see `docs/package-format.md`, compatibility).
pub const FORMAT: u32 = 1;

/// The service contract's file name inside a package.
pub const CONTRACT_FILE: &str = "contract.toml";

/// The largest artifact a format-1 manifest may declare: 64 MiB. Services are kilobytes to a few
/// megabytes; the bound exists so a manifest cannot make a verifier expect an unbounded stream.
pub const MAX_ARTIFACT_SIZE: u64 = 64 * 1024 * 1024;

/// The longest description, in bytes.
pub const MAX_DESCRIPTION: usize = 256;

/// The longest license expression, in bytes.
pub const MAX_LICENSE: usize = 64;

/// What kind of program a package holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A managed component the OS spawns, with a service contract. The only kind GodspeedOS can run,
    /// so the only kind format 1 packages.
    Service,
}

impl Kind {
    /// The manifest spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Kind::Service => "service",
        }
    }
}

/// One architecture's binary: which architecture, how many bytes, and their digest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Artifact {
    /// The architecture the binary is for. Its file name in the package is [`Arch::artifact_file`].
    pub arch: Arch,
    /// Its exact length in bytes.
    pub size: u64,
    /// The SHA-256 of exactly those bytes.
    pub digest: Sha256Digest,
}

/// One version of one package.
///
/// It binds a name and version to the exact bytes of the service contract and of each architecture's
/// binary. It does NOT restate the contract's capabilities: the contract, bound here by digest, is the
/// single declaration of what the service requests, and it grants nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Manifest<'a> {
    /// The manifest format; must be [`FORMAT`].
    pub format: u32,
    /// `publisher/name`.
    pub name: PackageName<'a>,
    /// This version. Equal to the contract's version.
    pub version: Version,
    /// What the package holds.
    pub kind: Kind,
    /// One line for humans. Optional.
    pub description: Option<&'a str>,
    /// A license expression, e.g. `MIT` or `MIT OR Apache-2.0`. Optional; not interpreted.
    pub license: Option<&'a str>,
    /// The GodspeedOS release the package was built against. A fact about the build, not a claim of
    /// compatibility with any other release: GodspeedOS has no versioned public API yet to claim it of.
    pub os_release: Version,
    /// The SHA-256 of the service contract file ([`CONTRACT_FILE`]).
    pub contract: Sha256Digest,
    /// One artifact per architecture, in canonical order ([`Arch::ALL`]).
    pub artifacts: &'a [Artifact],
}

/// Which rule a manifest broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProblemKind {
    /// A required field is absent.
    Missing,
    /// A field's value has the wrong form; the `&str` says what form was expected.
    Malformed(&'static str),
    /// The manifest is a format this reader does not know.
    UnknownFormat(u32),
    /// The kind is one the format does not package; the `&str` says why.
    KindNotPackageable(&'static str),
    /// No artifacts at all.
    NoArtifacts,
    /// Two artifacts for the same architecture.
    DuplicateArch(Arch),
    /// Artifacts out of canonical order. One order means one text form for one manifest.
    ArtifactOrder,
    /// An artifact of zero bytes.
    EmptyArtifact(Arch),
    /// An artifact larger than [`MAX_ARTIFACT_SIZE`].
    ArtifactTooLarge(Arch),
    /// A text field is too long, or holds control characters.
    BadText,
    /// The package's name part differs from the contract's service name.
    NameDiffersFromContract,
    /// The package's version differs from the contract's.
    VersionDiffersFromContract,
}

/// A rule a manifest broke, and the field it broke it in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Problem {
    /// The manifest field, in its text form: `package.version`, `artifact.size`.
    pub field: &'static str,
    /// What is wrong.
    pub kind: ProblemKind,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let field = self.field;
        match self.kind {
            ProblemKind::Missing => write!(f, "{field} is required"),
            ProblemKind::Malformed(what) => write!(f, "{field}: {what}"),
            ProblemKind::UnknownFormat(n) => {
                write!(
                    f,
                    "{field}: this is manifest format {n}; this reader knows format {FORMAT}"
                )
            }
            ProblemKind::KindNotPackageable(why) => write!(f, "{field}: {why}"),
            ProblemKind::NoArtifacts => write!(f, "{field}: a package carries at least one architecture's binary"),
            ProblemKind::DuplicateArch(a) => write!(f, "{field}: two artifacts for {a}"),
            ProblemKind::ArtifactOrder => {
                write!(
                    f,
                    "{field}: artifacts are listed in the order x86_64, aarch64, armv7, riscv64"
                )
            }
            ProblemKind::EmptyArtifact(a) => write!(f, "{field}: the {a} artifact is empty"),
            ProblemKind::ArtifactTooLarge(a) => {
                write!(f, "{field}: the {a} artifact is larger than {MAX_ARTIFACT_SIZE} bytes")
            }
            ProblemKind::BadText => write!(f, "{field}: too long, or contains control characters"),
            ProblemKind::NameDiffersFromContract => {
                write!(f, "{field}: the package's name must be its service contract's name")
            }
            ProblemKind::VersionDiffersFromContract => {
                write!(
                    f,
                    "{field}: the package's version must be its service contract's version"
                )
            }
        }
    }
}

fn text_ok(s: &str, max: usize) -> bool {
    s.len() <= max && !s.chars().any(char::is_control)
}

impl Manifest<'_> {
    /// Check every rule that spans fields. Every problem is reported, through `report`, so one pass
    /// shows everything wrong; nothing is allocated. Returns whether the manifest is valid.
    ///
    /// The field-level rules (a name is a name, a version is a version) are already enforced by the
    /// types; this checks what the types cannot.
    pub fn validate(&self, mut report: impl FnMut(Problem)) -> bool {
        let mut ok = true;
        let mut bad = |field, kind| {
            ok = false;
            report(Problem { field, kind });
        };

        if self.format != FORMAT {
            bad("format", ProblemKind::UnknownFormat(self.format));
        }
        if let Some(d) = self.description {
            if !text_ok(d, MAX_DESCRIPTION) {
                bad("package.description", ProblemKind::BadText);
            }
        }
        if let Some(l) = self.license {
            if !text_ok(l, MAX_LICENSE) {
                bad("package.license", ProblemKind::BadText);
            }
        }

        if self.artifacts.is_empty() {
            bad("artifact", ProblemKind::NoArtifacts);
        }
        for (i, a) in self.artifacts.iter().enumerate() {
            if self.artifacts[..i].iter().any(|b| b.arch == a.arch) {
                bad("artifact.arch", ProblemKind::DuplicateArch(a.arch));
            } else if i > 0 && self.artifacts[i - 1].arch > a.arch {
                bad("artifact.arch", ProblemKind::ArtifactOrder);
            }
            if a.size == 0 {
                bad("artifact.size", ProblemKind::EmptyArtifact(a.arch));
            } else if a.size > MAX_ARTIFACT_SIZE {
                bad("artifact.size", ProblemKind::ArtifactTooLarge(a.arch));
            }
        }
        ok
    }

    /// Check the manifest against its service contract's own name and version (read from the contract
    /// by the caller). Returns whether they agree.
    pub fn validate_against_contract(
        &self,
        contract_name: &str,
        contract_version: &str,
        mut report: impl FnMut(Problem),
    ) -> bool {
        let mut ok = true;
        if self.name.name() != contract_name {
            ok = false;
            report(Problem {
                field: "package.name",
                kind: ProblemKind::NameDiffersFromContract,
            });
        }
        if contract_version.parse::<Version>() != Ok(self.version) {
            ok = false;
            report(Problem {
                field: "package.version",
                kind: ProblemKind::VersionDiffersFromContract,
            });
        }
        ok
    }

    /// The artifact for `arch`, if this package carries one.
    pub fn artifact(&self, arch: Arch) -> Option<&Artifact> {
        self.artifacts.iter().find(|a| a.arch == arch)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::vec::Vec;

    pub(crate) fn artifacts() -> [Artifact; 2] {
        [
            Artifact {
                arch: Arch::X86_64,
                size: 4096,
                digest: Sha256Digest::of(b"x86"),
            },
            Artifact {
                arch: Arch::Riscv64,
                size: 4100,
                digest: Sha256Digest::of(b"rv"),
            },
        ]
    }

    pub(crate) fn manifest(artifacts: &[Artifact]) -> Manifest<'_> {
        Manifest {
            format: FORMAT,
            name: PackageName::parse("alice/hello").unwrap(),
            version: Version::new(1, 0, 0),
            kind: Kind::Service,
            description: Some("Says hello"),
            license: Some("MIT"),
            os_release: Version::new(0, 28, 0),
            contract: Sha256Digest::of(b"name = \"hello\""),
            artifacts,
        }
    }

    fn problems(m: &Manifest) -> Vec<ProblemKind> {
        let mut out = Vec::new();
        m.validate(|p| out.push(p.kind));
        out
    }

    #[test]
    fn a_good_manifest_has_no_problems() {
        let a = artifacts();
        assert_eq!(problems(&manifest(&a)), []);
        assert!(manifest(&a).validate(|_| {}));
        assert_eq!(manifest(&a).artifact(Arch::Riscv64).unwrap().size, 4100);
        assert!(manifest(&a).artifact(Arch::Armv7).is_none());
    }

    #[test]
    fn every_problem_is_reported_in_one_pass() {
        let mut a = artifacts().to_vec();
        a.reverse(); // out of order
        a[0].size = 0; // riscv64 empty
        a.push(Artifact {
            arch: Arch::X86_64,
            size: MAX_ARTIFACT_SIZE + 1,
            digest: Sha256Digest::of(b""),
        });
        let mut m = manifest(&a);
        m.format = 2;
        m.description = Some("line one\nline two");
        let found = problems(&m);
        assert_eq!(
            found,
            [
                ProblemKind::UnknownFormat(2),
                ProblemKind::BadText,
                ProblemKind::EmptyArtifact(Arch::Riscv64),
                ProblemKind::ArtifactOrder,
                ProblemKind::DuplicateArch(Arch::X86_64),
                ProblemKind::ArtifactTooLarge(Arch::X86_64),
            ]
        );
        assert!(!m.validate(|_| {}));
    }

    #[test]
    fn no_artifacts_is_refused() {
        assert_eq!(problems(&manifest(&[])), [ProblemKind::NoArtifacts]);
    }

    #[test]
    fn the_contract_must_agree_on_name_and_version() {
        let a = artifacts();
        let m = manifest(&a);
        assert!(m.validate_against_contract("hello", "1.0.0", |_| panic!()));
        let mut found = Vec::new();
        assert!(!m.validate_against_contract("hullo", "1.0.1", |p| found.push(p.kind)));
        assert_eq!(
            found,
            [
                ProblemKind::NameDiffersFromContract,
                ProblemKind::VersionDiffersFromContract
            ]
        );
    }
}
