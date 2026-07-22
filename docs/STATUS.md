# Tuider 现状（方案 A）

目录：`~/cleantest/tuider`

## 加载机制

1. 主二进制**不链接** url/hn/dict/code 业务代码。
2. 启动扫描 `plugins_dir`（默认 `~/.local/share/tuider/plugins`）。
3. 只有目录里存在 `.so` 才 `dlopen`，才有对应 CLI。
4. yml `plugins.*.enabled: false` 可再挡一层；**不能代替「无文件」**。

已验证：

- 空目录 + `-u` / `--code` / `.mdx` → `need plugin … — copy .so`
- `libtuider_code.so` + `--code -l src/main.rs` → 列出 `main.rs`
- `libtuider_dict.so` + `-l some.mdx` → 词条列表
- `libtuider_url.so` + `-l -u https://example.com` → Example Domain
- help 列出 `LOADED PLUGINS`

## 包结构

```
src/           本体（md/txt/ai/loader）
crates/
  tuider-plugin-api/     ABI v1
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
```

## 已知

- AI：多 provider 时 429/5xx 自动轮换下一个（Tab 仍手动切换）
- dict：mdx-tui HTML+CSS 渲染（无 CSS 时用 mdx-tui 内建青/绿/黄 cascade）
- md/txt 仍走 mdterm 风格 `md::render`
- `tuider pkg list|install|remove`：本地 cargo 构建/拷贝/删除 .so
- dict 依赖 `mdx-tui-mdict`（AGPL）— 发行注意
