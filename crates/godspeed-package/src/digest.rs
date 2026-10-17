//! Content digests: `sha256:<64 lowercase hex digits>`.

use core::fmt;
use core::str::FromStr;
use sha2::{Digest as _, Sha256};

/// A SHA-256 digest of some bytes.
///
/// Written with its algorithm (`sha256:...`) so a later format can add another without ambiguity. It
/// establishes that bytes are the ones the manifest named - integrity - and nothing about who produced
/// them.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sha256Digest(pub [u8; 32]);

/// Why a string is not a digest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigestError {
    /// It does not start with `sha256:`, the only algorithm this format knows.
    UnknownAlgorithm,
    /// The hex part is not exactly 64 lowercase hex digits.
    BadHex,
}

impl DigestError {
    /// What the rule is, in one line.
    pub const fn message(self) -> &'static str {
        match self {
            DigestError::UnknownAlgorithm => "a digest starts with \"sha256:\"",
            DigestError::BadHex => "a sha256 digest is 64 lowercase hex digits",
        }
    }
}

impl fmt::Display for DigestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl Sha256Digest {
    /// The digest of `bytes`, all at once. For bytes that arrive in pieces, [`Sha256Stream`]; for an artifact,
    /// whose size the manifest also states, [`crate::ArtifactVerifier`].
    pub fn of(bytes: &[u8]) -> Self {
        Sha256Digest(Sha256::digest(bytes).into())
    }
}

/// A SHA-256 taken in pieces: the same digest [`Sha256Digest::of`] gives the bytes joined. For a document too
/// large to hold - the snapshot, kept on disk by a client with no heap - hashed as it is written.
#[derive(Clone, Default)]
pub struct Sha256Stream(Sha256);

impl Sha256Stream {
    /// Nothing hashed yet.
    pub fn new() -> Self {
        Sha256Stream(Sha256::new())
    }

    /// Hash the next piece.
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    /// The digest of every piece, in order.
    pub fn finish(self) -> Sha256Digest {
        Sha256Digest(self.0.finalize().into())
    }
}

fn nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None, // uppercase is refused: one spelling per digest
    }
}

impl FromStr for Sha256Digest {
    type Err = DigestError;

    fn from_str(s: &str) -> Result<Self, DigestError> {
        let hex = s
            .strip_prefix("sha256:")
            .ok_or(DigestError::UnknownAlgorithm)?
            .as_bytes();
        if hex.len() != 64 {
            return Err(DigestError::BadHex);
        }
        let mut out = [0u8; 32];
        for (i, pair) in hex.chunks_exact(2).enumerate() {
            let hi = nibble(pair[0]).ok_or(DigestError::BadHex)?;
            let lo = nibble(pair[1]).ok_or(DigestError::BadHex)?;
            out[i] = hi << 4 | lo;
        }
        Ok(Sha256Digest(out))
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("sha256:")?;
        for b in self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SHA-256 of the empty string, from FIPS 180-2 test vectors.
    const EMPTY: &str = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn known_vector_and_round_trip() {
        let d = Sha256Digest::of(b"");
        assert_eq!(std::format!("{d}"), EMPTY);
        assert_eq!(EMPTY.parse(), Ok(d));
    }

    #[test]
    fn refuses_other_spellings() {
        assert_eq!("md5:00".parse::<Sha256Digest>(), Err(DigestError::UnknownAlgorithm));
        assert_eq!("sha256:abc".parse::<Sha256Digest>(), Err(DigestError::BadHex));
        let upper = EMPTY.to_uppercase().replace("SHA256:", "sha256:");
        assert_eq!(upper.parse::<Sha256Digest>(), Err(DigestError::BadHex));
    }
}
