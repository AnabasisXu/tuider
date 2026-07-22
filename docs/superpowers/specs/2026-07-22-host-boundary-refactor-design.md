# Tuider Host 边界重构（方案 A：外科清债）

> 日期：2026-07-22  
> 状态：设计已批准，待实施计划  
> 来源：架构锐评（Ousterhout 深模块）+ brainstorming  
> 范围：**P0 泄漏清理 + 边界收口**；行为冻结；`InputMode` 路由收敛

---

## 1. 目标 / 非目标 / 成功标准

### 1.1 目标

| 项 | 说明 |
|----|------|
| 单一插件知识源 | `claim_by_id` / `missing_plugin_hint` / `pkg::CATALOG` 收成一张 catalog |
| 删 host 死代码 | 删除无引用的 `src/cache.rs`（含插件路径泄漏 API） |
| 双 trait 命名收口 | API 侧改名 `PluginTextSource`；host 保留 `ContentSource` |
| HTML_V1 文档化 | 协议字符串不变；常量 + ABI 文档写明 body 扩展格式 |
| App 键位路由 | `InputMode` 收敛 `handle_key`；不改 ui getter / 布局 |
| 文档对齐现实 | STATUS / PLAN / complexity-review / plugins / NEXT 与动态 `.so` 一致 |

### 1.2 非目标（本轮不做）

- HTML/CSS 渲染下沉到插件 so 或拆成可选 feature
- `AppView` 只读快照给 `ui::draw`
- 去掉 claims 兜底、仅信 `handles`（需先审计四插件等价）
- bump `TUIDER_PLUGIN_ABI` 数值
- 删除 Cargo 空 feature 别名 `url`/`hn`/`dict`/`code`
- 重写 AI / md 渲染 / 改插件业务逻辑

### 1.3 成功标准

1. **行为冻结**：现有 CLI 认领、缺 so 提示文案结构、exit code、HTML_V1 渲染、md/txt/AI 路径与重构前一致（允许内部符号改名）。
2. **单一 catalog**：host 内无第二份 id→flag `match`。
3. **无 dead cache 模块**：`src/cache.rs` 不存在；`main` 无 `mod cache`。
4. **`cargo test -q` 通过**；catalog / InputMode / 既有 loader body 测试绿。
5. **文档**：`STATUS.md` 为加载机制权威；PLAN 中「非动态 so」矛盾已消除。

### 1.4 约束

- 所有对话与文档中文；提交信息可英文 concise。
- 多对话防冲突：本任务独占 `main.rs`、`loader.rs`、`app/keys.rs`、`pkg.rs`、plugin-api、catalog 新文件；结束前提交。
- 不默认 `cargo install`；验证用 `cargo test` / `cargo run`。

---

## 2. 架构

### 2.1 目标结构

```
src/
  plugin_catalog.rs   ← NEW：发行侧插件知识（id / so / claims / summary）
  pkg.rs              ← 复用 catalog，不再平行维护语义
  main.rs             ← 编排：registry → open / missing / files
  plugin.rs           ← host ContentSource（已渲染）
  loader.rs           ← DynSource + HostSource；HTML_V1 识别不变
  app/
    mode.rs           ← NEW：InputMode + App::input_mode()
    keys.rs           ← match input_mode() 分发
  （删除 cache.rs）

crates/tuider-plugin-api/
  lib.rs              ← C ABI 不变；PluginTextSource；BODY_HTML_V1_PREFIX
```

### 2.2 数据流（对外不变）

```
args
  → PluginRegistry (dlopen)
  → for plug: handles_args || catalog.claims(id)
  → open → HostSource → run_tui(Box<dyn ContentSource>)
  → else if catalog 启发式缺 so → need plugin hint
  → else scan_docs → FileTreeSource
```

插件 body 文本 → `render_plugin_body`（`BODY_HTML_V1_PREFIX` | md | plain）→ `App.body`。

### 2.3 `plugin_catalog`

```rust
pub struct CatalogEntry {
    pub id: &'static str,
    pub crate_name: &'static str,
    pub so_name: &'static str,
    pub summary: &'static str,
    /// 与历史 claim_by_id 语义等价；handles 失败时的 host 兜底 + 缺 so 提示
    pub claims: fn(args: &[String]) -> bool,
}

pub const CATALOG: &[CatalogEntry] = &[ /* url, hn, code, dict */ ];

pub fn find(id: &str) -> Option<&'static CatalogEntry>;
pub fn claims(id: &str, args: &[String]) -> bool;
pub fn missing_plugin_hint(args: &[String], loaded: impl Fn(&str) -> bool) -> Option<&'static str>;
```

**claims 语义（冻结，与现 `main::claim_by_id` 一致）：**

| id | 条件 |
|----|------|
| `url` | 任一项为 `-u` / `--url`，或 `starts_with("http://")` / `https://` |
| `hn` | 任一项为 `-hn` / `--hn` |
| `code` | 任一项为 `--code` |
| `dict` | 任一项为 `-g` / `--group`，或 `ends_with(".mdx")` / `.MDX` |

**missing hint（与现 `missing_plugin_hint` 一致）：** 按 url → hn → code → dict 顺序，第一个 `claims && !loaded(id)` 的 id。

`pkg` 的 install/list/remove 使用同一 `CATALOG` 的 id/crate/so/summary。

### 2.4 双 trait 收口

| 位置 | 现名 | 新名 | 职责 |
|------|------|------|------|
| `tuider-plugin-api` | `ContentSource` | `PluginTextSource` | FFI 适配后：`title` / `entries` / `load_text` |
| `src/plugin.rs` | `ContentSource` | **保持** | App：`load` → Lines + links + headings |
| `api::LoadResult` | `LoadResult` | 删除或改名（若未被使用） | 避免与 host `LoadResult` 混淆 |

