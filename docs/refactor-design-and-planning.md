# mdx-tui → Tuider：重构设计与规划

> 日期：2026-07-22  
> 类型：方法论文档（非实施 plan）  
> 背景：将 mdx-tui 重构为 tuider——终端阅读器为产品核心，MDX/词典仅为插件。  
> 动机：用 OMP/agent 写了大量难读的 spec，最终仍靠修修补补；本文整理更专业的重构做法与对本仓库的落地建议。

---

## 1. 诊断：哪里已经对了，哪里在空转

### 1.1 对的部分

| 决策 | 为什么合理 |
|------|------------|
| 新产品 `tuider`，冻结 `mdx-tui` | 避免在旧神对象上边砍边加，降低冲突 |
| 核心是**终端阅读**，MDX 是插件血统 | 把「产品中心」从词典挪到阅读路径 |
| 用 `ContentSource` / body 文本契约收口 | 这是正确的 seam（接缝） |

### 1.2 翻车模式（文档和代码里可见）

1. **决策反复写进多份「权威」文档，互相打架**  
   - `PLAN.md` / core-roadmap 一度写：**Cargo features + 静态 trait，非动态 `.so`**。  
   - `STATUS.md` / `plugins.md` / `README` 又写成：**方案 A = 动态 `.so`**。  
   - `complexity-review.md` 还写着 *Rejected alternative: 动态 dlopen*。  
   同一棵树里同时存在「拒绝 so」和「已经是 so」——agent 会继续为旧决策写新 spec，人只能靠补丁对齐现实。

2. **规格量远超决策量**  
   `docs/superpowers/` 下 specs + plans 体量很大（曾约 2000 行量级），其中 host-boundary plan 可长达数百行并塞满可粘贴实现。  
   这不是设计，是**预写实现却没有行为锁**——读不懂很正常。

3. **「插件化」被提前做成了平台**  
   对「同仓、自写、Linux TUI」场景，第一目标应是**编译期边界**（trait + 可选依赖），不是**运行时 ABI**。  
   Rust 社区对动态插件的共识是：`cdylib` + C ABI 可行，但代价是 ABI 稳定、分配器、符号、版本协商、调试难度；适合「第三方分发插件」，不适合「先把 god-app 拆开」。

4. **复杂度仍漏在 host**  
   host 仍可能持有 HTML/CSS 渲染路径；dict 用 `TUIDER_HTML_V1` 信封把 HTML/CSS 渲染留在本体。  
   文档说「插件把复杂性拉下去」，实现却是「插件吐 HTML，host 继续当浏览器」。这和 Ousterhout 的 *pull complexity down* 相反。

5. **OMP / superpowers 的失败形态**  
   brainstorm → 长 design → 更长 plan（逐步 checkbox + 完整代码）→ 子 agent 按步骤打补丁 → 绿了但架构叙事过期。  
   结果：**看不懂的 spec + 只能修修补补**。

---

## 2. 专业重构怎么做（不是再写一份超长 plan）

### 2.1 三种策略，先选对层

业界对「从旧系统抽新产品」通常只有三类：

| 策略 | 含义 | 本仓库场景 |
|------|------|------------|
| **Big-bang rewrite** | 停旧、全新写、最后切换 | 风险大；半条腿重写后仍靠补丁收尾 |
| **Strangler Fig**（Martin Fowler） | 新旧并行，按入口/能力逐条接管，旧系统逐步被「勒死」 | **最适合**：`mdx-tui` 当 legacy，`tuider` 当新宿主 |
| **In-place modularization** | 仍在旧仓库里切 crate / trait | 适合 mdx-tui 内部清债，**不适合**改产品定位 |

Strangler 的关键不是「写更多文档」，而是：

1. 立一个**稳定的接缝**（facade / trait / CLI 入口）  
2. **一次只迁一条用户可感知能力**  
3. 每条能力有 **characterization tests**（锁定旧行为）  
4. 迁完就删旧路径，文档只记**当前真源**

参考：

- Strangler Fig 增量现代化（相对 big-bang 的价值交付与风险控制）  
- Shopify 等对 legacy 的 strangler 实践  
- Michael Feathers《Working Effectively with Legacy Code》：先 characterization test 再动刀  

### 2.2 插件机制：两阶段，不要一步到位

```
阶段 0（工程边界，默认推荐）
  host + trait ContentSource
  dict/url/hn/code = workspace 内 crate
  Cargo features 控制是否编进 bin
  同一 rustc、同一类型、可单测、可 step-debug

阶段 1（分发边界，仅在真有需求时）
  同一 trait 的 C ABI 投影
  cdylib + libloading
  需要：ABI 版本、string free、allocator 约定、安装路径
```

