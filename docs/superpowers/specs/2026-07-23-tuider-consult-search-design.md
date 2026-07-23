# Tuider Consult 搜索集成

> 日期：2026-07-23  
> 状态：草案（需求/差距/落地切片）  
> 范围：强化现有 `Alt+f` consult overlay；不新建并行搜索系统  
> 参考：Emacs consult（live preview / narrowing / async two-stage）

## 1. 结论（先做啥）

Tuider **已有** consult 壳：

| 已有 | 位置 |
|------|------|
| 入口 `Alt+f` | `src/app/keys.rs` → `open_consult` |
| 底栏：输入 + hits + preview | `src/app/nav.rs` `draw_consult` |
| 子串过滤、Enter 跳行、Esc 关 | `refilter_consult` / `nav_activate` |
| 与 `/` vim、侧栏 filter 并存 | 不同模式，不合并 |

要「像 consult 一样好用」，**先补当前文档内体验**，不要一上来做项目级 `rg`。

```
P0  当前 body 过滤体验（orderless + 高亮 + 计数 + 预览跟选）
P1  预览增强 + 空查询/上限策略
P2  可选：项目/目录异步 rg（真需要再开）
```

---

## 2. 现状差距

相对 Emacs `consult-line`（当前缓冲行搜索）：

| 能力 | 现状 | 缺口 |
|------|------|------|
| 实时过滤 | `contains` 子串 | 仅单串；无空格分词任意序 |
| 实时预览 | 选中行 ±2 | 有；可加匹配高亮、可配上下文行数 |
| 匹配高亮 | 无 | hit 行与 preview 均无 query 高亮 |
| 计数 | 标题 `N hits` | 缺 `选中/总数`、status 同步 |
| 上限 | 硬截 500 | 应 status 提示 truncated |
| 空 query | 列出全文前 500 行 | OK；保持 |
| 异步/外部 | 无 | 当前 body 同步即可，YAGNI |
| 两阶段 `#rg#local` | 无 | 仅 P2 项目搜索需要 |
| 导出编辑 | 无 | 不做（阅读器非 wgrep） |

相对 `consult-ripgrep`（跨文件）：

| 能力 | 现状 | 决策 |
|------|------|------|
| spawn rg | 无 | **P2 可选**，默认不做 |
| debounce | 无 | 仅 P2 需要 |
| 结果 `path:line:text` | 无 | P2 |
| 打开文件跳行 | 仅当前 body 行号 | P2 才扩 |

---

## 3. 产品定位（避免和现有搜索打架）

| 入口 | 职责 | 保持 |
|------|------|------|
| `/` `n` `N` | vim 连续命中跳转 + 正文 span 高亮 | 不动契约 |
| 侧栏 filter | 过滤**文档/词条列表** | 不动 |
| `f` / `o` | 链接 / 大纲列表过滤 | 不动 |
| **`Alt+f` consult** | **当前正文行级模糊检索 + 预览跳转** | **本文件只改这条** |

原则：consult 是 **jump UI**，不是替换 vim 高亮，也不是侧栏搜索。

---

## 4. P0 — 当前文档 consult 最小可用增强

### 4.1 行为

1. `Alt+f` 打开底栏（现逻辑保留）。
2. 输入即时过滤；**空格分词 = 全部 token 均匹配（任意顺序）**，大小写不敏感。  
   - 例：`foo bar` ≡ 行内同时含 `foo` 与 `bar`。  
   - 不做 regex（P0）；需要时用户继续用 `/` 或以后加开关。
3. hits 行：`行号 │ 文本`，query token 高亮。
4. 上下移动：preview 跟选中行（已有）；preview 内命中行高亮。
5. status：`consult 3/42` 或 `consult 0`；截断时 `consult 3/500+ (truncated)`。
6. Enter：跳到选中行并关 overlay（已有）。
7. Esc：关 overlay，不改 scroll（已有 close 行为；保持）。

### 4.2 过滤算法（stdlib，无新依赖）

```text
tokens = query.split_whitespace().map(lowercase).filter(|t| !t.is_empty())
match line if tokens.is_empty() || tokens.iter().all(|t| line_lower.contains(t))
```

- 上限仍 500；超过则 `truncated = true`。
- `filtered: Vec<usize>` 语义不变（body 行下标）。

### 4.3 UI 改动（仅 `draw_consult` + status）

- 输入行：`> query` + 右侧或 title 显示 `sel/total`。
- hits `ListItem`：对 plain 文本按 tokens 做 span 高亮（复用 `ui.rs` 里 vim 高亮思路，抽小函数即可）。
- preview：命中行 token 高亮；上下文默认 ±2，常量 `CONSULT_PREVIEW_CTX: usize = 2`。

### 4.4 代码落点（最短 diff）

