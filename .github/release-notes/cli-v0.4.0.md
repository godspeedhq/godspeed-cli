# 🚀 godspeed 0.4.0 - drivers on gs::driver, packages that need packages, and every kind proven

The developer CLI for [GodspeedOS](https://github.com/godspeedhq/godspeed-os), released alongside
**GodspeedOS v0.30.0**. That OS release finishes `gs::driver`: every driver in the OS is written on it, from its
register window to its interrupts. This CLI follows straight away: **the driver you make is written the same
way**, a package can **require another package**, and **every kind of program is now built in CI** and
installed and run on a real GodspeedOS.

```sh
godspeed new driver sensor        # 🔌 now on gs::driver::registers and gs::driver::wait
godspeed new utility greet        # 🔧 greet Bankole  ->  hello, Bankole
godspeed new app notes            # 🖼️ the screen is yours until a key
godspeed new service clock        # ⚙️ kept running by the system
godspeed new library parser       # 📚 shared, tested on your workstation
```

📖 **New guide, one section for every kind:**

| Kind | Make it | 📖 Step-by-step guide | 🧰 API reference |
|---|---|---|---|
| 🔁 Service | `godspeed new service clock` | [Make a service](https://godspeedhq.github.io/godspeed-cli/kinds.html#service) | [`gs::ipc`](https://godspeedhq.github.io/godspeed-os/api/godspeed/ipc/index.html) |
| ⚡ Utility | `godspeed new utility greet` | [Make a utility](https://godspeedhq.github.io/godspeed-cli/kinds.html#utility) | [`gs::program`](https://godspeedhq.github.io/godspeed-os/api/godspeed/program/index.html) |
| 🖼️ App | `godspeed new app notes` | [Make an app](https://godspeedhq.github.io/godspeed-cli/kinds.html#app) | [`gs::app`](https://godspeedhq.github.io/godspeed-os/api/godspeed/app/index.html) |
| 🔌 Driver | `godspeed new driver sensor` | [Make a driver](https://godspeedhq.github.io/godspeed-cli/kinds.html#driver) | [`gs::driver`](https://godspeedhq.github.io/godspeed-os/api/godspeed/driver/index.html) |
| 📚 Library | `godspeed new library parser` | [Make a library](https://godspeedhq.github.io/godspeed-cli/kinds.html#library) | [`gs`](https://godspeedhq.github.io/godspeed-os/api/godspeed/index.html) |

🗺️ Not sure which? [Which kind?](https://godspeedhq.github.io/godspeed-cli/kinds.html#choose) · ✍️ The whole story: [Writing a program for GodspeedOS](https://godspeedhq.github.io/godspeed-os/stdlib.html)

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
  `^0.0.0` can never be confused. Before 1.0 a caret holds the minor: `^0.3.1` takes `0.3.5` but never `0.4.0`,
  and `^0.0.3` is exactly `0.0.3`.
- 📦 A package with requirements is manifest **format 2**; one without stays **format 1** and installs on
  GodspeedOS 0.29.0 exactly as before. An older reader refuses format 2 by name, as "a format this reader does not
  know", rather than misreading it.
- 🌐 **The registry reads format 2**: it is rebuilt on this release's package crate, so a package with
  requirements can be published. Its contract names the package it sends to (`godspeed/dep-store`), which the
  contract schema allows from **GodspeedOS v0.30.0**.
- 🛑 **`godspeed publish` refuses a requirement the registry cannot supply**, before a single byte is uploaded,
  and names the newest release there is. `--dry-run` gives the same answer, and an index that cannot be read stops
  the publish rather than letting it through unchecked.
- 🔍 **`godspeed check`** refuses a contract peer that is a package (`publisher/name`) but is not in
  `[requires]`, and says the line to add. A built-in such as `fs` is never required, a library requires nothing,
  and a package has at most 8 requirements, none twice and never itself.
- 🧾 **`godspeed fetch` and `godspeed inspect`** say what a release requires, in text and in `--json`.
- 🔐 A requirement grants nothing: the contract still names every peer, and the operator still approves each.

On the machine (GodspeedOS v0.30.0), the whole plan is **one question**, dependencies first:

```text
gsh> godspeed install alice/notes
alice/notes 1.0.0 needs 1 more package:
  alice/notes-store            1.2.4     a service  asks for: fs   (needed by alice/notes)
  alice/notes                  1.0.0     an app     asks for: console, alice/notes-store
Install all 2 and grant them? (no privilege) [y/N]:
```

- ▶️ Each dependency is started before the package that needs it, at install and at every boot.
- 🧱 `uninstall` refuses a package another one requires, and `upgrade` refuses a version that would break a
  dependent's range, each naming the package in the way.
- 🧪 Seen on GodspeedOS in QEMU: `osdev test deps`, **19 of 19**.

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
- Requirements on GodspeedOS in QEMU, from refusal to boot order (`osdev test deps`, 19/0).

## 🚧 Still waiting on GodspeedOS

- 🔌 A driver package is given its device's **registers only**: no DMA arena and no interrupt yet.
- 📴 `godspeed fetch --offline` does not yet carry a package's requirements in the same file: install each
  requirement from its own file first, then the package (that order works today).
- ▶️ `godspeed run` and `godspeed test` for a program: there is no public way yet to boot a machine with your
  program on it, so the CLI says so instead of pretending.

## 💻 Builds

🐧 Linux x86_64 and Arm64 (static, any distribution) · 🍎 macOS Apple silicon and Intel · 🪟 Windows x64 and Arm64.
Built against GodspeedOS **v0.30.0**.

## ⬇️ Install

Install on Linux or macOS:

    curl --proto '=https' --tlsv1.2 -sSf https://github.com/godspeedhq/godspeed-cli/releases/latest/download/install.sh | sh

Install on Windows (PowerShell):

    irm https://github.com/godspeedhq/godspeed-cli/releases/latest/download/install.ps1 | iex

The installers check each binary against SHA256SUMS before running it. A checksum shows the file is
the one this release lists; releases are not signed yet.
