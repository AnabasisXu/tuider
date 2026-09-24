# tuider 兼容 org-mode 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `tuider file.org` 可读 org-mode 文件：org 专用渲染（标题层级配色、TODO/优先级/标签/计划日期、表格 CJK 对齐、链接/标题索引）+ 通用大纲折叠 + 状态栏面包屑，复用现有 TUI 全部能力。

**Architecture:** 新增 core 渲染后端 `src/org.rs`（orgize → ratatui Line，产出 `RenderedDoc{lines,links,headings}`），挂进现有 `scan::is_doc` + `FileTreeSource::load` 分发链；`RenderedDoc` 从 md.rs 外移到 plugin.rs 共享；折叠实现为基于 `App.headings` 的行过滤纯函数（`z`/`Z`），对所有源（md/org/html 插件）生效。

**Tech Stack:** 现有栈（ratatui 0.30、crossterm 0.29、unicode-width 0.2）+ 新依赖 `orgize = "=0.10.0-alpha.10"`（orgcat 已验证）。

**Spec:** `/root/cleantest/tuider/docs/design-org-support.md`（本计划论证该设计；已确认方案 A、通用折叠、m3 syntect stretch）。

## Global Constraints

- 工作目录 `/root/cleantest/tuider/`，edition 2024，git 仓库（现有），每次任务提交
- 新依赖仅允许 `orgize = "=0.10.0-alpha.10"`（锁版本，同 orgcat）；不引入 async/其他新依赖
- `RenderedDoc`/`LinkEntry`/`HeadingEntry` 类型统一引用（Task 1 外移后全仓单一定义）；org.rs 不得反向依赖 md.rs 内部
- 渲染颜色走 `theme`/`MdTheme` 风格语义色；CJK 宽度一律 unicode-width
- 标题行号必须在渲染器内"产出时记录"（`lines.len()`），不得事后从语法树换算（wrap 会错位）
- 折叠是纯函数，不触碰 App 的 `body`/`headings` 原数据；`jump_section`（HN `[`/`]`）等既有 headings 消费方不得受影响
- 不写 TUI 交互自动化；验收用 `script` 伪终端 + 现有 `scripts/smoke-*.sh`

## File Structure

```
tuider/src/
├── org.rs          — 新建：orgize → RenderedDoc 渲染后端（Task 1/3）
├── plugin.rs       — 修改：RenderedDoc 移入（Task 1）
├── md.rs           — 修改：RenderedDoc 改 pub use 重导出（Task 1）
├── scan.rs         — 修改：is_doc 加 org（Task 2）
├── source.rs       — 修改：load 加 org 分支（Task 2）
├── app/mod.rs      — 修改：collapsed 字段、display_body、load 重置、跳转展开（Task 4/5）
├── app/fold.rs     — 新建：fold_visible 纯函数（Task 4）
├── app/keys.rs     — 修改：z/Z 键、面包屑状态刷新（Task 4）
└── (测试内联于各模块 #[cfg(test)]，沿袭现有风格)
```

## 现状关键行（改动锚点，已核实）

- `md.rs:90` `pub struct RenderedDoc { lines, links, headings }` —— 外移对象
- `plugin.rs` 顶部即 `LinkEntry`/`HeadingEntry` —— RenderedDoc 移到同处
- `md.rs:276` `heading_line = Some(self.lines.len())` + `:313` `headings.push(HeadingEntry{level,text,line})` —— org 渲染照抄的索引模式
- `app/mod.rs:74` `headings: Vec<HeadingEntry>`（load 时 `self.headings = result.headings`，`:670` 附近）—— 折叠状态并行数组
- `source.rs:load()` 三路分发（md / code / plain）—— 加 org 第四路

---

### Task 1: 共享类型外移 + org.rs 渲染骨架

**Files:**
- Modify: `src/md.rs`、`src/plugin.rs`
- Create: `src/org.rs`
- Test: `src/org.rs` 内 `#[cfg(test)]`

