# 下一步目标

> 目录：`~/cleantest/tuider`

功能 parity 清单（mdx-tui 有而 Tuider 无）：[mdx-tui-unimplemented-gap.md](mdx-tui-unimplemented-gap.md)

## 已完成（方案 A + P0 + pkg + host-boundary + epub + nav）

- [x] 动态 `.so` ABI v1 + host `loader.rs`
- [x] url / hn / dict / **epub** 均导出 ABI；code 在 core
- [x] `scripts/install-plugins.sh` + `tuider pkg list|install|remove`（含 epub）
- [x] AI 429/5xx 自动换下一个 provider
- [x] dict：mdx-tui HTML+CSS→Lines（非 HTML→md 纯文本）
- [x] **plugin_catalog** 单一知识源（claims / missing hint / pkg；url→hn→dict→epub）
- [x] **删 host cache** 死模块
- [x] **PluginTextSource** + host `ContentSource` 命名分离
- [x] **BODY_HTML_V1_PREFIX** 文档化（ABI 仍为 1）
- [x] **InputMode** 键位路由收敛
- [x] 切片 E：R-01/02/03 html_render + HN/URL 差异文档（`plugins.md`）
- [x] app 键位拆分（`src/app/{keys,search,visual,nav}.rs`）
- [x] visual 字符级 `v` + 行级 `V`（caret 处进入）
- [x] vim 匹配计数 status + 当前命中高亮；`/` / consult ↑↓ 历史
- [x] `s` 视口 line-jump 多键标签；`zz` avy 连续子串跳转
- [x] URL/HN 拦 private/local IP

## P1

1. dict 大库索引 / code 高亮增强（code 在 core；索引仍在 dict 插件包内）
2. ABI 版本协商与 `tuider --list-plugins`
3. release strip / 体积

## 二期（host-boundary 之后）

- HTML/CSS 渲染下沉到插件 so 或可选 feature
- `AppView` 只读快照给 `ui::draw`
- 去掉 claims 兜底、**只信 handles**（需先审计插件等价）
- ABI 协商 / `--list-plugins` 增强

## 不做

- 把插件再链回主 bin
- 用 yml 假开关冒充模块分离（无 so 即无代码）
