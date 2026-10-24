//! The Godspeed package contract.
//!
//! What a Godspeed package IS, as types and rules that the developer CLI (which produces packages) and the
//! native package manager (which verifies and installs them) both depend on, so the two can never disagree
//! about it. The format is described in `docs/package-format.md`.
//!
//! # The two halves
//!
//! - **The core** - names, versions, architectures, digests, the manifest model, its validation, and
//!   streaming artifact verification - is `no_std` and never allocates. The native package manager runs on
//!   Godspeed, under the OS's no-heap rule, and must be able to use all of it.
//!   Building for `x86_64-unknown-none` (or any x86_64 target without SSE) needs
//!   `--cfg curve25519_dalek_backend="serial"` in RUSTFLAGS: the Ed25519 library otherwise picks a SIMD
//!   backend that target cannot compile, and offers no Cargo feature to choose. Other targets need nothing.
//! - **The `toml` feature** reads and writes the manifest's text form. It needs `std` and a heap, so it is
//!   host-only. (The native side reads the text without a heap with a reader of its own, in GodspeedOS; see the
//!   format document.)
//!
//! # What this crate does not decide
//!
//! - **Authority.** A package carries its OS service contract, which REQUESTS capabilities. Nothing here
//!   grants anything, and nothing here restates the contract's capabilities: the manifest binds the
//!   contract by digest, so there is one declaration of a service's authority, not two.
//! - **Trust.** A digest proves bytes are the ones the manifest names. A signature (`manifest.sig`) proves
//!   which key signed the manifest. Neither says who holds the key - that is the registry's trust model -
//!   nor whether the package is safe.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(any(test, feature = "std"))]
extern crate std;

mod arch;
mod digest;
pub mod gsp;
mod hex;
mod index;
mod manifest;
mod name;
mod sign;
#[cfg(feature = "toml")]
mod text;
pub mod trust;
mod verify;
mod version;

pub use arch::Arch;
pub use digest::{DigestError, Sha256Digest, Sha256Stream};
#[cfg(feature = "toml")]
pub use index::{index_to_toml, parse_index, OwnedPackageIndex};
pub use index::{is_timestamp, PackageIndex, Release, INDEX_FORMAT};
pub use manifest::{
    Artifact, Kind, Manifest, Problem, ProblemKind, Requirement, CONTRACT_FILE, FORMAT, FORMAT_REQUIRES,
    MAX_ARTIFACT_SIZE, MAX_REQUIRES,
};
pub use name::{NameError, PackageName, FULL_MAX, PART_MAX};
pub use sign::{
    signed_message, ManifestSignature, PublicKey, Signature, SignatureError, MANIFEST_CONTEXT, SIGNATURE_FILE,
    SIGNATURE_FORMAT,
};
#[cfg(feature = "toml")]
pub use text::{parse, to_toml, OwnedManifest, ParseError};
pub use verify::{ArtifactVerifier, VerifyError};
pub use version::{Version, VersionError, VersionReq, VersionReqError};
