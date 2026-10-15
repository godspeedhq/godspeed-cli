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

/// The CLI, isolated from the developer's own setup: no OS checkout, registry or token from the environment,
/// and a configuration directory of its own, so no test reads or writes real credentials.
fn command(dir: &Path, os: Option<&Path>, args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_godspeed"));
    c.current_dir(dir)
        .env_remove("GODSPEED_OS")
        .env_remove("GODSPEED_REGISTRY")
        .env_remove("GODSPEED_TOKEN")
        .env_remove("GODSPEED_SIGNING_KEY")
        .env("GODSPEED_CONFIG_DIR", dir.join(".godspeed-config"));
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
fn kinds_the_os_cannot_run_are_refused_with_the_reason() {
    let dir = scratch("kinds");
    let os = fixture_os();
    for (kind, mentions) in [
        ("utility", "terminating task"),
        ("app", "application interfaces"),
        ("driver", "device"),
    ] {
        let o = godspeed(&dir, Some(&os), &["new", kind, "x"]);
        assert_eq!(code(&o), 20, "{kind}");
        assert!(stderr(&o).contains(mentions), "{kind}: {}", stderr(&o));
        assert!(!dir.join("x").exists(), "{kind}: a refused kind must write nothing");
    }
    let o = godspeed(&dir, Some(&os), &["new", "widget", "x"]);
    assert_eq!(code(&o), 10);
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
fn without_an_os_checkout_the_cli_says_how_to_name_one() {
    let dir = scratch("no_os");
    let o = godspeed(&dir, None, &["new", "service", "hello"]);
    assert_eq!(code(&o), 11);
    assert!(stderr(&o).contains("GODSPEED_OS"));
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
fn account_commands_need_a_registry_and_say_how_to_name_one() {
    let dir = scratch("no_registry");
    for verb in ["login", "logout", "whoami"] {
        let o = godspeed(&dir, None, &[verb]);
        assert_eq!(code(&o), 11, "{verb}: {}", stderr(&o));
        assert!(stderr(&o).contains("--registry <url>"), "{verb}: {}", stderr(&o));
    }
}

#[test]
fn a_registry_is_reached_over_https_unless_it_is_on_this_machine() {
    let dir = scratch("plain_http");
    let o = godspeed(&dir, None, &["whoami", "--registry", "http://registry.example"]);
    assert_eq!(code(&o), 11, "{}", stderr(&o));
    assert!(stderr(&o).contains("HTTPS"), "{}", stderr(&o));
}

#[test]
fn login_refuses_what_is_not_a_token_without_contacting_anyone() {
    let dir = scratch("bad_token");
    // 127.0.0.1:9 (discard) is never a registry: had the CLI tried to contact it, the error would differ.
    let o = godspeed_with_input(&dir, &["login", "--registry", "http://127.0.0.1:9"], "hunter2\n");
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
        &["login", "--registry", "http://127.0.0.1:9"],
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
    assert_eq!(code(&godspeed(&dir, Some(&os), &["new", "service", "hello"])), 0);
    let p = dir.join("hello");
    let manifest = std::fs::read_to_string(p.join("godspeed.toml")).unwrap();
    std::fs::write(
        p.join("godspeed.toml"),
        manifest.replace("# publisher = \"your-name\"", "publisher = \"alice\""),
    )
    .unwrap();

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
