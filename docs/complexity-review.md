# Tuider 复杂度审查（2026-07-22 host boundary）

## plugin_catalog
Verdict: leave  
Evidence: claims + pkg + missing hint 单源；无第二份 match

## cache
Verdict: leave (deleted)  
Evidence: host 死代码已删；插件自管磁盘缓存

## PluginTextSource vs ContentSource
Verdict: leave  
Evidence: 文本源 / 渲染源命名分离

## HTML_V1
Verdict: leave (documented); deepen later  
Evidence: 常量 + 文档；渲染仍在 host（二期）

## App InputMode
Verdict: leave  
Evidence: 路由收敛；ui getter 面仍宽（View 快照二期）