| 文件 | 改动 |
|------|------|
| `src/app/nav.rs` | `refilter_consult` → orderless tokens；truncated 标志；`draw_consult` 高亮与计数 |
| `src/app/mod.rs` 或 `nav` | 可选 `nav.truncated: bool` 挂 `NavState` |
| `src/ui.rs` help 文案 | 一句：`Alt+f` 空格多词过滤 |
| **不改** | `search.rs` vim、`keys` 除文案外、插件 ABI |

`ConsultHit` 结构体目前未真正使用（filtered 直接存行号）。P0 **继续用 `Vec<usize>`**，不引入新类型。

### 4.5 验收

- 打开任意 md → `Alt+f` → 输入两词 → 仅两词皆命中的行出现。
- 上下移动 preview 跟随；Enter 跳行正确。
- 超 500 命中有 truncated 提示。
- `/` 与侧栏 filter 行为无回归。

可跑检查（实现后）：

```fish
cd ~/cleantest/tuider && cargo check
# 手动：cargo run -- path/to/file.md → Alt+f
```

---

## 5. P1 — 体验打磨（仍无 rg）

| 项 | 说明 |
|----|------|
| 预览上下文可配 | 常量或以后 yml；默认 2 足够 |
| 空 query 策略 | 保持「前 500 行」或改为「从当前 scroll 附近起」——默认保持简单 |
| j/k 与输入冲突 | 已用 j/k 移动；字母进 query。保持；不引入复杂 modal |
| 当前行优先 | 打开 consult 时选中最接近 `scroll` 的 hit（可选，小改） |
| 匹配字符级 | hit 列表可显示首个 token 偏移（非必须） |

---

## 6. P2 — 项目级异步搜索（可选，默认不做）

**仅当**需要跨文件搜源码/文档树时再做。对标 `consult-ripgrep` 的最小子集：

### 6.1 行为

1. 新入口建议：`Alt+g` 或 `consult` 内前缀 `#` 切换「项目模式」——**二选一，实现时再定**；推荐 **独立 `Alt+g`**，避免污染当前文档 consult。
2. debounce ~100ms 后 `tokio`/`std::process` spawn：

```text
rg --json --smart-case --max-columns=300 --glob '!target' --glob '!.git' <pattern> <root>
```

3. 旧进程 kill，新进程读 stdout 行解析 JSON `type=match`。
4. 候选：`path:line:text`；Enter：若 path 已在 source 列表则 load+跳行，否则打开文件（走现有 open 路径）。
5. 本地二次过滤：可选 `#pattern#local tokens`（两阶段）；P2 第一刀可只做单阶段 pattern→rg。

### 6.2 约束

- **不**引入新 Cargo 依赖解析 JSON：手写最小匹配字段，或 `serde_json` 若项目已有。
- 无 `rg`：status 提示安装，不崩溃。
- 搜索根：`current_path` 父目录或 cwd；不做 git root 探测（需要时再加）。
- 仍不阻塞 draw：结果经 channel / 轮询 `try_recv` 合并进 `NavState`。

### 6.3 为何默认砍掉

阅读器主路径是**当前打开文档**。项目 rg 是 IDE 功能；P0 补齐 line consult 已覆盖 Emacs 日常 80% 跳转场景。

---

## 7. 非目标

- Embark / wgrep 式导出编辑  
- orderless 的 `!` 否定、`~` flex 等全套 style dispatcher  
- 替换 vim `/`  
- 动态 so 插件做搜索（搜索属 core UI）  
- 新 crate / 新依赖仅为 fuzzy（`nucleo` 等）——P0 子串足够  

---

## 8. 实施顺序（给实现对话用）

1. **只改** `nav.rs`：`refilter_consult` tokens + truncated status。  
2. **只改** `draw_consult`：计数 + token 高亮。  
3. help 一行。  
4. `cargo check` + 手动 Alt+f。  
5. 停。P2 另开对话、另开文档修订。

验收口令：

```fish
cd ~/cleantest/tuider && cargo check && cargo run -- README.md
# Alt+f → 输入多词 → ↑↓ 预览 → Enter 跳转
```

---

## 9. 相关代码索引

| 符号 | 文件 |
|------|------|
| `open_consult` / `refilter_consult` / `draw_consult` | `src/app/nav.rs` |
| `Alt+f` | `src/app/keys.rs` |
| vim `/` | `src/app/search.rs` |
| overlay 绘制入口 | `src/ui.rs` → `draw_nav_overlay` |
| 产品边界 | `docs/PLAN.md`、`docs/NEXT.md` |

---

## 10. 自检

- [x] 基于现有 Alt+f，不平行造轮子  
- [x] P0 可独立交付，无 rg / 无新依赖  
- [x] 与 `/`、侧栏职责切开  
- [x] P2 明确可选与砍因  
