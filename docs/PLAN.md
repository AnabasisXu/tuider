# Tuider — 对话规划总览

> 日期：2026-07-22（文档更新：host-boundary / 动态 cdylib）  
> 来源：mdx-tui 定位讨论 + core/插件边界重划 + 独立 crate 落地 + 动态 so 演化。  
> 状态：**可测交付**；**加载权威见 [STATUS.md](STATUS.md)**；**下一步见 [NEXT.md](NEXT.md)**。

---

## 1. 问题

`mdx-tui` 以 MDX 词典为中心，同时塞进：

- Markdown / txt 目录阅读  
- URL → Markdown  
- Hacker News  
- AI 对话  

`App`（~2.5k 行）与 `main`（~1.1k 行）把 `ContentSource::{Dictionaries, Markdown*, HackerNews}` 和网络依赖硬编码在一起。  
继续加「脚本 / 词典 / HN」只会更臃肿。

用户判断：

> **代码（阅读路径）才是核心；其余都是可选项。**  
> 定位应改为**终端阅读器**；MDX 只是插件。

---

## 2. 战略决策（已拍板）

| # | 决策 | 说明 |
|---|------|------|
| D1 | **新产品名 Tuider** | Terminal UI reader；不是改名凑合 |
| D2 | **新开项目 / 新仓库目录** | `~/cleantest/tuider`（本树）；**不在 mdx-tui 内改定位** |
| D3 | **mdx-tui 先不动代码** | 冻结；UI 后续复用移植，不边砍边写新产品 |
| D4 | **mdx-tui 不再作为长期独立应用形态** | 用户意图：词典能力将来是插件血统，而不是继续以「全能 mdx-tui」演进；**本阶段仍不删不改其代码** |
| D5 | **默认内容：md + txt** | 裸命令扫 cwd 一层 `*.md` / `*.txt` |
| D6 | **插件机制** | **动态 cdylib**（`crates/tuider-plugin-*` → `.so`）+ host `dlopen` + `plugin_catalog`；yml `enabled` 为额外门禁。历史「Cargo feature 链入」已作废 |
| D7 | **插件集** | `url` · `hn` · `dict` · `code`（**不含 AI**） |
| D8 | **架构方案 A** | 单 bin + 动态 so + `ContentSource`（host）/ `PluginTextSource`（api） |
| D9 | **兼容名** | 主命令 `tuider` |
| D10 | **交付** | core reader + AI + 四插件 crate 可测 |
| D11 | **AI 属产品 core** | 编译上仍为 `feature = "ai"`，且 **`default = ["ai"]`**；无 key 仍可阅读 |
| D12 | **default 不含 url** | `url` 仅插件 so；日常 `cargo build` = 阅读器 + AI |

---

## 3. 产品边界

> 加载与插件边界**权威**：[STATUS.md](STATUS.md)  
> 早期路线图：[`docs/superpowers/specs/2026-07-21-tuider-core-roadmap-design.md`](superpowers/specs/2026-07-21-tuider-core-roadmap-design.md)（历史）  
> host-boundary 设计：[`docs/superpowers/specs/2026-07-22-host-boundary-refactor-design.md`](superpowers/specs/2026-07-22-host-boundary-refactor-design.md)

### 3.1 Core（产品一等公民）

- 打开文件 / 目录（md + txt）
- 侧栏文档列表、滚动、焦点、主题、帮助 `?`
- **md/txt 渲染**（pulldown-cmark / mdterm 风格路线）
- **vim 式 `/` `n` `N` 搜索**
- **visual 选区 + yank**（OSC 52 优先）
- **AI 对话/共读**（`feature = "ai"`，default 开启；无 API key 不阻断阅读）
- 最小配置；**无词典 / 无 url 也可运行**

### 3.2 插件（无 `.so` 则 CLI 入口不存在）

| 插件 id | 入口 | 依赖倾向（在插件 crate 内） |
|---------|------|---------------------------|
| `url` | `-u` / 裸 URL | HTTP + readability；缓存自管 |
| `hn` | `-hn` | HTTP；缓存自管 |
| `dict` | `.mdx`、群组、CLI 查词 | mdict、HTML/CSS 信封 |
| `code` | `--code` / 源码扩展 | plain → syntect → 可选 tree-sitter |

Host 空 Cargo feature 名 `url`/`hn`/… 仅为兼容壳，**不**链入代码。发行知识见 `plugin_catalog`。

### 3.3 非目标（第一期 / 二期边界）

- ~~动态 `.so` 插件~~ → **已做**（见 STATUS）  
- 脚本引擎实现  
- 在本阶段修改 `mdx-tui` 源码或「精简砍模式」落地  
- 完整重绘视觉语言（先复用 mdx-tui 的布局/主题原则）  
- HTML 渲染下沉 / AppView / 只信 handles / ABI 协商 → **二期**（见 NEXT）

---

## 4. 架构（方案 A 摘要）

