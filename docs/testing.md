# Tuider 测试注意点

目录：`~/cleantest/tuider`  
日期：2026-07-26  
相关：[`STATUS.md`](STATUS.md) · [`plugins.md`](plugins.md) · [`FEATURES.md`](FEATURES.md) · [`ui-testing.md`](ui-testing.md) · CI [`.github/workflows/test.yml`](../.github/workflows/test.yml)

本文汇总**测什么、怎么隔离、哪些语义不能回归**。不是测试计划排期表。
**TUI / 按键 / 画屏自动化**见 [`ui-testing.md`](ui-testing.md)（四层漏斗：L1 状态机 · L2 TestBackend · L3 PTY · L4 人工）。

---

## 1. 架构决定测试切面

| 层 | 内容 | 测试重点 |
|----|------|----------|
| **Core binary** | md / txt、code 高亮、TUI shell、AI | **不依赖任何 `.so`** 也能跑 |
| **Plugins** | `url` / `hn` / `dict` / `epub` → `libtuider_*.so` | 必须 **dlopen**；业务不链进 bin |
| **Host boundary** | ABI v1 + `TUIDER_HTML_V1` 信封 | **禁止**跨 so 传 ratatui 类型 |
| **Catalog** | `src/plugin_catalog.rs` 单源 | claims 路由、缺 so 提示、`pkg` 同源 |

**原则：**

1. core 用例尽量 `TUIDER_PLUGINS_DIR=<空目录>`，验证「无插件仍可用」。
2. 插件用例必须 **build 出对应 profile 的 `.so`，再拷到临时目录**；不要默认依赖 `~/.local/share/tuider/plugins`。
3. 优先用 `-l` / `--print` 做非交互验收；全屏 TUI 按 [`ui-testing.md`](ui-testing.md) 分层（L1 状态机为主，L3 PTY 仅少量路径）。

现成范本：

- `scripts/smoke-core.sh`
- `scripts/smoke-epub.sh`
- CI：`cargo test --workspace` → smoke-core → smoke-epub

---

## 2. 环境隔离（几乎必做）

不隔离会读到本机插件 / `tuider.yml` / URL·HN 缓存，出现「本地绿、CI 红」或反向假绿。

每个测例建议固定：

```bash
export TUIDER_PLUGINS_DIR="$(mktemp -d)"   # 空，或只放被测 .so
export XDG_CONFIG_HOME="$(mktemp -d)"     # 避免写真实 ~/.config/tuider.yml
export XDG_CACHE_HOME="$(mktemp -d)"      # 隔离 URL / HN 缓存
# 若实现还会落到 HOME：
export HOME="$XDG_CONFIG_HOME/home"
unset TUIDER_AI_KEY TUIDER_AI_BASE_URL    # 默认测不要打真 API
```

还要注意：

| 陷阱 | 说明 |
|------|------|
| 配置搜索顺序 | cwd → parents → `~/.config/tuider.yml`；在 repo 根跑可能读到工作区 yml |
| TUI 入口 auto-create config | 无配置时进入 TUI 会写配置；非交互优先 `-l` |
| wordlists dual-read | yml 无 wordlists 时可能读 `~/.config/mdx-tui.yml`；dict 测要隔离或显式 fixture |
| profile 对齐 | debug host 配 debug so；release 配 release；混用易 dlopen/符号异常 |

---

## 3. 插件 / ABI 回归面

### 3.1 文件存在 vs 能加载

- `pkg list` 区分：**installed** vs **`file-only`**（文件在但 dlopen 失败）。
- 冒烟应覆盖：空 dir、有效 so、损坏/ABI 不匹配 so（若可构造）。

### 3.2 ABI 与 body 信封

- `tuider_plugin_abi_version` = **`TUIDER_PLUGIN_ABI = 1`**（未 bump 前保持 1）。
- open / close、CString 所有权（谁 `free`）错误 → 泄漏或 double-free。
- Body 可选：

  ```text
  TUIDER_HTML_V1\n
  + css + "\n\u{1e}\n" + html
  ```

  前缀或分隔符错了会当纯文本渲。core code 与插件共用 host `loader::render_plugin_body_doc`。

### 3.3 「无 so 就没有 CLI 面」

产品不变量（见 STATUS）：

```text
TUIDER_PLUGINS_DIR=/tmp/empty tuider -l https://example.com
# → need plugin `url` — copy .so …
```

同理覆盖：`-hn`、`*.epub` / `-e`、`-g` / mdx。  
缺 so 提示优先级（catalog）：**url → hn → dict → epub**。

### 3.4 `enabled: false` 是第二道门

- 有 so + `plugins.<id>.enabled: false` → 仍不可用。
- **不能**用 yml 开关替代「文件必须在 `plugins_dir`」。

### 3.5 claims 路由

`handles_args || catalog.claims` 决定是否 `open`：

| 入口 | 插件 |
|------|------|
| bare `http(s)://…` | url（feed → sidebar） |
| `-hn` / `--hn` | hn |
| `-e` / `--epub` / 路径以 `.epub` 结尾 | epub |
| `-g` / group / `.mdx` 等 | dict |
| 源码文件 | **core**（不再 claims 插件） |

多参数 / 重叠时锁顺序与错误文案。

---

## 4. 网络与缓存（HN / URL）

插件自管磁盘缓存；host **无** `cache` 模块。根目录：

```text
$XDG_CACHE_HOME/tuider   # 否则 ~/.cache/tuider
```

