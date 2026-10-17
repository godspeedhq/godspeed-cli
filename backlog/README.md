# backlog/

Open items for the `godspeed` CLI that are **recorded rather than closed**, one file each. This follows
the GodspeedOS repository's `backlog/`, and its rules are the same.

## What belongs here

An item earns a file when it is **real, reproducible or evidenced, and not being fixed right now**.
Every entry carries:

- **Evidence**: a file and line, a log, a measurement. Not a suspicion.
- **What is RULED OUT**, so the next attempt does not re-derive the dead ends.
- **The next concrete step**: what would actually move it, and what it costs.
- **A status line at the top**, beginning `**Status:` and carrying OPEN, CLOSED or FIXED in capitals.

An item leaves by being **closed in place**: its status line says CLOSED or FIXED and the date, and the
post-mortem goes at the end. A FIXED item's file then moves to `fixed/` with its number unchanged, so the
open items are the ones in this folder. Nothing is deleted. The post-mortem is usually worth more than
the item was.

## One place

**Status lives here and nowhere else.** Design reasoning stays where it is written (`docs/`, or the OS
repository's design documents) and is linked, not copied. What must never exist twice is the claim about
state.

**Work that belongs to the OS is not a CLI item.** When the CLI waits on the OS, the item here records
what the CLI does meanwhile and what it will change; the OS's side is the OS's to track, in its own
repository.

## Current items

| # | Item | Severity | Blocks |
|---|------|----------|--------|
| [1](01-the-platform-kit-reads-the-os-repository-s-private-build-files.md) | The platform kit reads six of the OS repository's own build files at their paths, and hard-codes the entry symbol no file holds; option B (the OS publishes `godspeed-platform.toml`) is approved and built on both sides, released on neither | Contract / fragility | nothing today; any OS release that moves one of the six files |
| [2](02-one-snapshot-names-every-package-so-every-client-downloads-it-all.md) | One signed snapshot names every package, so every client downloads all of it whenever it changes (the native side's 150-package cliff is fixed: it keeps the snapshot on disk and reads it by offset) | Scale | nothing today (two packages); bandwidth at a large registry |
