#!/usr/bin/env bash
# Build Tuider plugin .so files and copy into plugins dir.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PROFILE="${1:-debug}" # debug | release
DEST="${TUIDER_PLUGINS_DIR:-$HOME/.local/share/tuider/plugins}"

case "$PROFILE" in
  debug|release) ;;
  *)
    echo "usage: $0 [debug|release]" >&2
    exit 2
    ;;
esac

PACKAGES=(
  tuider-plugin-url
  tuider-plugin-hn
  tuider-plugin-dict
)

echo "building plugins ($PROFILE)…"
if [[ "$PROFILE" == "release" ]]; then
  cargo build --release -p tuider-plugin-url -p tuider-plugin-hn -p tuider-plugin-dict
else
  cargo build -p tuider-plugin-url -p tuider-plugin-hn -p tuider-plugin-dict
fi

mkdir -p "$DEST"
TARGET_DIR="target/$PROFILE"
for so in libtuider_url.so libtuider_hn.so libtuider_dict.so; do
  src="$TARGET_DIR/$so"
  if [[ -f "$src" ]]; then
    cp -f "$src" "$DEST/"
    echo "installed $so → $DEST/"
  else
    echo "warn: missing $src" >&2
  fi
done

echo "done. plugins dir: $DEST"
