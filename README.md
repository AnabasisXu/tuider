# Tuider

轻量终端阅读器：同一界面里读文档、查 MDX 词典、看网页 / HN / EPUB，并可选 AI 对话。  
基于 Rust + Ratatui。**md / txt / 源码** 在主程序内；**URL · HN · 词典 · EPUB** 以动态 `.so` 插件加载（主程序不链插件业务代码）。

从 mdx-tui 演进而来：界面围绕**阅读**，词典是插件能力之一，而不是唯一中心。

## 特色

- 可配置词典群组与词表；TUI 多词典切换、CLI 查词 / 批量词 / 导出
- AI 对话（查词工具、翻译、导出记录；多 provider 可 failover）
- Markdown / txt / 源码阅读（vim `/`、visual yank、大纲与链接）
- Hacker News、网页与 RSS/Atom feed、EPUB 章节
- 插件按需安装；缺 `.so` 就没有对应入口

## 快速开始

### 从源码构建

```fish
git clone https://github.com/AnabasisXu/tuider.git
cd tuider
cargo build

# 安装插件（默认 url / hn / epub，不含 AGPL 的 dict）
./scripts/install-plugins.sh
# 需要词典：
# INCLUDE_DICT=1 ./scripts/install-plugins.sh
# 或：./scripts/install-plugins.sh debug with-dict

./target/debug/tuider README.md
```

### 使用示例

```fish
# 读文档（core，无需插件）
./target/debug/tuider README.md
./target/debug/tuider src/main.rs

# 词典 CLI（需 libtuider_dict.so）
./target/debug/tuider /path/to/dict.mdx hello
./target/debug/tuider /path/to/dicts/ "take,make"
./target/debug/tuider /path/to/dicts/ -l hello
./target/debug/tuider /path/to/dicts/ -n 2 hello
./target/debug/tuider -g english hello
./target/debug/tuider /path/to/dicts/ --html hello
./target/debug/tuider /path/to/dicts/ --db

# 词典 TUI
./target/debug/tuider /path/to/dicts/
./target/debug/tuider -g english -w gre -s make

# 网页 / feed（需 libtuider_url.so；仅裸 http(s) URL，无 -u）
./target/debug/tuider https://example.com
./target/debug/tuider 'https://example.com/feed.xml'

# Hacker News（需 libtuider_hn.so；默认只读缓存）
./target/debug/tuider -hn --sync    # 联网刷新
./target/debug/tuider -hn
./target/debug/tuider -hn -n 10
./target/debug/tuider -hn -l

# EPUB（需 libtuider_epub.so）
./target/debug/tuider book.epub
# 或：./target/debug/tuider -e book.epub
```

缺插件时：

```fish
TUIDER_PLUGINS_DIR=/tmp/empty ./target/debug/tuider https://example.com
# → need plugin `url` — copy .so …
```

### Windows

- 运行 `tuider.exe`
- 配置：`%USERPROFILE%\.config\tuider.yml`
- 插件：`%USERPROFILE%\.local\share\tuider\plugins\`
- 词典路径推荐正斜杠：`C:/dict/YourDict.mdx`

## 核心 vs 插件

| 主程序内 | 仅 `.so` |
|----------|----------|
| md / txt | **url** — 抓页 → markdown；RSS/Atom → 侧栏多条目 |
| 源码高亮 | **hn** — top 故事；键 `a` 抓外链全文 |
| AI 面板（默认 feature） | **dict** — MDX 群组 / CLI / corpus 全文 |
| `/` · visual · 大纲/链接 · line-jump · avy | **epub** — spine 章节 → markdown |

| crate | Linux 产物 |
|-------|------------|
| `tuider-plugin-url` | `libtuider_url.so` |
| `tuider-plugin-hn` | `libtuider_hn.so` |
| `tuider-plugin-dict` | `libtuider_dict.so` |
| `tuider-plugin-epub` | `libtuider_epub.so` |

插件目录默认 `~/.local/share/tuider/plugins`。  
覆盖：`TUIDER_PLUGINS_DIR` 或配置 `plugins_dir:`。  
规则：**没有 `.so` 就没有该 CLI/TUI 面**；yml 里 `enabled: false` 只是第二道门。

```fish
./scripts/install-plugins.sh              # debug → 用户插件目录
./scripts/install-plugins.sh release
INCLUDE_DICT=1 ./scripts/install-plugins.sh
# 或源码树内：
cargo run -- pkg list
cargo run -- pkg install url
cargo run -- pkg install all
```

## 配置文件

文件名：`tuider.yml` / `.tuider.yml`  
查找顺序：当前目录及父目录 → `~/.config/tuider.yml`（亦认 `XDG_CONFIG_HOME`；Windows 见上）。

**第一次进入 TUI** 且搜索路径上尚无配置时，会自动写一份最小模板到用户级路径。`-h` / `pkg` / 纯 CLI 查词 **不会**写文件。

```yaml
# 插件目录（可选）
# plugins_dir: ~/.local/share/tuider/plugins
# plugins:
#   url:
#     enabled: true

# 词典群组：-g <名称>
groups:
  english:
    - /path/to/YourEnglishDict.mdx
    - /path/to/AnotherDict.mdx

