# Godspeed CLI - Skill Definition

## Purpose

Build and maintain the Godspeed CLI (`godspeed`).

The CLI is the primary developer-facing tool for creating, building, testing, running, packaging, installing, updating, inspecting, and eventually publishing Godspeed applications.

The CLI must make the correct path the easiest path.

The CLI is not the OS.
The CLI is not the kernel.
The CLI is not a second supervisor.
The CLI is not a package marketplace.

It is developer tooling around the public Godspeed programming and packaging contracts.

---

## Core Principle

The CLI adapts itself to Godspeed.

Godspeed does not change its kernel architecture to make the CLI convenient.

If the CLI appears to require:

- a new kernel syscall
- new kernel policy
- direct kernel knowledge
- raw IPC
- cap-table manipulation
- supervisor internals
- ISA-specific application logic
- undocumented service behavior

STOP.

First determine whether the CLI is violating an existing architectural boundary or whether an already-public contract is genuinely missing.

A missing CLI convenience is preferable to architectural leakage.

---

## Architectural Boundary

The intended dependency direction is:

    Developer
        |
        v
    godspeed CLI
        |
        v
    Public Godspeed contracts
        |
        +--> Runtime / SDK / stdlib
        |
        +--> Package format
        |
        +--> Machine-readable platform contracts
        |
        v
    Godspeed OS

The CLI must not depend directly on:

- MISCIS implementation
- kernel internals
- supervisor implementation details
- raw IPC protocols
- internal capability-table representation
- architecture implementation details
- private service APIs
- undocumented filesystem/network semantics

The CLI may invoke public interfaces exposed specifically for developer tooling.

---

## What the CLI Owns

The CLI owns developer workflow.

Examples:

    godspeed new hello
    godspeed build
    godspeed test
    godspeed run
    godspeed info
    godspeed install
    godspeed update
    godspeed publish

The CLI is responsible for composing these operations.

It is not responsible for redefining what the underlying system means.

---

## Initial Command Surface

### Project creation

    godspeed new <name>

Creates the smallest valid Godspeed application.

The generated project should:

- use the public stdlib
- contain the correct manifest
- declare required capabilities
- contain a valid entry point
- avoid raw IPC
- avoid private APIs
- avoid architecture-specific application code

The generated project is itself a reference for how developers are expected to write Godspeed applications.

---

### Build

    godspeed build

Build the current project for the appropriate target.

Examples:

    godspeed build --arch x86_64
    godspeed build --arch aarch64
    godspeed build --arch armv7
    godspeed build --arch riscv64
    godspeed build --all

Do not create a universal binary unless Godspeed's architecture contracts explicitly introduce such a format.

Godspeed applications may contain separate binaries for separate ISAs while remaining one logical package/version.

---

### Test

    godspeed test

Run project tests using the supported Godspeed testing mechanisms.

The CLI should distinguish:

- compilation failure
- test failure
- runtime failure
- unavailable target
- timeout
- unknown operation outcome

Do not collapse meaningful failure states into a generic "failed".

---

### Run

    godspeed run

Run the application on the selected target.

Examples:

    godspeed run --target qemu
    godspeed run --target hardware

Target selection belongs to the CLI.

Application code must not need to know whether it is running on QEMU, x86_64 hardware, ARM hardware, or RISC-V hardware unless that distinction is explicitly part of a public application contract.

---

## Package Management

Godspeed requires a package registry, not a marketplace.

The initial system must not require a web frontend.

The CLI is the primary interface.

Required operations:

    godspeed search <query>
    godspeed info <package>
    godspeed install <package>
    godspeed update
    godspeed publish

Potential future operations:

    godspeed remove <package>
    godspeed list
    godspeed registry
    godspeed login
    godspeed logout

Do not implement marketplace concepts such as:

- ratings
- reviews
- likes
- recommendations
- storefronts
- social profiles
- payment processing

unless explicitly introduced as a separate future product.

---

## Package Registry Architecture

Separate package metadata from binary artifacts.

Conceptually:

    Godspeed CLI
         |
         v
    Registry API
         |
         +--> package metadata
         +--> versions
         +--> publisher identity
         +--> dependency metadata
         +--> architecture availability
         +--> checksums
         +--> signatures
         |
         v
    Object/blob storage
         |
         +--> x86_64 binary
         +--> aarch64 binary
         +--> armv7 binary
         +--> riscv64 binary

A CDN may sit in front of immutable artifacts.

The registry should not need to serve large binaries itself.

The package format must be independent of the registry implementation.

A package should be capable of being hosted by:

- the official Godspeed registry
- a private registry
- a company registry
- a local registry
- a mirror

Do not encode the official registry as a fundamental property of the package format.

---

