//! The GodspeedOS platform the CLI builds against: a checkout of the OS repository named with `--os` or
//! `GODSPEED_OS`, or otherwise a platform kit for one OS release, fetched and cached (`kit.rs`). A kit holds
//! exactly the files below, at the same paths, so both are read by the same code.
//!
//! EVERYTHING the CLI reads from the OS is read here, and nowhere else. Each item below is a file the OS
//! keeps for its own build, not yet a published developer contract, so this module is the stopgap seam:
//! when the OS exports a fact publicly (the linker script through Cargo `links` metadata, a validator
//! crate for contracts), the reading changes here and no command changes.
//!
//! Read, never written. The CLI does not modify the OS checkout.
//!
//! | fact | read from |
//! |---|---|
//! | OS version | `Cargo.toml`, `[workspace.package] version` |
//! | toolchain pin and the targets the OS builds | `rust-toolchain.toml` |
//! | per-target compiler flags | `.cargo/config.toml`, `[target.<triple>] rustflags` |
//! | the service linker script | `services/user.ld` |
//! | the service contract schema | `contracts/schema/service.schema.json` |
//! | the stdlib crate exists, and is named `godspeed` | `stdlib/rust/Cargo.toml` |
//! | the entry symbol | nowhere: every OS service's `build.rs` repeats it, so it is [`DEFAULT_ENTRY`] |
//!
//! **A release that publishes a platform file** (`godspeed-platform.toml`, the OS's
//! `docs/developer-services-design.md` section 11) carries all seven facts in that one file instead, the
//! entry symbol included. A kit fetched from such a release holds the file, and the linker script it
//! carries written out beside it, and is read by [`Platform::open`] from the file: the files above are read
//! only from a checkout, or from a release that has no platform file. `backlog/01` has the history.

use crate::diag::{Diagnostic, Kind, Stage};
use crate::kit;
use crate::project::Project;
use godspeed_package::Version;
use std::path::{Path, PathBuf};

/// The environment variable naming the OS checkout, when `--os` is not given.
pub const OS_ENV: &str = "GODSPEED_OS";

/// The public repository the generated project's stdlib dependency points at.
pub const OS_GIT_URL: &str = "https://github.com/godspeedhq/godspeed-os";

/// The file a GodspeedOS release publishes for developer tooling: the facts below, in one place.
pub const PLATFORM_FILE: &str = "godspeed-platform.toml";
/// The platform-file formats this CLI reads. A newer one is refused, never guessed at.
pub const PLATFORM_FORMAT: i64 = 1;
/// Where a service starts, for a release that does not say: every release before the platform file, whose
/// services' `build.rs` all link with `--entry=service_main`.
pub const DEFAULT_ENTRY: &str = "service_main";

/// Where the platform's files came from.
#[derive(Debug, Clone)]
pub enum Source {
    /// A checkout named with `--os` or `GODSPEED_OS`.
    Checkout,
    /// A cached platform kit, fetched from this commit (or tag) of the OS repository.
    Kit { reference: String },
}

