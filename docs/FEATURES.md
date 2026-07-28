# Tuider 功能说明

日期：2026-07-25  
权威边界见 [STATUS.md](STATUS.md)；插件 ABI/缓存细节见 [plugins.md](plugins.md)。  
本文描述**当前代码已实现**的用户可见功能（非路线图）。

---

## 1. 产品定位

终端阅读器：**md / txt / 源码**在主二进制内；**URL · Hacker News · MDX 词典 · EPUB** 以动态 `.so` 插件加载（方案 A）。  
无对应 `.so` ⇒ 无该 CLI 面。yml `plugins.<id>.enabled: false` 只是第二道门，不能代替「文件存在」。

---

## 2. CLI

### 2.1 调用形态

```text
tuider [OPTIONS] [PATH...] [WORD...]
tuider -g <group> <word>          # dict CLI 查词（无 TUI）
tuider -e book.epub               # 或裸路径 *.epub
tuider pkg list|install|remove …
tuider -h | -V
```

### 2.2 Host 自有选项

| 选项 | 作用 |
|------|------|
| `-r` / `--recursive` | 递归扫目录 |
| `-l` / `--list` / `--print` / `--lite` | 无 TUI：打印列表或正文 |
| `-h` / `--help` | 帮助（含 LOADED PLUGINS、plugins 目录） |
| `-V` / `--version` | 版本 + 构建时间 |
| `--html` | dict CLI：输出原始 HTML 释义 |
| `--db` | dict：每个 `.mdx` 导出旁路 `.db` 后退出（无 TUI） |
| `--sync` | 与 `-hn` 联用：联网刷新（默认 cache-only） |
| `--code` | 保留兼容；code 已在 core |

### 2.3 插件认领选项（需对应 `.so`）

| 选项 / 形态 | 插件 |
|-------------|------|
| 裸 `http(s)://…`（**无** `-u`/`--url`） | url |
| `-hn` / `--hn` | hn |
| `-g` / `--group` / 路径 `*.mdx` | dict |
| `-s` / `--search` / `-w` / `-n` / `--limit` / `-m` | dict 辅助（**`-s`  alone 不认领**） |
| `-e` / `--epub` / 路径 `*.epub` | epub |

认领：`handles_args` **或** `plugin_catalog::claims`。缺 so 时按 **url → hn → dict → epub** 给 `need plugin …` 提示。

### 2.4 默认打开（无插件认领时）

- 路径参数：文件 / 目录扫描 **md / txt / 常见源码扩展**（code 高亮在 core）。
- 无路径：扫 cwd 一层 md/txt（`-r` 递归）。
- `-l`：打印 entry 列表或选中正文，不进 TUI。

### 2.5 `tuider pkg`

源码树内本地装插件（`cargo build` + 拷贝 `.so`）：

| 子命令 | 作用 |
|--------|------|
| `list` / `ls` | 目录 + 已装 / 已加载 |
| `install` / `i` `<id\|all>` | 构建并安装 |
| `remove` / `rm` `<id\|all>` | 删除 so |
| `help` | 帮助 |

id：`url` · `hn` · `dict` · `epub`。  
`TUIDER_PKG_RELEASE=1` → release 产物。  
脚本等价：`./scripts/install-plugins.sh [debug|release]`。

---

## 3. 配置与环境

搜索顺序：`./tuider.yml` · `./.tuider.yml` · 向上父目录 · 用户级 `~/.config/tuider.yml`（Windows：`%USERPROFILE%\.config\tuider.yml`；亦认 `XDG_CONFIG_HOME/tuider.yml`）。

**进 TUI 时**（仅交互环前）：若搜索路径上**尚无任何**配置文件，则自动创建最小模板到用户级路径；cwd/父目录已有配置 → **不**创建、不覆盖。`-h` / `-V` / `pkg` / `-l` / dict 无 TUI 查词 **不**写文件。  
CLI `-h` 与应用内 `?` 显示 `CONFIG:` 路径（已加载或默认用户路径）。

