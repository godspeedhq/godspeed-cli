# Publishing: the registry protocol, its control plane, and its trust model

**Status: DESIGN, decisions taken 2026-10-15 (section 9). Being built in `godspeed-registry`.** It is the
design `godspeed publish` waits on.

Storage is chosen: the Cloudflare R2 bucket `godspeedos-packages`, with a custom domain to come. Everything
else here is built so that choice stays an implementation detail (CLAUDE.md, "External Services").

---

## 1. Shape

    developer                        Godspeed machine / anyone
       |                                     |
       | godspeed publish                    | godspeed install, search, info  (or curl)
       v                                     v
    publishing API  (Cloudflare Worker)    https://<custom domain>/   (static, read-only)
       |   authenticates the publisher            ^
       |   checks the namespace                   |
       |   verifies the package                   |
       |   finalises atomically                   |
       v                                          |
    R2 bucket godspeedos-packages  ---------------+
       index/      mutable, small, signed
       packages/   immutable, content-checked

Two paths, deliberately separate:

- **Reading is static.** The index and every package are plain files served from the custom domain. No API
  call, no account, no R2-specific interface. Downloads keep working when the publishing API is down,
  because nothing on the read path is the publishing API.
- **Writing goes through one small trusted service**, which holds the bucket binding. No developer, CI job
  or CLI ever holds storage credentials, unrestricted or otherwise.

The R2 S3 endpoint (`<account>.r2.cloudflarestorage.com`) is on neither path: it requires signed requests
and is an administrator's tool. The CLI never names it.

## 2. The bucket layout

    packages/<publisher>/<name>/<version>/manifest.toml
    packages/<publisher>/<name>/<version>/contract.toml
    packages/<publisher>/<name>/<version>/<arch>.elf
    index/<publisher>/<name>.toml
    index/root.json            (section 6)
    index/timestamp.json       (section 6)

    staging/<publisher>/<name>/<version>/<upload-id>/...   (in-progress uploads; section 3)

- **`packages/` is immutable.** A key is written once, at finalisation, and never again. Served with a
  long cache lifetime; a CDN may hold it forever.
- **`index/` is the only mutable part**, small, served with a short cache lifetime, and signed.
- The files under a version are exactly a package directory as `godspeed package` writes it and
  `godspeed verify` checks it (`docs/package-format.md`). A downloaded version is verifiable with the same
  command, by anyone.
- **Staging shares the bucket for now, and is unfindable rather than private** (maintainer's decision,
  2026-10-15, to run one bucket during alpha). An R2 public domain serves every key in its bucket, so a
  staged file is downloadable by anyone holding its exact key. What keeps that harmless: the upload id in
  the key is 128 random bits, the public URL does not list keys, the index never names a staged file (so
  nothing staged is installable), staged files are deleted at finalisation, and a lifecycle rule on
  `staging/` sweeps the rest within a day. **Before anyone but the maintainer holds a publisher token,
  staging moves to a separate private bucket** with no public domain at all.

### The package index

One file per package, listing every published version:

    format = 1
    name = "alice/hello"

    [[version]]
    version = "1.0.0"
    manifest = "sha256:..."     # digest of that version's manifest.toml
    published = "2026-11-02T10:14:00Z"
    publisher = "alice"         # the authenticated account that published it (section 4)

    [[version]]
    version = "1.1.0"
    ...

The manifest digest is the link from mutable to immutable: the index names a manifest by its bytes, and the
manifest names every other file by its bytes. Trust the index entry and you can verify everything under
it without trusting the storage, the CDN or the network.

`godspeed search` and `godspeed info` read these files. There is no search service: a top-level listing
(`index/packages.toml`, every name and its latest version) is enough for a registry this size, and is
rebuilt at each finalisation.

## 3. Publishing, step by step

The CLI's side, in the order its output reports it (CLAUDE.md, "Publishing Experience"), each step
reported only once its outcome is established:

1. **Validate, build, package, verify**: `godspeed package`, unchanged.
2. **Authenticate** (section 4).
3. **Reserve** `alice/hello@1.0.0`: `POST /v1/reservations` with the manifest. The API checks the token may
   publish under `alice`, that 1.0.0 does not exist and is not reserved, and that the manifest is valid.
   It returns an upload id and an expiry (15 minutes). **Concurrent publishes are decided here**: exactly
   one reservation per version succeeds; the other is told "1.0.0 is being published by another upload"
   or "1.0.0 already exists", and nothing it does can overwrite the winner.
4. **Upload** each file: `PUT /v1/uploads/<id>/<file>`. The Worker writes to `staging/`. Only the files
   the reserved manifest names are accepted, each checked against its declared size and digest as it
   streams in - the same `ArtifactVerifier`, compiled to WebAssembly, that the CLI and the native package
   manager use. A file that fails is refused at once.
