// SPDX-License-Identifier: Apache-2.0 OR MIT
//! The package index: every published version of one package, as the registry serves it.
//!
//! The index is the registry's only mutable file per package, and the link from mutable to immutable: each
//! release names its manifest by digest, and the manifest names every other file by digest. Whoever can
//! trust an index entry can verify everything under it without trusting the storage or the network. (Who
//! can trust the index itself is the registry's trust model - `docs/publishing-design.md`, section 6 - and
//! is not this module's to decide.)

use crate::digest::Sha256Digest;
use crate::manifest::{Problem, ProblemKind};
use crate::name::PackageName;
use crate::version::Version;

/// The index format this crate reads and writes. Versioned separately from the manifest's.
pub const INDEX_FORMAT: u32 = 1;

/// One published version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Release<'a> {
    /// The version.
    pub version: Version,
    /// The SHA-256 of that version's `manifest.toml`.
    pub manifest: Sha256Digest,
    /// When the registry finalised it, as `YYYY-MM-DDTHH:MM:SSZ`.
    pub published: &'a str,
    /// The authenticated account that published it. Not necessarily the namespace's owner: release
    /// attribution and package ownership are separate.
    pub published_by: &'a str,
}

/// Every published version of one package, in the order they were published.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackageIndex<'a> {
    /// The index format; must be [`INDEX_FORMAT`].
    pub format: u32,
    /// The package.
    pub name: PackageName<'a>,
    /// Its releases, oldest first. Versions need not increase - a 1.0.1 may follow a 1.1.0 - but each
    /// appears once, and publication times never go backwards.
    pub releases: &'a [Release<'a>],
}

/// Is `s` exactly `YYYY-MM-DDTHH:MM:SSZ`, with plausible field ranges? Such strings sort as times.
pub fn is_timestamp(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 20 {
        return false;
    }
    let digits = |r: core::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    let num = |r: core::ops::Range<usize>| b[r].iter().fold(0u32, |n, d| n * 10 + u32::from(d - b'0'));
    let shape = digits(0..4)
        && b[4] == b'-'
        && digits(5..7)
        && b[7] == b'-'
        && digits(8..10)
        && b[10] == b'T'
        && digits(11..13)
        && b[13] == b':'
        && digits(14..16)
        && b[16] == b':'
        && digits(17..19)
        && b[19] == b'Z';
    shape
        && (1..=12).contains(&num(5..7))
        && (1..=31).contains(&num(8..10))
        && num(11..13) <= 23
        && num(14..16) <= 59
        && num(17..19) <= 60 // a leap second
}

impl PackageIndex<'_> {
    /// Check every rule, reporting each problem through `report`. Returns whether the index is valid.
    pub fn validate(&self, mut report: impl FnMut(Problem)) -> bool {
        let mut ok = true;
        let mut bad = |field, kind| {
            ok = false;
            report(Problem { field, kind });
        };
        if self.format != INDEX_FORMAT {
            bad(
                "format",
                ProblemKind::UnknownFormat {
                    found: self.format,
                    known: INDEX_FORMAT,
                },
            );
        }
        for (i, r) in self.releases.iter().enumerate() {
            if self.releases[..i].iter().any(|p| p.version == r.version) {
                bad("release.version", ProblemKind::DuplicateVersion(r.version));
            }
            if !is_timestamp(r.published) {
                bad("release.published", ProblemKind::BadTimestamp);
            } else if i > 0
                && is_timestamp(self.releases[i - 1].published)
                && self.releases[i - 1].published > r.published
            {
                bad("release.published", ProblemKind::TimestampOrder);
            }
            if !PackageName::is_valid_part(r.published_by) {
                bad(
                    "release.published_by",
                    ProblemKind::Malformed("an account name, like a publisher name"),
                );
            }
        }
        ok
    }

    /// The release of `version`, if published.
    pub fn release(&self, version: Version) -> Option<&Release<'_>> {
        self.releases.iter().find(|r| r.version == version)
    }

    /// The highest version published, which is not necessarily the most recent.
    pub fn latest(&self) -> Option<&Release<'_>> {
        self.releases.iter().max_by_key(|r| r.version)
    }
}

