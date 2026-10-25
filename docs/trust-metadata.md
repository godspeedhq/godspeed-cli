# The registry's trust metadata, version 1

**Status: DRAFT, implemented** - `crates/godspeed-package/src/trust.rs` (reading and verifying, `no_std`,
no allocator), `godspeed-registry`'s `tools/trust` (the maintainer's offline signing) and Worker (snapshot
and timestamp), and `godspeed fetch` (the client). In use by the official registry since its first public
publish, 2026-10-16; format 1 is frozen.

What a client needs, beyond a package's own files (`docs/package-format.md`), to know that a release is one
its publisher's key signed and the registry vouches for - without trusting the registry's storage, its
network, or even the registry service itself. The design and its reasons are `docs/publishing-design.md`,
section 6; this document is the format.

## 1. The files

In the registry's public bucket, beside `index/` and `packages/`:

    trust/root/1.toml   trust/root/1.sig      every root version, kept, so a client can follow rotations
    trust/root/2.toml   trust/root/2.sig
    trust/namespaces.toml   trust/namespaces.sig
    trust/snapshot.toml     trust/snapshot.sig
    trust/timestamp.toml    trust/timestamp.sig

| document | signed by | key held | says |
|---|---|---|---|
| `root` | root keys, by threshold | offline, by the maintainer | which keys hold each role, and how many must sign |
| `namespaces` | namespaces keys, by threshold | offline | each publisher's keys, each active, retired or revoked |
| `snapshot` | snapshot key | online, by the registry | the digest of every package index, and of `namespaces` |
| `timestamp` | timestamp key | online | the current snapshot; valid for one day |

## 2. The format

Every document and signature file is a sequence of `name = value` lines, where a value is a decimal integer
(no leading zeros) or a double-quoted string (no quotes, backslashes or control characters inside). Fields
come in exactly the order shown below; repeated lines are strictly ascending, so none repeats. LF line
endings, one final newline, nothing else - no comments, no blank lines.

That is valid TOML, but it is read by these rules, not by a TOML parser: a document has exactly one
spelling, which is what its signatures cover, and a Godspeed machine checks it without a heap.

Every document begins:

    format = 1
    type = "<root | namespaces | snapshot | timestamp>"
    version = <1 or more>
    expires = "YYYY-MM-DDTHH:MM:SSZ"

### root

    role = "root 2"
    key = "root ed25519:<64 hex>"            one line per key, ascending
    role = "namespaces 2"
    key = "namespaces ed25519:<64 hex>"
    role = "snapshot 1"
    key = "snapshot ed25519:<64 hex>"
    role = "timestamp 1"
    key = "timestamp ed25519:<64 hex>"

All four roles, in this order; each `role` line is the role and its threshold (at least 1, at most the
number of its keys). One key may hold more than one role.

### namespaces

    key = "alice active ed25519:<64 hex>"
    key = "alice retired 2026-11-01T00:00:00Z ed25519:<64 hex>"
    key = "bob revoked 2026-12-01T09:30:00Z ed25519:<64 hex>"

Ascending by publisher, then key. A key is never removed: its status only moves from `active` to `retired`
or `revoked`, and from `retired` to `revoked`.

### snapshot

    namespaces = "<namespaces version> sha256:<64 hex>"
    index = "alice/hello sha256:<64 hex>"      one per package, ascending by name

### timestamp

    snapshot = "<snapshot version> sha256:<64 hex>"

### Signature files (`<document>.sig`)

    format = 1
    signature = "ed25519:<public key> ed25519:<128 hex: signature>"     one per key, ascending by key

Each signature is Ed25519 (`verify_strict`) over a fixed message: the document type's purpose tag -
`godspeed <type> metadata v1` and a newline - followed by the SHA-256 of the document's exact bytes. A
signature therefore never counts for another type of document, nor for a package manifest, whose tag is
different (`docs/package-format.md`, 4a).

A signature counts towards a role's threshold only if its key is one of that role's keys in the root, and
each key counts once.

## 3. What a client checks, in order

1. **Root.** On first contact, root 1, signed by its own root threshold - trusted on first use, unless the
   client pins its SHA-256 (`godspeed fetch --trust-root`; the official registry's is built into the CLI). Then every later root in turn: root N+1 must be
   exactly the next version and signed by the threshold of root N's root keys AND its own. The last root
   must not have expired. The client remembers the root it reached.
2. **Timestamp.** Signed by the root's timestamp threshold; not expired; not an older version than the
   client has seen (rollback).
3. **Snapshot.** Exactly the digest and version the timestamp names; signed by the snapshot threshold; not
   expired.
4. **Namespaces.** Exactly the digest and version the snapshot names; signed by the namespaces threshold;
   not expired; not older than the client has seen.
5. **Index.** Exactly the bytes the snapshot names for the package.
6. **Manifest.** Exactly the digest the index names for the release.
7. **Signature.** `manifest.sig` verifies over the manifest, and its key is, for this publisher in
   `namespaces`: **active**; or **retired at T** with the release published (per the index) before T; never
   **revoked**, nor absent.
8. **Files.** Every file the manifest names, by size and digest, exactly as `godspeed verify` checks them.

Only then is anything kept. The client then remembers the timestamp and namespaces versions it accepted.

## 4. Retire and revoke

- **Retire** is routine rotation. The key signs nothing new - the registry refuses it - and releases
  published before the retirement stay trusted.
- **Revoke** is for compromise. Nothing the key ever signed is trusted, whenever it was published: the
  moment a key was stolen is rarely known. The publisher releases a new version under a new key; published
  bytes never change.

## 5. Compatibility

As for packages: `format` is the whole story, a reader refuses a format it does not know, and any change to
any of these documents is a new format number. New roles, or hashed bins for a snapshot too large for one
file, would be such a change.
