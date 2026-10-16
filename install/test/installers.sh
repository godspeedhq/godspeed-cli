#!/usr/bin/env bash
# Test install.sh against a fake release served from <dist> on localhost: a real install (PATH edits
# included - run this only somewhere disposable, like a CI runner), a reinstall that must not edit the profile
# twice, and a tampered SHA256SUMS that must install nothing.
#
#   install/test/installers.sh <dist>      (<dist> holds godspeed-<target> files and SHA256SUMS)
set -euo pipefail
DIST=$(cd "$1" && pwd)
HERE=$(cd "$(dirname "$0")/.." && pwd)
PORT=${PORT:-8790}
URL="http://127.0.0.1:$PORT"

work=$(mktemp -d)
mkdir -p "$work/srv/bad"
cp "$DIST"/* "$work/srv/"
cp "$DIST"/godspeed-* "$work/srv/bad/"
for f in "$work"/srv/bad/godspeed-*; do
  printf '%s  %s\n' "$(printf '0%.0s' $(seq 64))" "$(basename "$f")"
done > "$work/srv/bad/SHA256SUMS"

python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$work/srv" > /dev/null 2>&1 &
server=$!
trap 'kill $server 2>/dev/null; rm -rf "$work"' EXIT
curl -s --retry 30 --retry-connrefused --retry-delay 1 -o /dev/null "$URL/SHA256SUMS"

echo "--- install"
GODSPEED_DOWNLOAD_URL="$URL" sh "$HERE/install.sh"
# shellcheck disable=SC1091
version=$(. "$HOME/.profile" && godspeed --version)
echo "--- found on the PATH the installer set: $version"

echo "--- reinstall"
GODSPEED_DOWNLOAD_URL="$URL" sh "$HERE/install.sh" > /dev/null
lines=$(grep -c "added by the godspeed installer" "$HOME/.profile")
[ "$lines" = 1 ] || { echo "FAIL: .profile has $lines installer lines"; exit 1; }
echo "--- the profile was edited once"

echo "--- tampered SHA256SUMS"
if GODSPEED_DOWNLOAD_URL="$URL/bad" GODSPEED_INSTALL_DIR="$work/tampered" sh "$HERE/install.sh"; then
  echo "FAIL: the installer accepted a checksum mismatch"; exit 1
fi
[ ! -e "$work/tampered/godspeed" ] || { echo "FAIL: a binary was installed despite the mismatch"; exit 1; }
echo "--- refused, and nothing was installed"
echo "install.sh: all checks passed"
