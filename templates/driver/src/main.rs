//! {{name}}: a Godspeed driver.
//!
//! A driver is a service that drives one device. The supervisor starts it with that device's register window
//! mapped and nothing more: no DMA arena and no interrupt, so this driver reads and writes registers and that is
//! all its device can be told to do. The device is QEMU's `edu` (`-device edu`): its first register holds an
//! identity, the next inverts what is written to it (a liveness check), and it computes factorials.

#![deny(unsafe_code)]
#![no_std]
#![no_main]

use godspeed::{self as gs, ServiceContext};

/// `edu`'s registers, as offsets into BAR 0.
const ID: usize = 0x00;
const LIVENESS: usize = 0x04;
const FACTORIAL: usize = 0x08;
const STATUS: usize = 0x20;
/// STATUS bit 0: a factorial is being computed.
const COMPUTING: u32 = 1;

// `#[no_mangle]` is what lets the loader find the entry point. Without it the driver starts at
// address 0 and faults before its first line. The lint allowance is needed because `#[no_mangle]`
// itself counts as unsafe code; nothing else in this crate may be.
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn {{entry}}(ctx: ServiceContext) -> ! {
    let Some(regs) = ctx.mmio() else {
        ctx.log("{{name}}: no device was granted - nothing to drive");
        gs::ipc::park(&ctx)
    };
    let id = regs.read32(ID);
    regs.write32(LIVENESS, 0x1234_5678);
    let alive = regs.read32(LIVENESS) == !0x1234_5678u32;
    regs.write32(FACTORIAL, 5);
    // A bounded wait: the device answers in microseconds, and one that never does is said, not waited on forever.
    let mut tries = 0u32;
    while regs.read32(STATUS) & COMPUTING != 0 && tries < 1000 {
        gs::task::sleep_ms(&ctx, 1);
        tries += 1;
    }
    let five = regs.read32(FACTORIAL);
    ctx.log_fmt(format_args!("{{name}}: device id {:#010x}, alive {}, 5! = {}", id, alive, five));
    gs::ipc::park(&ctx)
}
