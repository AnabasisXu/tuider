# Tuider Design: Terminal Reader + Optional Plugins

**Date:** 2026-07-21  
**Status:** Partially superseded — **product boundary** now in [`2026-07-21-tuider-core-roadmap-design.md`](2026-07-21-tuider-core-roadmap-design.md) (AI is core; `default=["ai"]`; `url` is plugin-only). Architecture (traits/features) still useful.  
**Product:** Tuider (new project; not an in-place rewrite of mdx-tui)

---

## 1. Goal

Ship a **terminal reader** whose default binary only reads **`.md` and `.txt`**.  
Optional capabilities (dictionary, Hacker News, URL fetch, AI) are **plugins**: Cargo features + a plugin trait/registry. Default builds stay slim.

### 1.1 Why a new project

mdx-tui grew around MDict and accumulated reader/HN/URL/AI modes inside one `App`. Re-positioning in-tree forces a destructive cut and confuses identity.

**Decision:** freeze mdx-tui code for now; start **Tuider** clean. Reuse UI *ideas and modules by port*, not by continuing to bolt modes onto the dictionary app.

### 1.2 Non-goals (v1)

- Dynamic `.so` / script-engine plugins  
- Editing mdx-tui in the same effort  
- Pixel-perfect new visual language  
- Full MDX/CSS engine in core  
- PDF / DOCX / EPUB in **default** binary (see format comparison doc; EPUB may become an optional feature later)  
- MD → plain → justify pipeline (hygg-style); core MD follows mdterm-style structure render  


---

## 2. Product behavior

### 2.1 Default CLI

| Invocation | Behavior |
|------------|----------|
| `tuider` | Scan **cwd, non-recursive** for `*.md` and `*.txt`. Sidebar = file display names. Empty → status + help. |
| `tuider <file.md\|file.txt>` | Single-file reading mode. |
| `tuider <dir>` | Scan that directory one level. |
| `tuider -r [path]` | Recursive md/txt scan. |
| `tuider -l <file>` or non-TTY stdout | Print file text; multi-file non-TTY → **list paths** (not concatenated bodies). |
| `-h` / `-V` | Help / version. |

**Must not (default binary):** scan `.mdx`, open network, start AI, require config file.

### 2.2 Plugin CLIs (only if feature enabled)

| Feature | Flags / triggers | Behavior sketch |
|---------|------------------|-----------------|
| `dict` | `.mdx` paths, `-g`, word args, `--db`, `--html`, wordlists | Dictionary browse + CLI lookup (port from mdx-tui) |
| `hn` | `-hn` / `--hn`, `-n` limit | Top stories; titles as sidebar keys |
| `url` | `-u` / `--url`, bare `http(s)://` | Fetch → markdown → reader |
| `ai` | config `ai:` + TUI keys (e.g. Alt+L) | Chat overlay; dict tools only if `dict` also on |

Unknown flags when feature off: clear error (“rebuild with `--features hn`” or “not in this build”).

### 2.3 Release shapes

- **slim:** `default-features = false` → core only  
- **full:** all plugin features (optional package name `tuider-full` or CI matrix artifact)  
- **name compatibility:** primary binary `tuider`. `mdx-tui` as alias is a **later** packaging concern (old repo or symlink); not required for first green build.

### 2.4 Config

- Prefer `tuider.yml` / `tuider.yaml` / `.tuider.yml`  
- Search: cwd → parents → user config dir  
- Core keys: optional `docs_root`, theme — keep minimal  
- Plugin sections (`groups`, `ai`, …) ignored if feature off; parsed only when needed  

---

## 3. Architecture

### 3.1 Approach (chosen)

**Single binary + Plugin trait + Cargo features** (Approach A).

Rejected:

- **B multi-crate plugin packages first** — wide interface cost, premature package explosion  
- **C cfg-only gates without trait** — leaves god-`App`, fails “interface drawn as plugins”

### 3.2 Crate layout (target workspace)

```
tuider/                 # bin + feature wiring
tuider-core/            # App shell, events, sidebar model, plain text helpers
tuider-md/              # md → ratatui Lines (port of mdx-md)
tuider-plugin-api/      # Plugin + ContentSource traits (tiny)
# optional later / feature-gated members:
# tuider-dict, tuider-hn, …  OR modules under tuider/src/plugins/ with cfg
```

v1 may keep plugins as `tuider/src/plugins/{dict,hn,url,ai}.rs` behind features **if** they implement the same traits — crates can split when a plugin’s dependency tree hurts compile times.

### 3.3 Core abstractions

