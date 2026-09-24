# tuider 兼容 org-mode 文件 — 设计方案

> 状态：设计稿。目标：`tuider file.org` 直接可读，复用现有 TUI（侧栏/搜索/大纲/链接/AI），并补上 org 的灵魂能力——折叠。

## 0. 结论先行

**推荐方案 A（core 渲染后端）**：在 host 新增 `src/org.rs`（orgize 解析 → ratatui Lines），把 `.org` 挂进现有本地文档管线（`scan::is_doc` + `source.rs::load` 一个分支），同时把**折叠**实现为基于 `headings` 索引的通用 App 能力——不止 org，markdown/HTML 插件源一并受益。

不推荐方案 B（`.so` 插件）：org 是纯文本本地格式，与 md/txt 同类（code 也因同因并入 core）；插件通道只能传 UTF-8 文本，TODO/优先级/标签/计划日期的着色与层级信息在 markdown 转换中丢失，阅读质量降级，而折叠仍必须改 host——两头不讨好。

## 1. 现状接线（改动的锚点）

```
CLI args / 文件树
  → plugin_catalog::claims（url/hn/dict/epub 按参数/扩展名认领，.org 无主）
  → open → source: Box<dyn ContentSource>
      FileTreeSource（本地 md/txt/mdx/scripts）
        load(index, width):
          md   → md::render_md_doc(text, w)      → RenderedDoc{lines, links, headings}
          code → syntect→HTML → loader::render_plugin_body_doc → (lines, links, headings)
          其它 → md::render_txt_width → outline_from_plain_lines
  → App：
      body: Vec<Line>           （渲染缓存）
      LoadResult.links          （f / 链接跳转 link_at）
      LoadResult.headings       （o 大纲跳转、Alt+f）
      搜索 find_hits(body)      （/ 增量、visual、avy）
      status / AI (plain_body)  （复用）
```

关键事实：`App` 已消费 `headings: Vec<HeadingEntry{level,text,line}>` 做大纲跳转——org 的标题层级天然落进这个结构，折叠可以直接建立在它之上。

## 2. 方案 A：core org 渲染后端

### 2.1 新增 `src/org.rs`（核心）

接口与 `md.rs` 对齐：

```rust
/// org → RenderedDoc（行 + 链接索引 + 标题索引），word-wrap 语义同 md。
pub fn render_org_doc(text: &str, width: usize) -> RenderedDoc;
pub fn render_org_width(text: &str, width: usize) -> Vec<Line<'static>>; // 便捷
```

实现：`Org::parse` + 遍历事件（orgcat 已验证的全部逻辑，从 `orgcat/src/main.rs` 移植并适配 ratatui Span）：

| org 元素 | 渲染 |
|---|---|
| 标题行 | 缩进 `2×(level-1)` 空格（层级靠缩进区分）+ 星号×level dim + 标题文字按 **6 级调色板**（orgcat `HEADING_COLORS`，h1 青→h6 红，7 级封顶） |
| TODO/DONE | 关键词黄/绿粗体（其他状态紫） |
| 优先级 `[#A]` | A 红、其余紫 |
| 标签 `:a:b:` | 青 |
| SCHEDULED/DEADLINE/CLOSED | 蓝 |
| FILETAGS（文件级） | 顶部 `(文件标签: …)` dim 行 |
| 粗体/斜体/下划线/删除线/等宽/代码/上下标 | 语义色 + 标记（span-depth 防覆盖，orgcat 已验证） |
| 链接 `[[url][desc]]` | 着色 + 进 `LinkEntry`（`f` 跳转） |
| 表格 | markdown 对齐 + CJK 宽度（unicode-width）+ 超长截断（orgcat `render_table` 移植） |
| 列表 `- + 1.` | 符号 dim，缩进按 indent |
| `#+BEGIN_SRC` 代码块 | 围栏 dim + 内容原样（v1；syntect 高亮=stretch） |
| `#+BEGIN_QUOTE` | `> ` 前缀 dim |
| `:PROPERTIES:` 抽屉 / `#+` 关键字行 | 跳过不渲染（元数据，orgcat 已定） |

`headings` 产出：每标题 `HeadingEntry{level, text(纯文本), line}`——`o` 大纲/`Alt+f` 零改动复用。标题行号映射用 `heading.syntax().text_range().start()` 或遍历计数器（实现时定，保证与渲染行对齐）。

### 2.2 挂载改动（3 处小改 + 0 处大改）

1. `src/scan.rs::is_doc`：扩展名白名单加 `org`（`.org` 归入"文档"，与 md/txt 并列；不进 code）。
2. `src/source.rs::FileTreeSource::load`：新增分支
   ```rust
   } else if ext.eq_ignore_ascii_case("org") {
       let doc = org::render_org_doc(&text, w);
       LoadResult {
           status: format!("{name}  ({} lines)", doc.lines.len()),
           lines: doc.lines,
           links: doc.links,
           headings: doc.headings,
       }
   }
   ```
   文件树扫描、相对链接 `ensure_local_doc`、`plain_body`（AI/剪贴板）自动生效——`ContentSource::plain_body` 默认实现走 `load()`，org 分支天然支持。
3. `src/md.rs`：`RenderedDoc` 已是 `pub`（source.rs 在用），org.rs 直接复用；若否，把 `RenderedDoc`/`HeadingEntry`/`LinkEntry` 提到共享模块（设计定案：检查后决定，倾向提 `plugin.rs` 旁的新 `doc.rs` 共享类型——见风险 2）。

CLI 入口无需改：`tuider file.org` 走 `open_files` 的现有路径；`plugin_catalog` 不认领 `.org`，无冲突。