**判定要不要上 so：**

- 要「用户拷贝一个文件就扩展功能、主包永远不链业务」→ so 有意义。  
- 要「架构上词典不是核心」→ **features 就够**，复杂度低一个数量级。  
- 两者都要：先完成阶段 0 且 trait **极窄**，再机械投影到 so，而不是边迁业务边发明 ABI。

已付代价的典型形态：ABI v1、HTML_V1 信封、catalog 兜底 claims、pkg install——这些都是**平台税**，在 dict 渲染是否该在 host 都没定清时就先交了。

### 2.3 深模块原则（设计检验，不是写长文）

对每个边界问一句：

> 模块价值 ≈ 提供的功能 / 接口复杂度  

| 模块 | 应是深还是浅 | 风险 |
|------|--------------|------|
| host | 打开源 + 跑 TUI + 搜选区 +（可选）AI | 键位/多模式路由仍可能过宽 |
| plugin-api | 标题 / 条目 / 取 body | HTML_V1 把 CSS 引擎绑死在 host |
| dict 插件 | mdict + HTML→终端行 | 若渲染在 host，dict 就浅、host 就胖 |

**Verdict 模板（比长 spec 有用）：**

```
Verdict: deepen | merge | split | simplify-interface | leave
Evidence: <具体泄漏点>
Change: <1–3 条最小动作>
Rejected: <备选为何不做>
```

---

## 3. 建议的目标架构

```
                    ┌─────────────────────────────┐
  CLI / 扫描  ───►  │  host shell                 │
                    │  app (keys/nav/search/visual)│
                    │  ui · theme · md(plain/md)   │
                    │  ai (product core, feature)  │
                    └─────────────┬───────────────┘
                                  │ dyn ContentSource
                    ┌─────────────▼───────────────┐
                    │  source contract (窄)        │
                    │  title / entries / load(i)  │
                    │  body: Plain | Md | Rendered │
                    └─────────────┬───────────────┘
           ┌──────────┬──────────┼──────────┐
           ▼          ▼          ▼          ▼
        FileTree    url        hn         dict
        (core)    (plugin)  (plugin)   (plugin)
                                   复杂性在这里：
                                   mdict + HTML/CSS→Lines
```

**关键设计选择（写进一页决策表即可）：**

1. **Body 契约**  
   - 推荐：插件返回**已排版的终端语义**（Lines / 带 span 的中间 IR），或至少 **Md/Plain only**。  
   - 不推荐长期：host 内嵌半套 CSS 引擎只为 dict。

2. **AI 是 core overlay**  
   - 不是 `ContentSource`；是「当前源旁边的会话」。  
   - core-roadmap 中的定位应保留。

3. **mdx-tui 角色**  
   - 行为金标 + 可移植算法源（render/theme/键位）。  
   - 不是并行产品演进轨；能力迁完再归档。

---

## 4. 可执行路线图（Strangler 切片）

每一切片：**用户故事 + 行为锁 + 入口路由 + 删旧/标废弃 + 一页 STATUS 更新**。  
禁止：切片里同时改 ABI、渲染引擎、键位状态机。

### 切片 0 — 冻结真相（1 次会话）

- 选**唯一真源**：`docs/STATUS.md`（现状）+ 一页决策表（仅拍板项，可放 `docs/DECISIONS.md` 或 ADR）。  
- 把矛盾文档标 `historical/` 或页首写 **SUPERSEDED**，禁止 agent 再当权威。  
- 明确当前实际架构：so / features / 混合。

### 切片 1 — Characterization harness（先测后迁）

对 mdx-tui 与 tuider 各固定一组金样，例如：

- `tuider README.md`：侧栏条目、正文前 N 行  
- 单文件 / 目录 / `-l`  
- 若有 so：url 一个域名、dict 一个小 mdx fixture  

**先锁「现在长什么样」**，再改实现。没有这层，agent 只能补丁撞绿。

### 切片 2 — Core reader 变「深」

- 默认 bin：md/txt + 搜索 + visual + AI（无 key 可降级）  
- 默认依赖树**禁止** mdict / readability  
- 验收：`cargo tree` / 体积 / `--no-default-features`  

### 切片 3 — 第一个插件用最窄路径（建议 url 或 hn）

- 只证明：`args → open source → entries → body(md)`  
- 不要 HTML_V1、不要 pkg 市场、不要 claims 双路径  
- features 先通；若坚持 so，**同一 trait 投影**，业务零分叉  

