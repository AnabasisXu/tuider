# Host Boundary Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 按 `docs/superpowers/specs/2026-07-22-host-boundary-refactor-design.md` 完成 host 边界外科清债：单一 plugin catalog、删死 cache、双 trait 改名、HTML_V1 常量文档化、`InputMode` 路由、文档对齐；**对外 CLI/渲染行为冻结**。

**Architecture:** 新建 `src/plugin_catalog.rs` 作为 id/so/claims/missing-hint 唯一知识源；`pkg`/`main` 只消费它。`tuider-plugin-api` 将内部 Rust trait 改名为 `PluginTextSource` 并导出 `BODY_HTML_V1_PREFIX`（字符串值不变）。`App` 增加 `InputMode` 收敛 `handle_key` 分发，不改 ui 布局。

**Tech Stack:** Rust 2024 edition workspace、libloading 动态插件、ratatui TUI、`cargo test` / `cargo check`。

**Spec:** `docs/superpowers/specs/2026-07-22-host-boundary-refactor-design.md`  
**Baseline commit:** `406fb86`（initial import）

---

## File map

| 路径 | 动作 |
|------|------|
| `src/plugin_catalog.rs` | **Create** — `CatalogEntry` + `CATALOG` + `claims` + `missing_plugin_hint` + 真值表测试 |
| `src/main.rs` | **Modify** — `mod plugin_catalog`；删 `mod cache`；删本地 claim/missing；接线 catalog |
| `src/pkg.rs` | **Modify** — 删除本地 `CatalogEntry`/`CATALOG`/`find`；`use crate::plugin_catalog::{...}` |
| `src/cache.rs` | **Delete** |
| `crates/tuider-plugin-api/src/lib.rs` | **Modify** — `PluginTextSource`；删未用 `LoadResult`；`BODY_HTML_V1_PREFIX` + 文档 |
| `src/loader.rs` | **Modify** — `PluginTextSource`；用 API 常量替换本地 `HTML_V1` |
| `crates/tuider-plugin-dict/src/lib.rs` | **Modify** — 用 `BODY_HTML_V1_PREFIX` |
| `crates/tuider-plugin-code/src/lib.rs` | **Modify** — 用 `BODY_HTML_V1_PREFIX` |
| `src/app/mode.rs` | **Create** — `InputMode` + `derive_input_mode` / `App::input_mode` |
| `src/app/mod.rs` | **Modify** — `mod mode`；`pub use` 若需要 |
| `src/app/keys.rs` | **Modify** — `handle_key` 按 mode 结构分发（语义同序） |
| `docs/STATUS.md` | **Modify** — 权威快照 |
| `docs/PLAN.md` | **Modify** — D6/动态 so 对齐 |
| `docs/complexity-review.md` | **Rewrite** 短页 |
| `docs/plugins.md` | **Modify** — catalog + BODY_HTML_V1 |
| `docs/NEXT.md` | **Modify** — 勾本轮；二期项 |

**不改：** `src/ui.rs` 布局、`src/ai.rs` 业务、`src/md.rs` 算法、C ABI 符号名与 `TUIDER_PLUGIN_ABI = 1`。

---

### Task 1: `plugin_catalog` + main/pkg 接线

**Files:**
- Create: `src/plugin_catalog.rs`
- Modify: `src/main.rs`
- Modify: `src/pkg.rs`
- Test: `src/plugin_catalog.rs` 内 `#[cfg(test)]`；保留 `src/pkg.rs` 内 catalog 测试

- [ ] **Step 1: 新增 `src/plugin_catalog.rs`（实现 + 真值表测试）**