**Interfaces:**
- Consumes: `plugin::LinkEntry`、`plugin::HeadingEntry`（现状即 plugin.rs）
- Produces: `plugin::RenderedDoc`（外移）；`org::render_org_doc(text: &str, width: usize) -> RenderedDoc`；`org::render_org_width(text: &str, width: usize) -> Vec<Line<'static>>`
- 依赖：`Cargo.toml` 加 `orgize = "=0.10.0-alpha.10"`

- [ ] **Step 1: 写失败测试**（骨架：标题渲染 + 索引产出；org .rs 内）

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::{HeadingEntry};

    fn doc(src: &str) -> RenderedDoc { render_org_doc(src, 88) }

    #[test]
    fn heading_tree_indexed() {
        let d = doc("* 一级\nbody\n** 二级\n* 三级\n");
        assert_eq!(d.headings.len(), 3);
        assert_eq!(d.headings[0].level, 1);
        assert_eq!(d.headings[0].text, "一级");
        assert_eq!(d.headings[0].line, 0);
        assert_eq!(d.headings[2].line, 4); // 三级标题位于第 4 行
    }

    #[test]
    fn bold_italic_spans_styled() {
        let d = doc("* 标题 *粗* /斜/\n");
        let l0 = d.lines[0].to_string();
        assert!(l0.contains("粗") && l0.contains("斜"));
    }

    #[test]
    fn link_index_populated() {
        let d = doc("* t\n[[https://orgmode.org][Org]]\n");
        assert_eq!(d.links.len(), 1);
        assert_eq!(d.links[0].url, "https://orgmode.org");
        assert_eq!(d.links[0].text, "Org");
    }

    #[test]
    fn levels_distinguished_by_indent() {
        let d = doc("* 一\n** 二\n*** 三\n");
        assert!(d.lines[0].to_string().starts_with("* 一"));
        assert!(d.lines[1].to_string().starts_with("  ** 二"), "level2 应缩进 2 格");
        assert!(d.lines[2].to_string().starts_with("    *** 三"), "level3 应缩进 4 格");
    }
}
```

- [ ] **Step 2: 确认失败**：`cargo test org::` → fail（org.rs 不存在）
- [ ] **Step 3: 外移 RenderedDoc**：删除 `md.rs` 的 struct 定义，改为 `pub use crate::plugin::RenderedDoc;`；在 `plugin.rs`（LinkEntry/HeadingEntry 旁）加同构定义（字段不变：lines/links/headings）。全仓编译通过（loader.rs/source.rs 零改动即兼容）
- [ ] **Step 4: org.rs 最小实现**（本任务范围：标题行 + 正文 + 行内标记 + 链接/标题索引）

```rust
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use orgize::{Org, SyntaxElement, SyntaxKind};
use orgize::export::{Container, Event, TraversalContext, Traverser};
use crate::plugin::{HeadingEntry, LinkEntry, RenderedDoc};

const HEADING_COLORS: [Color; 6] = [
    Color::LightCyan, Color::LightBlue, Color::LightGreen,
    Color::LightYellow, Color::LightMagenta, Color::LightRed,
];

pub fn render_org_doc(text: &str, width: usize) -> RenderedDoc {
    let org = Org::parse(text);
    let mut r = Renderer::new(width.max(20));
    org.traverse(&mut r);
    r.finish_doc()
}
pub fn render_org_width(text: &str, width: usize) -> Vec<Line<'static>> {
    render_org_doc(text, width).lines
}
// Renderer: 事件遍历 → lines/links/headings（标题行号按 md.rs 模式：记录 lines.len()）
// 标题：`"  ".repeat(level-1)` 缩进（层级靠缩进区分）+ 星号×level dim + 层级色标题文字；TODO 等元信息 Task 3 补
```

（完整 Renderer 实现按 orgcat `src/main.rs` 的 `Outline`/`Full` 合并思路：Enter(Headline) 时记 `heading_line = Some(lines.len())`、渲染标题行、Leave 时 flush + push HeadingEntry；Text 事件按 span_depth 语义色出 Span；Link Leave 时 push LinkEntry{text: 描述, url: path, line}。）

- [ ] **Step 5: 测试通过**：`cargo test org::` → 3 条 PASS
- [ ] **Step 6: 提交**：`git add -A && git commit -m "task1: shared RenderedDoc + org.rs render skeleton"`

---

### Task 2: 挂载扫描与加载

**Files:**
- Modify: `src/scan.rs`、`src/source.rs`
- Test: scan.rs 既有 `#[cfg(test)]` 追加；source.rs 追加

