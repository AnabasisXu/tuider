# Tuider 未实现的 mdx-tui 功能（一次性实现清单）

> 日期：2026-07-23  
> 目的：把 **mdx-tui 已有、Tuider 尚未对等实现** 的功能收成可勾选 backlog，便于一次做完。  
> 范围：仅列 **gap**；Tuider 已有且 mdx-tui 无/弱的能力（visual yank、mdterm 渲染、动态 so、AI failover 等）不写入。  
> 权威现状：[STATUS.md](STATUS.md) · 产品边界：[PLAN.md](PLAN.md)

---

## 0. 怎么读这份清单

| 标记 | 含义 |
|------|------|
| **P0** | 词典/CLI 主路径；没有则 dict 不像 mdx-tui |
| **P1** | AI 与词典联动；mdx-tui 的核心差异 |
| **P2** | TUI 交互壳（面板、搜索框、键位） |
| **P3** | 渲染边角 / 导出 / 体验抛光 |
| **OUT** | mdx-tui 有，但 Tuider 产品边界明确不做或另议（仍列出以免漏） |

实现落点约定（与方案 A 一致）：

| 落点 | 内容 |
|------|------|
| **dict so** | `crates/tuider-plugin-dict` |
| **host** | `src/`（AI、键位、布局、CLI 分发、HTML 渲染） |
| **hn/url so** | 仅列仍弱于 mdx-tui 的交互项 |
| **config** | `~/.config/tuider.yml`（已有字段未接线的要接通） |

**验收总原则：** 每条 gap 应有一条可运行对照（同一词典/同一 flag，mdx-tui vs tuider 行为一致或文档声明的有意差异）。

---

## 1. 总览（按域）

| 域 | 缺口体量 | 主要落点 |
|----|----------|----------|
| 词典 CLI / 导出 | 大 | dict so + host CLI 分发 |
| 词表 / 侧栏过滤 | 大 | dict so + host UI |
| 词典 TUI 面板 | 中 | host UI + cycle ABI |
| AI tools + 斜杠命令 | 大 | host `ai` + dict 查询 ABI |
| 输入框编辑键 | 小 | host app/ai |
| HTML 渲染边角 | 小–中 | host `html_*` |
| HN 交互细节 | 小 | hn so + host `a`（大部分已有） |
| 配置兼容 | 小 | config + dict |

---

## 2. P0 — 词典 CLI 与数据路径

mdx-tui：`mdx-tui <dicts> <word>` 有词参数即 CLI 查词；管道自动纯文本。

Tuider 现状：dict 侧栏列词头 + TUI；`-l` 只打印 entry 名/纯文本 body，**无「词参数 → 查词 stdout」主路径**；无 `--html` / `--db`。

| ID | 功能 | mdx-tui 行为 | Tuider 现状 | 落点 | 验收 |
|----|------|--------------|-------------|------|------|
| D-CLI-01 | **有词参数 CLI 查词** | `tuider … hello` → 各词典释义 stdout 后退出 | 无对等；进 TUI 或 `-l` 列名 | dict so 查词 + host 识别「词参数」不进 TUI | `tuider -g eng hello` 打印释义；`echo $?`=0 |
| D-CLI-02 | **批量词** | `"take,make"` 逗号分隔逐词输出 | 无 | dict + host | 两词两段输出 |
| D-CLI-03 | **`-l` 精简查词** | 仅词头+词典名（非「只列侧栏」） | `-l` 是 print-without-TUI 通用开关 | dict 语义与 host 对齐 | 有词时 `-l` 只出 headword/dict 名 |
| D-CLI-04 | **`-n N` 限词典数** | 只显示前 N 本（按名排序） | 无 | dict open | `-n 1` 只一本有输出 |
| D-CLI-05 | **`--html <word>`** | 原始 HTML 不渲染 | 无 | dict | 输出含标签；无 ANSI 装饰依赖 |
| D-CLI-06 | **`--db` 导出 SQLite** | 整库/路径导出 `.db` | 无 | dict（可仿 mdx-core export） | 生成可读 sqlite；表内有词条 |
| D-CLI-07 | **非 TTY 自动纯文本** | 管道/重定向关彩色 | 部分 list 路径有；**查词路径需统一** | host | `\| cat` 无 ANSI |
| D-CLI-08 | **TTY 彩色 CLI 释义** | 终端内彩色 HTML→text | 无 CLI 渲染路径 | host html 或 dict 预渲染文本 | 直接跑终端有颜色 |
| D-CFG-01 | **`wordlists` 接线** | yml `wordlists:` + `-w` | `FileConfig.wordlists` 已解析但 **dead** | config + dict + host | yml 词表生效 |
| D-CFG-02 | **`-w <name>`** | 侧栏/索引只保留词表内词头 | 无 | dict | 侧栏条目 ⊆ 词表 |
| D-CFG-03 | **`-L` 列词表** | 打印可用词表名 | 无 | host/dict | 列出 yml 中 wordlists keys |
| D-CFG-04 | **`-s "a,b"` 临时词表** | 逗号 = 临时 wordlist，侧栏仅这些 | `-s` 仅单词跳转 | dict | `-s "a,b"` 侧栏两项 |
| D-CFG-05 | **groups 与 mdx-tui 配置互通说明** | `groups` 在 mdx-tui.yml | Tuider 只读 `tuider.yml` groups | docs + 可选双读 | 文档写清路径；可选读旧文件名 |

