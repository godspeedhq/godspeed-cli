# 🚀 godspeed 0.2.0 - it updates itself, and knows the registry

The developer CLI for [GodspeedOS](https://github.com/godspeedhq/godspeed-os). This release makes the everyday
parts shorter: the CLI updates itself, finds the official registry on its own, and checks a publish before
anything leaves your machine.

```sh
godspeed self update               # ⬆️ from now on, this is how you update
godspeed fetch godspeed/hello      # 🛡️ no flags: the official registry and its root of trust are built in
godspeed publish --dry-run         # 🧪 everything but the upload
```

📖 **Guide and examples:** https://godspeedhq.github.io/godspeed-cli/

> **Coming from 0.1.0?** It has no `self update` yet, so update this once by running the installer again (below).
> From 0.2.0 on, `godspeed self update` does it.

## ✨ What's new

- ⬆️ **`godspeed self update`**, as with rustup: it downloads the latest release's build for your machine,
  checks it against the release's `SHA256SUMS`, runs it, and only then swaps it in. A mismatch, or a binary that
  does not run, changes nothing. The update notice and `doctor` now point here.
- 🧹 **`godspeed self uninstall`** removes the binary, its folder and the PATH entries the installer added -
  after listing them and asking (`--yes` to skip). It **keeps your signing key and tokens**, and says where they are.
- 🌍 **The official registry is the default**, with its root of trust built into the CLI. `fetch`, `login`,
  `whoami` and `publish` need no `--registry`, and a first `fetch` accepts the registry only if its root matches
  the one built in. `--registry`, `GODSPEED_REGISTRY` and `login` still choose any other.
- 🧪 **`godspeed publish --dry-run`** packages, signs, checks your token, your namespace and whether the version is
  free, then says what publishing would do. Nothing is uploaded.
- 🆕 **`godspeed new service hello --publisher alice`** fills in the publisher, so the project packages straight away.
- ⌨️ **Tab completion:** `godspeed completions bash | zsh | fish | powershell | elvish`
  (`godspeed completions --help` says how to load it).
- 🤖 **`--json`** on `doctor`, `verify`, `package`, `fetch` and `whoami`, for CI and scripts. Refusals come as JSON
  too, with a stable `kind` and the same exit code.
- 🎨 **Colour** in error messages on a terminal (`NO_COLOR` turns it off).
- 🚦 **New exit code 29**, *cancelled*: you declined, or there was no terminal to ask and `--yes` was not given.

## 🔬 Tested harder

- The "nobody knows if it published" path now has an end-to-end test: a real build and publish against a stand-in
  registry whose final step never answers. The CLI reads the index and reports *published* when it was, and
  *outcome unknown* (exit 25) when it cannot tell - never a guess.

## 🚧 Still waiting on GodspeedOS

`godspeed run`, `godspeed install`, and the utility, app and driver kinds - as in 0.1.0, the CLI says what the OS
is missing instead of pretending.

## 💻 Builds

🐧 Linux x86_64 and Arm64 (static, any distribution) · 🍎 macOS Apple silicon and Intel · 🪟 Windows x64 and Arm64.
Built against GodspeedOS **v0.28.0**.
