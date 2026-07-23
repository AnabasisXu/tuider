# Tuider ↔ mdx-tui 对比工作流设计

> 日期：2026-07-23  
> 状态：draft → 待用户审文件  
> 背景：Tuider 从 mdx-tui 吸收能力并做方案 A 等改动；现有 `docs/mdx-tui-unimplemented-gap.md` 是单向 gap backlog，且 A–E 已勾完，**不能**再当双向/可复跑权威。

## 1. 目标

经常对比 **tuider** 与 **mdx-tui**：

1. 一张矩阵看清：`parity` / `intentional-diff` / `tuider-only` / `gap`
2. 一套 CLI smoke：同一 case 两边跑，stdout/exit 可 diff
3. 与 STATUS / plugins / 旧 gap 文档职责不打架

**成功标准：**

- 改 dict/AI/插件后，10 分钟内能复跑相关 case 并更新矩阵一行
- 新同事只读 `docs/compare/README.md` 会用
- 无 dict / 无网时脚本 SKIP 不炸

## 2. 非目标

- 自动回写 MATRIX 勾选
- TUI 键位录制 / 像素对比
- 修改 mdx-tui 源码（金标只读）
- CI 门禁（可后置；本设计不要求）
- 完整 CSS 引擎等 OUT 项重新争论（矩阵标 intentional 或 OUT 指针）

## 3. 产物

| 路径 | 角色 | 维护 |
|------|------|------|
| `docs/compare/README.md` | 怎么用（短） | 手维 |
| `docs/compare/MATRIX.md` | **功能对比权威** | 手维 |
| `docs/compare/SMOKE.md` | case 人类说明 | 手维 |
| `docs/compare/cases.tsv` | 脚本唯一输入 | 手维 |
| `scripts/compare-mdx-tui.fish` | 执行对照 | 手维 |
| `docs/compare/runs/<ts>/` | 输出产物 | gitignore 推荐 |

**指针（实现阶段改，不复制状态）：**

- `README.md` 文档表加 compare 一行
- `docs/STATUS.md` / `docs/NEXT.md` 链到 MATRIX
- `docs/mdx-tui-unimplemented-gap.md` 顶栏：历史 backlog → MATRIX

## 4. 环境契约

| 变量 | 默认 | 说明 |
|------|------|------|
| `MDX_TUI_BIN` | `~/cleantest/mdx-tui/target/release/mdx-tui`，无则 debug | 金标二进制 |
| `TUIDER_BIN` | 本仓 `target/release/tuider`，无则 debug | 被测 |
| `TUIDER_PLUGINS_DIR` | 调用方或本机已装插件目录 | dict/url/hn/code |
| `COMPARE_DICT` | 无默认；未设则 `need` 含 `dict` 的 case **SKIP** | 小 mdx 路径或目录；README 给 export 示例 |
| `COMPARE_GROUP` | 可选 yml group 名 | `-g` 场景 |
| `COMPARE_URL` | `https://example.com` | url case |
| `COMPARE_RUN_DIR` | `docs/compare/runs/<ts>` | 可覆盖 |

缺 bin → 脚本失败并打印如何 build。  
`need=dict` 且无 so/无 `COMPARE_DICT` → **SKIP**。  
`need=net` 失败可标 FAIL 或 SKIP（实现选：默认尝试，超时 FAIL；`--offline` 全 SKIP net）。

## 5. MATRIX schema

### 5.1 列

| 列 | 含义 |
|----|------|
| `id` | 稳定 ID。优先复用 gap 旧 ID（`D-CLI-01`、`AI-05`…）；Tuider 独有用 `T-` 前缀（`T-CODE-01`、`T-SO-01`） |
| `domain` | `core` \| `dict-cli` \| `dict-tui` \| `ai` \| `html` \| `hn` \| `url` \| `code` |
| `capability` | 一句话 |
| `mdx-tui` | 金标行为（短） |
| `tuider` | 当前行为（短） |
| `status` | 见 5.2 |
| `note` | intentional：原因 + 是否冻结；gap：落点；tuider-only：超集理由 |
| `smoke` | `cases.tsv` 的 `case` id；TUI-only 空 |
| `updated` | `YYYY-MM-DD` |

