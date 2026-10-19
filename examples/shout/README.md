# shout

A request/reply server: send it text and it answers in capitals. The shape of every Godspeed server - receive, reply over the capability the request carries, give that capability back.

A Godspeed service, created by `godspeed new service shout` against GodspeedOS v0.29.0.

## Files

- `src/main.rs` - the service. It starts at `service_main` and never returns.
- `contracts/shout.toml` - the authority the service **requests**. It grants nothing; the OS decides.
- `godspeed.toml` - what kind of project this is, for the `godspeed` CLI.
- `rust-toolchain.toml` - the compiler GodspeedOS v0.29.0 is built with.

## Build

    godspeed check                 # validate the project and its contract
    godspeed build                 # build for x86_64
    godspeed build --arch riscv64  # or aarch64, armv7
    godspeed build --all           # every architecture the OS supports

## Run and test

Not yet. GodspeedOS can only run a service compiled into its own image, and it has no public way to
add one from outside its source tree. `godspeed run` and `godspeed test` say so, and say what is
missing, rather than editing the OS for you.
