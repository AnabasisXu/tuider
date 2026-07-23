# Tuider ↔ mdx-tui Compare Workflow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the recurring compare workflow from the design spec: four-state MATRIX, cases.tsv smoke table, fish runner, doc pointers; no product feature work.

**Architecture:** Hand-maintained `docs/compare/MATRIX.md` is the authority for parity / intentional-diff / tuider-only / gap. Machine cases live in `docs/compare/cases.tsv`. `scripts/compare-mdx-tui.fish` runs both binaries, writes `docs/compare/runs/<ts>/`, never rewrites MATRIX. Old gap doc becomes a historical pointer.

**Tech Stack:** Markdown, TSV, fish shell, system `diff`/`sed`; no new Rust crates.

**Spec:** `docs/superpowers/specs/2026-07-23-tuider-mdx-tui-compare-workflow-design.md`

---

## File map

| Path | Action | Responsibility |
|------|--------|----------------|
| `docs/compare/README.md` | Create | How to run, env vars |
| `docs/compare/MATRIX.md` | Create | Four-state capability matrix (seed rows) |
| `docs/compare/SMOKE.md` | Create | Human smoke docs + compare modes |
| `docs/compare/cases.tsv` | Create | Script input |
| `scripts/compare-mdx-tui.fish` | Create | Runner |
| `.gitignore` | Modify | Ignore `docs/compare/runs/` |
| `README.md` | Modify | Doc table row |
| `docs/STATUS.md` | Modify | Pointer to MATRIX |
| `docs/NEXT.md` | Modify | Pointer to MATRIX |
| `docs/mdx-tui-unimplemented-gap.md` | Modify | Historical banner |
| `docs/superpowers/specs/2026-07-23-tuider-mdx-tui-compare-workflow-design.md` | Modify | status → approved/implemented note |

**Non-goals in this plan:** implement product gaps; auto-check MATRIX; build mdx-tui if missing (script fails with message or SKIP when bin absent for optional paths—see Task 4: missing **required** bin → exit 2).

---

### Task 1: Scaffold `docs/compare/` docs + cases.tsv

**Files:**
- Create: `docs/compare/README.md`
- Create: `docs/compare/SMOKE.md`
- Create: `docs/compare/cases.tsv`
- Create: `docs/compare/MATRIX.md` (seed only; full seed content in Task 2 may be same commit if preferred—this plan splits shell vs matrix for review size)

- [ ] **Step 1: Create directory and README**

Write `docs/compare/README.md` exactly:

````markdown
# Tuider ↔ mdx-tui 对比

权威矩阵：[MATRIX.md](MATRIX.md)  
Smoke 说明：[SMOKE.md](SMOKE.md) · 机器表：[cases.tsv](cases.tsv)  
设计：[../superpowers/specs/2026-07-23-tuider-mdx-tui-compare-workflow-design.md](../superpowers/specs/2026-07-23-tuider-mdx-tui-compare-workflow-design.md)

## 跑对照

```fish
cd ~/cleantest/tuider
# 可选：
# set -x MDX_TUI_BIN ~/cleantest/mdx-tui/dist/mdx-tui-linux
# set -x TUIDER_BIN ./target/debug/tuider
# set -x TUIDER_PLUGINS_DIR ~/.local/share/tuider/plugins
# set -x COMPARE_DICT "/path/to/small.mdx"   # 或词典目录
# set -x COMPARE_GROUP eng
# set -x COMPARE_URL https://example.com

./scripts/compare-mdx-tui.fish
./scripts/compare-mdx-tui.fish --list
./scripts/compare-mdx-tui.fish --case help-exit0
./scripts/compare-mdx-tui.fish --offline
```

产物：`docs/compare/runs/<timestamp>/`（gitignore）。**不**自动改 MATRIX——对照后人手改 status。

## 状态四态

| status | 含义 |
|--------|------|
| parity | 可互换 |
| intentional-diff | 接受的差异 |
| tuider-only | mdx-tui 无 |
| gap | 应对齐未对齐 |

## 加 case

1. MATRIX 加/改行  
2. 能 CLI → `cases.tsv` 加一行  
3. 在 SMOKE.md 补一句说明（可选）
````

- [ ] **Step 2: Write SMOKE.md**

Write `docs/compare/SMOKE.md`:

