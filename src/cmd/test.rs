//! `godspeed test`.
//!
//! A library's tests are plain Rust and run on the workstation. A service's cannot: the stdlib talks to
//! the kernel, so a service is tested by booting it - and GodspeedOS has no public way to boot a service
//! built outside its tree yet. That is said, not worked around.

use super::{cargo, check};
use crate::diag::{Diagnostic, Kind, Result, Stage};
use crate::platform::Platform;
use crate::project::Project;

pub fn run(platform: &Platform, project: &Project) -> Result<()> {
    if project.kind.runs() {
        return Err(super::unavailable::run_service("test").into());
    }

    eprintln!("Validating project...");
    check::run(platform, project)?;
    let name = project.cargo_name().unwrap_or("the project").to_string();

    // Compile first, separately, so a compiler error is never reported as a failing test.
    eprintln!("Compiling tests...");
    match cargo(platform, &project.root).args(["test", "--no-run"]).status() {
        Ok(s) if s.success() => {}
        Ok(_) => {
            return Err(Diagnostic::new(
                Kind::CompilationFailure,
                Stage::Compilation,
                format!("test {name}"),
                "The tests did not compile (errors above).",
            )
            .into())
        }
        Err(e) => {
            return Err(Diagnostic::new(
                Kind::Environment,
                Stage::Compilation,
                format!("test {name}"),
                format!("`cargo` could not be run: {e}"),
            )
            .into())
        }
    }

    eprintln!("Running tests...");
    match cargo(platform, &project.root).arg("test").status() {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => Err(Diagnostic::new(
            Kind::TestFailure,
            Stage::Testing,
            format!("pass the tests of {name}"),
            "At least one test failed (output above).",
        )
        .into()),
        Err(e) => Err(Diagnostic::new(
            Kind::Environment,
            Stage::Testing,
            format!("test {name}"),
            format!("`cargo` could not be run: {e}"),
        )
        .into()),
    }
}
