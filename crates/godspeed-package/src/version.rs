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

/// A range of versions a requirement accepts (GodspeedOS `docs/developer-services-design.md` 16.2): one of
/// three forms, each written with the whole version, because before 1.0 a shortened caret is ambiguous
/// (`^0` and `^0.0.0` accept different versions).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionReq {
    /// `^1.2.0`: the same major version and at least this one; for `0.x` the same minor, and for `0.0.z`
    /// exactly that version - before 1.0 a minor change is allowed to break.
    Caret(Version),
    /// `=1.2.3`: exactly this version.
    Exact(Version),
    /// `>=1.2.0`: this version or any later one, later majors included.
    AtLeast(Version),
}

/// Why a string is not a version range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionReqError {
    /// It does not begin with `^`, `=` or `>=`.
    NoOperator,
    /// What follows the operator is not a version.
    Version(VersionError),
}

impl VersionReqError {
    /// What the rule is, in one line.
    pub const fn message(self) -> &'static str {
        match self {
            VersionReqError::NoOperator => {
                "a version range is ^MAJOR.MINOR.PATCH, =MAJOR.MINOR.PATCH or >=MAJOR.MINOR.PATCH"
            }
            VersionReqError::Version(e) => e.message(),
        }
    }
}

impl fmt::Display for VersionReqError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl VersionReq {
    /// Whether `v` is in the range.
    pub fn matches(&self, v: &Version) -> bool {
        match *self {
            VersionReq::Exact(b) => *v == b,
            VersionReq::AtLeast(b) => *v >= b,
            VersionReq::Caret(b) => {
                *v >= b
                    && if b.major > 0 {
                        v.major == b.major
                    } else if b.minor > 0 {
                        v.major == 0 && v.minor == b.minor
                    } else {
                        *v == b
                    }
            }
        }
    }
}

impl FromStr for VersionReq {
    type Err = VersionReqError;

    fn from_str(s: &str) -> Result<Self, VersionReqError> {
        let (make, rest): (fn(Version) -> VersionReq, &str) = if let Some(r) = s.strip_prefix(">=") {
            (VersionReq::AtLeast, r)
        } else if let Some(r) = s.strip_prefix('=') {
            (VersionReq::Exact, r)
        } else if let Some(r) = s.strip_prefix('^') {
            (VersionReq::Caret, r)
        } else {
            return Err(VersionReqError::NoOperator);
        };
        rest.parse().map(make).map_err(VersionReqError::Version)
    }
}

impl fmt::Display for VersionReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionReq::Caret(v) => write!(f, "^{v}"),
            VersionReq::Exact(v) => write!(f, "={v}"),
            VersionReq::AtLeast(v) => write!(f, ">={v}"),
        }
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

    #[test]
    fn ranges_read_and_match_as_written() {
        let r = |s: &str| s.parse::<VersionReq>().unwrap();
        let v = |s: &str| s.parse::<Version>().unwrap();
        assert!(r("^1.2.0").matches(&v("1.2.0")) && r("^1.2.0").matches(&v("1.9.3")));
        assert!(!r("^1.2.0").matches(&v("1.1.9")) && !r("^1.2.0").matches(&v("2.0.0")));
        assert!(
            r("^0.3.1").matches(&v("0.3.5")) && !r("^0.3.1").matches(&v("0.4.0")) && !r("^0.3.1").matches(&v("0.3.0"))
        );
        assert!(r("^0.0.3").matches(&v("0.0.3")) && !r("^0.0.3").matches(&v("0.0.4")));
        assert!(r("=1.2.3").matches(&v("1.2.3")) && !r("=1.2.3").matches(&v("1.2.4")));
        assert!(r(">=1.2.0").matches(&v("7.0.0")) && !r(">=1.2.0").matches(&v("1.1.9")));
        for s in ["^1.2.0", "=1.2.3", ">=0.1.0"] {
            assert_eq!(std::format!("{}", r(s)), s, "one range, one spelling");
        }
    }

    #[test]
    fn refuses_what_is_not_a_range() {
        assert_eq!("1.2.0".parse::<VersionReq>(), Err(VersionReqError::NoOperator));
        assert_eq!("~1.2.0".parse::<VersionReq>(), Err(VersionReqError::NoOperator));
        assert_eq!(
            "^1.2".parse::<VersionReq>(),
            Err(VersionReqError::Version(VersionError::NotThreeParts))
        );
        assert_eq!(
            ">=1.02.0".parse::<VersionReq>(),
            Err(VersionReqError::Version(VersionError::LeadingZero))
        );
    }
}