---

## 3. P0/P2 — 词典 TUI 交互

| ID | 功能 | mdx-tui 行为 | Tuider 现状 | 落点 | 验收 |
|----|------|--------------|-------------|------|------|
| D-UI-01 | **实时词头搜索框** | 输入即过滤词头；主交互 | 无独立搜索框；侧栏靠方向键 | host UI + dict filter API 或 host 滤 `entry_at` 缓存 | 打字侧栏收缩 |
| D-UI-02 | **`Ctrl+U` 清搜索框保留结果** | 有 | 无对等 | host | 键位一致 |
| D-UI-03 | **`Esc` 清搜索+结果** | 有 | Esc 多用于关 overlay | host | 词典模式下行为对齐 |
| D-UI-04 | **`Ctrl+B` 词典选择面板** | 面板选词典；↑↓ Enter | theme 留 list 色；**无面板** | host UI + `tuider_source_cycle` / 选 active | Ctrl+B 开关；选中换正文 |
| D-UI-05 | **多词典模式标题/切换反馈** | Tab 切词典有明确状态 | Tab→`cycle` 已有；面板与 status 弱 | host status | Tab 后 title/status 显示当前词典 |
| D-UI-06 | **输入框编辑键** | Backspace/Delete/Ctrl+W/Alt+BS/Shift+Del/Home/End 词级删 | AI 输入有基础；**词典搜索框未建** | host | 与 README 键位表一致 |
| D-UI-07 | **`Ctrl+Y` 复制当前释义** | OSC52 纯文本释义 | 仅 visual `y` 选区 | host | 无 visual 也可一键复制全文 body |
| D-UI-08 | **词表模式下打开词不破坏过滤** | `open_word_show_wordlist` | 无 | dict/host | 查词后侧栏仍是词表子集 |

---

## 4. P1 — AI（mdx-tui 最大差异）

Tuider 已有：浮层/全屏、流式、多 provider、Tab、429/5xx failover、快捷 `1`=翻译全文。  
**缺：function calling 与词典/内容工具、斜杠命令集。**

