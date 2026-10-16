//! heartbeat: logs how long the machine has been up, every five seconds.
//!
//! The smallest service that does something over time: a loop that sleeps - giving the CPU back, not
//! spinning - and a formatted log line built with `format_args!`, which needs no heap.

#![deny(unsafe_code)]
#![no_std]
#![no_main]

use godspeed::{self as gs, ServiceContext};

/// How often to log.
const EVERY_MS: u64 = 5_000;

// `#[no_mangle]` is what lets the loader find the entry point; it counts as unsafe code, so it alone is
// allowed.
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn service_main(ctx: ServiceContext) -> ! {
    ctx.log("heartbeat: started");
    let mut beats: u64 = 0;
    loop {
        gs::task::sleep_ms(&ctx, EVERY_MS);
        beats = beats.wrapping_add(1);
        let up = gs::task::uptime_secs(&ctx);
        ctx.log_fmt(format_args!(
            "heartbeat: beat {}, up {}m {:02}s",
            beats,
            up / 60,
            up % 60
        ));
    }
}