C ABI（`FnOpen` / `FnLoadBody` 等）**零变更**。插件 crate 不依赖 host trait。

### 2.5 HTML_V1（文档化，不改协议）

- 在 `tuider-plugin-api` 增加：
  - `pub const BODY_HTML_V1_PREFIX: &str = "TUIDER_HTML_V1\n";`
  - 模块文档：payload 为 `css + "\n\u{1e}\n" + html`；host 负责 CSS 子集 → ratatui Lines。
- host `loader`、`tuider-plugin-dict`、`tuider-plugin-code` 改用该常量。
- **不** bump `TUIDER_PLUGIN_ABI`（值仍为 1）。旧 so 字面量与常量相同，无需重编。

### 2.6 `InputMode`

```rust
pub enum InputMode {
    Help,
    #[cfg(feature = "ai")]
    Ai,
    Nav,
    VimSearch,
    Visual,
    Normal,
}
```

`App::input_mode()` 派生优先级（与现 `keys.rs` if 链一致）：

1. `show_help` → `Help`
2. `ai.is_open()` → `Ai`（feature = ai）
3. `nav_open()` → `Nav`
4. `vim_mode` → `VimSearch`
5. `visual.is_some()` → `Visual`
6. else → `Normal`

`handle_key`：

- `Help`：任意键关 help（同现）
- 否则先 `handle_global`（quit / toggle 等，同现）
- 再 `match input_mode()` 分发到现有 handler

**不改** `ui` 的 getter 面与布局。

### 2.7 删除

- `src/cache.rs` 整文件 + `main` 中 `mod cache`
- `main` 内 `claim_by_id` / 旧 `missing_plugin_hint` 实现（改调 catalog）
- `pkg` 内平行 `CATALOG` 定义（改为 re-export / 使用 `plugin_catalog`）

### 2.8 空 Cargo features

保留 `url = []` 等兼容壳；文档标明已废、不链接代码。本轮不删。

---

## 3. 错误处理（冻结）

| 场景 | 行为 |
|------|------|
| flag 存在、无 so | stderr：`need plugin \`{id}\` — build and copy .so to:` + 目录路径；exit 2 |
| 插件 open 失败 | stderr：`plugin \`{id}\`: {e}`；exit 1 |
| 未知 core flag | stderr：`unknown flag`；exit 2（插件 flag 仍由 claims 吞掉） |
| load body 失败 | 正文 `error: …`，不崩 |
| 无 md/txt | `no .md / .txt under given path(s)`；exit 1 |

---

## 4. 测试

### 4.1 自动化

1. **`plugin_catalog` 真值表**  
   - 固定 args 切片，断言 `claims` / `missing_plugin_hint` 与历史语义一致（url/hn/code/dict 覆盖）。

2. **loader `body_render_tests`**  
   - 继续覆盖 HTML_V1 + CSS；改用 `BODY_HTML_V1_PREFIX` 后不退化。

3. **`InputMode` 派生**  
   - 表驱动：状态组合 → `input_mode()`（可构造最小 App 或抽纯函数 `derive_mode(...)` 便于测）。

### 4.2 手测清单（验收）

```fish
cargo test -q
cargo run -- -l README.md
TUIDER_PLUGINS_DIR=/tmp/empty-tuider cargo run -- -u https://example.com
# 若已 install url：
# cargo run -- -l -u https://example.com
```

---

## 5. 文档更新清单

| 文件 | 动作 |
|------|------|
| `docs/STATUS.md` | 权威快照：动态 so、catalog、无 host cache |
| `docs/PLAN.md` | D6/架构：已演化为动态 so；矛盾句删除或标历史 |
| `docs/complexity-review.md` | 按本轮 verdict 重写短页 |
| `docs/plugins.md` | 补 BODY_HTML_V1、catalog 说明 |
| `docs/NEXT.md` | 勾本轮；二期：HTML 下沉、View 快照、只信 handles |

---

## 6. 实施切片（建议提交粒度）

1. `plugin_catalog` + main/pkg 接线 + 单测  
2. 删 `cache.rs`  
3. API `PluginTextSource` + `BODY_HTML_V1_PREFIX`；loader/dict/code 改常量  
4. `InputMode` + keys 改 match  
5. 文档四件套 + NEXT  
6. 全量 `cargo test` + 手测

每步 `cargo check`；不跨步半拉子 API。

---

## 7. 风险与回滚

| 风险 | 缓解 |
|------|------|
| claims 漏分支 | 真值表单测；与旧函数逐分支对照 |
| Mode 改序抢键 | 只改结构不改条件；global 仍最前 |
| 常量替换 | 值相同，旧 so 兼容 |
| 多对话冲突 | 独占文件；结束提交 |

回滚：按切片 `git revert` 单提交即可。

---

## 8. 二期（本 spec 不实施）

- HTML/CSS 迁出 host 或可选 feature  
- `AppView` 只读快照  
- 去掉 claims、只信 `handles`  
- ABI 版本协商 / `--list-plugins` 增强  

---

## 9. 设计决策摘要

| 决策 | 选择 | 拒绝 |
|------|------|------|
| 范围 | P0 + 边界收口 | 全量渲染下沉 |
| 兼容 | 行为冻结 | 收紧仅 handles |
| App | Mode 枚举路由 | View 快照（二期） |
| 做法 | 外科清债（方案 A） | 微内核 / 渲染 so 总线 |

**Rejected alternative：** 为「纯净」重做成微内核 + 多 so 渲染链——改变积面与调试成本，违反 YAGNI 与本产品「终端阅读器」定位。
