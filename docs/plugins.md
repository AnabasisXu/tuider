# Tuider 插件机制（方案 A：动态 `.so`）

## 你要的分离，现在怎么落地

| 要求 | 实现 |
|------|------|
| 本体尽量小 | 主包 **不再静态链接** url/hn/dict/code |
| 拷贝插件文件才能用 | 只有 `plugins_dir` 里存在对应 `.so` 才能 `-u`/`-hn`/… |
| 独立包 | `crates/tuider-plugin-*` 编成 **cdylib** |

```
~/.local/share/tuider/plugins/     # 默认目录（可用 TUIDER_PLUGINS_DIR 或 yml plugins_dir）
  libtuider_url.so
  libtuider_hn.so
  ...
```

**没有文件 = 没有功能**（不是配置里假装关掉）。

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

插件返回 **UTF-8 文本**；主机负责 md 渲染。  
**禁止**跨 so 传 ratatui 类型。

## 与旧「Cargo features 插件」的区别

| 旧 | 新（A） |
|----|---------|
| `cargo build --features url` 把代码链进 bin | 主 bin **不**链 url |
| yml 开关只挡入口 | **无 so 则无代码** |
| 源码独立但产物一体 | 源码独立且**产物独立** |

遗留 feature 名 `url`/`hn`/… 在 host 上为空兼容，**不会**再拉进插件依赖。

## 当前实现进度

- [x] ABI + host `loader.rs`（libloading）
- [x] `tuider-plugin-url` → `libtuider_url.so`
- [x] `tuider-plugin-hn` → `libtuider_hn.so`
- [x] `tuider-plugin-code` → `libtuider_code.so`
- [x] `tuider-plugin-dict` → `libtuider_dict.so`
- [x] 帮助列出已加载插件与 plugins 目录
- [x] `scripts/install-plugins.sh`