```text
ContentSource (trait)
  - title / status chrome
  - list entries (sidebar keys) + search
  - load body for selected key → Lines or plain
  - optional: on_enter, background jobs channel

Plugin (trait)
  - name, feature id
  - register_cli(Command or custom flag table)
  - try_open(cli_match) -> Option<Box<dyn ContentSource>>
  - optional: tui_keys, ai_tools

PluginRegistry
  - compile-time list via cfg + inventory-style push
  - main: parse global flags → ask plugins in order → else core md/txt opener
```

**App** owns: focus, scroll, input, layout rects, theme, help overlay, vim search.  
**App does not own:** HN fetch kinds, dict panel index, URL cache paths — those stay in plugin state objects behind the trait.

### 3.4 Managing complexity rules

- Deep modules: one `open_reader(path)` for core path; plugins hide network/index  
- Pull complexity down: partial failure (HN fetch error) → status string inside plugin, not 5 `App` enums  
- Define errors out: missing plugin feature → single “capability disabled” path  
- No empty pass-through layers  

### 3.5 Dependency policy

| Component | Allowed deps (illustrative) |
|-----------|-----------------------------|
| core | ratatui, crossterm, unicode-width, serde/yaml optional |
| tuider-md | ratatui |
| dict feature | mdict stack, html/css render, rusqlite if export kept |
| hn/url | reqwest (rustls), serde_json, readability (url) |
| ai | reqwest + stream/tokio as today |

Default build **must not** link plugin-only deps.

---

## 4. UI reuse from mdx-tui

Port principles, not the whole god-file:

1. **Theme roles** — search amber, selection band, quiet borders (`theme.rs`)  
2. **Layout breakpoints** — min 40×12, sidebar width 18/30, top headword rows (`ui.rs`)  
3. **Focus scrolling** — bare arrows = sidebar list when visible; Alt = content; vim `/` when sidebar hidden  
4. **Status + `?` help** — capability list reflects **compiled** plugins only  

AI split-pane and dict panel UI land only with their features.

---

## 5. Data flow (core)

```
argv → parse core flags → registry.try_plugins()
     → else scan md/txt → FileTreeSource
     → App::run(source)
     → event loop → draw(ui)
```

Plugin path injects a different `ContentSource`; draw path stays shared.

---

## 6. Testing strategy

- Unit: path scan, display name collision, utf8 truncate, md render fixtures  
- Feature matrix: `cargo test -p tuider --no-default-features` and `--features dict` etc.  
- Smoke: open a fixture `README.md` in TUI is manual; CLI `-l` automated  
- Do not require network in default tests  

---

## 7. Migration / relationship to mdx-tui

| Phase | mdx-tui | Tuider |
|-------|---------|--------|
| Now | **No code changes** | Spec + plan only |
| Implement reader | Still frozen or separate | Core md/txt |
| Plugins | Source of port for dict/HN/url/ai | Features land one by one |
| Later optional | Slim to dict-only **or** retire as app | Dict plugin may path/git depend on extracted libs |

User intent: mdx-tui will not remain the long-term “everything app”. Execution of slimming/removal is **out of scope for Tuider’s first OMP**.

---

## 8. Risks

| Risk | Mitigation |
|------|------------|
| Porting UI pulls dict assumptions | Extract layout against `ContentSource` mock first |
| Feature cfg spaghetti | All cfg at registry + plugin modules only |
| Name collision / user confusion | README states new product; mdx-tui freeze noted |
| Premature plugin crates | Start as modules; split crates when deps hurt |

---

## 9. Success criteria

1. Default binary: md/txt only; no mdict/reqwest/rusqlite/tokio-ai in `cargo tree -p tuider --no-default-features`  
2. Plugin enablement does not require editing `App` match forests for each mode  
3. Help text is reader-first  
4. Spec + plan sufficient for a cold OMP session in this directory  

---

## 10. Open items

- Exact crate names (`tuider-core` vs single crate) — implementer may start single-crate and split  
- Whether `url` shares HTTP client module with `hn`  
- License files and CI  
- When/if to vendor `mdx-md` vs path dependency  
- Format roadmap vs hygg/mdterm: [hygg-mdterm-format-comparison.md](../../hygg-mdterm-format-comparison.md)  

---

## Self-review (2026-07-21)

- [x] No TBD placeholders left without owner (open items listed)  
- [x] Consistent with PLAN.md decisions D1–D10  
- [x] Scope is one product core + plugin boundary; not multi-app monorepo rewrite  
- [x] Ambiguity on mdx-tui deletion resolved: **do not touch now**  
