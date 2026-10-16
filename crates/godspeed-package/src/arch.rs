//! The architectures a package can carry an artifact for.

use core::fmt;
use core::str::FromStr;

/// An instruction set a Godspeed package can carry a binary for.
///
/// One logical package version carries one artifact per architecture it supports. The artifact's file
/// name inside a package is a function of its architecture ([`Arch::artifact_file`]), never a free
/// string from the manifest, so a manifest cannot name a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Arch {
    /// 64-bit x86.
    X86_64,
    /// 64-bit Arm.
    Aarch64,
    /// 32-bit Arm (ARMv7-A).
    Armv7,
    /// 64-bit RISC-V.
    Riscv64,
}

impl Arch {
    /// Every architecture, in canonical order (the order a manifest lists its artifacts in).
    pub const ALL: [Arch; 4] = [Arch::X86_64, Arch::Aarch64, Arch::Armv7, Arch::Riscv64];

    /// The name used in manifests and on the command line.
    pub const fn as_str(self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64",
            Arch::Aarch64 => "aarch64",
            Arch::Armv7 => "armv7",
            Arch::Riscv64 => "riscv64",
        }
    }

    /// The artifact's file name inside a package.
    pub const fn artifact_file(self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64.elf",
            Arch::Aarch64 => "aarch64.elf",
            Arch::Armv7 => "armv7.elf",
            Arch::Riscv64 => "riscv64.elf",
        }
    }
}

impl FromStr for Arch {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        Arch::ALL.into_iter().find(|a| a.as_str() == s).ok_or(())
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_files_are_distinct() {
        for a in Arch::ALL {
            assert_eq!(a.as_str().parse(), Ok(a));
            for b in Arch::ALL {
                if a != b {
                    assert_ne!(a.artifact_file(), b.artifact_file());
                }
            }
        }
        assert_eq!("mips".parse::<Arch>(), Err(()));
        assert!(
            Arch::ALL.windows(2).all(|w| w[0] < w[1]),
            "ALL is in canonical (Ord) order"
        );
    }
}