/// Which OS release a command needs, when no checkout is named.
pub enum Want<'a> {
    /// This release (`godspeed new --os-release`).
    Release(&'a str),
    /// The release the project is built against.
    Project(&'a Project),
    /// The latest published release, or failing that, the newest one cached.
    Latest,
}

#[derive(Debug, Clone)]
pub struct Platform {
    pub root: PathBuf,
    pub source: Source,
    pub version: String,
    pub toolchain: String,
    pub targets: Vec<String>,
    pub linker_script: PathBuf,
    pub schema: serde_json::Value,
    /// The symbol a service is linked to start at.
    pub entry: String,
    config: toml::Table,
}

fn not_a_checkout(root: &Path, why: String) -> Diagnostic {
    Diagnostic::new(
        Kind::Environment,
        Stage::Setup,
        "use the GodspeedOS checkout",
        format!("{} does not look like a GodspeedOS checkout: {why}", root.display()),
    )
    .expected("a clone of the GodspeedOS repository, e.g. `git clone https://github.com/godspeedhq/godspeed-os`")
    .next(format!("Pass the clone's path with `--os <path>` or set {OS_ENV}."))
}

fn read_toml(root: &Path, rel: &str) -> Result<toml::Table, Diagnostic> {
    let path = root.join(rel);
    let text = std::fs::read_to_string(&path).map_err(|e| not_a_checkout(root, format!("cannot read {rel}: {e}")))?;
    text.parse::<toml::Table>()
        .map_err(|e| not_a_checkout(root, format!("{rel} is not valid TOML: {e}")))
}

impl Platform {
    /// Locate the platform: `--os` wins, then `GODSPEED_OS`, then the platform kit for the release `want`
    /// names. Never "some" release: a project is built against the release it records, and a new one
    /// against the release asked for, or the latest - and the CLI says which.
    pub fn locate(flag: Option<&Path>, want: Want) -> Result<Platform, Diagnostic> {
        let named = flag
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os(OS_ENV).filter(|v| !v.is_empty()).map(PathBuf::from));
        if let Some(root) = named {
            return Self::open(&root);
        }
        let version = match want {
            Want::Release(v) => parse_release(v)?,
            Want::Project(p) => match p
                .stdlib_dependency()
                .and_then(|d| d.get("tag"))
                .and_then(|t| t.as_str())
            {
                Some(tag) => parse_release(tag)?,
                // A library records no OS release, only its toolchain: the newest kit with that toolchain.
                None => {
                    let pin = p.toolchain();
                    let matching = kit::all_cached().into_iter().find(|k| {
                        Self::open(&k.dir).is_ok_and(|plat| pin.is_none() || pin.as_deref() == Some(&plat.toolchain))
                    });
                    match matching {
                        Some(k) => return Self::from_kit(k),
                        None => latest()?,
                    }
                }
            },
            Want::Latest => latest()?,
        };
        Self::from_kit(kit::ensure(&version)?)
    }

    fn from_kit(kit: kit::Kit) -> Result<Platform, Diagnostic> {
        let mut platform = Self::open(&kit.dir)?;
        if platform.version != kit.version.to_string() {
            let _ = std::fs::remove_dir_all(&kit.dir);
            return Err(Diagnostic::new(
                Kind::Environment,
                Stage::Setup,
                format!("use GodspeedOS {}", kit.version),
                format!(
                    "The files fetched for GodspeedOS {} say they are version {}. The cached copy was removed.",
                    kit.version, platform.version
                ),
            )
            .next("Run the command again; if it persists, name an OS checkout with `--os <path>`."));
        }
        platform.source = Source::Kit {
            reference: kit.reference,
        };
        Ok(platform)
    }

    pub fn open(root: &Path) -> Result<Platform, Diagnostic> {
        if !root.is_dir() {
            return Err(not_a_checkout(root, "the directory does not exist".into()));
        }
        if root.join(PLATFORM_FILE).is_file() {
            return Self::open_platform_file(root);
        }

        let ws = read_toml(root, "Cargo.toml")?;
        let version = ws
            .get("workspace")
            .and_then(|w| w.get("package"))
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| not_a_checkout(root, "Cargo.toml has no [workspace.package] version".into()))?
            .to_string();

        let stdlib = read_toml(root, "stdlib/rust/Cargo.toml")?;
        let stdlib_name = stdlib
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str());
        if stdlib_name != Some("godspeed") {
            return Err(not_a_checkout(
                root,
                "stdlib/rust/Cargo.toml is not the `godspeed` crate".into(),
            ));
        }

        let pin = read_toml(root, "rust-toolchain.toml")?;
        let tc = pin
            .get("toolchain")
            .ok_or_else(|| not_a_checkout(root, "rust-toolchain.toml has no [toolchain]".into()))?;
        let toolchain = tc
            .get("channel")
            .and_then(|c| c.as_str())
            .ok_or_else(|| not_a_checkout(root, "rust-toolchain.toml has no channel".into()))?
            .to_string();
        let targets = tc
            .get("targets")
            .and_then(|t| t.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        // Optional: an OS with no per-target flags is valid; it just has none to pass on.
        let config = match std::fs::read_to_string(root.join(".cargo/config.toml")) {
            Ok(text) => text
                .parse::<toml::Table>()
                .map_err(|e| not_a_checkout(root, format!(".cargo/config.toml is not valid TOML: {e}")))?,
            Err(_) => toml::Table::new(),
        };

        let linker_script = root.join("services").join("user.ld");
        if !linker_script.is_file() {
            return Err(not_a_checkout(
                root,
                "services/user.ld (the service linker script) is missing".into(),
            ));
        }

        let schema_path = root.join("contracts/schema/service.schema.json");
        let schema_text = std::fs::read_to_string(&schema_path)
            .map_err(|e| not_a_checkout(root, format!("cannot read the contract schema: {e}")))?;
        let schema = serde_json::from_str(&schema_text)
            .map_err(|e| not_a_checkout(root, format!("the contract schema is not valid JSON: {e}")))?;

        Ok(Platform {
            root: root.to_path_buf(),
            source: Source::Checkout,
            version,
            toolchain,
            targets,
            linker_script,
            schema,
            entry: DEFAULT_ENTRY.to_string(),
            config,
        })
    }

    /// A kit fetched from a release that publishes `godspeed-platform.toml`: every fact from that file, and
    /// the linker script from the copy written beside it when the kit was fetched.
    fn open_platform_file(root: &Path) -> Result<Platform, Diagnostic> {
        let text = std::fs::read_to_string(root.join(PLATFORM_FILE))
            .map_err(|e| not_a_checkout(root, format!("cannot read {PLATFORM_FILE}: {e}")))?;
        let file = PlatformFile::parse(&text).map_err(|e| e.diagnostic(root))?;
        let linker_script = root.join(LINKER_SCRIPT_IN_KIT);
        if !linker_script.is_file() {
            return Err(not_a_checkout(root, format!("{LINKER_SCRIPT_IN_KIT} is missing")));
        }
        let mut target = toml::Table::new();
        for (triple, flags) in &file.rustflags {
            let mut t = toml::Table::new();
            t.insert(
                "rustflags".into(),
                toml::Value::Array(flags.iter().cloned().map(toml::Value::String).collect()),
            );
            target.insert(triple.clone(), toml::Value::Table(t));
        }
        let mut config = toml::Table::new();
        config.insert("target".into(), toml::Value::Table(target));
        Ok(Platform {
            root: root.to_path_buf(),
            source: Source::Checkout,
            version: file.os,
            toolchain: file.toolchain,
            targets: file.targets,
            linker_script,
            schema: file.schema,
            entry: file.entry,
            config,
        })
    }

    /// Where it came from, for a person: the checkout's path, or the kit's release and commit.
    pub fn describe(&self) -> String {
        match &self.source {
            Source::Checkout => format!("{} (checkout at {})", self.version, self.root.display()),
            Source::Kit { reference } => format!(
                "{} (platform kit from {}, cached in {})",
                self.version,
                &reference[..reference.len().min(12)],
                self.root.display()
            ),
        }
    }

    /// The release tag for this OS version, which a generated project's stdlib dependency names.
    pub fn tag(&self) -> String {
        format!("v{}", self.version)
    }

    /// Does the OS build services for this target triple?
    pub fn supports(&self, triple: &str) -> bool {
        self.targets.iter().any(|t| t == triple)
    }

    /// The compiler flags the OS applies to every crate it builds for `triple`.
    pub fn rustflags(&self, triple: &str) -> Vec<String> {
        self.config
            .get("target")
            .and_then(|t| t.get(triple))
            .and_then(|t| t.get("rustflags"))
            .and_then(|f| f.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default()
    }

    /// The pattern a service name must match, read from the schema so the CLI never keeps a second copy.
    pub fn name_pattern(&self) -> Option<&str> {
        self.schema.get("properties")?.get("name")?.get("pattern")?.as_str()
    }
}

