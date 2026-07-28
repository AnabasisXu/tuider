#!/usr/bin/env bash
# Core CLI smoke (no plugins). Exit non-zero on first failure.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PROFILE="${1:-debug}"
case "$PROFILE" in debug|release) ;; *)
  echo "usage: $0 [debug|release]" >&2
  exit 2
  ;;
esac

BIN="target/$PROFILE/tuider"

echo "== build ($PROFILE) =="
if [[ "$PROFILE" == "release" ]]; then
  cargo build --release -p tuider
else
  cargo build -p tuider
fi
test -x "$BIN"

fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "OK: $*"; }

echo "== version =="
out="$("$BIN" -V 2>&1)"
echo "$out" | grep -q 'tuider' || fail "version: $out"
pass "version"

echo "== -l md body =="
out="$("$BIN" -l README.md)"
echo "$out" | grep -q 'Tuider' || fail "md body missing Tuider: $out"
pass "README.md body"

echo "== -l code body =="
out="$("$BIN" -l src/main.rs)"
echo "$out" | grep -q 'fn main' || fail "code body: $out"
pass "src/main.rs body"

echo "== -l dir list =="
out="$("$BIN" -l docs)"
echo "$out" | grep -q 'FEATURES.md' || fail "dir list: $out"
pass "docs list"

echo "== all core smoke passed =="
