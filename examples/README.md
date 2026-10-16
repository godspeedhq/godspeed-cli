# Examples

Real Godspeed projects, each started with `godspeed new` and written only against the public standard
library. CI builds every one for all four architectures against each GodspeedOS release the CLI supports,
and runs the library's tests, so nothing here is code that does not compile. They are explained on the
[examples page](https://godspeedhq.github.io/godspeed-cli/examples.html).

| example | kind | shows |
|---|---|---|
| [heartbeat](heartbeat/) | service | a main loop that sleeps; formatted log lines with no heap |
| [shout](shout/) | service | a request/reply server: receive, reply over the request's capability, give it back |
| [bootcount](bootcount/) | service | files through `gs::fs`; depending on a library; a write whose outcome may be unknown |
| [fixedbuf](fixedbuf/) | library | text in a fixed-size buffer, refusing what does not fit; tests run on your workstation |

Try one:

    export GODSPEED_OS=/path/to/godspeed-os   # a GodspeedOS release checkout
    cd examples/heartbeat
    godspeed check
    godspeed build --all

None of the services can be run yet, from here or anywhere outside the OS tree: GodspeedOS starts only
the services compiled into its image. They build, package and publish today; running them waits on the OS.
