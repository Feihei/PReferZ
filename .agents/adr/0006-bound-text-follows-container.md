# ADR-0006: 绑定文本跟随容器，不可独立选中

- 状态：已接受
- 日期：2026-08-31 ~ 09-01（`ed538cf` + `8eb996a`）
- 参考：spec §2（文本入形）、Excalidraw label 语义

## 背景

绑定在封闭形状上的文本曾有两个割裂：渲染位置按容器实时居中，但它自身是独立
Scene item——可被点选/框选并单独拖动，而拖动只改其创建时的 transform 快照，
选中框与文字、图形三者错位。

## 决策

对齐 Excalidraw label 语义：**绑定文本不是独立交互对象**。

- 命中重定向：`interaction::get_item_at()` 命中绑定文本时返回其容器（点 label
  即点容器）；点选、拖动、双击编辑、hover 光标四条路径共用此入口
- 框选过滤：BoxSelect 跳过绑定文本；容器被框选时经 `Scene::texts_bound_to()` 联动
- `Scene::is_bound_text()` 判定"绑定在**存活**容器上的文本"；容器删除后的孤儿引用
  按自由文本交互（`cleanup_orphan_containers()` 清引用）
- 渲染：每帧按容器实时 `bounding_rect()` 居中排版；文字背景默认全透明
  （`background: Option<[u8;4]>` 字段预留，见 plan.md Phase H 侧栏）

## 后果

- ✅ 文字永远随图形联动，不存在"单独移动的框"
- ✅ 编辑入口唯一化（双击容器 / Enter / 点容器），无双路径漂移
- ⚠️ 绑定文本自身的 transform 从此只是历史快照，不再有任何渲染语义