**Interfaces:**
- Consumes: `org::render_org_doc`
- Produces: `scan::is_doc` 接受 `.org`；`FileTreeSource::load` org 分支
- `source.rs` 中 org 分支（放 md 分支之后、code 分支之前或之后皆可，与 md 并列）：

```rust
} else if ext.eq_ignore_ascii_case("org") {
    let doc = org::render_org_doc(&text, w);
    LoadResult {
        status: format!("{name}  ({} lines)", doc.lines.len()),
        lines: doc.lines,
        links: doc.links,
        headings: doc.headings,
    }
}
```

- [ ] **Step 1: 失败测试**

```rust
// scan.rs tests
#[test]
fn org_is_doc() {
    assert!(is_doc(std::path::Path::new("notes.org")));
}
// source.rs tests
#[test]
fn org_file_loads_with_headings() {
    let dir = std::env::temp_dir().join(format!("tuider-org-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.org"), "* 一级\n** 二级\n").unwrap();
    let mut s = FileTreeSource::new(vec![("a.org".into(), dir.join("a.org"))]);
    let r = s.load(0, 80);
    assert_eq!(r.headings.len(), 2);
    let _ = std::fs::remove_dir_all(dir);
}
```

- [ ] **Step 2: 确认失败**（`is_doc("notes.org")` false）
- [ ] **Step 3: 实现**：`scan.rs::is_doc` 扩展名白名单 + `"org"`；`source.rs::load` 加分支
- [ ] **Step 4: 测试通过**：`cargo test scan:: source::`
- [ ] **Step 5: 冒烟**：`cargo run -- /tmp/demo.org` 进入 TUI，`o` 大纲能看到 3 个标题，`q` 干净退出
- [ ] **Step 6: 提交**：`git commit -am "task2: mount .org into scan/load pipeline"`

---

### Task 3: org 元信息渲染补全

**Files:** Modify: `src/org.rs`（Renderer 事件细分）

**Interfaces:** 不变（`render_org_doc` 签名稳定）；补渲染分支

- [ ] **Step 1: 失败测试**（每个特性一条断言）

