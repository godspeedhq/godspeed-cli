//! fixedbuf: text built into a fixed-size buffer, for programs with no heap.
//!
//! A Godspeed service has no allocator, so there is no `String` and no `format!`. Logging needs neither
//! (`ctx.log_fmt(format_args!(...))` formats straight to the log), but an IPC message or a file needs the
//! bytes themselves. `FixedBuf` is a `core::fmt::Write` over an array: `write!` into it, then hand
//! `as_bytes()` to whatever sends or stores them. Text that does not fit is refused - `write!` returns an
//! error and `is_truncated()` says so - never wrapped around or silently lost.
//!
//! Plain `no_std` Rust with no dependencies, so it builds for every Godspeed architecture and its tests run
//! on your workstation with `godspeed test`.

#![cfg_attr(not(test), no_std)]
#![deny(unsafe_code)]

use core::fmt;

/// Up to `N` bytes of UTF-8 text, built in place.
pub struct FixedBuf<const N: usize> {
    bytes: [u8; N],
    len: usize,
    truncated: bool,
}

impl<const N: usize> FixedBuf<N> {
    /// An empty buffer.
    pub const fn new() -> Self {
        FixedBuf {
            bytes: [0; N],
            len: 0,
            truncated: false,
        }
    }

    /// What has been written so far.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    /// The same, as text. Only whole characters are ever kept, so this always succeeds.
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(self.as_bytes()).unwrap_or("")
    }

    /// Whether some text was refused because it did not fit.
    pub fn is_truncated(&self) -> bool {
        self.truncated
    }

    /// Empty it, to build the next piece of text in the same space.
    pub fn clear(&mut self) {
        self.len = 0;
        self.truncated = false;
    }
}

impl<const N: usize> Default for FixedBuf<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> fmt::Write for FixedBuf<N> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let room = N - self.len;
        if s.len() <= room {
            self.bytes[self.len..self.len + s.len()].copy_from_slice(s.as_bytes());
            self.len += s.len();
            return Ok(());
        }
        // Keep what fits, cut at a character boundary so the text stays valid UTF-8, and say so.
        let mut cut = room;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        self.bytes[self.len..self.len + cut].copy_from_slice(&s.as_bytes()[..cut]);
        self.len += cut;
        self.truncated = true;
        Err(fmt::Error)
    }
}

/// Parse an unsigned decimal number, allowing surrounding ASCII whitespace (a trailing newline, say).
///
/// Returns `None` for no digits, any other byte, or a value that does not fit in a `u64` - a refusal,
/// never a wrapped or truncated number.
pub fn parse_u64(text: &[u8]) -> Option<u64> {
    let text = text.trim_ascii();
    if text.is_empty() {
        return None;
    }
    let mut value: u64 = 0;
    for &b in text {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u64::from(b - b'0'))?;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::{parse_u64, FixedBuf};
    use core::fmt::Write;

    #[test]
    fn builds_text_in_place() {
        let mut buf = FixedBuf::<32>::new();
        write!(buf, "boot {} of {}", 3, "this machine").unwrap();
        assert_eq!(buf.as_str(), "boot 3 of this machine");
        assert!(!buf.is_truncated());
        buf.clear();
        write!(buf, "{}", 42).unwrap();
        assert_eq!(buf.as_bytes(), b"42");
    }

    #[test]
    fn refuses_what_does_not_fit_and_says_so() {
        let mut buf = FixedBuf::<8>::new();
        assert!(write!(buf, "{}", "123456789").is_err());
        assert_eq!(buf.as_str(), "12345678");
        assert!(buf.is_truncated());
    }

    #[test]
    fn never_splits_a_character() {
        let mut buf = FixedBuf::<5>::new();
        assert!(write!(buf, "abcé!").is_err()); // 'é' is two bytes: only "abc" + 'é' would be 5
        assert_eq!(buf.as_str(), "abcé");
        let mut buf = FixedBuf::<4>::new();
        assert!(write!(buf, "abcé").is_err());
        assert_eq!(buf.as_str(), "abc");
    }

    #[test]
    fn parses_numbers_and_refuses_the_rest() {
        assert_eq!(parse_u64(b"7\n"), Some(7));
        assert_eq!(parse_u64(b"  18446744073709551615 "), Some(u64::MAX));
        assert_eq!(parse_u64(b""), None);
        assert_eq!(parse_u64(b"\n"), None);
        assert_eq!(parse_u64(b"12a"), None);
        assert_eq!(parse_u64(b"18446744073709551616"), None); // one past u64::MAX
    }
}