#[cfg(feature = "toml")]
pub use text::{index_to_toml, parse_index, OwnedPackageIndex};

#[cfg(feature = "toml")]
mod text {
    use super::*;
    use crate::text::ParseError;
    use serde::Deserialize;
    use std::fmt::Write as _;
    use std::string::{String, ToString};
    use std::vec::Vec;

    /// An index read from text, owning its strings.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct OwnedPackageIndex {
        name: String,
        versions: Vec<Version>,
        manifests: Vec<Sha256Digest>,
        published: Vec<String>,
        published_by: Vec<String>,
    }

    impl OwnedPackageIndex {
        /// A new, empty index for `name`, which must be a valid package name.
        pub fn empty(name: &PackageName<'_>) -> Self {
            OwnedPackageIndex {
                name: name.to_string(),
                versions: Vec::new(),
                manifests: Vec::new(),
                published: Vec::new(),
                published_by: Vec::new(),
            }
        }

        /// Append a release. The caller validates the result.
        pub fn push(&mut self, version: Version, manifest: Sha256Digest, published: &str, published_by: &str) {
            self.versions.push(version);
            self.manifests.push(manifest);
            self.published.push(published.to_string());
            self.published_by.push(published_by.to_string());
        }

        /// The releases, borrowed; pass the result to [`OwnedPackageIndex::index`].
        pub fn releases(&self) -> Vec<Release<'_>> {
            (0..self.versions.len())
                .map(|i| Release {
                    version: self.versions[i],
                    manifest: self.manifests[i],
                    published: &self.published[i],
                    published_by: &self.published_by[i],
                })
                .collect()
        }

        /// The index, borrowed, over `releases` from [`OwnedPackageIndex::releases`].
        pub fn index<'a>(&'a self, releases: &'a [Release<'a>]) -> PackageIndex<'a> {
            PackageIndex {
                format: INDEX_FORMAT,
                name: PackageName::parse(&self.name).expect("validated when created or read"),
                releases,
            }
        }
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Raw {
        format: Option<u32>,
        name: Option<String>,
        #[serde(default)]
        release: Vec<RawRelease>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RawRelease {
        version: Option<String>,
        manifest: Option<String>,
        published: Option<String>,
        published_by: Option<String>,
    }

    /// Read an index from its text form, strictly: unknown keys and every broken rule are errors.
    pub fn parse_index(text: &str) -> Result<OwnedPackageIndex, ParseError> {
        let raw: Raw = toml::from_str(text).map_err(|e| ParseError::Syntax(e.message().to_string()))?;
        let mut problems = Vec::new();
        let mut bad = |field: &'static str, kind| problems.push(Problem { field, kind });
        match raw.format {
            None => bad("format", ProblemKind::Missing),
            Some(n) if n != INDEX_FORMAT => {
                bad(
                    "format",
                    ProblemKind::UnknownFormat {
                        found: n,
                        known: INDEX_FORMAT,
                    },
                );
                return Err(ParseError::Invalid(problems));
            }
            Some(_) => {}
        }
        let name = match raw.name.as_deref().map(PackageName::parse) {
            None => {
                bad("name", ProblemKind::Missing);
                None
            }
            Some(Err(e)) => {
                bad("name", ProblemKind::Malformed(e.message()));
                None
            }
            Some(Ok(n)) => Some(n),
        };
        let mut owned = name.map(|n| OwnedPackageIndex::empty(&n));
        for r in &raw.release {
            let version = match r.version.as_deref().map(str::parse::<Version>) {
                None => {
                    bad("release.version", ProblemKind::Missing);
                    None
                }
                Some(Err(e)) => {
                    bad("release.version", ProblemKind::Malformed(e.message()));
                    None
                }
                Some(Ok(v)) => Some(v),
            };
            let manifest = match r.manifest.as_deref().map(str::parse::<Sha256Digest>) {
                None => {
                    bad("release.manifest", ProblemKind::Missing);
                    None
                }
                Some(Err(e)) => {
                    bad("release.manifest", ProblemKind::Malformed(e.message()));
                    None
                }
                Some(Ok(d)) => Some(d),
            };
            if r.published.is_none() {
                bad("release.published", ProblemKind::Missing);
            }
            if r.published_by.is_none() {
                bad("release.published_by", ProblemKind::Missing);
            }
            if let (Some(o), Some(v), Some(m), Some(p), Some(b)) =
                (owned.as_mut(), version, manifest, &r.published, &r.published_by)
            {
                o.push(v, m, p, b);
            }
        }
        let Some(owned) = owned else {
            return Err(ParseError::Invalid(problems));
        };
        let releases = owned.releases();
        owned.index(&releases).validate(|p| problems.push(p));
        drop(releases);
        if problems.is_empty() {
            Ok(owned)
        } else {
            Err(ParseError::Invalid(problems))
        }
    }

    /// Write an index in its one canonical text form.
    pub fn index_to_toml(index: &PackageIndex<'_>) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "format = {}", index.format);
        let _ = writeln!(s, "name = \"{}\"", index.name);
        for r in index.releases {
            let _ = writeln!(
                s,
                "\n[[release]]\nversion = \"{}\"\nmanifest = \"{}\"\npublished = \"{}\"\npublished_by = \"{}\"",
                r.version, r.manifest, r.published, r.published_by
            );
        }
        s
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn sample() -> OwnedPackageIndex {
            let mut o = OwnedPackageIndex::empty(&PackageName::parse("alice/hello").unwrap());
            o.push(
                Version::new(1, 0, 0),
                Sha256Digest::of(b"m1"),
                "2026-11-02T10:14:00Z",
                "alice",
            );
            o.push(
                Version::new(1, 1, 0),
                Sha256Digest::of(b"m2"),
                "2026-11-09T08:00:00Z",
                "bob",
            );
            o.push(
                Version::new(1, 0, 1),
                Sha256Digest::of(b"m3"),
                "2026-11-10T08:00:00Z",
                "alice",
            );
            o
        }

        #[test]
        fn round_trips_and_answers_lookups() {
            let o = sample();
            let rs = o.releases();
            let idx = o.index(&rs);
            assert!(idx.validate(|p| panic!("{p}")));
            let text = index_to_toml(&idx);
            let back = parse_index(&text).unwrap();
            assert_eq!(back, o);
            assert_eq!(
                idx.latest().unwrap().version,
                Version::new(1, 1, 0),
                "highest, not newest"
            );
            assert_eq!(idx.release(Version::new(1, 0, 1)).unwrap().published_by, "alice");
            assert!(idx.release(Version::new(2, 0, 0)).is_none());
        }

        #[test]
        fn refuses_duplicates_bad_times_and_unknown_keys() {
            let text = index_to_toml(&sample().index(&sample().releases()));
            let dup = text.replace("version = \"1.0.1\"", "version = \"1.0.0\"");
            let Err(ParseError::Invalid(ps)) = parse_index(&dup) else {
                panic!()
            };
            assert_eq!(ps[0].kind, ProblemKind::DuplicateVersion(Version::new(1, 0, 0)));

            let back_in_time = text.replace("2026-11-10T08:00:00Z", "2026-11-01T08:00:00Z");
            let Err(ParseError::Invalid(ps)) = parse_index(&back_in_time) else {
                panic!()
            };
            assert_eq!(ps[0].kind, ProblemKind::TimestampOrder);

            let not_utc = text.replace("2026-11-02T10:14:00Z", "2026-11-02 10:14:00");
            let Err(ParseError::Invalid(ps)) = parse_index(&not_utc) else {
                panic!()
            };
            assert_eq!(ps[0].kind, ProblemKind::BadTimestamp);

            let extra = text.replace("[[release]]\n", "[[release]]\nyanked = true\n");
            assert!(matches!(parse_index(&extra), Err(ParseError::Syntax(_))));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_timestamp;

    #[test]
    fn timestamps() {
        assert!(is_timestamp("2026-10-15T13:00:31Z"));
        assert!(is_timestamp("2016-12-31T23:59:60Z"));
        for bad in [
            "2026-10-15T13:00:31",
            "2026-13-15T13:00:31Z",
            "2026-10-15T24:00:00Z",
            "2026-1-15T13:00:31Z",
            "2026-10-15t13:00:31z",
        ] {
            assert!(!is_timestamp(bad), "{bad}");
        }
    }
}
