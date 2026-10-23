# 🚀 godspeed 0.4.0 - drivers on gs::driver, packages that need packages, and every kind proven

The developer CLI for [GodspeedOS](https://github.com/godspeedhq/godspeed-os), released alongside
**GodspeedOS v0.30.0**. That OS release finishes `gs::driver`: every driver in the OS is written on it, from its
register window to its interrupts. This CLI follows straight away: **the driver you make is written the same
way**, a package can **require another package**, and **every kind of program is now built in CI** and
installed and run on a real GodspeedOS.

```sh
godspeed new driver sensor        # 🔌 now on gs::driver::registers and gs::driver::wait
godspeed new utility greet        # 🔧 greet Ada  ->  hello, Ada
godspeed new app notes            # 🖼️ the screen is yours until a key
godspeed new service clock        # ⚙️ kept running by the system
godspeed new library parser       # 📚 shared, tested on your workstation
```

📖 **New guide, one page for every kind:** https://godspeedhq.github.io/godspeed-cli/kinds.html

> **Updating from 0.3.0:** `godspeed self update`.

## ✨ What's new

### 🔌 The driver template is written on `gs::driver`

The OS's own drivers moved onto `gs::driver` this release - waits, holds, interrupts, register windows, DMA
arenas, input, power - and the template you start from now speaks the same language:

- 🪟 **`gs::driver::registers(&ctx)`** takes the device's register window, instead of the SDK's raw `ctx.mmio()`.
- ⏱️ **`gs::driver::wait::until(&ctx, Budget::ms(10), ..)`** waits on the device **by time**, measured on the
  machine's own clock, instead of counting a thousand 1 ms sleeps. A device that never answers is **said**, not
  waited on forever.
- 🧪 Installed on QEMU with its `edu` device, it logs `device id 0x010000ed, alive true, 5! = 120`.
- ⚠️ It needs **GodspeedOS v0.30.0** or later, the first release with `gs::driver::registers`.

### 🧩 A package can require another

- 📋 Name what your program needs in `godspeed.toml`, and a Godspeed machine installs and starts it **first**:

  ```toml
  [requires]
  "alice/notes-store" = "^1.2.0"
  ```

- 🎯 Ranges are written with the whole version: `^1.2.0`, `=1.2.3` or `>=1.2.0`. One spelling each, so `^0` and
  `^0.0.0` can never be confused.
- 📦 A package with requirements is manifest **format 2**; one without stays **format 1** and installs on
  GodspeedOS 0.29.0 exactly as before.
- 🛑 **`godspeed publish` refuses a requirement the registry cannot supply**, before a single byte is uploaded.
- 🔐 A requirement grants nothing: the contract still names every peer, and the operator still approves each.

### ✅ Every kind is built in CI, and run on the OS

- 🏗️ The end-to-end test used to build only the service template. It now makes, checks and builds **service,
  utility, app and driver** for **all four architectures** and checks every ELF: the machine it claims, a real
  entry point.
- 🖥️ GodspeedOS's new `osdev test kinds` installs one package of each kind, made by this CLI's templates
  **untouched**, and sees each do its job: the service starts and stays up, the utility answers and pipes, the app
  holds the screen and gives it back, and the driver drives its device. **19 of 19.**

### 📖 A guide that matches what you can build

- 🗺️ **What you can make** (`kinds.html`): a page for each kind, from `godspeed new` to `godspeed install` on the
  machine - what it is given, what its template does, and which kind to pick.
- 🔄 The overview, examples and reference now say what is true: **every kind works**, `install` works on the
  machine, `package --local`, `fetch --offline` and `inspect` are in the reference, and what still waits is named.
- 💬 `godspeed run` says what it would add - booting a machine with your program already on it - and how to run
  it today: `package --local`, then `godspeed install` on the machine.

## 🧪 Tested

- The whole test suite, `cargo fmt` and `clippy -D warnings`.
- Every program kind generated, checked and built for all four architectures, and the library's tests run,
  against a checkout of GodspeedOS (the code that becomes v0.30.0).
- The four templates packaged and installed on GodspeedOS in QEMU, each seen working (`osdev test kinds`, 19/0).
- Every example checked and built for all four architectures.

## 🚧 Still waiting on GodspeedOS

- 🔌 A driver package is given its device's **registers only**: no DMA arena and no interrupt yet.
- ▶️ `godspeed run` and `godspeed test` for a program: there is no public way yet to boot a machine with your
  program on it, so the CLI says so instead of pretending.

## 💻 Builds

🐧 Linux x86_64 and Arm64 (static, any distribution) · 🍎 macOS Apple silicon and Intel · 🪟 Windows x64 and Arm64.
Built against GodspeedOS **v0.30.0**.