5. **Sign** (section 6) and **finalise**: `POST /v1/uploads/<id>/finalise`. The API re-verifies the whole
   staged package, copies it to `packages/.../<version>/`, then writes the index entry, then the timestamp.
   **The index write is the commit point.** Before it, the version does not exist for any reader; after it,
   every file it names is already in place. A crash anywhere before it leaves only staged files, which no
   index names and the lifecycle rule sweeps.
6. **Report** `Published alice/hello@1.0.0` and the install line.

### When the outcome is unknown

A finalise request that times out may have succeeded. The CLI never retries it blindly: it reads the
public index for `alice/hello` and reports what it finds - published (with the manifest digest, which must
equal the local one), or not published and the reservation still open (finalise again, which is idempotent),
or not published and the reservation expired (start again). A version that is published with a DIFFERENT
manifest digest than the local one is reported as exactly that, and is not "fixed" - published versions
are immutable.

### Errors the API returns

Structured, so the CLI can turn each into a "Cannot ... / Reason / Next" diagnostic: `unauthenticated`,
`token-expired`, `namespace-not-yours`, `version-exists`, `version-being-published`, `manifest-invalid`
(with the package crate's problems), `file-not-in-manifest`, `size-mismatch`, `digest-mismatch`,
`reservation-expired`, `signature-invalid`, `registry-unavailable`.

## 4. Who may publish: authentication and namespaces

Authentication answers "may this request publish under `alice`". It is not signing (section 6).

**Phase 1 (proposed): hand-approved publishers with API tokens.** CLAUDE.md allows approving publishers by
hand first. The maintainer creates a publisher (`alice`) and issues a token scoped to that namespace.
`godspeed login` stores it in the operating system's credential store (Windows Credential Manager, macOS
Keychain, the Secret Service on Linux) and never in a project or package file; `godspeed whoami` asks
the API who it belongs to; `godspeed logout` deletes it locally and revokes it remotely. Tokens expire
(90 days proposed; still open) and can be revoked by the maintainer at any time.

**Phase 2: CI without long-lived secrets.** GitHub Actions can present a short-lived OIDC token naming the
repository and workflow; the API maps an allowed repository to a namespace ("trusted publishing", as
PyPI does). Nothing to rotate, nothing to leak. Added without changing the package format.

**Namespaces** are records in the API's database: `alice` belongs to these accounts. Package ownership and
release attribution stay distinct: the index records which account published each version, which need
not be the namespace's first owner.

Where the API keeps tokens, namespaces and reservations: **Cloudflare D1** for the records, and a
**Durable Object per package** to serialise reservation and finalisation, so "exactly one wins" holds
without relying on R2 conditional writes (decided, section 9).

## 5. Why a Worker, and what it runs

- It holds the R2 binding, so no credential ever leaves Cloudflare - "scoped, short-lived upload
  authorisation" is the upload id, valid for one reservation's files for 15 minutes.
- It can run **the same Rust**: `crates/godspeed-package` is `no_std`, so it compiles to WebAssembly for
  `workers-rs` unchanged. The manifest rules and the artifact check are then identical in the CLI, the
  registry and the native package manager - one implementation, not three.
- Request bodies up to 100 MB fit format 1's 64 MiB artifact ceiling, so uploads stream through the
  Worker without presigned URLs. (Presigned R2 URLs remain an option if that ceiling grows.)

Where it lives: **a separate repository** (`godspeed-registry`, decided), depending on
`godspeed-package` from this one. It is infrastructure with its own deploy cycle and secrets, and keeping
it out of the CLI keeps the CLI free of anything Cloudflare-specific.

## 6. Trust: who signed this, and is this the real index

Integrity (section 2) proves files match an index entry. It does not prove the index is the registry's,
that it is current, or that a publisher's key was not swapped. CLAUDE.md names The Update Framework (TUF)
as the reference for exactly that, and this section follows its roles rather than inventing new ones.

| TUF role | Here | Key held |
|---|---|---|
| root | `index/root.json`: lists the keys allowed for every other role, and itself | offline, by the maintainer; 2 of 3 to change it (to confirm) |
| targets, delegated | each **publisher** signs their own release: the canonical bytes of `manifest.toml` | by the publisher, on their machine; CI via phase 2 |
| snapshot | the set of current index files and their digests | online, by the Worker |
| timestamp | `index/timestamp.json`: the snapshot's digest, expiring in a day | online, by the Worker |

What each defends against:

- **A swapped artifact**: the manifest digest no longer matches. (Already true today.)
- **A forged release**: the manifest is not signed by a key the root lists for that namespace.
- **A swapped publisher key**: keys are listed in root-signed metadata, so changing one needs the offline
  root keys, not a compromised Worker.
