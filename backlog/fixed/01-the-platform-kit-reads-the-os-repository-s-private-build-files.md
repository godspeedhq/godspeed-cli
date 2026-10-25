# 1. The platform kit reads six of the OS repository's private build files, and hard-codes a seventh fact

**Status: FIXED 2026-10-24 (closed in place; the condition below was met with GodspeedOS v0.30.0 and
cli-v0.4.0, both published 2026-10-23). Recorded 2026-10-17. Option A (the bridge) was built and released in
`cli-v0.2.0`. Option B was APPROVED by the maintainer on 2026-10-17 (the OS's
`docs/developer-services-design.md` 11.5) and built on both sides. It was to close when a released CLI had
read the file from a published OS release.**

## What is wrong

Since 0.2.0, the CLI needs no clone of GodspeedOS (`docs/no-checkout.md`). The first time it needs a
release, `src/kit.rs` fetches these files from the release's tag, resolved first to one commit, and caches
them with their digests:

| fact | file in the OS repository |
|---|---|
| the OS version | `Cargo.toml` |
| the stdlib crate is `godspeed` | `stdlib/rust/Cargo.toml` |
| the toolchain pin and targets | `rust-toolchain.toml` |
| per-target compiler flags | `.cargo/config.toml` |
| the service linker script | `services/user.ld` |
| the contract schema | `contracts/schema/service.schema.json` |

Those are the OS's own build inputs, not a contract. **The OS moving or reshaping any of them breaks
every installed CLI** for every release after the move, and nothing in the OS repository would notice.

A seventh fact is in none of them: **the entry symbol**. Every OS service's `build.rs` passes
`--entry=service_main` (27 of 27, counted 2026-10-17), so the CLI cannot fetch it and hard-codes it:

- `src/cmd/build.rs` and `src/cmd/check.rs`;
- `templates/service/src/main.rs` and `templates/service/README.md`.

Renaming the entry point in the OS would build and boot every OS service and break every service the CLI
builds, with nothing on either side to say so.

## Evidence

- `src/kit.rs`, `FILES`: the six paths.
- `src/platform.rs`, module docs: "Each item below is a file the OS keeps for its own build, not yet a
  published developer contract, so this module is the stopgap seam."
- OS repository: `grep -rn 'entry=' --include=build.rs services` gives 27 lines, all `service_main`.
- `.cargo/config.toml` in the OS also carries flags for `riscv32`, `loongarch64` and `s390x`, which
  `rust-toolchain.toml` does not list. The CLI already takes its targets from `rust-toolchain.toml`, so it
  is right today, but only because it knows which file to trust.

## The proposal (option B)

The OS's release workflow attaches one generated file, `godspeed-platform.toml`, carrying all seven facts,
with its digest in `SHA256SUMS-<tag>`. Its full text, including the format, the generator's `--check`
mode in `osdev`'s `EXTRA_CHECKS`, and what it does not do, is section 11 of the OS's
`docs/developer-services-design.md`. That document owns the design; this item owns the CLI's state.

## Open questions, and the CLI side's recommended answers

