//! Lowercase hexadecimal, both ways, without a heap. One spelling per value: uppercase is refused.

use core::fmt;

fn nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

/// Decode exactly `out.len() * 2` lowercase hex digits into `out`. False if the length or a digit is wrong.
pub fn decode(hex: &[u8], out: &mut [u8]) -> bool {
    if hex.len() != out.len() * 2 {
        return false;
    }
    for (i, pair) in hex.chunks_exact(2).enumerate() {
        match (nibble(pair[0]), nibble(pair[1])) {
            (Some(hi), Some(lo)) => out[i] = hi << 4 | lo,
            _ => return false,
        }
    }
    true
}

/// Write `bytes` as lowercase hex.
pub fn encode(bytes: &[u8], f: &mut impl fmt::Write) -> fmt::Result {
    for b in bytes {
        write!(f, "{b:02x}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_refusals() {
        let mut out = [0u8; 3];
        assert!(decode(b"00ff7a", &mut out));
        assert_eq!(out, [0x00, 0xff, 0x7a]);
        let mut s = std::string::String::new();
        encode(&out, &mut s).unwrap();
        assert_eq!(s, "00ff7a");
        assert!(!decode(b"00FF7A", &mut out), "uppercase");
        assert!(!decode(b"00ff7", &mut out), "short");
        assert!(!decode(b"00ff7g", &mut out), "not hex");
    }
}