- **A replayed old index (rollback or freeze)**: the timestamp expires daily and versions only increase;
  a reader refuses metadata older than what it last saw.
- **A compromised Worker**: it can withhold or delay releases, but cannot forge a publisher's signature, so
  it cannot ship code under someone else's name.

**Publisher keys** are Ed25519. **Signing keys are never in a package, a log line, or a repository.**

### Slice 1 - built (2026-10-15): publisher signatures

- `godspeed key generate` makes a key on the publisher's machine and stores the secret half in the
  configuration directory, readable by the user alone (`GODSPEED_SIGNING_KEY` replaces it in CI, never
  stored). Only the public half is ever sent anywhere: the publisher gives it to the maintainer.
- Every release carries `manifest.sig` (`docs/package-format.md` section 4a). `godspeed publish` signs; the
  registry refuses a release whose signature does not verify (`signature-invalid`) or whose key may not sign
  for the namespace (`key-not-registered`) - checked on upload and again at finalisation, so a key retired
  or revoked mid-publish cannot finish one. `manifest.sig` is published beside the manifest.

### Slice 2 - built (2026-10-15): the trust anchor

The roles above, implemented directly from TUF's design and scoped to these four (no TUF library: none is
`no_std`, and the native package manager must verify under the no-heap rule). The formats and a client's
exact checks are `docs/trust-metadata.md`.

- **Which keys may sign for whom lives in `namespaces`, signed OFFLINE** with `godspeed-trust` (in
  `godspeed-registry`), and nowhere else - the registry's database no longer holds keys. The Worker checks
  every new release against it, after verifying the whole root chain exactly as a client does.
- **Retire and revoke are separate actions** (maintainer's decision, 2026-10-15). Retire (rotation): the key
  signs nothing new; releases published before the retirement stay trusted. Revoke (compromise): nothing the
  key ever signed is trusted. Neither alters a published byte.
- **The registry signs `snapshot` and `timestamp` with its one online key** (a Worker secret): the snapshot
  rebuilt from the bucket's actual index files after every publish and whenever asked, the timestamp valid
  for a day and re-signed every six hours by a cron.
- **`godspeed fetch`** is the client: it verifies root, timestamp, snapshot, namespaces, index, manifest,
  signature and files before keeping anything, remembers the root and the newest versions it has seen, and
  refuses rollback.
- **The root threshold is the maintainer's choice when making the root**; the format takes any. The
  recommendation stands: two of three root keys, held apart.

**What it defends against now:** a swapped artifact, index, snapshot or namespaces document; a forged
release; a publisher key added or swapped by anyone without the offline namespaces keys - including a fully
compromised registry; a replayed old index; a key used after its retirement; anything signed by a revoked
key.

**What it does not:**

- **A compromised online key** can withhold or delay releases and show a frozen view - for at most a
  timestamp's lifetime (one day), after which clients refuse it. It cannot forge a publisher's release.
- **First contact trusts root 1** unless the client pins its digest (`--trust-root`). The official
  registry's root 1 digest should be published somewhere independent of the registry - its documentation,
  the CLI's release notes - and, eventually, built into the CLI.
- **A stolen offline key** is as bad as it sounds; the threshold is what makes one stolen key not enough.

## 7. What is not in this design

- Deleting or replacing a published version. Immutable means immutable; a bad release is superseded by
  a newer one and may be **yanked** (marked in the index as not-for-new-installs) - a later, separate design.
- Self-service publisher registration, accounts, a web frontend, ratings, download counts, or analytics.
- Private registries. Nothing here prevents one: the CLI takes a registry URL, and the format names none.

## 8. Order of work

1. This design, reviewed and decided.
2. The registry repository: the Worker, reservation and finalisation, D1 records, the static read path on
   the custom domain. Tested against a local R2 (`wrangler dev`) before the real bucket.
3. The CLI: `login`/`logout`/`whoami` (phase 1 tokens), `publish`, and the unknown-outcome reconciliation.
4. Signing: publisher keys, `manifest.sig`, root/snapshot/timestamp, verification in the package crate.
   **The first public publish waits for this step** (CLAUDE.md: key registration, rotation and revocation
   are "defined before the first public publish, not after").
5. Phase 2 trusted publishing from CI.

## 9. Decisions (taken 2026-10-15)

1. **Phase 1 authentication: hand-issued, namespace-scoped API tokens.** Lifetime still open (90 days
   proposed).
2. **The control plane is a Cloudflare Worker, in a separate `godspeed-registry` repository.**
3. **D1 plus a Durable Object per package** for reservations (taken with 2: the Worker's design).
4. **TUF's four roles, implemented directly** (no TUF library), so verification can be `no_std`. Root
   keys offline, 2 of 3 still to confirm.
5. Open: the custom domain's name, and whether it is the CLI's default registry URL.
