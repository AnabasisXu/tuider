# Featured 审核第二轮：选区 → dict / AI

> 日期：2026-07-25  
> 状态：已定稿（待实现）  
> 前置：第一轮 `2026-07-25-featured-review-fixes-design.md` 已交付并通过手测  
> 策略：最短 diff；复用 visual 选区与现有 filter / AI；无新依赖、无 ABI、无 keymap 框架

## 1. 结论

| # | 项 | 类型 |
|---|----|------|
| 1 | visual `d`：选区 → dict 侧栏 filter 查词 | 交互 |
| 2 | visual `a`：选区 → AI 系统上下文并打开面板 | 交互 |
| 3 | 成功后退出 visual（与 `y` 一致） | 行为一致 |
| 4 | FEATURES §6.5 / §6.7 同步 | docs |

**硬边界（已确认）**

- **dict**：仅当**当前会话已是 dict**（`source.list_dicts()` 非空）时生效；非 dict 只 status，不切源、不挂旁路 lookup。
- **AI**：选区写入 **system 上下文**；`input` **保持空**；**不**自动发送。
- **键**：仅 visual 内 `d` / `a`（`KeyModifiers::NONE`）。
- **后效**：动作成功 → `visual = None`；无选区 / 空 trim / 非 dict 会话 / AI 未编译 → **不**退出 visual。

## 2. 非目标

- URL RSS/Atom feed  
- 书签保存 / 跳转  
- `spc m m` / `spc b b` 及通用快捷键自定义  
- 跨文档挂载 dict / 独立查词弹层  
- 新依赖、插件 ABI 变更、配置语言  

## 3. 行为规格

### 3.1 共享：取选区文本

与 `yank_selection` 同源：

| `VisualKind` | 文本 |
|--------------|------|
| `Char` | `selected_plain_char(body, &sel)` |
| `Line` | `selected_plain(body, a_line..=b_line)` |
| `Cursor` | **无选区** → status，不动作、不退出 visual |

规则：

1. 抽出后 **`trim`**（两端空白）。  
2. trim 后为空 → status `…: empty selection`，不退出 visual。  
3. 实现 `selection_plain(&self) -> Option<String>`（或等价），`y` / `d` / `a` 共用，避免三份复制。

### 3.2 `d` → dict filter

**前置：** `!source.list_dicts().is_empty()`（dict 多词典列表非空 = dict 会话信号）。

**步骤：**

1. 取选区 plain；失败则 status，return。  
2. 可选：长度上限 **200** 字符（超出截断；status 可注明 truncated）。  
3. `self.filter = text`；`self.list_sel = 0`。  
4. `self.refilter()`（现有：无命中 → 清 body + `no matches`；有命中 → `load_selected`）。  
5. `self.visual = None`。  
6. 若 refilter 未覆盖 status，可设 `dict filter: {preview}`。

**非 dict 会话：**

- `status = "dict: not a dictionary session"`  
- **保留** visual  

### 3.3 `a` → AI selection context

**feature = `ai` 时：**

1. 取选区 plain；失败则 status，return。  
2. `refresh_ai_context()`（全文仍作 doc preview）。  
3. `ai.set_selection_context(text)`：写入 `selection_context` 字段（覆盖旧值，不累加）。  
4. 若 `!ai.open` → `ai.toggle()`；已开则保持。  
5. **不**改 `input` / `cursor`；**不**调用发送。  
6. `visual = None`。  
7. `status = "AI: selection in context ({n} chars)"`（n = 写入前或截断后字符数，实现时固定一种并写清）。

**system prompt 注入**（在现有 `--- preview ---` … `--- end ---` **之后**）：

```text
--- user selection ---
{selection}
--- end selection ---
```

- 截断上限与 doc preview 同量级：**4000** 字符（或与 `PREVIEW_CHARS` 共用常量策略；选其一写死）。  
- 无 selection 时 **不**追加上述块。  
- 再次 `a`：**覆盖** `selection_context`。

**`feature` 未编 AI：**

- `status = "AI: not in this build"`  
- **保留** visual  

### 3.4 键位

`handle_visual_key`（`KeyModifiers::NONE`）：

| 键 | 行为 |
|----|------|
| `y` | 现有 yank（成功退出 visual） |
| `d` | `dict_from_selection` |
| `a` | `ai_from_selection` |

- visual 内 **`a` 不触发** HN normal 的 plugin action（`InputMode::Visual` 已截断 fallthrough）。  
- visual status 串更新为含 `d dict · a AI`（与现有 `y · Esc` 并列）。

### 3.5 文档

`docs/FEATURES.md`：

- §6.5 Visual：补充 `d` / `a` 一行。  
- §6.7 AI：补充「visual `a` 将选区注入 system，input 空」。  
- 帮助 overlay / 底部 hint 若枚举 visual 键，同步 `d`/`a`（有则改，无则不扩 scope）。

## 4. 代码落点

| 文件 | 改动 |
|------|------|
| `src/app/visual.rs` | `selection_plain`；`dict_from_selection`；`ai_from_selection`；status 串；可选让 `yank` 走 helper |
| `src/app/keys.rs` | visual 绑 `d` / `a` |
| `src/ai.rs` | `selection_context` + setter；`send`/system 拼装注入 |
| `docs/FEATURES.md` | §6.5 / §6.7 |
| 测试 | helper 切片；非 dict `d` 不改 filter；AI setter / system 含 selection 块（测可测的纯函数或字段） |

## 5. 验收

```fish
cargo build
cargo test -q
# 手动（tdd 需先 cargo build）：
# 1. dict 会话：v 选 headword 片段 → d → filter=选区，有命中则打开释义，visual 关
# 2. md/url 等：v 选字 → d → status「not a dictionary session」，visual 仍在
# 3. 任意：v 选段 → a → AI 开、input 空；提问时 selection 在 system；visual 关
# 4. Cursor 态 d/a → empty/no selection status，不退出
# 5. y 行为不变
```

## 6. 实施顺序

1. `selection_plain` + 单测（可顺手让 yank 复用）  
2. `dict_from_selection` + visual `d`  
3. AI `selection_context` + system 注入 + visual `a`  
4. status 串 / FEATURES  
5. `cargo build` + `cargo test`  

## 7. 自检

- [x] 无 TBD/占位  
- [x] 与 feed / 书签 / leader 无交叉实现  
- [x] 非 dict / 无 AI build / 空选区不误清 visual  
- [x] visual `a` 与 HN normal `a` 不冲突（Mode 隔离）  
- [x] AI 不自动发送、input 空  
