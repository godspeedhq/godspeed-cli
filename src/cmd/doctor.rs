//! `godspeed doctor`: check the developer's environment and say what to fix.
//!
//! Every check is run and reported, even after one fails, so one run shows the whole picture. Required
//! items decide the exit status; optional ones (QEMU, needed only once `run` exists) are reported but do
//! not fail it.

use super::installed_targets;
use crate::diag::{Diagnostic, Kind, Stage};
use crate::platform::{Platform, Want};
use crate::target::{self, Arch};
use std::path::{Path, PathBuf};

struct Line {
    what: String,
    ok: bool,
    /// Reported but never fails the check: absent is "not found", not MISSING.
    optional: bool,
    detail: String,
    fix: Option<String>,
}

pub fn run(os_flag: Option<&Path>, json: bool) -> Result<(), Diagnostic> {
    let mut lines = Vec::new();
    let mut required_failed = false;

    // This CLI itself: asked directly, since `doctor` is where someone looks. Never fails the check.
    let current = crate::update::current();
    let latest = if std::env::var_os(crate::update::OFF_ENV).is_some_and(|v| !v.is_empty() && v != "0") {
        Err("not checked: ".to_string() + crate::update::OFF_ENV + " is set")
    } else {
        crate::update::latest_release().map_err(|()| "could not ask GitHub for the latest release".to_string())
    };
    lines.push(Line {
        what: "godspeed CLI".into(),
        ok: true,
        optional: true,
        detail: match latest {
            Ok(Some(l)) if l > current => format!("{current}; {l} is available - run `godspeed self update`"),
            Ok(_) => format!("{current}; no newer release is published"),
            Err(why) => format!("{current} ({why})"),
        },
        fix: None,
    });

    // In a project, the release it is built against; elsewhere, the latest.
    let project = std::env::current_dir()
        .ok()
        .and_then(|d| crate::project::Project::find(&d).ok());
    let want = project.as_ref().map_or(Want::Latest, Want::Project);
    let platform = match Platform::locate(os_flag, want) {
        Ok(p) => {
            lines.push(Line {
                what: "GodspeedOS".into(),
                ok: true,
                optional: false,
                detail: p.describe(),
                fix: None,
            });
            Some(p)
        }
        Err(d) => {
            required_failed = true;
            lines.push(Line {
                what: "GodspeedOS".into(),
                ok: false,
                optional: false,
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
                    optional: false,
                    detail: p.toolchain.clone(),
                    fix: None,
                });
                for a in target::ARCHES.iter().filter(|a| p.supports(a.triple)) {
                    let ok = installed.iter().any(|t| t == a.triple);
                    required_failed |= !ok;
                    lines.push(Line {
                        what: format!("target {}", a.name),
                        ok,
                        optional: false,
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
                    optional: false,
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
                optional: true,
                detail: found
                    .map(|f| f.display().to_string())
                    .unwrap_or_else(|| format!("{} not found", a.qemu)),
                fix: None,
            });
        }
    }

    if json {
        let checks: Vec<_> = lines
            .iter()
            .map(|l| {
                serde_json::json!({
                    "name": l.what,
                    "status": status(l),
                    "required": !l.optional,
                    "detail": l.detail,
                    "fix": l.fix,
                })
            })
            .collect();
        println!(
            "{:#}",
            serde_json::json!({ "ready": !required_failed, "checks": checks })
        );
    }
    for l in lines.iter().filter(|_| !json) {
        println!(
            "{:<26} {:<8} {}",
            l.what,
            match status(l) {
                "missing" => "MISSING",
                s => s,
            },
            l.detail
        );
        if let Some(fix) = &l.fix {
            for f in fix.lines() {
                println!("{:<26} {:<8} {}", "", "fix:", f);
            }
        }
    }
    if !json {
        println!();
        println!("QEMU is only needed by `godspeed run`, which GodspeedOS cannot support yet.");
    }

    if required_failed {
        Err(Diagnostic::new(
            Kind::Environment,
            Stage::Setup,
            "confirm the development environment",
            "At least one required item is missing (marked MISSING above).",
        )
        .next("Apply each `fix:` line, then run `godspeed doctor` again."))
    } else {
        if !json {
            println!("Ready to build.");
        }
        Ok(())
    }
}

/// "ok", "absent" (optional and not there) or "missing" (required and not there).
fn status(l: &Line) -> &'static str {
    match (l.ok, l.optional) {
        (true, _) => "ok",
        (false, true) => "absent",
        (false, false) => "missing",
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