| 键 / 环境 | 作用 |
|-----------|------|
| `plugins_dir` | 插件目录 |
| `plugins.<id>.enabled` | 有 so 后的额外开关（url/hn/dict；其它 id 默认允许） |
| `wordlists` / `groups` | dict 词表 / 群组路径 |
| `ai` / `ai.providers` | AI 单 provider 或列表（429/5xx 自动 failover） |
| `TUIDER_PLUGINS_DIR` | 覆盖 plugins 目录 |
| `TUIDER_AI_*` | AI 相关覆盖（见 help / config） |
| `TUIDER_HN_FETCH_ARTICLE=1` | HN open 时自动抓外链全文（非 list） |
| `TUIDER_PKG_RELEASE=1` | pkg install 用 release |

---

## 4. Core 阅读能力

| 能力 | 说明 |
|------|------|
| md / txt | `md::render`（mdterm 风格） |
| 源码 | `code.rs` + **syntect** → `TUIDER_HTML_V1` → host 渲染 |
| 侧栏列表 | 多 entry 默认开侧栏；`single_entry` 强制关 |
| 过滤 | 侧栏开时键入过滤列表 |
| outline / links | 正文内 TOC、链接跳转 |
| 状态栏 | 位置 / 状态 / `? help` / `C-q quit` |

### Body 渲染管线

1. 插件或 code 产出 UTF-8 文本  
2. 若以 `TUIDER_HTML_V1\n` 开头：`css + "\n\u{1e}\n" + html` → CSS 子集 + `html_render` → Lines  
3. 否则 markdown → `md::render`，或纯文本 + outline/links  

HTML 边角：`<img alt>` → `[alt]`；`<table>` 行分隔 + ` | ` 单元格。

---

## 5. 插件功能

安装目录默认 `~/.local/share/tuider/plugins/`（Windows：`%USERPROFILE%\.local\share\tuider\plugins\`；`TUIDER_PLUGINS_DIR` 覆盖）。

### 5.1 url — `libtuider_url.so`

- 打开：**仅**裸 `http://` / `https://` 参数（`-u`/`--url` **已移除**）  
- **Feed：** RSS/Atom → 侧栏多条目；正文把 content/description 转 markdown（**保留链接 / 列表 / 小标题**）；**不**为每条再抓文章页  
- **窗口：** 默认保留约 **180 天**内条目；WP 类短 feed 会翻 category `page/N` 补齐（归档 stub 仅标题+链接）  
- 缓存：页 `pages/<fnv>.md`；feed 首屏原始 `pages/<fnv>.feed`（命中仍会 expand）  
- 单页 HTML：readability → markdown  
- 网络：超时 10s 连接 / 30s 总；gzip；拦 localhost / private IP  
- 失败前缀：`timeout` / `connect` / `request` / `http {status}` / `network`  

### 5.2 hn — `libtuider_hn.so`

- 打开：`-hn`；**默认只读缓存**；`--sync` 才拉 top+items  
- `-n` / `--limit`：默认 30；`-l` 且未传 `-n` 时 15  
- 列表：TUI 标题行；`-l` 为 score/comments/title 行（非 markdown 表）  
- 正文：meta + 可选正文 + 评论；键 **`a`** → action `article` 抓外链全文  
- 评论 BFS **cap 20**  
- 缓存：`hn/topstories.json`（TTL 15m，仅 sync 新鲜度）、`hn/item/`、`hn/body/v2/`、外链同 url `pages/`  
- 外链同样 SSRF 护栏  

### 5.3 dict — `libtuider_dict.so`

- 打开：`.mdx` 路径、`-g` 群组；`-s` 跳转词头；`-w` 词表；`-n` 限制词典数  
- CLI 查词：dict 认领 + 词 token → 无 TUI 打印释义  
- 正文：`BODY_HTML_V1`（旁路 `.css` + entry HTML）  
- 多词典：`Tab` 循环层；`Ctrl+B` 词典面板  
- 无网络；`--db` 导出 sibling `.db`  

### 5.4 epub — `libtuider_epub.so`

- 打开：`-e` / `--epub` / `*.epub`  
- entry = spine 章节；body = 章节 markdown  
- XHTML→md；数值实体解码；`img` → `![alt](src)` 占位  
- 无网络 / 无缓存；冒烟：`scripts/smoke-epub.sh`  

---

## 6. TUI 输入模型

### 6.1 InputMode 优先级