| ID | 功能 | mdx-tui 行为 | Tuider 现状 | 落点 | 验收 |
|----|------|--------------|-------------|------|------|
| AI-01 | **`/exp` 导出对话** | 写 `mdx-tui-chat-{ts}.md` | 无 | `src/ai.rs` | 文件落地，含 user/assistant |
| AI-02 | **`/exp last` / `/exp N`** | 导出末条/第 N 条 | 无 | ai | 只含目标消息 |
| AI-03 | **`/switch` 列表** | 列出 provider，当前 `*` | 仅 Tab 轮换 | ai | 输入 `/switch` 列出 |
| AI-04 | **`/switch <名\|序号>`** | 切换 | 无斜杠 | ai | 切换后 status 显示新名 |
| AI-05 | **工具 `query_word`** | AI 查词典 | 无 tools | ai + dict 查询通道 | mock/真 dict 可查 |
| AI-06 | **`search_headwords`** | 前缀搜词头 | 无 | 同上 | 返回词头列表 |
| AI-07 | **`list_dicts`** | 列已加载词典 | 无 | 同上 | 名称列表 |
| AI-08 | **`batch_query`** | 多词批量 | 无 | 同上 | 多词结果 |
| AI-09 | **`analyze_vocab`** | 词汇分析工具 | 无 | 同上 | 工具可调用 |
| AI-10 | **`reverse_lookup`** | 释义反查词头 | 无 | dict 索引能力 + ai | 中文/片段能命中词头 |
| AI-11 | **`get_current_content`** | 读当前面板正文 | 仅 system 塞上下文；无 tool | ai | 模型 tool 调用可读全文/切片 |
| AI-12 | **`export_content`** | AI 写 md 到 cwd | 无 | ai | 生成文件 |
| AI-13 | **`web_search`** | AI 可搜网 | 无 | ai（可可选 feature） | 有 key/配置时可调；失败明确 |
| AI-14 | **词典上下文注入** | 当前词+词典名+释义进 prompt | 通用「当前文档」；**无词条结构** | ai + dict | 查词态 system 含 headword |
| AI-15 | **tool 多轮 loop** | 多轮 tool_calls | 无 | ai | 一次提问可多次查词再答 |
| AI-16 | **快捷提示词 2–4** | 数字键预设 | 仅 `1` | ai | 2–4 有合理默认或可配 |
| AI-17 | **AI 工具日志** | `ai-tools.log` | 无 | ai | 可选写 `~/.config/tuider/…` |
| AI-18 | **无 dict so 时 tools 降级** | N/A | 需设计 | ai | 仅阅读 tools；不 panic |

### 实现依赖（AI tools）

dict 插件目前只有 `entry_at` / `load_body` / `cycle`，**没有**「按词查询 / 前缀搜索 / 反查」的稳定 host API。一次性实现需先补其一：

1. **扩展 ABI v1 可选符号**（推荐，不 bump 也可弱符号）：  
   `tuider_dict_lookup` / `tuider_dict_search` / `tuider_dict_reverse` / `tuider_dict_list`  
2. 或 host 内嵌「仅 AI 用」查询（破坏方案 A，**不推荐**）。

---

## 5. P2 — 通用 TUI / 输入（词典模式以外仍缺的）

| ID | 功能 | mdx-tui | Tuider | 落点 | 验收 |
|----|------|---------|--------|------|------|
| UI-01 | **搜索栏布局切换语义对齐** | `Ctrl+F` 搜索栏左/上 | `Ctrl+F` 侧栏显隐 | 文档或键位 | 帮助文案与行为一致（可有意差异，需写进 help） |
| UI-02 | **词典面板关闭残影** | mdx-tui TECH_DEBT 未修 | 实现面板时避免 | host ui | 关面板无残影 |
| UI-03 | **Kitty/WezTerm Shift+Enter 协议** | app 里 best-effort 开 keyboard protocol | 需确认是否已开 | host main/event | Shift+Enter 在 AI 中换行或发送策略明确 |

---

## 6. P3 — HTML/CSS 渲染边角

host 已移植 method A 主干（tag/class、`display:none`、`::before`、暗色地板、hex/dec 实体等）。相对 mdx-tui 文档/体验仍可能缺：

| ID | 功能 | 说明 | 落点 | 验收 |
|----|------|------|------|------|
| R-01 | **`<img>` alt 提取** | ✅ 2026-07-23：void `<img>` 输出 `[alt]`（无 alt 则 `title`） | `html_render` | 有 alt 的 img 出文本 |
| R-02 | **`<table>` 简单网格** | ✅ 行=行、`td/th` 用 ` \| ` 分隔（无列对齐） | `html_render` | 简单 2×2 表不错行糊成一团 |
| R-03 | **标签解析嵌套引号外 `>`** | ✅ 已有 quote-aware tag 扫描 + 嵌套引号 fixture | `html_render` | fixture |
| R-04 | **词典 CSS fixture 回归** | 用真实词典 HTML 金样 | tests | 与 mdx-tui 同 fixture 行级近似 |