```markdown
# Smoke 对照

机器输入只有 [cases.tsv](cases.tsv)。本文件解释语义。

## compare 列

| 值 | 判定 |
|----|------|
| stdout-eq | 去行尾空白后 stdout 全等 |
| stdout-norm | 去 ANSI + `$HOME` 路径归一后再比 |
| exit0-both | 两边 exit 0 |
| tuider-only-exit0 | 只跑 tuider，exit 0 |

## need 列

`none` · `dict` · `net` · `ai-key`（可逗号组合）。  
`dict`：需 `COMPARE_DICT` 或 `COMPARE_GROUP`，且 tuider 能加载 dict so（`TUIDER_PLUGINS_DIR` 或默认插件目录有 `libtuider_dict.so`）。缺则 SKIP。  
`net`：`--offline` 时 SKIP。  
`ai-key`：首批 tsv **不用**。

## 占位

`{DICT}` `{GROUP}` `{URL}` 由脚本替换。

## 首批 case 索引

见 `cases.tsv`；与 MATRIX `smoke` 列对应。
```

- [ ] **Step 3: Write cases.tsv**

**Important:** real TAB characters between columns (not spaces). File content:

```text
case	matrix_ids	need	mdx_args	tuider_args	compare	notes
help-exit0	CORE-01	none	-h	-h	exit0-both	help text differs by design
tuider-help-only	T-SO-01	none	-	-h	tuider-only-exit0	
dict-lookup-hello	D-CLI-01	dict	{DICT} hello	{DICT} hello	stdout-norm	needs small mdx with hello
dict-batch	D-CLI-02	dict	{DICT} take,make	{DICT} take,make	stdout-norm	
dict-list-lite	D-CLI-03	dict	{DICT} -l hello	{DICT} -l hello	stdout-norm	
dict-limit-n	D-CLI-04	dict	{DICT} -n 1 hello	{DICT} -n 1 hello	stdout-norm	
dict-html	D-CLI-05	dict	{DICT} --html hello	{DICT} --html hello	stdout-norm	raw tags
dict-wordlists-list	D-CFG-03	none	-L	-W	exit0-both	flag names may differ; both list wordlists
missing-url-plugin	T-SO-02	none	-	-u {URL}	tuider-only-exit0	run with TUIDER_PLUGINS_DIR empty in notes; see script special: skip if not empty env FORCE_EMPTY_PLUGINS
hn-list	HN-01	net	-hn -l	-hn -l	exit0-both	intentional content shape diff
url-example	URL-01	net	-u {URL}	-u {URL}	exit0-both	
code-list	T-CODE-01	none	-	--code -l src/main.rs	tuider-only-exit0	needs code so; SKIP if missing so
```

**Clarification for implementer (encode in script Task 4, not in tsv):**

- `missing-url-plugin`: script treats this case specially: run with `TUIDER_PLUGINS_DIR` set to a fresh empty temp dir for **tuider only**, expect **non-zero** exit OR stdout/stderr containing `need plugin`. Prefer: compare mode stays `tuider-only-exit0` is **wrong** for fail-path — fix tsv row:

Replace the `missing-url-plugin` line with:

```text
missing-url-plugin	T-SO-02	none	-	-u {URL}	tuider-only-need-plugin	empty plugins dir; expect need plugin
```

And implement compare mode `tuider-only-need-plugin` in Task 4 (exit ≠ 0 **or** combined out/err matches `need plugin`).

- `code-list`: if `libtuider_code.so` not found in plugins dir → SKIP.

- [ ] **Step 4: Commit scaffold (without MATRIX yet is OK)**

```fish
git add docs/compare/README.md docs/compare/SMOKE.md docs/compare/cases.tsv
git commit -m "docs(compare): scaffold README, SMOKE, cases.tsv"
```

---

### Task 2: Seed MATRIX.md

**Files:**
- Create: `docs/compare/MATRIX.md`

- [ ] **Step 1: Write MATRIX with schema header + seed rows covering all domains and all four statuses**

Write `docs/compare/MATRIX.md`:

```markdown
# Tuider ↔ mdx-tui 功能矩阵

> 权威对比表。日期种子：2026-07-23。  
> 状态：`parity` | `intentional-diff` | `tuider-only` | `gap`  
> Smoke：`docs/compare/cases.tsv` · 跑：`./scripts/compare-mdx-tui.fish`  
> 旧 backlog：[mdx-tui-unimplemented-gap.md](../mdx-tui-unimplemented-gap.md)（历史）

## 图例

| status | 含义 |
|--------|------|
| parity | 同输入可互换（允许 smoke 声明的归一差） |
| intentional-diff | 已知且接受 |
| tuider-only | mdx-tui 无 |
| gap | 应对齐未对齐 |

**种子说明：** 行状态按 2026-07-23 代码/文档**重验意图**填写；CLI 行以 smoke 为准复验后改 `updated`。gap 旧文档的 `[x]` **不**自动等于 parity。

## core

| id | capability | mdx-tui | tuider | status | note | smoke | updated |
|----|------------|---------|--------|--------|------|-------|---------|
| CORE-01 | CLI help | `-h` 退出并打印用法 | `-h`/`--help` 打印用法 | intentional-diff | 文案/插件段不同；smoke 只比 exit0 | help-exit0 | 2026-07-23 |
| CORE-02 | 打开 md/txt 阅读 | 支持 | 支持（core） | parity | TUI；无 CLI smoke | | 2026-07-23 |
| CORE-03 | vim `/` `n` `N` 搜索 | 有 | 有 | parity | TUI | | 2026-07-23 |
| CORE-04 | visual yank | 弱/无对等 | `v`/`V` + `y` OSC52 | tuider-only | 阅读器超集 | | 2026-07-23 |

## dict-cli

| id | capability | mdx-tui | tuider | status | note | smoke | updated |
|----|------------|---------|--------|--------|------|-------|---------|
| D-CLI-01 | 词参数 CLI 查词 | 有词即 stdout | host `lookup_word` 路径 | parity | 需 COMPARE_DICT 复验 | dict-lookup-hello | 2026-07-23 |
| D-CLI-02 | 逗号批量词 | `take,make` | 支持 | parity | 复验 | dict-batch | 2026-07-23 |
| D-CLI-03 | `-l` 精简 | 词头+词典名 | 对齐意图 | parity | 复验 | dict-list-lite | 2026-07-23 |
| D-CLI-04 | `-n N` 限词典 | 有 | 有 | parity | 复验 | dict-limit-n | 2026-07-23 |
| D-CLI-05 | `--html` | 原始 HTML | 有 | parity | 复验 | dict-html | 2026-07-23 |
| D-CLI-06 | `--db` 导出 | 有 | open 路径导出 | parity | 无强制 smoke（副作用文件） | | 2026-07-23 |
| D-CLI-07 | 非 TTY 纯文本 | 关彩色 | 查词路径对齐意图 | parity | 人工/管道 | | 2026-07-23 |
| D-CLI-08 | TTY 彩色 CLI | 有 | 有意图 | intentional-diff | 色码/渲染实现不同可接受 | | 2026-07-23 |
| D-CFG-01 | yml wordlists | 有 | 已接线 dict | parity | | | 2026-07-23 |
| D-CFG-02 | `-w` 词表过滤 | 有 | 有 | parity | TUI/侧栏 | | 2026-07-23 |
| D-CFG-03 | 列词表 | `-L` | `-W`（`-L` alias） | intentional-diff | flag 名；smoke exit0-both | dict-wordlists-list | 2026-07-23 |
| D-CFG-04 | `-s a,b` 临时词表 | 有 | 有 | parity | | | 2026-07-23 |
| D-CFG-05 | groups 配置路径 | mdx-tui.yml | tuider.yml + 可选 legacy wordlists | intentional-diff | 见 config 双读 | | 2026-07-23 |

## dict-tui

| id | capability | mdx-tui | tuider | status | note | smoke | updated |
|----|------------|---------|--------|--------|------|-------|---------|
| D-UI-01 | 实时词头搜索框 | 主交互 | 有（dict 源） | parity | TUI 人工 | | 2026-07-23 |
| D-UI-02 | Ctrl+U 清搜索 | 有 | 有意图 | parity | TUI | | 2026-07-23 |
| D-UI-03 | Esc 清搜索 | 有 | overlay 语义不同处见 note | intentional-diff | Esc 还关 AI/help | | 2026-07-23 |
| D-UI-04 | Ctrl+B 词典面板 | 有 | 有 | parity | TUI | | 2026-07-23 |
| D-UI-05 | Tab 切词典反馈 | 有 | cycle + status | parity | TUI | | 2026-07-23 |
| D-UI-06 | 搜索框编辑键 | 全套 | 基础+词级 | parity | TUI | | 2026-07-23 |
| D-UI-07 | Ctrl+Y 复制释义 | 有 | 有意图 | parity | TUI | | 2026-07-23 |
| D-UI-08 | 词表模式保持过滤 | 有 | 有意图 | parity | TUI | | 2026-07-23 |

## ai

| id | capability | mdx-tui | tuider | status | note | smoke | updated |
|----|------------|---------|--------|--------|------|-------|---------|
| AI-01 | `/exp` 导出 | mdx-tui-chat-*.md | tuider-chat-*.md | intentional-diff | 文件名前缀不同 | | 2026-07-23 |
| AI-02 | `/exp last\|N` | 有 | 有 | parity | | | 2026-07-23 |
| AI-03 | `/switch` 列表 | 有 | 有 | parity | | | 2026-07-23 |
| AI-04 | `/switch` 切换 | 有 | 有 | parity | | | 2026-07-23 |
| AI-05 | tool query_word | 有 | 有 | parity | 需 dict so | | 2026-07-23 |
| AI-06 | search_headwords | 有 | 有 | parity | | | 2026-07-23 |
| AI-07 | list_dicts | 有 | 有 | parity | | | 2026-07-23 |
| AI-08 | batch_query | 有 | 有 | parity | | | 2026-07-23 |
| AI-09 | analyze_vocab | 有 | 有 | parity | | | 2026-07-23 |
| AI-10 | reverse_lookup | 有 | 有 | parity | | | 2026-07-23 |
| AI-11 | get_current_content | 有 | 有 | parity | | | 2026-07-23 |
| AI-12 | export_content | 有 | 有 | parity | | | 2026-07-23 |
| AI-13 | web_search | 有 | 有（需 TAVILY） | parity | 无 key 明确失败 | | 2026-07-23 |
| AI-14 | 词典上下文注入 | 词条结构 | 文档+词条意图 | intentional-diff | prompt 结构不必逐字 | | 2026-07-23 |
| AI-15 | tool 多轮 loop | 有 | 有 | parity | | | 2026-07-23 |
| AI-16 | 快捷键 2–4 | 有 | 有意图 | parity | | | 2026-07-23 |
| AI-17 | ai-tools 日志 | 有 | 可选 | intentional-diff | 路径/开关可不同 | | 2026-07-23 |
| AI-18 | 无 dict tools 降级 | N/A | 阅读 tools 不 panic | tuider-only | 方案 A | | 2026-07-23 |
| AI-19 | 429/5xx provider 轮换 | 弱/无 | 有 | tuider-only | | | 2026-07-23 |

## html

| id | capability | mdx-tui | tuider | status | note | smoke | updated |
|----|------------|---------|--------|--------|------|-------|---------|
| R-01 | img alt | 有 | host html_render | parity | unit test | | 2026-07-23 |
| R-02 | table 简单网格 | 有 | ` \| ` 分隔 | parity | | | 2026-07-23 |
| R-03 | 引号感知 tag | 有 | 有 | parity | | | 2026-07-23 |
| R-04 | 真实词典 CSS 金样 | mdx 侧 fixture | 未强制同行级金样 | gap | 可选后续；非阻塞工作流 | | 2026-07-23 |

## hn

| id | capability | mdx-tui | tuider | status | note | smoke | updated |
|----|------------|---------|--------|--------|------|-------|---------|
| HN-01 | `-hn -l` 输出形态 | Markdown 表 | 标题行（侧栏复用） | intentional-diff | plugins.md | hn-list | 2026-07-23 |
| HN-02 | 评论 cap | 不同 | BFS cap 40 | intentional-diff | plugins.md | | 2026-07-23 |
| HN-03 | 原文键 | o / 修饰 Enter | `a` article | intentional-diff | plugins.md | | 2026-07-23 |

## url

| id | capability | mdx-tui | tuider | status | note | smoke | updated |
|----|------------|---------|--------|--------|------|-------|---------|
| URL-01 | URL 抓取阅读 | 有 | so 插件 | intentional-diff | 缓存路径 `~/.cache/tuider` | url-example | 2026-07-23 |
| URL-02 | 失败/重试文案 | 有 | 无重试+open err | intentional-diff | plugins.md | | 2026-07-23 |

## code

| id | capability | mdx-tui | tuider | status | note | smoke | updated |
|----|------------|---------|--------|--------|------|-------|---------|
| T-CODE-01 | `--code` 源码模式 | 无 | so 插件 | tuider-only | | code-list | 2026-07-23 |

## platform / packaging

| id | capability | mdx-tui | tuider | status | note | smoke | updated |
|----|------------|---------|--------|--------|------|-------|---------|
| T-SO-01 | 动态 so 插件 | 单体 bin | dlopen 方案 A | tuider-only | | tuider-help-only | 2026-07-23 |
| T-SO-02 | 缺 so 提示 | N/A | `need plugin …` | tuider-only | | missing-url-plugin | 2026-07-23 |
| T-PKG-01 | `tuider pkg` | 无 | list/install/remove | tuider-only | | | 2026-07-23 |
```

