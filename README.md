# Tuider

**Tuider** = **T**erminal **UI** **r**ea**der**.

**本体尽量小**：只含 md/txt 阅读 +（默认）AI。  
**插件**：独立编译为 `.so`，**拷贝到插件目录才能用**（方案 A）。

工作目录：`~/cleantest/tuider`（不要在 `mdx-tui` 里跑）。

## 文档

| 文件 | 内容 |
|------|------|
| [docs/STATUS.md](docs/STATUS.md) | 现状 |
| [docs/NEXT.md](docs/NEXT.md) | 下一步 |
| [docs/plugins.md](docs/plugins.md) | **动态插件加载** |
| [docs/compare/MATRIX.md](docs/compare/MATRIX.md) | **vs mdx-tui 功能矩阵** + [smoke](docs/compare/README.md) |
| [docs/complexity-review.md](docs/complexity-review.md) | 复杂度 |
| [docs/PLAN.md](docs/PLAN.md) | 决策 |
| [docs/refactor-design-and-planning.md](docs/refactor-design-and-planning.md) | mdx-tui→Tuider 重构方法与规划 |

## 快速开始

```fish
cd ~/cleantest/tuider

# 本体
cargo build
cargo run -- README.md

# 插件
./scripts/install-plugins.sh
# 或单独：
cargo build -p tuider-plugin-url
mkdir -p ~/.local/share/tuider/plugins
cp target/debug/libtuider_url.so ~/.local/share/tuider/plugins/

# 没有 so 时：
TUIDER_PLUGINS_DIR=/tmp/empty cargo run -- -u https://example.com
# → need plugin `url` — copy .so to ...
```

## 插件目录

默认：`~/.local/share/tuider/plugins`  
覆盖：`TUIDER_PLUGINS_DIR` 或 yml `plugins_dir:`

| 插件包 | 产出 so（Linux） |
|--------|------------------|
| `tuider-plugin-url` | `libtuider_url.so` |
| `tuider-plugin-hn` | `libtuider_hn.so` |
| `tuider-plugin-dict` | `libtuider_dict.so` |

## Core vs 插件

| Core（在 bin 内） | 插件（仅 .so） |
|-------------------|----------------|
| md/txt + **code 高亮**（syntect） | url |
| AI（default feature） | hn |
| vim / visual yank | dict |

## 配置

`~/.config/tuider.yml`：`ai.providers` + `plugins.*.enabled`（有 so 时的额外门禁）。

## License

待定。dict 若启用 mdict（AGPL）注意发行合规。
