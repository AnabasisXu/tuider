# 下一步目标

> 目录：`~/cleantest/tuider`

## 已完成（方案 A + P0 + pkg + host-boundary）

- [x] 动态 `.so` ABI v1 + host `loader.rs`
- [x] url / hn / code / dict 均导出 ABI
- [x] `scripts/install-plugins.sh` + `tuider pkg list|install|remove`
- [x] AI 429/5xx 自动换下一个 provider
- [x] dict：mdx-tui HTML+CSS→Lines（非 HTML→md 纯文本）
- [x] **plugin_catalog** 单一知识源（claims / missing hint / pkg）
- [x] **删 host cache** 死模块
- [x] **PluginTextSource** + host `ContentSource` 命名分离
- [x] **BODY_HTML_V1_PREFIX** 文档化（ABI 仍为 1）
- [x] **InputMode** 键位路由收敛

## P1

1. [x] app 键位拆分（`src/app/{keys,search,visual}.rs`）  
2. [x] visual 字符级 `v` + 行级 `V`  
3. [x] vim 匹配计数 status + 当前命中高亮  
4. dict 大库索引 / code 高亮（在**插件包内**）  
5. ABI 版本协商与 `tuider --list-plugins`  
6. release strip / 体积

## 二期（host-boundary 之后）

- HTML/CSS 渲染下沉到插件 so 或可选 feature  
- `AppView` 只读快照给 `ui::draw`  
- 去掉 claims 兜底、**只信 handles**（需先审计四插件等价）  
- ABI 协商 / `--list-plugins` 增强  

## 不做

- 把插件再链回主 bin  
- 用 yml 假开关冒充模块分离（无 so 即无代码）
