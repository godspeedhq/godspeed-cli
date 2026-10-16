//! bootcount: counts how many times this machine has started, in a file.
//!
//! Reads the count, adds one, writes it back, and logs it - using the filesystem through the standard
//! library, and the `fixedbuf` library from this repository to turn the number into bytes without a heap.
//! The part worth reading is the error handling: a write that may or may not have happened is reported
//! as exactly that, never retried blindly.

#![deny(unsafe_code)]
#![no_std]
#![no_main]

use core::fmt::Write;
use fixedbuf::{parse_u64, FixedBuf};
use godspeed::{self as gs, Error, ServiceContext};

const PATH: &str = "/bootcount.txt";

#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn service_main(ctx: ServiceContext) -> ! {
    // Grants nothing by itself: it reaches the filesystem only because the contract asks for
    // `ipc_send = ["fs"]` and the OS granted it. Without that, every call below fails `Unreachable`.
    let mut disk = gs::fs::Fs::new(&ctx);

    let mut stored = [0u8; 32];
    let previous = match disk.read_into(PATH, &mut stored) {
        Ok(n) => match parse_u64(&stored[..n]) {
            Some(count) => count,
            None => {
                // Never overwrite something we do not understand.
                ctx.log_fmt(format_args!(
                    "bootcount: {} does not hold a count; leaving it alone",
                    PATH
                ));
                gs::ipc::park(&ctx);
            }
        },
        Err(Error::NotFound) => 0, // the first boot
        Err(e) => {
            gs::io::report(&ctx, "bootcount: read", e);
            gs::ipc::park(&ctx);
        }
    };

    let count = previous.saturating_add(1);
    let mut text = FixedBuf::<24>::new();
    // A u64 and a newline always fit in 24 bytes, so this cannot fail.
    let _ = writeln!(text, "{}", count);

    match disk.write(PATH, text.as_bytes()) {
        Ok(()) => ctx.log_fmt(format_args!("bootcount: this is boot {}", count)),
        // No answer, or the filesystem died holding the request: the count may or may not be saved.
        // Saying so is the honest answer; writing again could count this boot twice.
        Err(e @ (Error::OutcomeUnknown | Error::PeerDied)) => ctx.log_fmt(format_args!(
            "bootcount: boot {}, but whether it was saved is unknown ({})",
            count,
            e.as_str()
        )),
        Err(e) => gs::io::report(&ctx, "bootcount: write", e),
    }

    gs::ipc::park(&ctx)
}