> 完整 CSS 引擎、后代选择器：两边都 **OUT**（设计取舍）。

---

## 7. 插件侧相对 mdx-tui 的剩余弱项

### 7.1 HN

多数已迁（top、缓存 TTL、评论、`a`→article action）。剩余：

| ID | 功能 | 说明 | 落点 |
|----|------|------|------|
| HN-01 | **`-hn -l` CLI Markdown 表** | ✅ **有意差异**（文档化）：`entry_at` 兼 TUI 侧栏，`-l` 只出标题行；mdx-tui 专表见 `docs/plugins.md` | hn so + host |
| HN-02 | **评论树深度/全量** | ✅ 文档化：BFS **cap 40**（`MAX_COMMENTS`）；ponytail | hn so |
| HN-03 | **Enter vs Shift+Enter 评论/原文分离** | ✅ 文档化：Enter=评论+meta，`a`=全文；保留差异 | 可保留差异，**文档化**；若要对齐则 host keys |

### 7.2 URL

| ID | 功能 | 说明 |
|----|------|------|
| URL-01 | **缓存目录/TTL 与可清缓存 UX** | ✅ 已写入 `docs/plugins.md`（`~/.cache/tuider`、URL 无 TTL / HN TTL） |
| URL-02 | **失败重试与错误文案** | ✅ 文档化：URL 无重试+open err；HN open 5 次退避；article 失败进正文 |

### 7.3 code

mdx-tui **无**独立 code 模式 → **无 gap**（Tuider 超集）。

---

## 8. OUT — 不纳入「一次性实现」或需另决策

| 项 | 原因 |
|----|------|
| 把 url/hn/dict 链回主 bin | 违反方案 A |
| 完整浏览器 CSS / 后代选择器 | 两边明确不做 |
| PDF/EPUB/DOCX | mdx-tui 也无；Tuider 明确 YAGNI |
| 对话持久化到 chats/ | mdx-tui TECH_STATUS 也未完成；可选后置 |
| AI 50ms 轮询→事件驱动 | 性能债，非功能缺口 |
| 在 mdx-tui 仓库内改代码 | 冻结策略；金标只读 |
| Windows 发布包与 mdx-tui 对等 | 产品发行另项 |
| 阅读进度/书签（hygg） | mdx-tui 也弱；属 Tuider 增强非 gap |

---

## 9. 一次性实现建议切片（可并行）

按依赖排序，便于 agent 并行（同切片内串行）：

### 切片 A — dict CLI 完整（P0）

1. D-CLI-01..08  
2. D-CFG-01..05  
3. 最小：词参数不进 TUI；`--html`；`-w`/`-L`；`--db` 可第二刀但同一切片交付  

**出口：** `tuider -g eng hello`、`… --html hello`、`… -w gre -l hello` 与 mdx-tui 对照通过。

### 切片 B — dict TUI 搜索 + 面板（P0/P2）

1. D-UI-01..08  
2. 搜索框状态机 + Ctrl+B 面板 + Ctrl+Y  

**出口：** 无词参数进 TUI，打字滤词头；Ctrl+B 切词典。

### 切片 C — dict 查询 ABI（P1 前置）

1. 可选 C 符号：lookup / search_prefix / reverse / list_dicts  
2. host loader 弱符号绑定  
3. 单元测试不启 TUI  

**出口：** host 测试能查词、前缀、列词典。

### 切片 D — AI tools + 斜杠（P1）

1. AI-01..04 斜杠（可无 dict）  
2. AI-05..15 tools + loop（依赖 C）  
3. AI-16..18 抛光  

**出口：** 配置 AI 后「查一下 etymology of X」会 tool_call；`/exp` 有文件。

### 切片 E — 渲染 + HN/URL 收尾（P3）

1. R-01..04  
2. HN-01..03 文档或对齐  
3. URL-01..02  

