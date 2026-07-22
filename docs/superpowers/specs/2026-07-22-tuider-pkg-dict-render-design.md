# Tuider pkg 子命令 + dict 轻量 HTML 渲染

> 日期：2026-07-22  
> 状态：已批准（方案 A）  
> 范围：主机 `pkg list|install|remove`；dict 插件 HTML→近似 Markdown。

## 决策

| 项 | 选择 |
|----|------|
| 包来源 | 本地 workspace `cargo build -p …` + cp |
| Catalog | 主机内静态表（url/hn/code/dict） |
| dict 渲染 | 插件内 HTML→md；主机现有 mdterm 风格 `md::render` |
| 不做 | 远程包仓、CSS 引擎移植、`pkg update` |

## CLI

```
tuider pkg list
tuider pkg install <id|all>
tuider pkg remove  <id|all>
```

- `list`：catalog 每项 → installed / missing，附 yml enabled
- `install`：`cargo build -p <crate>`，复制 `target/debug|release/libtuider_*.so` → `plugins_dir`
- `remove`：删除 `plugins_dir` 下对应 so
- 工作目录：必须在含 workspace 的源码树（以 `CARGO_MANIFEST_DIR` 为根）

## Catalog

| id | crate | so (Linux) |
|----|-------|------------|
| url | tuider-plugin-url | libtuider_url.so |
| hn | tuider-plugin-hn | libtuider_hn.so |
| code | tuider-plugin-code | libtuider_code.so |
| dict | tuider-plugin-dict | libtuider_dict.so |

## dict HTML→md

映射：`h1–h3`、`p`/`br`、`ul/ol/li`、`b|strong`、`i|em`、`a[href]`、`code`/`pre`；其余标签剥离。输出 UTF-8 文本；主机 `looks_like_md` 走 `render_md_width`。

## 验收

```fish
cargo run -- pkg list
cargo run -- pkg remove url
cargo run -- pkg install url
cargo run -- -l -u https://example.com
# dict 词条可见标题/列表/加粗结构
```

## 文件

- `src/pkg.rs`（新）
- `src/main.rs`（pkg 入口 + help）
- `crates/tuider-plugin-dict/src/lib.rs`（html_to_md）
