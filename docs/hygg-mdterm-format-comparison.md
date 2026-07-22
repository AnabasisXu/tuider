# hygg / mdterm 与 Tuider：格式与 MD 渲染对照

**日期：** 2026-07-21  
**状态：** 调研结论（决策记录，非实现任务）  
**范围：** 对标 [kruseio/hygg](https://github.com/kruseio/hygg)、[bahdotsh/mdterm](https://github.com/bahdotsh/mdterm)；指导 Tuider 后续取舍。

---

## 1. 结论摘要

| 问题 | 答案 |
|------|------|
| MD 渲染跟谁？ | **mdterm**（Tuider `src/md.rs` 已沿此路线） |
| 阅读壳跟谁？ | **hygg**（进度 / 书签 / 选区 yank） |
| 最主要从 hygg 学的是 EPUB 吗？ | **不是**。主学阅读壳；EPUB 只是格式侧最划算的可选项 |
| EPUB 工程量大吗？ | **不大**（~200 行级 + feature） |
| DOCX 工程量大吗？ | glue **小**，产品债 **大**（外挂 pandoc；默认不做） |
| 默认 binary | 仍仅 **md + txt**；EPUB/DOCX/PDF 不进 core |

---

## 2. 三方定位

| | hygg | mdterm | Tuider（目标） |
|--|------|--------|----------------|
| 一句话 | 多格式 → 纯文本 TUI 阅读器 | 终端 **Markdown 浏览器** | slim 终端阅读器；MD 结构化渲染 + 插件 |
| MD | 基本 **不渲染**：plain + justify | **专用渲染**（AST + 样式 + 扩展） | 跟 mdterm 的结构渲染；不 plain 化 |
| 格式广度 | 宽（PDF/EPUB/pandoc…） | 窄（md + json） | 默认窄；格式用 feature 后置 |
| 阅读壳 | 强（进度/书签/高亮/sync） | 弱（会话内导航为主） | 壳能力可吸收 hygg；与格式正交 |

---

## 3. Markdown 渲染：mdterm vs hygg

### 3.1 管线

```text
mdterm:  .md → pulldown-cmark → 结构化 Line(+meta) → TUI
hygg:    .md → pandoc --to=plain（或当文本）→ justify(col) → 阅读器
Tuider:  .md → pulldown-cmark → ratatui Lines（已实现基础集）
```

hygg 源码路径（`packages/hygg/src/input_pipeline.rs`）：

- `.txt` 直读  
- `.epub` → `cli_epub_to_text`  
- `.pdf` → `cli_pdf_to_text`  
- **其余（含 `.md` / `.docx`）** → `pandoc --to=plain` 优先  

**hygg 没有 MD AST，没有标题色阶 / fence / 表格语义渲染。**  
比「谁 MD 更强」时 **mdterm 碾压**；hygg 赢的是长文对齐与多格式壳，不是 MD。

### 3.2 能力对照

| 能力 | mdterm | hygg |
|------|--------|------|
| 标题层级样式 | 有 | 无（plain） |
| 粗斜体 / 删除线 | 有 | 丢失 |
| 列表 / task list | 有 | 残留字符 |
| 表格 | 有结构样式 | plain 后易烂 |
| 代码块高亮 | syntect | 无 |
| 引用 bar | 有 | 无 |
| 链接 / 图片 | OSC8、图协议等 | MD 路径无 |
| Mermaid / Math | 有 | 无 |
| TOC / 按标题跳 | 有（LineMeta） | 无结构 |
| 长文两端对齐 | 弱 | **强** |
| 进度 / 书签 | 弱 | **强** |

### 3.3 Tuider 已吸收 / 明确不做

已从 mdterm 吸收（见 `src/md.rs`）：

- pulldown-cmark 事件流  
- 标题色阶、H1 分隔  
- code fence + 语言标签  
- tables / task lists / strike / quote bar  
- 宽度感知 wrap  

**core 不做（YAGNI）：** syntect、mermaid、math、终端图片协议。

**禁止从 hygg 学的 MD 路径：** `MD → plain → justify` 会毁掉结构渲染。  
justify 若需要，仅考虑 **txt** 可选，不对结构化 md 正文做报纸两端对齐。

### 3.4 仍可从 mdterm 按需吸收（非 core 必须）

按 ROI：

1. `LineMeta::Heading` → TOC / `[` `]` 跳节（最高）  
2. 代码块元数据 → 复制整个 fence  
3. 链接列表 / 本地 `.md` 跳转  
4. syntect / mermaid / math / 图片 → 重，用户真要再开  

---

## 4. 从 hygg 真正该学什么

### 4.1 主优先级：阅读壳（与格式无关）

1. **阅读进度持久化**（路径 + 行/偏移，再开跳回）  
2. **书签**（可选：高亮）  
3. **选区 + yank**（给 AI / shell）  
4. 最近打开 / start screen（目录扫已有一半基础）  

这些是「最主要从 hygg 学习」的内容，**不是 EPUB**。

### 4.2 次优先级：多格式接入策略

| 格式 | hygg 做法 | 工程量 | Tuider 建议 |
|------|-----------|--------|-------------|
| **EPUB** | 原生小库 ~200 行：`epub` + `html2text`，按 spine 拼章 → plain → justify | **小** | 可选 `feature = "epub"`；默认关。MVP 可整书一条；进阶 spine = 侧栏章节 |
| **DOCX** | 外挂 **pandoc**，不进 Rust 树 | glue 小 | **默认不做**；若要则 `feature = "pandoc"` 统一 docx/odt/rtf，不自研 OOXML |
| **PDF** | 自研重栈 + 可选 OCR + 图 | **大** | 第一期及可预见期内 **不做** |
| **MD** | plain 化 | — | **不学** |

### 4.3 EPUB 细节（后置规格草案）

- 依赖倾向：`epub` + `html2text`，或 html→md 复用 url 插件思路再喂 `render_md`  
- 边界：忽略 DRM / 复杂 CSS / 插图（MVP）  
- UI：先单条目全文；需要时再按 spine 拆 `ContentSource` 条目  
- **不**默认扫目录里的 `.epub`（避免 core 行为膨胀）；仅显式打开或 feature 扫描策略另定  

### 4.4 DOCX 细节（后置）

```text
pandoc --to=markdown|plain --wrap=none -- <file>
```

- 无 pandoc → 明确错误 + 安装提示  
- 不进 default-features  
- 不承诺 Word 版式保真  

---

## 5. 错误排序纠正

| 说法 | 对错 |
|------|------|
| 「从 hygg 最该学的是 EPUB」 | **错**。主学阅读壳 |
| 「EPUB 是格式侧最值得抄的」 | **对**。小、纯 Rust、无 pandoc |
| 「DOCX 太重做不了」 | **半对**。代码不重，外挂与体验债重 |
| 「hygg MD 渲染比 mdterm 好」 | **错**。hygg 几乎不做 MD 渲染 |

---

## 6. 决策（拍板）

| # | 决策 |
|---|------|
| C1 | MD 渲染继续 **mdterm 路线**；禁止 hygg plain 化 |
| C2 | 从 hygg **优先吸收阅读壳**（进度 / 书签 / yank），与格式插件解耦 |
| C3 | **EPUB / DOCX / PDF 均非当前 core**；EPUB 为格式扩展第一候选 feature |
| C4 | DOCX 仅允许 pandoc 可选 feature，禁止自研解析进树 |
| C5 | syntect / mermaid / math / 图片 / PDF 保持 YAGNI，直至明确需求 |

---

## 7. 相关文档

| 文件 | 关系 |
|------|------|
| [PLAN.md](PLAN.md) | 产品总览；本文件为对标补充 |
| [superpowers/specs/2026-07-21-tuider-reader-plugins-design.md](superpowers/specs/2026-07-21-tuider-reader-plugins-design.md) | 插件架构规格 |
| [ai-cli-doc-readers.md](ai-cli-doc-readers.md) | AI 共读对标；亦写明一期不上 PDF/EPUB 解析 |
| `src/md.rs` | 已实现的 mdterm 风格渲染 |

---

## 8. 源码锚点（调研时）

- hygg 输入：`packages/hygg/src/input_pipeline.rs`（`read_content_without_ocr` / `pandoc_to_text`）  
- hygg EPUB：`packages/cli-epub-to-text/src/lib.rs`（`epub_to_text` / spine + `html2text`）  
- hygg 文档：`docs/pages/detailed-installation.md`（pandoc 附加格式）  
- mdterm 渲染：`src/markdown.rs`（`render_with` + `ENABLE_TABLES|TASKLISTS|STRIKETHROUGH|MATH`）  
