# godspeed-cli

`godspeed`, the developer CLI for [GodspeedOS](https://github.com/godspeedhq/godspeed-os): create, check,
build, package, sign and publish Godspeed software on your workstation, against the public contracts of a
GodspeedOS release - and fetch anyone's published package with every signature checked.

    godspeed new service hello && cd hello
    godspeed build --all          # one binary per architecture: x86_64, aarch64, armv7, riscv64
    godspeed publish              # signed, uploaded, and installable as you/hello

**Guide and examples: [godspeedhq.github.io/godspeed-cli](https://godspeedhq.github.io/godspeed-cli/)**
(source in [`site/`](site/)). The rules the CLI is built to are in [CLAUDE.md](CLAUDE.md).

**Status:** the first half of the workflow works end to end - create, check, build, test (libraries),
package, sign, publish, fetch - against the live registry. Installing and running on a Godspeed machine
waits on the OS ([below](#what-works-and-what-cannot-yet)). Godspeed programs are written in Rust today
([why, and what about C, C++ or Go](#other-languages)).

## Install

**No release has been cut yet**, so the installers below have nothing to fetch until the first
`cli-v*` release; until then, build it with Rust:

    cargo install --git https://github.com/godspeedhq/godspeed-cli --branch feat/cli-foundation --locked godspeed-cli

Once released - Linux or macOS:

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

**Updating:** run the same installer again. It fetches the latest release, checks it the same way, and
swaps it in for the old binary only once it is verified. (`GODSPEED_VERSION` pins a version, including an
older one.) The CLI does not update itself, but it tells you when there is something to update to: at
most once a day it asks GitHub which release is the latest - in the background, never slowing or failing
a command - and if it is newer, says so after the command's output. It sends nothing but that request;
it stays quiet in scripts, pipes and CI; `GODSPEED_NO_UPDATE_CHECK=1` turns it off; and `godspeed
doctor` asks on demand.

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

## What you can create

    godspeed new <kind> <name>

| kind | what it is | today |
|---|---|---|
| `service` | a managed program with a lifecycle that requests its authority in a contract - every runnable Godspeed program is one | **works**: `godspeed new service clock` |
| `library` | reusable `no_std` Rust for other Godspeed projects; not run on its own, and its tests run on your workstation | **works**: `godspeed new library parser` |
| `utility` | a program that does one job and ends | refused: GodspeedOS has no terminating task yet (no `fn main()`, no exit) |
| `app` | a user-facing program | refused: GodspeedOS has no public application interfaces yet |
| `driver` | a userspace driver for a device | refused: a driver built outside the OS tree has no way yet to be granted a device |

A kind is offered only once GodspeedOS can run it. Ask for one it cannot and the CLI says what is missing,
where the OS tracks it, and what to make instead (exit 20) - never a template that pretends. The
[examples](examples/) show what a service and a library can do today.

## Use

    godspeed new service hello
    cd hello
    godspeed check
    godspeed build                 # x86_64
    godspeed build --arch riscv64  # aarch64, armv7, riscv64
    godspeed build --all
    godspeed package               # needs publisher = "<you>" in godspeed.toml
    godspeed verify target/package/<you>-hello-0.1.0

It is an ordinary Rust project, so `cargo fmt` and `cargo clippy` work in it as usual.

[`examples/`](examples/) has real services and a library to start from - a periodic logger, a
request/reply server, a service that keeps state in a file, and a no-heap text buffer - each built for
every architecture in CI and explained on the [examples page](https://godspeedhq.github.io/godspeed-cli/examples.html).

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
| `fetch <publisher/name>[@version]` | works: downloads a release and verifies it through the registry's whole chain of trust before keeping it |
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
    godspeed login --registry <url>    # makes your token; send the FINGERPRINT it prints to the maintainer
    godspeed whoami                    # once they approve it: your account and namespaces
    godspeed publish                   # needs publisher = "<you>" in godspeed.toml

Your token is made on your machine, like an SSH key, and never leaves it: the maintainer approves only its
fingerprint (its SHA-256), which is all the registry stores. `login` keeps it in
`~/.config/godspeed/credentials.toml` (`%APPDATA%\godspeed` on Windows), readable by you alone; running it
again is safe - it reports whether the token is still waiting, and makes a new one only if the old can never
work again. For CI, `godspeed token new` prints a token once and stores nothing: put it in a CI secret as
`GODSPEED_TOKEN`, and send the fingerprint as usual. The CLI holds a publisher token only - never storage
credentials.

The official registry ([godspeed-registry](https://github.com/godspeedhq/godspeed-registry)) is live at

    https://godspeed-registry.bankole-422.workers.dev

That address is temporary - it moves to a custom domain - so the CLI has no built-in default yet: name it
with `--registry`, `GODSPEED_REGISTRY`, or once with `login`. Publishers and their namespaces are approved
by hand for now, by the registry's maintainer. Its trust root, to pin on first contact:

    godspeed fetch godspeed/hello --registry <url> \
      --trust-root sha256:d769d1a64c3105c8da0ffcc2f54aba1c3a16ce4ae707a6d206556f08824a146d

Every release is signed with your key (`manifest.sig`), and the registry refuses one that is not signed by a
key registered for your namespace. Your secret key stays in `signing-key.toml` beside the credentials,
readable by you alone - back it up; in CI, set `GODSPEED_SIGNING_KEY` instead. A signature proves which key
signed; which keys may sign for a publisher is the registry's offline-signed namespaces metadata, which even
the registry cannot alter. `godspeed fetch alice/hello` checks all of it the way a Godspeed machine will
before installing: root (trusted on first contact - pin it with `--trust-root`), timestamp, snapshot,
namespaces, index, manifest, signature, files ([docs/trust-metadata.md](docs/trust-metadata.md)).

## Other languages

**Today a Godspeed program is Rust, and the CLI builds Rust.** Not because the CLI insists - the package
format is just a contract and one ELF per architecture, and does not care what compiled them - but because
the only public way to talk to GodspeedOS is its Rust standard library, `gs`. A program in another
language would need that interface in a form its compiler can call, and GodspeedOS does not provide one
yet:

- **C and C++** would need a C ABI over `gs` (a header and a static library) and a freestanding toolchain
  per architecture. That is a plausible addition, it is the OS's to define, and the CLI would then gain a
  second build backend without any change to packages or publishing.
- **Go** would need much more: a port of the Go runtime (its scheduler, garbage collector and system
  interface) to GodspeedOS, which has no heap for services and no POSIX layer.

Until the OS defines one of those, the CLI does not pretend: it builds Rust projects, and a C, C++ or Go
"template" would be one that cannot do what it promises.

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
| 27 | untrusted: the registry's signed metadata does not vouch for it - an unknown, retired or revoked key, or metadata that is unsigned, expired, rolled back or inconsistent |
| 28 | not found: the registry has no such package, or no such version of it |
| 30 | a local file could not be read or written |

## Development

    cargo test                                                     # offline, against tests/fixtures/os
    GODSPEED_OS=/path/to/godspeed-os cargo test -- --ignored       # real build for every architecture

The ignored tests need the OS's pinned toolchain with its four targets, and network access to fetch the
stdlib at its release tag.

CI (`.github/workflows/ci.yml`) runs both: formatting, lints and the offline tests on Linux and Windows,
then every template generated and built against each supported GodspeedOS release, for every
architecture that release builds, with `godspeed doctor` run first as the gate a developer would hit.
