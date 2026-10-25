# {{name}}

A Godspeed service, created by `godspeed new service {{name}}` against GodspeedOS {{tag}}.

## Files

- `src/main.rs` - the service. It starts at `{{entry}}` and never returns.
- `contracts/{{name}}.toml` - the authority the service **requests**. It grants nothing; the OS decides.
- `godspeed.toml` - what kind of project this is, for the `godspeed` CLI.
- `rust-toolchain.toml` - the compiler GodspeedOS {{tag}} is built with.

## Build

    godspeed check                 # validate the project and its contract
    godspeed build                 # build for x86_64
    godspeed build --arch riscv64  # or aarch64, armv7
    godspeed build --all           # every architecture the OS supports

## Run and test

Package and publish it (`godspeed package`, `godspeed publish`), then on a Godspeed machine (GodspeedOS 0.29.0
or later):

    godspeed fetch <publisher>/{{name}}
    godspeed install <publisher>/{{name}}

The supervisor starts it, and starts it again at every boot. `godspeed run` and `godspeed test` refuse a service,
and say why: nothing yet boots a machine with your program already on it.