- [ ] **Step 2: Commit MATRIX**

```fish
git add docs/compare/MATRIX.md
git commit -m "docs(compare): seed four-state MATRIX"
```

---

### Task 3: gitignore runs/

**Files:**
- Modify: `.gitignore`

- [ ] **Step 1: Append ignore rule**

Add at end of `.gitignore`:

```gitignore

# Compare workflow run artifacts
/docs/compare/runs/
```

- [ ] **Step 2: Commit**

```fish
git add .gitignore
git commit -m "chore: gitignore docs/compare/runs"
```

---

### Task 4: Implement `scripts/compare-mdx-tui.fish`

**Files:**
- Create: `scripts/compare-mdx-tui.fish` (executable)

- [ ] **Step 1: Write the script**

Create `scripts/compare-mdx-tui.fish` with full content below (ponytail: one file, no framework).

```fish
#!/usr/bin/env fish
# Compare tuider vs mdx-tui CLI cases from docs/compare/cases.tsv
# Never modifies MATRIX.md.

set -l ROOT (dirname (status filename))/..
set -l ROOT (cd $ROOT; pwd)
set -l TSV $ROOT/docs/compare/cases.tsv

function usage
    echo "usage: "(status filename)" [--list] [--offline] [--case ID]..."
    echo "env: MDX_TUI_BIN TUIDER_BIN TUIDER_PLUGINS_DIR COMPARE_DICT COMPARE_GROUP COMPARE_URL COMPARE_RUN_DIR"
end

set -l LIST 0
set -l OFFLINE 0
set -l CASES

set -l i 1
while test $i -le (count $argv)
    set -l a $argv[$i]
    switch $a
        case --list
            set LIST 1
        case --offline
            set OFFLINE 1
        case --case
            set i (math $i + 1)
            if test $i -gt (count $argv)
                echo "missing id after --case" >&2
                usage
                exit 2
            end
            set -a CASES $argv[$i]
        case -h --help
            usage
            exit 0
        case '*'
            echo "unknown arg: $a" >&2
            usage
            exit 2
    end
    set i (math $i + 1)
end

if not test -f $TSV
    echo "missing $TSV" >&2
    exit 2
end

function resolve_mdx_bin
    if set -q MDX_TUI_BIN; and test -x $MDX_TUI_BIN
        echo $MDX_TUI_BIN
        return
    end
    for p in \
        $HOME/cleantest/mdx-tui/target/release/mdx-tui \
        $HOME/cleantest/mdx-tui/target/debug/mdx-tui \
        $HOME/cleantest/mdx-tui/dist/mdx-tui-linux
        if test -x $p
            echo $p
            return
        end
    end
    return 1
end

function resolve_tuider_bin
    if set -q TUIDER_BIN; and test -x $TUIDER_BIN
        echo $TUIDER_BIN
        return
    end
    for p in $ROOT/target/release/tuider $ROOT/target/debug/tuider
        if test -x $p
            echo $p
            return
        end
    end
    return 1
end

set -l MDX_BIN (resolve_mdx_bin)
or begin
    echo "mdx-tui binary not found; set MDX_TUI_BIN (tried release/debug/dist)" >&2
    exit 2
end
set -l TUI_BIN (resolve_tuider_bin)
or begin
    echo "tuider binary not found; set TUIDER_BIN or cargo build" >&2
    exit 2
end

set -l PLUGINS $TUIDER_PLUGINS_DIR
if not set -q TUIDER_PLUGINS_DIR
    set PLUGINS $HOME/.local/share/tuider/plugins
end

set -l URL https://example.com
if set -q COMPARE_URL
    set URL $COMPARE_URL
end

set -l DICT ""
if set -q COMPARE_DICT
    set DICT $COMPARE_DICT
end
set -l GROUP ""
if set -q COMPARE_GROUP
    set GROUP $COMPARE_GROUP
end

function expand_args
    # stdin: args string → stdout expanded
    set -l s $argv[1]
    if test -z "$s"; or test "$s" = "-"
        echo ""
        return
    end
    set s (string replace -a '{URL}' $URL -- $s)
    set s (string replace -a '{DICT}' $DICT -- $s)
    set s (string replace -a '{GROUP}' $GROUP -- $s)
    echo $s
end

function has_dict_so
    test -f $PLUGINS/libtuider_dict.so
end

function has_code_so
    test -f $PLUGINS/libtuider_code.so
end

function has_hn_so
    test -f $PLUGINS/libtuider_hn.so
end

function has_url_so
    test -f $PLUGINS/libtuider_url.so
end

function strip_ansi
    # basic CSI strip
    sed -E 's/\x1B\[[0-9;]*[A-Za-z]//g'
end

function norm_out
    strip_ansi | sed -E "s|$HOME|\$HOME|g" | sed -E 's/[[:space:]]+$//'
end

function eq_out
    set -l a $argv[1]
    set -l b $argv[2]
    set -l mode $argv[3]
    switch $mode
        case stdout-eq
            set -l aa (sed -E 's/[[:space:]]+$//' $a | string collect)
            set -l bb (sed -E 's/[[:space:]]+$//' $b | string collect)
            test "$aa" = "$bb"
        case stdout-norm
            set -l aa (norm_out < $a | string collect)
            set -l bb (norm_out < $b | string collect)
            test "$aa" = "$bb"
        case '*'
            return 1
    end
end

if test $LIST -eq 1
    echo "MDX_BIN=$MDX_BIN"
    echo "TUIDER_BIN=$TUI_BIN"
    echo "PLUGINS=$PLUGINS"
    tail -n +2 $TSV | while read -l line
        test -z "$line"; and continue
        string match -q '#*' -- $line; and continue
        set -l f (string split \t -- $line)
        echo $f[1]\t$f[2]\t$f[3]\t$f[6]
    end
    exit 0
end

set -l TS (date +%Y%m%d-%H%M%S)
set -l RUN $ROOT/docs/compare/runs/$TS
if set -q COMPARE_RUN_DIR
    set RUN $COMPARE_RUN_DIR
end
mkdir -p $RUN

set -l n_pass 0
set -l n_fail 0
set -l n_skip 0

set -l summary $RUN/summary.md
echo "# compare run $TS" > $summary
echo "" >> $summary
echo "| case | result | note |" >> $summary
echo "|------|--------|------|" >> $summary

tail -n +2 $TSV | while read -l line
    test -z "$line"; and continue
    string match -q '#*' -- $line; and continue
    set -l f (string split \t -- $line)
    # case matrix need mdx tuider compare notes
    if test (count $f) -lt 6
        echo "bad row: $line" >&2
        set n_fail (math $n_fail + 1)
        continue
    end
    set -l case $f[1]
    set -l need $f[3]
    set -l mdx_args $f[4]
    set -l tui_args $f[5]
    set -l compare $f[6]
    set -l notes ""
    if test (count $f) -ge 7
        set notes $f[7]
    end

    if test (count $CASES) -gt 0
        set -l hit 0
        for c in $CASES
            if test $c = $case
                set hit 1
                break
            end
        end
        test $hit -eq 0; and continue
    end

    set -l skip_reason ""
    for n in (string split , -- $need)
        set n (string trim -- $n)
        switch $n
            case none ''
                true
            case dict
                if test -z "$DICT" -a -z "$GROUP"
                    set skip_reason "no COMPARE_DICT/GROUP"
                else if not has_dict_so
                    set skip_reason "no libtuider_dict.so in $PLUGINS"
                end
            case net
                if test $OFFLINE -eq 1
                    set skip_reason "offline"
                end
            case ai-key
                set skip_reason "ai-key not in first batch automation"
            case '*'
                set skip_reason "unknown need $n"
        end
    end

    # code so optional skip
    if test $case = code-list; and not has_code_so
        set skip_reason "no libtuider_code.so"
    end
    if test $case = hn-list; and not has_hn_so
        set skip_reason "no libtuider_hn.so"
    end
    if test $case = url-example; and not has_url_so
        set skip_reason "no libtuider_url.so"
    end

    if test -n "$skip_reason"
        echo "SKIP  $case  ($skip_reason)"
        echo "| $case | SKIP | $skip_reason |" >> $summary
        set n_skip (math $n_skip + 1)
        continue
    end

    set -l cdir $RUN/$case
    mkdir -p $cdir

    set -l mdx_s (expand_args $mdx_args)
    set -l tui_s (expand_args $tui_args)

    set -l env_tui TUIDER_PLUGINS_DIR=$PLUGINS
    if test $case = missing-url-plugin
        set -l empty (mktemp -d)
        set env_tui TUIDER_PLUGINS_DIR=$empty
    end

    set -l mdx_code 0
    set -l tui_code 0

    if test $compare != tuider-only-exit0; and test $compare != tuider-only-need-plugin
        if test -n "$mdx_s"
            # fish: split args carefully
            set -l margv (string split -n ' ' -- $mdx_s)
            $MDX_BIN $margv >$cdir/mdx.out 2>$cdir/mdx.err
            set mdx_code $status
        else
            echo -n >$cdir/mdx.out
            echo -n >$cdir/mdx.err
            set mdx_code 0
        end
        echo $mdx_code >$cdir/mdx.code
    end

    set -l targv (string split -n ' ' -- $tui_s)
    env $env_tui $TUI_BIN $targv >$cdir/tuider.out 2>$cdir/tuider.err
    set tui_code $status
    echo $tui_code >$cdir/tuider.code

    set -l ok 0
    set -l detail ""
    switch $compare
        case exit0-both
            if test $mdx_code -eq 0; and test $tui_code -eq 0
                set ok 1
            else
                set detail "mdx=$mdx_code tuider=$tui_code"
            end
        case tuider-only-exit0
            if test $tui_code -eq 0
                set ok 1
            else
                set detail "tuider=$tui_code"
            end
        case tuider-only-need-plugin
            set -l blob (cat $cdir/tuider.out $cdir/tuider.err | string collect)
            if test $tui_code -ne 0; or string match -q '*need plugin*' -- $blob
                set ok 1
            else
                set detail "expected need plugin, code=$tui_code"
            end
        case stdout-eq stdout-norm
            if eq_out $cdir/mdx.out $cdir/tuider.out $compare
                set ok 1
            else
                set detail "stdout mismatch (see $cdir)"
                diff -u $cdir/mdx.out $cdir/tuider.out >$cdir/diff.txt 2>/dev/null
            end
        case '*'
            set detail "unknown compare $compare"
    end

    if test $ok -eq 1
        echo "PASS  $case"
        echo "| $case | PASS | |" >> $summary
        set n_pass (math $n_pass + 1)
    else
        echo "FAIL  $case  $detail"
        echo "| $case | FAIL | $detail |" >> $summary
        set n_fail (math $n_fail + 1)
    end
end

echo ""
echo "PASS=$n_pass FAIL=$n_fail SKIP=$n_skip  run=$RUN"
echo "" >> $summary
echo "PASS=$n_pass FAIL=$n_fail SKIP=$n_skip" >> $summary

if test $n_fail -gt 0
    exit 1
end
exit 0
```

