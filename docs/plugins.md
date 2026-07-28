> 用户可见功能总表：[FEATURES.md](FEATURES.md)。加载权威：[STATUS.md](STATUS.md)。

# Tuider 插件机制（方案 A：动态 `.so`）

## 你要的分离，现在怎么落地

| 要求 | 实现 |
|------|------|
| 本体尽量小 | 主包 **不再静态链接** url/hn/dict/epub；**code 在 core**（syntect） |
| 拷贝插件文件才能用 | 只有 `plugins_dir` 里存在对应 `.so` 才能裸 URL/`-hn`/dict/epub… |
| 独立包 | `crates/tuider-plugin-*` 编成 **cdylib** |
| 发行知识单源 | host `src/plugin_catalog.rs`：claims / 缺 so 提示 / `pkg` |

```
~/.local/share/tuider/plugins/     # 默认目录（可用 TUIDER_PLUGINS_DIR 或 yml plugins_dir）
  libtuider_url.so
  libtuider_hn.so
  libtuider_dict.so
  libtuider_epub.so
  ...
```

**没有文件 = 没有功能**（不是配置里假装关掉）。

## plugin_catalog（host）

`plugin_catalog` 是 id → so / crate / summary / **claims** 的**唯一**表：

- `main`：`handles_args || catalog.claims` 决定是否 `open`；否则 `missing_plugin_hint` 打 `need plugin …`
- `pkg list|install|remove`：同一 `CATALOG`（id / crate / so / summary）
- **无**第二份 id→flag `match`

claims 语义：

| id | 认领 |
|----|------|
| url | 裸 `http(s)://`（**无** `-u` / `--url`） |
| hn | `-hn` / `--hn` |
| dict | `-g` / `--group` / `.mdx`（`-s`  alone 不认领） |
| epub | `-e` / `--epub` / `.epub` |

code 已并入 core，不再 claims 插件。缺 so 提示优先级：url → hn → dict → epub。

## 构建与安装

或用主机子命令（在源码树内）：

```fish
cargo run -- pkg list
cargo run -- pkg install url
cargo run -- pkg install all
cargo run -- pkg remove hn
```

```fish
cd ~/cleantest/tuider

# 本体
cargo build --release

# 插件（示例：url）
cargo build -p tuider-plugin-url --release
mkdir -p ~/.local/share/tuider/plugins
cp target/release/libtuider_url.so ~/.local/share/tuider/plugins/
```

其它插件同样：`tuider-plugin-hn` → `libtuider_hn.so` 等（crate 名见各 `Cargo.toml` `[lib] name`）。  
一键：`./scripts/install-plugins.sh`（含 epub）。冒烟：`./scripts/smoke-epub.sh`。

## 运行

```fish
# 无插件目录 → 拒绝裸 URL
TUIDER_PLUGINS_DIR=/tmp/empty tuider https://example.com

# 有 .so → 可用
tuider https://example.com
tuider book.epub
```

配置（可选）：

```yaml
plugins_dir: /path/to/plugins
plugins:
  url:
    enabled: true   # false = 即使有 .so 也不加载
```

`enabled: false` 是**额外门禁**；**不能替代**「文件必须在目录里」。

## ABI（v1）

每个 `.so` 导出（见 `tuider-plugin-api`）：

- `tuider_plugin_abi_version` / `id` / `name` / `open` / `close`
- `tuider_source_title` / `entry_count` / `entry_at` / `load_body`
- `tuider_string_free`

插件返回 **UTF-8 文本**；主机负责渲染。  
**禁止**跨 so 传 ratatui 类型。

### Body 格式与 `BODY_HTML_V1_PREFIX`

默认：UTF-8 markdown 或纯文本（host 走 md/plain）。

可选 **HTML 信封**（不 bump ABI；值为 body 约定）：

1. 正文以常量 `BODY_HTML_V1_PREFIX`（`"TUIDER_HTML_V1\n"`）开头  
2. 随后 payload：`css + "\n\u{1e}\n" + html`  
3. host `loader` 识别前缀后做 CSS 子集 → ratatui Lines  

API 侧适配后文本源 trait 名：`PluginTextSource`。App 侧已渲染源仍为 host `ContentSource`。

## 与旧「Cargo features 插件」的区别

| 旧 | 新（A） |
|----|---------|
| `cargo build --features url` 把代码链进 bin | 主 bin **不**链 url |
| yml 开关只挡入口 | **无 so 则无代码** |
| 源码独立但产物一体 | 源码独立且**产物独立** |
| host 平行 claims 表 | **plugin_catalog** 单源 |

遗留 feature 名 `url`/`hn`/… 在 host 上为空兼容，**不会**再拉进插件依赖。

## 当前实现进度

