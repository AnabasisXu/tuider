# Tuider Design: Core vs Plugins Roadmap (Revised)

**Date:** 2026-07-21  
**Status:** Approved for documentation; supersedes earlier “AI-as-plugin” product boundary  
**Product:** Tuider (terminal UI reader)

---

## 1. Goal

Ship a **terminal reader** whose product core is:

- Local **Markdown (`.md`)** and **plain text (`.txt`)** reading and rendering  
- **AI** co-reading / chat (product first-class; still a Cargo feature)  
- **Vim-style** in-content search (`/`, `n`, `N`)  
- **Visual selection + yank** (keyboard; OSC 52 preferred)  
- Shell: sidebar, scroll, theme, help  

**Plugins** (Cargo features; off ⇒ no CLI flags, no UI, no deps):

| Feature | Role |
|---------|------|
| `url` | Fetch URL → markdown → reader |
| `hn` | Hacker News top (and later comments) |
| `dict` | MDX dictionary (mdx-tui lineage) |
| `code` | Source-code reading (plain → syntect → optional tree-sitter later) |

### 1.1 Change from earlier planning

| Earlier (2026-07-21 first pass) | This revision |
|--------------------------------|---------------|
| AI is a plugin like dict/hn/url | AI is **core product**; compile-time still `feature = "ai"` |
| `default` empty or later `["ai","url"]` | **`default = ["ai"]` only** — `url` stays plugin-only |
| Code reading deferred entirely | **`code` plugin** on the roadmap (not core) |

### 1.2 Non-goals (near term)

- Dynamic `.so` / script-engine plugins  
- Editing `mdx-tui` in the same effort  
- tree-sitter in **core** or default binary  
- Mouse-first selection (terminal-native select remains available; app visual is keyboard)  
- PDF/DOCX/EPUB in default binary  

---

## 2. Release / features

```toml
[features]
default = ["ai"]
ai = []          # + HTTP/stream deps as implemented
url = []         # HTTP + readability; NOT in default
hn = []
dict = []
code = []
full = ["ai", "url", "hn", "dict", "code"]
```

| Build | Command (illustrative) | User gets |
|-------|------------------------|-----------|
| Daily | `cargo build` / `cargo install` | md/txt + AI |
| Slim reader | `--no-default-features` | md/txt only |
| Full | `--features full` | all plugins |

Unknown flags when feature off: clear error pointing at `--features <name>`.

---

## 3. Architecture

**Unchanged spine:** single binary + `ContentSource` / `Plugin` traits + Cargo features (Approach A).

```
tuider
├── core
│   ├── scan / FileTreeSource      # .md / .txt only
│   ├── md render                  # pulldown-cmark path (mdterm-inspired)
│   ├── app shell                  # layout, keys, theme (mdx-tui-inspired)
│   ├── vim search                 # core
│   ├── visual + yank              # core (OSC 52 first)
│   └── ai/                        # #[cfg(feature = "ai")] core module
├── plugin-api                     # ContentSource, Plugin, registry
└── plugins/
    url | hn | dict | code         # #[cfg(feature = "...")]
```

### 3.1 AI placement

- **Product:** always described as a core capability when the binary includes `ai`.  
- **Compile:** `feature = "ai"`; listed in `default`.  
- **Runtime without API key:** reading still works; AI entry shows configure hint (no hard lock).  
- **Not** registered as a `ContentSource` plugin for “open a source”; it is an overlay/session beside the current source.

### 3.2 Plugin placement

- Plugins may contribute CLI flags and/or a `ContentSource`.  
- `code` opens source files as a source (or extends scan when enabled)—exact open rules in implementation plan.  
- Core never links dict/mdict, HN, or readability unless features on.

### 3.3 Why this plugin style (not a “standard marketplace”)

- **Cargo features** for optional deps: industry-normal in Rust.  
- **Trait + static registry**: engineering boundary to avoid god-`App` (mdx-tui failure mode)—not a formal plugin standard like WebExtensions.  
- **Dynamic plugins**: deferred; higher cost, not required for v1.

---

## 4. Core behavior (product)

### 4.1 Local files

