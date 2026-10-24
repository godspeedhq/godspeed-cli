//! Commands the contract names that cannot work yet, and commands that belong to another program.
//!
//! Each answers with what is missing, whose it is to build, and what to do meanwhile. "Not provided by
//! the OS" and "not implemented in this CLI" are different exit codes, because they call for different
//! responses: one is waiting on GodspeedOS, the other on this tool.

use crate::diag::{Diagnostic, Kind, Stage};

/// `run` and `test` for a program: blocked on the OS.
pub fn run_service(verb: &str) -> Diagnostic {
    Diagnostic::new(
        Kind::NotProvidedByOs,
        Stage::Setup,
        format!("{verb} a program"),
        "A program built outside the GodspeedOS tree runs only as a package installed on a Godspeed machine, and          there is no public way yet for this CLI to boot one - in QEMU or on hardware - with your program on it.          Doing it anyway would mean this CLI building the OS image itself, which it does not do.",
    )
    .expected("an OS interface that boots a machine with a package already installed")
    .next(
        "Build and check it now with `godspeed build` and `godspeed check`. To run it today, `godspeed package                --local` writes a `.gsp` file; on a Godspeed machine, trust your key and `godspeed install <file>.gsp`                at its `gsh>` prompt.",
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
    .next(match verb {
        // The machine has no `update`: a newer release of what is installed is `upgrade` there.
        "update" => "On the Godspeed machine, at its `gsh>` prompt, a newer release is `godspeed upgrade`, and \
                     `godspeed list updates` shows what has one. To update this CLI itself: `godspeed self update`."
            .to_string(),
        "upgrade" => "Run it on the Godspeed machine, at its `gsh>` prompt: `godspeed upgrade` there. To update \
                      this CLI itself: `godspeed self update`."
            .to_string(),
        "uninstall" => "Run it on the Godspeed machine, at its `gsh>` prompt: `godspeed uninstall` there. To \
                        uninstall this CLI itself: `godspeed self uninstall`."
            .to_string(),
        _ => format!("Run it on the Godspeed machine, at its `gsh>` prompt: `godspeed {verb}` there."),
    })
}
