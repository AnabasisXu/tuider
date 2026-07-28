# Featured Review Fixes (Phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver phase-1 featured-review fixes: O key, nav j/k input, AI Enter/C-j swap, search current red, config bootstrap + help path, drop `-u/--url`, URL error kinds, pkg hint, docs.

**Architecture:** Surgical edits in existing modules only. No new crates, no keymap framework. Spec: `docs/superpowers/specs/2026-07-25-featured-review-fixes-design.md`.

**Tech Stack:** Rust, ratatui, crossterm, reqwest (url plugin), serde_yaml.

**Constraints:**
- fish shell; after code changes `cargo build` (not only check) so `tdd` binary updates.
- Ponytail: shortest diff; `// ponytail:` only when deliberate shortcut.
- Skip formatters/full suite mid-task; final task runs `cargo test` + `cargo build`.
- 回复中文。

---

## File map

| File | Responsibility |
|------|----------------|
| `src/app/keys.rs` | `O` accepts SHIFT |
| `src/app/nav.rs` | Nav overlay j/k → query |
| `src/ai.rs` | Enter send, C-j newline + UI strings |
| `src/ui.rs` | Current search hit red; help shows config path |
| `src/config.rs` | `ensure_user_config` / `config_display_path` |
| `src/main.rs` | TUI ensure; help CONFIG; drop `-u` host skip |
| `src/plugin_catalog.rs` | drop url `-u/--url` claims |
| `crates/tuider-plugin-url/src/lib.rs` | bare URL only; error classification |
| `src/pkg.rs` | Cargo.toml missing hint |
| `docs/FEATURES.md`, `README.md`, `docs/plugins.md` | docs sync |

---

### Task 1: O key + Nav j/k + AI key swap

**Files:**
- Modify: `src/app/keys.rs` (~261–281)
- Modify: `src/app/nav.rs` (~244–260)
- Modify: `src/ai.rs` (~348–354, status/title strings with `C-j send`)

- [ ] **Step 1: Fix `O` to accept SHIFT**

In `keys.rs`, change the block that requires `modifiers == NONE` for command letters so `O` also fires with SHIFT (same pattern as `V`):

```rust
// Prefer: split O out, or widen the gate for Char('O'):
KeyCode::Char('O')
    if key.modifiers == KeyModifiers::NONE
        || key.modifiers == KeyModifiers::SHIFT =>
{
    self.open_current_dir();
    return false;
}
```

If `O` stays inside the `modifiers == NONE` match, move it out so SHIFT works. Leave lowercase `o` (TOC) as NONE-only.

- [ ] **Step 2: Nav j/k insert into query**

In `nav.rs` `handle_nav_key`, **delete** the dedicated `KeyCode::Char('k')` and `Char('j')` list-nav arms (lines ~244–260). Printable `j`/`k` then fall through to existing:

```rust
KeyCode::Char(c)
    if list_filter
        && (key.modifiers == KeyModifiers::NONE
            || key.modifiers == KeyModifiers::SHIFT) =>
```

Keep `Up`/`Down` as-is (Consult history / list).

- [ ] **Step 3: AI Enter send / Ctrl+j newline**

```rust
KeyCode::Char('j') if ctrl => self.insert_at_cursor('\n'),
KeyCode::Enter => self.send(),
```

Update every user-visible string that says `C-j send` / `Enter ↵` (status_line, input block title, etc.) to `Enter send · C-j ↵` (or equivalent short form).

- [ ] **Step 4: Build**

```fish
cargo build -q
```

Expected: success.

- [ ] **Step 5: Commit**

```fish
git add src/app/keys.rs src/app/nav.rs src/ai.rs
git commit -m "fix: O+SHIFT, nav j/k type, AI Enter send"
```

---

### Task 2: Search current match red + config bootstrap + help path

**Files:**
- Modify: `src/ui.rs` (`highlight_line` current style ~694–697; `draw_help_overlay` / in-app help)
- Modify: `src/config.rs`
- Modify: `src/main.rs` (`run_tui`, `print_help`)
- Modify: `src/app/mod.rs` if help needs path on App (optional: pass via `config::config_display_path()`)

- [ ] **Step 1: Current hit color**

In `ui.rs` `highlight_line`, change current match bg from peach to red:

```rust
let cur = Style::default()
    .fg(theme.status_focus_fg())
    .bg(Color::Rgb(220, 50, 47))
    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
```

Leave non-current hit on `theme.search_text()`.

- [ ] **Step 2: config helpers**

Add to `config.rs`:

```rust
/// Default user config path (~/.config/tuider.yml or XDG).
pub fn user_config_path() -> PathBuf {
    dirs_config().join("tuider.yml")
}

/// Path that would be / is loaded; for help display.
pub fn config_display_path() -> PathBuf {
    load()
        .map(|(p, _)| p)
        .unwrap_or_else(user_config_path)
}

/// If no config file exists on search path, write minimal template to user path.
/// Call only when entering TUI. Returns path used for display.
pub fn ensure_user_config() -> PathBuf {
    if let Some((p, _)) = load() {
        return p;
    }
    let path = user_config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    const TEMPLATE: &str = "\
# tuider.yml — auto-created; see `tuider -h` / docs/FEATURES.md
# plugins_dir: ~/.local/share/tuider/plugins
# plugins:
#   url: { enabled: true }
# ai:
#   providers: []
";
    if !path.is_file() {
        let _ = fs::write(&path, TEMPLATE);
    }
    path
}
```

