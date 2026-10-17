//! {{name}}: a Godspeed utility.
//!
//! A utility does one job and ends. The shell starts it for one command line, hands it its arguments,
//! shows (or pipes) what it writes, and reports the status it ends with: 0 for success, anything else
//! for a failure it has already said.

#![deny(unsafe_code)]
#![no_std]
#![no_main]

use core::fmt::Write as _;
use godspeed::{self as gs, ServiceContext};

// `#[no_mangle]` is what lets the loader find the entry point. Without it the program starts at
// address 0 and faults before its first line. The lint allowance is needed because `#[no_mangle]`
// itself counts as unsafe code; nothing else in this crate may be.
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn {{entry}}(ctx: ServiceContext) -> ! {
    let mut buf = [0u8; gs::program::ARGS_MAX];
    let args = gs::program::args(&ctx, &mut buf);
    let mut out = gs::program::Output::new(&ctx);
    let who = if args.is_empty() { "world" } else { args };
    let _ = writeln!(out, "hello, {}", who);
    out.finish(0)
}