/// Where a kit built from a platform file keeps the linker script the file carries, so a build passes it to
/// the linker by path exactly as it does a checkout's.
pub const LINKER_SCRIPT_IN_KIT: &str = "services/user.ld";

/// What a release's `godspeed-platform.toml` says, checked: every fact present and of the right shape, and a
/// format this CLI knows.
pub struct PlatformFile {
    pub os: String,
    pub commit: Option<String>,
    pub toolchain: String,
    pub targets: Vec<String>,
    pub rustflags: Vec<(String, Vec<String>)>,
    pub entry: String,
    pub linker_script: String,
    pub schema: serde_json::Value,
}

/// Why a platform file was refused.
pub enum PlatformFileError {
    /// A format this CLI does not read: a newer release than this CLI knows.
    NewerFormat(i64),
    /// Not a platform file this CLI can use.
    Malformed(String),
}

impl PlatformFileError {
    pub fn diagnostic(&self, from: &Path) -> Diagnostic {
        match self {
            PlatformFileError::NewerFormat(n) => Diagnostic::new(
                Kind::Environment,
                Stage::Setup,
                "read the GodspeedOS platform file",
                format!("This GodspeedOS release describes itself in platform-file format {n}, and this godspeed reads format {PLATFORM_FORMAT}."),
            )
            .expected(format!("a godspeed that reads format {n}"))
            .next("Run `godspeed self update`, then the command again."),
            PlatformFileError::Malformed(why) => not_a_checkout(from, format!("its {PLATFORM_FILE} {why}")),
        }
    }
}

