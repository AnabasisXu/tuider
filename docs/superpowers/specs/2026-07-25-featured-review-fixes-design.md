# Featured 文档审核修复（第一轮）

> 日期：2026-07-25  
> 状态：已定稿（待实现）  
> 范围：bug / 文档 / 小改；**不含** feed、书签、选区联动、完整快捷键自定义  
> 策略：最短 diff；不引入新依赖；不建 keymap 框架

## 1. 结论

第一轮只交付「读 FEATURES 时踩到的」可立即修的项。大功能（书签 / leader 自定义 / 选区→dict·AI / URL feed）另开 spec。

| # | 项 | 类型 |
|---|----|------|
| 1 | `O` 打开目录（SHIFT 修饰） | bug |
| 2 | OSC 52 文档释义 | docs |
| 3 | AI：`Enter` 发送 · `Ctrl+j` 换行 | 行为翻转 |
| 4 | Nav 过滤：`j`/`k` 始终进 query | bug |
| 5 | Windows 安装 / `pkg` / 配置路径说明 | docs |
| 6 | 进 TUI 时创建最小 `~/.config/tuider.yml`；help 显示路径 | 行为+docs |
| 7 | `/` 当前匹配：桃粉底 → 红底 | UI |
| 8 | 删除 `-u` / `--url` 认领与解析 | CLI 破坏性小改 |
| 9 | URL 失败：分类错误 + HTTP status | 错误信息 |

## 2. 非目标（明确推迟）

- URL RSS/Atom feed（eilmeldung 式）
- 选中文本 → dict / AI
- 书签保存 / 跳转
- `spc m m` / `spc b b` 及通用快捷键自定义
- 侧栏过滤框 j/k 策略（本轮仅 Nav：Links / Toc / Consult）
- 新依赖、新插件 ABI、keymap 配置语言

## 3. 行为规格

### 3.1 `O` 打开当前目录

**根因：** `keys.rs` 中 `Char('O')` 仅在 `modifiers == NONE` 时触发。Windows（及多数终端）Shift+O 带 `SHIFT`，故 `O` 失效；`Alt+o` 走另一分支故仍可用。

**修复：**

```text
Char('O') if modifiers ⊆ {NONE, SHIFT} → open_current_dir
```

与现有 `Char('V')` 行 visual 一致。

**顺带：** 扫描其它「大写字母命令」是否同样只认 `NONE`（如有同样修）。`open_external`（xdg-open / cmd start）本轮不改；Linux 本机可做最小静态检查。

### 3.2 OSC 52 文档

在 `docs/FEATURES.md` §6.5（及 README 若有 yank 一句）写清：

- OSC 52 是终端剪贴板转义序列，非本地进程剪贴板 API。
- 形态：`ESC ] 52 ; c ; <base64> BEL`（文中可写 `\e]52;c;{base64}\a`）。
- `c` = clipboard 选择；payload 为 UTF-8 文本的标准 base64。
- 依赖终端支持（Windows Terminal / WezTerm / 多数 SSH 客户端可开）；不支持则 yank 无感或失败，status 已有失败文案。
- 同类术语（若同节出现）：仅解释用户会碰到的；不写 VT 百科。

### 3.3 AI 键位翻转

| 键 | 现行为 | 新行为 |
|----|--------|--------|
| `Enter` | 换行 | **发送** |
| `Ctrl+j` | 发送 | **换行** |

改动：`src/ai.rs` 键处理 + 所有 status/title 文案（`C-j send` → `Enter send · C-j ↵` 一类）。`FEATURES.md` §6.7 同步。

### 3.4 Nav 过滤 j/k

**范围：** Links / Toc / Consult 三个 overlay 共用的 `handle_nav_key` 路径。

| 键 | 新行为 |
|----|--------|
| `j` / `k` | **始终**插入 query 字符并 refilter |
| `↑` / `↓` | 列表导航（Consult 上：`↑↓` **仍为查询历史**，与现契约一致） |
| 其它可打印 | 不变（进 query） |

**Consult 历史：** 保持仅 `↑↓`；不要用 j/k 绑历史。

实现：`nav.rs` 中 `KeyCode::Char('j'|'k')` 导航分支删除；落入通用 `Char` 过滤逻辑。

### 3.5 Windows / pkg / 路径文档

扩充 `FEATURES.md` §8 Windows 与 `pkg` 说明（README 可交叉链）：

