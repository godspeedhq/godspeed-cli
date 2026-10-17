# The Godspeed package format, version 1

**Status: DRAFT. Implemented by `crates/godspeed-package`; `godspeed package` produces packages,
`godspeed sign` signs them and `godspeed verify` checks them. Nothing installs them yet, and nothing here is a promise until the first
package is published.**

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
| `format` | required; `1`. Any other value is refused before anything else is read (section 5). |
| `package.name` | required; `publisher/name`, each part a lowercase letter then lowercase letters, digits and `-`, at most 64 bytes - the OS contract schema's rule for a service name. |
| `package.version` | required; `MAJOR.MINOR.PATCH`, decimal, no leading zeros, no suffix. Equal to the contract's `version`. |
| `package.kind` | required; `service` or `utility` (added 2026-10-17, still format 1: a reader that does not know it refuses the manifest by naming the kind). A `library` is refused with the reason: libraries are depended on, not installed, and their distribution is not defined yet. `app` and `driver` cannot exist until the OS can run them. |
| `package.description` | optional; at most 256 bytes, no control characters. |
| `package.license` | optional; at most 64 bytes, no control characters. Not interpreted. |
| `os.release` | required; the GodspeedOS release the package was built against. |
| `contract.digest` | required; SHA-256 of `contract.toml`. |
| `artifact` | at least one; at most one per architecture; listed in the order x86_64, aarch64, armv7, riscv64. |
| `artifact.arch` | `x86_64`, `aarch64`, `armv7` or `riscv64`. |
| `artifact.size` | the exact byte length; at least 1, at most 64 MiB. |
| `artifact.digest` | SHA-256 of exactly those bytes, as `sha256:` and 64 lowercase hex digits. |

Reading is strict. An unknown key is an error, not ignored: a reader that skipped what it did not
understand would install a package on the strength of half its manifest. Every problem is reported in one
pass.

Writing is canonical: the fields in the order above, one text form per manifest. A signature covers the
manifest's exact bytes, so two spellings of one manifest would be two different things to sign.

### The package name and the service name

The `name` part of `publisher/name` must equal the contract's `name`. On a Godspeed machine a service is
known by its contract name, so the package cannot call itself one thing and run as another. Two publishers'
`hello` packages therefore both run as `hello`; what a machine does about that is the native package
manager's question, and is not answered here.

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
- **Dependencies.** A service has no runtime dependency mechanism on Godspeed, so format 1 has no
  dependency field rather than an unenforceable one.
- **API compatibility.** `os.release` records what the package was built against. It is not a claim that
  the package works on any other release: GodspeedOS has no versioned public API or ABI yet to make that
  claim about. When it does, compatibility becomes a field and the format a new version.
- **A registry.** Nothing in a package names where it is hosted. The same files work from the official
  registry, a private one, a mirror or a local directory.

## 4. Verifying a package

A reader, holding a manifest it has decided to trust (how is section 3's open question):

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

## 6. Open questions

These are recorded, not answered:

1. **How the native side reads the text without a heap.** The core types and validation are `no_std` and
   allocation-free; the TOML parser is not. Either a bounded TOML-subset reader for exactly this format,
   or a canonical binary encoding beside the text. Decided when the native package manager is.
2. **Trust in the manifest**: signatures exist (section 4a). Still open: binding a key to a publisher in
   metadata the registry cannot alter, and the index's own integrity - TUF's root, snapshot and timestamp
   roles (`docs/publishing-design.md` section 6).
3. **What `os.release` permits on install**: exact match only, or a range, once the OS has an API version.
4. **Two publishers, one service name** on one machine (section 2).
5. **Library packages**: whether, and how, a library is distributed at all.
