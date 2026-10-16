//! {{name}}: a Godspeed service.
//!
//! A service starts at `service_main`, is handed its `ServiceContext` - the only authority it holds -
//! and never returns. This one logs that it started, then parks: it holds still and uses no CPU until
//! it is stopped.

#![deny(unsafe_code)]
#![no_std]
#![no_main]

use godspeed::{self as gs, ServiceContext};

// `#[no_mangle]` is what lets the loader find the entry point. Without it the service starts at
// address 0 and faults before its first line. The lint allowance is needed because `#[no_mangle]`
// itself counts as unsafe code; nothing else in this crate may be.
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn service_main(ctx: ServiceContext) -> ! {
    ctx.log("{{name}}: started");
    gs::ipc::park(&ctx)
}
