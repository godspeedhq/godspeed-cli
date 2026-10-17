//! The CLI's promises, tested through the binary.
//!
//! Offline tests run against `tests/fixtures/os`, the parts of a GodspeedOS checkout the CLI reads. The
//! end-to-end build test needs a real checkout, the pinned toolchain and the network, so it is
//! `#[ignore]`d and run deliberately:
//!
//!     GODSPEED_OS=<path to a GodspeedOS checkout> cargo test -- --ignored

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture_os() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/os")
}

/// A fresh, empty directory for one test, under Cargo's target directory.
fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Never a GodspeedOS release server: the tests reach GitHub for nothing.
const NO_OS_SERVER: &str = "http://127.0.0.1:9";

/// The CLI, isolated from the developer's own setup: no OS checkout, registry or token from the environment,
/// a configuration directory and a cache of its own, so no test reads or writes real credentials, and no
/// GodspeedOS release server unless a test starts one.
fn command(dir: &Path, os: Option<&Path>, args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_godspeed"));
    c.current_dir(dir)
        .env_remove("GODSPEED_OS")
        .env_remove("GODSPEED_REGISTRY")
        .env_remove("GODSPEED_TOKEN")
        .env_remove("GODSPEED_SIGNING_KEY")
        .env("GODSPEED_CONFIG_DIR", dir.join(".godspeed-config"))
        .env("GODSPEED_CACHE_DIR", dir.join(".godspeed-cache"))
        .env("GODSPEED_TEST_OS_URL", NO_OS_SERVER);
    if let Some(os) = os {
        c.arg("--os").arg(os);
    }
    c.args(args);
    c
}

fn godspeed(dir: &Path, os: Option<&Path>, args: &[&str]) -> Output {
    command(dir, os, args).output().unwrap()
}

