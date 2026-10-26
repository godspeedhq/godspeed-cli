// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Streaming artifact verification, with no heap.

use crate::digest::Sha256Digest;
use crate::manifest::Artifact;
use core::fmt;
use sha2::{Digest as _, Sha256};

/// Checks that a stream of bytes is exactly the artifact a manifest names: the right length and the
/// right SHA-256 digest.
///
/// Feed it chunks as they arrive, from a download or a file, in a buffer the caller owns; it holds only
/// the hash state. It refuses as soon as more bytes arrive than the manifest declared, so a hostile or
/// broken source cannot make it hash forever.
///
/// Passing proves integrity only: these bytes are the ones the manifest names. Whether the manifest
/// itself can be trusted is a separate question this type does not answer.
pub struct ArtifactVerifier {
    expected: Artifact,
    seen: u64,
    hasher: Sha256,
}

/// Why bytes are not the artifact the manifest names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerifyError {
    /// More bytes arrived than the manifest declared. Detected while streaming.
    TooLong {
        /// The size the manifest declares.
        expected: u64,
    },
    /// The stream ended before the declared size: a truncated or interrupted transfer.
    TooShort {
        /// The size the manifest declares.
        expected: u64,
        /// The bytes actually received.
        actual: u64,
    },
    /// The size matched and the content did not.
    DigestMismatch {
        /// The digest the manifest declares.
        expected: Sha256Digest,
        /// The digest of the bytes received.
        actual: Sha256Digest,
    },
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerifyError::TooLong { expected } => {
                write!(f, "more than the declared {expected} bytes arrived")
            }
            VerifyError::TooShort { expected, actual } => {
                write!(f, "{actual} of the declared {expected} bytes arrived")
            }
            VerifyError::DigestMismatch { expected, actual } => {
                write!(f, "checksum mismatch: expected {expected}, got {actual}")
            }
        }
    }
}

impl ArtifactVerifier {
    /// Start verifying against `expected`.
    pub fn new(expected: Artifact) -> Self {
        ArtifactVerifier {
            expected,
            seen: 0,
            hasher: Sha256::new(),
        }
    }

    /// Feed the next chunk. Once this returns an error the verification has failed; further calls keep
    /// failing.
    pub fn update(&mut self, chunk: &[u8]) -> Result<(), VerifyError> {
        let len = chunk.len() as u64;
        match self.seen.checked_add(len) {
            Some(total) if total <= self.expected.size => {
                self.seen = total;
                self.hasher.update(chunk);
                Ok(())
            }
            _ => {
                self.seen = u64::MAX; // poisoned: `finish` cannot pass after this
                Err(VerifyError::TooLong {
                    expected: self.expected.size,
                })
            }
        }
    }

    /// The stream has ended: check the length and the digest.
    pub fn finish(self) -> Result<(), VerifyError> {
        if self.seen == u64::MAX {
            return Err(VerifyError::TooLong {
                expected: self.expected.size,
            });
        }
        if self.seen != self.expected.size {
            return Err(VerifyError::TooShort {
                expected: self.expected.size,
                actual: self.seen,
            });
        }
        let actual = Sha256Digest(self.hasher.finalize().into());
        if actual != self.expected.digest {
            return Err(VerifyError::DigestMismatch {
                expected: self.expected.digest,
                actual,
            });
        }
        Ok(())
    }

    /// Verify a whole buffer at once.
    pub fn verify(expected: Artifact, bytes: &[u8]) -> Result<(), VerifyError> {
        let mut v = ArtifactVerifier::new(expected);
        v.update(bytes)?;
        v.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arch::Arch;

    fn artifact(bytes: &[u8]) -> Artifact {
        Artifact {
            arch: Arch::X86_64,
            size: bytes.len() as u64,
            digest: Sha256Digest::of(bytes),
        }
    }

    #[test]
    fn the_right_bytes_pass_in_any_chunking() {
        let bytes: std::vec::Vec<u8> = (0..1000u32).map(|i| (i * 7) as u8).collect();
        let a = artifact(&bytes);
        for chunk in [1, 7, 64, 999, 1000] {
            let mut v = ArtifactVerifier::new(a);
            for c in bytes.chunks(chunk) {
                v.update(c).unwrap();
            }
            assert_eq!(v.finish(), Ok(()), "chunk size {chunk}");
        }
    }

    #[test]
    fn corrupted_truncated_and_padded_bytes_fail_distinctly() {
        let good = b"godspeed service image".as_slice();
        let a = artifact(good);

        let mut corrupt = good.to_vec();
        corrupt[3] ^= 1;
        assert!(matches!(
            ArtifactVerifier::verify(a, &corrupt),
            Err(VerifyError::DigestMismatch { .. })
        ));

        assert_eq!(
            ArtifactVerifier::verify(a, &good[..10]),
            Err(VerifyError::TooShort {
                expected: good.len() as u64,
                actual: 10
            })
        );

        let mut v = ArtifactVerifier::new(a);
        v.update(good).unwrap();
        assert_eq!(
            v.update(b"x"),
            Err(VerifyError::TooLong {
                expected: good.len() as u64
            })
        );
        assert_eq!(
            v.finish(),
            Err(VerifyError::TooLong {
                expected: good.len() as u64
            })
        );
    }
}
