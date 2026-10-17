# {{name}}

A Godspeed utility, created by `godspeed new utility {{name}}` against GodspeedOS {{tag}}.

## Files

- `src/main.rs` - the utility. It starts at `{{entry}}`, reads its arguments, writes its output and ends with
  a status (`gs::program`).
- `contracts/{{name}}.toml` - the authority the utility **requests**. It grants nothing; the OS decides.
- `godspeed.toml` - what kind of project this is, for the `godspeed` CLI.
- `rust-toolchain.toml` - the compiler GodspeedOS {{tag}} is built with.

## Build

    godspeed check                 # validate the project and its contract
    godspeed build                 # build for x86_64
    godspeed build --all           # every architecture the OS supports

## Run

Package and publish it (`godspeed package`, `godspeed publish`), then on a Godspeed machine:

    godspeed fetch <publisher>/{{name}}
    godspeed install <publisher>/{{name}}
    {{name}} Ada                   # prints "hello, Ada"; `{{name}} Ada | count` pipes it

The shell says the status it ended with when it is not 0.