```
tuider (bin, src/)
├── core: app ui md scan config ai theme
├── loader + plugin_catalog   (dlopen .so；claims/missing/pkg 单源)
├── plugin-api  (crates/tuider-plugin-api)  ABI v1 + PluginTextSource + BODY_HTML_V1_PREFIX
└── plugin crates → cdylib
    libtuider_url.so | hn | code | dict
```

**深度模块原则**（Ousterhout）：

- Core 的接口应是「打开可读源 + 跑 TUI +（可选）AI 会话」，不暴露 HN job / dict panel 细节  
- 插件把复杂性拉下去：网络、缓存 TTL、词典索引都在**独立 cdylib** 内  
- 拒绝 pass-through 空 crate；插件只依赖 `tuider-plugin-api`

**默认依赖面（`default = ["ai"]`）**：`ratatui` + `crossterm` + md 渲染 + AI HTTP。  
**不得**出现在 default：`mdict`、`readable-readability`、HN 栈。  
**slim（`--no-default-features`）**：仅阅读依赖。

**历史注记：** 早期 D6 曾写「Cargo features + 非动态 so」；已演化为动态 cdylib，该表述作废。

---

## 5. 从 mdx-tui 复用什么（只读参考，不改源）

优先移植 / 抽象的 **UI 与交互**，而非业务模式：

| 来源（mdx-tui） | Tuider 用途 |
|-----------------|-------------|
| `theme.rs` | 选中条、边框、搜索色角色 |
| `ui.rs` 布局断点 | 侧栏宽、过窄终端提示、status 行 |
| 焦点滚动键位 | 裸 ↑↓ vs Alt+↑↓ 分工 |
| vim `/` `n` `N` | 正文搜索 |
| `mdx-md` 渲染 | 已本地化为 `src/md.rs`（pulldown 路线） |
| `mdx-ai` | **core** AI 模块（R1），非插件列表项 |

**不要**第一期整文件搬迁：`hn/`、`fetch.rs`、dict HTML/CSS 引擎——属插件里程碑（现已在插件 so 内）。

---

## 6. 仓库与工作流

```
~/cleantest/tuider/          ← 本产品（新 OMP 在此打开）
~/cleantest/mdx-tui/         ← 冻结参考；本阶段零 diff
```

建议后续：

1. 用户在 `tuider` 目录 **新开 OMP**  
2. 实现与边界以 [STATUS.md](STATUS.md) / host-boundary 设计为准  
3. mdx-tui「砍阅读/HN/URL、只留词典」若仍需要，**另开任务**，与 Tuider 实现解耦  

---

## 7. 成功标准

1. 规划文档齐全，新会话无需回读本对话即可开工  
2. `mdx-tui` 工作区无业务代码改动（本会话约束）  
3. 实现后：`cargo build` 默认产物可打开 md/txt，且依赖树无网络/词典栈  
4. 插件以动态 `.so` 加载；host 通过 catalog 认领/缺 so 提示，core 无业务分支森林  

---

## 8. 开放项 / 债

- License 与发行合规（dict 依赖 AGPL mdict 时）  
- AI 多 provider 自动 failover；上游 503 体验  
- dict 大词库索引；code syntect 增强  
- 详见 **[NEXT.md](NEXT.md)**

### 已修（曾记为问题）

- vim 关键词 span 高亮（非整行）  
- visual 扩展不再强制置顶  
- visual 字符级 `v` + 行级 `V`  
- app 键位 `InputMode` 路由  
- host 死 `cache` 删除；`plugin_catalog` 单源  

### 仍在

- visual 仍可继续打磨边界体验  
- 二期：HTML 下沉、AppView、只信 handles、ABI 协商（见 [NEXT.md](NEXT.md)）

---

## 9. 文档索引

| 文件 | 内容 |
|------|------|
| [README.md](../README.md) | 产品入口 |
| [STATUS.md](STATUS.md) | **现状权威快照** |
| [NEXT.md](NEXT.md) | **下一步目标** |
| [refactor-design-and-planning.md](refactor-design-and-planning.md) | **重构方法论与切片路线**（mdx-tui→Tuider） |
| [plugins.md](plugins.md) | 插件运行机制 |
| [complexity-review.md](complexity-review.md) | 复杂度审查 |
| [superpowers/specs/2026-07-22-host-boundary-refactor-design.md](superpowers/specs/2026-07-22-host-boundary-refactor-design.md) | host-boundary 设计 |
| [superpowers/specs/2026-07-21-tuider-core-roadmap-design.md](superpowers/specs/2026-07-21-tuider-core-roadmap-design.md) | 早期路线图（历史） |
| [superpowers/specs/2026-07-21-tuider-reader-plugins-design.md](superpowers/specs/2026-07-21-tuider-reader-plugins-design.md) | 历史架构稿 |
| [superpowers/plans/2026-07-21-tuider-implementation.md](superpowers/plans/2026-07-21-tuider-implementation.md) | 旧实施规划 |
| [hygg-mdterm-format-comparison.md](hygg-mdterm-format-comparison.md) | hygg/mdterm 对标 |
| [ai-cli-doc-readers.md](ai-cli-doc-readers.md) | AI 共读 CLI 调研 |
