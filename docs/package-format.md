# The Godspeed package format, version 1

**Status: DRAFT. Implemented by `crates/godspeed-package`. Nothing produces or installs packages yet, and
nothing here is a promise until the first package is published.**

A package is one version of one Godspeed program, as files that can be inspected and verified without the
CLI, the registry or any network. This document defines those files and the rules for reading them. The
crate is the reference implementation; where the two disagree, this document is wrong or the crate is,
and either way it is a defect.

---

## 1. What a package is

One directory, or one registry prefix, holding exactly these files:

    manifest.toml       what the package is: name, version, the OS release it was built against, and
                        the size and digest of every other file
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
| `package.kind` | required; `service`. A `library` is refused with the reason: libraries are depended on, not installed, and their distribution is not defined yet. `utility`, `app` and `driver` cannot exist until the OS can run them. |
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

Writing is canonical: the fields in the order above, one text form per manifest. Nothing signs a manifest
yet, but when something does, two spellings of one manifest would be two different things to sign.

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
- **Publisher identity and signatures.** A digest proves that bytes are the ones the manifest names. It
  does not prove who wrote the manifest. Who published a release is the registry's record; which key
  signed it is a signature's job; neither is designed yet. The Update Framework (TUF) is the reference for
  that design, because signatures on artifacts are not enough if the index or a publisher's key can be
  swapped unnoticed.
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
2. **Trust in the manifest**: signatures, key registration and rotation, and the index's own integrity
   (TUF).
3. **What `os.release` permits on install**: exact match only, or a range, once the OS has an API version.
4. **Two publishers, one service name** on one machine (section 2).
5. **Library packages**: whether, and how, a library is distributed at all.
