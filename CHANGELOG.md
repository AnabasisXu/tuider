# Changelog

All notable changes to this project are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)-ish.  
Versions follow the workspace `Cargo.toml` (`0.1.0` until first tag).

## [Unreleased]

### Added
- org-mode 文件支持（core 渲染后端 `src/org.rs`）：标题层级缩进+6 级配色、TODO/优先级/标签/SCHEDULED/DEADLINE/CLOSED、表格 CJK 对齐+超长截断、代码块/引用块/列表、FILETAGS、链接/标题索引（`f`/`o` 复用）
- 通用大纲折叠：标题行 `z` 折叠子树、`Z` 全收(2 级及以上)/全展循环；md/org/html 源共用；状态栏当前标题面包屑
- `scripts/smoke-org.sh` — org 渲染管线非交互冒烟
- `LICENSE-MIT` + `LICENSE-APACHE` at repo root (matches `MIT OR Apache-2.0`)
- `.github/workflows/test.yml` — `cargo test --workspace`, `smoke-core`, `smoke-epub`
- `scripts/smoke-core.sh` — non-interactive core CLI smoke
- `scripts/package-slim.sh` — Linux slim tarball (host + url/hn/epub, **no** dict)
- `.github/workflows/release.yml` — tag `v*` → slim artifact on GitHub Releases
- AI `base_url` scheme validation (`http`/`https` only)
- URL/HN redirect SSRF re-check on every hop + final URL
- App TestBackend smoke: help overlay paints `Ctrl+Q` / `Quit`

### Changed
- `scripts/install-plugins.sh` defaults to **url / hn / epub** only; dict is opt-in (`INCLUDE_DICT=1` or second arg `with-dict`) because `mdx-tui-mdict` is AGPL
- README License section documents slim default and AGPL boundary
- Release profile: `strip = "symbols"`, `lto = "thin"`

### Security
- Block private/local redirect targets in url & hn fetch
- Reject non-http(s) AI provider base URLs from env and YAML