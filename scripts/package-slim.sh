#!/usr/bin/env bash
# Build Linux slim release tarball: host + url/hn/epub (no AGPL dict).
# usage: scripts/package-slim.sh [outdir]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

OUT_DIR="${1:-dist}"
VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
ARCH="$(uname -m)"
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
NAME="tuider-${VERSION}-${OS}-${ARCH}-slim"
STAGE="$(mktemp -d "${TMPDIR:-/tmp}/tuider-slim.XXXXXX")"
trap 'rm -rf "$STAGE"' EXIT

echo "== build host + slim plugins (release) =="
cargo build --release -p tuider \
  -p tuider-plugin-url \
  -p tuider-plugin-hn \
  -p tuider-plugin-epub

BIN="target/release/tuider"
test -x "$BIN"

PKG="$STAGE/$NAME"
mkdir -p "$PKG/plugins"
cp -f "$BIN" "$PKG/tuider"
chmod 755 "$PKG/tuider"

for short in url hn epub; do
  so="libtuider_${short}.so"
  src="target/release/$so"
  test -f "$src" || { echo "missing $src" >&2; exit 1; }
  cp -f "$src" "$PKG/plugins/$so"
done

cp -f LICENSE-MIT LICENSE-APACHE README.md CHANGELOG.md "$PKG/"
cat >"$PKG/INSTALL.txt" <<EOF
Tuider ${VERSION} — Linux slim (no dict / AGPL)

1. Unpack anywhere.
2. Run:  TUIDER_PLUGINS_DIR=\$PWD/plugins ./tuider README.md
   Or copy plugins/* to ~/.local/share/tuider/plugins and put tuider on PATH.
3. dict (AGPL) is not included. Build from source:
     INCLUDE_DICT=1 ./scripts/install-plugins.sh release
EOF

mkdir -p "$OUT_DIR"
TAR="$OUT_DIR/${NAME}.tar.gz"
tar -C "$STAGE" -czf "$TAR" "$NAME"
echo "wrote $TAR"
ls -lh "$TAR"
