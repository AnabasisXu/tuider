# Tuider 自动化 UI 测试

目录：`~/cleantest/tuider`  
日期：2026-07-26  
相关：[`testing.md`](testing.md) · [`STATUS.md`](STATUS.md) · [`FEATURES.md`](FEATURES.md) · CI [`.github/workflows/test.yml`](../.github/workflows/test.yml)

本文说明**如何把目前偏手动的 TUI 验收，收敛成可 CI 的自动化**。  
环境隔离、插件 / ABI / 缓存等通用约定见 [`testing.md`](testing.md)；本文只谈 **UI 层**。

---

## 1. 问题与目标

### 现状

| 已有 | 位置 | 能证明什么 |
|------|------|------------|
| 状态机 / 按键 | `src/app/mod.rs` tests（如 `sidebar_o_types_filter_not_toc`） | 键 → 状态，**不测画屏** |
| 画屏 | `testbackend_draws_help_overlay` | `?` → help 文案出现在 buffer |
| CLI 冒烟 | `scripts/smoke-core.sh` | 无 TUI 的打开 / 打印 |
| PTY 冒烟 | `scripts/smoke-epub.sh` 末尾 Python | 真进程 + 终端，字符串出现在 ANSI 流里 |
| 事件环 | `App::run` | `event::poll` / `read` 写死，**默认测不到 run 本身** |

设计侧也曾写过：TUI 打开 README 偏手动；CLI `-l` 已自动化。

### 目标

- **日常快捷键 / 模态 / 侧栏 / 搜索** 不再靠手点回归  
- CI 可重复、秒级～十秒级为主  
- PTY 只保留少量端到端冒烟，不当主回归  

### 非目标

- 像素级「好不好看」（人工 L4）  
- 全量 ANSI 截图锁布局  
- CI 默认真打 HN / URL / AI  

---

## 2. 四层漏斗（推荐架构）

```text
L1  逻辑 UI（主战场）     KeyEvent → App 状态
L2  虚拟终端绘制           App + ui::draw + TestBackend → buffer 文本
L3  进程级 PTY 冒烟        真 binary + pty.fork（少量）
L4  人工 / 录屏            仅视觉审美、难自动化路径
```

| 层 | 占比（建议） | 工具 | 速度 | 稳定性 |
|----|--------------|------|------|--------|
| L1 | ~60% | `handle_key` + assert | 最快 | 最高 |
| L2 | ~25% | `ratatui::backend::TestBackend` | 快 | 高（固定尺寸） |
| L3 | ~10% | `pty` + 剥 ANSI 后 expect | 中 | 中（限条数） |
| L4 | ~5% | 人工 | — | — |

**原则：** 能 L1 解决的不进 L2；能 L2 解决的不进 L3。

---

## 3. L1 — 逻辑 UI（优先扩展）

### 3.1 已有模式

```text
MockPlain / 真实 ContentSource
  → App::new(...)
  → app.handle_key(KeyEvent::...)
  → assert 状态字段
```

范例：`sidebar_o_types_filter_not_toc`、`ai_from_selection_opens_and_sets_context`（`feature = "ai"`）。

### 3.2 建议 harness

放在 `src/app/mod.rs` 的 `#[cfg(test)]`，或抽出 `src/app/test_support.rs`：

```rust
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

/// 依次投递按键；若某次 `handle_key` 返回 true（quit），提前返回 true。
fn drive(app: &mut App, keys: impl IntoIterator<Item = KeyEvent>) -> bool {
    for k in keys {
        if app.handle_key(k) {
            return true;
        }
    }
    false
}
```

构造 App 时用现有 `MockPlain`（或最小 fixture 文件源），避免依赖本机插件目录。

### 3.3 手动操作 → 状态断言对照表

| 手动操作 | 建议断言 |
|----------|----------|
| 打开列表，侧栏开，键入过滤 | `filter`、`filtered` 长度 / 内容 |
| `Ctrl+F` 切侧栏 | `show_sidebar()` |
| `/foo` + 确认 + `n` / `N` | `vim_mode`、命中下标 / hits 长度 |
| `v` … `y` | `visual` 范围、status；剪贴板可 mock |
| `?` / Esc | `show_help()` |
| `q` / `Ctrl+Q` | `handle_key` 返回 `true` |
| 侧栏 `o` vs 正文 `o` | filter vs `nav_open()`（已有模板） |
| `s` / `zz` | line-jump / avy 相关状态标志 |
| AI 面板（feature） | `ai.open`、input / selection_context |

### 3.4 边界

- **证明：** 输入路由与业务状态正确  
- **不证明：** 像素 / 布局 / 真实终端绘制 → 交给 L2  

---

