# 下一步目标

> 目录：`~/cleantest/tuider`

## 已完成（方案 A + P0 + pkg）

- [x] 动态 `.so` ABI v1 + host `loader.rs`
- [x] url / hn / code / dict 均导出 ABI
- [x] `scripts/install-plugins.sh` + `tuider pkg list|install|remove`
- [x] AI 429/5xx 自动换下一个 provider
- [x] dict：mdx-tui HTML+CSS→Lines（非 HTML→md 纯文本）

## P1

1. [x] app 键位拆分（`src/app/{keys,search,visual}.rs`）  
2. [x] visual 字符级 `v` + 行级 `V`  
3. [x] vim 匹配计数 status + 当前命中高亮  
4. dict 大库索引 / code 高亮（在**插件包内**）  
5. ABI 版本协商与 `tuider --list-plugins`  
6. release strip / 体积

## 不做

- 把插件再链回主 bin  
- 动态 so 之前的「yml 假开关当分离」
