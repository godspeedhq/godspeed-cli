//! {{name}}: a Godspeed driver.
//!
//! A driver is a service that drives one device. The supervisor starts it with what its contract asks for and the
//! operator granted: the device's register window, its interrupt, and a DMA arena - memory the device reads and
//! writes by itself. Everything here goes through `gs::driver`, the same library every driver in GodspeedOS uses.
//!
//! The device is QEMU's `edu` (`-device edu`). This driver uses all three grants on it:
//! - **registers**: its identity, and a liveness register that inverts what is written to it;
//! - **the interrupt**: it computes 5! and raises its interrupt when done, which this driver waits for;
//! - **DMA**: it copies a buffer from the arena into its own memory and back, by itself, and raises the interrupt
//!   after each copy. The buffer coming back intact is the proof the arena works.

#![deny(unsafe_code)]
#![no_std]
#![no_main]

use godspeed::driver::irq::{Irq, Woke};
use godspeed::driver::wait::Budget;
use godspeed::{self as gs, ServiceContext};

/// `edu`'s registers, as offsets into BAR 0.
const ID: usize = 0x00;
const LIVENESS: usize = 0x04;
const FACTORIAL: usize = 0x08;
const STATUS: usize = 0x20;
const IRQ_STATUS: usize = 0x24;
const IRQ_ACK: usize = 0x64;
const DMA_SRC: usize = 0x80;
const DMA_DST: usize = 0x88;
const DMA_COUNT: usize = 0x90;
const DMA_CMD: usize = 0x98;
/// STATUS bit 7: raise the interrupt when a factorial is done.
const RAISE_ON_FACTORIAL: u32 = 0x80;
/// DMA_CMD: start a copy; bit 1 says device-to-memory, else memory-to-device; bit 2 raises the interrupt after.
const DMA_START: u64 = 1;
const DMA_TO_MEMORY: u64 = 2;
const DMA_RAISE: u64 = 4;
/// Where `edu`'s own 4 KiB buffer is, in the addresses its DMA engine uses.
const DEVICE_BUFFER: u64 = 0x40000;
/// The bytes copied there and back.
const LEN: usize = 64;
/// `edu` reaches only addresses below 256 MiB unless QEMU is told otherwise (`-device edu,dma_mask=...`).
const EDU_DMA_LIMIT: u64 = 1 << 28;

// `#[no_mangle]` is what lets the loader find the entry point. Without it the driver starts at
// address 0 and faults before its first line. The lint allowance is needed because `#[no_mangle]`
// itself counts as unsafe code; nothing else in this crate may be.
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn {{entry}}(ctx: ServiceContext) -> ! {
    // The register window the supervisor granted: `None` when no device was, and then there is nothing to drive.
    let Some(regs) = gs::driver::registers(&ctx) else {
        ctx.log("{{name}}: no device was granted - nothing to drive");
        gs::ipc::park(&ctx)
    };
    let id = regs.read32(ID);
    regs.write32(LIVENESS, 0x1234_5678);
    let alive = regs.read32(LIVENESS) == !0x1234_5678u32;

    // THE INTERRUPT. The kernel chose the vector and programmed the device with it; this driver never names one.
    let irq = Irq::granted(&ctx);
    if !irq.routed() {
        ctx.log("{{name}}: no interrupt was routed to this driver - its contract asks for one (`hw_pci_irq`)");
        gs::ipc::park(&ctx)
    }
    regs.write32(STATUS, RAISE_ON_FACTORIAL);
    regs.write32(FACTORIAL, 5);
    // A bounded wait, by TIME: a device that never answers is said, not waited on forever.
    if !interrupted(&ctx, &irq, &regs) {
        ctx.log("{{name}}: the device did not raise its interrupt for 5! within a second");
        gs::ipc::park(&ctx)
    }
    let five = regs.read32(FACTORIAL);

    // THE ARENA, laid out by name: two parts, one the device reads from and one it writes back into.
    let Some(arena) = gs::driver::arena(&ctx) else {
        ctx.log("{{name}}: no DMA arena was granted - its contract asks for one (`hw_dma`)");
        gs::ipc::park(&ctx)
    };
    let mut layout = arena.layout();
    let (out, back) = match (layout.take("out", LEN, 64), layout.take("back", LEN, 64)) {
        (Ok(o), Ok(b)) => (o, b),
        (Err(e), _) | (_, Err(e)) => {
            ctx.log_fmt(format_args!("{{name}}: {}", e));
            gs::ipc::park(&ctx)
        }
    };
    if back.phys_at(LEN - 1) >= EDU_DMA_LIMIT {
        ctx.log_fmt(format_args!(
            "{{name}}: the arena is at {:#x}, above the 256 MiB `edu` reaches - run QEMU with `-device edu,dma_mask=0xffffffffffffffff`",
            out.phys()));
        gs::ipc::park(&ctx)
    }
    for i in 0..LEN { out.write8(i, i as u8 ^ 0x5a); }
    back.zero();
    // To the device, then back: each copy is the device's own, and each ends with its interrupt.
    let copied = copy(&ctx, &irq, &regs, out.phys(), DEVICE_BUFFER, 0)
        && copy(&ctx, &irq, &regs, DEVICE_BUFFER, back.phys(), DMA_TO_MEMORY);
    let intact = copied && (0..LEN).all(|i| back.read8(i) == i as u8 ^ 0x5a);
    ctx.log_fmt(format_args!(
        "{{name}}: device id {:#010x}, alive {}, 5! = {} by interrupt, {} bytes through DMA and back {}",
        id, alive, five, LEN, if intact { "intact" } else if copied { "CHANGED" } else { "NOT COPIED" }));
    gs::ipc::park(&ctx)
}

/// One DMA copy of `LEN` bytes, `src` to `dst`, ended by the device's interrupt.
fn copy(ctx: &ServiceContext, irq: &Irq, regs: &gs::driver::Registers, src: u64, dst: u64, direction: u64) -> bool {
    regs.write64(DMA_SRC, src);
    regs.write64(DMA_DST, dst);
    regs.write64(DMA_COUNT, LEN as u64);
    regs.write64(DMA_CMD, DMA_START | direction | DMA_RAISE);
    interrupted(ctx, irq, regs)
}

/// Wait up to a second for the device's interrupt, then clear its source and let it fire again.
fn interrupted(ctx: &ServiceContext, irq: &Irq, regs: &gs::driver::Registers) -> bool {
    loop {
        match irq.wait(ctx, Budget::ms(1000)) {
            Woke::Interrupt => {
                let why = regs.read32(IRQ_STATUS);
                regs.write32(IRQ_ACK, why);
                irq.rearm(ctx);
                return true;
            }
            // Nothing sends this driver requests; one that arrives is not what is being waited for.
            Woke::Request(_) => continue,
            Woke::Timeout => return false,
        }
    }
}
