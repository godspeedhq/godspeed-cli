//! `godspeed`: the developer CLI for GodspeedOS.
//!
//! It creates, checks and builds Godspeed projects on the developer's workstation, against the public
//! contracts of a named GodspeedOS release. It is not the OS, not a supervisor, and not the native
//! package manager; where a command needs something GodspeedOS does not provide yet, it says so.

mod cmd;
mod diag;
mod platform;
mod project;
mod target;

use clap::{Parser, Subcommand};
use diag::Diagnostic;
use platform::Platform;
use project::Project;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "godspeed", version, about = "Create, check and build GodspeedOS projects")]
struct Cli {
    /// The GodspeedOS checkout to build against. Defaults to the GODSPEED_OS environment variable.
    #[arg(long, global = true, value_name = "PATH")]
    os: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new project: `service` or `library`
    New {
        /// service | library
        kind: String,
        /// The project's name, as the OS contract schema allows it
        name: String,
        /// Create it here instead of in ./<name>
        #[arg(long, value_name = "PATH")]
        dir: Option<PathBuf>,
    },
    /// Validate the project, its contract and its toolchain, without building
    Check,
    /// Build the project for one architecture (default x86_64) or all of them
    Build {
        /// x86_64 | aarch64 | armv7 | riscv64
        #[arg(long, conflicts_with = "all")]
        arch: Option<String>,
        /// Every architecture the OS supports
        #[arg(long)]
        all: bool,
    },
    /// Run the project's tests
    Test,
    /// Run the project on a target (not available yet)
    Run {
        /// qemu | hardware
        #[arg(long, default_value = "qemu")]
        target: String,
    },
    /// Build every architecture and assemble a verified package directory
    Package {
        /// Package one architecture only (default: every architecture the OS supports)
        #[arg(long, value_name = "ISA")]
        arch: Option<String>,
        /// Write the package here (default: target/package/<publisher>-<name>-<version>)
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
    },
    /// Verify a package directory against its manifest (needs no OS checkout)
    Verify {
        /// The package directory
        path: PathBuf,
    },
    /// Publish the package to a registry (not available yet)
    Publish,
    /// Sign in as a publisher (not available yet)
    Login,
    /// Sign out (not available yet)
    Logout,
    /// Show the signed-in publisher (not available yet)
    Whoami,
    /// Check the development environment: OS checkout, toolchain, targets, QEMU
    Doctor,

    // The native package manager's verbs. Hidden: they are answered only so that typing one here gets an
    // explanation instead of "unrecognised subcommand".
    #[command(hide = true)]
    Search {
        #[arg(trailing_var_arg = true)]
        _rest: Vec<String>,
    },
    #[command(hide = true)]
    Info {
        #[arg(trailing_var_arg = true)]
        _rest: Vec<String>,
    },
    #[command(hide = true)]
    Install {
        #[arg(trailing_var_arg = true)]
        _rest: Vec<String>,
    },
    #[command(hide = true)]
    List {
        #[arg(trailing_var_arg = true)]
        _rest: Vec<String>,
    },
    #[command(hide = true)]
    Update {
        #[arg(trailing_var_arg = true)]
        _rest: Vec<String>,
    },
    #[command(hide = true)]
    Uninstall {
        #[arg(trailing_var_arg = true)]
        _rest: Vec<String>,
    },
}

fn current_project() -> Result<Project, Diagnostic> {
    let cwd = std::env::current_dir().map_err(|e| Diagnostic::io("find the project", Path::new("."), &e))?;
    Project::find(&cwd)
}

fn dispatch(cli: Cli) -> diag::Result<()> {
    let os = cli.os.as_deref();
    match cli.command {
        Command::New { kind, name, dir } => {
            let platform = Platform::locate(os)?;
            let root = cmd::new::run(&platform, &kind, &name, dir.as_deref())?;
            println!(
                "Created {kind} {name} in {} (GodspeedOS {})",
                root.display(),
                platform.tag()
            );
            println!("Next:");
            println!("  cd {}", root.display());
            println!("  godspeed check");
            println!("  godspeed build");
            Ok(())
        }
        Command::Check => {
            let platform = Platform::locate(os)?;
            let project = current_project()?;
            cmd::check::run(&platform, &project)?;
            println!(
                "{} {} is valid against GodspeedOS {}.",
                project.kind.as_str(),
                project.cargo_name().unwrap_or("?"),
                platform.tag()
            );
            Ok(())
        }
        Command::Build { arch, all } => {
            let platform = Platform::locate(os)?;
            let project = current_project()?;
            let arches = cmd::build::select(&platform, arch.as_deref(), all)?;
            let built = cmd::build::run(&platform, &project, &arches)?;
            for b in &built {
                println!("Built {} {}", b.arch.name, b.artifact.display());
            }
            if project.kind == project::ProjectKind::Service {
                println!("These are service binaries, not an OS image; nothing was installed or run.");
            }
            Ok(())
        }
        Command::Test => {
            let platform = Platform::locate(os)?;
            let project = current_project()?;
            cmd::test::run(&platform, &project)?;
            println!("All tests passed.");
            Ok(())
        }
        Command::Run { target } => match target.as_str() {
            "qemu" | "hardware" => Err(cmd::unavailable::run_service("run").into()),
            other => Err(Diagnostic::new(
                diag::Kind::UnavailableTarget,
                diag::Stage::Validation,
                format!("run on \"{other}\""),
                "There is no such run target.",
            )
            .expected("qemu or hardware")
            .into()),
        },
        Command::Package { arch, out } => {
            let platform = Platform::locate(os)?;
            let project = current_project()?;
            let arches = cmd::build::select(&platform, arch.as_deref(), arch.is_none())?;
            let dir = cmd::package::run(&platform, &project, &arches, out.as_deref())?;
            let verified = cmd::verify::run(&dir)?;
            cmd::verify::report(&verified);
            println!("Packaged in {}", dir.display());
            Ok(())
        }
        Command::Verify { path } => {
            let verified = cmd::verify::run(&path)?;
            cmd::verify::report(&verified);
            Ok(())
        }
        Command::Publish => Err(cmd::unavailable::registry("publish").into()),
        Command::Login => Err(cmd::unavailable::registry("sign in").into()),
        Command::Logout => Err(cmd::unavailable::registry("sign out").into()),
        Command::Whoami => Err(cmd::unavailable::registry("show the signed-in publisher").into()),
        Command::Doctor => cmd::doctor::run(os).map_err(Into::into),
        Command::Search { .. } => Err(cmd::unavailable::native("search").into()),
        Command::Info { .. } => Err(cmd::unavailable::native("info").into()),
        Command::Install { .. } => Err(cmd::unavailable::native("install").into()),
        Command::List { .. } => Err(cmd::unavailable::native("list").into()),
        Command::Update { .. } => Err(cmd::unavailable::native("update").into()),
        Command::Uninstall { .. } => Err(cmd::unavailable::native("uninstall").into()),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match dispatch(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(diagnostics) => {
            let count = diagnostics.len();
            for (i, d) in diagnostics.iter().enumerate() {
                if i > 0 {
                    eprintln!();
                }
                eprint!("{d}");
            }
            if count > 1 {
                eprintln!();
                eprintln!("{count} problems found.");
            }
            // The first diagnostic's kind decides the exit code; `check` only ever returns one kind.
            let code = diagnostics.first().map(|d| d.kind.exit_code()).unwrap_or(1);
            ExitCode::from(code as u8)
        }
    }
}
