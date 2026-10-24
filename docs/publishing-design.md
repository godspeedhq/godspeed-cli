# Publishing: the registry protocol, its control plane, and its trust model

**Status: BUILT and live, decisions taken 2026-10-15 (section 9).** `godspeed-registry` is the Worker at
`https://godspeed-registry.bankole-422.workers.dev`, first published to on 2026-10-16, and `godspeed publish`
uses it. Phase 2 (trusted publishing from CI, section 4) is not built, and neither is the custom domain.

Storage is chosen: the Cloudflare R2 bucket `godspeedos-packages`, with a custom domain to come. Everything
else here is built so that choice stays an implementation detail (CLAUDE.md, "External Services").

---

## 1. Shape

    developer                        Godspeed machine / anyone
       |                                     |
       | godspeed publish                    | godspeed install, search, info  (or curl)
       v                                     v
    publishing API  (Cloudflare Worker)    https://<custom domain>/   (static, read-only; not yet:
                                           the Worker serves reads today)
       |   authenticates the publisher            ^
       |   checks the namespace                   |
       |   verifies the package                   |
       |   finalises atomically                   |
       v                                          |
    R2 bucket godspeedos-packages  ---------------+
       index/      mutable, small, signed
       packages/   immutable, content-checked

Two paths, deliberately separate:

- **Reading is meant to be static.** The index and every package are plain files, to be served from the
  custom domain: no API call, no account, no R2-specific interface, and downloads that keep working when the
  publishing API is down. **Until that domain exists, the Worker serves the reads too** (`GET /trust/...`,
  `/index/...`, `/packages/...`, straight from the bucket), so today a Worker outage stops downloads as well
  as publishing.
- **Writing goes through one small trusted service**, which holds the bucket binding. No developer, CI job
  or CLI ever holds storage credentials, unrestricted or otherwise.

The R2 S3 endpoint (`<account>.r2.cloudflarestorage.com`) is on neither path: it requires signed requests
and is an administrator's tool. The CLI never names it.

## 2. The bucket layout

    packages/<publisher>/<name>/<version>/manifest.toml
    packages/<publisher>/<name>/<version>/manifest.sig
    packages/<publisher>/<name>/<version>/contract.toml
    packages/<publisher>/<name>/<version>/<arch>.elf
    index/<publisher>/<name>.toml
    trust/root/<n>.toml, trust/namespaces.toml, trust/snapshot.toml, trust/timestamp.toml
                               (each with a .sig; section 6, `docs/trust-metadata.md`)

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

    [[release]]
    version = "1.0.0"
    manifest = "sha256:..."     # digest of that version's manifest.toml
    published = "2026-11-02T10:14:00Z"
    published_by = "alice"      # the authenticated account that published it (section 4)

    [[release]]
    version = "1.1.0"
    ...

The manifest digest is the link from mutable to immutable: the index names a manifest by its bytes, and the
manifest names every other file by its bytes. Trust the index entry and you can verify everything under
it without trusting the storage, the CDN or the network.

`godspeed search` and `godspeed info`, on a Godspeed machine, read these files. There is no search service
and no separate listing: search reads the signed snapshot, which names every package's index, and then the
indexes (`backlog/02` is what that costs at scale).

## 3. Publishing, step by step

The CLI's side, in the order its output reports it (CLAUDE.md, "Publishing Experience"), each step
reported only once its outcome is established:

1. **Validate, build, package, verify**: `godspeed package`, unchanged.
2. **Sign** the manifest with the publisher's key (section 6), on the publisher's machine.
3. **Authenticate** (section 4).
4. **Reserve** `alice/hello@1.0.0`: `POST /v1/packages/alice/hello/reservations` with the manifest. The API checks the token may
   publish under `alice`, that 1.0.0 does not exist and is not reserved, and that the manifest is valid.
   It returns an upload id and an expiry (15 minutes). **Concurrent publishes are decided here**: exactly
   one reservation per version succeeds; the other is told "1.0.0 is being published by another upload"
   or "1.0.0 already exists", and nothing it does can overwrite the winner.
5. **Upload** each file, `manifest.sig` among them: `PUT /v1/packages/alice/hello/1.0.0/uploads/<id>/<file>`. The Worker writes to `staging/`. Only the files
   the reserved manifest names are accepted, each checked against its declared size and digest as it
   streams in - the same `ArtifactVerifier`, compiled to WebAssembly, that the CLI and the native package
   manager use. A file that fails is refused at once.
6. **Finalise**: `POST /v1/packages/alice/hello/1.0.0/uploads/<id>/finalise`. The API re-verifies the whole
   staged package and its signature, copies it to `packages/.../<version>/`, then writes the index entry,
   then re-signs the snapshot and the timestamp.
   **The index write is the commit point.** Before it, the version does not exist for any reader; after it,
   every file it names is already in place. A crash anywhere before it leaves only staged files, which no
   index names and the lifecycle rule sweeps.
