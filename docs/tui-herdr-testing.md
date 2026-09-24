# Tuider TUI 黑盒测试：herdr pane 方案

目录：`~/cleantest/tuider`
日期：2026-09-22
相关：[`ui-testing.md`](ui-testing.md) · [`testing.md`](testing.md)

本文解决一个具体痛点：**开发 agent（OMP / Claude 等）改 TUI 时拿不到界面实际渲染结果**——只能读代码推状态，看不到画出来的屏。方案：用 herdr 的 pane 原语把 tuider 跑在真实 PTY 里，agent 直接读屏幕纯文本、发按键、事件驱动等待断言。已在 herdr 0.9.0 实测通过（2026-09-22）。

与 [`ui-testing.md`](ui-testing.md) 的关系：那篇的四层漏斗（L1 状态机 / L2 TestBackend / L3 PTY 冒烟 / L4 人工）是 **CI 回归**架构；本文是 **agent 工作期验证**通道 + L3 的现代化实现。L1/L2 不动，照旧。

---

## 1. 为什么是 herdr

| 需求 | tmux 做法 | herdr 做法 |
|------|-----------|------------|
| 启动 TUI | `new-session -d` + 会话名管理 | `tab create --no-focus` + `pane run` |
| 等文本出现 | 只能 `sleep` 盲等 | `wait-output --match/--regex` 事件驱动 |
| 读屏幕 | `capture-pane -p`（纯文本） | `read --source visible`（纯文本） |
| 发按键 | `send-keys` | `send-text`（字符）/ `send-keys`（键名） |
| 清理 | `kill-session`（易残留游离会话） | `tab close`（自建自删） |

关键差异：**等待是事件驱动的**。`wait-output` 在匹配到文本或超时之间毫秒级返回，不用 sleep 轮询（符合 herdr-long-task 的 watcher 原则，省上下文 token）；超时即失败并可 dump 屏幕，正是 agent 排查要的"实际结果"。

## 2. 环境前置

```bash
test "${HERDR_ENV:-}" = 1   # 不在 herdr 管理的 pane 里就停手，别控制会话
herdr --version             # 0.9.0 实测通过
```

## 3. 核心原语（速查）

| 命令 | 作用 |
|------|------|
| `herdr tab create --workspace <w> --cwd <dir> --label <名> --no-focus` | 建隔离测试 tab，返回 `root_pane.pane_id` / `tab_id` |
| `herdr pane run <pane> <cmd...>` | 在 pane 里跑命令（完整命令行，含 env 前缀） |
| `herdr pane wait-output [--match <文本>\|--regex <正则>] [--timeout <ms>] <pane>` | 事件驱动等输出；默认 source=recent（软换行拼接，适合日志） |
| `herdr pane read <pane> --source visible` | 读当前屏幕纯文本（剥完 ANSI） |
| `herdr pane send-text <pane> <文本>` | 发纯文本，**不加 Enter** —— 单字符按键（`?`/`q`）用它 |
| `herdr pane send-keys <pane> <键名>` | 发终端键/组合键：`esc`/`enter`/`ctrl+h` 等 |
| `herdr tab close <tab_id>` | 收尾清理，只关自己创建的 tab |

## 4. 实测完整序列（2026-09-22，herdr 0.9.0）

**启动 + 事件驱动等正文：**

```bash
herdr tab create --workspace w4 --cwd /root/cleantest/tuider --label tui-test-demo --no-focus
# → root_pane.pane_id=w4:pR, tab_id=w4:tQ
herdr pane run w4:pR 'TERM=xterm-256color TUIDER_PLUGINS_DIR=/root/cleantest/tuider/plugins ./target/debug/tuider /root/cleantest/tuider/README.md'
herdr pane wait-output --match '轻量终端阅读器' --timeout 8000 w4:pR
```

输出（节选）：

```text
matched_line: 轻量终端阅读器：同一界面里读文档、查 MDX 词典、看网页 / HN / EPUB，并可选 AI 对话。
```

**读屏幕 → 发 `?` → 等帮助面板 → 再读屏：**

```bash
herdr pane read w4:pR --source visible       # 正文屏幕：README ─── Tuider ─── 轻量终端阅读器…
herdr pane send-text w4:pR '?'               # 按键，不回车
herdr pane wait-output --match 'Ctrl+Q' --timeout 5000 w4:pR   # → HELP_OK
herdr pane read w4:pR --source visible       # 帮助面板出现：Type / ↑/↓ / Ctrl+F / Ctrl+Q…
```

**干净退出 + 清理：**

```bash
herdr pane send-text w4:pR 'q'
herdr tab close w4:tQ
```

## 5. 断言模式（wait-output 驱动的状态机）

| 阶段 | 等什么 | 说明 |
|------|--------|------|
| 启动 | 标题/正文关键词 | `--timeout` 给 5–8s 起步余量 |
| 按键后 | 新 UI 专属文案 | 如帮助面板的 `Ctrl+Q`，比 sleep 稳 |
| 干净退出 | shell 提示符 / 进程退出 | 验证没挂死 |
| 失败 | — | `wait-output` 超时 → 立即 `read --source visible` dump 屏幕再退出非零 |

## 6. 建议落地（下一步：scripts/smoke-tui-herdr.sh）

```text
tab create --label tui-smoke-$RANDOM --no-focus   → 解析 pane_id / tab_id
pane run  tuider <fixture>
断言序列:  wait-output 正文关键词
          → send-text '?' → wait-output 帮助关键词
          → send-text 'q' → wait-output shell 提示符
tab close
任一步超时 → dump `read --source visible` → exit 1
```

骨架（伪代码）：

```bash
tab=$(herdr tab create --workspace "$WS" --cwd "$TUIDER_DIR" --label "tui-smoke-$RANDOM" --no-focus)
pane=$(jq -r '.result.root_pane.pane_id' <<<"$tab")
tabid=$(jq -r '.result.tab.tab_id' <<<"$tab")
herdr pane run "$pane" "env TUIDER_PLUGINS_DIR=$PLUGINS ./target/debug/tuider $FIXTURE"
herdr pane wait-output --match "$NEEDLE" --timeout 8000 "$pane" || { herdr pane read "$pane" --source visible; exit 1; }
herdr tab close "$tabid"
```

纯 CI 环境（无 herdr server）时退回到 tmux 同款操作——两者语义一致，脚本把「跑命令 / 发键 / 等文本 / 读屏 / 清理」抽象成 5 个函数即可双后端复用。

## 7. 红线

- 一律 `--no-focus` 做后台活，别抢用户焦点。
- tab 自建自删；不关自己没创建的 tab。
- pane_id / tab_id 从 JSON 响应解析，不从侧栏猜。
- `wait-output` 超时是失败信号，不是"再等等"；超时先 dump 屏幕。
- 需要终端组合键（Esc 应答对话框）时用 `send-keys`（`esc`/`ctrl+h` 键名），普通字符键用 `send-text`。