| Invocation | Behavior |
|------------|----------|
| `tuider` | Scan cwd one level `*.md` / `*.txt` |
| `tuider <file>` | Single file |
| `tuider <dir>` | One-level dir |
| `tuider -r [path]` | Recursive |
| `tuider -l` / non-TTY | Print body (single) or path list (multi) |

### 4.2 Vim search (core)

- `/` enter query; `n` / `N` next/prev; Esc clear/cancel as today.  
- Match count + current match highlight to be reinforced (R3).

### 4.3 Visual + yank (core)

- `v` / `V` visual (char/line as implemented).  
- `y` yank selection; prefer **OSC 52**; on failure, status message (no hard `xclip` requirement).  
- Do not capture mouse by default (preserve terminal native select).

### 4.4 AI (core module, default feature)

- TUI entry (e.g. Alt+L or as documented in help).  
- Streams against configured provider; tools may later gate on enabled plugins (e.g. dict lookup only if `dict`).  

---

## 5. Roadmap (Core-first)

| Phase | Goal | Acceptance |
|-------|------|------------|
| **R0** | Spec/README/PLAN match this doc | Docs consistent; no “AI is a plugin” in product one-liner |
| **R1** | `default = ["ai"]`; AI as core module path | Default build has AI; `--no-default-features` has no AI/url; help reader+AI first |
| **R2** | Visual + yank | Select + yank works in TUI; OSC 52 or clear failure |
| **R3** | Vim search harden | Count + current highlight; no clash with visual |
| **R4** | MD render polish | Keep pulldown path; no syntect required in core |
| **R5** | `url` plugin polish | Cache + errors; still `--features url` |
| **R6** | `hn` deepen | Comments/article optional |
| **R7** | `dict` | Port under feature only |
| **R8** | `code` plugin | plain → syntect → tree-sitter only if needed |

### 5.1 Mapping from current tree (approx.)

- Done-ish: md/txt shell, vim `/` basic, mdterm-style render base, `url`/`hn` feature stubs+impl, plugin traits.  
- Next implementation focus: **R0 docs (this)** → **R1 default/AI** → **R2 visual-yank**.

---

## 6. Dependency policy

| Build | Allowed (illustrative) |
|-------|-------------------------|
| `--no-default-features` | ratatui, crossterm, pulldown-cmark, unicode-width, … — **no** reqwest/AI/dict |
| `default` (+ai) | above + AI HTTP/stream stack |
| `url` / `hn` | reqwest (+ readability for url) |
| `dict` | mdict / HTML stack |
| `code` | optional syntect; tree-sitter only if phase requires |

`cargo tree --no-default-features` must stay free of plugin-only and (when AI not default-tested) free of AI stack for the slim matrix job.

---

## 7. Risks

| Risk | Mitigation |
|------|------------|
| Default binary heavier with AI | Accept; slim via `--no-default-features` |
| Users expect url in default | Docs: url is plugin; install note `--features url` or `full` |
| Yank environments differ | OSC 52 first; status on failure |
| Spec drift vs old design file | This file **owns product boundary**; older “AI plugin” sections marked superseded |

---

## 8. Success criteria

1. Product docs: core = reader + AI + vim + visual-yank; plugins = url/hn/dict/code.  
2. `default = ["ai"]` documented and eventually encoded in `Cargo.toml`.  
3. Slim build remains a first-class path.  
4. Implementation plans schedule R1–R2 before heavy dict/code.  

---

## 9. Self-review (2026-07-21)

- [x] No TBD without owner  
- [x] Consistent with user decisions: AI core+feature, no url in default, visual+yank, code as plugin  
- [x] Scope is product/roadmap boundary—not a full AI protocol spec  
- [x] Ambiguity “is url default?” resolved: **no**  
- [x] Plugin “standard?” answered: features+trait, not dynamic marketplace  

---

## 10. Related docs

| Doc | Role after this revision |
|-----|---------------------------|
| [PLAN.md](../../PLAN.md) | Decision summary; update D5–D7 / core-plugin tables |
| [2026-07-21-tuider-reader-plugins-design.md](2026-07-21-tuider-reader-plugins-design.md) | Historical architecture; **product boundary superseded by this file** where they conflict |
| [2026-07-21-tuider-implementation.md](../plans/2026-07-21-tuider-implementation.md) | Old phase plan; re-plan after R0 against R1+ |
