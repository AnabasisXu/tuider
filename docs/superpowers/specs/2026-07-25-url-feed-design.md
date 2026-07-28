# Featured 审核第三轮：URL RSS/Atom feed

> 日期：2026-07-25  
> 状态：已定稿（待实现）  
> 前置：第一轮 fixes、第二轮选区→dict/AI 已交付  
> 策略：最短 diff；仅改 `tuider-plugin-url`（+ 文档）；宿主侧栏多 entry 已具备；不 bump ABI

## 1. 结论

| # | 项 | 类型 |
|---|----|------|
| 1 | 裸 URL 拉取后嗅探 RSS/Atom | 行为 |
| 2 | feed → 多 entry 侧栏列表 | 行为 |
| 3 | 条目正文优先 feed 内 content/description | 行为 |
| 4 | 非 feed 仍 HTML→markdown | 兼容 |
| 5 | FEATURES / plugins 文档一句 | docs |

**硬边界（已确认）**

- **实现落点：** `crates/tuider-plugin-url` + `feed-rs`；**不**新建 so、**不**改 claims（仍裸 `http(s)://`）。
- **正文：** 只用 feed 内字段；**不**为每条二次 `http_get(link)`。
- **检测：** 拉 body 后嗅探（Content-Type **或** XML 根 + `feed-rs` 成功）；失败回落现有页面路径。
- **缓存：** feed 按 URL 缓存原始响应体（磁盘）；命中则不再联网（与现 page 缓存策略同级：长期文件，无强制 TTL 本轮）。

## 2. 非目标

- eilmeldung / news-flash 订阅库、OPML、unread/mark  
- 打开条目时抓取文章页（readability）  
- 打开 feed 时预取全部 link  
- 书签、leader、`spc`  
- 主二进制新依赖、插件 ABI 变更、host API 扩展  

## 3. 行为规格

### 3.1 入口

与现网一致：`tuider https://…` → catalog/plugin `url` → `tuider_plugin_open`。

### 3.2 拉取与分类

1. `validate_fetch_url` + `http_get`（保留 timeout/connect/http N 分类错误）。  
2. 得到 `(content_type_opt, body: String)`。  
   - **实现注：** 今日 `http_get` 只返回 body；扩展为返回 headers 中的 Content-Type **或** 在 body 嗅探足够时可不传 CT。  
3. **尝试 feed：**

| 条件 | 动作 |
|------|------|
| CT 含 `xml` / `rss` / `atom` / `rdf`（大小写不敏感） | 走 `parse_feed` |
| 否则 body trim 后以 `<` 开头 | 仍尝试 `parse_feed` |
| `feed-rs` 解析 **成功** 且 **≥1 item** | `UrlState` = Feed 模式 |
| 解析失败或 0 item | **回落** `html_to_markdown` 单页（现路径） |

4. 空 body → 错误（`request`/`network` 类既有文案即可）。

### 3.3 Feed 状态

```text
struct FeedItem {
  title: String,      // 展示名；空 → "Untitled" 或截断 link
  body_md: String,    // load_body 返回
  link: Option<String>,
}

struct UrlState {
  title: String,           // channel/feed 标题；失败则用 URL
  items: Vec<FeedItem>,    // Page 模式：len == 1，title=页标题
}
```

ABI 映射（不变）：

| 符号 | 行为 |
|------|------|
| `tuider_source_title` | `state.title` |
| `tuider_source_entry_count` | `items.len()` |
| `tuider_source_entry_at(i)` | `items[i].title` |
| `tuider_source_load_body(i)` | `items[i].body_md` |

宿主：`entries.len() > 1` ⇒ 侧栏开（已有 `single_entry` 逻辑）；**无需改 App**。

### 3.4 条目正文构造

对每个 feed item，按序：