## 4. L2 — TestBackend 绘制

### 4.1 已有范本

`testbackend_draws_help_overlay`（`src/app/mod.rs`）：

1. `handle_key('?')`  
2. `Terminal::new(TestBackend::new(100, 30))`  
3. `term.draw(|f| ui::draw(f, &mut app))`  
4. 拼接 `backend().buffer().content()` 的 `symbol()`  
5. `assert!(text.contains(...))`  

### 4.2 建议工具函数

```rust
fn paint(app: &mut App, w: u16, h: u16) -> String {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| crate::ui::draw(f, app)).unwrap();
    term.backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect()
}

fn assert_paint_contains(app: &mut App, needles: &[&str]) {
    let text = paint(app, 100, 30);
    for n in needles {
        assert!(text.contains(n), "missing {n:?} in:\n{text}");
    }
}
```

### 4.3 典型用例

1. 打开 fixture 正文 → buffer 含标题 / 段落关键词  
2. `?` → 含 `Ctrl+Q` / `Quit`（已有）  
3. `/pattern` 后状态栏或命中提示（按实际 UI 文案）  
4. 侧栏开：列表项名出现（可按列切片 buffer，避免整屏模糊匹配）  
5. 空文档 / 错误 → 底栏 status 文案  

### 4.4 稳定技巧

| 做法 | 说明 |
|------|------|
| 固定尺寸 | 如 100×30，禁止依赖本机终端 |
| 断言稳定文案 | 少锁整屏快照 |
| 可选 insta | 纯文本 buffer + 绑定尺寸；theme 大改时批量更新 |
| 中文列宽 | 专用 fixture，与 ASCII 用例分开 |
| 不必改 `App::run` | L2 只调 `handle_key` + `ui::draw` |

---

## 5. L3 — 进程级 PTY（黑盒，少量）

### 5.1 已有范本

`scripts/smoke-epub.sh` 末尾：

- `pty.fork` + `execve(tuider, book)`  
- `TUIDER_PLUGINS_DIR`、`TERM=xterm-256color`  
- `TIOCSWINSZ` + `SIGWINCH`  
- 读输出，匹配 `Hello` / `One` / 章节等  
- `SIGKILL` 收尾  

### 5.2 建议抽公共脚本

```text
scripts/smoke-tui.sh
scripts/lib/pty_expect.py
```

避免每个插件复制一段 Python。

### 5.3 最小协议

```text
spawn:  env 隔离后的 tuider <fixture>
winsize: 40×120 + SIGWINCH
expect: 剥 ANSI 后的正文关键词
send:   b'?' → expect help 片段（可选）
send:   quit 键 → 进程退出（尽量 code 0）
timeout: 2–5s；失败 dump transcript
```

剥 ANSI 示例：

```python
import re
plain = re.sub(r"\x1b\[[0-9;?]*[a-zA-Z]", "", raw.decode("utf-8", "replace"))
```

### 5.4 建议保留的端到端路径（≤ 5 条）

| ID | 场景 | 原因 |
|----|------|------|
| TUI-01 | `README.md` 打开 + 退出 | core 主路径 |
| TUI-02 | epub 章节可见（已有） | 插件 + 画屏 |
| TUI-03 | 空 plugins + 需插件入口 → 错误可见或不挂死 | 失败路径 |
| TUI-04 | `Ctrl+F` 或 `/` 后屏上有反馈 | 交互冒烟 |
| TUI-05 | （可选 job）dict 有 so 时查词 | AGPL 可选 |

### 5.5 PTY 约束

- 固定 `TERM`、winsize、cwd、`TUIDER_*` / XDG  
- 失败保存 transcript  
- CI 用 `python3` 即可，无需图形  
- **禁止**用 PTY 覆盖全部快捷键（那是 L1）  

---

## 6. 可选：可测的事件环（L1.5）

当前 `App::run` 写死 `crossterm::event::poll/read`。若出现「只在 run 环里才有的 bug」（超时 poll、mouse、AI poll 交错），再抽：

```rust
pub trait EventSource {
    fn poll(&mut self, timeout: std::time::Duration) -> std::io::Result<bool>;
    fn read(&mut self) -> std::io::Result<crossterm::event::Event>;
}

// 生产：CrosstermEvents
// 测试：ScriptedEvents { queue: VecDeque<Event> }

pub fn run_with<B, E>(
    mut self,
    terminal: &mut ratatui::Terminal<B>,
    events: &mut E,
) -> std::io::Result<()>
where
    B: ratatui::backend::Backend,
    E: EventSource,
```

- 生产 `run()` → `run_with(CrosstermEvents)`  
- 测试：注入 Key/Mouse 队列 + `TestBackend`  