### 切片 4 — dict 真正下沉

- 从 mdx-tui 移植的是 **dict 域知识**（加载、lookup、HTML 策略），不是整 App  
- **HTML/CSS→Lines 进 dict 插件**（或独立 `tuider-render-html` 仅 dict 依赖）  
- host 只认：`RenderedBody` 或 md；删除 host 内 dict 专用泄漏  

### 切片 5 — 再谈 so 分发与 pkg

- 仅当切片 3–4 的 trait 稳定一段时间、且真需要「不重编主 bin」  
- 再冻结 ABI（试验期 ABI 允许 breaking 一次）

### 明确不做（防 agent 发散）

- 同时重写视觉语言 + 插件 ABI + app 状态机  
- 把 AI 再拆成 so  
- 在 mdx-tui 上继续加 HN/URL 功能  
- 为「将来可能」加 pass-through 层  

---

## 5. 文档该怎么写（治「看不懂的 spec」）

把 superpowers 流水线**收窄**，否则会复现 agent 失败：

| 文档类型 | 上限 | 内容 |
|----------|------|------|
| **ADR / 决策** | 半页/条 | 背景、选项、决定、后果 |
| **STATUS** | 1–2 页 | 现在能跑什么、目录结构、已知债 |
| **切片规格** | ≤1 页 | 目标/非目标、验收命令、行为冻结点 |
| **实现 plan** | 可逐步，但**不要**贴整文件实现 | 文件路径 + 测试命令 + 完成定义 |

**给 agent 的工作协议（比长 plan 更有效）：**

```
1. 只改本切片列出的路径
2. 先跑 characterization / cargo test，记录基线
3. 最小 diff 使验收绿
4. 更新 STATUS 一节（删过期句子，不追加新史诗）
5. 禁止新建第三份「权威架构」
```

**反模式黑名单：**

- 在 plan 里写完整模块实现全文  
- 同一决策在 PLAN / roadmap / complexity-review 各写一遍且日期不同  
- 「行为冻结」却改 CLI 文案与错误路径却不更新金样  
- 用 complexity-review 的 *leave* 掩盖未拆的 host 胖模块  

---

## 6. 针对现状的直接建议

当前状态可概括为：

> **产品叙事超前，机制（so）超前，行为锁与复杂度下沉落后；文档过期堆叠。**

务实排序：

1. **停写新史诗 spec**；整理真源（STATUS + 决策表）。  
2. **定 body 契约**：dict 渲染进插件还是 host——这是最大架构分叉，必须人拍板。  
3. **so vs features**：若目标是「自己用、架构干净」→ 暂时把 so 降为实现细节，先保证 trait 边界；若目标是「小主包 + 拷贝扩展」→ 接受 ABI 税，但**先瘦 host**。  
4. **按切片 2→3→4 推进**；每切片可演示，不攒大爆炸。  
5. mdx-tui **只读参考**；移植算法时带着 characterization，而不是整仓抄再改名。

---

## 7. 一句话总结

**专业做法不是让 AI 写更完整的 spec，而是：Strangler 薄切片 + characterization 锁行为 + 深模块接缝 + 文档只保留一份真源。**

「词典变插件」是正确的产品重构；「先发明动态插件平台再补丁对齐」是错误的工程路径。

---

## 8. 待拍板（下一步半页决策，不写长 design）

在开下一刀实现之前，只需定三件事：

1. **Body**：插件返回 **Rendered** 还是 host 继续吃 **HTML_V1**？  
2. **机制**：接下来 2–4 周 **features-first** 还是 **so-first**？  
3. **下一切片只选一个**：`core 依赖面验收` / `url 最窄插件` / `dict 渲染下沉`。

拍板后：写**一页**切片规格（目标/非目标/验收命令/可改路径），再实现；不要再开 800 行 plan。

---

## 9. 相关文档索引

| 文件 | 角色 |
|------|------|
| [STATUS.md](STATUS.md) | 建议作为**现状**真源 |
| [NEXT.md](NEXT.md) | 下一步清单 |
| [PLAN.md](PLAN.md) | 历史决策总览（若与 STATUS 冲突，以 STATUS + 本文 §8 拍板为准） |
| [plugins.md](plugins.md) | 当前 so 加载说明 |
| [complexity-review.md](complexity-review.md) | 复杂度审查（注意可能过期） |
| `superpowers/specs/*` | 历史规格；矛盾项应标 SUPERSEDED |
| `~/cleantest/mdx-tui/` | 冻结参考与行为金标来源 |
