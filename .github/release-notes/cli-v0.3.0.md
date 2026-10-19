# 🚀 godspeed 0.3.0 - every kind of program, and packages that travel without a network

The developer CLI for [GodspeedOS](https://github.com/godspeedhq/godspeed-os), released alongside
**GodspeedOS v0.29.0 "Signed, Sealed, Delivered"**. That OS release brings TLS 1.3, its own package manager,
and a supervisor that installs what you publish. This CLI is the other half: you can now build **all four
kinds** of Godspeed program, and ship any of them **as one file** to a machine that is never online.

```sh
godspeed new utility hello          # 🔧 a program that does one job and ends with a status
godspeed new app notes              # 🖼️ one that owns the screen and keyboard while it runs
godspeed new driver mydev           # 🔌 one that drives a device, named by its PCI class
godspeed fetch godspeed/hello --offline   # 💾 hello-0.3.0.gsp, installable with no network
godspeed inspect hello-0.3.0.gsp          # 🔍 checked offline; nothing kept
```

📖 **Guide and examples:** https://godspeedhq.github.io/godspeed-cli/

> **Updating from 0.2.0:** `godspeed self update`.

## ✨ What's new

### 🧩 Four kinds of program, not one

A GodspeedOS 0.29.0 machine installs four kinds of package, and `godspeed new` makes every one. Each has a
template, a project kind and its own package format:

| Kind | Whose life | New in 0.3.0 |
|---|---|---|
| ⚙️ **service** | the system's: started at boot, restarted if it dies | may declare `handover`, so `godspeed upgrade` replaces it **live** |
| 🔧 **utility** | its own: runs when typed, ends with a status, pipes like a built-in | ✅ new |
| 🖼️ **app** | the operator's: owns the console while it runs | ✅ new |
| 🔌 **driver** | its device's: given one device by PCI class, registers only | ✅ new |

`godspeed check` refuses `handover` on anything but a service. Only a service is ever replaced while it runs.

### 💾 Packages that travel as one file

- 📦 **`godspeed fetch <pkg> --offline`** writes **`<name>-<version>.gsp`** (`.gsp` for **G**odspeed**OS**
  **P**ackage). It holds every document the fetch read and checked: the roots, the timestamp, the snapshot,
  the namespaces, the package's index, its manifest, signature and contract, and its binaries (all of them,
  or `--arch` one). Copy it to a USB stick and run `godspeed install hello-0.3.0.gsp` on a machine with no
  network. The machine checks it with the same code as a download.
- 🏠 **`godspeed package --local`** makes a `.gsp` of your **own, unpublished** package, signed with your key.
  A machine installs it only once its operator trusts that key directly (`godspeed trust add` on the machine).
- 🔍 **`godspeed inspect`** checks a `.gsp` offline, or a release on the registry, and keeps nothing.
- 🏷️ You never name the file: the flag says what it is **for**, and the name comes from the manifest. Nobody
  names a `.deb` when they make one.

### 🛡️ Stronger, and in step with the OS

- 📏 **Package names follow the machine:** `publisher/name` at most **128 bytes**, the OS's new limit.
- 🌊 **The snapshot streams.** It is verified from its digest and read a line at a time, so a client never has
  to hold the whole registry in memory.
- 🧰 **The platform kit is read from the release's `godspeed-platform.toml`**, checked against its
  `SHA256SUMS`. GodspeedOS v0.29.0 is the first release to publish one, and this CLI uses it: a project made
  with no checkout fetched the 0.29.0 kit from the release the hour it was published.
- 🧪 **Building against a checkout** (`--os`) accepts a stdlib tag at or before its release, so a checkout
  ahead of its last release builds again.
- 🗣️ **`godspeed install`, `update` and the other native verbs** now say where they run: at the Godspeed
  machine's `gsh>` prompt. The CLI no longer says the package manager is not built, because it is.
- 🤫 Template manifests are `Cargo.toml.template`, so a git dependency on this repository builds quietly.

## 🧪 Tested

- `cargo fmt`, `clippy -D warnings`, and the whole test suite.
- Every template generated, built and tested against a checkout of **GodspeedOS v0.29.0** (4/4), which CI now
  proves on every change.
- Every example checked and built for all four architectures, and a library's tests run, against v0.29.0.
- A new service built **with no OS checkout at all**, from the published v0.29.0 platform file.

## 🚧 Still waiting on GodspeedOS

`godspeed run` and `godspeed test` for a service: the OS has no public way to start an outside program inside
its test image yet, so the CLI says so instead of pretending. Installing on a machine is the OS's own
`godspeed install`, at its prompt.

## 💻 Builds

🐧 Linux x86_64 and Arm64 (static, any distribution) · 🍎 macOS Apple silicon and Intel · 🪟 Windows x64 and Arm64.
Built against GodspeedOS **v0.29.0**.
