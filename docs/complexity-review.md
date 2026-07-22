# Tuider 复杂度审查（更新：独立插件 crate）

## 插件拆分后

```
Verdict: deepen
Evidence: 每个插件是独立 package，只依赖 tuider-plugin-api；host 通过 feature + config.plugin_enabled 双重门闸
Change: 保持；勿让插件 crate 依赖 app/ui
Rejected alternative: 动态 dlopen —— 接口与发布复杂度更高
```

## AI

```
Verdict: leave
Evidence: 非流式回退 + 多 provider；错误从 SSE/HTTP 拉到模块内；宽屏右侧布局
Change: provider 503 等服务端错误直接显示在气泡，不崩溃
Rejected alternative: 整 crate 依赖 mdx-ai —— 会拖入 dict tools 等，边界变浅
```

## App

```
Verdict: simplify-interface（债）
Evidence: handle_key 仍宽
Change: 后续按 Visual/Vim/Ai/Read 拆分
Rejected alternative: 现在大重构阻塞测试
```

## 配置

```
Verdict: leave
Evidence: plugin_enabled 把「编进去」和「允许用」拆开，避免配置 knobs 与 feature 混为一谈
```
