//! `godspeed doctor`: check the developer's environment and say what to fix.
//!
//! Every check is run and reported, even after one fails, so one run shows the whole picture. Required
//! items decide the exit status; optional ones (QEMU, needed only once `run` exists) are reported but do
//! not fail it.

use super::installed_targets;
use crate::diag::{Diagnostic, Kind, Stage};
use crate::platform::Platform;
use crate::target::{self, Arch};
use std::path::{Path, PathBuf};

struct Line {
    what: String,
    ok: bool,
    detail: String,
    fix: Option<String>,
}

pub fn run(os_flag: Option<&Path>) -> Result<(), Diagnostic> {
    let mut lines = Vec::new();
    let mut required_failed = false;

    let platform = match Platform::locate(os_flag) {
        Ok(p) => {
            lines.push(Line {
                what: "GodspeedOS checkout".into(),
                ok: true,
                detail: format!("{} ({})", p.root.display(), p.version),
                fix: None,
            });
            Some(p)
        }
        Err(d) => {
            required_failed = true;
            lines.push(Line {
                what: "GodspeedOS checkout".into(),
                ok: false,
                detail: d.reason.clone(),
                fix: d.next.clone(),
            });
            None
        }
    };

    if let Some(p) = &platform {
        match installed_targets(&p.toolchain) {
            Ok(installed) => {
                lines.push(Line {
                    what: "Rust toolchain".into(),
                    ok: true,
                    detail: p.toolchain.clone(),
                    fix: None,
                });
                for a in target::ARCHES.iter().filter(|a| p.supports(a.triple)) {
                    let ok = installed.iter().any(|t| t == a.triple);
                    required_failed |= !ok;
                    lines.push(Line {
                        what: format!("target {}", a.name),
                        ok,
                        detail: a.triple.into(),
                        fix: (!ok).then(|| format!("rustup target add --toolchain {} {}", p.toolchain, a.triple)),
                    });
                }
            }
            Err(d) => {
                required_failed = true;
                lines.push(Line {
                    what: "Rust toolchain".into(),
                    ok: false,
                    detail: d.reason.clone(),
                    fix: d.next.clone(),
                });
            }
        }
        for a in target::ARCHES.iter().filter(|a| p.supports(a.triple)) {
            let found = find_qemu(a);
            lines.push(Line {
                what: format!("QEMU {} (optional)", a.name),
                ok: found.is_some(),
                detail: found
                    .map(|f| f.display().to_string())
                    .unwrap_or_else(|| format!("{} not found", a.qemu)),
                fix: None,
            });
        }
    }

    for l in &lines {
        println!("{:<26} {:<8} {}", l.what, if l.ok { "ok" } else { "MISSING" }, l.detail);
        if let Some(fix) = &l.fix {
            for f in fix.lines() {
                println!("{:<26} {:<8} {}", "", "fix:", f);
            }
        }
    }
    println!();
    println!("QEMU is only needed by `godspeed run`, which GodspeedOS cannot support yet.");

    if required_failed {
        Err(Diagnostic::new(
            Kind::Environment,
            Stage::Setup,
            "confirm the development environment",
            "At least one required item is missing (marked MISSING above).",
        )
        .next("Apply each `fix:` line, then run `godspeed doctor` again."))
    } else {
        println!("Ready to build.");
        Ok(())
    }
}

/// Where the OS's own run scripts would find this emulator: its environment variable, then PATH, then
/// QEMU's default Windows install directory.
fn find_qemu(a: &Arch) -> Option<PathBuf> {
    if let Some(var) = a.qemu_env {
        if let Some(p) = std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_file()) {
            return Some(p);
        }
    }
    let exe = if cfg!(windows) {
        format!("{}.exe", a.qemu)
    } else {
        a.qemu.to_string()
    };
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let p = dir.join(&exe);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    if cfg!(windows) {
        let p = PathBuf::from(r"C:\Program Files\qemu").join(&exe);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}
