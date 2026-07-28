# Tuider

Slim terminal reader: **md / txt / source code** in the binary; **URL · Hacker News · MDX dict · EPUB** only after you copy their `.so` into the plugins dir (scheme A — no plugin code linked into the bin).

## Quick start

```fish
cd ~/cleantest/tuider   # this repo

cargo build
./target/debug/tuider README.md
# fish alias: tdd README.md  (does not rebuild — run cargo build after code changes)

# plugins → ~/.local/share/tuider/plugins
./scripts/install-plugins.sh
# or: cargo run -- pkg install all

./target/debug/tuider https://example.com   # needs libtuider_url.so; bare URL; feed URL → sidebar
./target/debug/tuider -hn --sync          # fill HN cache
./target/debug/tuider -hn                 # cache-only by default
./target/debug/tuider book.epub           # needs libtuider_epub.so
./target/debug/tuider 'https://sachachua.com/blog/category/emacs-news/feed'
# RSS/Atom → sidebar with ~180-day entries; feed body keeps links/headings/lists
```

Missing `.so`:

```fish
TUIDER_PLUGINS_DIR=/tmp/empty cargo run -- https://example.com
# → need plugin `url` — copy .so to ...
```

## What is core vs plugin

| In `tuider` binary | Only as `libtuider_*.so` |
|--------------------|---------------------------|
| md / txt (`md::render`) | **url** — fetch page → markdown; **RSS/Atom feed** → multi-entry sidebar |
| **code** highlight (syntect → HTML_V1) | **hn** — top stories; `a` = article body |
| AI panel (default feature) | **dict** — MDX groups / CLI lookup |
| vim search · visual yank · outline/links · line-jump · avy | **epub** — spine chapters → markdown |

| Crate | Linux `.so` |
|-------|-------------|
| `tuider-plugin-url` | `libtuider_url.so` |
| `tuider-plugin-hn` | `libtuider_hn.so` |
| `tuider-plugin-dict` | `libtuider_dict.so` |
| `tuider-plugin-epub` | `libtuider_epub.so` |

Plugins dir: `~/.local/share/tuider/plugins`  
Override: `TUIDER_PLUGINS_DIR` or `plugins_dir:` in config.  
Catalog (id / claims / missing hint / pkg): `src/plugin_catalog.rs`. Host empty Cargo features `url`/`hn`/`dict`/`code` are **compat shells only** — they do not link plugins.

## Architecture

```text
┌──────────────────────────────────────────────────────────────────────┐
│  CLI  main.rs                                                        │
│  config · plugin_catalog · pkg · scan                                │
│       │ open: FileTree  |  registry.open (handles ‖ claims)          │
│       │ missing .so → need plugin `id` — copy .so …                  │
│       ▼                                                              │
│  ┌──────────────── App shell ─────────────────────────────────────┐  │
│  │  app/{mod,keys,mode,nav,search,visual}  ·  ui · theme          │  │
│  │  InputMode: Help > Ai > Nav > VimSearch > Visual > Normal      │  │
│  │  (+ ai.rs overlay when feature=ai)                             │  │
│  │           │ dyn ContentSource                                  │  │
│  └───────────┼────────────────────────────────────────────────────┘  │
│              ▼                                                       │
│  ┌─ FileTreeSource ─┐     ┌─ HostSource (loader) ─────────────────┐  │
│  │ source · md      │     │ DynSource ──dlopen──► plugins/*.so    │  │
│  │ code → HTML_V1   │     │ url | hn | dict | epub  (ABI v1)      │  │
│  └────────┬─────────┘     └──────────────────┬────────────────────┘  │
│           │                                  │ body text / HTML_V1   │
│           └──────────────┬───────────────────┘                       │
│                          ▼                                           │
│              loader::render_plugin_body_doc                          │
│              ├ HTML_V1 → html_css + html_render → Lines              │
│              ├ markdown → md::render                                 │
│              └ plain + outline / links                               │
└──────────────────────────────────────────────────────────────────────┘

crates/
  tuider-plugin-api     ABI + PluginTextSource + BODY_HTML_V1_PREFIX
  tuider-plugin-{url,hn,dict,epub}   → libtuider_*.so  (not linked into bin)
```

**Rule:** no `.so` in `plugins_dir` ⇒ no CLI surface for that plugin. yml `enabled` is only a second gate.