impl PlatformFile {
    pub fn parse(text: &str) -> Result<PlatformFile, PlatformFileError> {
        let bad = |why: &str| PlatformFileError::Malformed(why.to_string());
        let t: toml::Table = text
            .parse()
            .map_err(|e| PlatformFileError::Malformed(format!("is not valid TOML: {e}")))?;
        // The format first: a newer file may have changed any key below, so nothing else is read from it.
        match t.get("format").and_then(|v| v.as_integer()) {
            Some(PLATFORM_FORMAT) => {}
            Some(n) if n > PLATFORM_FORMAT => return Err(PlatformFileError::NewerFormat(n)),
            Some(_) => return Err(bad("has a format number this CLI does not know")),
            None => return Err(bad("has no format")),
        }
        let s =
            |table: Option<&toml::Value>, key: &str| -> Option<String> { table?.get(key)?.as_str().map(String::from) };
        let strings = |v: Option<&toml::Value>| -> Option<Vec<String>> {
            v?.as_array()?.iter().map(|x| x.as_str().map(String::from)).collect()
        };
        let os = t
            .get("os")
            .and_then(|v| v.as_str())
            .ok_or_else(|| bad("has no os version"))?
            .to_string();
        if s(t.get("stdlib"), "crate").as_deref() != Some("godspeed") {
            return Err(bad("does not name the `godspeed` stdlib crate"));
        }
        let toolchain = s(t.get("toolchain"), "channel").ok_or_else(|| bad("has no toolchain channel"))?;
        let targets = strings(t.get("toolchain").and_then(|tc| tc.get("targets")))
            .filter(|v| !v.is_empty())
            .ok_or_else(|| bad("lists no targets"))?;
        let flags = t
            .get("rustflags")
            .and_then(|v| v.as_table())
            .ok_or_else(|| bad("has no [rustflags]"))?;
        let mut rustflags = Vec::new();
        for triple in &targets {
            let f = strings(flags.get(triple))
                .ok_or_else(|| PlatformFileError::Malformed(format!("has no rustflags for {triple}")))?;
            rustflags.push((triple.clone(), f));
        }
        let entry = s(t.get("link"), "entry").ok_or_else(|| bad("has no link entry symbol"))?;
        if entry.is_empty() || !entry.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(bad("names an entry symbol that is not an identifier"));
        }
        let script = s(t.get("link"), "script").ok_or_else(|| bad("has no link script"))?;
        let files = t
            .get("files")
            .and_then(|v| v.as_table())
            .ok_or_else(|| bad("has no [files]"))?;
        let linker_script = files
            .get(&script)
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                PlatformFileError::Malformed(format!("does not carry {script}, the linker script it names"))
            })?
            .to_string();
        let schema_text = files
            .get("service.schema.json")
            .and_then(|v| v.as_str())
            .ok_or_else(|| bad("does not carry the contract schema"))?;
        let schema = serde_json::from_str(schema_text)
            .map_err(|e| PlatformFileError::Malformed(format!("carries a contract schema that is not JSON: {e}")))?;
        Ok(PlatformFile {
            os,
            commit: t.get("commit").and_then(|v| v.as_str()).map(String::from),
            toolchain,
            targets,
            rustflags,
            entry,
            linker_script,
            schema,
        })
    }
}

/// `0.28.0` or `v0.28.0`.
fn parse_release(text: &str) -> Result<Version, Diagnostic> {
    text.trim_start_matches('v').parse().map_err(|_| {
        Diagnostic::new(
            Kind::InvalidProject,
            Stage::Setup,
            format!("use GodspeedOS \"{text}\""),
            "That is not a GodspeedOS release version.",
        )
        .expected("a release such as 0.28.0 (the stdlib tag in Cargo.toml is v<release>)")
        .next(format!(
            "See the releases at {OS_GIT_URL}/releases, or name an OS checkout with `--os <path>`."
        ))
    })
}

/// The latest release; offline, the newest cached kit's, said out loud.
fn latest() -> Result<Version, Diagnostic> {
    match kit::latest() {
        Ok(v) => Ok(v),
        Err(why) => match kit::all_cached().into_iter().next() {
            Some(k) => {
                eprintln!("{why}; using GodspeedOS {}, the newest release cached here.", k.version);
                Ok(k.version)
            }
            None => Err(Diagnostic::new(
                Kind::MissingDependency,
                Stage::Setup,
                "find GodspeedOS",
                format!("{why}, and no GodspeedOS release is cached on this machine yet."),
            )
            .expected("the network, once, to fetch the platform kit for a GodspeedOS release")
            .next(format!(
                "Connect and run the command again. Without a network, name a GodspeedOS checkout instead: \
                 clone {OS_GIT_URL} and pass `--os <path>` or set {OS_ENV}."
            ))),
        },
    }
}
