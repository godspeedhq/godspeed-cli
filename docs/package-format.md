# The Godspeed package format, version 1

**Status: in use. Implemented by `crates/godspeed-package`; `godspeed package` produces packages,
`godspeed sign` signs them and `godspeed verify` checks them, the official registry has published them since
2026-10-16, and GodspeedOS's `godspeed-pm` installs them (from GodspeedOS 0.29.0). Format 1 and format 2 are
frozen (section 5).**

A package is one version of one Godspeed program, as files that can be inspected and verified without the
CLI, the registry or any network. This document defines those files and the rules for reading them. The
crate is the reference implementation; where the two disagree, this document is wrong or the crate is,
and either way it is a defect.

---

## 1. What a package is

One directory, or one registry prefix, holding exactly these files:

    manifest.toml       what the package is: name, version, the OS release it was built against, and
                        the size and digest of every other file
    manifest.sig        the publisher's signature over manifest.toml (section 4a); optional in a
                        local package, required by the registry
    contract.toml       the OS service contract: the authority the service REQUESTS
    x86_64.elf          one binary per architecture the package supports,
    aarch64.elf         named by the architecture - never by the manifest
    armv7.elf
    riscv64.elf

A package need not carry every architecture, and must carry at least one. One logical version is one
manifest: the binaries for different instruction sets are artifacts of the same version, not separate
packages, and there is no universal binary.

File names are fixed. The manifest never names a path, so it cannot point a reader at `../../anything`.

## 2. The manifest

    format = 1

    [package]
    name = "alice/hello"
    version = "1.0.0"
    kind = "service"
    description = "Says hello"
    license = "MIT"

    [os]
    release = "0.28.0"

    [contract]
    digest = "sha256:5b1c..."

    [[artifact]]
    arch = "x86_64"
    size = 9184
    digest = "sha256:0f3a..."

    [[artifact]]
    arch = "riscv64"
    size = 9260
    digest = "sha256:77de..."

| field | rule |
|---|---|
| `format` | required; `1`, or `2` exactly when the manifest has requirements (below). Any other value is refused before anything else is read (section 5). |
| `package.name` | required; `publisher/name`, each part a lowercase letter then lowercase letters, digits and `-`, at most 64 bytes - the OS contract schema's rule for a service name. |
| `package.version` | required; `MAJOR.MINOR.PATCH`, decimal, no leading zeros, no suffix. Equal to the contract's `version`. |
| `package.kind` | required; `service`, `utility`, `app` or `driver` (the last three added 2026-10-17, still format 1: a reader that does not know one refuses the manifest by naming the kind). A `library` is refused with the reason: libraries are depended on, not installed, and their distribution is not defined yet. A `driver`'s contract names its device by PCI class (`hw_pci_class`, `hw_pci_bar`), with `hw_pci_irq` for its interrupt and `hw_dma` for a DMA arena, or a USB device by vendor and product (`hw_usb`); the last three need the GodspeedOS release after v0.30.0, whose schema has them. `hw_dma` is at most 2 MiB, and `hw_usb` goes with none of `hw_pci_class`, `hw_pci_irq` and `hw_dma`: the supervisor refuses either at install. |
| `package.description` | optional; at most 256 bytes, no control characters. |
| `package.license` | optional; at most 64 bytes, no control characters. Not interpreted. |
| `os.release` | required; the GodspeedOS release the package was built against. |
| `contract.digest` | required; SHA-256 of `contract.toml`. |
| `artifact` | at least one; at most one per architecture; listed in the order x86_64, aarch64, armv7, riscv64. |
| `artifact.arch` | `x86_64`, `aarch64`, `armv7` or `riscv64`. |
| `artifact.size` | the exact byte length; at least 1, at most 64 MiB. |
| `artifact.digest` | SHA-256 of exactly those bytes, as `sha256:` and 64 lowercase hex digits. |
| `requires` | optional, format 2 only, at most 8: the packages this one needs installed and running (GodspeedOS `docs/developer-services-design.md` 16). No duplicates, never the package itself. |
| `requires.name` | `publisher/name` of a service or a driver. Never a built-in service (`fs`, `net-stack`): those come with the OS. |
| `requires.version` | a range written with the whole version: `^1.2.0` (this major; for `0.x` this minor; for `0.0.z` exactly), `=1.2.3` or `>=1.2.0`. |

A manifest with requirements carries them after its artifacts, one table each:

    format = 2
    ...
    [[requires]]
    name = "alice/notes-store"
    version = "^1.2.0"

They are format 2, and only they are, so a package without any stays format 1 and every reader of format 1 still
installs it; a reader that predates requirements refuses one that has them as a format it does not read, which is
the true reason. A contract peer that is a package (`publisher/name`) must be required; `godspeed check` refuses
the package otherwise, and so does the machine.

Reading is strict. An unknown key is an error, not ignored: a reader that skipped what it did not
understand would install a package on the strength of half its manifest. Every problem is reported in one
pass.

Writing is canonical: the fields in the order above, one text form per manifest. A signature covers the
manifest's exact bytes, so two spellings of one manifest would be two different things to sign.

### The package name and the service name

The `name` part of `publisher/name` must equal the contract's `name`, so the package cannot call itself one
thing and be built as another. On a Godspeed machine an installed package runs under its package name,
`publisher/name`, which no built-in service can hold: two publishers' `hello` packages are two services,
`alice/hello` and `bob/hello`, and neither can stand in for a built-in one (GodspeedOS's CLAUDE.md 14.1,
the 2026-10-17 install amendment).

## 3. What the manifest deliberately does not carry