# 词表：-w <名称> 过滤词头
wordlists:
  gre: /path/to/gre-wordlist.txt

# AI
ai:
  api_key: sk-your-api-key          # 也可用 TUIDER_AI_* / 常见 OpenAI 环境变量
  base_url: https://api.openai.com/v1
  model: gpt-4o-mini

  # 或多 provider：429/5xx 自动 failover；对话里 /switch 手动切
  # providers:
  #   - name: openai
  #     base_url: https://api.openai.com/v1
  #     model: gpt-4o-mini
  #     api_key: sk-...
  #   - name: ollama
  #     base_url: http://localhost:11434/v1
  #     model: llama3
```

环境变量：`TUIDER_PLUGINS_DIR`、`TUIDER_AI_*`、`TUIDER_HN_FETCH_ARTICLE=1`（HN 打开时自动抓外链）等，见 `tuider -h`。

## CLI

```text
tuider [选项] [路径…] [词…]
tuider -g <群组> <词>
tuider pkg list|install|remove …
```

| 用法 | 说明 |
|------|------|
| `tuider <路径> <词>` | 有 dict 插件时：CLI 查词（不进 TUI） |
| `tuider <路径> "take,make"` | 逗号批量词 |
| `tuider <路径> -l <词>` | 精简输出（词头等） |
| `tuider <路径> -n N <词>` | 仅前 N 本词典 |
| `tuider <路径> --html <词>` | 原始 HTML 释义 |
| `tuider <路径> --db` | 每个 `.mdx` 导出旁路 `.db` |
| `tuider -g <组> <词>` | 配置中的词典群组 |
| `tuider -w <词表>` | 词表过滤 |
| `tuider -s <词\|a,b>` | TUI 初始跳转 / 临时词表 |
| `tuider -W` | 列出配置中的词表 |
| 裸 `http(s)://…` | 网页或 feed（url 插件） |
| `-hn` / `-hn --sync` | HN；默认缓存，`--sync` 联网 |
| `-e` / `*.epub` | EPUB |
| `-r` | 递归扫目录 |
| `-l` / `--print` | 不进 TUI，打印列表或正文 |

## TUI 快捷键

程序内 `?` 有完整帮助。下列为常用键。

### 全局

| 键 | 功能 |
|----|------|
| `Ctrl+Q` / `Ctrl+C` | 退出 |
| `?` | 帮助（任意键关闭） |
| `Ctrl+F` | 开关侧栏 |
| `Ctrl+S` | 侧栏布局 left ↔ top |
| `Ctrl+B` | 词典选择面板 |
| `Ctrl+Y` | 复制当前释义（OSC 52） |
| `Ctrl+U` | 清过滤，保留结果意图 |
| `Alt+L` | AI 面板 |
| `Alt+Shift+L` | AI 最大化 |
| `Tab` | 多词典 / 源之间循环 |

### 侧栏开（列表过滤）

- 可打印字符：过滤列表  
- `Enter` 打开；`↑↓` / Pg 移动列表；`Alt+↑↓` 滚正文  
- 侧栏开时，正文单键命令（`/` `f` `o` `s` `v` …）**禁用**

### 侧栏关（正文）

| 键 | 功能 |
|----|------|
| `/` `n` `N` | vim 搜索；当前命中红底；↑↓ 查询历史 |
| `v` / `V` / `y` | 字符 visual / 行 visual / yank |
| visual 内 `d` | 选区 → 词典过滤（仅 dict） |
| visual 内 `a` | 选区 → AI 上下文（不自动发送） |
| `s` | 视口行跳标签 |
| `zz` | avy：输入子串 → 标签跳转 |
| `f` / `o` | 链接列表 / 大纲（点蓝链复制 URL） |
| `Alt+f` | consult：当前正文多词过滤 |
| `Alt+Shift+f` | **corpus**：跨全部条目搜索；左右分栏选命中行；Enter 跳转 |
| `a` | 插件 action（HN 外链全文；非 visual） |
| `O` | 打开当前文件所在目录 |
| `h j k l` 等 | caret 运动；`gg` / `G` 等 |

### AI（`Alt+L`）

| 键 / 命令 | 功能 |
|-----------|------|
| `Enter` | 发送 |
| `Ctrl+J` | 输入换行 |
| `Alt+t` | 译当前全文 |
| `Esc` | 关闭 / 取消 |
| `/exp` · `/exp last` · `/exp N` | 导出对话（`tuider-chat-*.md`） |
| `/switch` · `/switch <名\|序号>` | 列出 / 切换 provider |

## 插件与许可

核心与 **url / hn / epub**：`MIT OR Apache-2.0`（`LICENSE-MIT`、`LICENSE-APACHE`）。

**dict 插件依赖 AGPL，默认不装：**

- 依赖 `mdx-tui-mdict`（AGPL-3.0）
- `./scripts/install-plugins.sh` 默认只装 url / hn / epub
- 精简包：`./scripts/package-slim.sh`（不含 dict）
- 需要词典：`INCLUDE_DICT=1 ./scripts/install-plugins.sh` 或自源码构建 dict
- 发行包**不要**附带 `libtuider_dict.so`，除非你同时履行 AGPL 义务

