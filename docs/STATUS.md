# Tuider 现状（权威快照）

目录：`~/cleantest/tuider`  
日期：2026-07-22（host-boundary 清债后）

## 加载机制

1. 主二进制**不链接** url/hn/dict/code 业务代码。
2. 启动扫描 `plugins_dir`（默认 `~/.local/share/tuider/plugins`）。
3. 目录内 `.so` 经 **`dlopen`**（`src/loader.rs`）加载；无文件则无对应 CLI。
4. **`src/plugin_catalog.rs`** 是发行侧插件**单一知识源**：id / so 名 / summary / `claims` / `missing_plugin_hint`；`pkg` 与 `main` 只消费它。
5. 认领顺序：`handles_args` **或** catalog `claims(id)`；缺 so 时 catalog 按 url→hn→code→dict 给 `need plugin …` 提示。
6. yml `plugins.*.enabled: false` 可再挡一层；**不能代替「无文件」**。
7. **无 host `cache` 模块**（已删）；网络/磁盘缓存由各插件自管。**HN 默认 cache-only**，`--sync` 才联网。

已验证：

- 空目录 + `-u` / `--code` / `.mdx` → `need plugin … — copy .so`
- `libtuider_code.so` + `--code -l src/main.rs` → 列出 `main.rs`
- `libtuider_dict.so` + `-l some.mdx` → 词条列表
- `libtuider_url.so` + `-l -u https://example.com` → Example Domain
- help 列出 `LOADED PLUGINS`
- `cargo test -q`：catalog / InputMode / loader body 绿

## Body / ABI

- C ABI 仍为 **`TUIDER_PLUGIN_ABI = 1`**（未 bump）。
- Body 可选信封：`BODY_HTML_V1_PREFIX`（`"TUIDER_HTML_V1\n"`）+ `css` + `"\n\u{1e}\n"` + `html`；host 做 CSS 子集 → Lines。
- API 侧文本源 trait：`PluginTextSource`；host App 侧仍为 `ContentSource`（已渲染 Lines）。

## App 输入

- `InputMode`（Help / Ai / Nav / VimSearch / Visual / Normal）仅收敛 `handle_key` 路由可读性；**不改** ui getter / 布局。

## 包结构

```
src/           本体（md/txt/ai/loader/plugin_catalog/…）
crates/
  tuider-plugin-api/     ABI v1 + PluginTextSource + BODY_HTML_V1_PREFIX
  tuider-plugin-url/     → libtuider_url.so
  tuider-plugin-hn/      → libtuider_hn.so
  tuider-plugin-code/    → libtuider_code.so
  tuider-plugin-dict/    → libtuider_dict.so
```

## 安装插件

```fish
./scripts/install-plugins.sh          # debug → ~/.local/share/tuider/plugins
./scripts/install-plugins.sh release  # release
# 或 TUIDER_PLUGINS_DIR=/path ./scripts/install-plugins.sh
# 或 cargo run -- pkg list|install|remove
```

## 已知

- AI：多 provider 时 429/5xx 自动轮换下一个（Tab 仍手动切换）
- dict：mdx-tui HTML+CSS 渲染（无 CSS 时用 mdx-tui 内建青/绿/黄 cascade）
- HTML 边角（2026-07-23）：`<img alt>` → `[alt]`；`<table>` 行分隔 + ` | ` 单元格；见 `html_render` 单测
- HN/URL：缓存路径与 TTL、HN `-l` 标题行（非 mdx-tui 表）、评论 cap 40、`a`=全文 — `docs/plugins.md`
- md/txt 仍走 mdterm 风格 `md::render`
- `tuider pkg list|install|remove`：本地 cargo 构建/拷贝/删除 .so（catalog 同源）
- dict 依赖 `mdx-tui-mdict`（AGPL）— 发行注意
- host 空 Cargo feature 名 `url`/`hn`/… 仅为兼容壳，**不**再链入插件依赖

## 权威顺序

加载与插件边界以**本文件**为准；历史规划见 [PLAN.md](PLAN.md)；下一步见 [NEXT.md](NEXT.md)。
