# Tuider 复杂度审查

日期：2026-07-25  
范围：`src/` + `crates/`（只读审计，未改代码）  
视角：ponytail-audit + deep modules

## 规模（前 10）

| 行 | 路径 | 职责 |
|----|------|------|
| ~1480 | `src/ai.rs` | 会话 / SSE / 工具循环 / failover / 面板 |
| ~1295 | `crates/tuider-plugin-hn` | HN 缓存 / 评论 / article |
| ~1146 | `src/app/nav.rs` | Links/Toc/Consult/Corpus |
| ~1094 | `src/app/mod.rs` | App 状态 + ui getter |
| ~1087 | `crates/tuider-plugin-url` | 页 + RSS/Atom feed |
| ~996 | `src/md.rs` | md/txt → Lines |
| ~975 | `src/html_render.rs` | HTML_V1 → Lines |
| ~968 | `src/app/keys.rs` | 键路由 |
| ~956 | `src/loader.rs` | dlopen + HostSource |
| ~939 | `src/ui.rs` | 布局绘制 |

host ~12k 行；workspace ~16k 行。插件边界（无 so ⇒ 无 CLI）仍是最深的正确切分。

## ponytail-audit（可砍优先）

```
delete: chat_request 死路径. nothing. [src/ai.rs]
delete: load_wordlist + docs_root 与未读的 plugins.*.cache/limit/comments/default_group. 只留 enabled + list_wordlists. [src/config.rs]
delete: Cargo 空 feature url/hn/dict/code. 文档一句 so 加载. [Cargo.toml]
delete: md/html/loader 固定宽 dead wrappers (render_md, html_to_lines, render_plugin_body…). 调 *_doc / *_width. [src/md.rs, html_render.rs, loader.rs]
yagni: PluginTextSource 仅 DynSource 一实现 + HostSource 透传. DynSource 直接 ContentSource. [plugin-api, loader.rs]
yagni: ContentSource 塞 dict 五件套 + action（FileTree 全 no-op）. 可选 capability / 与 LoadedPlugin 符号对齐. [src/plugin.rs]
yagni: App→ui 一堆 1:1 getter. AppView 只读快照（NEXT 已列）. [src/app/mod.rs, ui.rs]
yagni: AI 九工具无条件挂载. 按 dict 会话 / 有 key 再注册. [src/ai.rs]
yagni: Theme 单变体 enum. unit struct 或常量色. [src/theme.rs]
yagni: AiConfig::from_env 与 config::env_provider 双份. 只信 ai_providers. [src/ai.rs, config.rs]
yagni: handles_args || catalog claims 双轨. 审计 handles 后只留一边（NEXT）. [main.rs, plugin_catalog.rs]
shrink: url‖hn 复制 http_get/SSRF/html→md/cache. 共享 internal lib（勿回 host）. [plugin-url, plugin-hn]
shrink: strip_html 多份. 一处 plain_from_html. [main, loader, md, plugins]
shrink: ui::centered_rect ‖ nav::centered. 共用. [ui.rs, nav.rs]
leave: md/html_render/html_css 手写渲染（终端约束，深度模块）.
leave: keys + InputMode 优先级（清晰可测）.
leave: visual d/a、epub、plugin_catalog/pkg 单源.
leave: native/stdlib 无大块可替.

net: 约 -900～-1600 行；host 运行时 deps 0 可砍；空 feature 可删。
```

## managing-complexity（摘要）

| 模块 | Verdict | Evidence | Change |
|------|---------|----------|--------|
| `ai.rs` | simplify-interface | 会话+HTTP+9 工具+绘制混一文件；死路径 | 删 dead；按能力挂 tools；合并 env |
| `loader` HostSource | merge | 透传层浅 | DynSource→ContentSource，渲染保持自由函数 |
| `ContentSource` | split / simplify | 宽接口 + 默认 no-op | 核心 load/entries + 可选能力 |
| App + ui | deepen later | ~30 字段 + getter 面 | AppView；布局策略表 |
| nav | leave / 小 shrink | 叠层共享状态尚深 | 合并 centered |
| keys / mode / visual | leave | 路由与选区内聚 | — |
| md + html_* | leave | 深模块；删 dead API 即可 | — |
| config | simplify | 解析细字段却只用 enabled | 缩 PluginsSection |
| url + hn | merge I/O | 平行抓取栈 | 共享 lib，不回 host |
| dict | leave | 业务在 so 正确；yml 搜索序与 host 分裂 | 对齐搜索序 |
| epub / catalog / pkg | leave | 边界干净 | — |

Rejected（全局）：为「拆文件」而拆 `ai.rs` 而不先裁工具面 → 接口成本不降。  
Rejected：网络抓取回 host → 破坏方案 A（无 so 即无代码）。

## 与 2026-07-22 审查对照

| 项 | 当时 | 现在 |
|----|------|------|
| plugin_catalog | leave | leave（仍单源） |
| host cache | leave (deleted) | 仍无 |
| PluginTextSource | leave | **merge 优先**（仅一实现） |
| HTML_V1 | leave / deepen later | leave（渲染仍 host） |
| InputMode | leave | leave |
| epub | leave | leave |
| 新增债 | — | AI 工具面、url/hn 重复 I/O、config 死字段、App getter |

## 不在本审查

正确性 / 安全 / 性能另开 review。STATUS 日期仍写 07-24，功能权威以 FEATURES 为准。
