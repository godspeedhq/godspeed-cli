//! Diagnostics: every refusal says what failed, in which stage, why, what was expected, and what to do
//! next.
//!
//! A `Diagnostic` is the only way a command reports failure. Its `Kind` decides the process exit code,
//! so a script or CI job can tell an invalid project from a missing toolchain from an operation the OS
//! cannot do yet, without parsing text. Kinds are never merged to simplify a caller.

use std::fmt;

/// The stage an operation failed in. Shown on every diagnostic so the developer knows how far it got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Setup,
    Validation,
    Dependencies,
    Compilation,
    Testing,
    Packaging,
    Verification,
    Authentication,
    Upload,
    Finalisation,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Setup => "setup",
            Stage::Validation => "validation",
            Stage::Dependencies => "dependencies",
            Stage::Compilation => "compilation",
            Stage::Testing => "testing",
            Stage::Packaging => "packaging",
            Stage::Verification => "verification",
            Stage::Authentication => "authentication",
            Stage::Upload => "upload",
            Stage::Finalisation => "finalisation",
        }
    }
}

/// What class of failure this is. Each kind has its own exit code (see `exit_code`), and the codes are
/// part of the CLI's contract with scripts: they change only with a documented migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The project on disk is not a valid Godspeed project, or its files disagree with each other.
    InvalidProject,
    /// The developer's environment lacks something: the OS checkout, a toolchain, a target.
    Environment,
    /// The requested target or architecture is not one this OS version supports.
    UnavailableTarget,
    /// Fetching a dependency failed (network, registry, git).
    MissingDependency,
    /// The compiler ran and rejected the code.
    CompilationFailure,
    /// The tests ran and at least one failed.
    TestFailure,
    /// A package is malformed: a missing or extra file, a manifest that breaks the format's rules.
    InvalidPackage,
    /// A package's bytes are not the ones its manifest names: a size or checksum mismatch.
    IntegrityFailure,
    /// A signature is malformed, or does not verify: the named key did not sign this manifest.
    SignatureFailure,
    /// The registry's signed trust metadata does not vouch for this: an unknown, retired or revoked key, or
    /// metadata that is unsigned, expired, rolled back or inconsistent.
    Untrusted,
    /// The registry has no such package, or no such version of it.
    NotFound,
    /// The registry does not accept these credentials: none, unknown, revoked or expired.
    AuthenticationFailure,
    /// The registry could not be reached, or failed, before anything was decided.
    RegistryUnavailable,
    /// The credentials are valid but do not allow this: publishing under someone else's namespace.
    PermissionDenied,
    /// The registry refused because of what already exists: the version is published, or being published.
    Conflict,
    /// Nobody can tell yet whether the operation happened. Never a failure, never a success.
    OutcomeUnknown,
    /// The developer declined, or could not be asked to confirm, an operation that needs confirming.
    Cancelled,
    /// GodspeedOS does not provide what this operation needs yet. Not a bug in the project.
    NotProvidedByOs,
    /// The CLI does not implement this operation yet.
    NotImplemented,
    /// The operation belongs to another program (the native package manager on a Godspeed machine).
    WrongProgram,
    /// Reading or writing a local file failed.
    Io,
}

impl Kind {
    pub fn exit_code(self) -> i32 {
        match self {
            Kind::InvalidProject => 10,
            Kind::Environment => 11,
            Kind::UnavailableTarget => 12,
            Kind::MissingDependency => 13,
            Kind::CompilationFailure => 14,
            Kind::TestFailure => 15,
            Kind::InvalidPackage => 16,
            Kind::IntegrityFailure => 17,
            Kind::AuthenticationFailure => 18,
            Kind::RegistryUnavailable => 19,
            Kind::NotProvidedByOs => 20,
            Kind::NotImplemented => 21,
            Kind::WrongProgram => 22,
            Kind::PermissionDenied => 23,
            Kind::Conflict => 24,
            Kind::OutcomeUnknown => 25,
            Kind::SignatureFailure => 26,
            Kind::Untrusted => 27,
            Kind::NotFound => 28,
            Kind::Cancelled => 29,
            Kind::Io => 30,
        }
    }

