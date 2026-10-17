# {{name}}

A Godspeed app, created by `godspeed new app {{name}}` against GodspeedOS {{tag}}.

## Files

- `src/main.rs` - the app. It starts at `{{entry}}`, reads its arguments (`gs::program::args`), takes the
  screen and keyboard (`gs::app::App::open`), and gives them back when it ends (`close`).
- `contracts/{{name}}.toml` - the authority the app **requests**. It grants nothing; the OS decides. The
  console is asked of the operator at `godspeed install`, because an app is given it by its kind.
- `godspeed.toml` - what kind of project this is, for the `godspeed` CLI.
- `rust-toolchain.toml` - the compiler GodspeedOS {{tag}} is built with.

## Build

    godspeed check
    godspeed build --all

## Run

Package and publish it, then on a Godspeed machine:

    godspeed fetch <publisher>/{{name}}
    godspeed install <publisher>/{{name}}
    {{name}} Ada                   # the screen says "hello, Ada" until a key is pressed

An app cannot start a pipe: it has the screen, not a stream.
