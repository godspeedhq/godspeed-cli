# godspeed-cli

`godspeed`, the developer CLI for [GodspeedOS](https://github.com/godspeedhq/godspeed-os): create, check
and build Godspeed projects on your workstation, against the public contracts of a GodspeedOS release.

The rules it is built to are in [CLAUDE.md](CLAUDE.md).

## Setup

    git clone https://github.com/godspeedhq/godspeed-os
    export GODSPEED_OS=/path/to/godspeed-os      # or pass --os <path> to every command
    cargo install --path .                       # from this repository
    godspeed doctor

The CLI builds against the OS checkout you name: its stdlib release, its pinned Rust toolchain, its
target list and its contract schema. It reads that checkout and never writes to it. `godspeed doctor`
says what is missing and the command that fixes it.

## Use

    godspeed new service hello
    cd hello
    godspeed check
    godspeed build                 # x86_64
    godspeed build --arch riscv64  # aarch64, armv7, riscv64
    godspeed build --all

## What works, and what cannot yet

| command | status |
|---|---|
| `new service`, `new library` | works |
| `new utility`, `new app`, `new driver` | refused: GodspeedOS cannot run that kind yet, and the refusal says why |
| `check` | works: the project, its toolchain pin, its stdlib release, and the contract against the OS's schema |
| `build [--arch <isa> \| --all]` | works: one ELF per architecture for a service, an rlib for a library |
| `test` | works for a library (on the host); refused for a service (see below) |
| `doctor` | works |
| `run` | refused: GodspeedOS has no public way to run a service built outside its tree |
| `package`, `publish`, `login`, `logout`, `whoami` | not implemented: the package format and registry protocol are not defined yet |
| `install`, `search`, `info`, `list`, `update`, `uninstall` | belong to the native package manager on a Godspeed machine, and say so |

Running a service needs an OS build interface that takes a crate from outside the OS tree. Until
GodspeedOS has one, `godspeed run` says so rather than editing the OS for you.

## A project

    godspeed.toml            what kind of project this is, and where its contract is
    Cargo.toml               how Rust builds it; depends on the `godspeed` stdlib at an OS release tag
    contracts/<name>.toml    (services) the OS service contract: the authority the service REQUESTS
    rust-toolchain.toml      the compiler that OS release is built with
    src/

The contract grants nothing. What a service is actually given is the operating system's decision.

## Exit codes

Scripts and CI can tell failures apart without reading the text:

| code | meaning |
|---|---|
| 0 | success |
| 2 | the command line itself was wrong |
| 10 | invalid project |
| 11 | environment: no OS checkout, toolchain or target missing |
| 12 | unavailable target or architecture |
| 13 | a dependency could not be fetched |
| 14 | compilation failure |
| 15 | test failure |
| 20 | GodspeedOS does not provide this yet |
| 21 | this CLI does not implement this yet |
| 22 | this is the native package manager's command, not this CLI's |
| 30 | a local file could not be read or written |

## Development

    cargo test                                                     # offline, against tests/fixtures/os
    GODSPEED_OS=/path/to/godspeed-os cargo test -- --ignored       # real build for every architecture

The ignored tests need the OS's pinned toolchain with its four targets, and network access to fetch the
stdlib at its release tag.

CI (`.github/workflows/ci.yml`) runs both: formatting, lints and the offline tests on Linux and Windows,
then every template generated and built against each supported GodspeedOS release, for every
architecture that release builds, with `godspeed doctor` run first as the gate a developer would hit.
