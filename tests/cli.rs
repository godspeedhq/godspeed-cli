//! The CLI's promises, tested through the binary.
//!
//! Offline tests run against `tests/fixtures/os`, the parts of a GodspeedOS checkout the CLI reads. The
//! end-to-end build test needs a real checkout, the pinned toolchain and the network, so it is
//! `#[ignore]`d and run deliberately:
//!
//!     GODSPEED_OS=<path to a GodspeedOS checkout> cargo test -- --ignored

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

fn godspeed(dir: &Path, os: Option<&Path>, args: &[&str]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_godspeed"));
    c.current_dir(dir).env_remove("GODSPEED_OS");
    if let Some(os) = os {
        c.arg("--os").arg(os);
    }
    c.args(args).output().unwrap()
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
fn registry_and_packaging_are_not_implemented_and_say_so() {
    let dir = scratch("registry");
    for verb in ["package", "publish", "login", "logout", "whoami"] {
        let o = godspeed(&dir, None, &[verb]);
        assert_eq!(code(&o), 21, "{verb}");
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
    for triple in [
        "x86_64-unknown-none",
        "aarch64-unknown-none",
        "armv7a-none-eabi",
        "riscv64imac-unknown-none-elf",
    ] {
        let elf = std::fs::read(p.join("target").join(triple).join("release/hello")).unwrap();
        assert_eq!(&elf[..4], b"\x7fELF", "{triple}");
    }
}