`Help` > `Ai` > `Nav` > `VimSearch` > `Visual` > `Normal`

**非 Mode、更高优先级拦截器：** 词典面板 → `zz` avy → `s` line-jump → 全局键 → 再按 Mode 分发。

### 6.2 全局键

| 键 | 作用 |
|----|------|
| `Ctrl+Q` / `Ctrl+C` | 退出 |
| `?` | 帮助（VimSearch 中不打开） |
| `Ctrl+F` | 切换侧栏（`single_entry` 无效） |
| `Ctrl+B` | 词典选择面板 |
| `Ctrl+Y` | 整条释义 yank（OSC 52） |
| `Ctrl+U` | 清过滤保留结果 |
| `Ctrl+S` | 侧栏布局 left ↔ top（需侧栏可见） |
| `Alt+L` | AI 面板（`feature=ai`） |
| `Alt+Shift+L` | AI 最大化 |

帮助：**任意键关闭**（不执行该键动作）。

### 6.3 侧栏开（过滤焦点）

- 可打印字符：过滤；`Backspace` / `Ctrl+W` 编辑过滤  
- `Enter` 打开；`Tab` 循环 source 层  
- `↑↓` / Pg：列表；`Alt+↑↓`：滚正文  
- **单键正文命令**（`/ f o s v V z a [ ]` 等）在侧栏开时**禁用**

### 6.4 侧栏关（正文焦点）

| 键 | 作用 |
|----|------|
| `/` | Vim 搜索（↑↓ 历史，cap 50）；命中黄底，**当前**命中红底 |
| `n` / `N` | 下一/上一匹配（需已有 query） |
| `v` / `V` | 字符 visual（caret）/ 行 visual |
| `y` | visual 内 yank OSC 52 |
| `d` | visual 内选区 → dict filter（仅 dict） |
| `a` | visual 内选区 → AI system 上下文（input 空） |
| `s` | 视口 **line-jump** 多键标签（字符集 `qwedrasdfwzxcv`） |
| `zz` | **avy**：输入连续子串 → 标签 → 跳转（空格计入查询） |
| `f` | 链接列表（侧栏开也可用；点蓝链 / Enter → **OSC 52 复制 URL**，非开浏览器） |
| `o` | 大纲 / TOC（可过滤） |
| `Alt+f` | consult：当前正文 orderless 行过滤（↑↓ 历史） |
| `Alt+Shift+f` | **corpus**：跨**全部条目**（文件列表 / feed 列表 / dict 等）全文搜索，Enter 打开该条目 |
| `a` | 插件 action（HN 外链全文） |
| `O` / `Alt+o` | 打开当前目录（`O` 含 SHIFT 修饰，如 Windows 终端） |
| `[` / `]` | 上/下主章节 |
| `Backspace` | 弹出 in-app 链接历史 |
| `h j k l` 等 | vim 式 caret 运动；`gg`/`G`/`HML`；半页/整页 |

### 6.5 Visual

- `v` 直接 Char 选区；`V` Line  
- 运动扩展选区；`y` yank 后退出；`Esc` / 再 `v` 退出
- visual 内 **`d`**：选区 → dict 侧栏 filter（**仅 dict 会话**；非 dict status 并保留 visual）
- visual 内 **`a`**：选区写入 AI system 上下文并打开面板；**input 空**、不自动发送；成功后退出 visual
- **OSC 52** 是一种终端协议，让 TUI 应用可以将文本复制到系统剪贴板，即使在 SSH/远程会话中也能工作。原理：应用发送一段特殊的转义序列（包含要复制的文本），终端如果支持就会把它放进剪贴板。
- 序列格式：`ESC ] 52 ; c ; <base64> BEL`，其中 `ESC` = 转义字符（`\x1b`），`52` = 剪贴板操作码，`c` = clipboard 选择，`<base64>` = 文本的 Base64 编码，`BEL` = 结束标记（`\x07`）。
- 支持的终端：iTerm2、Windows Terminal、WezTerm、kitty、Alacritty、foot，以及大多数 SSH 客户端（mosh、eternalterminal 等）。
- 如果终端不支持，操作会静默失败或在状态栏显示 `yank failed`。

### 6.6 Nav 叠层（Links / Toc / Consult / Corpus）

