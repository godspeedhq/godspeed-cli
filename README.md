# godspeed-cli

`godspeed`, the developer CLI for [GodspeedOS](https://github.com/godspeedhq/godspeed-os): create, check
and build Godspeed projects on your workstation, against the public contracts of a GodspeedOS release.

The rules it is built to are in [CLAUDE.md](CLAUDE.md).

## Install

Linux or macOS:

    curl --proto '=https' --tlsv1.2 -sSf https://github.com/godspeedhq/godspeed-cli/releases/latest/download/install.sh | sh

Windows (PowerShell):

    irm https://github.com/godspeedhq/godspeed-cli/releases/latest/download/install.ps1 | iex

Either installs one prebuilt binary to `~/.godspeed/bin` (`%USERPROFILE%\.godspeed\bin` on Windows), after
checking it against the release's `SHA256SUMS` - a mismatch, or no way to check, installs nothing - and
adds that folder to your PATH, saying which files it changed. Both scripts are short and say at the top
what they do; read them first if you like (`install/`). Settings: `GODSPEED_VERSION`,
`GODSPEED_INSTALL_DIR`, `GODSPEED_NO_MODIFY_PATH=1`. Builds exist for x86_64 and Arm64 Linux (static, any
distribution), macOS on Apple silicon and Intel, and x64 and Arm64 Windows.

(Builds of the CLI for armv7 and riscv64 Linux hosts - a Raspberry Pi on a 32-bit OS, a VisionFive on
Debian - would be cheap to add: two more static cross-builds. They are left out because GitHub has no
runners for either, so they could only be tested under emulation. Packages, the programs the CLI builds,
target all four Godspeed architectures regardless.)

Or build it yourself with Rust:

    cargo install --git https://github.com/godspeedhq/godspeed-cli --locked godspeed-cli

A checksum proves a download is the file the release lists; releases are not signed yet.

## Setup

    git clone https://github.com/godspeedhq/godspeed-os
    export GODSPEED_OS=/path/to/godspeed-os      # or pass --os <path> to every command
    godspeed doctor

The CLI builds against the OS checkout you name: its stdlib release, its pinned Rust toolchain, its
target list and its contract schema. It reads that checkout and never writes to it. `godspeed doctor`
says what is missing and the command that fixes it. (Needing a checkout at all is temporary:
[docs/no-checkout.md](docs/no-checkout.md).)

## Releasing

Push a tag `cli-v<version>`, equal to the version in `Cargo.toml`. `.github/workflows/release.yml` builds
all six binaries, runs each where it can, tests both installers against exactly those binaries on Linux,
macOS and Windows, and then creates a **draft** release. Nothing is public until the draft is published;
the installers' `releases/latest` never resolves to a draft.

## Use

    godspeed new service hello
    cd hello
    godspeed check
    godspeed build                 # x86_64
    godspeed build --arch riscv64  # aarch64, armv7, riscv64
    godspeed build --all
    godspeed package               # needs publisher = "<you>" in godspeed.toml
    godspeed verify target/package/<you>-hello-0.1.0

## What works, and what cannot yet

| command | status |
|---|---|
| `new service`, `new library` | works |
| `new utility`, `new app`, `new driver` | refused: GodspeedOS cannot run that kind yet, and the refusal says why |
| `check` | works: the project, its toolchain pin, its stdlib release, and the contract against the OS's schema |
| `build [--arch <isa> \| --all]` | works: one ELF per architecture for a service, an rlib for a library |
| `test` | works for a library (on the host); refused for a service (see below) |
| `doctor` | works |
| `package [--arch <isa>] [--out <dir>]` | works: builds every architecture, writes a package directory, and verifies it from disk before reporting it |
| `verify <dir>` | works, with no OS checkout: checks every file against the manifest, and says it proves integrity only |
| `run` | refused: GodspeedOS has no public way to run a service built outside its tree |
| `key generate`, `key show` | work: an Ed25519 signing key, made and kept on this machine; only the public half is ever shared |
| `sign <dir>` | works: signs a package directory (`manifest.sig`) and verifies the result; `publish` does it for you |
| `login`, `whoami`, `logout` | work: a publisher token for a registry, checked with it before it is stored; `logout` revokes it there |
| `publish` | works: packages every architecture, then reserves, uploads and finalises; if finalising gets no answer it reads the registry's index and reports what it finds, and running it again is always safe |
| `install`, `search`, `info`, `list`, `update`, `uninstall` | belong to the native package manager on a Godspeed machine, and say so |

Running a service needs an OS build interface that takes a crate from outside the OS tree. Until
GodspeedOS has one, `godspeed run` says so rather than editing the OS for you.

## A project

    godspeed.toml            what kind of project this is, where its contract is, and the publisher,
                             description and license a package of it carries
    Cargo.toml               how Rust builds it; depends on the `godspeed` stdlib at an OS release tag
    contracts/<name>.toml    (services) the OS service contract: the authority the service REQUESTS
    rust-toolchain.toml      the compiler that OS release is built with
    src/

The contract grants nothing. What a service is actually given is the operating system's decision.

## Publishing

    godspeed key generate              # once; send the PUBLIC key it prints to the registry maintainer
    godspeed login --registry <url>    # paste the token the registry maintainer issued you
    godspeed whoami
    godspeed publish                   # needs publisher = "<you>" in godspeed.toml

The registry is the `godspeed-registry` service; there is no default until the official one is deployed,
so name it with `--registry`, `GODSPEED_REGISTRY`, or once with `login`. `login` stores the token in
`~/.config/godspeed/credentials.toml` (`%APPDATA%\godspeed` on Windows), readable by you alone; in CI, set
`GODSPEED_TOKEN` instead and nothing is stored. The CLI holds a publisher token only - never storage
credentials.

Every release is signed with your key (`manifest.sig`), and the registry refuses one that is not signed by a
key registered for your namespace. Your secret key stays in `signing-key.toml` beside the credentials,
readable by you alone - back it up; in CI, set `GODSPEED_SIGNING_KEY` instead. A signature proves which key
signed; that the key is the one registered for a publisher is, for now, the registry's own record - the
signed trust anchor that lets clients check it independently is the next step
([docs/publishing-design.md](docs/publishing-design.md), section 6).

## The package contract

`crates/godspeed-package` is what a Godspeed package is: the manifest's types and rules, and streaming
artifact verification. It is shared with the native package manager, so it is `no_std` and never
allocates (CI builds it for all four Godspeed targets and checks its dependency tree); reading and writing
the manifest's TOML is a host-only feature. The format, and what it deliberately leaves out, is in
[docs/package-format.md](docs/package-format.md). `godspeed package` produces packages with it and
`godspeed verify` checks them.

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
| 16 | invalid package: a missing or extra file, a manifest that breaks the format |
| 17 | integrity failure: a size or checksum mismatch |
| 18 | authentication failure: not logged in, or the token is unknown, revoked or expired |
| 19 | the registry could not be reached, before anything was published |
| 20 | GodspeedOS does not provide this yet |
| 21 | this CLI does not implement this yet |
| 22 | this is the native package manager's command, not this CLI's |
| 23 | permission denied: the namespace is not yours |
| 24 | conflict: the version is already published, or being published |
| 25 | outcome unknown: finalising got no answer and the index does not show it yet - run `publish` again |
| 26 | signature failure: a signature is malformed, or the named key did not sign this manifest |
| 30 | a local file could not be read or written |

## Development

    cargo test                                                     # offline, against tests/fixtures/os
    GODSPEED_OS=/path/to/godspeed-os cargo test -- --ignored       # real build for every architecture

The ignored tests need the OS's pinned toolchain with its four targets, and network access to fetch the
stdlib at its release tag.

CI (`.github/workflows/ci.yml`) runs both: formatting, lints and the offline tests on Linux and Windows,
then every template generated and built against each supported GodspeedOS release, for every
architecture that release builds, with `godspeed doctor` run first as the gate a developer would hit.