### 5.2 四态（互斥）

| status | 定义 |
|--------|------|
| `parity` | 同输入行为可互换。允许 SMOKE 声明的无关差（ANSI、绝对路径） |
| `intentional-diff` | 已知且**接受**的差异（方案 A so、HN `-l` 标题行、`a` vs `o` 等） |
| `tuider-only` | mdx-tui 无对等能力 |
| `gap` | mdx-tui 有、Tuider 未对等，且**未**接受为 intentional |

**禁止：** 同一 id 多 status；用「部分实现」糊成 parity——应 `gap` 或拆两行。

### 5.3 域覆盖

全量：core reader · dict-cli · dict-tui · ai · html · hn · url · code。

### 5.4 种子与重验

- 从 `mdx-tui-unimplemented-gap.md` + `STATUS.md` + `plugins.md` 灌入行  
- **不信任** gap 文档的 `[x]`：种子时按代码/smoke 重验后写 status  
- 每个 status 在首版矩阵至少 1 行示例

### 5.5 与旧文档

| 文件 | 关系 |
|------|------|
| MATRIX | 对比权威 |
| STATUS | Tuider 加载/ABI/安装；不写 mdx 对照长文 |
| plugins.md | 插件细节；intentional `note` 可 `see plugins.md#…` |
| NEXT | 工程债；gap 可链，不重复矩阵正文 |
| mdx-tui-unimplemented-gap.md | **冻结历史** backlog |

## 6. SMOKE 与 cases.tsv

### 6.1 人类：`SMOKE.md`

说明 compare 语义、依赖、如何加 case；索引到 tsv。

### 6.2 机器：`cases.tsv`

Tab 分隔，首行 header：

```text
case	matrix_ids	need	mdx_args	tuider_args	compare	notes
```

| 字段 | 含义 |
|------|------|
| `case` | 如 `dict-lookup-hello` |
| `matrix_ids` | 逗号分隔矩阵 id |
| `need` | `none` \| `dict` \| `net` \| `ai-key`（可逗号组合） |
| `mdx_args` | 传给 mdx-tui 的参数（不含 bin）；`tuider-only` case 可写 `-` |
| `tuider_args` | 同上 |
| `compare` | 见 6.3 |
| `notes` | 归一/期望说明（可空） |

占位：`{DICT}` `{GROUP}` `{URL}` 由脚本替换。

### 6.3 compare 模式

| 模式 | 判定 |
|------|------|
| `stdout-eq` | strip 行尾空白后 stdout 全等 |
| `stdout-norm` | strip ANSI + 路径归一（`$HOME`、cache 根）后再比 |
| `exit0-both` | 两边 exit 0 即 PASS（help/结构本就不同） |
| `tuider-only-exit0` | 只跑 tuider，exit 0 |

### 6.4 首批 case 范围（约 10–15）

- `none`：`--help` / 版本类 → `exit0-both` 或仅 tuider  
- dict：查词、批量、`-l`、`-n`、`--html`、`-W`（有 `COMPARE_DICT`）  
- 缺 so：`TUIDER_PLUGINS_DIR` 空目录 + 需插件入口 → 期望 need-plugin 文案（可 `tuider-only` 或单独 case）  
- hn：`-hn -l` → 多半 `intentional-diff` + `exit0-both` 或只存档不强制 eq  
- url：`need=net`，可 skip  
- code：`tuider-only-exit0`  
- AI tools：**不进**首批 CLI smoke（需 key + 多轮）；矩阵仍有行，smoke 空

