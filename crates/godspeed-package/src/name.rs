//! Package names: `publisher/name`.

use core::fmt;

/// The longest a publisher or a name may be, in bytes.
pub const PART_MAX: usize = 64;

/// The longest a whole package name, `publisher/name`, may be, in bytes. An installed package runs under
/// this name and is reached by it, and 32 bytes is the longest name a GodspeedOS machine's name directory and
/// task table hold. A longer name would install and then be unreachable by its own clients, so it is refused
/// when it is made: at `godspeed check` and `package`, at the registry, and on the machine at install.
pub const FULL_MAX: usize = 32;

/// A package name, namespaced by its publisher: `alice/clock`.
///
/// Each part follows the OS contract schema's rule for a service name - a lowercase ASCII letter, then
/// lowercase letters, digits and hyphens - so a package's `name` part can always be its service's name.
/// A package name is an identifier, not an identity: it says nothing about who published it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PackageName<'a> {
    publisher: &'a str,
    name: &'a str,
}

/// Why a string is not a package name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameError {
    /// There is no `/` between publisher and name.
    NoPublisher,
    /// More than one `/`.
    TooManyParts,
    /// The publisher or the name is empty.
    EmptyPart,
    /// The publisher or the name is longer than [`PART_MAX`].
    TooLong,
    /// `publisher/name` as a whole is longer than [`FULL_MAX`].
    FullTooLong,
    /// A part does not start with a lowercase ASCII letter.
    BadStart,
    /// A part contains something other than lowercase ASCII letters, digits and hyphens.
    BadCharacter,
}

impl NameError {
    /// What the rule is, in one line.
    pub const fn message(self) -> &'static str {
        match self {
            NameError::NoPublisher => "a package name is publisher/name, and this has no publisher",
            NameError::TooManyParts => "a package name has exactly one '/'",
            NameError::EmptyPart => "the publisher and the name must both be non-empty",
            NameError::TooLong => "the publisher and the name are each at most 64 bytes",
            NameError::FullTooLong => {
                "publisher/name is at most 32 bytes in all, the longest name a GodspeedOS machine reaches a program by"
            }
            NameError::BadStart => "the publisher and the name each start with a lowercase letter",
            NameError::BadCharacter => "the publisher and the name use only lowercase letters, digits and '-'",
        }
    }
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

fn check_part(part: &str) -> Result<(), NameError> {
    let bytes = part.as_bytes();
    let Some(&first) = bytes.first() else {
        return Err(NameError::EmptyPart);
    };
    if bytes.len() > PART_MAX {
        return Err(NameError::TooLong);
    }
    if !first.is_ascii_lowercase() {
        return Err(NameError::BadStart);
    }
    if !bytes
        .iter()
        .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(NameError::BadCharacter);
    }
    Ok(())
}

impl<'a> PackageName<'a> {
    /// Parse `publisher/name`.
    pub fn parse(s: &'a str) -> Result<Self, NameError> {
        let (publisher, name) = s.split_once('/').ok_or(NameError::NoPublisher)?;
        if name.contains('/') {
            return Err(NameError::TooManyParts);
        }
        check_part(publisher)?;
        check_part(name)?;
        if s.len() > FULL_MAX {
            return Err(NameError::FullTooLong);
        }
        Ok(PackageName { publisher, name })
    }

    /// Whether `s` is a valid service or package name part on its own (no publisher).
    pub fn is_valid_part(s: &str) -> bool {
        check_part(s).is_ok()
    }

    /// The namespace: `alice` in `alice/clock`.
    pub fn publisher(&self) -> &'a str {
        self.publisher
    }

    /// The name within the namespace: `clock` in `alice/clock`.
    pub fn name(&self) -> &'a str {
        self.name
    }
}

impl fmt::Display for PackageName<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.publisher, self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_namespaced_names() {
        let n = PackageName::parse("alice/clock-2").unwrap();
        assert_eq!((n.publisher(), n.name()), ("alice", "clock-2"));
        assert_eq!(std::format!("{n}"), "alice/clock-2");
    }

    #[test]
    fn refuses_what_is_not_a_package_name() {
        use NameError::*;
        for (s, e) in [
            ("clock", NoPublisher),
            ("a/b/c", TooManyParts),
            ("/clock", EmptyPart),
            ("alice/", EmptyPart),
            ("Alice/clock", BadStart),
            ("alice/1clock", BadStart),
            ("alice/clo_ck", BadCharacter),
            ("alice/clöck", BadCharacter),
            ("alice/../x", TooManyParts),
            ("../x", BadStart),
            ("alice/..", BadStart),
        ] {
            assert_eq!(PackageName::parse(s), Err(e), "{s}");
        }
        let long = "a".repeat(PART_MAX + 1);
        assert_eq!(PackageName::parse(&std::format!("alice/{long}")), Err(TooLong));
        // Within each part's limit, but past the whole name's: refused as too long in all.
        assert_eq!(
            PackageName::parse(&std::format!("alice/{}", "a".repeat(PART_MAX))),
            Err(FullTooLong)
        );
        // Exactly FULL_MAX bytes is accepted, one more is not.
        let fits = std::format!("alice/{}", "a".repeat(FULL_MAX - "alice/".len()));
        assert_eq!(fits.len(), FULL_MAX);
        assert!(PackageName::parse(&fits).is_ok());
        assert_eq!(PackageName::parse(&std::format!("{fits}a")), Err(FullTooLong));
        // The longest names published so far fit.
        assert!(PackageName::parse("godspeed/selfcheck-service").is_ok());
    }
}