```rust
#[test]
fn todo_priority_tags_dates_styled() {
    let d = doc("* TODO [#A] 买牛奶 :errand:\n  DEADLINE: <2026-09-25 Fri>\n");
    let l0 = d.lines[0].to_string();
    assert!(l0.contains("TODO") && l0.contains("[#A]") && l0.contains(":errand:"));
    assert!(l0.contains("DEADLINE:"));
}
#[test]
fn filetags_line() {
    let d = doc("#+FILETAGS: :cml:\n* a\n");
    assert!(d.lines[0].to_string().contains("文件标签"));
}
#[test]
fn org_table_aligned_cjk() {
    let d = doc("* t\n| 物品 | 数量 |\n|------+------|\n| 苹果 | 3    |\n");
    let joined = d.lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n");
    assert!(joined.contains("| 苹果 | 3    |"));
}
#[test]
fn quote_and_src_blocks() {
    let d = doc("* t\n#+BEGIN_QUOTE\n引文\n#+END_QUOTE\n#+BEGIN_SRC rust\nfn main() {}\n#+END_SRC\n");
    let joined = d.lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n");
    assert!(joined.contains("> 引文"));
    assert!(joined.contains("```"));
}
```

- [ ] **Step 2: 确认失败**
- [ ] **Step 3: 实现**（全部逻辑 orgcat 已验证，逐项移植）：
  - Headline Enter：`"  ".repeat(level-1)` 缩进 + 星号×level dim + 对应 `HEADING_COLORS[level-1]` 标题文字 + `todo_keyword`（黄/绿/紫，milestone 语义色） + `priority`（A 红） + `tags`（青） + `scheduled/deadline/closed`（蓝，`SCHEDULED:` 前缀）
  - Keyword(FILETAGS)：文案 `(文件标签: a:b:)` dim 行；其余 `#+` 关键字 `ctx.skip()` 不渲染（防粘行，orgcat 已验证的坑）
  - OrgTable：orgcat `render_table` 移植（unicode-width 对齐、`|` dim、超长 `…` 截断）
  - QuoteBlock：`> ` dim 前缀（行续 `>  ` 逻辑同 orgcat Full）
  - SourceBlock：` ```lang ` 围栏 dim + 内容原样（syntect 不接入，stretch）
  - List/ListItem：bullet dim、indent 空格
  - Inline：bold/italic/strike/underline/code/verbatim/sup/sub 语义色 + `span_depth` 防覆盖（orgcat Full 已验）
- [ ] **Step 4: 测试通过**：`cargo test org::` → 全绿
- [ ] **Step 5: 冒烟**：`cargo run -- /root/cleantest/clean-Taskwarrior/20260917T101118--taskwarrior__cml.org` —— FILETAGS 行在顶、五级树、TODO 过滤语义先不管（过滤用现成 `/` 搜索）
- [ ] **Step 6: 提交**：`git commit -am "task3: org meta rendering (todo/priority/tags/dates/table/blocks)"`

---

### Task 4: 通用折叠 + 面包屑

**Files:**
- Create: `src/app/fold.rs`
- Modify: `src/app/mod.rs`、`src/app/keys.rs`、`src/app/nav.rs`（跳转展开，最小改）

**Interfaces:**
- Creates: `pub fn fold_visible(headings: &[HeadingEntry], collapsed: &[bool], line: usize) -> bool`；`pub fn heading_at(headings: &[HeadingEntry], line: usize) -> Option<usize>`；`pub fn unfold_ancestors(headings: &[HeadingEntry], collapsed: &mut [bool], line: usize)`
- Modifies: `App` 增 `collapsed: Vec<bool>`（`load()` 时 `vec![false; result.headings.len()]`）；`App::display_body(&self) -> Vec<Line<'static>>`（按 fold_visible 过滤 self.body）；`App::toggle_fold_at_caret()`；`App::cycle_fold()`；`App::current_heading_text() -> Option<String>`（面包屑）

- [ ] **Step 1: 失败测试**（fold.rs 纯函数）

```rust
fn hs() -> Vec<HeadingEntry> {
    vec![
        HeadingEntry { level: 1, text: "a".into(), line: 0 },
        HeadingEntry { level: 2, text: "a1".into(), line: 2 },
        HeadingEntry { level: 2, text: "a2".into(), line: 3 },
        HeadingEntry { level: 1, text: "b".into(), line: 5 },
    ]
}
#[test]
fn collapse_hides_subtree_lines() {
    let h = hs();
    let mut c = vec![false; h.len()];
    c[0] = true; // 折叠 a（1级）
    assert!(!fold_visible(&h, &c, 2)); // a1 行
    assert!(!fold_visible(&h, &c, 3)); // a2 行
    assert!(fold_visible(&h, &c, 5));  // b 仍可见
}
#[test]
fn toggle_expands_back() {
    let h = hs(); let mut c = vec![false; h.len()];
    c[0] = true;
    assert_eq!(heading_at(&h, 3), Some(1));
    unfold_ancestors(&h, &mut c, 3);
    assert!(fold_visible(&h, &c, 3));
}
```

- [ ] **Step 2: 确认失败**
- [ ] **Step 3: 实现**
  - `fold.rs`：`heading_at` 线性倒查（lines 有序，`iter().rev()` 取 `line <= target` 的第一个）；`fold_visible`：命中所属标题，沿祖先链（`level` 递减前缀，用 `heading_at` 循环或单调栈）查 `collapsed`，任一折叠即不可见；`unfold_ancestors` 逐级清 false
  - `app/mod.rs`：字段 + `load()` 重置；`display_body()` 过滤（body 与 headings.line 都是渲染行坐标，对齐前提成立——Task 1 起标题行号即渲染行号）
  - `app/keys.rs`：`handle_body_motion` 或正常模式分支加 `z`（`toggle_fold_at_caret`）、`Z`（`cycle_fold`：全收 level≥2 → 全展 → 循环）；跳到折叠区（`o`/`/` 跳转路径）先 `unfold_ancestors(caret_line)`
  - 面包屑：状态栏文本在现有 status 刷新点追加 `当前标题`（`current_heading_text` 取 `headings` 倒查）
