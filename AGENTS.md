# Agent notes (tuider)

## 用户验证方式（强制）

- 用户用 fish 别名 **`tdd`** 测 TUI：`tdd` → `target/debug/tuider`（**不会**编译）。
- 定义位置：`~/.config/fish/config.fish`：`alias tdd '/root/cleantest/tuider/target/debug/tuider'`。
- 改代码后 agent **必须** `cargo build`（或会产出 debug bin 的 `cargo test`），刷新 `target/debug/tuider`。
- **禁止**只跑 `cargo check` 就当可测完：`check` 不更新该二进制。
- **禁止**让用户自己 `cargo run` / `cargo build` 才能看到改动。
- 用户侧只需：`tdd README.md`（或其它参数）。

## 其它

- shell 是 fish；命令用 fish 语法。
- 回复与思考用中文。
- Ponytail 活跃时保持最短 diff。
