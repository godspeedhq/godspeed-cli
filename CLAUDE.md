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

The CLI owns developer workflow on the developer's workstation.

Examples:

    godspeed new service hello
    godspeed build
    godspeed test
    godspeed run --target qemu
    godspeed check
    godspeed package
    godspeed publish

The CLI is responsible for composing these operations.

It is not responsible for redefining what the underlying system means.

Installing, updating and removing software ON a Godspeed machine belongs to the native package manager, a
separate program that shares the `godspeed` command vocabulary (see "Host CLI and Native Package Manager"
below). The two must never be confused: one builds and publishes, the other verifies and installs.

---

## Initial Command Surface

### Project creation

    godspeed new <kind> <name>

Creates the smallest valid Godspeed project of that kind (see "Project Types" below; a template exists only
for a kind the OS can actually run).

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

Required operations, by where they run:

    host CLI (the developer's workstation):
        godspeed publish
        godspeed login
        godspeed logout
        godspeed whoami

    native package manager (a Godspeed machine):
        godspeed search <query>
        godspeed info <package>
        godspeed install <package>
        godspeed list
        godspeed update <package>
        godspeed uninstall <package>

Potential future operations:

    godspeed registry
    godspeed delete cache <package>
    godspeed delete data <package>

`uninstall`, not `remove`: one word for one act, and `delete` keeps its Godspeed meaning of destroying data
(the shell's `delete`), which uninstalling does not do by default.

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

    godspeed new service hello
    godspeed build
    godspeed test
    godspeed run --target qemu
    godspeed publish

and someone else, on a Godspeed machine:

    godspeed install alice/hello

using public documentation without needing Godspeed kernel knowledge. The subject should build something real
(a service or a utility that does a job), not a package holding a static example. Until the OS can install an
image it did not ship with, the test stops at `godspeed run` (see "What GodspeedOS Does Not Provide Yet").

The test subject is allowed to fail.

The surrounding system is being tested on how respectfully it handles that failure.

---

# Developer Experience and Publishing Contract

**Amendment 2026-10-15.** The sections below supplement everything above. They do not replace its boundaries,
security rules or design principles; where they appear to, the sections above win and this part is wrong.

## Implementation Language

The CLI is written in Rust. This is an engineering choice for the tool, not a rule that every Godspeed program
must be Rust forever.

Rust because GodspeedOS and its SDK are Rust; the CLI drives Cargo and the Rust toolchain directly; the package
manifest can be one typed definition validated at compile time; and the CLI ships as native executables for the
supported workstations.

- Do not share host-specific code with the native package manager for the sake of reuse.
- One small, independently versioned **package-contract crate** may be shared between them: the manifest types,
  their validation and the artifact format. It must build `no_std` and without an allocator, because the native
  side runs on Godspeed under its no-heap rule (the OS's CLAUDE.md 26.6.1).
- The CLI never needs a change to MISCIS for developer convenience. If it seems to, read "Core Principle" again.

## The Primary Goal

A contributor who does not know Godspeed's internals can create, build, test, run and publish software through
documented public interfaces alone:

    godspeed new service hello
    cd hello
    godspeed build
    godspeed test
    godspeed run --target qemu
    godspeed publish

and another user can then install it:

    godspeed install alice/hello

No step may require knowledge of kernel internals, raw IPC, capability-table layout or private supervisor
protocols.

## Project Types

    godspeed new service clock
    godspeed new library parser
    godspeed new utility status
    godspeed new app hello
    godspeed new driver sensor

| Kind | What it is | Can the OS run it today? |
|---|---|---|
| `service` | A managed component with a lifecycle and declared authority requirements | **Yes.** Kept running by the supervisor; `godspeed install` starts it at every boot |
| `library` | Reusable code for other Godspeed projects; not executable or installable on its own | **Yes**, as a Rust crate. How it is distributed and depended on is defined separately from executable packages |
| `utility` | A program that performs one operation and ends | **Yes, from GodspeedOS 0.29.0** (and a checkout ahead of 0.28.0). The OS's `Exit` syscall (its CLAUDE.md 14.5 amendment, 2026-10-17) and `gs::program`: the shell runs an installed utility by its name, hands it its arguments, shows or pipes its output and says its status. A release before that cannot build one: the template uses `gs::program` |
| `app` | A user-facing program with the screen and the keyboard while it runs | **Yes, from GodspeedOS 0.29.0** (and a checkout ahead of 0.28.0): a utility that holds the console foreground, through `gs::app`. The operator approves the console at install. Not windows, not a graphics surface beyond the console's grid: those are later, if ever |
| `driver` | A userspace driver for one device, named in its contract by PCI class code and register BAR | **Yes, from GodspeedOS 0.29.0** (and a checkout ahead of 0.28.0), REGISTERS ONLY: the operator approves the device at install, the OS finds which device of that class it is, and the driver is given that device's register window - no DMA arena and no interrupt yet, and never a class a GodspeedOS driver already handles. Its template is written on `gs::driver` (`registers` and `wait`, 2026-10-22), which no release up to 0.29.0 has, so it builds against the first release after 0.29.0 or a checkout of `feat/gs-driver` |

These are developer-facing roles. They create no new kernel task class and no privileged identity.

**A template ships only for a kind the OS can run.** The rule above ("The CLI must not invent APIs that
GodspeedOS does not yet expose") applies here first: `godspeed new utility` before the OS has a terminating task
would generate a program that cannot do what its kind promises. Until then the CLI answers that kind with what is
missing and where it is tracked, never with a template that pretends.

## Generated Project Requirements

Every generated project:

- compiles with the supported Godspeed toolchain;
- is written on the public standard library (`gs`) and nothing below it, which the OS already enforces for its own
  services (`scripts/one_way_check.py` holds a new crate at zero raw SDK calls);
- carries a valid package manifest that declares the capabilities it requests;
- contains a minimal working example and a test that runs;
- says how to build and run it;
- uses no undocumented API, no unnecessary dependency, and no ISA-specific logic unless the project truly needs it.

Every template is built and run in CI. Templates are executable documentation: one that fails to build or run
under its documented environment is a defect in the CLI.

## Command Surface (host)

    godspeed new <kind> <name>
    godspeed build [--arch <isa> | --all]
    godspeed test
    godspeed run [--target qemu|hardware]
    godspeed check
    godspeed package
    godspeed publish
    godspeed login | logout | whoami
    godspeed doctor

- `check` validates the project, its manifest, its targets and its declared compatibility, without publishing.
- `package` builds and validates the distributable artifacts locally, so they can be inspected before anything
  leaves the machine.
- `publish` is the authenticated publication workflow.
- `doctor` checks the developer's environment: toolchains, targets, QEMU, credentials. It is where an error's
  "Next:" line can send someone.

Every command works non-interactively for CI, with the same validation and security. No command asks for
something the manifest already says.

## Package Manifest

A versioned, machine-readable TOML manifest. Illustrative only, not a claim that these fields or capability
names exist:

    [package]
    name = "hello"
    version = "1.0.0"
    kind = "service"
    description = "Example Godspeed service"
    license = "MIT"

    [compatibility]
    api = "1"

    [capabilities]
    request = ["log_write"]

- It describes identity, compatibility, components, dependencies and **requested** authority. It grants nothing.
- **It must be derived from the OS's existing service contract, not invented beside it.** Godspeed already has a
  contract format (`contracts/<name>.toml`, validated against `contracts/schema/service.schema.json`), and the OS
  is explicit that a contract is a build-time declaration and the spawn request is what grants (the OS's CLAUDE.md
  13.6). A second, competing declaration of a service's authority is the drift that section was written to stop.
  The package manifest should carry or reference the service contract, and use its capability names.
- Cargo's manifest and the Godspeed manifest serve different purposes. Do not merge them silently.
- The format has an explicit version and a written compatibility policy.
- `api = "1"` presumes a versioned Godspeed API that does not exist yet. The OS defines that version; the CLI
  only reads it.

## Package Identity

- Names are namespaced by publisher: `godspeed/ask`, `alice/clock`, `bob/editor`.
- A package's identity is stable and independent of where it is stored.
- **A published version is immutable.** The same version is never replaced with different bytes, silently or
  otherwise.
- One logical version carries one artifact per supported architecture.
- Compatibility is the Godspeed API/ABI contract and the platform's requirements, not the CPU architecture alone.

## Publishing Experience

Illustrative output:

    $ godspeed publish
    Validating project...
    Building supported targets...
    Creating package artifacts...
    Verifying package metadata...
    Publisher authentication required.
    Continue with sign-in? [Y/n]
    Authenticating...
    Checking namespace ownership...
    Signing package...
    Uploading artifacts...
    Finalising publication...
    Published alice/hello@1.0.0
    Install:
      godspeed install alice/hello

A step is reported as done only when its outcome has been established. The developer never has to create a
Cloudflare account, hold storage credentials, upload binaries by hand, edit a central index, know object keys,
compute checksums, or run a server. The CLI does the routine work.

## Authentication and Signing

- Downloading needs no account. Publishing needs an authenticated identity authorised for the namespace.
- **Authentication and signing are separate.** The registry establishes who may publish where. A signature
  establishes which key signed an artifact or its metadata.
- Signing keys are never embedded in artifacts or printed in logs. How a publisher's keys are registered, rotated
  and revoked is defined before the first public publish, not after.
- The CLI guides a first-time publisher through setup.
- CI publishes with narrowly scoped credentials and no interactive session.
- A valid signature does not mean a package is safe.

## Registry Backend: Cloudflare R2

R2 is the first object store behind the official registry. It holds immutable artifacts and their metadata.
Illustrative layout (exact naming is an implementation detail):

    packages/
      alice/
        hello/
          1.0.0/
            manifest.toml
            armv7.gsp
            aarch64.gsp
            riscv64.gsp
            x86_64.gsp

- The CLI talks to the **registry protocol**, never to R2's API (see "External Services" above). The public
  download path is a static one and does not depend on R2-specific interfaces.
- An index provides discovery, version resolution, compatibility and artifact locations.
- No marketplace, reviews, ratings, payments or storefront.

## Publishing Control Plane

Third-party publishing needs a small trusted API for: publisher authentication; namespace ownership; version
reservation; scoped, short-lived upload authorisation; artifact and metadata verification; atomic finalisation;
and index updates.

- Developers never receive unrestricted storage credentials.
- A partial or failed release is never installable.
- Concurrent attempts to publish the same version do not overwrite one another: exactly one wins, and the other
  is told so.
- Downloads keep working when the publishing API is down.
- The first version may approve publishers by hand. Self-service registration can come later without changing the
  package format.

## Integrity and Trust

- Every artifact is bound to its package, version, target and digest, and is verified before installation.
- The registry has a defined trust model for its index and metadata. **Signatures on artifacts are not enough if
  an attacker can replace the index or swap a publisher's key unnoticed.** The Update Framework (TUF) is the
  reference design for exactly this problem; read it before inventing one.
- Five properties are kept separate: content integrity, publisher identity, registry authorisation, local
  capability grants, and software safety.

The registry distributes software. The operating system decides what it may do.

## Host CLI and Native Package Manager

The two share the `godspeed` vocabulary and are separate executables.

- **The host CLI** creates, builds, tests, packages and publishes, on the developer's workstation.
- **The native package manager** searches, inspects, downloads, verifies, installs, updates and uninstalls, on a
  Godspeed machine. It is an ordinary Godspeed program on public interfaces: no privileged kernel knowledge, no
  private supervisor protocol.

On the native side:

- a package's kind comes from its verified metadata; a user does not need to know it to install it;
- uninstalling keeps the package's user and service data by default (on Godspeed, its folders under `/system/`
  and `/users/`), and `delete data` says exactly what it will destroy before it does;
- **installing or updating never grants a requested capability by itself.** Something the operator controls
  decides, and the decision is visible.

## What GodspeedOS Does Not Provide Yet

The CLI must not paper over these. Each is the OS's to build, in the OS repository, under its own constitution,
and each blocks a named part of this contract:

| Missing in the OS | What it blocks here | Where the OS tracks it |
|---|---|---|
| Starting an image the OS did not ship with: the supervisor runs only images compiled into it | `godspeed install` on a running machine; `godspeed run` without rebuilding the OS image | `docs/service-ownership.md`, step 2 (images from `fs` as signed packages): design, not built |
| A signature check on service images at spawn | Any trust in an installed package beyond "the registry said so" | the same step 2 |
| A local grant policy: who decides what an installed package's requested capabilities become | "Installation respects local capability policy" | not designed |
| A terminating task (`fn main()`, or an exit call) | `utility` and `app` templates | `docs/stdlib-design.md` 1 (options A and B) |
| Device grants for a driver outside the OS tree | the `driver` template | the supervisor's spawn rows, by device kind |
| A versioned public API/ABI | `[compatibility] api` | the OS's v1 work |

**Until then, `godspeed run --target qemu` works by building a Godspeed image with the project embedded**, the
way every OS service is built today. That is a development convenience, and the CLI says that is what it is
doing: it is not installation, and it must not be presented as installation.

## Failure and Recovery

Publishing, downloading, installing and updating fail in ordinary ways, and each is handled: interrupted
transfers; an unavailable registry; expired credentials; namespace conflicts; invalid signatures; digest
mismatches; incompatible targets; partial transfers; concurrent publishes; and outcomes nobody can establish.

- Operations are idempotent or reconcilable where practical.
- **A timed-out publication is not a failure.** It may have succeeded. Before retrying, the CLI reads the
  registry's authoritative state and reports what it finds. The same holds for installing and updating.
- Do not invent certainty (Design Rule 5, and the OS's `OutcomeUnknown`).

## Diagnostics

Every refusal says what happened, why, and what to do next:

    Cannot publish alice/hello@1.0.0
    Reason:
      Version 1.0.0 already exists.
    Next:
      Increment the package version and retry.

    Cannot build for riscv64
    Reason:
      The Godspeed riscv64 target toolchain is not installed.
    Next:
      Run `godspeed doctor` for setup guidance.

and names the stage it failed in: validation, compilation, testing, packaging, authentication, upload or
finalisation. Internal detail is shown only when it helps the developer act.

## Acceptance Criteria

The CLI is not done because its commands exist. At minimum:

1. Every template that ships generates a valid project, and a kind that cannot ship says why.
2. Generated projects use only documented public interfaces.
3. Every supported target builds.
4. An unsupported target fails with an actionable error.
5. Manifests are validated the same way everywhere.
6. Artifacts can be inspected and verified independently of the CLI.
7. First-time publisher setup works.
8. Unauthorised publication is rejected.
9. A published version cannot be overwritten.
10. An interrupted publication never exposes an incomplete release.
11. Published packages can be discovered and downloaded.
12. Downloads are verified before installation.
13. Installation respects local capability policy (blocked on the OS, above).
14. Uninstalling does not silently destroy retained data.
15. A new contributor completes the documented workflow without the maintainer's help (the Stranger Test above).

Failures of the Stranger Test are evidence for improving the CLI, the SDK, the documentation or the contracts.
Never bypass an architectural boundary to make the demonstration pass.

## Scope Discipline

The first milestone is the smallest complete workflow:

    create -> build -> test -> package -> publish -> discover -> install -> run

and its first half (create to publish, plus run in QEMU) can be built now; its second half waits on the OS.

No marketplace, social features, payments, elaborate dependency ecosystem or extra infrastructure before that
workflow works reliably. R2 is the first storage choice, not a property of the package format; Rust is the first
implementation choice, not a reason to couple the CLI to kernel internals. The public contract should outlive
both.

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

The CLI handles the routine mechanics. The public SDK defines the programming contract. The registry distributes
verified artifacts. The operating system keeps authority over what installed software may do.

Make the developer experience simple. Keep the architecture honest.
