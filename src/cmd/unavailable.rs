//! Commands the contract names that cannot work yet, and commands that belong to another program.
//!
//! Each answers with what is missing, whose it is to build, and what to do meanwhile. "Not provided by
//! the OS" and "not implemented in this CLI" are different exit codes, because they call for different
//! responses: one is waiting on GodspeedOS, the other on this tool.

use crate::diag::{Diagnostic, Kind, Stage};

/// `run` and `test` for a service: blocked on the OS.
pub fn run_service(verb: &str) -> Diagnostic {
    Diagnostic::new(
        Kind::NotProvidedByOs,
        Stage::Setup,
        format!("{verb} a service"),
        "GodspeedOS runs only services compiled into its own image, and has no public way yet to add a service \
         built outside its source tree. Doing it anyway would mean this CLI editing the OS's supervisor \
         source, which it does not do.",
    )
    .expected("an OS build interface that takes a service crate from outside the tree")
    .next(
        "Build and check the service now with `godspeed build` and `godspeed check`. To run it today, \
               add it to a GodspeedOS checkout by hand as GETTING_STARTED.md in the OS describes.",
    )
}

/// Verbs of the native package manager, typed on a workstation.
pub fn native(verb: &str) -> Diagnostic {
    Diagnostic::new(
        Kind::WrongProgram,
        Stage::Setup,
        format!("{verb} here"),
        format!(
            "`godspeed {verb}` is a command of the native package manager, which runs on a Godspeed \
                 machine. This is the developer CLI: it creates, builds, tests, packages and publishes."
        ),
    )
    .next(format!(
        "Run it on the Godspeed machine, at its `gsh>` prompt: `godspeed {verb}` there.{}",
        match verb {
            "update" | "uninstall" => format!(" To {verb} this CLI itself: `godspeed self {verb}`."),
            _ => String::new(),
        }
    ))
}