    /// The kind's name in JSON output. Part of the same contract as the exit codes.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::InvalidProject => "invalid-project",
            Kind::Environment => "environment",
            Kind::UnavailableTarget => "unavailable-target",
            Kind::MissingDependency => "missing-dependency",
            Kind::CompilationFailure => "compilation-failure",
            Kind::TestFailure => "test-failure",
            Kind::InvalidPackage => "invalid-package",
            Kind::IntegrityFailure => "integrity-failure",
            Kind::SignatureFailure => "signature-failure",
            Kind::Untrusted => "untrusted",
            Kind::NotFound => "not-found",
            Kind::AuthenticationFailure => "authentication-failure",
            Kind::RegistryUnavailable => "registry-unavailable",
            Kind::PermissionDenied => "permission-denied",
            Kind::Conflict => "conflict",
            Kind::OutcomeUnknown => "outcome-unknown",
            Kind::Cancelled => "cancelled",
            Kind::NotProvidedByOs => "not-provided-by-os",
            Kind::NotImplemented => "not-implemented",
            Kind::WrongProgram => "wrong-program",
            Kind::Io => "io",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub kind: Kind,
    pub stage: Stage,
    /// "Cannot <headline>": what the developer asked for that did not happen.
    pub headline: String,
    pub reason: String,
    pub expected: Option<String>,
    pub next: Option<String>,
}

impl Diagnostic {
    pub fn new(kind: Kind, stage: Stage, headline: impl Into<String>, reason: impl Into<String>) -> Self {
        Diagnostic {
            kind,
            stage,
            headline: headline.into(),
            reason: reason.into(),
            expected: None,
            next: None,
        }
    }

    pub fn expected(mut self, expected: impl Into<String>) -> Self {
        self.expected = Some(expected.into());
        self
    }

    pub fn next(mut self, next: impl Into<String>) -> Self {
        self.next = Some(next.into());
        self
    }

    pub fn io(headline: impl Into<String>, path: &std::path::Path, err: &std::io::Error) -> Self {
        Diagnostic::new(Kind::Io, Stage::Setup, headline, format!("{}: {}", path.display(), err))
    }

    /// For `--json`: every field, unwrapped, with the kind's name and exit code.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": self.kind.as_str(),
            "exit_code": self.kind.exit_code(),
            "stage": self.stage.as_str(),
            "headline": format!("Cannot {}", self.headline),
            "reason": self.reason,
            "expected": self.expected,
            "next": self.next,
        })
    }

    /// The same text as `Display`, with the headline in bold red and each label in bold, for a terminal.
    pub fn styled(&self) -> String {
        let headline = anstyle::Style::new()
            .bold()
            .fg_color(Some(anstyle::AnsiColor::Red.into()));
        let label = anstyle::Style::new().bold();
        let mut out = String::new();
        for (i, line) in self.to_string().lines().enumerate() {
            if i == 0 {
                out.push_str(&format!("{headline}{line}{headline:#}\n"));
            } else if let Some((name, rest)) = line.split_once(':').filter(|_| !line.starts_with(' ')) {
                out.push_str(&format!("{label}{name}:{label:#}{rest}\n"));
            } else {
                out.push_str(line);
                out.push('\n');
            }
        }
        out
    }
}

/// Body text is wrapped at this width (after the two-space indent), on word boundaries.
const WIDTH: usize = 96;