1. **Where the entry symbol comes from.** The generator reads it from the 27 `build.rs` files and refuses
   if they disagree. Scraping Rust source is fragile, but the agree-or-refuse rule makes it safe.
   *Recommended:* scrape now. Move it to the stdlib's Cargo `links` metadata with the linker script
   (that document's 3.6), so services and the platform file read one place.
2. **What the checksum proves.** `SHA256SUMS` comes from the same GitHub release as the file, so it catches
   corruption, not tampering. The trust is what downloading from GitHub gives today, no more.
   *Recommended:* say so, and let signed OS releases cover it later; no signature for this one file.
3. **A newer format than the CLI knows.** An old CLI meeting `format = 2` refuses with "this release
   needs a newer godspeed; run `godspeed self update`", and never guesses. This is cheap because 0.2.0
   has `self update`.
4. **Old releases.** v0.28.0 and earlier have no platform file, and published assets are never replaced.
   The CLI uses the file when a release has it, and falls back to the six files otherwise. The fallback is
   removed when no supported release lacks the file.

## Ruled out

- **Hard-coding more facts in the CLI** to need fewer files. Each one is another thing the OS can change
  without the CLI knowing.
- **Bundling the facts into the CLI binary.** One CLI then serves only the OS releases it was built
  for, and a new OS release would need a new CLI.
- **Writing the platform file by hand.** It would be a second declaration of facts the OS already holds,
  and would drift (the OS's CLAUDE.md 13.6). It is generated from the files it describes, or not at all.
- **Fetching from the default branch instead of the tag.** A project must build against the release
  it names, not whatever is newest.

## Next concrete step

1. **The OS maintainer decides** section 11.5 of the OS's `docs/developer-services-design.md`.
2. If yes, **the OS side**: a script in `scripts/` with a `--check` mode in `EXTRA_CHECKS`, and one step in
   `release.yml`. Small, and in the OS repository, so it is made there only when the maintainer says so.
3. **The CLI side**, which can be built and tested beforehand against a generated file:
   - `src/kit.rs`: fetch `godspeed-platform.toml` and check it against `SHA256SUMS-<tag>`, else fall back
     to the six files;
   - `src/platform.rs`: read the facts from it;
   - `build.rs` and `check.rs`: take the entry symbol from it, with `service_main` as the default for a
     release without it;
   - a refusal for an unknown `format`;
   - tests with the fake GitHub server (`os_server` in `tests/cli.rs`), including a release with the file,
     one without, and one with an unknown format.

   It is released after the first OS release that carries the file.

## As built, 2026-10-17 (CLI side, branch `feat/platform-file`)

- `src/kit.rs` asks for `releases/download/<tag>/godspeed-platform.toml` first. Present, it must be listed in
  `SHA256SUMS-<tag>` with the same digest (else `IntegrityFailure`, exit 17, and nothing kept), must describe
  the release asked for, and must be a format this CLI reads. The kit is then the file and the linker script
  it carries, written to `services/user.ld` beside it. Absent (every release up to 0.28.0), the six files, as
  before.
- `src/platform.rs` reads such a kit from the file (`PlatformFile::parse`); a newer `format` is refused with
  "run `godspeed self update`" (`Environment`, exit 11) before any other key is read. `Platform::entry` is the
  file's `[link] entry`, or `service_main` for a checkout or an older release.
- `build.rs` links with `--entry=<entry>`, `check.rs` looks for that symbol, and the service template's
  `src/main.rs` and README are filled with it (`{{entry}}`).
- Tests (`tests/cli.rs`, `a_release_with_a_platform_file_is_read_from_it_and_checked`): a release with the
  file and an entry of `start_here` (no raw file asked for; the generated service starts at `start_here`;
  `check` passes), one whose `SHA256SUMS` lists another digest, and one in format 2. The existing kit test
  now sees the platform file asked for, and missed, before the six files.
- The OS side was checked against this reader with its own output: `scripts/platform_file.py --out` on
  the OS repository at `bd4aee44`.

## Closed, 2026-10-24

GodspeedOS v0.30.0 publishes `godspeed-platform.toml` among its release assets: `format = 1`, `os = "0.30.0"`,
its commit, the stdlib crate and tag, the toolchain and targets, `[link] entry = "service_main"` and the
linker script, the compiler flags, and every file's digest. cli-v0.4.0's `src/kit.rs` fetches it first
(`fetch_platform_file`) and builds a kit from it, so a v0.30.0 project is built from the published contract
and not from the OS's private build files; the entry symbol, the seventh fact, comes from `[link] entry`.
A release before 0.30.0 has no file and still uses option A, which is why A stays in the CLI.

What the post-mortem is worth: the six files the kit read were the OS's build inputs, not a contract, and
nothing would have noticed them moving. The fix was a published file the OS's release workflow writes
(`scripts/platform_file.py` there), checked against `SHA256SUMS`, with a `format` a reader refuses when it
does not know it.
