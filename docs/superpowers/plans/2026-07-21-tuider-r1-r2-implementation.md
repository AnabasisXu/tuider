# Tuider R1–R2 Implementation Plan

> **For agentic workers:** Implement task-by-task. Checkboxes track progress.  
> Spec: `docs/superpowers/specs/2026-07-21-tuider-core-roadmap-design.md`

**Goal:** Encode `default = ["ai"]` with AI as a **core module** (not a plugin list item), then add **visual selection + yank** (OSC 52).

**Architecture:** Keep single bin + `ContentSource` plugins for url/hn/dict/code. AI lives under `src/ai.rs` behind `#[cfg(feature = "ai")]`, wired from App keys/help. Visual/yank is pure core UI state.

**Tech Stack:** Rust 2024, ratatui 0.30, crossterm 0.29. Yank via OSC 52 terminal sequence (no new dep for R2). Real AI HTTP providers can stay stubbed in R1 (status: configure).

---

## File map

| File | Role |
|------|------|
| `Cargo.toml` | `default = ["ai"]`; `full` includes code |
| `src/ai.rs` | Core AI module (cfg); config detect stub; overlay flag |
| `src/plugin.rs` | Registry **without** AI; plugins = url/hn/dict/code only |
| `src/app.rs` | Alt+L → AI overlay; `v`/`V`/`y` visual+yank; help labels |
| `src/ui.rs` | AI status chip; visual highlight; help text |
| `src/main.rs` | Help: CORE vs PLUGINS; default feature messaging |
| `README.md` | Already updated; verify after Cargo change |

---

### Task 1: Cargo default = ["ai"]

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Set features**

```toml
[features]
default = ["ai"]
dict = []
hn = ["dep:reqwest", "dep:serde", "dep:serde_json"]
url = ["dep:reqwest", "dep:readable-readability", "dep:url"]
ai = []
code = []
full = ["ai", "url", "hn", "dict", "code"]
```

- [ ] **Step 2: Verify trees**

```bash
cargo tree -p tuider --depth 1
cargo tree -p tuider --no-default-features --depth 1
```

Expected: default still slim (ai has no extra deps yet); no-default-features same reading deps.

- [ ] **Step 3: Tests**

```bash
cargo test -q
cargo test -q --no-default-features
```

Expected: pass.

---

### Task 2: Remove AI from plugin registry; add core AI module

**Files:**
- Create: `src/ai.rs`
- Modify: `src/plugin.rs`, `src/main.rs`, `src/app.rs`, `src/ui.rs`

- [ ] **Step 1: `src/ai.rs` (feature-gated)**

```rust
//! Core AI session (product core; feature = "ai").

#[derive(Debug, Clone, Default)]
pub struct AiSession {
    pub open: bool,
    /// True when API key / config present (R1: always false until config lands).
    pub configured: bool,
    pub status: String,
}

impl AiSession {
    pub fn new() -> Self {
        Self {
            open: false,
            configured: Self::detect_configured(),
            status: String::new(),
        }
    }

    fn detect_configured() -> bool {
        // R1 stub: env TUIDER_AI_KEY or OPENAI_API_KEY
        std::env::var("TUIDER_AI_KEY").is_ok() || std::env::var("OPENAI_API_KEY").is_ok()
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.status = if !self.open {
            String::new()
        } else if self.configured {
            "AI open (provider stub — chat not wired yet)".into()
        } else {
            "AI: set TUIDER_AI_KEY or OPENAI_API_KEY (reading still works)".into()
        };
    }

    pub fn focus_label(&self) -> Option<&'static str> {
        if self.open { Some("ai") } else { None }
    }
}
```

- [ ] **Step 2: `main.rs` — `mod ai` under cfg; help sections**

Help text pattern:

```
CORE:
    md/txt reader, vim /, visual+yank
    ai          (this build)   OR   ai (rebuild with --features ai)

PLUGINS IN THIS BUILD:
    url, hn, ...  OR  (none — --features url|hn|dict|code)
```

- [ ] **Step 3: `plugin.rs` — drop AI from registry**

`enabled_plugins()` only pushes dict/hn/url/code.  
`feature_enabled("ai")` may remain for diagnostics.  
`cfg(any(...))` lists **without** requiring ai for the registry module.

- [ ] **Step 4: App — hold `Option<AiSession>`**

```rust
#[cfg(feature = "ai")]
ai: crate::ai::AiSession,
```

On `Alt+l` / `Alt+L` (no shift full-screen yet): `ai.toggle()`; set status from `ai.status`.

Without feature: ignore or no binding.

- [ ] **Step 5: Tests**

```bash
cargo test -q
cargo test -q --no-default-features
cargo run -q -- -h   # lists AI under CORE when default
cargo run -q --no-default-features -- -h  # AI not in core compiled
```

- [ ] **Step 6: Commit** (if user wants git)  
`feat: R1 default ai as core module`

---

### Task 3: Visual selection + yank (R2)

**Files:**
- Modify: `src/app.rs`, `src/ui.rs`

- [ ] **Step 1: State**

```rust
enum VisualKind { Char, Line }
// fields:
visual: Option<VisualKind>,
visual_anchor: usize,  // line index in body
// selection end = scroll-aware caret line; R2 minimal: select line range by moving j/k while visual on
sel_start: usize,
sel_end: usize,
```

Minimal viable:
- `v` or `V` starts visual on current content line (`scroll` as caret line if content-focused; if sidebar, first enter content or use body line 0).
- `j`/`k`/arrows adjust `sel_end`.
- `Esc` clears visual.
- `y` yanks selected lines' plain text via OSC 52; clears visual; status `yanked N lines`.

- [ ] **Step 2: OSC 52 helper**

```rust
fn yank_osc52(text: &str) -> std::io::Result<()> {
    use std::io::Write;
    // base64 std encoding without extra crate: use a tiny manual encoder or `base64` dep
    ...
    let mut out = std::io::stdout();
    write!(out, "\x1b]52;c;{b64}\x07")?;
    out.flush()
}
```

Prefer adding `base64 = "0.22"` (small) over hand-rolled—acceptable for R2.

- [ ] **Step 3: Draw highlight**

In `draw_content`, for line indices in `sel_start..=sel_end` (normalized), apply bg amber/dim from theme (`search` family).

- [ ] **Step 4: Keys only when not typing filter / not vim mode**

- [ ] **Step 5: Test**

Manual TUI: open README, content focus, `V`, `j`, `y`, paste in another app.  
Unit: pure function `selected_plain(body, start, end) -> String`.

```bash
cargo test -q
```

- [ ] **Step 6: Commit**  
`feat: R2 visual selection and OSC52 yank`

---

### Task 4: Docs touch-up

- [ ] README status line: R1 done / R2 done as completed  
- [ ] Plan checkboxes updated  

---

## Spec coverage

| Spec item | Task |
|-----------|------|
| `default = ["ai"]` | T1 |
| AI core module not plugin | T2 |
| help reader+AI first | T2 |
| `--no-default-features` pure read | T1–T2 |
| visual + yank OSC52 | T3 |
| url not in default | T1 (unchanged) |

## Out of scope here

R3+ vim polish, url cache, hn comments, dict, code, real AI streaming.

---

## Execution

Default: **inline** in this session (user said 开始).