fn indented(f: &mut fmt::Formatter<'_>, label: &str, body: &str) -> fmt::Result {
    writeln!(f, "{label}:")?;
    for line in body.lines() {
        let mut current = String::new();
        for word in line.split(' ') {
            if !current.is_empty() && current.len() + 1 + word.len() > WIDTH {
                writeln!(f, "  {current}")?;
                current.clear();
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
        writeln!(f, "  {current}")?;
    }
    Ok(())
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Cannot {}", self.headline)?;
        writeln!(f, "Stage: {}", self.stage.as_str())?;
        indented(f, "Reason", &self.reason)?;
        if let Some(e) = &self.expected {
            indented(f, "Expected", e)?;
        }
        if let Some(n) = &self.next {
            indented(f, "Next", n)?;
        }
        Ok(())
    }
}

/// A command either succeeds or returns every problem it found. Most commands stop at the first; `check`
/// collects all of them so one run shows everything to fix.
pub type Result<T> = std::result::Result<T, Vec<Diagnostic>>;

impl From<Diagnostic> for Vec<Diagnostic> {
    fn from(d: Diagnostic) -> Self {
        vec![d]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_every_section_in_order() {
        let d = Diagnostic::new(
            Kind::Environment,
            Stage::Compilation,
            "build hello for riscv64",
            "The riscv64 target is not installed.",
        )
        .expected("riscv64imac-unknown-none-elf for nightly-2026-05-16")
        .next("Run `godspeed doctor` for setup guidance.");
        let text = d.to_string();
        assert_eq!(
            text,
            "Cannot build hello for riscv64\n\
                          Stage: compilation\n\
                          Reason:\n  The riscv64 target is not installed.\n\
                          Expected:\n  riscv64imac-unknown-none-elf for nightly-2026-05-16\n\
                          Next:\n  Run `godspeed doctor` for setup guidance.\n"
        );
    }

    #[test]
    fn styling_changes_only_the_look() {
        let d = Diagnostic::new(Kind::Io, Stage::Setup, "x", "a: b").next("c");
        let styled = d.styled();
        assert!(styled.contains('\u{1b}'));
        let mut plain = String::new();
        let mut chars = styled.chars();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                chars.by_ref().find(|&c| c == 'm');
            } else {
                plain.push(c);
            }
        }
        assert_eq!(plain, d.to_string());
    }

    #[test]
    fn json_carries_every_field() {
        let d = Diagnostic::new(Kind::NotFound, Stage::Verification, "fetch a/b", "No such package.");
        let j = d.to_json();
        assert_eq!(j["kind"], "not-found");
        assert_eq!(j["exit_code"], 28);
        assert_eq!(j["headline"], "Cannot fetch a/b");
        assert!(j["next"].is_null());
    }

    #[test]
    fn long_lines_wrap_on_words() {
        let reason = "word ".repeat(40);
        let d = Diagnostic::new(Kind::Io, Stage::Setup, "x", reason.trim());
        for line in d.to_string().lines() {
            assert!(line.len() <= WIDTH + 2, "{line:?}");
        }
        assert_eq!(d.to_string().matches("word").count(), 40);
    }

    #[test]
    fn exit_codes_are_distinct() {
        let kinds = [
            Kind::InvalidProject,
            Kind::Environment,
            Kind::UnavailableTarget,
            Kind::MissingDependency,
            Kind::CompilationFailure,
            Kind::TestFailure,
            Kind::InvalidPackage,
            Kind::IntegrityFailure,
            Kind::SignatureFailure,
            Kind::Untrusted,
            Kind::AuthenticationFailure,
            Kind::RegistryUnavailable,
            Kind::PermissionDenied,
            Kind::Conflict,
            Kind::OutcomeUnknown,
            Kind::Cancelled,
            Kind::NotFound,
            Kind::NotProvidedByOs,
            Kind::NotImplemented,
            Kind::WrongProgram,
            Kind::Io,
        ];
        let mut codes: Vec<i32> = kinds.iter().map(|k| k.exit_code()).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), kinds.len());
        let mut names: Vec<&str> = kinds.iter().map(|k| k.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), kinds.len());
        assert!(!codes.contains(&0) && !codes.contains(&1) && !codes.contains(&2));
    }
}
