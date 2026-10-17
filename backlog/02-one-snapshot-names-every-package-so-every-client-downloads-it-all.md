# 2. One snapshot names every package, so every client downloads all of it on every command

**Status: OPEN, NARROWED 2026-10-17 - the native side's cliff is FIXED; the download size is what is left.**
Recorded at the operator's request after `godspeed list available` was built, with two problems in it. The
second - the native package manager refusing every registry command past about 150 packages, because it held
the snapshot in a 16 KiB buffer - is gone, by the operator's suggestion the same day: keep the snapshot on the
DISK and read it back by offset. A signature covers the document's SHA-256, not its bytes, so the snapshot can
be hashed as it streams to a file, verified from the digest, and read a line at a time; the crate gained exactly
that (`SnapshotHead`, `IndexLines`, `verify_snapshot_stream`, `verify_namespaces_by_head`, `verify_index_digest`,
`Sha256Stream`), the OS keeps it in `/system/services/godspeed/trust/<registry>/snapshot.toml`, does not
download it again while the timestamp names the same one, and pages `list available` and `search` with
`from <n>`. Memory no longer depends on the registry's size; the disk bound is 256 MiB.

**What stays open is the first problem, the bytes**: a NEW snapshot is still downloaded whole, once per version.
Kept-and-reused helps only while it does not change, and a large registry's snapshot changes all the time. That
is the registry format's to fix, below, and nothing in it is urgent until the registry is large.

## What is wrong

`trust/snapshot.toml` lists the digest of every package's index (`docs/trust-metadata.md`, "snapshot"). A
client must download and verify the whole snapshot before it can trust any index, because the snapshot is
the only signed statement of what each index's bytes are. So the cost of every command grows with the size of
the registry, not with what the command asks about:

- a snapshot line is about 100 bytes, so 10,000 packages is about 1 MB, and a million about 100 MB, fetched
  and verified by every client for every `search`, `info`, `fetch` and `list available`;
- ~~the native package manager runs with no heap and a 256 KiB stack, and holds the snapshot in a 16 KiB
  buffer: about 150 packages.~~ Fixed 2026-10-17, above: it keeps the snapshot on disk and reads it by offset.

Paging the output does not help: the client has already had to fetch everything before it can show page one.

## Evidence

- `crates/godspeed-package/src/trust.rs`: `Snapshot::parse` reads one document holding every `index` line;
  `verify_index` looks a package up in it.
- The OS repository: `services/godspeed-pm/src/chain.rs`, `SNAPSHOT_MAX` (16 KiB), and
  `utilities/61_godspeed.md` 4a, "Limits".
- The registry today: one package, `godspeed/hello`, so nothing fails yet.

## Ruled out

- **Paging the display only.** The shell already pages any report (`godspeed list available | paginate`); the
  cost is the download and the verification, not the screen.
- **A bigger buffer on the native side.** It moves the cliff; it does not remove it, and the stack is fixed.
- **An unsigned listing endpoint for browsing.** A list the client cannot verify is a list a compromised
  registry can shape: hiding a package, or offering a look-alike first. Whatever pages must be signed.

## The likely shape (to be designed here, not decided)

1. **Hashed bins** (The Update Framework's delegated "hash bins"): the snapshot names a fixed number of
   bin documents, each signed and naming the indexes whose name hashes into it. A client fetches the
   snapshot (small, a line per bin) and only the one bin its package is in. `fetch` and `info` then cost
   the same at a million packages as at one.
2. **A signed, paged listing** for browsing: pages of names, each signed by the snapshot key and named by
   the snapshot (or a bin) by digest, so `list available` and `search` fetch one page at a time and the
   native side can offer `godspeed list available page <n>` / `next`.
3. **The crate** gains the bin and page documents' parsing and verification, no_std and heapless like the
   rest of `trust`, so the native side reads them with the same code.

## Next concrete step

Design the bin and page documents in `docs/trust-metadata.md` (format, which key signs them, how many bins,
how a client finds its bin), with the migration: the snapshot's format number changes, and an old client
must refuse a new snapshot loudly rather than read it as empty. Then the crate, the Worker and the
publishing tool; then the native side (`godspeed-pm`), which drops `SNAPSHOT_MAX` as a whole-registry limit.
Nothing is urgent until the registry nears a hundred packages; the native side refuses loudly before then
fails silently.