## Package Identity

A package consists of:

- name
- version
- Godspeed compatibility
- architectures
- dependencies
- requested capabilities
- resources
- artifact metadata
- integrity information
- publisher identity
- signature

Example conceptual package:

    foo 1.2.0

        x86_64
        aarch64
        armv7
        riscv64

The architecture-specific binaries are implementation artifacts of one logical package version.

---

## Artifact Integrity

Downloaded binaries must be verified before installation.

At minimum the registry/package metadata should provide:

- artifact size
- cryptographic hash
- architecture
- package/version association

Where signing is implemented, verify the publisher signature.

Do not confuse:

    integrity
    identity
    authorization
    safety

A valid signature means the artifact was signed by the corresponding publishing identity.

It does not mean the application is safe.

---

## Publisher Identity

Publishing requires authentication.

The registry should know who published each release.

Publishing should conceptually establish:

    package
       |
       +--> version
       |
       +--> publisher identity
       |
       +--> signing identity
       |
       +--> artifacts

Example:

    foo 1.4.2
    published by Alice
    signature verified

Package ownership and individual release attribution should remain distinguishable.

A maintainer may publish a release without necessarily being the original package author.

Do not make identity dependent on a web frontend.

The CLI should eventually support authentication directly.

---

## Capability Semantics

Package manifests may REQUEST capabilities.

They do not GRANT capabilities.

This distinction is fundamental.

    Package manifest
          |
          | requests
          v
    System/user/policy
          |
          | grants or denies
          v
    Runtime authority

Installing a package must never silently turn a package's requested authority into granted authority unless the platform explicitly defines that behaviour.

The registry distributes software.

The operating system decides authority.

---

## Dependencies

Keep dependency resolution explicit.

A dependency should identify:

- package name
- acceptable version range
- required platform compatibility where applicable

Do not allow dependency resolution to silently introduce incompatible architectures or unsupported Godspeed versions.

The CLI should explain resolution failures in terms a developer can act upon.

Prefer:

    foo requires bar >= 2.0
    available: bar 1.8
    resolution failed

over:

    dependency error: 17

---

## Update Semantics

    godspeed update

must not blindly replace working software.

The CLI should:

1. resolve available versions
2. check compatibility
3. verify artifacts
4. determine required changes
5. install the new version
6. preserve meaningful failure information

If an update operation reaches an uncertain state, do not pretend it succeeded.

Godspeed's broader rule applies:

    The SDK must not invent certainty.

The CLI must not invent certainty either.

---

## Authentication

Authentication is required for publishing.

It is not necessarily required for public package discovery or downloading.

Keep these concerns separate:

    anonymous/public:
        search
        info
        download

    authenticated:
        publish
        manage package ownership
        manage maintainers
        revoke/rotate publishing credentials

Do not introduce account complexity until the package distribution protocol requires it.

---

## Configuration

Configuration should be boring and predictable.

Potential configuration includes:

- default registry
- additional registries
- authentication credentials
- default target
- build configuration
- cache location

Do not make configuration an implicit source of authority.

Configuration can select behaviour.

It must not bypass capability enforcement.

---

## Caching

The CLI may cache:

- downloaded packages
- registry metadata
- build artifacts
- dependency resolution information

Caches must never be treated as authoritative truth when the protocol requires fresh metadata.

A corrupt or incomplete cache must be recoverable.

Prefer content-addressed storage for immutable artifacts where practical.

---

## Portability

The CLI itself must respect Godspeed's portability model.

Do not scatter ISA-specific application logic throughout the CLI.

If target-specific behaviour is required, isolate it behind explicit target implementations.

The CLI should consume facts such as:

    target architecture
    ABI
    supported runtime
    available platform features

rather than using architecture identity as a proxy for unrelated properties.

Follow the same principle as Godspeed OS:

    Architecture-neutral code may depend on facts,
    not use ISA identity as a proxy for facts.

---

## Error Model

Errors must be useful to developers.

Distinguish at least:

- invalid project
- compilation failure
- missing dependency
- incompatible package
- unavailable target
- permission denied
- authentication failure
- package not found
- checksum mismatch
- signature verification failure
- registry unavailable
- timeout
- cancellation
- outcome unknown

Never convert an important distinction into a generic error simply to simplify implementation.

The user needs enough information to decide what to do next.

---

## Security

Never:

- execute an unverified downloaded binary
- silently ignore checksum failures
- silently ignore signature failures
- downgrade security checks to make installation succeed
- store credentials in package metadata
- grant capabilities merely because a package requests them
- trust package names as identities
- treat registry availability as proof of package authenticity

Downloaded software crosses a trust boundary.

Make that boundary explicit.

---

## Developer Experience

The CLI should make ordinary work boring.

