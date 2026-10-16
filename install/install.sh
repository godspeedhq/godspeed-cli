#!/bin/sh
# Install the Godspeed developer CLI (`godspeed`) on Linux or macOS.
#
#   curl --proto '=https' --tlsv1.2 -sSf https://github.com/godspeedhq/godspeed-cli/releases/latest/download/install.sh | sh
#
# What it does, in order, and nothing else:
#   1. works out which build fits this machine (OS and CPU);
#   2. downloads that build and the release's SHA256SUMS from GitHub Releases;
#   3. checks the build's SHA-256 against SHA256SUMS, and stops if it does not match or cannot be checked;
#   4. installs it as ~/.godspeed/bin/godspeed and runs `godspeed --version` to confirm it works;
#   5. adds ~/.godspeed/bin to PATH in your shell profiles, saying which files it changed.
#
# Settings, all optional:
#   GODSPEED_VERSION=0.1.0           install that release instead of the latest
#   GODSPEED_INSTALL_DIR=<dir>       install there instead of ~/.godspeed/bin
#   GODSPEED_NO_MODIFY_PATH=1        leave shell profiles alone
#   GODSPEED_DOWNLOAD_URL=<url>      fetch from a mirror (or a local test server) instead of GitHub
#
# A checksum proves the download is the file the release lists. It does not prove who made the release:
# releases are not signed yet.

set -eu

REPO="godspeedhq/godspeed-cli"
VERSION="${GODSPEED_VERSION:-}"
INSTALL_DIR="${GODSPEED_INSTALL_DIR:-$HOME/.godspeed/bin}"

say() { printf 'godspeed-install: %s\n' "$1"; }
die() {
  printf 'godspeed-install: error: %s\n' "$1" >&2
  [ $# -gt 1 ] && printf 'godspeed-install: next: %s\n' "$2" >&2
  exit 1
}

# --- 1. Which build ----------------------------------------------------------------------------------

os=$(uname -s)
cpu=$(uname -m)
case "$cpu" in
  x86_64 | amd64) cpu=x86_64 ;;
  aarch64 | arm64) cpu=aarch64 ;;
  *) die "no build for CPU '$cpu'." "Builds exist for x86_64 and aarch64; or build it: cargo install --git https://github.com/$REPO --locked godspeed-cli" ;;
esac
case "$os" in
  Linux) target="$cpu-unknown-linux-musl" ;;
  Darwin)
    # A shell running under Rosetta reports x86_64 on Apple silicon; install the native build anyway.
    if [ "$cpu" = x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = 1 ]; then
      cpu=aarch64
    fi
    target="$cpu-apple-darwin"
    ;;
  MINGW* | MSYS* | CYGWIN*) die "this is the Linux and macOS installer." "On Windows, in PowerShell: irm https://github.com/$REPO/releases/latest/download/install.ps1 | iex" ;;
  *) die "no build for '$os'." "Builds exist for Linux and macOS (and Windows, via install.ps1)." ;;
esac
asset="godspeed-$target"

if [ -n "${GODSPEED_DOWNLOAD_URL:-}" ]; then
  base="${GODSPEED_DOWNLOAD_URL%/}"
elif [ -n "$VERSION" ]; then
  base="https://github.com/$REPO/releases/download/cli-v${VERSION#v}"
else
  base="https://github.com/$REPO/releases/latest/download"
fi

# --- 2. Download --------------------------------------------------------------------------------------

# HTTPS only - unless a download URL was given explicitly and is plain http (a local test server).
proto='=https'
case "${GODSPEED_DOWNLOAD_URL:-}" in http://*) proto='=https,http' ;; esac
if command -v curl > /dev/null 2>&1; then
  fetch() { curl --proto "$proto" --proto-redir "$proto" --tlsv1.2 -fsSL --retry 3 -o "$2" "$1"; }
elif command -v wget > /dev/null 2>&1; then
  fetch() { wget -q -O "$2" "$1"; }
else
  die "neither curl nor wget is installed." "Install one of them and run this again."
fi

if command -v sha256sum > /dev/null 2>&1; then
  sha256() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum > /dev/null 2>&1; then
  sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  # Never install what cannot be checked.
  die "neither sha256sum nor shasum is installed, so the download cannot be verified." "Install coreutils (or perl's shasum) and run this again."
fi

tmp=$(mktemp -d 2> /dev/null || mktemp -d -t godspeed)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "downloading $asset from $base"
fetch "$base/$asset" "$tmp/$asset" || die "could not download $base/$asset." "Check the network, and that ${VERSION:+release $VERSION}${VERSION:-a release} has been published with a build for $target."
fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || die "could not download $base/SHA256SUMS." "Without it the download cannot be verified, so nothing was installed."

# --- 3. Verify ----------------------------------------------------------------------------------------

expected=$(awk -v f="$asset" '$2 == f || $2 == "*" f { print $1 }' "$tmp/SHA256SUMS")
[ -n "$expected" ] || die "SHA256SUMS does not list $asset." "Nothing was installed."
actual=$(sha256 "$tmp/$asset")
[ "$actual" = "$expected" ] || die "checksum mismatch for $asset: expected $expected, got $actual." "Nothing was installed. Try again; if it persists, report it."
say "verified sha256 $actual"

# --- 4. Install ---------------------------------------------------------------------------------------

mkdir -p "$INSTALL_DIR"
chmod +x "$tmp/$asset"
# Move into place in one step, so a running `godspeed` is never half-overwritten.
mv "$tmp/$asset" "$INSTALL_DIR/.godspeed.new"
mv "$INSTALL_DIR/.godspeed.new" "$INSTALL_DIR/godspeed"
installed=$("$INSTALL_DIR/godspeed" --version) || die "the installed godspeed does not run." "Report it, with the output of: uname -a"
say "installed $installed at $INSTALL_DIR/godspeed"

# --- 5. PATH ------------------------------------------------------------------------------------------

case ":$PATH:" in
  *":$INSTALL_DIR:"*) on_path=1 ;;
  *) on_path=0 ;;
esac
if [ "$on_path" = 0 ] && [ "${GODSPEED_NO_MODIFY_PATH:-0}" != 1 ]; then
  line="export PATH=\"$INSTALL_DIR:\$PATH\"  # added by the godspeed installer"
  changed=""
  for rc in "$HOME/.profile" "$HOME/.bashrc" "$HOME/.zshrc"; do
    # .profile always (login shells read it); the others only if that shell is in use.
    if [ "$rc" = "$HOME/.profile" ] || [ -f "$rc" ]; then
      if ! grep -qsF "added by the godspeed installer" "$rc"; then
        printf '\n%s\n' "$line" >> "$rc"
        changed="$changed $rc"
      fi
    fi
  done
  [ -n "$changed" ] && say "added $INSTALL_DIR to PATH in:$changed"
  say "open a new terminal, or run: export PATH=\"$INSTALL_DIR:\$PATH\""
elif [ "$on_path" = 0 ]; then
  say "$INSTALL_DIR is not on PATH (GODSPEED_NO_MODIFY_PATH=1 left your profiles alone)."
fi

if ! command -v rustup > /dev/null 2>&1; then
  say "godspeed builds with Rust, and rustup is not installed: https://rustup.rs"
fi
say "next: godspeed doctor"