1. **完整 HTML 内容**（`feed-rs` content / content:encoded 一类）→ 转成 body 文本：  
   - **最短可用：** 剥标签得纯文本，或对片段跑现有可读/轻量 HTML→md 路径（**禁止**为整站再拉 link）。  
   - 若插件内已有 `html_to_markdown` 依赖整页 meta，允许抽一个 **snippet→markdown** 小函数（可只做标签剥离 + 保留换行）。  
2. 否则 **summary/description** 同样处理。  
3. 否则：

```markdown
# {title}

> link: {link_or_none}

(no content in feed)
```

正文前可统一加：

```markdown
# {title}

> link: {url}

{body}
```

（title 已是 entry 标题时避免双重 `#` 亦可：仅 `> link` + body — **实现选一种并在单测固定**。）

### 3.5 列表标题

- `entry_at` 字符串 = item.title（trim）；空则 `Untitled`；可选后缀日期 **本轮不做**。  
- 超长标题截断到 **120** 字符（防止侧栏炸宽）。

### 3.6 缓存

| 模式 | 键 | 值 |
|------|----|----|
| Page（现） | `page_rel(url)` | markdown |
| Feed | `page_rel(url)` 或 `feed/` + 同 rel | **原始 body 字符串**（XML） |

打开时：

1. 若缓存命中：  
   - 先当 feed 解析；成功 → Feed  
   - 失败 → 当 markdown 单页（兼容旧 page 缓存）**或** 当 HTML 再转（实现选最短：若解析失败且缓存看起来像 `# ` 开头 md → 单 entry Page）  
2. 未命中：联网 `http_get` → 分类 → 写缓存（feed 写 raw；page 写 md，与现一致）

**本轮无 TTL、无 `--sync`。** 清缓存仍靠用户删 cache 目录。

### 3.7 错误

- 网络：沿用 `map_reqwest_err` / `http {status}`。  
- feed 解析失败且不回落：仅当明确 CT 为 feed **且** 0 item 时，可报 `url plugin: empty feed`；否则回落 HTML。  
- **不**倾倒整份 XML 到 err。

### 3.8 文档

- `docs/FEATURES.md`：URL 能力 — 裸 URL 若为 RSS/Atom 则侧栏条目列表，正文来自 feed。  
- `docs/plugins.md`：url 插件 — 同上；依赖 `feed-rs`。  
- STATUS 可选补一条验证句（有则改）。

## 4. 代码落点

| 文件 | 改动 |
|------|------|
| `crates/tuider-plugin-url/Cargo.toml` | `feed-rs` |
| `crates/tuider-plugin-url/src/lib.rs` | `http_get` 可带 CT；`looks_like_feed` / `parse_feed_items`；`UrlState` 多 entry；缓存分支 |
| `crates/tuider-plugin-url` 测试 | fixture 字符串 RSS 2.0 + Atom；断言 count/title/body 含 description |
| `docs/FEATURES.md` / `docs/plugins.md` | 行为一句 |

**宿主 / ABI / catalog：不改。**

## 5. 验收

```fish
cargo build -p tuider-plugin-url
cargo test -p tuider-plugin-url -q
./scripts/install-plugins.sh   # 刷新 libtuider_url.so
cargo build                    # 刷新 tdd 用主 bin（若需）
# 手动：
# tdd https://sachachua.com/blog/category/emacs-news/feed
#   → 侧栏多条；打开一条见 description 文本；visual/d/a 仍可用
# tdd https://example.com
#   → 仍单页 Example Domain
# 断网 + 已缓存 feed → 仍可开列表（磁盘缓存）
```

## 6. 实施顺序

1. 加 `feed-rs`；纯函数 `parse_feed_items(xml) -> Result<(title, Vec<FeedItem>), _>` + fixture 单测  
2. 改 `open`：get → try feed → else page；多 entry ABI  
3. 缓存：feed raw vs page md  
4. 文档 + `install-plugins` / 手测  

## 7. 自检

- [x] 无 TBD/占位  
- [x] 不二次抓 link  
- [x] 非 feed 路径保持  
- [x] 无 host/ABI 变更  
- [x] 书签/leader/订阅库不在范围  
