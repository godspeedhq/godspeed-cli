//! The package manifest: what one version of one package is.

use crate::arch::Arch;
use crate::digest::Sha256Digest;
use crate::name::{PackageName, FULL_MAX};
use crate::version::{Version, VersionReq};
use core::fmt;

/// The manifest format this crate reads and writes for a package with no requirements. A reader refuses any
/// value it does not know: a format change is a new number, never a silent reinterpretation (see
/// `docs/package-format.md`, compatibility).
pub const FORMAT: u32 = 1;

/// The format of a manifest WITH requirements (`[[requires]]`, GodspeedOS `docs/developer-services-design.md`
/// 16). Written only when there is one, so a package without stays readable by every reader of format 1, and a
/// reader that predates requirements refuses one that has them as a format it does not read - the true reason.
pub const FORMAT_REQUIRES: u32 = 2;

/// The most packages one manifest may require.
pub const MAX_REQUIRES: usize = 8;

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
    /// A managed component the OS spawns, with a service contract, and keeps running.
    Service,
    /// A program that does one job and ends, with a service contract: started by the shell for one command line,
    /// handed its arguments, and ended with a status (GodspeedOS `Exit`, its CLAUDE.md 14.5 amendment of
    /// 2026-10-17). A new value in format 1, not a new format: a reader that does not know it refuses the
    /// manifest by naming the kind, which is the refusal it already gives any kind it does not package.
    Utility,
    /// A utility that owns the screen and the keyboard while it runs (GodspeedOS `gs::app`, the console
    /// foreground). The operator approves the console at install; when it ends the shell has the console back.
    App,
    /// A service that drives one device: its contract names the device by PCI class code and register window
    /// (`hw_pci_class`, `hw_pci_bar`), never an address. The operator approves the device at install, and the
    /// OS resolves which device that is against its own bus scan.
    Driver,
}

impl Kind {
    /// The manifest spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Kind::Service => "service",
            Kind::Utility => "utility",
            Kind::App => "app",
            Kind::Driver => "driver",
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

/// A package this one needs installed and running, and the versions of it that will do. It grants nothing: the
/// contract's peers are still what the operator approves.
///
/// The name is held INLINE, at most [`FULL_MAX`] bytes, so a requirement borrows nothing: an owned manifest can
/// hand out its requirements, and a `no_std` reader can keep them in a fixed array.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Requirement {
    name: [u8; FULL_MAX],
    len: u8,
    /// The versions of it that will do.
    pub range: VersionReq,
}

impl Requirement {
    /// A requirement of `name`, in `range`.
    pub fn new(name: PackageName<'_>, range: VersionReq) -> Self {
        let mut buf = [0u8; FULL_MAX];
        let (p, n) = (name.publisher().as_bytes(), name.name().as_bytes());
        // A parsed name is at most FULL_MAX bytes whole, so this fits.
        let len = (p.len() + 1 + n.len()).min(FULL_MAX);
        buf[..p.len()].copy_from_slice(p);
        buf[p.len()] = b'/';
        buf[p.len() + 1..len].copy_from_slice(&n[..len - p.len() - 1]);
        Requirement {
            name: buf,
            len: len as u8,
            range,
        }
    }

    /// The package needed, `publisher/name`.
    pub fn name(&self) -> PackageName<'_> {
        let s = core::str::from_utf8(&self.name[..self.len as usize]).unwrap_or("");
        PackageName::parse(s).expect("a requirement is built from a parsed name")
    }
}

/// An unfilled slot, for a reader that keeps requirements in fixed storage (GodspeedOS's `godspeed-pm`): a valid
/// name, `x/x`, and the version 0.0.0, never shown - a reader hands out only the slots it filled.
impl Default for Requirement {
    fn default() -> Self {
        let mut name = [0u8; FULL_MAX];
        name[..3].copy_from_slice(b"x/x");
        Requirement {
            name,
            len: 3,
            range: VersionReq::Exact(Version::new(0, 0, 0)),
        }
    }
}

impl fmt::Debug for Requirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.name(), self.range)
    }
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
    /// The packages this one needs, at most [`MAX_REQUIRES`]; empty for most. Non-empty means format
    /// [`FORMAT_REQUIRES`].
    pub requires: &'a [Requirement],
}