**第一周不必做**；L1 直接 `handle_key` 已覆盖绝大多数回归。

---

## 7. 落地顺序

### 第 0 步 — 约定与目录（约 0.5 天）

```text
src/app/          # harness 可放 test_support 或 mod tests 内
scripts/smoke-tui.sh
scripts/lib/pty_expect.py
docs/ui-testing.md          # 本文
docs/testing.md             # 交叉链接
```

CI 建议：

```text
cargo test --workspace
bash scripts/smoke-core.sh
bash scripts/smoke-epub.sh    # 或改为调用 smoke-tui 子集
bash scripts/smoke-tui.sh     # 新增后挂上
```

### 第 1 步 — 扩 L1（2–3 天）

把日常手测快捷键表变成 `drive` + 状态 assert。  
验收：`cargo test -p tuider` 覆盖 sidebar / search / visual / help / quit。

### 第 2 步 — 扩 L2（1–2 天）

每个 L1 关键路径加 **一条** paint 断言（不是每个键都 paint）。  
模板：`testbackend_draws_help_overlay`。

### 第 3 步 — 规范化 L3（约 1 天）

从 `smoke-epub.sh` 抽出 `pty_expect`；core 加 README 打开 + 退出；挂 `test.yml`。

### 第 4 步 — EventSource（按需）

仅当 L1/L2 盖不住 run 环问题时再做。

---

## 8. 用例优先级

### P0（CI 必绿）

1. 打开本地 md：状态有 body；paint 含标题词  
2. `?` help 开关 + paint  
3. 退出键 → quit  
4. 侧栏过滤  
5. vim `/` 命中计数  
6. CLI `-l`（`smoke-core.sh`，已有）  
7. epub 章节（PTY 已有，可再升一条 L2）  

### P1

- visual yank 状态、outline `o`、line-jump `s`  
- 空插件错误文案  
- HN cache-only：**CLI / 状态即可**，不必 TUI  

### P2

- AI 面板（mock HTTP）  
- dict（可选 job）  
- 鼠标点击链接  

---

## 9. 环境隔离（UI 专用提醒）

与 [`testing.md`](testing.md) §2 相同，UI 测额外强调：

```bash
export TUIDER_PLUGINS_DIR="$(mktemp -d)"
export XDG_CONFIG_HOME="$(mktemp -d)"
export XDG_CACHE_HOME="$(mktemp -d)"
export TERM=xterm-256color
unset TUIDER_AI_KEY TUIDER_AI_BASE_URL
```

| 点 | 说明 |
|----|------|
| TUI 会 auto-create config | 不隔离会写真实 `~/.config/tuider.yml` |
| L1/L2 | `cargo test` 内 tempdir / Mock 源 |
| L3 | 脚本里 export；固定 winsize |
| 插件 | 只测插件 UI 时拷对应 profile 的 `.so` 到临时 `TUIDER_PLUGINS_DIR` |

---

## 10. 手动 vs 自动化对照

| 现在手做的 | 落点 |
|------------|------|
| 打开文件看内容 | L2 paint / L3 一条 |
| 乱按快捷键看是否错乱 | L1 `drive` 序列 |
| 看 help / 状态栏文案 | L2 `assert_paint_contains` |
| 装插件后开 epub / url | L3 少量 + 插件 unit |
| 看「好不好看」 | L4 人工 |

---

## 11. 反模式

| 不建议 | 原因 |
|--------|------|
| 全量 ANSI 截图式 PTY | theme / 布局一改全红 |
| CI 真打 AI / HN 做 UI | 慢、脆、密钥 |
| 每插件复制一份 pty Python | 难维护；抽 helper |
| 只靠 `-l` 冒烟宣称「UI 测完」 | `-l` 不进 `ui::draw` / 按键路由 |
| 等完美框架再写用例 | 先扩现有 `handle_key` + TestBackend |
| PTY 覆盖全部快捷键 | 成本与噪声过高 |

---

## 12. 最小 PR（建议第一步）

1. 抽出 `key` / `drive` / `paint` 三个 test helper  
2. 新增约 5 个 L1：quit、sidebar toggle、filter、search、help  
3. 复制 help 的 TestBackend 模式 → 「打开 md 正文 paint」  
4. 将 epub PTY 块抽到 `scripts/lib/pty_expect.py`，core 加 README TUI 退出  

**不改生产事件环** 即可明显摆脱纯手动。

---

## 13. 一句话

UI 自动化 = **L1 状态机为主 + L2 固定尺寸画屏为辅 + L3 极少 PTY 冒烟**；先 harness 化现有 `handle_key` / `TestBackend` / `smoke-epub` PTY，再按 P0 清单填洞，而不是新建一套重型 UI 框架。
