# 3. An offline file does not carry what its package requires

**Status: OPEN, recorded 2026-10-23 at the operator's request.** `godspeed fetch <pkg> --offline` writes one
release into one `.gsp`. A package with `[[requires]]` (manifest format 2) therefore needs one file per
requirement, installed in order, on a machine with no network. The operator's suggestion: a flag,
`godspeed fetch <pkg> --offline --dependencies`, that writes the package and everything it requires into one
file.

## What is wrong

A `.gsp` is meant to be the whole of what an offline machine needs. For a package with requirements it is not:

- the file holds the package's release and no other, so installing it on a machine that has never seen the
  requirement is refused, naming the requirement;
- the operator must find each requirement, `fetch --offline` it separately, carry every file and install them
  dependencies first. That works today (it is how `osdev test deps` installs `dep-user`), but nothing tells
  the operator the order, and nothing checks that the files carried agree with each other.

## Evidence

- `src/cmd/fetch.rs`, `run`: an offline fetch keeps the documents of ONE release (`Source::keep`) and writes
  them with `pkg::gsp::write`; `gsp_spec` reads "the one index and the one release a `.gsp` holds".
- The OS repository: `services/godspeed-pm/src/packages.rs`, `unpack`: "a package file holds one index and one
  release", so a file carrying more is not read today either.
- The design: the OS repository's `docs/developer-services-design.md` 16.7, approved 2026-10-19 and not built.
  It gives the same reason this item is open: until the registry read format 2 (redeployed 2026-10-23,
  `godspeed-registry` PR #4), a file could not carry a requirement the registry had.

## Ruled out

- **Concatenating several `.gsp` files.** Each starts with its own header and ends with its own end line, so the
  result is not one valid file, and the OS reader rightly refuses it.
- **Carrying the requirements unsigned, or checked once on the workstation.** Section 15 of the design does not
  change: every release in the file is checked on the machine exactly as a fetched one is.

## To be decided by the operator

- **A flag, or the default.** The operator suggested `--dependencies`. Design 16.7 says `--offline` carries the
  requirements without being asked. With the flag, the plain `--offline` file can still be refused on a
  machine that lacks a requirement. With the default, one command always produces a file that installs alone.
  Either way the file is the same.

## The next concrete step

1. **The CLI**: resolve each requirement against the registry (`publish` already checks that a release
   satisfies it), take the highest release in its range, fetch and verify each, and write every document
   into the one file. `inspect <file>` lists every release the file carries. The `.gsp` layout already
   allows several documents (up to `pkg::gsp::ENTRIES_MAX`, 64), so the format stays; the "one index and one
   release" rule in `gsp_spec` is what changes.
2. **The OS** (its own item, in its repository): `unpack` reads every release in the file, and install resolves
   within the file and what is already installed, refusing an unsatisfied requirement by name (16.7).
3. **Tested end to end** once both are released: a format-2 package published to the registry (that needs the
   operator's approval), fetched with its requirements into one file, installed on a machine with no network.