A developer should be able to go from:

    godspeed new hello

to:

    godspeed run

without understanding:

- MISCIS
- kernel IPC
- capability-table implementation
- supervisor internals
- device registers
- ISA boot mechanisms

The developer should learn those things because they are interested in them, not because ordinary application development requires them.

The public API should answer:

    What do I want to do?

rather than:

    Which kernel mechanism do I need to manipulate?

---

## Diagnostics

Diagnostics are part of the developer contract.

When rejecting an operation, tell the developer:

1. what failed
2. why it failed
3. what was expected
4. what can be done next

A rejection that merely says "no" is an incomplete developer experience.

This principle also applies to automated development agents.

If a tool rejects an invalid implementation but provides enough information for the developer to correct it, the boundary is working.

---

## Generated Projects

`godspeed new` should generate code that represents the canonical public way of doing things.

Generated code must:

- use public APIs
- avoid private shortcuts
- follow current package conventions
- declare authority explicitly
- have useful error handling
- be minimal

Do not generate unnecessary abstractions merely because they are possible.

The template is part of the documentation.

---

## Dogfooding

The CLI should be developed against the same public contracts available to ordinary contributors wherever practical.

If the CLI repeatedly needs private interfaces, investigate why.

Possible conclusions:

1. the CLI is violating the boundary
2. the public API is incomplete
3. a developer-facing contract is missing

Do not immediately expose internal implementation details.

---

## Implementation Freedom

Implementation behind the CLI's public behaviour may change freely when contracts remain stable.

Do not unnecessarily constrain:

- internal data structures
- caching implementation
- HTTP implementation
- registry backend
- object storage provider
- build orchestration internals

Stable contracts are more important than stable implementations.

The package format and protocol should outlive the first implementation.

---

## External Services

Do not make the CLI dependent on one infrastructure provider unless the public contract requires it.

For example:

    registry protocol
          |
          +--> R2
          +--> S3
          +--> B2
          +--> local storage
          +--> private infrastructure

Object storage is an implementation detail of the registry.

The CLI should talk to the registry protocol, not directly to a particular cloud provider.

---

## What Not To Build Yet

Do not introduce these merely because they might eventually be useful:

- marketplace UI
- web storefront
- ratings
- reviews
- payments
- social accounts
- package recommendations
- telemetry platform
- proprietary analytics
- universal binaries
- container compatibility
- POSIX compatibility
- Docker compatibility
- kernel changes for CLI convenience

Solve the immediate developer workflow first.

---

## Testing

The CLI must be tested against the contracts it promises.

Minimum categories:

### Project tests

- new project creation
- generated project builds
- generated project runs
- invalid project diagnostics

### Build tests

- each supported architecture
- unsupported architecture
- missing toolchain
- build failure
- reproducible output where promised

### Package tests

- package creation
- package inspection
- architecture selection
- dependency resolution
- installation
- update
- corrupted artifact
- checksum mismatch
- signature mismatch
- unavailable registry
- unavailable artifact

### Authentication tests

- unauthenticated publish rejected
- valid credentials accepted
- invalid credentials rejected
- unauthorized package publish rejected
- publisher identity recorded
- signature verified

### Registry tests

- search
- info
- version resolution
- publish
- download
- metadata failure
- artifact failure
- retry behaviour
- cache behaviour

### Stranger Test

A weak AI or unfamiliar developer should be able to:

    godspeed new hello
    godspeed build
    godspeed run

using public documentation without needing Godspeed kernel knowledge.

The test subject is allowed to fail.

The surrounding system is being tested on how respectfully it handles that failure.

---

## Design Rules

1. Make the correct path the easiest path.
2. Prefer one obvious way to perform an established operation.
3. Do not expose implementation details merely because they exist.
4. Do not hide decision-relevant information.
5. Do not invent certainty.
6. Do not silently broaden authority.
7. Do not let developer tooling leak policy into the kernel.
8. Keep package identity independent from registry implementation.
9. Keep immutable artifacts separate from mutable metadata.
10. Prefer boring infrastructure.
11. Preserve stable contracts.
12. Let implementations change behind those contracts.
13. Make failures diagnosable and recoverable.
14. Never make the CLI responsible for decisions belonging to the OS.
15. If the CLI needs to understand a kernel implementation detail, stop and investigate the boundary.

---

## Final Principle

The Godspeed CLI should make Godspeed feel simple without making it dishonest.

A developer should be able to say:

    godspeed install foo

and have the system handle:

    discovery
    version resolution
    architecture selection
    download
    integrity verification
    publisher verification
    installation

without requiring the developer to understand how any of those mechanisms are implemented.

Hide implementation complexity.

Never hide decision-relevant truth.
