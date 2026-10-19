# fixedbuf

Text built into a fixed-size buffer, for programs with no heap: a `core::fmt::Write` over an array that refuses, and says so, when text does not fit. Its tests run on your workstation.

A Godspeed library, created by `godspeed new library fixedbuf` against GodspeedOS v0.29.0.

    godspeed check                 # validate the project
    godspeed test                  # run the tests on this machine
    godspeed build --all           # build for every architecture the OS supports

A library is not installable or runnable on its own; other Godspeed projects depend on it.
