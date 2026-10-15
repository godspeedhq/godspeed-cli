//! The architectures a Godspeed project can be built for, as facts.
//!
//! This is the ONE place the CLI names an instruction set. Everything else asks this table for a fact
//! (the Rust target triple, the QEMU binary that emulates it) instead of branching on the ISA's name.
//! Whether the OS actually supports an architecture is not decided here: `build` checks the triple
//! against the targets the OS checkout's `rust-toolchain.toml` pins, so an OS that drops a port drops it
//! for the CLI too, without a CLI release.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arch {
    /// The name a developer types: `--arch riscv64`.
    pub name: &'static str,
    /// The Rust target the OS builds services for on this architecture.
    pub triple: &'static str,
    /// The QEMU system emulator for this architecture, for `doctor`.
    pub qemu: &'static str,
    /// The environment variable the OS's own run scripts read to find that emulator, if any.
    pub qemu_env: Option<&'static str>,
}

pub const ARCHES: &[Arch] = &[
    Arch {
        name: "x86_64",
        triple: "x86_64-unknown-none",
        qemu: "qemu-system-x86_64",
        qemu_env: None,
    },
    Arch {
        name: "aarch64",
        triple: "aarch64-unknown-none",
        qemu: "qemu-system-aarch64",
        qemu_env: Some("QEMU_AARCH64"),
    },
    Arch {
        name: "armv7",
        triple: "armv7a-none-eabi",
        qemu: "qemu-system-arm",
        qemu_env: Some("QEMU_ARM"),
    },
    Arch {
        name: "riscv64",
        triple: "riscv64imac-unknown-none-elf",
        qemu: "qemu-system-riscv64",
        qemu_env: Some("QEMU_RISCV64"),
    },
];

pub fn by_name(name: &str) -> Option<&'static Arch> {
    ARCHES.iter().find(|a| a.name == name)
}

pub fn names() -> String {
    ARCHES.iter().map(|a| a.name).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_triples_are_unique() {
        for (i, a) in ARCHES.iter().enumerate() {
            for b in &ARCHES[i + 1..] {
                assert_ne!(a.name, b.name);
                assert_ne!(a.triple, b.triple);
            }
        }
    }

    #[test]
    fn lookup() {
        assert_eq!(by_name("riscv64").unwrap().triple, "riscv64imac-unknown-none-elf");
        assert!(by_name("mips").is_none());
    }
}