7. **Report** `Published alice/hello@1.0.0` and the lines to install it (`godspeed fetch`, then
   `godspeed install`, on a machine).

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

**Phase 1 (built): publisher-made tokens, approved by hand** (maintainer's decision, 2026-10-15). CLAUDE.md
allows approving publishers by hand first. The publisher makes their token on their own machine
(`godspeed login`, or `godspeed token new` for CI) and sends the maintainer only its fingerprint - its
SHA-256 - like an SSH public key. The maintainer approves the fingerprint for an account (the registry's
`admin` workflow), so the token itself never travels and never appears in any log, and the registry stores
only the fingerprint. `godspeed whoami` asks the API who the token belongs to; `godspeed logout` deletes it
locally and revokes it remotely. An approval lasts 90 days by default (the maintainer chooses, 1 to 366) and
can be revoked at any time. The token is kept in a file readable by the user alone, not an OS credential
store: on Linux that needs D-Bus and a C library the static builds cannot carry.

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
| root | `trust/root/<n>.toml`: lists the keys allowed for every other role, and itself | offline, by the maintainer; 2 of 3 to change it |
| targets, delegated | each **publisher** signs their own release: the canonical bytes of `manifest.toml` | by the publisher, on their machine; CI via phase 2 |
| snapshot | the set of current index files and their digests | online, by the Worker |
| namespaces | `trust/namespaces.toml`: each publisher's keys, active, retired or revoked | offline, by the maintainer; 1 of 2 |
| timestamp | `trust/timestamp.toml`: the snapshot's digest, expiring in a day | online, by the Worker |

What each defends against:

- **A swapped artifact**: the manifest digest no longer matches. (Already true today.)
- **A forged release**: the manifest is not signed by a key the offline-signed `namespaces` document lists
  for that publisher.
- **A swapped publisher key**: keys are listed in `namespaces`, signed offline, so changing one needs the
  offline namespaces keys, not a compromised Worker.
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
- **The root threshold is the maintainer's choice when making the root**; the format takes any. Root 1,
  the official registry's, is two of three root keys, held apart.

**What it defends against now:** a swapped artifact, index, snapshot or namespaces document; a forged
release; a publisher key added or swapped by anyone without the offline namespaces keys - including a fully
compromised registry; a replayed old index; a key used after its retirement; anything signed by a revoked
key.

**What it does not:**

- **A compromised online key** can withhold or delay releases and show a frozen view - for at most a
  timestamp's lifetime (one day), after which clients refuse it. It cannot forge a publisher's release.
- **First contact trusts root 1** unless the client pins its digest (`--trust-root`). The official
  registry's root 1 digest is built into the CLI, and published in its documentation, independent of the
  registry.
- **A stolen offline key** is as bad as it sounds; the threshold is what makes one stolen key not enough.

## 7. What is not in this design

- Deleting or replacing a published version. Immutable means immutable; a bad release is superseded by
  a newer one and may be **yanked** (marked in the index as not-for-new-installs) - a later, separate design.
- Self-service publisher registration, accounts, a web frontend, ratings, download counts, or analytics.
- Private registries. Nothing here prevents one: the CLI takes a registry URL, and the format names none.

## 8. Order of work

1. This design, reviewed and decided. Done, 2026-10-15.
2. The registry repository: the Worker, reservation and finalisation, D1 records. Done, tested against a
   local R2 (`wrangler dev`) before the real bucket. The static read path on the custom domain is not built:
   the Worker serves reads.
3. The CLI: `login`/`logout`/`whoami` (phase 1 tokens), `publish`, and the unknown-outcome reconciliation.
   Done.
4. Signing: publisher keys, `manifest.sig`, root/namespaces/snapshot/timestamp, verification in the package
   crate. Done, before the first public publish on 2026-10-16, as CLAUDE.md requires.
5. Phase 2 trusted publishing from CI. Not built.

## 9. Decisions (taken 2026-10-15)

1. **Phase 1 authentication: hand-issued, namespace-scoped API tokens.** Lifetime 90 days by default, 1 to
   366 at the maintainer's choice when approving one.
2. **The control plane is a Cloudflare Worker, in a separate `godspeed-registry` repository.**
3. **D1 plus a Durable Object per package** for reservations (taken with 2: the Worker's design).
4. **TUF's four roles, implemented directly** (no TUF library), so verification can be `no_std`. Root
   keys offline, two of three.
5. Open: the custom domain's name. When it exists, the CLI's default registry moves to it in a CLI release;
   until then the default is the Worker's own URL.