1. **`tdd` 仅 Linux agent 别名**；Windows 用 `tuider.exe`。
2. **`tuider pkg install` 需要源码工作区**（含 `Cargo.toml` 的 workspace root）；预编译 exe 旁若无源码会报错——写明原因与替代：从源码树装，或拷贝已构建的 `tuider_*.dll` / `.so` 到 plugins 目录。
3. **配置搜索顺序**（与代码一致）：`./tuider.yml` → `./.tuider.yml` → 向上父目录 → `%USERPROFILE%\.config\tuider.yml`（Linux：`~/.config/tuider.yml`）。
4. **插件目录默认**：`~/.local/share/tuider/plugins/`（Windows 对应 `%USERPROFILE%\.local\share\tuider\plugins\` 或文档写清实际 `dirs` 解析）；`TUIDER_PLUGINS_DIR` 覆盖。
5. **pkg 错误文案**（代码小改）：`workspace root missing Cargo.toml` 后追加一行：`hint: pkg install needs a source checkout; or copy prebuilt plugin into plugins_dir`。

### 3.6 配置自动创建 + help 路径

**何时：** 仅 **进入 TUI** 时（`run` 进交互环前）。`-h` / `-V` / `pkg` / `-l` CLI / dict 无 TUI 查词 **不**写文件。

**写哪里：** 若 `config_search_paths()` 全部不存在文件，则创建 `~/.config/tuider.yml`（`dirs_config()` 用户级路径）。cwd 或父目录已有配置 → **不**创建、不覆盖。

**最小模板**（注释 + 空结构即可）：

```yaml
# tuider.yml — auto-created; see `tuider -h` / docs/FEATURES.md
# plugins_dir: ~/.local/share/tuider/plugins
# plugins:
#   url: { enabled: true }
# ai:
#   providers: []
```

**Help：**

- CLI `-h`：增加一行 `CONFIG: <loaded path | default path (not created yet)>`；若本进程刚创建则写实际路径。
- 应用内 `?` help：增加配置文件路径一行（进入 TUI 时若创建则已存在）。

实现落点：`config.rs` 增加 `ensure_user_config() -> PathBuf`（返回将使用/已加载路径）；TUI 入口调用一次。

### 3.7 `/` 搜索当前匹配颜色

`src/ui.rs` `highlight_line`：

| 角色 | 现 | 新 |
|------|----|----|
| 普通命中 | `bg = theme.search_text()`（黄） | **不变** |
| 当前命中 | `bg = Rgb(250,179,135)` 桃 | **`bg = Color::Red`**（或 `Rgb(220,50,47)` 实红） |

前景保持 `status_focus_fg()` 以保证对比。仅改 current 样式常量。

### 3.8 删除 `-u` / `--url`

破坏性：显式 flag 不再认领；**裸 `http(s)://` 仍认领**。

| 位置 | 动作 |
|------|------|
| `plugin_catalog.rs` `claims("url")` | 去掉 `-u`/`--url` |
| `tuider-plugin-url` `handles` / `open` | 只解析裸 URL；缺则错误提示改文案 |
| `main.rs` host 选项跳过列表 | 去掉 `-u`/`--url`（避免当路径） |
| 测试断言 | 更新 |
| FEATURES / README / plugins.md / help | 主推裸 URL；注明 flag 已移除 |

### 3.9 URL 网络错误

`crates/tuider-plugin-url` `http_get` / `fetch_url_markdown`：

映射 `reqwest::Error`：

| 条件 | 前缀 |
|------|------|
| `is_timeout()` | `timeout` |
| `is_connect()` | `connect` |
| `is_request()` | `request` |
| 其它 | `network` |

若已有 HTTP 响应且非 success：`http {status}`（可附 `reason_phrase` 短串）。

最终：`url plugin: {kind}: {detail}`，**不**倾倒 HTML body。透传到 host 已有 `plugin … error …` 路径即可。

## 4. 代码落点（最短）

| 文件 | 改动 |
|------|------|
| `src/app/keys.rs` | `O` + SHIFT |
| `src/app/nav.rs` | j/k 进 query |
| `src/ai.rs` | Enter/C-j 翻转 + 文案 |
| `src/ui.rs` | 当前匹配红；help 配置路径 |
| `src/config.rs` | `ensure_user_config` / 路径展示 helper |
| `src/main.rs` | TUI 入口 ensure；help CONFIG；去 `-u` 跳过 |
| `src/plugin_catalog.rs` | 去 url flag claims |
| `crates/tuider-plugin-url/src/lib.rs` | 去 flag；错误分类 |
| `src/pkg.rs` | missing Cargo.toml hint |
| `docs/FEATURES.md` 等 | OSC52、AI 键、Windows/pkg、URL CLI、颜色一句 |

## 5. 验收

```fish
cargo build
cargo test -q
# 手动 / 逻辑核对：
# 1. sidebar off + Shift+O → open dir（或 status 非 silent ignore）
# 2. Alt+f / f / o 过滤框可输入 j、k；↑↓ 移动
# 3. AI：Enter 发送，C-j 换行
# 4. / 搜索 n/N：当前命中红，其它黄
# 5. 无配置时进 TUI → ~/.config/tuider.yml 出现；-h 显示 CONFIG 路径
# 6. tuider https://example.com 仍可（有 so）；tuider -u URL → 不再当 url flag
# 7. 断网/坏 URL → 错误含 timeout|connect|http NNN
```

## 6. 实施顺序

1. `O` SHIFT + Nav j/k  
2. AI 键翻转  
3. 搜索当前红  
4. config ensure + help 路径  
5. 删 `-u`/`--url` + URL 错误分类 + pkg hint  
6. 文档（FEATURES/README/plugins）  
7. `cargo build` + `cargo test`  

## 7. 自检

- [x] 无 TBD/占位  
- [x] 与延后项无交叉实现  
- [x] j/k 与 Consult 历史（↑↓）不矛盾  
- [x] `-u` 删除范围列全（catalog/plugin/main/docs/tests）  
- [x] 配置只在进 TUI 创建、不覆盖已有  
