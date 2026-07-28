# Tuider

终端阅读器：**md / txt / 源码** 编在主程序里；**网页 · HN · MDX 词典 · EPUB** 用动态 `.so` 插件（方案 A：主程序不链接插件业务代码）。

没有对应 `.so` → 没有该功能入口。

## 快速开始

```fish
cd ~/cleantest/tuider
cargo build
./target/debug/tuider README.md

# fish 别名 tdd 指向 debug 二进制，不会重新编译；改代码后必须 cargo build
# tdd README.md

# 装插件（默认 url / hn / epub，不含 AGPL 的 dict）
./scripts/install-plugins.sh
# 需要词典：INCLUDE_DICT=1 ./scripts/install-plugins.sh
# 或：cargo run -- pkg install all

./target/debug/tuider https://example.com          # 需 libtuider_url.so；裸 URL
./target/debug/tuider -hn --sync                   # 刷新 HN 缓存
./target/debug/tuider -hn                          # 默认只读缓存
./target/debug/tuider book.epub                    # 需 libtuider_epub.so
./target/debug/tuider /path/to/dict.mdx hello      # 需 libtuider_dict.so
```

缺插件时：

```fish
TUIDER_PLUGINS_DIR=/tmp/empty ./target/debug/tuider https://example.com
# → need plugin `url` — copy .so …
```

## 核心 vs 插件

| 主程序内 | 仅 `.so` |
|----------|----------|
| md / txt | **url** — 抓页 → markdown；**RSS/Atom** → 侧栏多条目 |
| 源码高亮（syntect） | **hn** — top 故事；`a` 抓外链全文 |
| AI 面板（默认 feature） | **dict** — MDX 群组 / CLI 查词 / corpus 全文 |
| `/` 搜索 · visual · 大纲/链接 · line-jump · avy | **epub** — spine 章节 → markdown |

| crate | Linux 产物 |
|-------|------------|
| `tuider-plugin-url` | `libtuider_url.so` |
| `tuider-plugin-hn` | `libtuider_hn.so` |
| `tuider-plugin-dict` | `libtuider_dict.so` |
| `tuider-plugin-epub` | `libtuider_epub.so` |

插件目录默认：`~/.local/share/tuider/plugins`  
覆盖：`TUIDER_PLUGINS_DIR` 或配置里 `plugins_dir:`。

## 用法

```text
tuider [选项] [路径…] [词…]
tuider -g <群组> <词>           # 词典 CLI 查词（不进 TUI）
tuider -e book.epub             # 或裸路径 *.epub
tuider pkg list|install|remove …
```

| 选项 | 作用 |
|------|------|
| `-r` | 递归扫目录 |
| `-l` / `--print` | 打印列表/正文，不进 TUI |
| 裸 `http(s)://…` | 打开网页或 feed（url 插件） |
| `-hn` | HN 列表；**`--sync`** 才联网刷新 |
| `-g` | 词典群组（`tuider.yml` 的 `groups`） |
| `-e` / `*.epub` | 打开 EPUB |
| `-s` / `-w` / `-n` | 跳词头 / 词表 / 限制数量 |
| `--html` / `--db` | 原始 HTML 释义 / 导出 sibling `.db` |
| `-h` / `-V` | 帮助 / 版本 |

### TUI 常用键（完整见 `?`）

| 键 | 作用 |
|----|------|
| `Ctrl+Q` | 退出 |
| `?` | 帮助（任意键关） |
| `Ctrl+F` | 开关侧栏 |
| 侧栏开 + 打字 | 过滤列表（此时单键正文命令禁用） |
| `Enter` | 打开 / 重载 |
| `/` `n` `N` | vim 搜索；当前命中红底；↑↓ 历史 |
| `v` `V` `y` | 字符/行 visual · yank（OSC 52） |
| visual 里 `d` / `a` | 选区 → 词典过滤 · AI 上下文 |
| `s` | 视口行跳标签 |
| `zz` | avy：输入子串 → 标签跳转 |
| `f` / `o` | 链接 / 大纲（点蓝链复制 URL） |
| `Alt+f` | consult：当前正文多词过滤 |
| `Alt+Shift+f` | **corpus**：跨全部条目搜索；左右分栏选行；Enter 跳转 |
| `O` | 打开当前文件目录 |
| `a` | HN 外链全文（非 visual） |
| `Ctrl+B` / `Ctrl+Y` | 词典面板 · 复制释义 |
| `Alt+L` | AI；**Enter 发送 · Ctrl+J 换行** · `Alt+t` 译全文 |

## 配置

`~/.config/tuider.yml`（Windows：`%USERPROFILE%\.config\tuider.yml`）。  
**第一次进 TUI** 且搜索路径上还没有配置时，会自动写一份最小模板。`-h` / `?` 会显示 `CONFIG:` 路径。

常用字段：

- `plugins_dir` — 插件目录  
- `plugins.<id>.enabled: false` — 有 so 之后的第二道开关（不能代替「没有文件」）  
- `groups` / `wordlists` — 词典群组与词表  
- `ai` / `ai.providers` — 多 provider；429/5xx 自动 failover  

环境变量：`TUIDER_PLUGINS_DIR`、`TUIDER_AI_*`（见 help）。

Windows：请直接跑 `tuider.exe`，不要用 Cygwin PATH 里的 `tdd`（容易撞到 Pandoc）。细节见 [FEATURES §8](docs/FEATURES.md)。

## 文档

| 文档 | 内容 |
|------|------|
| [docs/STATUS.md](docs/STATUS.md) | **权威**加载 / ABI / 包边界 |
| [docs/FEATURES.md](docs/FEATURES.md) | 已实现功能说明 |
| [docs/plugins.md](docs/plugins.md) | ABI、HTML_V1、HN/URL 缓存与有意差异 |
| [docs/NEXT.md](docs/NEXT.md) | 下一步 |
| [docs/PLAN.md](docs/PLAN.md) | 设计决策 |
| [docs/testing.md](docs/testing.md) | 测试注意点 |

## 许可

核心与非 dict 插件：`MIT OR Apache-2.0`（见 `LICENSE-MIT`、`LICENSE-APACHE`）。

**dict 插件依赖 AGPL，默认不装：**

- `tuider-plugin-dict` 依赖 `mdx-tui-mdict`（**AGPL-3.0**）
- `./scripts/install-plugins.sh` 默认只装 **url / hn / epub**
- 精简包：`./scripts/package-slim.sh` → 不含 dict
- 需要词典：`INCLUDE_DICT=1 ./scripts/install-plugins.sh` 或 `… debug with-dict`
- 发行 tar **不要**自带 `libtuider_dict.so`，除非你同时满足 AGPL 源码与声明义务
