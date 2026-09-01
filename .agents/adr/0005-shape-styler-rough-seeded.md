# ADR-0005: ShapeStyler 双风格器 + 确定性种子手绘（零新依赖）

- 状态：已接受
- 日期：2026-08-29（Phase F，commit `3a21aef`；`d6177b8` 修订曲线路径）
- 参考：[plans 归档说明](../CHANGELOG.md)、spec §2.2、rough.js 对照见原 rough-style-plan（已归档）

## 背景

手绘风描边（Excalidraw 观感）有现成 crate `roughr`，但引入即多一整棵依赖树；
且手绘抖动必须**可复现**——否则每次重绘（缩放、平移、存盘重开）轮廓跳变。

## 决策

- **零新依赖**，自移植 rough.js 算法：`preferz-core` 放确定性 PRNG `SeededRng`
  （xorshift64\*，seed=0 用黄金比例常数兜底）+ `ItemKind::Shape { seed, rough }` 字段
- binary 层 `ShapeStyler` trait 两实现，`build_shape_visuals()` 按 `rough` 统一分发，
  render_scene 与 Present 模式共用：
  - `CleanStyler`：精确几何（convex_polygon / line / dashed_line）
  - `RoughStyler`：逐边抖动三次贝塞尔 × 2 passes（笔触）；**箭头不抖**（Excalidraw 同款）
- 抖动幅度以**画布像素**计量再乘 zoom（对齐 ADR-0002），放大观感与真实手绘稿一致
- 曲线轮廓（椭圆）不走逐边抖动：整圈采样点独立抖动 + Catmull-Rom 插值成 C1 连续
  贝塞尔闭合环，否则采样折角刺眼；`is_smooth()` 判定，将来曲线类型只需扩此判断
- **填充保持精确几何**：本轮只手绘化描边，填充仍是凸多边形（hachure 排后，见 plan.md D5）

## 后果

- ✅ 同 seed 恒得同一轮廓（缩放/重开不跳变），单测直接断言确定性
- ✅ 依赖树不变，编译时间不涨
- ⚠️ rough.js 后续特性（hachure 填充等）需继续自移植