- [ ] **Step 4: 测试通过**：`cargo test fold::` + 既有 App 测试不回归
- [ ] **Step 5: 冒烟**：`cargo run -- /root/cleantest/clean-Taskwarrior/20260917T101118--taskwarrior__cml.org`：`z` 折叠当前标题子树（行数减少）、`Z` 全收/全展、`o` 跳转自动展开、状态栏显示当前标题
- [ ] **Step 6: 提交**：`git commit -am "task4: generic outline folding + breadcrumb"`

---

### Task 5: 收尾与验收

**Files:** Modify: `src/app/keys.rs`（帮助文案）、`scripts/smoke-*.sh` 或新增 `scripts/smoke-org.sh`、`CHANGELOG.md`、`README.md`

- [ ] **Step 1: 写帮助文案测试**（键位表含 z/Z——按现有 help 测试结构）
- [ ] **Step 2: 确认失败**
- [ ] **Step 3: 实现**：帮助文案 + CHANGELOG 条目 + `scripts/smoke-org.sh`（仿 smoke-epub.sh：造临时 .org → tuider -l 列表打印断言非空）
- [ ] **Step 4: 静态检查**：`cargo fmt && cargo clippy --all-features -D warnings` 干净；`cargo test` 全绿
- [ ] **Step 5: 验收冒烟**（AGENTS.md 最小案例要求）：
  1. `cargo run -- /tmp/demo.org`：3 标题、`o` 跳转、`z` 折叠、`/` 搜索命中（伪终端 script + cat -v 确认无 ANSI 泄漏）
  2. `cargo run -- /root/cleantest/clean-Taskwarrior/20260917T101118--taskwarrior__cml.org`：FILETAGS 行、五级树折叠、面包屑
  3. 回归：`./scripts/smoke-core.sh`、`./scripts/smoke-epub.sh`（依赖 so 已装）全绿
  4. 非 TTY：`tuider demo.org </dev/null >/dev/null 2>&1` 退出码非 0 且无转义泄漏
- [ ] **Step 6: 提交 + 标记**：`git commit -am "task5: polish, smoke, docs"`；`cargo build --release`

---

## Self-Review

1. **Spec coverage**（design-org-support.md）：§2.1 渲染表→Task 1/3；§2.2 三个挂载点→Task 2（scan/source）+ Task 1（RenderedDoc 外移）；§2.3 折叠/面包屑/搜索交互→Task 4；§2.4 测试与验收→各任务 + Task 5；风险 3（行号渲染时记录）→Task 1 Step 4 明示；风险 4（跳转展开）→Task 4 Step 3。无 gap。
2. **Placeholder scan**：无 "TBD/implement later"；折叠测试代码全文给出，渲染器实现引用 orgcat 已验证代码作为移植源（路径明确：`/root/cleantest/orgcat/src/main.rs`）。唯一未内联的是 org.rs Renderer 全量代码——Task 1 Step 4 给出入口签名与索引模式，逐事件分支在 Task 3 Step 3 逐项列出，执行者可对应移植。
3. **Type consistency**：`plugin::RenderedDoc`（Task 1 外移）被 md.rs re-export、source.rs、org.rs 共用，全仓单一定义；`fold_visible/heading_at/unfold_ancestors` 签名跨 Task 4 内部一致；`App.collapsed` 与 `headings` 等长不变式在 load 重置点唯一维护。
4. **风险新增**：折叠使 `caret_line` 可指向隐藏行（scroll 超界）——Task 4 冒烟必须验证 `z` 后滚动 clamp；若发现 display_body 与 caret 映射错位，退路=折叠时把 caret 吸附到最近可见标题（记录为后备措施，不在本期预实现）。