**Note on fish `env KEY=val cmd`:** if `env` form is awkward, use:

```fish
begin
    set -lx TUIDER_PLUGINS_DIR $PLUGINS
    if test $case = missing-url-plugin
        set -lx TUIDER_PLUGINS_DIR (mktemp -d)
    end
    $TUI_BIN $targv >$cdir/tuider.out 2>$cdir/tuider.err
    set tui_code $status
end
```

Prefer the `set -lx` block when implementing—more reliable in fish.

Also: **pipeline `while read` runs in a subshell in some fish versions and loses counters.** Implementer MUST avoid that: either

1. `set -l lines (tail -n +2 $TSV)`; `for line in $lines`; or  
2. write counts to temp files.

**Required fix:** use `for line in (tail -n +2 $TSV)` style so `n_pass` etc. update in the main shell.

- [ ] **Step 2: chmod + syntax check**

```fish
chmod +x scripts/compare-mdx-tui.fish
fish -n scripts/compare-mdx-tui.fish
```

Expected: no output, exit 0.

- [ ] **Step 3: Align cases.tsv missing-url-plugin compare mode**

Ensure Task 1 tsv uses `tuider-only-need-plugin` for that row (if still `tuider-only-exit0`, edit now).

- [ ] **Step 4: Commit script**

