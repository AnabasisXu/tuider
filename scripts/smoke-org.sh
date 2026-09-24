#!/usr/bin/env bash
# org-mode 冒烟：渲染管线 + CLI 列表/正文 + 错误路径。失败即非零退出。
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
FX="$(mktemp -d /tmp/tuider-org-smoke.XXXXXX)"
trap 'rm -rf "$FX"' EXIT

cat > "$FX/sample.org" <<'EOF'
#+FILETAGS: :smoke:
* TODO 一级 :tag:
  DEADLINE: <2026-09-30 Mon>
  正文。
| 物品 | 数量 |
|------+------|
| 苹果 | 3    |
** 二级
   内容。
EOF

echo "== build ($PROFILE) =="
cargo build -p tuider
test -x "$BIN"

fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "OK: $*"; }

echo "== unit: org render + fold =="
cargo test org:: -- --test-threads=1
cargo test fold:: -- --test-threads=1

echo "== CLI -l body (raw) =="
out="$("$BIN" -l "$FX/sample.org")"
echo "$out" | grep -q '^#+FILETAGS' || fail "filetags raw: $out"
echo "$out" | grep -q '苹果' || fail "table raw: $out"
echo "$out" | grep -q 'TODO 一级' || fail "todo raw: $out"
pass "raw body intact (render fidelity: unit tests)"

echo "== missing file =="
set +e
out="$("$BIN" -l /no/such.org 2>&1)"
code=$?
set -e
[[ $code -ne 0 ]] || fail "missing should fail: $out"
pass "missing file (exit $code)"

echo "== all org smoke passed =="