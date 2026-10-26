// SPDX-License-Identifier: Apache-2.0 OR MIT
//! The `.gsp` file: a package that travels as one file and installs with no network.
//!
//! A `.gsp` is a slice of a registry in a file. It holds the very documents a fetch reads - the trust chain from
//! root 1, the package's index, the release's manifest, signature and contract, and its binaries - under the
//! very paths the registry serves them at, so a client checks one exactly as it checks a fetch: the file is
//! one more source of those paths, and nothing about what is checked differs (GodspeedOS
//! `docs/developer-services-design.md` 15).
//!
//! The layout is text lines framing raw bytes, readable a line at a time with no heap:
//!
//! ```text
//! godspeed-package-file 1
//! entry <size> <path>
//! <size bytes>
//! entry <size> <path>
//! <size bytes>
//! end
//! ```
//!
//! Every line ends with `\n`. A reader takes the header, then one entry line, skips `<size>` bytes, takes the
//! next, until `end`: one pass gives the table of entries, and an entry is then read at its offset. The paths
//! are the registry's, relative, from a closed alphabet, so a file cannot name a place outside the package.

/// The first line of every `.gsp`.
pub const HEADER: &str = "godspeed-package-file 1";
/// The most entries one `.gsp` may hold: the trust chain, the index, a release's three documents and a binary
/// for every architecture, with room for root rotations.
pub const ENTRIES_MAX: usize = 64;
/// The longest path an entry may name.
pub const PATH_MAX: usize = 200;
/// The longest line in a `.gsp`: `entry `, a size, a space, a path.
pub const LINE_MAX: usize = 6 + 20 + 1 + PATH_MAX;

/// Why a line of a `.gsp` is not what one may hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GspError {
    /// The first line is not [`HEADER`]: not a `.gsp`, or a version this reader does not know.
    NotAGsp,
    /// A line is neither `entry <size> <path>` nor `end`.
    BadLine,
    /// An entry's path is absolute, climbs out (`..`), is too long, or uses a character outside `a-z 0-9 . _ - /`.
    BadPath,
    /// More than [`ENTRIES_MAX`] entries.
    TooMany,
    /// The same path twice.
    Repeated,
}

impl GspError {
    /// What is wrong, in one line.
    pub const fn message(self) -> &'static str {
        match self {
            GspError::NotAGsp => {
                "it does not begin `godspeed-package-file 1` - not a Godspeed package file, or a newer kind"
            }
            GspError::BadLine => "a line is neither `entry <size> <path>` nor `end`",
            GspError::BadPath => "an entry names a path outside the package",
            GspError::TooMany => "it holds more entries than a package file may",
            GspError::Repeated => "it holds the same path twice",
        }
    }
}

impl core::fmt::Display for GspError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.message())
    }
}

/// One line of a `.gsp`, after the header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line<'a> {
    /// `entry <size> <path>`: the next `size` bytes are the document the registry serves at `path`.
    Entry {
        /// How many bytes follow the line.
        size: u64,
        /// The registry path those bytes are served at.
        path: &'a str,
    },
    /// `end`: there are no more entries.
    End,
}

/// Is `path` one a `.gsp` may hold? Relative, at most [`PATH_MAX`] bytes, no empty or `.`/`..` segment, and only
/// `a-z 0-9 . _ - /`.
pub fn path_ok(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= PATH_MAX
        && !path.starts_with('/')
        && path.split('/').all(|seg| !seg.is_empty() && seg != "." && seg != "..")
        && path
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-' | b'/'))
}

/// Check the header line (without its `\n`).
pub fn header(line: &str) -> Result<(), GspError> {
    if line == HEADER {
        Ok(())
    } else {
        Err(GspError::NotAGsp)
    }
}

/// Read one line after the header (without its `\n`).
pub fn line(text: &str) -> Result<Line<'_>, GspError> {
    if text == "end" {
        return Ok(Line::End);
    }
    let rest = text.strip_prefix("entry ").ok_or(GspError::BadLine)?;
    let (size, path) = rest.split_once(' ').ok_or(GspError::BadLine)?;
    if size.is_empty() || !size.bytes().all(|b| b.is_ascii_digit()) {
        return Err(GspError::BadLine);
    }
    let size: u64 = size.parse().map_err(|_| GspError::BadLine)?;
    if !path_ok(path) {
        return Err(GspError::BadPath);
    }
    Ok(Line::Entry { size, path })
}

/// Write a `.gsp` holding `entries` (path, bytes), in the order given.
#[cfg(feature = "std")]
pub fn write(out: &mut impl std::io::Write, entries: &[(&str, &[u8])]) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    if entries.len() > ENTRIES_MAX {
        return Err(Error::new(ErrorKind::InvalidInput, GspError::TooMany.message()));
    }
    for (i, (path, _)) in entries.iter().enumerate() {
        if !path_ok(path) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                std::format!("{}: {path}", GspError::BadPath.message()),
            ));
        }
        if entries[..i].iter().any(|(p, _)| p == path) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                std::format!("{}: {path}", GspError::Repeated.message()),
            ));
        }
    }
    writeln!(out, "{HEADER}")?;
    for (path, bytes) in entries {
        writeln!(out, "entry {} {path}", bytes.len())?;
        out.write_all(bytes)?;
        writeln!(out)?;
    }
    writeln!(out, "end")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_read_back() {
        assert_eq!(header(HEADER), Ok(()));
        assert_eq!(header("godspeed-package-file 2"), Err(GspError::NotAGsp));
        assert_eq!(line("end"), Ok(Line::End));
        assert_eq!(
            line("entry 12 trust/root/1.toml"),
            Ok(Line::Entry {
                size: 12,
                path: "trust/root/1.toml"
            })
        );
        assert_eq!(line("entry x trust/root/1.toml"), Err(GspError::BadLine));
        assert_eq!(line("entry 12"), Err(GspError::BadLine));
        assert_eq!(line("file 12 a"), Err(GspError::BadLine));
    }

    #[test]
    fn a_path_cannot_leave_the_package() {
        for bad in ["/etc/x", "a/../b", "../x", "a//b", "./a", "A/b", "a b", "a\\b", ""] {
            assert!(!path_ok(bad), "{bad:?}");
        }
        assert!(path_ok("packages/godspeed/hello/0.3.0/x86_64.elf"));
        assert!(!path_ok(&"a".repeat(PATH_MAX + 1)));
    }

    #[test]
    fn written_then_read() {
        let mut out = std::vec::Vec::new();
        write(&mut out, &[("trust/root/1.toml", b"root"), ("index/a/b.toml", b"")]).unwrap();
        let text = std::str::from_utf8(&out).unwrap();
        assert_eq!(
            text,
            "godspeed-package-file 1\nentry 4 trust/root/1.toml\nroot\nentry 0 index/a/b.toml\n\nend\n"
        );
        assert!(
            write(&mut std::vec::Vec::new(), &[("a", b"x"), ("a", b"y")]).is_err(),
            "a path twice is refused"
        );
        assert!(
            write(&mut std::vec::Vec::new(), &[("../a", b"x")]).is_err(),
            "a path outside is refused"
        );
    }
}
