# Tuider 插件机制（方案 A：动态 `.so`）

## 你要的分离，现在怎么落地

| 要求 | 实现 |
|------|------|
| 本体尽量小 | 主包 **不再静态链接** url/hn/dict/code |
| 拷贝插件文件才能用 | 只有 `plugins_dir` 里存在对应 `.so` 才能 `-u`/`-hn`/… |
| 独立包 | `crates/tuider-plugin-*` 编成 **cdylib** |
| 发行知识单源 | host `src/plugin_catalog.rs`：claims / 缺 so 提示 / `pkg` |

```
~/.local/share/tuider/plugins/     # 默认目录（可用 TUIDER_PLUGINS_DIR 或 yml plugins_dir）
  libtuider_url.so
  libtuider_hn.so
  ...
```

**没有文件 = 没有功能**（不是配置里假装关掉）。

## plugin_catalog（host）

`plugin_catalog` 是 id → so / crate / summary / **claims** 的**唯一**表：

- `main`：`handles_args || catalog.claims` 决定是否 `open`；否则 `missing_plugin_hint` 打 `need plugin …`
- `pkg list|install|remove`：同一 `CATALOG`（id / crate / so / summary）
- **无**第二份 id→flag `match`

claims 语义（冻结）：url（`-u`/`--url`/裸 http(s)）、hn（`-hn`/`--hn`）、code（`--code`）、dict（`-g`/`--group`/`.mdx`）。

## 构建与安装

或用主机子命令（在源码树内）：

```fish
cargo run -- pkg list
cargo run -- pkg install url
cargo run -- pkg install all
cargo run -- pkg remove hn
```

```fish
cd ~/cleantest/tuider

# 本体
cargo build --release

# 插件（示例：url）
cargo build -p tuider-plugin-url --release
mkdir -p ~/.local/share/tuider/plugins
cp target/release/libtuider_url.so ~/.local/share/tuider/plugins/
```

其它插件同样：`tuider-plugin-hn` → `libtuider_hn.so`（crate 名见各 `Cargo.toml` `[lib] name`）。

## 运行

```fish
# 无插件目录 → 拒绝 -u
TUIDER_PLUGINS_DIR=/tmp/empty tuider -u https://example.com

# 有 .so → 可用
tuider -u https://example.com
```

配置（可选）：

```yaml
plugins_dir: /path/to/plugins
plugins:
  url:
    enabled: true   # false = 即使有 .so 也不加载
```

`enabled: false` 是**额外门禁**；**不能替代**「文件必须在目录里」。

## ABI（v1）

每个 `.so` 导出（见 `tuider-plugin-api`）：

- `tuider_plugin_abi_version` / `id` / `name` / `open` / `close`
- `tuider_source_title` / `entry_count` / `entry_at` / `load_body`
- `tuider_string_free`

插件返回 **UTF-8 文本**；主机负责渲染。  
**禁止**跨 so 传 ratatui 类型。

### Body 格式与 `BODY_HTML_V1_PREFIX`

默认：UTF-8 markdown 或纯文本（host 走 md/plain）。

可选 **HTML 信封**（不 bump ABI；值为 body 约定）：

1. 正文以常量 `BODY_HTML_V1_PREFIX`（`"TUIDER_HTML_V1\n"`）开头  
2. 随后 payload：`css + "\n\u{1e}\n" + html`  
3. host `loader` 识别前缀后做 CSS 子集 → ratatui Lines  

API 侧适配后文本源 trait 名：`PluginTextSource`。App 侧已渲染源仍为 host `ContentSource`。

## 与旧「Cargo features 插件」的区别

| 旧 | 新（A） |
|----|---------|
| `cargo build --features url` 把代码链进 bin | 主 bin **不**链 url |
| yml 开关只挡入口 | **无 so 则无代码** |
| 源码独立但产物一体 | 源码独立且**产物独立** |
| host 平行 claims 表 | **plugin_catalog** 单源 |

遗留 feature 名 `url`/`hn`/… 在 host 上为空兼容，**不会**再拉进插件依赖。

## 当前实现进度

- [x] ABI + host `loader.rs`（libloading / dlopen）
- [x] `plugin_catalog` + `pkg` 同源
- [x] `tuider-plugin-url` → `libtuider_url.so`
- [x] `tuider-plugin-hn` → `libtuider_hn.so`
- [x] `tuider-plugin-code` → `libtuider_code.so`
- [x] `tuider-plugin-dict` → `libtuider_dict.so`
- [x] `BODY_HTML_V1_PREFIX` + host 渲染
- [x] 帮助列出已加载插件与 plugins 目录
- [x] `scripts/install-plugins.sh`
