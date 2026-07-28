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
  tuider-plugin-epub
)
# dict pulls AGPL mdx-tui-mdict — opt-in only
if [[ "${INCLUDE_DICT:-0}" == "1" || "${2:-}" == "with-dict" ]]; then
  PACKAGES+=(tuider-plugin-dict)
fi

echo "building plugins ($PROFILE)…"
cargo_args=()
for p in "${PACKAGES[@]}"; do
  cargo_args+=(-p "$p")
done
if [[ "$PROFILE" == "release" ]]; then
  cargo build --release "${cargo_args[@]}"
else
  cargo build "${cargo_args[@]}"
fi

mkdir -p "$DEST"
TARGET_DIR="target/$PROFILE"
for pkg in "${PACKAGES[@]}"; do
  # tuider-plugin-url -> libtuider_url.so
  short="${pkg#tuider-plugin-}"
  so="libtuider_${short}.so"
  src="$TARGET_DIR/$so"
  if [[ -f "$src" ]]; then
    cp -f "$src" "$DEST/"
    echo "installed $so → $DEST/"
  else
    echo "warn: missing $src" >&2
  fi
done

echo "done. plugins dir: $DEST"