**出口：** 金样 HTML + `tuider -hn -l` 表可读。

---

## 10. 工作量粗估（人感，非排期）

| 切片 | 相对量 | 风险 |
|------|--------|------|
| A dict CLI | 中 | `--db` 导出格式；词参数与 path 消歧 |
| B dict TUI | 中–大 | 搜索框 vs 阅读壳焦点模型冲突 |
| C 查询 ABI | 中 | reverse 索引成本；大库内存 |
| D AI tools | 大 | tool 协议、取消、无 dict 降级 |
| E 收尾 | 小–中 | table 网格易咬时间 |

**最大风险：** B 的「词典搜索框」会把 host App 再次拉向 mdx-tui god-mode。  
约束：**搜索框状态只在 dict source 激活时启用**；md/url/hn 路径零新增焦点模式。

---

## 11. 词参数消歧规则（实现时冻结）

避免 path / word / flag 混乱（mdx-tui 也有历史债）：

```text
1. 已加载 dict so 且 claims 命中（.mdx / -g / 显式 dict 路径）
2. 若存在「非 flag、非路径存在的文件/目录」token → 视为 word(s)
3. 逗号分隔 → 批量词（D-CLI-02）
4. 仅有词、无 dict 路径 → 用 yml default group 或报错提示 -g
5. 有词且 stdout 非 TTY 或 -l/--html/--db → 不进 TUI
```

---

## 12. 验收矩阵（一次性做完的定义）

- [x] **A** D-CLI-01..08，D-CFG-01..05  
- [x] **B** D-UI-01..08  
- [x] **C** lookup/search/reverse/list 可测（plugin-api + loader + dict.so）  
- [x] **D** AI-01..18（AI-13 web_search 需 TAVILY_API_KEY；无 key 明确失败）  
- [x] **E** R-01..03 + unit tests；HN-01 有意差异文档化；HN-02/03 + URL-01/02 见 `docs/plugins.md`  
- [x] 无回归：`tuider README.md`、`-u`、`-hn`、`--code`、缺 so 提示仍绿  
- [x] `cargo test -q` host + dict/hn 插件测试绿  


---

## 13. 源码锚点

### mdx-tui（只读金标）

| 能力 | 路径 |
|------|------|
| CLI / 模式分发 | `~/cleantest/mdx-tui/crates/mdx-tui/src/main.rs` |
| App / 词表 / HN 键 | `…/src/app.rs` |
| 词典 + export + HTML | `…/mdx-core/` |
| AI tools | `…/mdx-ai/src/chat.rs`（`get_tools` / `execute_tool_call`） |
| README 键位与 flag | `~/cleantest/mdx-tui/README.md` |

### Tuider（改这里）

| 能力 | 路径 |
|------|------|
| CLI | `src/main.rs` |
| 配置 wordlists（未接线） | `src/config.rs` |
| AI | `src/ai.rs` |
| 键位 | `src/app/keys.rs` |
| HTML | `src/html_render.rs`, `src/html_css.rs` |
| dict so | `crates/tuider-plugin-dict/src/lib.rs` |
| hn so | `crates/tuider-plugin-hn/src/lib.rs` |
| ABI | `crates/tuider-plugin-api/src/lib.rs` |
| loader 弱符号 | `src/loader.rs` |

---

## 14. 相关文档

| 文件 | 关系 |
|------|------|
| [STATUS.md](STATUS.md) | 已实现权威 |
| [PLAN.md](PLAN.md) | 边界；本清单不推翻 D6/D7 |
| [plugins.md](plugins.md) | so 机制；C 切片扩 ABI 时更新 |
| [NEXT.md](NEXT.md) | 工程债；本清单是 **功能 parity** |
| [refactor-design-and-planning.md](refactor-design-and-planning.md) | 移植方法 |

---

**一句话：**  
Tuider 缺的不是「阅读器」，而是 **mdx-tui 词典工作流（CLI/词表/面板）+ AI function calling**；按 A→C→D→B→E 或 A∥C 后 D∥B 一次做完即可宣称词典血统 parity（OUT 除外）。
