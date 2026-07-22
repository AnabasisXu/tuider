# 能和 AI 对话的命令行文档阅读器（调研）

日期：2026-07-21  
用途：Tuider `ai` 插件 / 产品对标参考

## 首选（真正的「阅读器 + AI」）

### 1. Gorae — 最贴题

- 仓库：https://github.com/Han8931/gorae
- 形态：Go TUI 知识库 / 文档馆 + 内置 AI 共读
- 格式：PDF / EPUB / Markdown
- 能力：
  - Vim 式浏览、全文搜索、阅读状态、标签
  - 内置 RAG：整库问答、摘要、语义检索
  - 模型：Ollama / OpenAI / 任意 OpenAI-compatible
- 安装：release 二进制

```sh
gorae
# TUI 内
:index    # 建索引
:gorae    # 打开 AI 对话
```

对 Tuider 的启发：阅读库 + `:chat` 浮层；本地索引 + BYO model。

### 2. aichat — 最成熟的 CLI RAG

- 仓库：https://github.com/sigoden/aichat
- 形态：通用 LLM CLI（REPL + RAG + tools），不是翻页阅读器
- 能力：内置向量库 + 全文检索；PDF/MD 等 loader；多模型
- 用法：

```sh
aichat
# REPL 内
.rag mydocs
# 添加文档: notes/*.pdf; docs/*.md
```

对 Tuider 的启发：AI 不必自建向量库，可 shell out / 对接 aichat；或只做「当前打开文档 + 选中段落」上下文。

### 3. Cliven — 纯本地 PDF 对话

- 仓库：https://github.com/krey-yon/Cliven
- 形态：Python CLI
- 栈：PDF → chunk → ChromaDB → Ollama
- 特点：数据不出机；星少、功能窄，路径干净

```sh
pip install cliven
cliven docker start
cliven ingest paper.pdf
cliven chat
```

对 Tuider 的启发：`ai` feature 第一版可只做「当前文件上下文聊天」，不做全库 RAG。

---

## 备选

| 工具 | 形态 | 说明 |
|------|------|------|
| [BookWith](https://github.com/shutootaki/bookwith) | Web/PWA 电子书 | EPUB + 上下文感知 AI；非纯 CLI |
| [llm](https://github.com/simonw/llm) | CLI | 通用 LLM + 插件 RAG，需自己拼阅读流 |
| [karpathy/reader3](https://github.com/karpathy/reader3) | 自托管 EPUB | 按章读，方便拷章节给 LLM |
| pdftotext / Glow + aichat | 组合拳 | 阅读与 AI 分离 |

---

## 怎么选（对用户）

| 需求 | 选 |
|------|-----|
| 终端「翻库 + 问文档」一体 | **Gorae** |
| 已有文档，只想 REPL 问答 | **aichat** |
| 只要本地 PDF + Ollama | **Cliven** |
| EPUB 伴读 | BookWith（非纯 CLI） |

---

## 对 Tuider 的简短结论

1. **产品定位**：Gorae 是最接近的「TUI 阅读库 + AI」对标；Tuider 默认更 slim（md/txt，AI 为 feature）。
2. **`ai` 插件最小路径**：当前打开文档（+ 可选选中区）作上下文 → OpenAI-compatible / Ollama 流式对话浮层。全库 RAG 后置。
3. **不必重造**：重度 RAG 可文档引导用户用 aichat；Tuider 只做阅读态共读。
4. **不做**：第一期不上 PDF/EPUB 解析、不上内置向量库（YAGNI）。

## 参考链接

- Gorae 介绍：https://han8931.github.io/gorae
- aichat RAG：https://github.com/sigoden/aichat/wiki/RAG-Guide
- BookWith HN：https://news.ycombinator.com/item?id=44811387
