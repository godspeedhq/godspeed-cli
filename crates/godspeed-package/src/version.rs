//! Versions: `MAJOR.MINOR.PATCH`.

use core::fmt;
use core::str::FromStr;

/// A package version, or an OS release: three unsigned numbers.
///
/// The OS contract schema allows `N.N.N` and nothing else, so there are no pre-release or build suffixes
/// here either. Leading zeros are refused, so every version has exactly one spelling: `1.01.0` and
/// `1.1.0` naming the same version would make "the same version is never replaced with different bytes"
/// depend on how someone typed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    /// Incompatible changes.
    pub major: u32,
    /// Compatible additions.
    pub minor: u32,
    /// Compatible fixes.
    pub patch: u32,
}

/// Why a string is not a version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionError {
    /// Not three dot-separated parts.
    NotThreeParts,
    /// A part is empty or not all ASCII digits.
    NotANumber,
    /// A part has a leading zero, so the version would have two spellings.
    LeadingZero,
    /// A part does not fit in 32 bits.
    TooLarge,
}

impl VersionError {
    /// What the rule is, in one line.
    pub const fn message(self) -> &'static str {
        match self {
            VersionError::NotThreeParts => "a version is MAJOR.MINOR.PATCH",
            VersionError::NotANumber => "each part of a version is a decimal number",
            VersionError::LeadingZero => "a version part has no leading zero (1.2.0, not 1.02.0)",
            VersionError::TooLarge => "a version part is at most 4294967295",
        }
    }
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

fn part(s: &str) -> Result<u32, VersionError> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(VersionError::NotANumber);
    }
    if s.len() > 1 && s.starts_with('0') {
        return Err(VersionError::LeadingZero);
    }
    s.parse().map_err(|_| VersionError::TooLarge)
}

impl Version {
    /// A version from its parts.
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Version { major, minor, patch }
    }
}

impl FromStr for Version {
    type Err = VersionError;

    fn from_str(s: &str) -> Result<Self, VersionError> {
        let mut it = s.split('.');
        let (Some(a), Some(b), Some(c), None) = (it.next(), it.next(), it.next(), it.next()) else {
            return Err(VersionError::NotThreeParts);
        };
        Ok(Version {
            major: part(a)?,
            minor: part(b)?,
            patch: part(c)?,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_orders() {
        let v: Version = "1.10.2".parse().unwrap();
        assert_eq!(v, Version::new(1, 10, 2));
        assert_eq!(std::format!("{v}"), "1.10.2");
        assert!(Version::new(1, 9, 9) < v);
        assert!(Version::new(2, 0, 0) > v);
        assert_eq!("0.0.0".parse(), Ok(Version::new(0, 0, 0)));
    }

    #[test]
    fn refuses_what_is_not_a_version() {
        use VersionError::*;
        for (s, e) in [
            ("1.2", NotThreeParts),
            ("1.2.3.4", NotThreeParts),
            ("1.2.x", NotANumber),
            ("1..3", NotANumber),
            ("1.2.3-beta", NotANumber),
            ("+1.2.3", NotANumber),
            ("1.02.3", LeadingZero),
            ("1.2.4294967296", TooLarge),
        ] {
            assert_eq!(s.parse::<Version>(), Err(e), "{s}");
        }
    }
}