```rust
//! Single source of truth for shippable plugin ids, .so names, and host CLI claims.

/// One installable / claimable plugin.
#[derive(Clone, Copy)]
pub struct CatalogEntry {
    pub id: &'static str,
    pub crate_name: &'static str,
    pub so_name: &'static str,
    pub summary: &'static str,
    /// Host fallback when plugin `handles` is absent/false; also drives missing-so hints.
    pub claims: fn(args: &[String]) -> bool,
}

fn claims_url(args: &[String]) -> bool {
    args.iter().any(|a| {
        a == "-u" || a == "--url" || a.starts_with("http://") || a.starts_with("https://")
    })
}

fn claims_hn(args: &[String]) -> bool {
    args.iter().any(|a| a == "-hn" || a == "--hn")
}

fn claims_code(args: &[String]) -> bool {
    args.iter().any(|a| a == "--code")
}

fn claims_dict(args: &[String]) -> bool {
    args.iter().any(|a| a == "-g" || a == "--group")
        || args.iter().any(|a| a.ends_with(".mdx") || a.ends_with(".MDX"))
}

/// Static catalog (order = missing-hint priority: url → hn → code → dict).
pub const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "url",
        crate_name: "tuider-plugin-url",
        so_name: "libtuider_url.so",
        summary: "fetch URL → markdown",
        claims: claims_url,
    },
    CatalogEntry {
        id: "hn",
        crate_name: "tuider-plugin-hn",
        so_name: "libtuider_hn.so",
        summary: "Hacker News top stories",
        claims: claims_hn,
    },
    CatalogEntry {
        id: "code",
        crate_name: "tuider-plugin-code",
        so_name: "libtuider_code.so",
        summary: "source file tree",
        claims: claims_code,
    },
    CatalogEntry {
        id: "dict",
        crate_name: "tuider-plugin-dict",
        so_name: "libtuider_dict.so",
        summary: "MDX dictionary",
        claims: claims_dict,
    },
];

pub fn find(id: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.id == id)
}

pub fn claims(id: &str, args: &[String]) -> bool {
    find(id).is_some_and(|e| (e.claims)(args))
}

/// First catalog entry that claims `args` but is not loaded.
///
/// Historical note: old `missing_plugin_hint` used `starts_with("http")` for url.
/// Spec freezes **claims** table (`http://` / `https://`). Hints use the same
/// `claims` predicates so there is one knowledge source.
pub fn missing_plugin_hint(args: &[String], loaded: impl Fn(&str) -> bool) -> Option<&'static str> {
    for e in CATALOG {
        if (e.claims)(args) && !loaded(e.id) {
            return Some(e.id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| (*a).to_string()).collect()
    }

    #[test]
    fn claims_url_flags_and_urls() {
        assert!(claims("url", &s(&["-u", "https://x"])));
        assert!(claims("url", &s(&["--url", "http://x"])));
        assert!(claims("url", &s(&["https://example.com"])));
        assert!(claims("url", &s(&["http://example.com"])));
        assert!(!claims("url", &s(&["README.md"])));
        assert!(!claims("url", &s(&["http"]))); // no ://
    }

    #[test]
    fn claims_hn_code_dict() {
        assert!(claims("hn", &s(&["-hn"])));
        assert!(claims("hn", &s(&["--hn"])));
        assert!(!claims("hn", &s(&["-u"])));
        assert!(claims("code", &s(&["--code", "src"])));
        assert!(!claims("code", &s(&["-c"])));
        assert!(claims("dict", &s(&["-g", "en"])));
        assert!(claims("dict", &s(&["--group", "en"])));
        assert!(claims("dict", &s(&["foo.mdx"])));
        assert!(claims("dict", &s(&["FOO.MDX"])));
        assert!(!claims("dict", &s(&["foo.md"])));
    }

    #[test]
    fn missing_hint_order_and_loaded() {
        let none = |_: &str| false;
        assert_eq!(
            missing_plugin_hint(&s(&["-u", "https://x"]), none),
            Some("url")
        );
        assert_eq!(missing_plugin_hint(&s(&["-hn"]), none), Some("hn"));
        assert_eq!(missing_plugin_hint(&s(&["--code"]), none), Some("code"));
        assert_eq!(missing_plugin_hint(&s(&["a.mdx"]), none), Some("dict"));
        // url claims first when both present
        assert_eq!(
            missing_plugin_hint(&s(&["-hn", "-u"]), none),
            Some("url")
        );
        let has_url = |id: &str| id == "url";
        assert_eq!(
            missing_plugin_hint(&s(&["-u", "https://x"]), has_url),
            None
        );
        assert_eq!(missing_plugin_hint(&s(&["README.md"]), none), None);
    }

    #[test]
    fn catalog_ids_unique() {
        let mut ids: Vec<_> = CATALOG.iter().map(|e| e.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len());
    }
}
```

- [ ] **Step 2: 改 `src/main.rs`**

1. 在 `mod` 列表加入 `mod plugin_catalog;`（位置靠近 `mod pkg` / `mod plugin`）。
2. **本 Task 先不要删 `mod cache`**（Task 2）。
3. 删除函数 `claim_by_id` 与 `missing_plugin_hint` 整段。
4. 将调用改为：

```rust
if plug.handles_args(&raw) || plugin_catalog::claims(plug.id.as_str(), &raw) {
```

```rust
if let Some(need) = plugin_catalog::missing_plugin_hint(&raw, |id| registry.has(id)) {
```

- [ ] **Step 3: 改 `src/pkg.rs`**

1. 删除本地 `pub struct CatalogEntry { ... }`、`pub const CATALOG`、`pub fn find`（约 L9–47）。
2. 文件顶部增加：

```rust
use crate::plugin_catalog::{self, CatalogEntry, CATALOG};

pub use plugin_catalog::find;
```

（若不想 `pub use find`，则把 `find(` 全部改为 `plugin_catalog::find(`，并更新 tests。）

3. 保留 `so_path` / `install` / `list` 等逻辑；它们继续用 `CATALOG` 与 `CatalogEntry`。
4. `pkg` 内 `catalog_ids_unique` / `find_url` 测试仍可工作（`find` 与 `CATALOG` 来自 catalog）。

- [ ] **Step 4: 编译与测试**

```fish
cargo test -q plugin_catalog
cargo test -q pkg::
cargo check -q
```

Expected: 全部 PASS；无 unresolved `claim_by_id`。

- [ ] **Step 5: Commit**

```fish
git add src/plugin_catalog.rs src/main.rs src/pkg.rs
git commit -m "refactor: single plugin_catalog for claims and pkg"
```

---

### Task 2: 删除 `src/cache.rs`

**Files:**
- Delete: `src/cache.rs`
- Modify: `src/main.rs`（去掉 `mod cache;`）

- [ ] **Step 1: 确认无引用**

```fish
rg -n "cache::|crate::cache|mod cache" src crates
```

Expected: 仅 `src/main.rs` 的 `mod cache;`（及将删文件自身）。

- [ ] **Step 2: 删除模块**

1. 从 `src/main.rs` 删除 `mod cache;`。
2. 删除文件 `src/cache.rs`。

- [ ] **Step 3: 验证**

```fish
cargo test -q
```

Expected: PASS（原 `cache::tests` 消失，其余不变）。

- [ ] **Step 4: Commit**

```fish
git add -u src/cache.rs src/main.rs
git commit -m "refactor: remove unused host cache module"
```

---

### Task 3: API `PluginTextSource` + `BODY_HTML_V1_PREFIX`

**Files:**
- Modify: `crates/tuider-plugin-api/src/lib.rs`
- Modify: `src/loader.rs`
- Modify: `crates/tuider-plugin-dict/src/lib.rs`
- Modify: `crates/tuider-plugin-code/src/lib.rs`

- [ ] **Step 1: 改 `tuider-plugin-api`**

在 crate 文档 `# Contract` 增加 body 扩展说明，并加入常量与改名：

```rust
//! # Body text formats
//! - Default: UTF-8 markdown or plain text (host renders via md/plain).
//! - HTML envelope (optional): body starts with [`BODY_HTML_V1_PREFIX`], then
//!   `css`, then `"\n\u{1e}\n"`, then `html`. Host runs CSS subset → terminal lines.
//!   This is a **body payload** convention; it does not bump [`TUIDER_PLUGIN_ABI`].

/// Prefix for HTML+CSS body payloads (value stable; do not change without migration).
pub const BODY_HTML_V1_PREFIX: &str = "TUIDER_HTML_V1\n";

/// Text source after FFI adaptation (host loader only).
pub trait PluginTextSource: Send {
    fn title(&self) -> &str;
    fn entries(&self) -> &[String];
    /// Markdown / plain / HTML_V1 envelope.
    fn load_text(&mut self, index: usize, width: usize) -> Result<String, String>;
}
```

删除未使用的：

```rust
pub struct LoadResult {
    pub text: String,
    pub status: String,
}
```

（若存在。）删除旧名 `ContentSource` trait（已改名，勿保留 type alias，避免双名）。

- [ ] **Step 2: 改 `src/loader.rs`**

```rust
use tuider_plugin_api::{
    PluginTextSource, BODY_HTML_V1_PREFIX, FnAbiVersion, /* … rest unchanged */
    TUIDER_PLUGIN_ABI,
};

impl PluginTextSource for DynSource { /* same methods */ }

// remove: const HTML_V1: &str = ...
fn render_plugin_body(text: &str, width: usize) -> Vec<ratatui::text::Line<'static>> {
    if let Some(rest) = text.strip_prefix(BODY_HTML_V1_PREFIX) {
        // …
    }
    // …
}
```

测试里构造 body 时用 `format!("{BODY_HTML_V1_PREFIX}...")` 或字面量 `"TUIDER_HTML_V1\n..."`（值相同）。

- [ ] **Step 3: dict / code 插件改用常量**

`tuider-plugin-dict` / `tuider-plugin-code`：

```rust
use tuider_plugin_api::{
    args_vec, cstring_or_null, free_cstring, write_err, BODY_HTML_V1_PREFIX, TUIDER_PLUGIN_ABI,
};

// remove local const HTML_V1
// use BODY_HTML_V1_PREFIX everywhere former HTML_V1 was used
```

- [ ] **Step 4: 验证**

```fish
cargo test -q -p tuider-plugin-api
cargo test -q -p tuider-plugin-code
cargo test -q -p tuider-plugin-dict
cargo test -q
cargo check -q
```

Expected: PASS。旧已安装 `.so` 无需重编即可被 host 识别（前缀字符串未变）；**源码树内**插件测试用常量。

- [ ] **Step 5: Commit**

```fish
git add crates/tuider-plugin-api/src/lib.rs src/loader.rs \
  crates/tuider-plugin-dict/src/lib.rs crates/tuider-plugin-code/src/lib.rs
git commit -m "refactor: PluginTextSource and BODY_HTML_V1_PREFIX"
```

---

### Task 4: `InputMode` + `keys` 路由收敛

**Files:**
- Create: `src/app/mode.rs`
- Modify: `src/app/mod.rs`
- Modify: `src/app/keys.rs`

**行为约束：** 不得改变键位优先级。现序为：

1. `show_help` → 任意键关 help  
2. AI open → AI 独占（含 Ctrl+Q / Alt+L）  
3. `handle_global`（quit / `?` / Ctrl+F / Ctrl+S / Alt+L）  
4. `nav_open` → `handle_nav_key`  
5. `vim_mode` → `handle_vim_key`  
6. `visual` → `handle_visual_key`  
7. Normal 路径：启动 visual、`f`/`o`/Alt+f、`O`、`/`、`n`/`N`、sidebar filter、`handle_nav`

- [ ] **Step 1: 新增 `src/app/mode.rs`**

```rust
//! Input mode derived from App flags (readability for key routing).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Help,
    #[cfg(feature = "ai")]
    Ai,
    Nav,
    VimSearch,
    Visual,
    Normal,
}

/// Pure derivation for tests and `App::input_mode`.
pub fn derive_input_mode(
    show_help: bool,
    #[cfg(feature = "ai")] ai_open: bool,
    nav_open: bool,
    vim_mode: bool,
    visual: bool,
) -> InputMode {
    if show_help {
        return InputMode::Help;
    }
    #[cfg(feature = "ai")]
    if ai_open {
        return InputMode::Ai;
    }
    if nav_open {
        return InputMode::Nav;
    }
    if vim_mode {
        return InputMode::VimSearch;
    }
    if visual {
        return InputMode::Visual;
    }
    InputMode::Normal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_help_over_all() {
        assert_eq!(
            derive_input_mode(
                true,
                #[cfg(feature = "ai")]
                true,
                true,
                true,
                true
            ),
            InputMode::Help
        );
    }

    #[cfg(feature = "ai")]
    #[test]
    fn priority_ai_over_nav() {
        assert_eq!(
            derive_input_mode(false, true, true, true, true),
            InputMode::Ai
        );
    }

    #[test]
    fn priority_nav_vim_visual_normal() {
        assert_eq!(
            derive_input_mode(
                false,
                #[cfg(feature = "ai")]
                false,
                true,
                true,
                true
            ),
            InputMode::Nav
        );
        assert_eq!(
            derive_input_mode(
                false,
                #[cfg(feature = "ai")]
                false,
                false,
                true,
                true
            ),
            InputMode::VimSearch
        );
        assert_eq!(
            derive_input_mode(
                false,
                #[cfg(feature = "ai")]
                false,
                false,
                false,
                true
            ),
            InputMode::Visual
        );
        assert_eq!(
            derive_input_mode(
                false,
                #[cfg(feature = "ai")]
                false,
                false,
                false,
                false
            ),
            InputMode::Normal
        );
    }
}
```

- [ ] **Step 2: `src/app/mod.rs` 注册模块**

在 `mod keys;` 旁加入：

```rust
mod mode;
pub use mode::InputMode;
```

在 `impl App` 中增加：

```rust
pub(crate) fn input_mode(&self) -> mode::InputMode {
    mode::derive_input_mode(
        self.show_help,
        #[cfg(feature = "ai")]
        self.ai.is_open(),
        self.nav_open(),
        self.vim_mode,
        self.visual.is_some(),
    )
}
```

- [ ] **Step 3: 重写 `handle_key` 外壳（保留 handler 体）**

将 `keys.rs` 中 `handle_key` 改为等价结构（**handler 函数体原样剪切**，勿改匹配条件）：

```rust
pub(crate) fn handle_key(&mut self, key: KeyEvent) -> bool {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    // Help consumes any key (same as before).
    if self.input_mode() == super::InputMode::Help {
        self.show_help = false;
        return false;
    }

    #[cfg(feature = "ai")]
    if self.input_mode() == super::InputMode::Ai {
        // … existing AI block unchanged …
        return false;
    }

    if self.handle_global(key, ctrl, alt) {
        return true;
    }

    match self.input_mode() {
        super::InputMode::Help => unreachable!("handled above"),
        #[cfg(feature = "ai")]
        super::InputMode::Ai => unreachable!("handled above"),
        super::InputMode::Nav => {
            self.handle_nav_key(key);
            false
        }
        super::InputMode::VimSearch => self.handle_vim_key(key),
        super::InputMode::Visual => {
            let _ = self.handle_visual_key(key);
            false
        }
        super::InputMode::Normal => self.handle_normal_key(key, ctrl, alt, shift),
    }
}
```

将原 `handle_key` 中「启动 visual / f,o / vim enter / n,N / sidebar / handle_nav」整段移入新方法：

```rust
fn handle_normal_key(
    &mut self,
    key: KeyEvent,
    ctrl: bool,
    alt: bool,
    shift: bool,
) -> bool {
    // paste existing body from old handle_key after vim/visual branches
    // …
    false
}
```

**注意：** 旧代码在 visual 存在时先 `handle_visual_key`，未消费则继续往下；Mode 设计中 Visual 独占。旧逻辑：`if self.visual.is_some() { if self.handle_visual_key(key) { return false; } }` —— 未消费时 fallthrough 到启动 visual / f / filter。  
**行为冻结要求：** Visual 模式下未匹配键应 fallthrough 到与旧相同的后续逻辑，或确认 `handle_visual_key` 对未匹配返回 false 后旧代码会执行 sidebar/`handle_nav`。

**正确冻结实现：**

```rust
super::InputMode::Visual => {
    if self.handle_visual_key(key) {
        return false;
    }
    // fall through like before: do not start new visual; still allow nav/filter paths
    self.handle_normal_key(key, ctrl, alt, shift)
}
```

且 `handle_normal_key` 开头保留 `if self.visual.is_none()` 条件（旧代码已有）—— visual 存在时不会重新 `start_visual`。

- [ ] **Step 4: 验证**

```fish
cargo test -q app::mode
cargo test -q
cargo check -q
```

Expected: PASS。

- [ ] **Step 5: Commit**

```fish
git add src/app/mode.rs src/app/mod.rs src/app/keys.rs
git commit -m "refactor: InputMode for key routing"
```

---

### Task 5: 文档对齐

**Files:**
- Modify: `docs/STATUS.md`
- Modify: `docs/PLAN.md`
- Modify: `docs/complexity-review.md`
- Modify: `docs/plugins.md`
- Modify: `docs/NEXT.md`

- [ ] **Step 1: `STATUS.md` 重写权威快照（保留验证命令风格）**

要点：

- 动态 `.so` + `plugin_catalog` 单一知识源  
- 无 host `cache` 模块  
- `BODY_HTML_V1_PREFIX` 为 body 约定，ABI 仍为 1  
- `InputMode` 仅路由可读性  

- [ ] **Step 2: `PLAN.md`**

- D6 改为：已演化为**动态 cdylib**；历史「Cargo feature 链入」作废  
- 架构图改为 so + catalog  
- 「动态 so 不做」删除或标历史  

- [ ] **Step 3: `complexity-review.md` 整页替换为短页**

```markdown
# Tuider 复杂度审查（2026-07-22 host boundary）

## plugin_catalog
Verdict: leave
Evidence: claims + pkg + missing hint 单源；无第二份 match

## cache
Verdict: leave (deleted)
Evidence: host 死代码已删；插件自管磁盘缓存

## PluginTextSource vs ContentSource
Verdict: leave
Evidence: 文本源 / 渲染源命名分离

## HTML_V1
Verdict: leave (documented); deepen later
Evidence: 常量 + 文档；渲染仍在 host（二期）

## App InputMode
Verdict: leave
Evidence: 路由收敛；ui getter 面仍宽（View 快照二期）
```

- [ ] **Step 4: `plugins.md`**

- 补：`BODY_HTML_V1_PREFIX` 与 payload 格式  
- 补：host `plugin_catalog` 负责缺 so 提示与 claim 兜底  

- [ ] **Step 5: `NEXT.md`**

- 勾：catalog / 删 cache / PluginTextSource / HTML 文档化 / InputMode  
- 二期：HTML 下沉、AppView、只信 handles、ABI 协商  

- [ ] **Step 6: Commit**

```fish
git add docs/STATUS.md docs/PLAN.md docs/complexity-review.md docs/plugins.md docs/NEXT.md
git commit -m "docs: align STATUS/PLAN with dynamic plugins and catalog"
```

---

### Task 6: 全量验收

**Files:** 无新代码（除非修回归）

- [ ] **Step 1: 自动化**

```fish
cargo test -q
cargo check -q
```

Expected: 全绿。

- [ ] **Step 2: 手测（fish）**

```fish
cargo run -- -l README.md
TUIDER_PLUGINS_DIR=/tmp/empty-tuider cargo run -- -u https://example.com
```

Expected：

- 打印 README 内容或路径列表（`-l` 行为与前一致）  
- 空插件目录：`need plugin \`url\`` 与 plugins 路径  

可选（若本机已 install url so）：

```fish
cargo run -- -l -u https://example.com
```

- [ ] **Step 3: 成功标准核对（spec §1.3）**

- [ ] host 无第二份 id→flag match（仅 `plugin_catalog`）  
- [ ] 无 `src/cache.rs`  
- [ ] API 为 `PluginTextSource` + `BODY_HTML_V1_PREFIX`  
- [ ] `InputMode` 存在且测试绿  
- [ ] 文档无「非动态 so / 拒绝 dlopen」矛盾  

- [ ] **Step 4: 若有修复，单独 commit；否则无需空提交**

---

## Spec coverage checklist

| Spec 项 | Task |
|---------|------|
| 单一 catalog | 1 |
| 删 cache | 2 |
| PluginTextSource / 删 api LoadResult | 3 |
| BODY_HTML_V1 常量 + 文档 | 3 |
| InputMode 路由 | 4 |
| 文档 STATUS/PLAN/complexity/plugins/NEXT | 5 |
| 行为冻结验收 | 6 |
| 非目标（HTML 下沉、View、bump ABI） | 不实施 |

## Placeholder scan

无 TBD；claims 表与测试字面量完整；Mode fallthrough 在 Task 4 写明。

## Type consistency

- `CatalogEntry.claims: fn(&[String]) -> bool`  
- `missing_plugin_hint(args, loaded: impl Fn(&str) -> bool)`  
- `PluginTextSource::load_text`  
- `BODY_HTML_V1_PREFIX`  
- `InputMode` / `derive_input_mode`

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-07-22-host-boundary-refactor.md`.

**Two execution options:**

1. **Subagent-Driven（推荐）** — 每 Task 新 subagent，Task 间审查，快迭代  
2. **Inline Execution** — 本会话按 executing-plans 批量执行并设检查点  

**Which approach?**