/// The CLI with `input` on standard input.
fn godspeed_with_input(dir: &Path, args: &[&str], input: &str) -> Output {
    use std::io::Write;
    let mut child = command(dir, None, args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn code(o: &Output) -> i32 {
    o.status.code().unwrap()
}

/// Stderr with every run of whitespace collapsed to one space, so a match does not depend on where a
/// diagnostic happened to wrap.
fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn new_service_creates_a_project_that_checks_clean() {
    let dir = scratch("new_service");
    let os = fixture_os();
    let o = godspeed(&dir, Some(&os), &["new", "service", "hello"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));

    let p = dir.join("hello");
    for f in [
        "godspeed.toml",
        "Cargo.toml",
        "contracts/hello.toml",
        "src/main.rs",
        "README.md",
        "rust-toolchain.toml",
        ".gitignore",
    ] {
        assert!(p.join(f).is_file(), "missing {f}");
    }
    let cargo = std::fs::read_to_string(p.join("Cargo.toml")).unwrap();
    assert!(cargo.contains("tag = \"v0.28.0\""), "{cargo}");
    let main = std::fs::read_to_string(p.join("src/main.rs")).unwrap();
    assert!(main.contains("#[no_mangle]") && main.contains("fn service_main"));
    assert!(!main.contains("{{"), "unfilled placeholder in src/main.rs");

    let o = godspeed(&p, Some(&os), &["check"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
}

#[test]
fn new_with_a_publisher_fills_it_in_and_still_checks_clean() {
    let dir = scratch("new_publisher");
    let os = fixture_os();
    let o = godspeed(&dir, Some(&os), &["new", "service", "hello", "--publisher", "alice"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let p = dir.join("hello");
    let manifest = std::fs::read_to_string(p.join("godspeed.toml")).unwrap();
    assert!(manifest.lines().any(|l| l == "publisher = \"alice\""), "{manifest}");
    assert!(!manifest.contains("your-name"), "{manifest}");
    let o = godspeed(&p, Some(&os), &["check"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
}

#[test]
fn new_refuses_a_bad_publisher_or_one_for_a_library_and_writes_nothing() {
    let dir = scratch("new_bad_publisher");
    let os = fixture_os();
    for (args, says) in [
        (
            &["new", "service", "hello", "--publisher", "Alice"][..],
            "first part of a package name",
        ),
        (
            &["new", "library", "parser", "--publisher", "alice"],
            "has no publisher",
        ),
    ] {
        let o = godspeed(&dir, Some(&os), args);
        assert_eq!(code(&o), 10, "{}", stderr(&o));
        assert!(stderr(&o).contains(says), "{}", stderr(&o));
    }
    assert!(!dir.join("hello").exists() && !dir.join("parser").exists());
}

#[test]
fn new_library_creates_a_project_that_checks_clean() {
    let dir = scratch("new_library");
    let os = fixture_os();
    assert_eq!(code(&godspeed(&dir, Some(&os), &["new", "library", "parser"])), 0);
    let p = dir.join("parser");
    assert!(p.join("src/lib.rs").is_file());
    assert!(
        !p.join("contracts").exists(),
        "a library requests no authority, so it has no contract"
    );
    let o = godspeed(&p, Some(&os), &["check"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
}

#[test]
fn every_kind_is_made_and_an_unknown_one_is_refused() {
    // Every kind GodspeedOS runs has a template now: service, utility, app and driver (and library). A kind that
    // is not one of them is refused and writes nothing.
    let dir = scratch("kinds");
    let os = fixture_os();
    let o = godspeed(&dir, Some(&os), &["new", "driver", "drv"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let contract = std::fs::read_to_string(dir.join("drv").join("contracts").join("drv.toml")).unwrap_or_default();
    assert!(contract.contains("hw_pci_class"), "a driver's contract names its device by class");
    let o = godspeed(&dir, Some(&os), &["new", "widget", "x"]);
    assert_eq!(code(&o), 10);
    assert!(!dir.join("x").exists(), "a refused kind must write nothing");
}

#[test]
fn names_follow_the_os_contract_schema() {
    let dir = scratch("names");
    let os = fixture_os();
    for bad in ["Hello", "1st", "my_service", "-x"] {
        let o = godspeed(&dir, Some(&os), &["new", "service", "--", bad]);
        assert_eq!(code(&o), 10, "{bad}: {}", stderr(&o));
    }
}

#[test]
fn new_never_writes_into_a_non_empty_directory() {
    let dir = scratch("non_empty");
    let os = fixture_os();
    std::fs::create_dir_all(dir.join("hello")).unwrap();
    std::fs::write(dir.join("hello/keep.txt"), "mine").unwrap();
    let o = godspeed(&dir, Some(&os), &["new", "service", "hello"]);
    assert_eq!(code(&o), 10);
    assert_eq!(std::fs::read_dir(dir.join("hello")).unwrap().count(), 1);
}

#[test]
fn check_reports_every_problem_at_once() {
    let dir = scratch("check_problems");
    let os = fixture_os();
    assert_eq!(code(&godspeed(&dir, Some(&os), &["new", "service", "hello"])), 0);
    let p = dir.join("hello");
    let contract = p.join("contracts/hello.toml");
    let text = std::fs::read_to_string(&contract)
        .unwrap()
        .replace("request = \"32MiB\"", "request = \"lots\"")
        .replace("version = \"0.1.0\"", "version = \"0.2.0\"");
    std::fs::write(&contract, text).unwrap();
    let main = std::fs::read_to_string(p.join("src/main.rs"))
        .unwrap()
        .replace("#[no_mangle]", "");
    std::fs::write(p.join("src/main.rs"), main).unwrap();

    let o = godspeed(&p, Some(&os), &["check"]);
    assert_eq!(code(&o), 10);
    let err = stderr(&o);
    assert!(err.contains("/resources/memory/request"), "{err}");
    assert!(err.contains("version 0.2.0"), "{err}");
    assert!(err.contains("#[no_mangle]"), "{err}");
    assert!(err.contains("3 problems found"), "{err}");
}

#[test]
fn check_refuses_a_toolchain_the_os_is_not_built_with() {
    let dir = scratch("toolchain");
    let os = fixture_os();
    assert_eq!(code(&godspeed(&dir, Some(&os), &["new", "library", "lib"])), 0);
    let p = dir.join("lib");
    let pin = std::fs::read_to_string(p.join("rust-toolchain.toml"))
        .unwrap()
        .replace("2026-05-16", "2026-01-01");
    std::fs::write(p.join("rust-toolchain.toml"), pin).unwrap();
    let o = godspeed(&p, Some(&os), &["check"]);
    assert_eq!(code(&o), 10);
    assert!(stderr(&o).contains("nightly-2026-01-01"), "{}", stderr(&o));
}

#[test]
fn offline_with_nothing_cached_the_cli_says_what_it_needs() {
    let dir = scratch("no_os");
    let o = godspeed(&dir, None, &["new", "service", "hello"]);
    assert_eq!(code(&o), 13, "{}", stderr(&o));
    assert!(stderr(&o).contains("no GodspeedOS release is cached"), "{}", stderr(&o));
    assert!(stderr(&o).contains("--os <path>"), "{}", stderr(&o));
    assert!(!dir.join("hello").exists());
    let o = godspeed(&dir, Some(&dir), &["new", "service", "hello"]);
    assert_eq!(code(&o), 11);
    assert!(stderr(&o).contains("does not look like a GodspeedOS checkout"));
}

#[test]
fn unknown_architectures_are_refused_before_anything_runs() {
    let dir = scratch("arch");
    let os = fixture_os();
    assert_eq!(code(&godspeed(&dir, Some(&os), &["new", "service", "hello"])), 0);
    let o = godspeed(&dir.join("hello"), Some(&os), &["build", "--arch", "mips"]);
    assert_eq!(code(&o), 12);
    assert!(!dir.join("hello/target").exists(), "nothing should have been built");
}

#[test]
fn run_and_service_test_say_the_os_cannot_do_it_yet() {
    let dir = scratch("run");
    let os = fixture_os();
    assert_eq!(code(&godspeed(&dir, Some(&os), &["new", "service", "hello"])), 0);
    let p = dir.join("hello");
    for args in [&["run"][..], &["run", "--target", "hardware"], &["test"]] {
        let o = godspeed(&p, Some(&os), args);
        assert_eq!(code(&o), 20, "{args:?}");
        assert!(stderr(&o).contains("supervisor"), "{args:?}");
    }
}

#[test]
fn with_no_registry_named_the_official_one_is_used() {
    const OFFICIAL: &str = "https://godspeed-registry.bankole-422.workers.dev";
    let dir = scratch("no_registry");
    // Without a token, neither needs the network to refuse: they say which registry they meant.
    for verb in ["logout", "whoami"] {
        let o = godspeed(&dir, None, &[verb]);
        assert_eq!(code(&o), 18, "{verb}: {}", stderr(&o));
        assert!(
            stderr(&o).contains(&format!("not logged in to {OFFICIAL}")),
            "{verb}: {}",
            stderr(&o)
        );
    }
    // Making a token needs no network either.
    let o = godspeed(&dir, None, &["login"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(
        stdout(&o).contains(&format!("Made a publisher token for {OFFICIAL}")),
        "{}",
        stdout(&o)
    );
}

#[test]
fn a_registry_is_reached_over_https_unless_it_is_on_this_machine() {
    let dir = scratch("plain_http");
    let o = godspeed(&dir, None, &["whoami", "--registry", "http://registry.example"]);
    assert_eq!(code(&o), 11, "{}", stderr(&o));
    assert!(stderr(&o).contains("HTTPS"), "{}", stderr(&o));
}

/// The value after `marker` on the first line of `text` that contains it, trimmed.
fn after<'a>(text: &'a str, marker: &str) -> &'a str {
    text.lines()
        .find_map(|l| l.split_once(marker).map(|(_, v)| v.trim()))
        .unwrap_or("")
}

#[test]
fn login_makes_a_token_here_and_prints_only_its_fingerprint() {
    let dir = scratch("login_makes");
    // 127.0.0.1:9 is never a registry: making a token needs no network.
    let o = godspeed(&dir, None, &["login", "--registry", "http://127.0.0.1:9"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    let stored = std::fs::read_to_string(dir.join(".godspeed-config/credentials.toml")).unwrap();
    let token = after(&stored, "token = ").trim_matches('"');
    assert!(token.starts_with("gsp_") && token.len() == 4 + 64, "{token}");
    let fp = godspeed_package::Sha256Digest::of(token.as_bytes()).to_string();
    assert!(
        out.contains(&fp),
        "the fingerprint printed is the token's SHA-256: {out}"
    );
    assert!(!out.contains(token), "the token itself is never printed: {out}");

    // Again: the same token, whose standing could not be checked - not a new one.
    let o = godspeed(&dir, None, &["login", "--registry", "http://127.0.0.1:9"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(String::from_utf8_lossy(&o.stdout).contains(&fp));
    assert_eq!(
        std::fs::read_to_string(dir.join(".godspeed-config/credentials.toml")).unwrap(),
        stored
    );
}

#[test]
fn token_new_prints_a_token_once_and_stores_nothing() {
    let dir = scratch("token_new");
    let o = godspeed(&dir, None, &["token", "new"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let token = String::from_utf8_lossy(&o.stdout).trim().to_string();
    assert!(token.starts_with("gsp_") && token.len() == 4 + 64, "{token}");
    let fp = godspeed_package::Sha256Digest::of(token.as_bytes()).to_string();
    assert!(stderr(&o).contains(&fp), "{}", stderr(&o));
    assert!(!dir.join(".godspeed-config").exists(), "nothing stored");
}

#[test]
fn login_refuses_what_is_not_a_token_without_contacting_anyone() {
    let dir = scratch("bad_token");
    // 127.0.0.1:9 (discard) is never a registry: had the CLI tried to contact it, the error would differ.
    let o = godspeed_with_input(
        &dir,
        &["login", "--with-token", "--registry", "http://127.0.0.1:9"],
        "hunter2\n",
    );
    assert_eq!(code(&o), 18, "{}", stderr(&o));
    assert!(stderr(&o).contains("not a Godspeed publisher token"), "{}", stderr(&o));
    assert!(
        !dir.join(".godspeed-config/credentials.toml").exists(),
        "nothing is stored"
    );
}

#[test]
fn an_unreachable_registry_is_reported_as_unreachable_not_as_a_refusal() {
    let dir = scratch("unreachable");
    let o = godspeed_with_input(
        &dir,
        &["login", "--with-token", "--registry", "http://127.0.0.1:9"],
        "gsp_0123456789abcdef\n",
    );
    assert_eq!(code(&o), 19, "{}", stderr(&o));
    assert!(stderr(&o).contains("could not be reached"), "{}", stderr(&o));
    assert!(
        !dir.join(".godspeed-config/credentials.toml").exists(),
        "an unchecked token is not stored"
    );
}

#[test]
fn publish_without_a_login_is_refused_before_building_anything() {
    let dir = scratch("publish_no_login");
    let os = fixture_os();
    assert_eq!(code(&godspeed(&dir, Some(&os), &["new", "service", "hello"])), 0);
    let o = godspeed(
        &dir.join("hello"),
        Some(&os),
        &["publish", "--registry", "http://127.0.0.1:9"],
    );
    assert_eq!(code(&o), 18, "{}", stderr(&o));
    assert!(stderr(&o).contains("not logged in"), "{}", stderr(&o));
    assert!(!dir.join("hello/target").exists(), "refused before building");
}

#[test]
fn package_needs_a_publisher_and_says_which_line_to_add() {
    let dir = scratch("package_publisher");
    let os = fixture_os();
    assert_eq!(code(&godspeed(&dir, Some(&os), &["new", "service", "hello"])), 0);
    let o = godspeed(&dir.join("hello"), Some(&os), &["package"]);
    assert_eq!(code(&o), 10, "{}", stderr(&o));
    assert!(stderr(&o).contains("publisher = \"<your-name>\""), "{}", stderr(&o));
    assert!(!dir.join("hello/target").exists(), "refused before building anything");
}

#[test]
fn package_refuses_a_library_with_the_reason() {
    let dir = scratch("package_library");
    let os = fixture_os();
    assert_eq!(code(&godspeed(&dir, Some(&os), &["new", "library", "parser"])), 0);
    let o = godspeed(&dir.join("parser"), Some(&os), &["package"]);
    assert_eq!(code(&o), 21);
    assert!(stderr(&o).contains("not defined yet"), "{}", stderr(&o));
}

// ---- verify: packages built here byte by byte, so no compiler or OS checkout is needed ----

/// The first bytes of a little-endian 64-bit ELF for `machine`, padded to `len`.
fn fake_elf(machine: u16, len: usize) -> Vec<u8> {
    let mut b = vec![0u8; len.max(64)];
    b[..4].copy_from_slice(b"\x7fELF");
    b[4] = 2; // 64-bit
    b[5] = 1; // little-endian
    b[6] = 1; // ELF version
    b[18..20].copy_from_slice(&machine.to_le_bytes());
    b[24..32].copy_from_slice(&0x40_0000u64.to_le_bytes());
    b
}

const CONTRACT: &str = "name = \"hello\"\nversion = \"1.0.0\"\n\n[resources.memory]\nrequest = \"32MiB\"\nlimit = \"32MiB\"\n\n[capabilities]\nlog_write = true\n";

/// Write a valid two-architecture package for alice/hello 1.0.0 into a fresh directory.
fn write_package(test: &str) -> PathBuf {
    use godspeed_package as pkg;
    let dir = scratch(test).join("alice-hello-1.0.0");
    std::fs::create_dir_all(&dir).unwrap();
    let files = [
        (pkg::Arch::X86_64, fake_elf(0x3E, 300)),
        (pkg::Arch::Riscv64, fake_elf(0xF3, 400)),
    ];
    let artifacts: Vec<pkg::Artifact> = files
        .iter()
        .map(|(arch, bytes)| pkg::Artifact {
            arch: *arch,
            size: bytes.len() as u64,
            digest: pkg::Sha256Digest::of(bytes),
        })
        .collect();
    for (arch, bytes) in &files {
        std::fs::write(dir.join(arch.artifact_file()), bytes).unwrap();
    }
    std::fs::write(dir.join(pkg::CONTRACT_FILE), CONTRACT).unwrap();
    let m = pkg::Manifest {
        format: pkg::FORMAT,
        name: pkg::PackageName::parse("alice/hello").unwrap(),
        version: pkg::Version::new(1, 0, 0),
        kind: pkg::Kind::Service,
        description: Some("Says hello"),
        license: None,
        os_release: pkg::Version::new(0, 28, 0),
        contract: pkg::Sha256Digest::of(CONTRACT.as_bytes()),
        artifacts: &artifacts,
    };
    std::fs::write(dir.join("manifest.toml"), pkg::to_toml(&m)).unwrap();
    dir
}

/// A tampering case: its name, what it does to a package, and what the refusal must say.
type Tamper = (&'static str, fn(&Path), &'static str);

fn verify(dir: &Path) -> Output {
    godspeed(dir.parent().unwrap(), None, &["verify", dir.to_str().unwrap()])
}

// ---- signing ----

/// A signing key's secret seed, for tests only.
const TEST_SEED: &str = "0707070707070707070707070707070707070707070707070707070707070707";

#[test]
fn key_generate_shows_the_public_half_and_never_replaces_a_key() {
    let dir = scratch("keys");
    let o = godspeed(&dir, None, &["key", "show"]);
    assert_eq!(code(&o), 11, "no key yet: {}", stderr(&o));
    assert!(stderr(&o).contains("godspeed key generate"), "{}", stderr(&o));

    let o = godspeed(&dir, None, &["key", "generate"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    let o = godspeed(&dir, None, &["key", "show"]);
    let public = String::from_utf8_lossy(&o.stdout).trim().to_string();
    assert!(public.starts_with("ed25519:") && public.len() == 8 + 64, "{public}");
    assert!(out.contains(&public), "generate prints the public key: {out}");

    let stored = std::fs::read_to_string(dir.join(".godspeed-config/signing-key.toml")).unwrap();
    let o = godspeed(&dir, None, &["key", "generate"]);
    assert_eq!(code(&o), 24, "{}", stderr(&o));
    assert_eq!(
        std::fs::read_to_string(dir.join(".godspeed-config/signing-key.toml")).unwrap(),
        stored,
        "unchanged"
    );
}

#[test]
fn sign_then_verify_names_the_key_and_any_change_breaks_the_signature() {
    let dir = write_package("sign_verify");
    let o = verify(&dir);
    assert_eq!(code(&o), 0);
    assert!(
        String::from_utf8_lossy(&o.stdout).contains("(not signed)"),
        "an unsigned package says so"
    );

    let mut c = command(dir.parent().unwrap(), None, &["sign", dir.to_str().unwrap()]);
    let o = c.env("GODSPEED_SIGNING_KEY", TEST_SEED).output().unwrap();
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let o = verify(&dir);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(
        out.contains("manifest.sig") && out.contains("signed by ed25519:"),
        "{out}"
    );
    assert!(
        out.contains("is the registry's to establish"),
        "a signature is not presented as identity: {out}"
    );

    // The manifest edited after signing: the signature no longer covers it.
    let manifest = std::fs::read_to_string(dir.join("manifest.toml")).unwrap();
    std::fs::write(
        dir.join("manifest.toml"),
        manifest.replace("Says hello", "Says goodbye"),
    )
    .unwrap();
    let o = verify(&dir);
    assert_eq!(code(&o), 26, "{}", stderr(&o));
    assert!(
        stderr(&o).contains("this key did not sign this manifest"),
        "{}",
        stderr(&o)
    );
    std::fs::write(dir.join("manifest.toml"), &manifest).unwrap();

    // The signature file itself altered.
    let sig = std::fs::read_to_string(dir.join("manifest.sig")).unwrap();
    std::fs::write(dir.join("manifest.sig"), sig.replace("format = 1", "format = 1 ")).unwrap();
    let o = verify(&dir);
    assert_eq!(code(&o), 26, "{}", stderr(&o));
    assert!(stderr(&o).contains("canonical form"), "{}", stderr(&o));
}

#[test]
fn fetch_refuses_a_bad_name_or_version_before_contacting_anyone() {
    let dir = scratch("fetch_names");
    for (spec, says) in [("hello", "publisher/name"), ("alice/hello@1.02.0", "not a version")] {
        let o = godspeed(&dir, None, &["fetch", spec, "--registry", "http://127.0.0.1:9"]);
        assert_eq!(code(&o), 10, "{spec}: {}", stderr(&o));
        assert!(stderr(&o).contains(says), "{spec}: {}", stderr(&o));
    }
}

#[test]
fn fetch_from_an_unreachable_registry_says_so_and_trusts_nothing() {
    let dir = scratch("fetch_unreachable");
    let o = godspeed(
        &dir,
        None,
        &["fetch", "alice/hello", "--registry", "http://127.0.0.1:9"],
    );
    assert_eq!(code(&o), 19, "{}", stderr(&o));
    assert!(
        !dir.join(".godspeed-config/trust").exists(),
        "nothing remembered from a registry never reached"
    );
    assert!(!dir.join("alice-hello").exists());
}

#[test]
fn a_bad_signing_key_in_the_environment_is_refused() {
    let dir = write_package("bad_seed");
    let mut c = command(dir.parent().unwrap(), None, &["sign", dir.to_str().unwrap()]);
    let o = c.env("GODSPEED_SIGNING_KEY", "not-hex").output().unwrap();
    assert_eq!(code(&o), 11, "{}", stderr(&o));
    assert!(!dir.join("manifest.sig").exists());
}

#[test]
fn verify_accepts_a_good_package_without_an_os_checkout() {
    let dir = write_package("verify_good");
    let o = verify(&dir);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(
        out.contains("Verified alice/hello 1.0.0 (service), built against GodspeedOS 0.28.0"),
        "{out}"
    );
    assert!(out.contains("requests: log_write"), "{out}");
    assert!(
        out.contains("Nothing says who made them: the package is") && out.contains("not signed"),
        "an unsigned package must not imply more than integrity: {out}"
    );
}

#[test]
fn verify_catches_altered_bytes_as_integrity_failures() {
    let cases: [Tamper; 4] = [
        (
            "flip",
            |d| {
                let p = d.join("riscv64.elf");
                let mut b = std::fs::read(&p).unwrap();
                b[200] ^= 1;
                std::fs::write(p, b).unwrap();
            },
            "checksum mismatch",
        ),
        (
            "truncate",
            |d| {
                let p = d.join("x86_64.elf");
                let b = std::fs::read(&p).unwrap();
                std::fs::write(p, &b[..100]).unwrap();
            },
            "100 of the declared 300 bytes",
        ),
        (
            "pad",
            |d| {
                let p = d.join("x86_64.elf");
                let mut b = std::fs::read(&p).unwrap();
                b.push(0);
                std::fs::write(p, b).unwrap();
            },
            "more than the declared 300 bytes",
        ),
        (
            "contract",
            |d| {
                let p = d.join("contract.toml");
                let t = std::fs::read_to_string(&p).unwrap() + "service_control = true\n";
                std::fs::write(p, t).unwrap();
            },
            "is not the contract the manifest names",
        ),
    ];
    for (name, tamper, says) in cases {
        let dir = write_package(&format!("verify_integrity_{name}"));
        tamper(&dir);
        let o = verify(&dir);
        assert_eq!(code(&o), 17, "{name}: {}", stderr(&o));
        assert!(stderr(&o).contains(says), "{name}: {}", stderr(&o));
    }
}

#[test]
fn verify_catches_malformed_packages() {
    let cases: [Tamper; 6] = [
        (
            "extra",
            |d| std::fs::write(d.join("notes.txt"), "x").unwrap(),
            "notes.txt is not part of the package",
        ),
        (
            "missing",
            |d| std::fs::remove_file(d.join("riscv64.elf")).unwrap(),
            "riscv64.elf cannot be read",
        ),
        (
            "no_manifest",
            |d| std::fs::remove_file(d.join("manifest.toml")).unwrap(),
            "manifest.toml cannot be read",
        ),
        (
            "unknown_key",
            |d| {
                let p = d.join("manifest.toml");
                let t = std::fs::read_to_string(&p)
                    .unwrap()
                    .replace("[os]\n", "[os]\nlatest = true\n");
                std::fs::write(p, t).unwrap();
            },
            "unknown field `latest`",
        ),
        (
            "format",
            |d| {
                let p = d.join("manifest.toml");
                let t = std::fs::read_to_string(&p).unwrap().replace("format = 1", "format = 9");
                std::fs::write(p, t).unwrap();
            },
            "this is format 9; this reader knows format 1",
        ),
        (
            // The right bytes for the manifest, but an x86 binary filed as riscv64: the digest cannot catch
            // a packager's mistake, so the ELF machine check must.
            "misfiled",
            |d| {
                use godspeed_package as pkg;
                let wrong = fake_elf(0x3E, 400);
                std::fs::write(d.join("riscv64.elf"), &wrong).unwrap();
                let p = d.join("manifest.toml");
                let t = std::fs::read_to_string(&p).unwrap();
                let old = pkg::Sha256Digest::of(&fake_elf(0xF3, 400)).to_string();
                std::fs::write(p, t.replace(&old, &pkg::Sha256Digest::of(&wrong).to_string())).unwrap();
            },
            "riscv64.elf is an ELF for machine 0x3e, not riscv64",
        ),
    ];
    for (name, tamper, says) in cases {
        let dir = write_package(&format!("verify_malformed_{name}"));
        tamper(&dir);
        let o = verify(&dir);
        assert_eq!(code(&o), 16, "{name}: {}", stderr(&o));
        assert!(stderr(&o).contains(says), "{name}: {}", stderr(&o));
    }
}

#[test]
fn native_package_manager_verbs_point_at_the_right_program() {
    let dir = scratch("native");
    for args in [
        &["install", "alice/hello"][..],
        &["search", "clock"],
        &["uninstall", "x"],
        &["list"],
    ] {
        let o = godspeed(&dir, None, args);
        assert_eq!(code(&o), 22, "{args:?}");
        assert!(stderr(&o).contains("native package manager"));
    }
}

/// The whole first half of the workflow on a real OS checkout: new, check, build for every
/// architecture. Needs GODSPEED_OS, the pinned toolchain with its targets, and the network.
#[test]
#[ignore = "needs GODSPEED_OS, the pinned toolchain and network access"]
fn generated_service_builds_for_every_architecture() {
    let os = PathBuf::from(std::env::var("GODSPEED_OS").expect("set GODSPEED_OS to a GodspeedOS checkout"));
    let dir = scratch("e2e");
    assert_eq!(code(&godspeed(&dir, Some(&os), &["new", "service", "hello"])), 0);
    let p = dir.join("hello");
    let o = godspeed(&p, Some(&os), &["build", "--all"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    // (triple, ELF e_machine): each binary must be for the machine it claims, with a real entry point.
    for (triple, machine) in [
        ("x86_64-unknown-none", 0x3E),
        ("aarch64-unknown-none", 0xB7),
        ("armv7a-none-eabi", 0x28),
        ("riscv64imac-unknown-none-elf", 0xF3),
    ] {
        let elf = std::fs::read(p.join("target").join(triple).join("release/hello")).unwrap();
        assert_eq!(&elf[..4], b"\x7fELF", "{triple}: not an ELF");
        assert_eq!(elf[5], 1, "{triple}: every Godspeed port is little-endian");
        assert_eq!(
            u16::from_le_bytes([elf[18], elf[19]]),
            machine,
            "{triple}: wrong machine"
        );
        // An entry of 0 is what an unexported `service_main` produces: it links, then faults at boot.
        let entry = match elf[4] {
            1 => u64::from(u32::from_le_bytes(elf[24..28].try_into().unwrap())),
            2 => u64::from_le_bytes(elf[24..32].try_into().unwrap()),
            c => panic!("{triple}: unknown ELF class {c}"),
        };
        assert_ne!(entry, 0, "{triple}: entry point is 0");
    }
}

/// The library template on a real OS checkout: its tests run on the host, and it builds for every
/// architecture.
#[test]
#[ignore = "needs GODSPEED_OS, the pinned toolchain and network access"]
fn generated_library_tests_pass_and_it_builds_for_every_architecture() {
    let os = PathBuf::from(std::env::var("GODSPEED_OS").expect("set GODSPEED_OS to a GodspeedOS checkout"));
    let dir = scratch("e2e_library");
    assert_eq!(code(&godspeed(&dir, Some(&os), &["new", "library", "parser"])), 0);
    let p = dir.join("parser");
    let o = godspeed(&p, Some(&os), &["test"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let o = godspeed(&p, Some(&os), &["build", "--all"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    for triple in [
        "x86_64-unknown-none",
        "aarch64-unknown-none",
        "armv7a-none-eabi",
        "riscv64imac-unknown-none-elf",
    ] {
        assert!(
            p.join("target").join(triple).join("release/libparser.rlib").is_file(),
            "{triple}"
        );
    }
}

/// The packaging half on a real OS checkout: a generated service, given a publisher, packages for every
/// architecture, and the package verifies on its own.
#[test]
#[ignore = "needs GODSPEED_OS, the pinned toolchain and network access"]
fn generated_service_packages_and_the_package_verifies() {
    let os = PathBuf::from(std::env::var("GODSPEED_OS").expect("set GODSPEED_OS to a GodspeedOS checkout"));
    let dir = scratch("e2e_package");
    let o = godspeed(&dir, Some(&os), &["new", "service", "hello", "--publisher", "alice"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let p = dir.join("hello");

    let o = godspeed(&p, Some(&os), &["package"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let pkg_dir = p.join("target/package/alice-hello-0.1.0");
    let mut files: Vec<String> = std::fs::read_dir(&pkg_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    assert_eq!(
        files,
        [
            "aarch64.elf",
            "armv7.elf",
            "contract.toml",
            "manifest.toml",
            "riscv64.elf",
            "x86_64.elf"
        ]
    );
    assert!(
        !p.join("target/package/.alice-hello-0.1.0.partial").exists(),
        "staging is moved, not left"
    );

    let o = verify(&pkg_dir);
    assert_eq!(code(&o), 0, "{}", stderr(&o));

    // Packaging again replaces the previous package of this version in place.
    assert_eq!(code(&godspeed(&p, Some(&os), &["package", "--arch", "riscv64"])), 0);
    assert!(!pkg_dir.join("x86_64.elf").exists());
    assert_eq!(code(&verify(&pkg_dir)), 0);
}

// ---- self update and self uninstall -----------------------------------------------------------------

/// The release build `self update` looks for on this machine.
fn host_asset() -> String {
    let target = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", a) => format!("{a}-unknown-linux-musl"),
        ("macos", a) => format!("{a}-apple-darwin"),
        ("windows", a) => format!("{a}-pc-windows-msvc"),
        (os, a) => format!("{a}-{os}"),
    };
    format!("godspeed-{target}{}", std::env::consts::EXE_SUFFIX)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let d = godspeed_package::Sha256Digest::of(bytes).to_string();
    d.trim_start_matches("sha256:").to_string()
}

/// A stand-in for GitHub Releases on this machine: `releases/latest` points at `cli-v<latest>`, whose
/// downloads are `files`. Returns the address to put in GODSPEED_TEST_RELEASES_URL.
fn releases_server(latest: &str, files: Vec<(String, Vec<u8>)>) -> String {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/releases", listener.local_addr().unwrap());
    let latest = latest.to_string();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            let _ = reader.read_line(&mut request);
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) <= 2 {
                    break;
                }
            }
            let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
            let (status, extra, body): (&str, String, &[u8]) = if path == "/releases/latest" {
                let tag = format!("https://github.com/godspeedhq/godspeed-cli/releases/tag/cli-v{latest}");
                ("302 Found", format!("Location: {tag}\r\n"), b"")
            } else if let Some((_, b)) = files
                .iter()
                .find(|(n, _)| path == format!("/releases/download/cli-v{latest}/{n}"))
            {
                ("200 OK", String::new(), b)
            } else {
                ("404 Not Found", String::new(), b"")
            };
            let head = format!(
                "HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            if !request.starts_with("HEAD") {
                let _ = stream.write_all(body);
            }
        }
    });
    url
}

/// A copy of the CLI installed the way the installer installs it, under a home of its own. Returns the
/// home and the installed binary.
fn installed_copy(test: &str) -> (PathBuf, PathBuf) {
    let home = scratch(test);
    let bin = home.join(".godspeed").join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join(format!("godspeed{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(env!("CARGO_BIN_EXE_godspeed"), &exe).unwrap();
    (home, exe)
}

fn run_installed(home: &Path, exe: &Path, releases: Option<&str>, args: &[&str]) -> Output {
    let mut c = Command::new(exe);
    c.current_dir(home)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("CARGO_HOME", home.join(".cargo"))
        .env("GODSPEED_CONFIG_DIR", home.join("config"))
        .env("GODSPEED_CACHE_DIR", home.join("cache"))
        .env("GODSPEED_TEST_OS_URL", NO_OS_SERVER)
        .env_remove("GODSPEED_TEST_RELEASES_URL")
        .stdin(std::process::Stdio::null())
        .args(args);
    if let Some(r) = releases {
        c.env("GODSPEED_TEST_RELEASES_URL", r);
    }
    c.output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn self_update_says_unchanged_when_this_is_the_latest() {
    let (home, exe) = installed_copy("self_update_unchanged");
    let url = releases_server(env!("CARGO_PKG_VERSION"), vec![]);
    let o = run_installed(&home, &exe, Some(&url), &["self", "update"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(stdout(&o).contains(&format!("godspeed unchanged - {}", env!("CARGO_PKG_VERSION"))));
}

#[test]
fn self_update_replaces_nothing_when_the_checksum_does_not_match() {
    let (home, exe) = installed_copy("self_update_mismatch");
    let before = std::fs::read(&exe).unwrap();
    let asset = host_asset();
    let sums = format!("{}  {asset}\n", "0".repeat(64));
    let url = releases_server(
        "999.0.0",
        vec![
            (asset, b"not godspeed".to_vec()),
            ("SHA256SUMS".into(), sums.into_bytes()),
        ],
    );
    let o = run_installed(&home, &exe, Some(&url), &["self", "update"]);
    assert_eq!(code(&o), 17, "{}", stderr(&o));
    assert!(stderr(&o).contains("is not the file the release lists"));
    assert_eq!(std::fs::read(&exe).unwrap(), before, "the binary is untouched");
    assert_eq!(
        std::fs::read_dir(exe.parent().unwrap()).unwrap().count(),
        1,
        "nothing left beside it"
    );
}

#[test]
fn self_update_replaces_nothing_when_the_download_does_not_run() {
    let (home, exe) = installed_copy("self_update_broken");
    let before = std::fs::read(&exe).unwrap();
    let asset = host_asset();
    let body = b"verified, but not a program".to_vec();
    let sums = format!("{}  {asset}\n", sha256_hex(&body));
    let url = releases_server("999.0.0", vec![(asset, body), ("SHA256SUMS".into(), sums.into_bytes())]);
    let o = run_installed(&home, &exe, Some(&url), &["self", "update"]);
    assert_eq!(code(&o), 11, "{}", stderr(&o));
    assert_eq!(std::fs::read(&exe).unwrap(), before, "the binary is untouched");
    assert_eq!(
        std::fs::read_dir(exe.parent().unwrap()).unwrap().count(),
        1,
        "nothing left beside it"
    );
}

/// The whole swap, with a script standing in for the new release (Unix runs one as a program).
#[cfg(unix)]
#[test]
fn self_update_swaps_in_a_verified_release_that_runs() {
    let (home, exe) = installed_copy("self_update_swaps");
    let asset = host_asset();
    let body = b"#!/bin/sh\necho godspeed 999.0.0\n".to_vec();
    let sums = format!("{}  {asset}\n", sha256_hex(&body));
    let url = releases_server(
        "999.0.0",
        vec![(asset, body.clone()), ("SHA256SUMS".into(), sums.into_bytes())],
    );
    let o = run_installed(&home, &exe, Some(&url), &["self", "update"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(stdout(&o).contains("godspeed updated - 999.0.0 (from"));
    assert_eq!(std::fs::read(&exe).unwrap(), body);
    assert_eq!(std::fs::read_dir(exe.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn self_commands_leave_a_cargo_install_to_cargo() {
    let home = scratch("self_cargo");
    let bin = home.join(".cargo").join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join(format!("godspeed{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(env!("CARGO_BIN_EXE_godspeed"), &exe).unwrap();
    for args in [&["self", "update"][..], &["self", "uninstall", "--yes"]] {
        let o = run_installed(&home, &exe, None, args);
        assert_eq!(code(&o), 22, "{}", stderr(&o));
        assert!(stderr(&o).contains("installed by `cargo install`"));
    }
    assert!(exe.exists());
}

#[test]
fn self_uninstall_asks_first_and_changes_nothing_without_an_answer() {
    let (home, exe) = installed_copy("self_uninstall_asks");
    let o = run_installed(&home, &exe, None, &["self", "uninstall"]);
    assert_eq!(code(&o), 29, "{}", stderr(&o));
    assert!(stderr(&o).contains("Nothing was changed"));
    assert!(exe.exists());
}

#[test]
fn self_uninstall_removes_what_the_installer_added_and_keeps_the_keys() {
    let (home, exe) = installed_copy("self_uninstall");
    let config = home.join("config");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("signing-key.toml"), "kept").unwrap();
    let kit = home.join("cache").join("platform").join("0.28.0");
    std::fs::create_dir_all(&kit).unwrap();
    std::fs::write(home.join("cache").join("other-tool-data"), "not ours").unwrap();
    let profile = home.join(".profile");
    let line = "export PATH=\"$HOME/.godspeed/bin:$PATH\"  # added by the godspeed installer";
    std::fs::write(&profile, format!("export A=1\n\n{line}\n")).unwrap();

    let o = run_installed(&home, &exe, None, &["self", "uninstall", "--yes"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(stdout(&o).contains(&format!("keep    {}", config.display())));
    // Windows deletes the binary from a helper once the command has exited.
    let gone = (0..60).any(|_| {
        if home.join(".godspeed").exists() {
            std::thread::sleep(std::time::Duration::from_millis(250));
            false
        } else {
            true
        }
    });
    assert!(gone, "the binary and the installer's folders are removed");
    assert_eq!(
        std::fs::read_to_string(config.join("signing-key.toml")).unwrap(),
        "kept"
    );
    assert!(
        !home.join("cache").join("platform").exists(),
        "the cached kits are removed"
    );
    assert!(
        home.join("cache").join("other-tool-data").exists(),
        "and nothing else in the cache folder"
    );
    if cfg!(unix) {
        assert_eq!(std::fs::read_to_string(&profile).unwrap(), "export A=1\n");
    }
}

// ---- publishing against a stand-in registry ---------------------------------------------------------

/// What the stand-in registry's index says about alice/hello@0.1.0.
#[derive(Clone, Copy, PartialEq)]
enum IndexShows {
    Nothing,
    /// The manifest the CLI reserved with: the publish went through.
    Ours,
    /// Another manifest.
    Another,
}

#[derive(Default)]
struct FakeRegistry {
    index: Option<IndexShows>,
    /// The manifest the CLI last reserved with.
    manifest: Vec<u8>,
    /// Every request but the index: what the CLI asked the registry to do.
    calls: Vec<String>,
    /// Answer finalisation by closing the connection: the outcome nobody knows.
    drop_finalise: bool,
}

/// Enough of the registry protocol for `publish`, on this machine. Returns its address.
fn fake_registry(state: std::sync::Arc<std::sync::Mutex<FakeRegistry>>) -> String {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            let _ = reader.read_line(&mut request);
            let mut length = 0usize;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) <= 2 {
                    break;
                }
                if let Some((k, v)) = header.split_once(':') {
                    if k.eq_ignore_ascii_case("content-length") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                }
            }
            let mut body = vec![0u8; length];
            let _ = reader.read_exact(&mut body);
            let mut parts = request.split_whitespace();
            let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            let mut s = state.lock().unwrap();
            if path != "/index/alice/hello.toml" {
                s.calls.push(format!("{method} {path}"));
            }
            let reservation = r#"{"upload":"u1","expires":"2099-01-01T00:00:00Z","files":["contract.toml","x86_64.elf","aarch64.elf","armv7.elf","riscv64.elf","manifest.sig"],"staged":[]}"#;
            let (status, json) = match (method, path) {
                ("GET", "/v1/whoami") => (
                    "200 OK",
                    r#"{"account":"alice","publishers":["alice"],"token_expires":"2099-01-01T00:00:00Z"}"#.to_string(),
                ),
                ("POST", "/v1/packages/alice/hello/reservations") => {
                    s.manifest = body;
                    ("200 OK", reservation.to_string())
                }
                ("PUT", p) if p.starts_with("/v1/packages/alice/hello/0.1.0/uploads/u1/") => {
                    ("200 OK", reservation.to_string())
                }
                ("POST", "/v1/packages/alice/hello/0.1.0/uploads/u1/finalise") if s.drop_finalise => {
                    // No answer at all: the connection closes. Whether it published, nobody here says.
                    continue;
                }
                ("GET", "/index/alice/hello.toml") => match s.index {
                    None | Some(IndexShows::Nothing) => ("404 Not Found", String::new()),
                    Some(shows) => {
                        let digest = if shows == IndexShows::Ours {
                            sha256_hex(&s.manifest)
                        } else {
                            "0".repeat(64)
                        };
                        let index = format!(
                            "format = {}\nname = \"alice/hello\"\n\n[[release]]\nversion = \"0.1.0\"\n\
                             manifest = \"sha256:{digest}\"\npublished = \"2026-10-16T12:00:00Z\"\n\
                             published_by = \"alice\"\n",
                            godspeed_package::INDEX_FORMAT
                        );
                        ("200 OK", index)
                    }
                },
                _ => (
                    "404 Not Found",
                    r#"{"error":"not-found","message":"no such route"}"#.to_string(),
                ),
            };
            drop(s);
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
                    json.len()
                )
                .as_bytes(),
            );
        }
    });
    url
}

/// The unknown-outcome path end to end, and `--dry-run`, with a real build against a stand-in registry:
/// finalisation that gets no answer is reconciled from the index - published if the index shows this
/// manifest, "outcome unknown" (25) if it shows nothing - and a dry run asks the registry to do nothing.
#[test]
#[ignore = "needs GODSPEED_OS, the pinned toolchain and network access"]
fn publish_reconciles_an_unanswered_finalise_and_a_dry_run_uploads_nothing() {
    use std::sync::{Arc, Mutex};
    let os = PathBuf::from(std::env::var("GODSPEED_OS").expect("set GODSPEED_OS to a GodspeedOS checkout"));
    let dir = scratch("e2e_publish");
    let o = godspeed(&dir, Some(&os), &["new", "service", "hello", "--publisher", "alice"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let p = dir.join("hello");
    assert_eq!(code(&godspeed(&p, None, &["key", "generate"])), 0);

    let state = Arc::new(Mutex::new(FakeRegistry::default()));
    let url = fake_registry(state.clone());
    let publish = |extra: &[&str]| {
        let mut args = vec!["publish", "--registry", url.as_str()];
        args.extend_from_slice(extra);
        command(&p, Some(&os), &args)
            .env("GODSPEED_TOKEN", "gsp_test")
            .output()
            .unwrap()
    };
    let calls = |s: &Arc<Mutex<FakeRegistry>>| std::mem::take(&mut s.lock().unwrap().calls);

    // A dry run: authenticated and checked, nothing reserved or uploaded.
    let o = publish(&["--dry-run"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(stdout(&o).contains("Would publish alice/hello@0.1.0"), "{}", stdout(&o));
    assert_eq!(calls(&state), ["GET /v1/whoami"]);

    // A dry run that would collide with a different published 0.1.0.
    state.lock().unwrap().index = Some(IndexShows::Another);
    let o = publish(&["--dry-run"]);
    assert_eq!(code(&o), 24, "{}", stderr(&o));
    assert_eq!(calls(&state), ["GET /v1/whoami"]);

    // Finalisation unanswered, and the index does not show it: nobody knows yet, and the CLI says so.
    {
        let mut s = state.lock().unwrap();
        s.index = Some(IndexShows::Nothing);
        s.drop_finalise = true;
    }
    let o = publish(&[]);
    assert_eq!(code(&o), 25, "{}", stderr(&o));
    assert!(stderr(&o).contains("did not answer finalisation"), "{}", stderr(&o));
    assert!(stderr(&o).contains("It may still complete"), "{}", stderr(&o));
    assert!(calls(&state).iter().any(|c| c.ends_with("/finalise")));

    // Finalisation unanswered, but it went through: the index shows this manifest, so it is published.
    state.lock().unwrap().index = Some(IndexShows::Ours);
    let o = publish(&[]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(stderr(&o).contains("did not answer finalisation"), "{}", stderr(&o));
    assert!(stdout(&o).contains("Published alice/hello@0.1.0"), "{}", stdout(&o));
}

// ---- the platform kit: no OS checkout needed ---------------------------------------------------------

/// What the stand-in GodspeedOS release server saw.
#[derive(Default)]
struct OsServer {
    /// Paths asked for, in order.
    asked: Vec<String>,
    /// The release's `godspeed-platform.toml` and `SHA256SUMS-v0.28.0`, when it publishes them (`backlog/01`).
    platform: Option<(Vec<u8>, Vec<u8>)>,
}

/// A stand-in for GitHub: `releases/latest` is v0.28.0, the tag is commit `c0ffee...`, and `raw/<commit>/...`
/// serves the fixture OS. Returns the address for GODSPEED_TEST_OS_URL.
fn os_server(state: std::sync::Arc<std::sync::Mutex<OsServer>>) -> String {
    use std::io::{BufRead, BufReader, Write};
    const COMMIT: &str = "c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00";
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            let _ = reader.read_line(&mut request);
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) <= 2 {
                    break;
                }
            }
            let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
            state.lock().unwrap().asked.push(path.clone());
            let file = path
                .strip_prefix(&format!("/raw/{COMMIT}/"))
                .map(|rel| fixture_os().join(rel))
                .and_then(|p| std::fs::read(p).ok());
            let asset = {
                let s = state.lock().unwrap();
                match (path.as_str(), &s.platform) {
                    ("/releases/download/v0.28.0/godspeed-platform.toml", Some((p, _))) => Some(p.clone()),
                    ("/releases/download/v0.28.0/SHA256SUMS-v0.28.0", Some((_, sums))) => Some(sums.clone()),
                    _ => None,
                }
            };
            let (status, extra, body) = match path.as_str() {
                _ if asset.is_some() => ("200 OK", String::new(), asset.unwrap()),
                "/releases/latest" => (
                    "302 Found",
                    "Location: https://github.com/godspeedhq/godspeed-os/releases/tag/v0.28.0\r\n".to_string(),
                    Vec::new(),
                ),
                "/commits/v0.28.0" => ("200 OK", String::new(), COMMIT.as_bytes().to_vec()),
                _ => match file {
                    Some(b) => ("200 OK", String::new(), b),
                    None => ("404 Not Found", String::new(), Vec::new()),
                },
            };
            let head = format!(
                "HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            if !request.starts_with("HEAD") {
                let _ = stream.write_all(&body);
            }
        }
    });
    url
}

#[test]
fn without_a_checkout_the_platform_kit_is_fetched_once_and_cached() {
    use std::sync::{Arc, Mutex};
    let dir = scratch("kit");
    let state = Arc::new(Mutex::new(OsServer::default()));
    let url = os_server(state.clone());
    let with_server = |cwd: &Path, args: &[&str], url: &str| {
        command(cwd, None, args)
            .env("GODSPEED_CACHE_DIR", dir.join("cache"))
            .env("GODSPEED_TEST_OS_URL", url)
            .output()
            .unwrap()
    };

    // `new` with no checkout: the latest release, fetched from one commit, and said.
    let o = with_server(&dir, &["new", "service", "hello", "--publisher", "alice"], &url);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(stdout(&o).contains("(GodspeedOS v0.28.0)"), "{}", stdout(&o));
    assert!(
        stderr(&o).contains("Fetching the GodspeedOS 0.28.0 platform kit"),
        "{}",
        stderr(&o)
    );
    let asked = std::mem::take(&mut state.lock().unwrap().asked);
    // The release has no platform file (every release up to 0.28.0), so after asking for it, the six files.
    assert_eq!(
        asked[..3],
        [
            "/releases/latest",
            "/releases/download/v0.28.0/godspeed-platform.toml",
            "/commits/v0.28.0"
        ]
    );
    assert!(asked[3..].iter().all(|p| p.starts_with("/raw/c0ffee")), "{asked:?}");
    let kit = dir.join("cache/platform/0.28.0");
    assert!(kit.join("kit.toml").is_file() && kit.join("services/user.ld").is_file());

    // In the project, offline: the release the project records, from the cache, with no request at all.
    let p = dir.join("hello");
    let o = with_server(&p, &["check"], NO_OS_SERVER);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let o = with_server(&p, &["doctor", "--json"], NO_OS_SERVER);
    assert!(stdout(&o).contains("platform kit from c0ffee00c0ff"), "{}", stdout(&o));

    // A damaged cache is discarded and fetched again, never used.
    std::fs::write(kit.join("services/user.ld"), "tampered").unwrap();
    let o = with_server(&p, &["check"], NO_OS_SERVER);
    assert_eq!(code(&o), 13, "{}", stderr(&o));
    assert!(!kit.exists(), "the damaged kit is removed");
    let o = with_server(&p, &["check"], &url);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_eq!(
        std::fs::read(kit.join("services/user.ld")).unwrap(),
        std::fs::read(fixture_os().join("services/user.ld")).unwrap()
    );

    // A release that does not exist is refused by name.
    let o = with_server(&dir, &["new", "service", "other", "--os-release", "9.9.9"], &url);
    assert_eq!(code(&o), 28, "{}", stderr(&o));
    assert!(stderr(&o).contains("GodspeedOS v9.9.9 has no"), "{}", stderr(&o));
    assert!(!dir.join("other").exists());
}

/// The fixture OS as a release's `godspeed-platform.toml` would carry it, with this entry symbol and format.
fn fixture_platform_file(entry: &str, format: u32) -> String {
    let read = |rel: &str| {
        std::fs::read_to_string(fixture_os().join(rel))
            .unwrap()
            .replace("\r\n", "\n")
    };
    let toml_str = |s: &str| toml_string(s);
    let targets = [
        "x86_64-unknown-none",
        "armv7a-none-eabi",
        "aarch64-unknown-none",
        "riscv64imac-unknown-none-elf",
    ];
    let mut t = format!(
        "format = {format}\nos = \"0.28.0\"\ncommit = \"c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00\"\n\n\
         [stdlib]\ncrate = \"godspeed\"\ntag = \"v0.28.0\"\n\n\
         [toolchain]\nchannel = \"nightly-2026-05-16\"\ntargets = [{}]\n\n\
         [link]\nentry = \"{entry}\"\nscript = \"user.ld\"\n\n[rustflags]\n",
        targets
            .iter()
            .map(|t| format!("\"{t}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    for target in targets {
        t.push_str(&format!("\"{target}\" = [\"-C\", \"relocation-model=static\"]\n"));
    }
    t.push_str(&format!(
        "\n[files]\n\"user.ld\" = {}\n\"service.schema.json\" = {}\n",
        toml_str(&read("services/user.ld")),
        toml_str(&read("contracts/schema/service.schema.json"))
    ));
    t
}

/// A TOML basic string.
fn toml_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn sums_for(file: &str, extra_digest: Option<&str>) -> Vec<u8> {
    let d = godspeed_package::Sha256Digest::of(file.as_bytes()).to_string();
    let hex = extra_digest.unwrap_or(d.trim_start_matches("sha256:"));
    format!("{hex}  godspeed-platform.toml\n0000  godspeed-os-v0.28.0-x86_64.img.xz\n").into_bytes()
}

#[test]
fn a_release_with_a_platform_file_is_read_from_it_and_checked() {
    use std::sync::{Arc, Mutex};
    let state = Arc::new(Mutex::new(OsServer::default()));
    let url = os_server(state.clone());
    let run = |dir: &Path, cwd: &Path, args: &[&str]| {
        command(cwd, None, args)
            .env("GODSPEED_CACHE_DIR", dir.join("cache"))
            .env("GODSPEED_TEST_OS_URL", &url)
            .output()
            .unwrap()
    };

    // The file, checked against SHA256SUMS: no build file of the OS repository is asked for, and the entry
    // symbol is the file's, not the CLI's own default.
    let dir = scratch("platform-file");
    let file = fixture_platform_file("start_here", 1);
    state.lock().unwrap().platform = Some((file.clone().into_bytes(), sums_for(&file, None)));
    let o = run(&dir, &dir, &["new", "service", "hello", "--publisher", "alice"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let asked = std::mem::take(&mut state.lock().unwrap().asked);
    assert!(asked.iter().all(|p| !p.starts_with("/raw/")), "{asked:?}");
    assert!(asked.iter().any(|p| p.ends_with("/SHA256SUMS-v0.28.0")), "{asked:?}");
    let kit = dir.join("cache/platform/0.28.0");
    assert!(kit.join("godspeed-platform.toml").is_file() && kit.join("services/user.ld").is_file());
    assert_eq!(
        std::fs::read_to_string(kit.join("services/user.ld")).unwrap(),
        std::fs::read_to_string(fixture_os().join("services/user.ld"))
            .unwrap()
            .replace("\r\n", "\n")
    );
    let main = std::fs::read_to_string(dir.join("hello/src/main.rs")).unwrap();
    assert!(
        main.contains("fn start_here") && !main.contains("service_main"),
        "{main}"
    );
    let o = run(&dir, &dir.join("hello"), &["check"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));

    // A file that is not the one SHA256SUMS lists is refused, and nothing is kept.
    let dir = scratch("platform-file-tampered");
    state.lock().unwrap().platform = Some((file.clone().into_bytes(), sums_for(&file, Some(&"ab".repeat(32)))));
    let o = run(&dir, &dir, &["new", "service", "hello", "--publisher", "alice"]);
    assert_eq!(code(&o), 17, "{}", stderr(&o));
    assert!(
        stderr(&o).contains("is not the file SHA256SUMS-v0.28.0 lists"),
        "{}",
        stderr(&o)
    );
    assert!(!dir.join("cache/platform/0.28.0").exists() && !dir.join("hello").exists());

    // A newer format than this CLI reads is refused, with the way forward, never guessed at.
    let dir = scratch("platform-file-newer");
    let newer = fixture_platform_file("service_main", 2);
    state.lock().unwrap().platform = Some((newer.clone().into_bytes(), sums_for(&newer, None)));
    let o = run(&dir, &dir, &["new", "service", "hello", "--publisher", "alice"]);
    assert_eq!(code(&o), 11, "{}", stderr(&o));
    assert!(stderr(&o).contains("platform-file format 2"), "{}", stderr(&o));
    assert!(stderr(&o).contains("godspeed self update"), "{}", stderr(&o));
    assert!(!dir.join("cache/platform/0.28.0").exists());
}