## Usage sketch

```text
tuider [OPTIONS] [PATH...] [WORD...]
tuider -g <group> <word>          # dict CLI (no TUI), needs dict .so
tuider -e book.epub               # or bare path ending in .epub
tuider pkg list|install|remove …
```

| Flag | Role |
|------|------|
| `-r` | recursive file scan |
| `-l` / `--print` | print list/body, no TUI |
| bare `http(s)://…` | open URL or feed (url plugin; feed → sidebar) |
| `-hn` / `--hn` | HN list; **`--sync`** to refresh network |
| `-g` / `--group` | dict group |
| `-e` / `--epub` | open EPUB (epub plugin) |
| `-n` | limit (dicts / HN stories) |
| `-h` / `-V` | help / version |

**TUI (essentials)** — `?` for full help:

| Key | Action |
|-----|--------|
| `Ctrl+F` | toggle sidebar |
| type | filter list (sidebar on; body single-key cmds off) |
| `Enter` | open / reload |
| `/` `n` `N` | vim search (↑↓ query history; current hit red) |
| `v` `V` `y` | char visual · line visual · yank (OSC 52) |
| visual `d` / `a` | selection → dict filter · AI context (input empty) |
| `s` | line-jump labels (viewport) |
| `zz` | avy char jump (type → labels) |
| `f` / `o` | links / outline (**sidebar off**) · click blue link copies URL |
| `Alt+f` / `Alt+Shift+f` | consult · corpus (search all entries) |
| `O` / `Alt+o` | open current file's directory |
| `a` | HN article action (sidebar off; not visual) |
| `Ctrl+B` / `Ctrl+Y` | dict panel · yank definition |
| `Alt+L` | AI panel · **Enter send · Ctrl+J newline** · `Alt+t` translate |

## Config

`~/.config/tuider.yml` (Windows: `%USERPROFILE%\.config\tuider.yml`). Auto-created on **TUI entry** if none found on the search path. Help (`-h` / `?`) shows `CONFIG:` path.

- `ai.providers` — multi-provider; **429/5xx auto-failover** (Tab still switches manually)
- `plugins_dir` (default `~/.local/share/tuider/plugins`; Windows `%USERPROFILE%\.local\share\tuider\plugins`)
- `plugins.<id>.enabled: false` — extra gate **after** `.so` is present (cannot replace “no file”)
- wordlists / groups for dict (plugin-side)

Env: `TUIDER_PLUGINS_DIR`, `TUIDER_AI_*` (see help / config).  
`tuider pkg install` needs a **source checkout**; without it the error shows the plugins dir for manual `.so`/`.dll` copy. Windows: run `tuider.exe` (not fish `tdd`); details in [FEATURES §8](docs/FEATURES.md).

## Docs

| Doc | Content |
|-----|---------|
| [docs/STATUS.md](docs/STATUS.md) | **Authoritative** load / ABI / package state |
| [docs/FEATURES.md](docs/FEATURES.md) | **Functional feature reference** |
| [docs/plugins.md](docs/plugins.md) | ABI, body HTML_V1, HN/URL cache & intentional diffs |
| [docs/NEXT.md](docs/NEXT.md) | Next steps |
| [docs/PLAN.md](docs/PLAN.md) | Design decisions |
| [docs/complexity-review.md](docs/complexity-review.md) | Complexity audit (2026-07-25) |

## License

Core + non-dict plugins: `MIT OR Apache-2.0` — see `LICENSE-MIT` and `LICENSE-APACHE`.

**dict plugin is AGPL-adjacent and opt-in:**

- `tuider-plugin-dict` depends on `mdx-tui-mdict` (**AGPL-3.0**).
- Default `./scripts/install-plugins.sh` installs **url / hn / epub only** (slim; no `libtuider_dict.so`).
- Slim tarball (CI / local): `./scripts/package-slim.sh` → `dist/tuider-*-slim.tar.gz` (host + url/hn/epub only).
- To build/install dict: `INCLUDE_DICT=1 ./scripts/install-plugins.sh` or `./scripts/install-plugins.sh debug with-dict`.
- Prebuilt / release tarballs **must not** ship `libtuider_dict.so` unless you also ship AGPL source offers and notices for that dependency.
- Users who need MDX lookup: build dict from this repo themselves.
