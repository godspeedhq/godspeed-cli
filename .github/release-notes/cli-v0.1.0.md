# 🚀 godspeed 0.1.0 - the first release

The developer CLI for [GodspeedOS](https://github.com/godspeedhq/godspeed-os): create, build, package, sign and publish Godspeed software from your workstation, for all four architectures at once, and verify anyone's, without touching the kernel, raw IPC or the supervisor.

```sh
godspeed new service hello && cd hello
godspeed build --all      # 🧩 x86_64, aarch64, armv7, riscv64
godspeed publish          # ✍️ signed, uploaded, installable as you/hello
```

📖 **Guide and examples:** https://godspeedhq.github.io/godspeed-cli/

## ✨ What's in it

- 🆕 **`godspeed new service | library`**: the smallest valid project of each kind, on the public stdlib only. Kinds the OS cannot run yet (`utility`, `app`, `driver`) are refused, with the reason and where the OS tracks it.
- 🔍 **`check`** validates the project, its toolchain pin and its contract against the OS's own schema.
- 🏗️ **`build [--arch | --all]`** builds one ELF per architecture. **`test`** runs a library's tests on your machine.
- 🩺 **`doctor`** checks your toolchain, targets and QEMU, and this CLI's version, and prints the command that fixes each problem.
- 📦 **`package`** and **`verify`**: see exactly what would be published, with every file's size and SHA-256, before anything leaves your machine. Verifying a package needs no registry.
- 🔑 **`key generate`** makes your Ed25519 signing key. It stays on your machine; only the public half is shared.
- 🎫 **`login`** makes your publisher token on your machine, like an SSH key; the registry stores only its fingerprint. **`token new`** makes one for CI.
- 🚢 **`publish`**: package, sign, reserve, upload and finalise. A published version never changes, nothing half-published is ever visible, and if the network drops it reads the registry instead of guessing.
- 🛡️ **`fetch`** downloads a release and verifies the registry's whole chain of trust (root → timestamp → snapshot → namespaces → index → manifest → signature → files) before keeping anything. The design follows [The Update Framework](https://theupdateframework.io/).
- 🔔 **Update notice**: at most once a day, in the background, it tells you when a newer release is out. It's quiet in CI and scripts; `GODSPEED_NO_UPDATE_CHECK=1` turns it off.
- 🚦 **A distinct exit code for every kind of failure**, and every refusal says what failed, why, and what to do next.

## 🌍 The registry is live

`godspeed/hello@0.1.0` is already published and verifiable:

```sh
godspeed fetch godspeed/hello --registry https://godspeed-registry.bankole-422.workers.dev \
  --trust-root sha256:d769d1a64c3105c8da0ffcc2f54aba1c3a16ce4ae707a6d206556f08824a146d
```

## 🧪 Examples

[`examples/`](https://github.com/godspeedhq/godspeed-cli/tree/cli-v0.1.0/examples) has real projects, each built for every architecture in CI:

- 💓 `heartbeat`: a loop that sleeps
- 📣 `shout`: a request/reply server
- 🔢 `bootcount`: files, a library dependency, and an honest unknown outcome
- 🧵 `fixedbuf`: a no-heap text library

## 🚧 Not yet: these wait on GodspeedOS

- ▶️ **`godspeed run`** and **`godspeed install`**: GodspeedOS starts only the services compiled into its image.
- 🛠️ **Utilities, apps and drivers**: the OS has no terminating task, no app interfaces, and no device grants for outside drivers yet.
- 🦀 **Rust only, for now**: C and C++ wait on a C interface to the OS; Go would need its runtime ported.

The CLI says so instead of pretending. Each of these is the OS's to build, and the CLI will follow.

## 💻 Builds

🐧 Linux x86_64 and Arm64 (static, any distribution) · 🍎 macOS Apple silicon and Intel · 🪟 Windows x64 and Arm64. Built against GodspeedOS **v0.28.0**.