/// Which rule a manifest broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProblemKind {
    /// A required field is absent.
    Missing,
    /// A field's value has the wrong form; the `&str` says what form was expected.
    Malformed(&'static str),
    /// The file is a format this reader does not know.
    UnknownFormat {
        /// The format the file declares.
        found: u32,
        /// The format this reader reads.
        known: u32,
    },
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
    /// An index lists the same version twice.
    DuplicateVersion(Version),
    /// A timestamp is not `YYYY-MM-DDTHH:MM:SSZ`.
    BadTimestamp,
    /// An index's releases are not in publication order.
    TimestampOrder,
    /// The format and the requirements disagree: format 1 with requirements, or format 2 without.
    FormatForRequirements,
    /// More than [`MAX_REQUIRES`] requirements.
    TooManyRequirements,
    /// The same package required twice.
    DuplicateRequirement,
    /// A package that requires itself.
    RequiresItself,
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
            ProblemKind::UnknownFormat { found, known } => {
                if known > 1 {
                    write!(
                        f,
                        "{field}: this is format {found}; this reader knows formats 1 to {known}"
                    )
                } else {
                    write!(f, "{field}: this is format {found}; this reader knows format {known}")
                }
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
            ProblemKind::DuplicateVersion(v) => write!(f, "{field}: version {v} is listed twice"),
            ProblemKind::BadTimestamp => write!(f, "{field}: a timestamp is YYYY-MM-DDTHH:MM:SSZ, in UTC"),
            ProblemKind::TimestampOrder => write!(f, "{field}: releases are listed in the order they were published"),
            ProblemKind::FormatForRequirements => write!(
                f,
                "{field}: a manifest with requirements is format {FORMAT_REQUIRES}, and one without is format {FORMAT}"
            ),
            ProblemKind::TooManyRequirements => write!(f, "{field}: at most {MAX_REQUIRES} packages may be required"),
            ProblemKind::DuplicateRequirement => write!(f, "{field}: a package is required twice"),
            ProblemKind::RequiresItself => write!(f, "{field}: a package cannot require itself"),
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

        if self.format != FORMAT && self.format != FORMAT_REQUIRES {
            bad(
                "format",
                ProblemKind::UnknownFormat {
                    found: self.format,
                    known: FORMAT_REQUIRES,
                },
            );
        } else if (self.format == FORMAT_REQUIRES) == self.requires.is_empty() {
            bad("format", ProblemKind::FormatForRequirements);
        }
        if self.requires.len() > MAX_REQUIRES {
            bad("requires", ProblemKind::TooManyRequirements);
        }
        for (i, r) in self.requires.iter().enumerate() {
            if r.name() == self.name {
                bad("requires.name", ProblemKind::RequiresItself);
            } else if self.requires[..i].iter().any(|q| q.name() == r.name()) {
                bad("requires.name", ProblemKind::DuplicateRequirement);
            }
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
            requires: &[],
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
        m.format = 3;
        m.description = Some("line one\nline two");
        let found = problems(&m);
        assert_eq!(
            found,
            [
                ProblemKind::UnknownFormat {
                    found: 3,
                    known: FORMAT_REQUIRES
                },
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

    fn req(name: &str, range: &str) -> Requirement {
        Requirement::new(PackageName::parse(name).unwrap(), range.parse().unwrap())
    }

    #[test]
    fn a_requirement_holds_its_name_whole() {
        let r = req("alice/notes-store", "^1.2.0");
        assert_eq!(std::format!("{}", r.name()), "alice/notes-store");
        assert_eq!(r.name().publisher(), "alice");
        assert_eq!(std::format!("{r:?}"), "alice/notes-store ^1.2.0");
        let long = std::format!("{}/{}", "p".repeat(63), "n".repeat(64));
        assert_eq!(std::format!("{}", req(&long, "=1.0.0").name()), long);
    }

    #[test]
    fn requirements_need_format_2_and_format_2_needs_requirements() {
        let a = artifacts();
        let r = [req("alice/store", "^1.2.0")];
        let mut m = manifest(&a);
        m.requires = &r;
        assert_eq!(problems(&m), [ProblemKind::FormatForRequirements]);
        m.format = FORMAT_REQUIRES;
        assert_eq!(problems(&m), []);
        m.requires = &[];
        assert_eq!(problems(&m), [ProblemKind::FormatForRequirements]);
    }

    #[test]
    fn requirements_are_bounded_unique_and_never_the_package_itself() {
        let a = artifacts();
        let r = [
            req("alice/store", "^1.2.0"),
            req("alice/store", "=1.3.0"),
            req("alice/hello", ">=1.0.0"),
        ];
        let mut m = manifest(&a);
        m.format = FORMAT_REQUIRES;
        m.requires = &r;
        assert_eq!(
            problems(&m),
            [ProblemKind::DuplicateRequirement, ProblemKind::RequiresItself]
        );
        const NAMES: [&str; MAX_REQUIRES + 1] = [
            "alice/p0", "alice/p1", "alice/p2", "alice/p3", "alice/p4", "alice/p5", "alice/p6", "alice/p7", "alice/p8",
        ];
        let many: Vec<_> = NAMES.iter().map(|n| req(n, "^1.0.0")).collect();
        m.requires = &many;
        assert_eq!(problems(&m), [ProblemKind::TooManyRequirements]);
    }
}