- [x] ABI + host `loader.rs`（libloading / dlopen）
- [x] `plugin_catalog` + `pkg` 同源
- [x] `tuider-plugin-url` → `libtuider_url.so`
- [x] `tuider-plugin-hn` → `libtuider_hn.so`
- [x] code 高亮并入 core（`src/code.rs` + syntect；无 so）
- [x] `tuider-plugin-dict` → `libtuider_dict.so`
- [x] `tuider-plugin-epub` → `libtuider_epub.so`
- [x] `BODY_HTML_V1_PREFIX` + host 渲染
- [x] 帮助列出已加载插件与 plugins 目录
- [x] `scripts/install-plugins.sh` / `scripts/smoke-epub.sh`

## URL / HN 缓存与错误（用户可知路径）

插件自管磁盘缓存；host **无** `cache` 模块。

### 路径

| | |
|--|--|
| 根目录 | `$XDG_CACHE_HOME/tuider`，否则 `~/.cache/tuider` |
| URL 页 | `pages/<fnv1a64(url)>.md`（markdown 正文） |
| URL feed 原始 | `pages/<fnv1a64(url)>.feed` |
| HN top | `hn/topstories.json` |
| HN item | `hn/item/<id>.json` |
| HN body | `hn/body/v2/<id>.md`（+ `.comments` 计数戳） |
| HN 外链全文 | 同 URL 的 `pages/…` |

**清缓存：** 删整个目录或子路径即可，例如 `rm -rf ~/.cache/tuider/pages`。无专用 CLI 清缓存命令（ponytail）。

### HN 开关

| 调用 | 网络 | 行为 |
|------|------|------|
| `tuider -hn` / `-hn -l` | **否** | 只读磁盘缓存（**忽略 TTL**）；无 top 缓存 → 报错提示 `--sync` |
| `tuider -hn --sync` | **是** | 刷新 top + items；TUI 下再预建 body（meta+评论） |
| 键 `a`（action `article`） | **是**（按需） | 抓当前条外链全文；可写 `pages/` 缓存 |
| `-n` / `--limit` | — | 默认 **30**（`-l` 且未传 `-n` 时 **15**） |

首次使用：`tuider -hn --sync`（或 `-hn --sync -l`）灌满缓存，之后日常 `tuider -hn` 秒开。

### TTL（仅 `--sync` / 外链抓取写缓存时）

| 键 | 时长 |
|----|------|
| topstories | 15 min（`--sync` 时 `cache_get_fresh`） |
| item JSON | 6 h |
| 外链 page | 24 h |

默认 cache-only 路径用 `cache_get_any`：**过期也读**。URL 插件：命中即用（**无 TTL**）。

### HTTP / gzip / SSRF

HN 与 URL 的 reqwest 启用 **`gzip`** feature。部分站点（如强制 `Content-Encoding: gzip`）无此 feature 会把压缩体当 UTF-8 → 全文 `�` 乱码并污染 `pages/` 缓存；修完后删对应 `pages/<hash>.md` 再 `a`。

URL/HN 抓取前拦 **localhost / `.local` / private·link-local·loopback IPv4/IPv6**（host 字符串判断，不解析 DNS）。错误文案含 `blocked local host` / `blocked local/private host`。

### 错误文案 / 重试

- **URL open 失败**：`tuider_plugin_open` 经 `write_err` 返回可读字符串（`timeout`/`connect`/`request`/`http NNN`/`network`、`only http(https)`/`blocked local host`）；host 打印后 exit。**无自动重试**（ponytail）。
- **HN open（`--sync`）**：top/item 拉取最多 **3** 次（list **2**）指数退避；失败文案 `HN open failed after retries: …`。
- **HN open（默认 cache-only）**：无 `hn/topstories.json` 或条目不足 → `… — run tuider -hn --sync once to populate cache`。
- **HN 外链 article**（键 `a`）：失败写入正文 `_article fetch failed: …_`，不崩 TUI。

### HN 与 mdx-tui 有意差异（HN-01..04）

| ID | 行为 |
|----|------|
| **HN-01** | mdx-tui `-hn -l` 打 markdown 表；Tuider `-l` 是通用「打印 entry_at」，HN 侧栏/列表为**纯标题行**（与 TUI 侧栏共用 `entry_at`，不能塞表行）。需要表格式时进 TUI 或自行管道处理标题列表。 |
| **HN-02** | 评论 BFS **cap 20**（`MAX_COMMENTS`）；更深树以后再开。 |
| **HN-03** | mdx-tui：Enter/Shift+Enter 分评论与原文；Tuider：**Enter 默认评论+meta**，键 **`a`**（action `article`）再抓外链全文。保留差异。 |
| **HN-04** | Tuider 默认 **cache-only**；`--sync` 才联网刷新。mdx-tui 若每次 open 都拉网，属有意差异。 |

## EPUB（epub 插件）

- claims：`-e` / `--epub` / 路径以 `.epub` 结尾
- open：按 spine 拆章节；`entry_at` = 章节标题；`load_body` = 章节 markdown
- 实体：数值 HTML 实体解码；`img` → `![alt](src)` 占位（不嵌图）
- 测试：`crates/tuider-plugin-epub/tests/fixtures/*`；host `loader`/`App` 可选 so 测；`scripts/smoke-epub.sh`
