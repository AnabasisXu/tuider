# Tuider Implementation Plan

> **For agentic workers:** implement in a **new OMP session opened in `~/cleantest/tuider`**.  
> Do **not** modify `~/cleantest/mdx-tui` unless the user explicitly starts a separate slimming task.  
> Spec: `docs/superpowers/specs/2026-07-21-tuider-reader-plugins-design.md`  
> Conversation summary: `docs/PLAN.md`

**Goal:** Working slim terminal reader for md/txt, with a plugin trait and feature-gated stubs/registration for dict/hn/url/ai.

**Architecture:** Single bin + `Plugin`/`ContentSource` traits + Cargo features (Approach A).

**Tech:** Rust 2024 edition (or match local toolchain), ratatui + crossterm, optional serde_yaml later.

---

## Phase 0 — Workspace skeleton

### Task 0.1: Cargo workspace + hello bin

- [ ] Create `Cargo.toml` workspace  
- [ ] Create `crates/tuider` (or root package) with `main` printing version/help  
- [ ] `cargo build` / `cargo run -- -V` works  
- [ ] Add MIT OR Apache-2.0 licenses if desired  

### Task 0.2: README already present

- [ ] Keep product README; link plan/spec  
- [ ] Optional: `.gitignore` for `/target`  

**Checkpoint:** empty product builds; no mdx-tui deps.

---

## Phase 1 — Core reader (default features)

### Task 1.1: Scan md/txt

- [ ] `scan_docs(root, recursive) -> Vec<(display_name, PathBuf)>`  
- [ ] Collision-safe display names (port idea from mdx-tui `markdown_display_names`)  
- [ ] Unit tests: empty dir, nested with/without `-r`, mixed extensions ignore `.mdx`  

### Task 1.2: Markdown + plain text body

- [ ] Port or reimplement minimal md → `Vec<Line>` (`tuider-md`)  
- [ ] txt → lines (preserve content; no HTML)  
- [ ] CLI `-l` / non-TTY print  

### Task 1.3: App shell + UI port

- [ ] Minimal `App` with sidebar list + content scroll + status  
- [ ] Port theme roles and layout breakpoints from mdx-tui **by copy into this repo**  
- [ ] Keys: quit, arrows, page, `?`, `/` search in content  
- [ ] **No** AI, dict panel, HN channels  

### Task 1.4: Default entry

- [ ] `tuider` → cwd scan  
- [ ] file/dir args  
- [ ] help text reader-first  

**Checkpoint:** daily-driver for reading local notes without network.

---

## Phase 2 — Plugin API (still no heavy plugins)

### Task 2.1: Traits

- [ ] `ContentSource` trait (list, select, body, status)  
- [ ] `Plugin` trait (cli contribution, try_open)  
- [ ] `PluginRegistry` built with `#[cfg(feature)]`  

### Task 2.2: Refactor core to FileTreeSource

- [ ] md/txt path implements `ContentSource`  
- [ ] `App` depends only on trait object or enum of core+plugins via trait  

### Task 2.3: Feature wiring

```toml
[features]
default = []
dict = []
hn = []
url = []
ai = []
full = ["dict", "hn", "url", "ai"]
```

- [ ] Help lists only enabled capabilities  
- [ ] Disabled flag → explicit error  

**Checkpoint:** `cargo tree --no-default-features` still slim; API ready.

---

## Phase 3 — Plugins one by one

Implement **in this order** unless user prioritizes otherwise:

### Task 3.1: `url` (smallest network path)

- [ ] Feature deps: reqwest, readability, cache dir under `tuider` config  
- [ ] `-u` / bare URL → markdown body → reader  
- [ ] Tests with fixtures (no live network in CI)  

### Task 3.2: `hn`

- [ ] Port client/format ideas from mdx-tui `hn/`  
- [ ] Titles as sidebar keys; enter loads comments/article as plugin state  
- [ ] Share HTTP/cache helpers with `url` if natural  

### Task 3.3: `dict`

- [ ] Port dictionary stack carefully; prefer extracting shared lib **later**  
- [ ] Until then: optional path/git dep on frozen mdx-tui crates **only under feature**  
- [ ] CLI word lookup + TUI browse  

### Task 3.4: `ai`

- [ ] Chat overlay; tools gated on available sources  
- [ ] No dict feature ⇒ no query_word tool  

**Checkpoint:** `full` feature set approximates former mdx-tui power modes without polluting default.

---

## Phase 4 — Packaging & docs

- [ ] Release profiles, optional `tuider-full` alias  
- [ ] Document feature matrix in README  
- [ ] Note mdx-tui freeze / future slim-or-retire  
- [ ] Optional: compatibility symlink story  

---

## Out of scope (do not sneak in)

- Editing or deleting modes inside `mdx-tui`  
- Script engine  
- Dynamic library plugins  
- Large CSS/HTML work in core  

---

## Suggested first commands in new OMP

```bash
cd ~/cleantest/tuider
# read docs/PLAN.md + specs + this plan
# then Phase 0
```

## Reference paths in frozen mdx-tui (read-only)

| Concern | Path |
|---------|------|
| Theme | `../mdx-tui/crates/mdx-tui/src/theme.rs` |
| Layout | `../mdx-tui/crates/mdx-tui/src/ui.rs` |
| App patterns | `../mdx-tui/crates/mdx-tui/src/app.rs` |
| CLI scan | `../mdx-tui/crates/mdx-tui/src/main.rs` |
| MD render | `../mdx-tui/crates/mdx-md/` |
| HN | `../mdx-tui/crates/mdx-tui/src/hn/` |
| Fetch URL | `../mdx-tui/crates/mdx-tui/src/fetch.rs` |
| Dict core | `../mdx-tui/crates/mdx-core/` |
| AI | `../mdx-tui/crates/mdx-ai/` |