```fish
git add scripts/compare-mdx-tui.fish docs/compare/cases.tsv
git commit -m "feat(compare): fish runner for mdx-tui CLI smoke"
```

---

### Task 5: Wire doc pointers

**Files:**
- Modify: `README.md` (doc table)
- Modify: `docs/STATUS.md`
- Modify: `docs/NEXT.md`
- Modify: `docs/mdx-tui-unimplemented-gap.md` (banner)
- Modify: design spec status line

- [ ] **Step 1: README.md**

In the 文档 table, add row after plugins or STATUS:

```markdown
| [docs/compare/MATRIX.md](docs/compare/MATRIX.md) | **vs mdx-tui 功能矩阵** + [smoke](docs/compare/README.md) |
```

- [ ] **Step 2: STATUS.md**

After 权威顺序 section or top, add:

```markdown
功能 vs mdx-tui：**[compare/MATRIX.md](compare/MATRIX.md)**（四态）；CLI 对照 `./scripts/compare-mdx-tui.fish`。
```

- [ ] **Step 3: NEXT.md**

Replace or augment the gap pointer line:

```markdown
功能对照权威（parity / intentional / tuider-only / gap）：[compare/MATRIX.md](compare/MATRIX.md)  
历史单向 backlog：[mdx-tui-unimplemented-gap.md](mdx-tui-unimplemented-gap.md)
```

