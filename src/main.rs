//! `godspeed`: the developer CLI for GodspeedOS.
//!
//! It creates, checks and builds Godspeed projects on the developer's workstation, against the public
//! contracts of a named GodspeedOS release. It is not the OS, not a supervisor, and not the native
//! package manager; where a command needs something GodspeedOS does not provide yet, it says so.

// Nothing in the CLI needs `unsafe`, and `forbid` (unlike `deny`) cannot be switched off further in.
#![forbid(unsafe_code)]

mod cmd;
mod diag;
mod kit;
mod platform;
mod project;
mod registry;
mod signing;
mod target;
mod update;

use clap::{Parser, Subcommand};
use diag::{Diagnostic, Kind, Stage};
use platform::{Platform, Want};
use project::Project;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "godspeed",
    version,
    about = "Create, build, package and publish GodspeedOS software, and verify anyone's"
)]
struct Cli {
    /// A GodspeedOS checkout to build against, instead of the cached platform kit. Defaults to GODSPEED_OS.
    #[arg(long, global = true, value_name = "PATH")]
    os: Option<PathBuf>,

    /// The package registry to publish to. Defaults to GODSPEED_REGISTRY, then the last `login`.
    #[arg(long, global = true, value_name = "URL")]
    registry: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new project: `service`, `utility` or `library`
    New {
        /// service | library
        kind: String,
        /// The project's name, as the OS contract schema allows it
        name: String,
        /// Create it here instead of in ./<name>
        #[arg(long, value_name = "PATH")]
        dir: Option<PathBuf>,
        /// The namespace a package of it is published under (a service's `publisher` in godspeed.toml)
        #[arg(long, value_name = "NAME")]
        publisher: Option<String>,
        /// The GodspeedOS release to build against (default: the latest)
        #[arg(long, value_name = "VERSION")]
        os_release: Option<String>,
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
        /// Also sign it with your key and write it as one file, <name>-<version>.gsp here, for a machine whose
        /// operator trusts your key (`godspeed trust add` there): no registry and no network on either side
        #[arg(long)]
        local: bool,
        /// Print the result as JSON
        #[arg(long)]
        json: bool,
    },
    /// Sign a package directory with your signing key (`publish` signs for you)
    Sign {
        /// The package directory
        path: PathBuf,
    },
    /// Download a published release and verify it through the registry's whole chain of trust
    Fetch {
        /// publisher/name, optionally @version (default: the highest published)
        package: String,
        /// Write it here (default: ./<publisher>-<name>-<version>; with --offline, the folder for its file)
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
        /// On first contact with a registry, trust its root 1 only if it has this digest (sha256:...)
        #[arg(long, value_name = "DIGEST")]
        trust_root: Option<String>,
        /// Write it as one file, <name>-<version>.gsp, that a machine with no network installs
        #[arg(long)]
        offline: bool,
        /// With --offline: the binary to carry - x86_64 | aarch64 | armv7 | riscv64 | all (default all)
        #[arg(long, value_name = "ARCH", requires = "offline")]
        arch: Option<String>,
        /// Print the result as JSON
        #[arg(long)]
        json: bool,
    },
    /// Make or show your signing key
    Key {
        #[command(subcommand)]
        action: KeyAction,
    },
    /// Check a release as installing it would, and keep nothing: a `.gsp` file offline, or a release on the registry
    Inspect {
        /// <file>.gsp, or publisher/name optionally @version
        target: String,
        /// On first contact with a registry, trust its root 1 only if it has this digest (sha256:...)
        #[arg(long, value_name = "DIGEST")]
        trust_root: Option<String>,
        /// Print the result as JSON
        #[arg(long)]
        json: bool,
    },
    /// Verify a package directory against its manifest (needs no OS checkout)
    Verify {
        /// The package directory
        path: PathBuf,
        /// Print the result as JSON
        #[arg(long)]
        json: bool,
    },
    /// Package the project and publish it to a registry
    Publish {
        /// Package, sign, authenticate and check the namespace and version, but upload nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Make a publisher token for a registry and print its fingerprint, for the maintainer to approve
    Login {
        /// Store an existing token instead (read from the terminal, or from standard input), checked first
        #[arg(long)]
        with_token: bool,
    },
    /// Publisher tokens for CI
    Token {
        #[command(subcommand)]
        action: TokenAction,
    },
    /// Revoke the token at the registry and remove it from this machine
    Logout,
    /// Show which account the token belongs to, and where it may publish
    Whoami {
        /// Print the result as JSON
        #[arg(long)]
        json: bool,
    },
    /// Check the development environment: OS checkout, toolchain, targets, QEMU
    Doctor {
        /// Print the result as JSON
        #[arg(long)]
        json: bool,
    },
    /// Print a shell completion script: bash, zsh, fish, powershell or elvish
    #[command(after_help = COMPLETIONS_HELP)]
    Completions { shell: clap_complete::Shell },
    /// Update or uninstall this CLI
    #[command(name = "self")]
    SelfManage {
        #[command(subcommand)]
        action: SelfAction,
    },

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

const COMPLETIONS_HELP: &str = "Load them in your shell's startup file:
  bash        echo 'eval \"$(godspeed completions bash)\"' >> ~/.bashrc
  zsh         echo 'eval \"$(godspeed completions zsh)\"' >> ~/.zshrc
  fish        godspeed completions fish > ~/.config/fish/completions/godspeed.fish
  PowerShell  Add-Content $PROFILE 'godspeed completions powershell | Out-String | Invoke-Expression'";

/// The command tree that completions describe: every command but the hidden ones (the native package
/// manager's verbs), which some shells' generators would otherwise offer.
fn completion_command() -> clap::Command {
    use clap::CommandFactory;
    let full = Cli::command();
    let mut tree = clap::Command::new("godspeed").version(env!("CARGO_PKG_VERSION"));
    for arg in full.get_arguments() {
        tree = tree.arg(arg.clone());
    }
    for sub in full.get_subcommands().filter(|c| !c.is_hide_set()) {
        tree = tree.subcommand(sub.clone());
    }
    tree
}

impl Command {
    /// Whether the command was asked for JSON: then its result is one JSON document on stdout, and a refusal
    /// is one on stderr.
    fn json(&self) -> bool {
        matches!(
            self,
            Command::Verify { json: true, .. }
                | Command::Whoami { json: true }
                | Command::Doctor { json: true }
                | Command::Package { json: true, .. }
                | Command::Fetch { json: true, .. }
                | Command::Inspect { json: true, .. }
        )
    }
}

#[derive(Subcommand)]
enum TokenAction {
    /// Make a token, print it once and store it nowhere (for a CI secret), with its fingerprint
    New,
}

#[derive(Subcommand)]
enum SelfAction {
    /// Download the latest release, verify it, and replace this binary with it
    Update,
    /// Remove this binary and the PATH entries the installer added; keep your keys and tokens
    Uninstall {
        /// Do not ask for confirmation
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum KeyAction {
    /// Make a new signing key (never replaces an existing one)
    Generate,
    /// Print your public key, to give to the registry maintainer
    Show,
}

fn current_project() -> Result<Project, Diagnostic> {
    let cwd = std::env::current_dir().map_err(|e| Diagnostic::io("find the project", Path::new("."), &e))?;
    Project::find(&cwd)
}

fn dispatch(cli: Cli) -> diag::Result<()> {
    let os = cli.os.as_deref();
    let reg = cli.registry.as_deref();
    match cli.command {
        Command::New {
            kind,
            name,
            dir,
            publisher,
            os_release,
        } => {
            let want = os_release.as_deref().map_or(Want::Latest, Want::Release);
            let platform = Platform::locate(os, want)?;
            let root = cmd::new::run(&platform, &kind, &name, dir.as_deref(), publisher.as_deref())?;
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
            let project = current_project()?;
            let platform = Platform::locate(os, Want::Project(&project))?;
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
            let project = current_project()?;
            let platform = Platform::locate(os, Want::Project(&project))?;
            let arches = cmd::build::select(&platform, arch.as_deref(), all)?;
            let built = cmd::build::run(&platform, &project, &arches)?;
            for b in &built {
                println!("Built {} {}", b.arch.name, b.artifact.display());
            }
            if project.kind.runs() {
                println!("These are program binaries, not an OS image; nothing was installed or run.");
            }
            Ok(())
        }
        Command::Test => {
            let project = current_project()?;
            let platform = Platform::locate(os, Want::Project(&project))?;
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
        Command::Package { arch, out, local, json } => {
            let project = current_project()?;
            let platform = Platform::locate(os, Want::Project(&project))?;
            let arches = cmd::build::select(&platform, arch.as_deref(), arch.is_none())?;
            let dir = cmd::package::run(&platform, &project, &arches, out.as_deref())?;
            // AN UNPUBLISHED PACKAGE FILE: signed here, with the key a machine's operator is asked to trust.
            let signer = if local {
                let key = signing::load()?;
                let public = cmd::sign::sign_dir(&dir, &key)?;
                Some((cmd::package::write_gsp(&dir, Path::new("."))?, public))
            } else {
                None
            };
            let verified = cmd::verify::run(&dir)?;
            if json {
                let mut doc = cmd::verify::to_json(&verified);
                doc["dir"] = dir.display().to_string().into();
                println!("{doc:#}");
            } else {
                cmd::verify::report(&verified);
                println!("Packaged in {}", dir.display());
                if let Some((file, public)) = signer {
                    let m = verified.manifest.manifest();
                    println!(
                        "Wrote {}@{} to {}, UNPUBLISHED: no registry vouches for it",
                        m.name,
                        m.version,
                        file.display()
                    );
                    println!("  on the machine, trust your key for {} once:", m.name.publisher());
                    println!("    godspeed trust add {} {public}", m.name.publisher());
                    println!(
                        "  then: godspeed install {}",
                        file.file_name()
                            .map(|f| f.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    );
                }
            }
            Ok(())
        }
        Command::Sign { path } => cmd::sign::run(&path),
        Command::Fetch {
            package,
            out,
            trust_root,
            offline,
            arch,
            json,
        } => {
            let offline = match offline {
                false => None,
                true => {
                    let arches = match arch.as_deref() {
                        None | Some("all") => cmd::fetch::Arches::All,
                        Some(a) => match a.parse::<godspeed_package::Arch>() {
                            Ok(a) => cmd::fetch::Arches::One(a.as_str().to_string()),
                            Err(_) => {
                                return Err(Diagnostic::new(
                                    Kind::InvalidProject,
                                    Stage::Validation,
                                    format!("fetch {package}"),
                                    format!("\"{a}\" is not an architecture."),
                                )
                                .expected("x86_64, aarch64, armv7, riscv64 or all")
                                .into())
                            }
                        },
                    };
                    Some(arches)
                }
            };
            cmd::fetch::run(
                &package,
                reg,
                out.as_deref(),
                trust_root.as_deref(),
                offline,
                None,
                json,
            )
        }
        Command::Key { action } => match action {
            KeyAction::Generate => cmd::sign::key_generate(),
            KeyAction::Show => cmd::sign::key_show(),
        },
        Command::Inspect {
            target,
            trust_root,
            json,
        } => {
            if target.ends_with(".gsp") {
                let how = cmd::fetch::Inspect::File(PathBuf::from(&target));
                cmd::fetch::run("", reg, None, trust_root.as_deref(), None, Some(&how), json)
            } else {
                let how = cmd::fetch::Inspect::Registry;
                cmd::fetch::run(&target, reg, None, trust_root.as_deref(), None, Some(&how), json)
            }
        }
        Command::Verify { path, json } => {
            let verified = cmd::verify::run(&path)?;
            if json {
                println!("{:#}", cmd::verify::to_json(&verified));
            } else {
                cmd::verify::report(&verified);
            }
            Ok(())
        }
        Command::Publish { dry_run } => {
            let project = current_project()?;
            let platform = Platform::locate(os, Want::Project(&project))?;
            cmd::publish::run(&platform, &project, reg, dry_run)
        }
        Command::Login { with_token } => cmd::account::login(reg, with_token),
        Command::Token {
            action: TokenAction::New,
        } => cmd::account::token_new(),
        Command::Logout => cmd::account::logout(reg),
        Command::Whoami { json } => cmd::account::whoami(reg, json),
        Command::Doctor { json } => cmd::doctor::run(os, json).map_err(Into::into),
        Command::Completions { shell } => {
            clap_complete::generate(shell, &mut completion_command(), "godspeed", &mut std::io::stdout());
            Ok(())
        }
        Command::SelfManage { action } => match action {
            SelfAction::Update => cmd::self_manage::update().map_err(Into::into),
            SelfAction::Uninstall { yes } => cmd::self_manage::uninstall(yes).map_err(Into::into),
        },
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
    cmd::self_manage::tidy();
    // Asked in the background while the command runs; told, if at all, after its output. Not while the CLI
    // is updating or uninstalling itself: that command says everything there is to say.
    // Nor for output another program reads: completions are loaded at every shell start, JSON is parsed.
    let json = cli.command.json();
    let update = match cli.command {
        Command::SelfManage { .. } | Command::Completions { .. } => None,
        _ if json => None,
        _ => update::start(),
    };
    let code = match dispatch(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(diagnostics) => {
            let count = diagnostics.len();
            if json {
                let errors: Vec<_> = diagnostics.iter().map(Diagnostic::to_json).collect();
                eprintln!("{:#}", serde_json::json!({ "errors": errors }));
            } else {
                // Styled for a terminal; anstream drops the styling where colour is unsupported or unwanted
                // (not a terminal, NO_COLOR) and turns it on for older Windows consoles.
                for (i, d) in diagnostics.iter().enumerate() {
                    if i > 0 {
                        eprintln!();
                    }
                    anstream::eprint!("{}", d.styled());
                }
                if count > 1 {
                    eprintln!();
                    eprintln!("{count} problems found.");
                }
            }
            // The first diagnostic's kind decides the exit code; `check` only ever returns one kind.
            let code = diagnostics.first().map(|d| d.kind.exit_code()).unwrap_or(1);
            ExitCode::from(code as u8)
        }
    };
    update::finish(update);
    code
}
