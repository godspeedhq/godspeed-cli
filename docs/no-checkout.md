# Installing the CLI should be enough: removing the OS checkout requirement

**Status: BUILT, both options. Option A 2026-10-16 (`src/kit.rs`, released in cli-v0.2.0), as the bridge
the maintainer chose; option B approved 2026-10-17 and released: GodspeedOS v0.30.0 publishes
`godspeed-platform.toml` and cli-v0.4.0 reads it (`backlog/fixed/01`). A release without the file still uses
A. Default release for `godspeed new`: the latest.**

What was built differs from the sketch below in two details: the cache is the platform's cache directory
(`~/.cache/godspeed/platform/<release>/`, `%LOCALAPPDATA%` on Windows), not `~/.godspeed`; and the tag is
resolved to its commit first, so all of a kit's files come from one snapshot even if the tag moves. A
library records no OS release (it has no stdlib dependency), so it uses the newest cached kit whose
toolchain matches its pin, or the latest release.

## The problem

`godspeed` installs in one line (`install/install.sh`, `install/install.ps1`). Before cli-v0.2.0, every
command but `verify` then refused until it was told where a clone of the GodspeedOS repository is (`--os` or
`GODSPEED_OS`).
A newcomer has to clone a 1,000-file repository to read five small files out of it. That fails the Stranger
Test before `godspeed new` has run.

What the CLI reads from the checkout (`src/platform.rs`, the only place that does):

| fact | file | size |
|---|---|---|
| the OS version | `Cargo.toml`, `[workspace.package] version` | one line |
| the toolchain pin and the targets the OS builds | `rust-toolchain.toml` | a few lines |
| per-target compiler flags | `.cargo/config.toml` | a few lines |
| the service linker script | `services/user.ld` | ~40 lines |
| the contract schema | `contracts/schema/service.schema.json` | one file |

The stdlib itself is already fetched by Cargo from the release tag; it never came from the checkout.

## The proposal: a platform kit per OS release, fetched and cached by the CLI

    godspeed new service hello          # no --os, no GODSPEED_OS
      -> resolves the OS release (the latest, or --os-release 0.28.0)
      -> fetches that release's platform kit, verifies it, caches it in ~/.godspeed/platform/0.28.0/
      -> generates the project against it

- The kit is exactly the five facts above, for one OS release.
- It is cached per release and never refetched; a cache entry that fails its checksum is discarded and
  fetched again (CLAUDE.md, Caching: "a corrupt or incomplete cache must be recoverable").
- A project already records its OS release (the stdlib tag in `Cargo.toml`), so `build` and `check` use
  that release's kit, never "whatever is newest".
- `--os <path>` keeps working and wins: someone developing the OS itself builds against their own tree.
- Offline with a cached kit: works. Offline without one: refuses, saying what it needs and where from.

### Option A - fetch the files from the release tag (no OS change)

`https://raw.githubusercontent.com/godspeedhq/godspeed-os/v0.28.0/<path>` for each of the five files.
Works today, with nothing changed in the OS.

But it makes the CLI depend on file paths inside the OS repository, which are implementation details: the
OS moving `services/user.ld` would break every CLI. And a git tag can be moved, so "the files at v0.28.0" is
weaker than it reads.

### Option B - the OS publishes the kit as a release asset (recommended)

The OS's release workflow, which already builds and publishes images, also attaches
`godspeed-platform.toml` - one TOML file carrying the version, the toolchain pin, the per-target flags, the
schema and the linker script - with its SHA-256 in the release's checksums.

That turns five internal files into **one declared developer contract**, versioned with the OS: the CLI
then consumes published facts, not private files, which is what `CLAUDE.md` asks of it. It is the same change the developer-services proposal (`docs/developer-services-design.md`
in the OS repository, section 3.6) makes for the linker script, generalised to all five facts. The CLI's
side stays in `src/platform.rs`.

**Recommendation: B, with A as the bridge.** Build the cache and the "no --os needed" path now on option A,
behind `src/platform.rs`; switch the fetch to B's asset when the OS publishes it, and drop A then.

## Decisions, as the maintainer took them

1. Option A as a bridge now: yes (2026-10-16).
2. The OS's release workflow publishes `godspeed-platform.toml` (option B): approved 2026-10-17, first
   published with GodspeedOS v0.30.0.
3. Default OS release for `godspeed new` with no flag: the latest published release.
