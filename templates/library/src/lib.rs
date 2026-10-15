//! {{name}}: a Godspeed library.
//!
//! Plain `no_std` Rust that needs no heap, so it builds for every Godspeed architecture, and its tests
//! run on your workstation with `godspeed test`. Add a dependency on the `godspeed` stdlib only when
//! the library needs to talk to the system; then its tests can no longer run off the target.

#![cfg_attr(not(test), no_std)]
#![deny(unsafe_code)]

/// Parse an unsigned decimal number from ASCII digits.
///
/// Returns `None` for an empty input, any byte that is not a digit, or a value that does not fit in a
/// `u32` - a refusal, never a wrapped or truncated number.
pub fn parse_u32(digits: &[u8]) -> Option<u32> {
    if digits.is_empty() {
        return None;
    }
    let mut value: u32 = 0;
    for &b in digits {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u32::from(b - b'0'))?;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::parse_u32;

    #[test]
    fn parses_digits() {
        assert_eq!(parse_u32(b"0"), Some(0));
        assert_eq!(parse_u32(b"4096"), Some(4096));
        assert_eq!(parse_u32(b"4294967295"), Some(u32::MAX));
    }

    #[test]
    fn refuses_what_is_not_a_number() {
        assert_eq!(parse_u32(b""), None);
        assert_eq!(parse_u32(b"12a"), None);
        assert_eq!(parse_u32(b"-1"), None);
        assert_eq!(parse_u32(b"4294967296"), None); // one past u32::MAX
    }
}