Make `dirs_config` usable (already private — fine for same module).

- [ ] **Step 3: TUI entry + CLI help**

```rust
fn run_tui(source: Box<dyn ContentSource>) -> Result<(), ExitCode> {
    let _cfg_path = crate::config::ensure_user_config();
    // ... existing init
}
```

In `print_help`, add after PLUGINS DIR or similar:

```text
CONFIG:
    {config::config_display_path().display()}
```

In-app `?` help (`ui.rs` `draw_help_overlay` / long help lines): one line `config  <path>` via `config::config_display_path()`.

- [ ] **Step 4: Build + commit**

```fish
cargo build -q
git add src/ui.rs src/config.rs src/main.rs
git commit -m "feat: search current red, auto config, help path"
```

---

### Task 3: Drop `-u/--url` + URL errors + pkg hint

**Files:**
- Modify: `src/plugin_catalog.rs`
- Modify: `src/main.rs` (arg skip lists with `-u`/`--url`)
- Modify: `crates/tuider-plugin-url/src/lib.rs`
- Modify: `src/pkg.rs`
- Tests in those files

- [ ] **Step 1: catalog claims**

```rust
// url claims: only bare http(s)
args.iter().any(|a| a.starts_with("http://") || a.starts_with("https://"))
```

Update unit tests that assert `-u` / `--url` claims.

- [ ] **Step 2: main.rs host parsing**

Remove `"-u" | "--url"` from plugin-flag skip / consume lists so they are no longer treated as url flags (may become unknown path tokens — OK). Grep `main.rs` for `-u` and clean.

- [ ] **Step 3: url plugin**

`handles` / `open`: only bare URL; error text: `url plugin: need bare http(s) URL`.

`http_get` error mapping:

```rust
fn map_reqwest_err(e: reqwest::Error) -> String {
    let kind = if e.is_timeout() {
        "timeout"
    } else if e.is_connect() {
        "connect"
    } else if e.is_request() {
        "request"
    } else if e.is_status() {
        // prefer status code if available
        if let Some(s) = e.status() {
            return format!("http {s}");
        }
        "http"
    } else {
        "network"
    };
    format!("{kind}: {e}")
}
```

For response path, avoid only `error_for_status().map_err(to_string)`; map status explicitly:

```rust
let resp = client.get(...).send().map_err(map_reqwest_err)?;
let status = resp.status();
if !status.is_success() {
    return Err(format!("http {status}"));
}
resp.text().map_err(map_reqwest_err)
```

- [ ] **Step 4: pkg hint**

```rust
eprintln!("tuider pkg: workspace root missing Cargo.toml: {}", root.display());
eprintln!("hint: pkg install needs a source checkout; or copy prebuilt plugin into plugins_dir");
```

- [ ] **Step 5: Build plugins host tests**

```fish
cargo test -q -p tuider --lib
cargo build -q -p tuider-plugin-url
```

- [ ] **Step 6: Commit**

```fish
git add src/plugin_catalog.rs src/main.rs src/pkg.rs crates/tuider-plugin-url/src/lib.rs
git commit -m "fix: drop -u/--url, classify URL errors, pkg hint"
```

---

### Task 4: Docs + final verify

**Files:**
- Modify: `docs/FEATURES.md`
- Modify: `README.md` (url examples, windows if present)
- Modify: `docs/plugins.md` (url claim row)

- [ ] **Step 1: FEATURES.md**

- §2.3: url = 裸 `http(s)://` only; remove `-u/--url`
- §3: note auto-create on TUI entry; CONFIG in help
- §5.1: bare URL; error kinds one line
- §6.5: expand OSC 52 (sequence meaning, terminal dependency)
- §6.6: j/k type in filter; ↑↓ list/history
- §6.7: Enter send, C-j newline
- §8 Windows: tdd not for Windows; pkg needs source tree; config path `%USERPROFILE%\.config\tuider.yml` / XDG; plugins dir

- [ ] **Step 2: README + plugins.md**

Replace `-u URL` examples with bare URL. Claim table sync.

- [ ] **Step 3: Final gate**

```fish
cargo test -q
cargo build
```

Expected: green; `target/debug/tuider` updated.

- [ ] **Step 4: Commit**

```fish
git add docs/FEATURES.md README.md docs/plugins.md
git commit -m "docs: featured review phase-1 sync"
```

---

## Spec coverage checklist

| Spec § | Task |
|--------|------|
| 3.1 O SHIFT | T1 |
| 3.2 OSC52 docs | T4 |
| 3.3 AI keys | T1 |
| 3.4 Nav j/k | T1 |
| 3.5 Windows/pkg docs + hint | T3 + T4 |
| 3.6 config ensure + help | T2 |
| 3.7 search red | T2 |
| 3.8 drop -u | T3 + T4 |
| 3.9 URL errors | T3 |

## Out of plan

feed, bookmarks, selection→dict/AI, leader keymaps (spec §2).