- [ ] **Step 4: gap doc banner**

Insert after title (line 1–2 area):

```markdown
> **历史 backlog（2026-07-23 起不再作权威）。**  
> 双向状态与可跑 smoke 见 **[compare/MATRIX.md](compare/MATRIX.md)** · [compare/README.md](compare/README.md)。
```

- [ ] **Step 5: design spec status**

Change header status to: `状态：approved；实现见 plans/2026-07-23-tuider-mdx-tui-compare-workflow.md`

- [ ] **Step 6: Commit**

```fish
git add README.md docs/STATUS.md docs/NEXT.md docs/mdx-tui-unimplemented-gap.md docs/superpowers/specs/2026-07-23-tuider-mdx-tui-compare-workflow-design.md
git commit -m "docs: point STATUS/NEXT/README/gap at compare MATRIX"
```

---

### Task 6: Verify workflow acceptance

**Files:** none (run only)

- [ ] **Step 1: Syntax**

```fish
fish -n scripts/compare-mdx-tui.fish
```

Expected: exit 0.

- [ ] **Step 2: --list**

```fish
./scripts/compare-mdx-tui.fish --list
```

Expected: prints bins + case ids including `help-exit0`.

- [ ] **Step 3: none case PASS**

```fish
./scripts/compare-mdx-tui.fish --case help-exit0 --case tuider-help-only
```

