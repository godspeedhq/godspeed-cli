//! shout: a request/reply server. Send it text; it answers with the same text in capitals.
//!
//! The shape of every Godspeed server: wait for a request, answer over the reply capability the request
//! carries, and give that capability back. It has no way to reach anyone else - not even the client,
//! except through the capability the client chose to hand it.

#![deny(unsafe_code)]
#![no_std]
#![no_main]

use godspeed::{self as gs, ipc::Message, ServiceContext};

#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn service_main(ctx: ServiceContext) -> ! {
    ctx.log("shout: ready");

    loop {
        // Block until a request arrives. With no client, the service simply waits here, idle.
        let mut request = gs::ipc::recv(&ctx);

        // A request carries a capability to reply with. One without is malformed: say so and move on.
        let Some(reply_to) = gs::ipc::take_sent_cap(&ctx) else {
            ctx.log("shout: a request came with no way to reply; dropped");
            continue;
        };

        // Capitals, in place: a message is 4 KiB, and there is no heap to spend on a copy.
        let len = request.payload_bytes().len();
        request.payload[..len].make_ascii_uppercase();
        let reply = Message::from_bytes(&request.payload[..len]);

        // Never block on a client: a slow or vanished one must not stall the server. A send that
        // succeeds means the reply was queued, not that it was read.
        if let Err(e) = gs::ipc::try_send_to(&ctx, reply_to, &reply) {
            ctx.log_fmt(format_args!(
                "shout: could not reply ({}); the client must ask again",
                e.as_str()
            ));
        }

        // Done with the reply capability: give its slot back, so a long-running server stays bounded.
        gs::cap::remove(&ctx, reply_to);
    }
}
