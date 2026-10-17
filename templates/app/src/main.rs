//! {{name}}: a Godspeed app.
//!
//! An app is a utility that has the screen and the keyboard while it runs. The shell starts it for one
//! command line and hands it its arguments; it takes the console (`gs::app::App::open`), draws, reads
//! keys, and gives the console back when it ends (`close`). If it dies instead, the console comes back
//! all the same: the kernel releases a dead holder's. The operator approves the console at install.

#![deny(unsafe_code)]
#![no_std]
#![no_main]

use godspeed::{self as gs, ServiceContext};

// `#[no_mangle]` is what lets the loader find the entry point. Without it the program starts at
// address 0 and faults before its first line. The lint allowance is needed because `#[no_mangle]`
// itself counts as unsafe code; nothing else in this crate may be.
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn {{entry}}(ctx: ServiceContext) -> ! {
    let mut buf = [0u8; gs::program::ARGS_MAX];
    let args = gs::program::args(&ctx, &mut buf);
    let who = if args.is_empty() { "world" } else { args };
    let app = gs::app::App::open(&ctx);
    app.print_fmt(format_args!("hello, {}\n\n", who));
    app.println("This screen is {{name}}'s until a key is pressed.");
    app.print("Press any key to give it back to the shell.");
    let key = app.wait_key();
    let _ = key;
    app.close(0)
}
