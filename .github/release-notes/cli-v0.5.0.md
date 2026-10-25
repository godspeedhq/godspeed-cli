# 🚀 godspeed 0.5.0 - drivers with an interrupt and a DMA arena

The developer CLI for [GodspeedOS](https://github.com/godspeedhq/godspeed-os), released alongside
**GodspeedOS v0.31.0**. That OS release gives a driver package its device's interrupt, a DMA arena, a
device taken over from the built-in driver, and USB devices. This CLI's driver template uses the first two.

```sh
godspeed new driver sensor        # 🔌 now waits for its interrupt and owns a DMA arena
```

> **Updating from 0.4.0:** `godspeed self update`.

## ✨ What's new

### 🔌 The driver template takes its interrupt and a DMA arena

- 🔔 `hw_pci_irq = true` in its contract, and it waits with **`gs::driver::irq`** instead of polling.
- 🧱 `hw_dma = "..."`, laid out by name with **`gs::driver::arena`**, never an address.
- ✅ It needs **GodspeedOS v0.31.0** or later: `godspeed check` against v0.30.0 refuses `hw_dma`, by name.

### 🧭 The machine's verbs, said right

- ⬆️ `upgrade` is a verb on the machine, not on your workstation. Typing it here says to run it at the `gsh>`
  prompt (exit 22, like the other machine verbs). `update` isn't a verb anywhere, and points you to
  `upgrade`, `list updates` and `self update`.
- 📦 After `publish`, the next-step hint prints `godspeed fetch <name>` **before** `godspeed install <name>`,
  because a machine installs only what it has fetched (or a `.gsp` file).

### 📚 Documentation

The README, the site, `CLAUDE.md`, the template and example READMEs and the design documents were checked
against the CLI, the registry and the OS, and corrected where they said something that was not true. The
README no longer names versions that move with every release.

## 💻 Builds

🐧 Linux x86_64 and Arm64 (static, any distribution) · 🍎 macOS Apple silicon and Intel · 🪟 Windows x64 and Arm64.
Built against GodspeedOS **v0.31.0**, and CI proves every template against it.
