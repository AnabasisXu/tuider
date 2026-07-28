# Changelog

All notable changes to this project are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)-ish.  
Versions follow the workspace `Cargo.toml` (`0.1.0` until first tag).

## [Unreleased]

### Added
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