- **Capabilities.** The contract is the one declaration of what a service requests, and the manifest
  binds it by digest instead of restating it. A second list would be the drift the OS's constitution
  (13.6) was amended to stop. A manifest that tries to carry capabilities is refused as an unknown key.
- **Grants.** Nothing in a package grants anything. Installing a package must never turn what its contract
  requests into what it is given; the OS decides, and the decision is visible.
- **Signatures.** The signature is a separate file (`manifest.sig`), not a manifest field: a manifest
  cannot contain a signature over itself, and keeping it outside means the manifest's bytes - and its
  digest in the index - are the same signed or unsigned.
- **Publisher identity.** A signature proves which KEY signed. Which publisher a key belongs to is not in
  the package at all: it is the registry's trust model (`docs/publishing-design.md` section 6), because a
  package that vouched for its own key would vouch for nothing.
- **Dependencies, until format 2.** Format 1 had no dependency field rather than an unenforceable one. Since
  GodspeedOS can resolve and enforce them (`docs/developer-services-design.md` 16), format 2 carries
  `[[requires]]` above; a requirement grants nothing, and the contract still holds every peer the operator approves.
- **API compatibility.** `os.release` records what the package was built against. It is not a claim that
  the package works on any other release: GodspeedOS has no versioned public API or ABI yet to make that
  claim about. When it does, compatibility becomes a field and the format a new version.
- **A registry.** Nothing in a package names where it is hosted. The same files work from the official
  registry, a private one, a mirror or a local directory.

## 4. Verifying a package

A reader, holding a manifest it has decided to trust (through the registry's signed metadata,
`docs/trust-metadata.md`, or, for a local `.gsp` file, a publisher key its operator trusts):

1. parses it, refusing an unknown format and any rule broken;
2. checks `contract.toml` against `contract.digest`, and the contract's `name` and `version` against the
   manifest's;
3. for the architecture it needs, streams that one artifact through a verifier that knows the declared
   size and digest - refusing as soon as more bytes arrive than declared, and on completion reporting
   a short transfer, a digest mismatch, or success, each distinctly.

Nothing is executed, installed or written to its final place before step 3 passes. `ArtifactVerifier`
holds only the hash state, so this runs without a heap on a Godspeed machine.

### By hand, without the CLI

Every check in steps 2 and 3 except the ELF machine is a size and a SHA-256, so standard tools repeat it:

    $ cd alice-hello-1.0.0
    $ grep -A3 'arch = "x86_64"' manifest.toml
    arch = "x86_64"
    size = 18952
    digest = "sha256:2f3fa20d..."
    $ wc -c < x86_64.elf
    18952
    $ sha256sum x86_64.elf contract.toml
    2f3fa20d...  x86_64.elf
    762be027...  contract.toml

The digests must equal the manifest's, after its `sha256:` prefix. `godspeed verify <dir>` does all of it,
also refuses any file the manifest does not name, and checks that each `<arch>.elf` is an ELF for that
architecture - a mistake a digest cannot catch, since it binds whatever bytes the packager wrote.

## 4a. The signature: `manifest.sig`

Exactly three lines, and nothing else:

    format = 1
    key = "ed25519:<64 lowercase hex digits: the Ed25519 public key>"
    signature = "ed25519:<128 lowercase hex digits>"

- **What is signed:** the purpose tag `godspeed manifest signature v1` and a newline (31 bytes), then
  the 32-byte SHA-256 of `manifest.toml`'s exact bytes - 63 bytes in all. The tag means a manifest
  signature can never be presented as a signature over anything else the registry signs; signing the
  digest keeps the message fixed-size, so verification needs no heap.
- **Verification is strict** (Ed25519 `verify_strict`): malleable signatures and weak keys are refused.
- **Reading is exact:** the file is compared against its canonical form rather than parsed as TOML - no
  comments, no extra spaces, LF line endings, one final newline. A reader on a Godspeed machine needs no
  TOML parser for it, and a signature file has one spelling.
- **A signature proves which key, not whose.** `godspeed verify` reports the key and says so.

Signing with your own tools: `sha256(manifest.toml)` appended to the tag, signed with Ed25519, written in
the form above. `godspeed sign <dir>` does it with your key (`godspeed key generate`).

## 5. Compatibility policy

- **`format` is the whole compatibility story.** A reader refuses a format it does not know, with a message
  naming both numbers. It never guesses.
- **Any change to the manifest is a new format number**: an added field, a removed one, a changed rule.
  There are no minor revisions and no "ignore what you don't know". A format is frozen once a package in
  it has been published.
- **Readers keep reading old formats.** Once format 2 exists, a reader that knows it also reads format 1
  for as long as any published package is in format 1. Published versions are immutable, so old packages
  never migrate; readers do.
- **The crate is versioned separately from the CLI** and follows semver for its Rust API. Its major version
  changes when its API does; the format number changes when the files do.

## 6. Questions

Answered, by what was built:

1. **How the native side reads the text without a heap**: a bounded reader for exactly this format, in
   GodspeedOS (`services/godspeed-pm/src/toml_lines.rs`), reading into this crate's own types, so every rule
   here is checked there by the same code.
2. **Trust in the manifest**: a publisher's keys are listed in the registry's `namespaces` document, signed
   offline, and the index's integrity is TUF's snapshot and timestamp roles (`docs/trust-metadata.md`).
3. **What `os.release` permits on install**: a floor. GodspeedOS refuses a package built for a release before
   0.29.0, when what a program reads at start changed (`LAYOUT_RELEASE`); nothing caps it from above. A range
   waits for the OS to have an API version.
4. **Two publishers, one service name** on one machine: each runs under its package name (section 2).

Still open:

5. **Library packages**: whether, and how, a library is distributed at all.
