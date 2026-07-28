# Selection → dict / AI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In visual mode, `d` filters dict headwords with the selection, and `a` injects the selection into AI system context (empty input, no auto-send).

**Architecture:** Reuse existing visual plain-text helpers and App filter/AI paths. Extract `selection_plain` next to `yank_selection`. Dict path only when `source.list_dicts()` is non-empty. AI path stores `selection_context` on `AiSession` and appends a marked block when building the system prompt for send.

**Tech Stack:** Rust, ratatui Lines, existing `ContentSource` / `AiSession`, cargo test, fish shell for smoke.

**Spec:** `docs/superpowers/specs/2026-07-25-selection-dict-ai-design.md`

---

## File map

| File | Responsibility |
|------|----------------|
| `src/app/visual.rs` | `selection_plain`, `dict_from_selection`, `ai_from_selection`; yank may call helper |
| `src/app/keys.rs` | visual `d` / `a` bindings |
| `src/ai.rs` | `selection_context` field, setter, system prompt inject; test helper `build_system_prompt` or package-visible assemble |
| `docs/FEATURES.md` | §6.5 / §6.7 |
| tests in same modules | pure selection + AI system string + App mock for dict gate |

Constants (spec):

- dict filter text cap: **200** chars  
- AI selection inject cap: **4000** chars (`PREVIEW_CHARS` in `ai.rs` is already 4000 — reuse)

---

### Task 1: `selection_plain` helper + tests

**Files:**
- Modify: `src/app/visual.rs` (`selection_plain` on `App`, refactor `yank_selection` to use it)
- Test: `src/app/visual.rs` `#[cfg(test)]` module (extend existing tests at bottom)

- [ ] **Step 1: Write the failing test**

In `src/app/visual.rs` tests module, add:

```rust
#[test]
fn selection_plain_char_and_line() {
    let body = vec![
        Line::from("  hello  "),
        Line::from("world"),
    ];
    // Char: cols exclusive end on last line (same as selected_plain_char)
    let char_sel = VisualSel {
        kind: VisualKind::Char,
        a_line: 0,
        a_col: 2,
        b_line: 0,
        b_col: 7,
    };
    assert_eq!(selected_plain_char(&body, &char_sel), "hello");

    let line_sel = VisualSel {
        kind: VisualKind::Line,
        a_line: 0,
        a_col: 0,
        b_line: 1,
        b_col: 0,
    };
    let a = line_sel.a_line.min(line_sel.b_line);
    let b = line_sel.a_line.max(line_sel.b_line);
    assert_eq!(selected_plain(&body, a, b), "  hello  \nworld");
}

#[test]
fn selection_plain_cursor_is_none_logic() {
    // Document contract: Cursor kind must not produce action text.
    // Implemented via App::selection_plain → None; pure kind check here.
    assert_eq!(VisualKind::Cursor, VisualKind::Cursor);
}
```

Prefer a **pure** helper if App is heavy:

```rust
/// Returns trimmed selection text, or None for Cursor / empty after trim.
pub(crate) fn selection_plain_from(
    body: &[Line<'static>],
    v: VisualSel,
) -> Option<String> {
    let text = match v.kind {
        VisualKind::Cursor => return None,
        VisualKind::Line => {
            let a = v.a_line.min(v.b_line);
            let b = v.a_line.max(v.b_line);
            selected_plain(body, a, b)
        }
        VisualKind::Char => selected_plain_char(body, &v),
    };
    let t = text.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}
```

Test that:

```rust
#[test]
fn selection_plain_from_trims_and_skips_cursor() {
    let body = vec![Line::from("  ab  ")];
    let cursor = VisualSel {
        kind: VisualKind::Cursor,
        a_line: 0,
        a_col: 0,
        b_line: 0,
        b_col: 0,
    };
    assert!(selection_plain_from(&body, cursor).is_none());

    let char_sel = VisualSel {
        kind: VisualKind::Char,
        a_line: 0,
        a_col: 0,
        b_line: 0,
        b_col: 6,
    };
    assert_eq!(selection_plain_from(&body, char_sel).as_deref(), Some("ab"));

    let whitespace = VisualSel {
        kind: VisualKind::Char,
        a_line: 0,
        a_col: 0,
        b_line: 0,
        b_col: 2, // "  "
    };
    assert!(selection_plain_from(&body, whitespace).is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

```fish
cargo test -q --lib selection_plain_from_trims -- --nocapture
```

Expected: FAIL (function not found) or compile error.

- [ ] **Step 3: Implement `selection_plain_from` + thin App wrapper**

```rust
pub(crate) fn selection_plain_from(
    body: &[Line<'static>],
    v: VisualSel,
) -> Option<String> {
    let text = match v.kind {
        VisualKind::Cursor => return None,
        VisualKind::Line => {
            let a = v.a_line.min(v.b_line);
            let b = v.a_line.max(v.b_line);
            selected_plain(body, a, b)
        }
        VisualKind::Char => selected_plain_char(body, &v),
    };
    let t = text.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

impl App {
    pub(crate) fn selection_plain(&self) -> Option<String> {
        let v = self.visual?;
        selection_plain_from(&self.body, v)
    }
}
```

Refactor `yank_selection` to:

```rust
pub(crate) fn yank_selection(&mut self) {
    let Some(v) = self.visual else {
        return;
    };
    if v.kind == VisualKind::Cursor {
        self.status = "yank: no selection".into();
        return;
    }
    let Some(text) = selection_plain_from(&self.body, v) else {
        self.status = "yank: empty selection".into();
        return;
    };
    match yank_osc52(&text) {
        Ok(()) => {
            let n = text.lines().count();
            self.status = format!("yanked {n} line(s) via OSC 52");
            self.visual = None;
        }
        Err(e) => {
            self.status = format!("yank failed: {e}");
        }
    }
}
```

Note: after trim, multi-line selections keep internal newlines; only ends trimmed — correct.

- [ ] **Step 4: Run tests**

```fish
cargo test -q --lib selection_plain_from_trims
cargo test -q --lib char_selection_slice
```

Expected: PASS.

- [ ] **Step 5: Commit**

```fish
git add src/app/visual.rs
git commit -m "feat(visual): selection_plain helper for yank/dict/AI"
```

---

### Task 2: AI `selection_context` + system prompt

**Files:**
- Modify: `src/ai.rs` (struct field, `from_file_config` / `empty_session`, setter, send system string)
- Test: `src/ai.rs` tests

- [ ] **Step 1: Extract / add testable system builder**

Add near other AI helpers (before `send` or as method):

```rust
const SELECTION_CONTEXT_CHARS: usize = PREVIEW_CHARS; // 4000

fn append_selection_block(system: &mut String, selection: &str) {
    let sel: String = selection.chars().take(SELECTION_CONTEXT_CHARS).collect();
    if sel.is_empty() {
        return;
    }
    system.push_str("\n--- user selection ---\n");
    system.push_str(&sel);
    system.push_str("\n--- end selection ---");
}
```

In `send` (where `system` is built after preview/`--- end ---`), call:

```rust
if !self.selection_context.is_empty() {
    append_selection_block(&mut system, &self.selection_context);
}
```

Field:

```rust
pub struct AiSession {
    // ...existing...
    /// Visual `a` inject; empty = no block.
    selection_context: String,
}
```

Init to `String::new()` in `from_file_config` and `empty_session`.

Setter:

```rust
pub fn set_selection_context(&mut self, text: &str) {
    self.selection_context = text.chars().take(SELECTION_CONTEXT_CHARS).collect();
}
```

- [ ] **Step 2: Write failing tests**

```rust
#[test]
fn append_selection_block_formats() {
    let mut s = String::from("base");
    append_selection_block(&mut s, "hello");
    assert!(s.contains("--- user selection ---"));
    assert!(s.contains("hello"));
    assert!(s.contains("--- end selection ---"));
}

#[test]
fn set_selection_context_overwrites() {
    let mut sess = empty_session();
    sess.set_selection_context("one");
    sess.set_selection_context("two");
    assert_eq!(sess.selection_context, "two");
}

#[test]
fn append_selection_block_skips_empty() {
    let mut s = String::from("base");
    append_selection_block(&mut s, "");
    assert_eq!(s, "base");
}
```

- [ ] **Step 3: Run tests (expect fail until implemented)**

```fish
cargo test -q --lib append_selection_block -- --nocapture
cargo test -q --lib set_selection_context_overwrites -- --nocapture
```

- [ ] **Step 4: Implement field + wire into system build**

In the `send` path after:

```rust
system.push_str("\n--- end ---");
```

add the selection append. Do **not** clear `selection_context` on send (spec: overwrite only on next `a`).

Update every `AiSession { ... }` literal in tests (`empty_session`) with `selection_context: String::new()`.

- [ ] **Step 5: Run tests + build**

```fish
cargo test -q --lib append_selection_block
cargo test -q --lib set_selection_context
cargo test -q --lib enter_triggers_send_path
cargo build
```

Expected: PASS; `target/debug/tuider` refreshed.

- [ ] **Step 6: Commit**

```fish
git add src/ai.rs
git commit -m "feat(ai): selection_context in system prompt"
```

---

### Task 3: `dict_from_selection` + `ai_from_selection`

**Files:**
- Modify: `src/app/visual.rs`
- Test: `src/app/mod.rs` or `visual.rs` with mock `ContentSource`

- [ ] **Step 1: Mock source + failing App tests**

In `src/app/mod.rs` tests (or visual tests if App methods are `pub(crate)` visible):

```rust
struct MockDict {
    entries: Vec<String>,
    dicts: Vec<String>,
}

impl crate::plugin::ContentSource for MockDict {
    fn title(&self) -> &str {
        "mock-dict"
    }
    fn entries(&self) -> &[String] {
        &self.entries
    }
    fn load(&mut self, index: usize, _width: usize) -> crate::plugin::LoadResult {
        let name = self.entries.get(index).cloned().unwrap_or_default();
        crate::plugin::LoadResult::plain(
            vec![ratatui::text::Line::from(name.clone())],
            name,
        )
    }
    fn list_dicts(&self) -> Vec<String> {
        self.dicts.clone()
    }
}

struct MockPlain {
    entries: Vec<String>,
}

impl crate::plugin::ContentSource for MockPlain {
    fn title(&self) -> &str {
        "plain"
    }
    fn entries(&self) -> &[String] {
        &self.entries
    }
    fn load(&mut self, _i: usize, _w: usize) -> crate::plugin::LoadResult {
        crate::plugin::LoadResult::plain(vec![], "x".into())
    }
    // list_dicts default empty
}

#[test]
fn dict_from_selection_sets_filter() {
    let mut app = App::new(Box::new(MockDict {
        entries: vec!["apple".into(), "apricot".into(), "banana".into()],
        dicts: vec!["en".into()],
    }));
    app.body = vec![Line::from("xx apple yy")];
    app.visual = Some(VisualSel {
        kind: VisualKind::Char,
        a_line: 0,
        a_col: 3,
        b_line: 0,
        b_col: 8,
    });
    app.dict_from_selection();
    assert_eq!(app.filter, "apple");
    assert!(app.visual.is_none());
    assert!(app.filtered.iter().any(|&i| app.source.entries()[i].contains("apple")));
}

#[test]
fn dict_from_selection_rejects_non_dict() {
    let mut app = App::new(Box::new(MockPlain {
        entries: vec!["f".into()],
    }));
    app.body = vec![Line::from("word")];
    app.visual = Some(VisualSel {
        kind: VisualKind::Char,
        a_line: 0,
        a_col: 0,
        b_line: 0,
        b_col: 4,
    });
    app.dict_from_selection();
    assert!(app.visual.is_some());
    assert!(app.status.contains("not a dictionary"));
    assert!(app.filter.is_empty());
}
```

`#[cfg(feature = "ai")]` test for AI:

```rust
#[cfg(feature = "ai")]
#[test]
fn ai_from_selection_opens_and_sets_context() {
    let mut app = App::new(Box::new(MockPlain {
        entries: vec!["f".into()],
    }));
    app.body = vec![Line::from("selected text")];
    app.visual = Some(VisualSel {
        kind: VisualKind::Char,
        a_line: 0,
        a_col: 0,
        b_line: 0,
        b_col: 13,
    });
    app.ai.open = false;
    app.ai_from_selection();
    assert!(app.visual.is_none());
    assert!(app.ai.open);
    assert!(app.ai.input.is_empty());
    // selection_context is private — either pub(crate) getter for tests
    // or assert status contains "selection in context"
    assert!(app.status.contains("selection in context"));
}
```

If `selection_context` is private, add `#[cfg(test)] pub(crate) fn selection_context_for_test(&self) -> &str` **or** make field `pub(crate)`. Prefer `pub(crate)` field for simplicity in this crate.

- [ ] **Step 2: Run tests (expect fail)**

```fish
cargo test -q --lib dict_from_selection -- --nocapture
cargo test -q --lib ai_from_selection -- --nocapture
```

- [ ] **Step 3: Implement methods on `App` in `visual.rs`**

```rust
const DICT_FILTER_MAX: usize = 200;

impl App {
    pub(crate) fn dict_from_selection(&mut self) {
        if self.source.list_dicts().is_empty() {
            self.status = "dict: not a dictionary session".into();
            return;
        }
        let Some(text) = self.selection_plain() else {
            self.status = if self.visual.map(|v| v.kind) == Some(VisualKind::Cursor) {
                "dict: no selection".into()
            } else {
                "dict: empty selection".into()
            };
            return;
        };
        let text: String = text.chars().take(DICT_FILTER_MAX).collect();
        self.filter = text.clone();
        self.list_sel = 0;
        self.refilter();
        self.visual = None;
        if !self.status.contains("no matches") {
            self.status = format!("dict filter: {text}");
        }
    }

    #[cfg(feature = "ai")]
    pub(crate) fn ai_from_selection(&mut self) {
        let Some(text) = self.selection_plain() else {
            self.status = if self.visual.map(|v| v.kind) == Some(VisualKind::Cursor) {
                "AI: no selection".into()
            } else {
                "AI: empty selection".into()
            };
            return;
        };
        let n = text.chars().count();
        self.refresh_ai_context();
        self.ai.set_selection_context(&text);
        if !self.ai.open {
            self.ai.toggle();
        }
        self.visual = None;
        self.status = format!("AI: selection in context ({n} chars)");
    }

    #[cfg(not(feature = "ai"))]
    pub(crate) fn ai_from_selection(&mut self) {
        self.status = "AI: not in this build".into();
    }
}
```

Update visual status strings in `start_visual` / char convert paths:

```text
VISUAL — hjkl bw e · y · d dict · a AI · Esc
VISUAL LINE — jk gg G HML · y · d dict · a AI · Esc
```

- [ ] **Step 4: Run tests**

```fish
cargo test -q --lib dict_from_selection
cargo test -q --lib ai_from_selection
cargo build
```

Expected: PASS.

- [ ] **Step 5: Commit**

```fish
git add src/app/visual.rs src/app/mod.rs
git commit -m "feat(visual): dict/AI from selection"
```

---

### Task 4: Wire keys + docs

**Files:**
- Modify: `src/app/keys.rs` (`handle_visual_key` match arms near `y`)
- Modify: `docs/FEATURES.md` §6.5, §6.7
- Optional: `src/ui.rs` help lines if visual keys listed

- [ ] **Step 1: Bind keys**

In `handle_visual_key`, after `y` arm:

```rust
KeyCode::Char('d') if none => {
    self.dict_from_selection();
    true
}
KeyCode::Char('a') if none => {
    self.ai_from_selection();
    true
}
```

- [ ] **Step 2: FEATURES.md**

§6.5 Visual — add:

```markdown
- visual 内 **`d`**：选区 → dict 侧栏 filter（**仅 dict 会话**；非 dict status 并保留 visual）
- visual 内 **`a`**：选区写入 AI system 上下文并打开面板；**input 空**、不自动发送；成功后退出 visual
```

§6.7 AI — add:

```markdown
- visual **`a`**：选区注入 system（`--- user selection ---`）；与 `Alt+L` 共用面板
```

§6.4 table row for visual already has `y`; optionally add `d`/`a` in that table.

- [ ] **Step 3: Build + full lib tests**

```fish
cargo build
cargo test -q
```

Expected: green. `target/debug/tuider` ready for `tdd`.

- [ ] **Step 4: Commit**

```fish
git add src/app/keys.rs docs/FEATURES.md
git commit -m "feat: bind visual d/a; docs selection→dict/AI"
```

---

### Task 5: Manual smoke checklist (agent verifies what it can)

- [ ] **Step 1: Logic smoke without full TUI**

```fish
cargo test -q --lib dict_from_selection
cargo test -q --lib ai_from_selection
cargo test -q --lib selection_plain
cargo test -q --lib append_selection_block
```

- [ ] **Step 2: Binary exists**

```fish
test -x target/debug/tuider; and echo ok
```

- [ ] **Step 3: User handoff**

```text
tdd <dict.mdx 或 -g group>
# v 选词 → d → 侧栏 filter
# v 选段 → a → AI 开、input 空
# md 文件 → d → not a dictionary session
```

No extra commit unless fixes found.

---

## Spec coverage (self-check)

| Spec item | Task |
|-----------|------|
| `selection_plain` shared | Task 1 |
| `d` dict filter + list_dicts gate | Task 3–4 |
| `a` AI context + empty input | Task 2–4 |
| success clears visual | Task 3 |
| fail keeps visual | Task 3 tests |
| FEATURES | Task 4 |
| no feed/bookmarks/leader | not in plan |

## Placeholder scan

None intentionally left. If `LoadResult::plain` signature differs, open `src/plugin.rs` and match existing constructors used in `default_source_has_no_action` / epub tests.

## Type names

- `selection_plain_from` / `App::selection_plain`
- `App::dict_from_selection` / `App::ai_from_selection`
- `AiSession::selection_context` / `set_selection_context`
- `append_selection_block`
- `DICT_FILTER_MAX = 200`, `SELECTION_CONTEXT_CHARS = PREVIEW_CHARS`