| 行为 | 测试期望 |
|------|----------|
| `tuider -hn` 默认 | **cache-only**；无缓存要提示 `run … -hn --sync`，**不偷偷联网** |
| `tuider -hn --sync` | 才刷新；可 mock HTTP 或预置 `hn/topstories.json` |
| HN 默认读缓存 | 过期也可读（`cache_get_any` 语义） |
| URL 页缓存 | 命中即用（文档：无 TTL） |
| Feed | 侧栏条目 + body 保留 links / headings / lists |

**CI 默认禁网**；真联网 case 标 `#[ignore]` 或单独 job。  
清缓存：删目录即可（无专用 CLI）。

---

## 5. 按功能面的清单

### Core（无插件）

- [ ] 打开 md / txt / 源码；`-l` 不进 TUI
- [ ] `-r` 递归扫描
- [ ] markdown 渲染；code → HTML_V1 → host Lines
- [ ] 空 plugins dir 时本地文件仍可读

### EPUB（现成冒烟最全）

对照 `scripts/smoke-epub.sh`：

- [ ] 缺 so 提示 `need plugin \`epub\``
- [ ] list 章节（含中文 epub3）
- [ ] body 按章；`--epub` / 裸路径
- [ ] 坏文件 / 空 spine → 非 0 退出
- [ ] host `epub_plugin_dlopen_and_render`（或等价）dlopen 单测

### URL / Feed

- [ ] bare URL vs feed URL 分支
- [ ] 无 so / 有 so / 缓存命中
- [ ] feed 侧栏时间窗语义用固定 fixture，不绑「今天」

### HN

- [ ] 无缓存默认路径文案
- [ ] 有缓存列表；`--sync` 写缓存
- [ ] 文章动作 `a`（可测 API 层或有限 TUI）

### Dict

- [ ] 默认 install / slim **不含** `libtuider_dict.so`（AGPL / `mdx-tui-mdict`）
- [ ] `INCLUDE_DICT=1` 或 `with-dict` 才装
- [ ] group 查找、HTML_V1 释义；最小 mdx fixture 或 mock open
- [ ] 勿把真实词典路径 / 版权材料写进仓库测资

### pkg

- [ ] `pkg list|install|remove` 与 catalog 同源
- [ ] install 需要**源码 workspace**；预编译-only 环境给出可操作错误 + plugins 目录
- [ ] install 落点 = `TUIDER_PLUGINS_DIR` / yml `plugins_dir`

### AI（default feature）

- [ ] 无 key 失败路径
- [ ] 429 / 5xx failover（mock HTTP）
- [ ] 多 provider 解析；测试默认不真打网

### TUI / 输入模态

全自动截图式 TUI 维护成本高，建议分层：

1. **逻辑单测**：`InputMode` 优先级  
   `Help > Ai > Nav > VimSearch > Visual > Normal`
2. **非交互 CLI** 覆盖打开 / 打印 / 错误
3. **少量 PTY** 只锁：打开 README、退出、sidebar 等
4. 宽字符 / OSC 52 yank / 固定 viewport fixture，不绑真实终端尺寸

---

## 6. 构建与 CI

```yaml
# .github/workflows/test.yml（摘要）
cargo test --workspace
bash scripts/smoke-core.sh
bash scripts/smoke-epub.sh
```

注意：

1. `cargo test --workspace` **不等于**用户目录已安装插件。
2. cdylib 需 `cargo build -p tuider-plugin-*` 再 dlopen；单测内要么产出 so，要么像 smoke 一样显式 build + copy。
3. 矩阵建议：  
   - 默认：core + url / hn / epub  
   - 可选 job：`with-dict`
4. 共享状态测例用私有 tempdir；必要时 `--test-threads=1`（epub smoke 已示范）。
5. Windows：`.dll` 名、路径、`tuider.exe`；勿依赖 fish alias `tdd`。
6. `target/` 体积大：并行多 agent 注意磁盘；**不要**为普通重编动辄 `cargo clean`（会清掉 release 产物）。

---

## 7. 高价值用例（优先锁）

按投入产出：

1. 空 `plugins_dir` + 各入口 → `need plugin \`id\``（防静默失败）
2. 临时 dir 只放一个 so → 对应入口成功，其它仍提示缺插件
3. ABI / HTML_V1：code 与插件 body 共用 `render_plugin_body_doc`
4. HN cache-only vs `--sync`
5. `scripts/smoke-epub.sh` 全路径保持绿灯
6. `enabled: false` 与「so 存在」正交
7. 损坏输入：坏 epub / 坏 mdx / 超时 URL → exit code + 文案
8. AI mock failover（改 ai 配置路径时）

---

## 8. 反模式

| 不建议 | 原因 |
|--------|------|
| 依赖本机 `~/.local/share/tuider/plugins` 做默认绿 | 不可复现 |
| CI 默认真打 HN / URL / AI | 脆、慢、有密钥风险 |
| 全屏 TUI 截图覆盖一切 | 维护成本高、噪声大 |
| 跨 so 断言 ratatui `Line` / `Span` | 违反 ABI 边界 |
| slim 验收默认带 AGPL dict so | 许可与发布策略冲突 |
| debug 二进制 + release so（或反过来）不声明 | 难查的 dlopen 失败 |

---

## 9. 一句话

把 **host + 可选 dlopen 插件 + 可覆盖的 `TUIDER_*` / XDG 环境** 当成产品不变量：每个用例显式控制 `TUIDER_PLUGINS_DIR` 与缓存目录，优先 `-l` 非交互验收，TUI 只测状态机与少量关键路径。