### 2.3 折叠（通用 App 能力，org 的杀手锏）

不锁死在 org 上：基于 `LoadResult.headings` 的"大纲折叠加法"对所有源生效（md 也能折叠，接近 org 的 TAB 体验）。

- **状态**：`App` 新增 `collapsed: Vec<bool>`（与 `headings` 等长，默认全展开）+ `fold_active: bool`。
- **可见性**：纯函数 `fn fold_visible(headings, collapsed, line) -> bool`——行号在某标题区间内且该标题无 collapsed 祖先则为可见。渲染时对 `body` 做一次行过滤（App 每次 draw 前算 `display_body`，过滤是 O(n) 拷贝，万行级无压力）。
- **键位**（app/keys.rs，`handle_body_motion`/`handle_normal_key` 加分支）：
  - `z`：折叠/展开当前游标所在标题（游标行 → 所属 heading 索引 → toggle）
  - `Z`：循环 全收（只留 1 级）→ 全展 → 恢复
  - `Enter` 进入被折叠区间：自动展开其祖先链（可后置，v1 允许滚动即展开——实现时取最简：跳到折叠区时全展祖先）
- **与搜索的交互**：`find_hits` 基于完整 `body`（不被折叠遮挡）；命中跳转时 `fold_visible` 由 `o`/搜索跳转自动展开祖先（开放：实现时先做"跳转即展开"，展开逻辑 5 行）。
- **状态栏**：游标行所属 heading 面包屑（`headings` 倒查），org 阅读刚需。
- **测试**：折叠可见性是纯函数，直接单测（折叠 1 级隐藏整棵 2 级子树、展开恢复、区间边界）。

### 2.4 测试与验收

- 单测：`scan::is_doc(".org")==true`；`source.rs` org 分支产出的 headings 数量/层级与样例一致；`org.rs` 渲染断言（TODO 黄、DEADLINE 蓝、表格对齐含 CJK、链接进 LinkEntry）；折叠纯函数。
- 冒烟（AGENTS.md 最小案例）：`tuider /tmp/demo.org`（3 标题+标签+DEADLINE）→ 大纲跳转 `o`、链接 `f`、折叠 `z` 后行数减少、搜索命中；`tuider /root/cleantest/clean-Taskwarrior/20260917T101118--taskwarrior__cml.org`（FILETAGS+五级树）→ 折叠顶层、状态栏面包屑、AI 面板取段落。
- 回归：md/txt/epub 路径不受影响（现有 smoke 脚本全绿）。

## 3. 方案 B（备选）：org 插件

- 新 crate `crates/tuider-plugin-org`（cdylib），`plugin_catalog` 加 claims（`.org` 结尾 / `-org`），导出 `load_body` = orgize → markdown（含自研表格修复）。
- 优点：host 零渲染代码、符合"本体尽量小"哲学、epub 同款范式。
- 代价（决定性问题）：
  1. ABI 只传 UTF-8 文本 → TODO/优先级/标签/计划日期无 markdown 载体，全部丢失样式（可 hack：转成 `**` 前后缀假样式，污染语义）；
  2. orgize `MarkdownExport` 上游不渲染表格/丢时间戳（orgcat 已实测并自修）——修复代码放插件 crate，host 侧无法复用到；
  3. 折叠仍需 host 通用能力（2.3 无论如何都要做）；
  4. 本地纯文本格式做成插件 = 每次改动重新 `cargo build -p ... && cp .so`，而 md/txt/code 都在 core——org 归属混乱。
- 结论：**除非未来 org 解析器体积爆炸，否则不值**。

## 4. 里程碑

- **M1（渲染挂载）**：`org.rs` + `scan.rs` + `source.rs` → `tuider x.org` 可读、大纲/链接可用。单测+冒烟。
- **M2（折叠）**：通用折叠状态 + `z`/`Z` + 跳转展开 + 状态栏面包屑。纯函数单测 + TUI 冒烟。
- **M3（打磨）**：代码块 syntect 高亮（stretch）、折叠-搜索交互细节、light 主题对照、CHANGELOG/帮助文案。

## 5. 风险登记

1. **orgize alpha API**：已在 orgcat 全量验证（标题/INLINE/PLANNING/表格/FILETAGS），风险低；锁定 `=0.10.0-alpha.10`。
2. **共享类型归属**：`RenderedDoc` 目前在 md.rs（pub），org.rs 跨模块引用会制造 md↔org 耦合；倾向新建 `src/doc.rs` 收 `RenderedDoc/LinkEntry/HeadingEntry`，md/html/org 三方引用——若改动面大（loader.rs 也用），退化为"org.rs 引用 md.rs 类型"最小改。
3. **行号对齐精度**：orgize 遍历事件与渲染行可能因 wrap 错位（表格多行、长行 wrap）；headings.line 必须在渲染产出时记录（渲染器内计数器），不能事后从语法树换算——设计已按"渲染时记录"定案。
4. **折叠与搜索/跳转语义**：v1 采用"跳转即展开祖先"，避免"命中在看不见的地方"；若体验差再引入"折叠区可穿透"。
5. **大文件**：`display_body` 每帧过滤 O(n)（万行内可忽略）；不需要增量渲染。

## 6. 待你拍板的三个点

1. 方案 A（core）确认？—— 上文已论证，默认 A。
2. 折叠做成**通用**（md/org/html 都能折）还是**仅 org 启用**？（默认通用，实现成本相同）
3. M3 的代码块 syntect 高亮：本期做还是 stretch？（默认 stretch，控制范围）