TUI 键位 / 面板：矩阵有行，`smoke` 空；人工验收写 `note`+`updated`。

## 7. 脚本：`scripts/compare-mdx-tui.fish`

### 7.1 CLI

```fish
./scripts/compare-mdx-tui.fish              # 全 case
./scripts/compare-mdx-tui.fish --case X --case Y
./scripts/compare-mdx-tui.fish --list
./scripts/compare-mdx-tui.fish --offline     # SKIP need 含 net
```

### 7.2 行为

1. 解析 env，解析 bin 存在性  
2. 读 `docs/compare/cases.tsv`（`while read -l`；跳过空行与 `#`）  
3. 过滤 `--case`  
4. 每 case：检查 `need` → SKIP 或执行  
5. 写 `runs/<ts>/<case>/{mdx,tuider}.{out,err,code}`（tuider-only 无 mdx 文件）  
6. 按 `compare` 判定  
7. 终端一行状态 + 末尾计数；写 `runs/<ts>/summary.md`  
8. **永不**改 MATRIX.md  

实现约束（ponytail）：纯 fish + 系统 `diff`；无新依赖；无通用测试框架。

### 7.3 退出码

- 0：无 FAIL（允许 SKIP）  
- 1：存在 FAIL  
- 2：用法/缺 bin/缺 tsv

## 8. 日常流程

1. 怀疑漂移或改完功能 → 开 `MATRIX.md` 找 `id`  
2. 有 smoke → `./scripts/compare-mdx-tui.fish --case …`  
3. 看 PASS/FAIL 与 `runs/<ts>/`  
4. **人手**更新矩阵 `status` / `tuider` / `note` / `updated`  
5. 新能力：先 MATRIX 行；能 CLI 再加 tsv 一行  

全量复扫（少做）：脚本全跑 + 扫 `smoke` 空的 TUI/AI 行人工勾。

## 9. 实现顺序（本 spec 批准后）

1. `docs/compare/`：README、MATRIX 壳+种子行、SMOKE、cases.tsv  
2. `scripts/compare-mdx-tui.fish`  
3. `.gitignore`：`docs/compare/runs/`（若尚未）  
4. 指针：README / STATUS / NEXT / gap 顶栏  
5. 自检：`fish -n`；无 dict 时 SKIP；≥1 个 `none` case PASS  

**不做：** writing-plans 前的代码；本文件只定工作流。

## 10. 验收（工作流本身）

- [ ] `fish -n scripts/compare-mdx-tui.fish`  
- [ ] 无 dict → dict case SKIP  
- [ ] ≥1 `need=none` case PASS  
- [ ] MATRIX 四态各 ≥1 行  
- [ ] 旧 gap 文档顶栏指向 MATRIX  
- [ ] README 文档表可发现 compare  

## 11. 源码锚点（对照用，只读 mdx-tui）

| 侧 | 路径 |
|----|------|
| mdx-tui CLI | `~/cleantest/mdx-tui/crates/mdx-tui/src/main.rs` |
| mdx-tui App | `…/src/app.rs` |
| mdx AI tools | `…/mdx-ai/src/chat.rs` |
| tuider CLI | `src/main.rs` |
| tuider AI | `src/ai.rs` |
| dict so | `crates/tuider-plugin-dict` |
| 旧 gap | `docs/mdx-tui-unimplemented-gap.md` |

## 12. 决策记录

| 项 | 选择 |
|----|------|
| 结构 | 单矩阵 + fish 脚本（方案 A） |
| 状态模型 | 四态互斥 |
| 机器输入 | `cases.tsv`（不解析 md 表） |
| 矩阵更新 | 纯人工 |
| 自动化上限 | CLI diff；无 CI、无自动勾 |

---

**一句话：** 用 `docs/compare/MATRIX.md` 记四态差异，用 `cases.tsv` + `compare-mdx-tui.fish` 复跑 CLI 对照；旧 gap 文档退休为历史。