- `Esc` 关闭  
- **`j` / `k` 与其它可打印字符一样进 filter query** 并 refilter（**不**作列表导航）  
- `↑` / `↓`：列表移动；Consult 上 `↑↓` 仍为**查询历史**（j/k 不绑历史）  
- `Enter` 跳转（Corpus：打开对应 entry 并可选高亮 query）  

### 6.7 AI（default feature）

- `Alt+L` 开；**`Enter` 发送**；**`Ctrl+J` 换行**  
- `Tab` 切 provider；`Alt+t` 全文翻译路径  
- visual **`a`**：选区注入 system（`--- user selection ---`）；与 `Alt+L` 共用面板
- 429/5xx **自动换下一个 provider**  
- loading 时 `Esc` 取消请求  

---

## 7. 布局与 UI

- 宽屏：侧栏 | 正文 |（可选）AI  
- 窄屏：无侧栏或上下分栏  
- AI 最大化：占满，无 reader chrome  
- 正文行预 wrap 到 content 宽；search / avy 高亮；line-jump / avy 标签单遍绘制  

---

## 8. 构建与验证

| 动作 | 命令 / 产物 |
|------|-------------|
| 本体 debug (Linux) | `cargo build` → `target/debug/tuider` |
| 用户测 TUI (Linux agent) | fish 别名 `tdd` → 上式路径（**不编译**；改码后 agent 必须 `cargo build`） |
| Linux release | `cargo build --release -p tuider` → `target/release/tuider` |
| Windows release（本机构交叉） | `cargo build --release -p tuider --target x86_64-pc-windows-gnu` → `target/x86_64-pc-windows-gnu/release/tuider.exe` |
| 插件 | `./scripts/install-plugins.sh` 或 `tuider pkg install …`（**需源码工作区**） |
| 测试 | `cargo test`；epub：`./scripts/smoke-epub.sh` |

### Windows / Cygwin

- **`tdd` 不是 Windows 命令。** 仅 Linux agent 的 fish 别名；Cygwin 若 `which tdd` 指向 `…/Pandoc/tdd`，那是别的程序，勿当 Tuider 用。用 **`tuider.exe`**。
- **`tuider pkg install` 需要源码工作区**（含 `Cargo.toml` 的 workspace root）。预编译 exe 旁若无源码会报 `workspace root missing Cargo.toml` + hint。替代：在源码树装，或拷贝已构建的 `tuider_*.dll` / `.so` 到 plugins 目录。
- **配置路径**：`./tuider.yml` → `./.tuider.yml` → 父目录 → `%USERPROFILE%\.config\tuider.yml`（Linux：`~/.config/tuider.yml`；`XDG_CONFIG_HOME` 优先）。
- **插件目录默认**：`%USERPROFILE%\.local\share\tuider\plugins\`（Linux：`~/.local/share/tuider/plugins/`）；`TUIDER_PLUGINS_DIR` 覆盖。
- 运行示例：
  ```bash
  # Cygwin：路径含空格要加引号
  "/cygdrive/d/path/to/tuider.exe" -V
  "/cygdrive/d/path/to/tuider.exe" README.md
  "/cygdrive/d/path/to/tuider.exe" https://example.com
  ```
  或 cmd：`G:\path\to\tuider.exe -V`
- 本构建 PE 仅依赖系统 DLL（kernel32 / msvcrt / user32 / ws2_32 等），**无需**另拷 `libwinpthread` / `libgcc`。
- TUI 需真实 Windows 控制台；Cygwin 下若花屏，改用 **cmd / Windows Terminal** 跑 exe。

License：core `MIT OR Apache-2.0`；dict 依赖 **AGPL** `mdx-tui-mdict` 时发行自担。

---

## 9. 文档索引

| 文件 | 内容 |
|------|------|
| [FEATURES.md](FEATURES.md) | **本文件：功能说明** |
| [STATUS.md](STATUS.md) | 加载 / ABI / 包结构权威快照 |
| [plugins.md](plugins.md) | ABI、缓存、HN 有意差异、SSRF |
| [NEXT.md](NEXT.md) | 未做 / 二期 |
| [PLAN.md](PLAN.md) | 战略决策 |
| [../README.md](../README.md) | 快速入门 |
