#!/usr/bin/env bash
# TUI 黑盒冒烟：用 herdr pane 驱动 tuider，断言屏幕文本。
# 机制：pane = 真实 PTY 终端；wait-output 匹配屏幕输出 = 断言；超时 = FAIL + dump 屏幕。
# 详见 docs/tui-herdr-testing.md
set -uo pipefail

TUIDER_DIR="${1:-/root/cleantest/tuider}"
BIN="${TUIDER_DIR}/target/debug/tuider"
FIXTURE="${TUIDER_DIR}/README.md"
PLUGINS="${TUIDER_DIR}/plugins"
LABEL="tui-smoke-$$"

if [ "${HERDR_ENV:-}" != "1" ]; then
  echo "SKIP: 不在 herdr 环境（HERDR_ENV != 1）"
  exit 0
fi

# 定位当前 workspace（focused agent 所在），避免写死
WS=$(herdr agent list | jq -r '.result.agents[] | select(.focused==true) | .workspace_id' | head -1)
[ -n "$WS" ] || WS=w1

tab=$(herdr tab create --workspace "$WS" --cwd "$TUIDER_DIR" --label "$LABEL" --no-focus)
pane=$(jq -r '.result.root_pane.pane_id' <<<"$tab")
tabid=$(jq -r '.result.tab.tab_id' <<<"$tab")

pass() { echo "PASS: $1"; }
fail() {
  echo "FAIL: $1"
  herdr pane read "$pane" --source visible   # dump 屏幕，留给 agent 排查
  herdr tab close "$tabid" 2>/dev/null
  exit 1
}

echo "== tuider TUI 冒烟（herdr $WS:$pane）=="

# ① 启动：真实 TUI 进程在 pane 里跑起来
herdr pane run "$pane" "TERM=xterm-256color TUIDER_PLUGINS_DIR=$PLUGINS $BIN $FIXTURE" \
  || fail "pane run 启动失败"
herdr pane wait-output --match '轻量终端阅读器' --timeout 8000 "$pane" >/dev/null \
  && pass "启动：README 正文渲染" || fail "启动：正文未出现"

# ② 帮助面板：? 键 → 屏幕出现 Ctrl+Q 键位说明
herdr pane send-text "$pane" '?'
herdr pane wait-output --match 'Ctrl+Q' --timeout 5000 "$pane" >/dev/null \
  && pass "帮助：? 弹出帮助面板" || fail "帮助：Ctrl+Q 未出现"

# ③ Esc 关帮助，/ 进入搜索，输入词回车 → 状态栏报命中数 1/17
herdr pane send-keys "$pane" esc
herdr pane send-text "$pane" '/插件'
herdr pane send-keys "$pane" enter
herdr pane wait-output --match '/插件' --timeout 5000 "$pane" >/dev/null \
  && pass "搜索：/插件 命中并显示计数" || fail "搜索：命中计数未出现"

# ④ n 跳转下一命中：状态栏 1/17 → 2/17
herdr pane send-text "$pane" 'n'
herdr pane wait-output --match '2/17' --timeout 5000 "$pane" >/dev/null \
  && pass "导航：n 跳转到第 2 处命中" || fail "导航：2/17 未出现"

# ⑤ 干净退出：Ctrl+Q → 回到 shell 提示符
herdr pane send-keys "$pane" ctrl+q
herdr pane wait-output --match 'cleantest/tuider' --timeout 5000 "$pane" >/dev/null \
  && pass "退出：Ctrl+Q 回到 shell" || fail "退出：未回到 shell"

herdr tab close "$tabid"
echo "== 全部通过 =="