Expected: both PASS (or help-exit0 PASS); exit 0.  
`docs/compare/runs/*/` exists with summary.md.

- [ ] **Step 4: dict SKIP without COMPARE_DICT**

```fish
set -e COMPARE_DICT
set -e COMPARE_GROUP
./scripts/compare-mdx-tui.fish --case dict-lookup-hello
```

Expected: `SKIP  dict-lookup-hello` and exit 0.

- [ ] **Step 5: missing plugin case**

```fish
./scripts/compare-mdx-tui.fish --case missing-url-plugin
```

Expected: PASS (need plugin path).

- [ ] **Step 6: Optional dict PASS (if machine has dict)**

```fish
set -x COMPARE_DICT "/root/cleantest/dict/AHD Usage Notes.mdx"
# or a dict that contains headword hello
./scripts/compare-mdx-tui.fish --case dict-lookup-hello
```

If headword missing → FAIL is OK; update MATRIX note or pick better DICT. Do **not** block workflow merge on content FAIL if SKIP path works.

- [ ] **Step 7: Confirm MATRIX four statuses present**

```fish
rg -c '\| parity \|' docs/compare/MATRIX.md
rg -c 'intentional-diff' docs/compare/MATRIX.md
rg -c 'tuider-only' docs/compare/MATRIX.md
rg -c '\| gap \|' docs/compare/MATRIX.md
```

Expected: each ≥ 1.

- [ ] **Step 8: Final commit if verification fixed script bugs**

```fish
git add -u scripts/compare-mdx-tui.fish docs/compare
git status
# commit only if dirty:
git commit -m "fix(compare): runner edge cases from smoke verify"
```

---

## Spec coverage checklist

| Spec section | Task |
|--------------|------|
| §3 产物 README/MATRIX/SMOKE/tsv/script/runs | 1,2,4 |
| §4 env + COMPARE_DICT skip | 4,6 |
| §5 MATRIX schema + four states + seed | 2 |
| §6 cases + compare modes | 1,4 |
| §7 script CLI/exit codes | 4,6 |
| §8 daily flow (docs only) | 1 README |
| §9 pointers + gitignore | 3,5 |
| §10 acceptance | 6 |
| Non-goal: no MATRIX auto-write | 4 |
| dist/mdx-tui-linux fallback | 4 resolve_mdx_bin |

## Self-review notes

- No TBD steps; full script body included.  
- `tuider-only-need-plugin` added beyond first draft of tsv—keep tsv and script in sync (Tasks 1+4).  
- fish subshell counter pitfall called out—implementer must use `for line in ...`.  
- Seed MATRIX marks many D-CLI as parity from code presence; smoke may FAIL on formatting—then set `intentional-diff` or fix notes, not silent ignore.

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-07-23-tuider-mdx-tui-compare-workflow.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks  
2. **Inline Execution** — this session with executing-plans + checkpoints  

Which approach?
