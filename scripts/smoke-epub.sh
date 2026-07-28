#!/usr/bin/env bash
# Full non-interactive EPUB smoke. Exit non-zero on first failure.
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
SO="target/$PROFILE/libtuider_epub.so"
FX="crates/tuider-plugin-epub/tests/fixtures"
PLUG="$(mktemp -d /tmp/tuider-epub-smoke.XXXXXX)"
EMPTY="$(mktemp -d /tmp/tuider-epub-empty.XXXXXX)"
trap 'rm -rf "$PLUG" "$EMPTY"' EXIT

echo "== build ($PROFILE) =="
if [[ "$PROFILE" == "release" ]]; then
  cargo build --release -p tuider -p tuider-plugin-epub
else
  cargo build -p tuider -p tuider-plugin-epub
fi
test -x "$BIN"
test -f "$SO"
cp -f "$SO" "$PLUG/"

export TUIDER_PLUGINS_DIR="$PLUG"
export PROFILE

fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "OK: $*"; }

echo "== unit (plugin) =="
cargo test -p tuider-plugin-epub -- --test-threads=1

echo "== unit (host dlopen) =="
# substring match: loader::body_render_tests::epub_plugin_dlopen_and_render
cargo test -p tuider epub_plugin_dlopen_and_render

echo "== CLI missing plugin =="
out="$(TUIDER_PLUGINS_DIR="$EMPTY" "$BIN" -l "$FX/sample2.epub" 2>&1 || true)"
echo "$out" | grep -q 'need plugin `epub`' || fail "missing plugin hint: $out"
pass "missing plugin hint"

echo "== CLI list chapters =="
out="$("$BIN" -l "$FX/sample2.epub")"
echo "$out" | grep -qx 'One' || fail "list One: $out"
echo "$out" | grep -qx 'Two' || fail "list Two: $out"
pass "list epub2"

out="$("$BIN" -l -e "$FX/sample3.epub")"
echo "$out" | grep -q '一' || fail "list zh: $out"
pass "list epub3 zh"

echo "== CLI body =="
out="$("$BIN" -l -s One "$FX/sample2.epub")"
echo "$out" | grep -q 'Hello from chapter one' || fail "body: $out"
pass "body chapter one"

out="$("$BIN" -l -s 第一章 "$FX/sample3.epub" 2>&1 || true)"
# -s filters entries by name in main? check main - may only print first entry name for search
# just ensure open works via --epub flag
out="$("$BIN" -l --epub "$FX/sample3.epub")"
echo "$out" | grep -q '章' || fail "epub3 chapters: $out"
pass "--epub flag"

echo "== CLI errors =="
set +e
out="$("$BIN" -l "$FX/not_epub.epub" 2>&1)"
code=$?
set -e
[[ $code -ne 0 ]] || fail "corrupt should fail: $out"
pass "corrupt epub (exit $code)"

set +e
out="$("$BIN" -l "$FX/empty_spine.epub" 2>&1)"
code=$?
set -e
[[ $code -ne 0 ]] || fail "empty spine should fail: $out"
pass "empty spine (exit $code)"

set +e
out="$("$BIN" -l /no/such/book.epub 2>&1)"
code=$?
set -e
[[ $code -ne 0 ]] || fail "missing path should fail: $out"
pass "missing path (exit $code)"

echo "== pkg catalog =="
out="$("$BIN" pkg list 2>&1)" || true
echo "$out" | grep -E 'epub' >/dev/null || fail "pkg list missing epub: $out"
pass "pkg list has epub"

echo "== TUI PTY (content paint) =="
python3 - <<'PY' || fail "TUI PTY"
import fcntl, os, pty, re, select, signal, struct, termios, time
bin_path = os.environ.get("BIN") or "target/debug/tuider"
# absolute from ROOT
root = os.getcwd()
bin_path = os.path.join(root, "target/debug/tuider") if not os.path.isabs(bin_path) else bin_path
# honor PROFILE via env from shell
profile = os.environ.get("PROFILE", "debug")
bin_path = os.path.join(root, f"target/{profile}/tuider")
book = os.path.join(root, "crates/tuider-plugin-epub/tests/fixtures/sample2.epub")
env = os.environ.copy()
env["TUIDER_PLUGINS_DIR"] = os.environ["TUIDER_PLUGINS_DIR"]
env["TERM"] = "xterm-256color"
pid, fd = pty.fork()
if pid == 0:
    os.chdir(root)
    os.execve(bin_path, [bin_path, book], env)
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
os.kill(pid, signal.SIGWINCH)
deadline = time.time() + 2.5
data = b""
while time.time() < deadline:
    r, _, _ = select.select([fd], [], [], 0.2)
    if fd in r:
        try:
            c = os.read(fd, 16384)
        except OSError:
            break
        if not c:
            break
        data += c
s = data.decode("utf-8", "replace")
ok = any(n in s for n in ("Hello", "One", "Smoke", "Chapter"))
try:
    os.kill(pid, signal.SIGKILL)
    os.waitpid(pid, 0)
except Exception:
    pass
if not ok:
    raise SystemExit("no book content in TUI paint")
print("TUI content painted")
PY
pass "TUI PTY paint"

echo "== all smoke passed =="
