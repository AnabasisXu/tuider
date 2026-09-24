#!/usr/bin/env bash
# Build tuider release + plugins and install into user dirs (~/.local).
# Usage: ./scripts/install-local.sh [release|debug]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PROFILE="${1:-release}"
case "$PROFILE" in
  debug|release) ;;
  *) echo "usage: $0 [debug|release]" >&2; exit 2 ;;
esac

BINDIR="${BINDIR:-$HOME/.local/bin}"

echo "[1/3] building tuider ($PROFILE)…"
cargo build --$PROFILE

echo "[2/3] building + installing plugins…"
INCLUDE_DICT=1 ./scripts/install-plugins.sh "$PROFILE" with-dict

echo "[3/3] installing main binary → $BINDIR/tuider"
mkdir -p "$BINDIR"
cp -f "target/$PROFILE/tuider" "$BINDIR/tuider"
chmod +x "$BINDIR/tuider"

echo "=== installed ==="
"$BINDIR/tuider" --help | head -3
ls -la "$BINDIR/tuider"
ls -la "$HOME/.local/share/tuider/plugins/"