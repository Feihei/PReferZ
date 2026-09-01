# PReferZ 对齐 Excalidraw：主题 / 属性侧栏 / 图形类型统一 实施规划

> **状态（2026-09-01）**：规划定稿，待拍板决策点后分 Phase 实施。文档先提交，实现拆 commit。
> **参考**：`.ref/excalidraw`（`packages/element/src`：`newElement.ts`、`linearElementEditor.ts`、
> `renderElement.ts`、`showSelectedShapeActions.ts`）。
> **惯例**：零新依赖；所有 item 变更走 undo 栈（`push_cmd`）；`.prz` serde 迁移用缺字段 default。

**Goal:** 从三个方向对齐 Excalidraw 的核心体验——①明暗两套样式主题；②选中对象后在
侧栏集中编辑属性；③直线/多段线/曲线/不规则多边形统一为一类线性对象，支持端点类型、
弯曲类型、填充类型。

**Architecture:** 主题层新增 `ThemeMode` + palette 模块集中管理主题化颜色（UI chrome 走
egui `Visuals`，画布语义含底色与新建元素默认色）；交互层新增右侧 `SidePanel` 属性面板，
按 `ItemKind` 分节渲染，所有改动经 `SetItemStyle` 类命令入 undo 栈；数据层把
`ShapeType::Polyline` 升级为完整线性对象（`CurveType` + 扩展 `ArrowHeadStyle` + 闭合
多边形），闭合 Polyline 即不规则多边形，不新增独立类型。

**Tech Stack:** Rust、egui/eframe 0.29、serde。**三个 Phase 均零新依赖。**

---

## 1. 现状盘点

| 现状 | 与目标差距 |
|---|---|
| `ItemKind::Shape { shape_type, base_size, points, stroke, fill, start/end_arrow, closed, seed, rough }` | `Polyline` 已是 points 驱动，但无弯曲类型；箭头仅 `Arrow` 一种；无 roundness |
| `StrokeStyle { color, width, dash }`、`fill: Option<[u8;4]>` | 纯色填充已有；无 hachure 类填充风格 |
| 无主题系统：UI 用 egui 默认 Visuals，画布/默认色散落硬编码 | 无 Light/Dark，暗底上默认白描边恰好可用但不可切换 |
| 仅左侧工具 `SidePanel`；样式只能靠双击/Enter 编辑文本 | 选中对象后无属性面板；描边/填充/手绘风暂无 UI 入口 |
| 绑定文本已不可独立选中（命中重定向容器），渲染随容器实时居中 | ✓ 已具备容器联动基础 |

## 2. Phase G — 明暗两套样式主题（小，独立）

- [ ] G1 `ThemeMode { Light, Dark }`（预留 `Auto`），持久化进 `~/.preferz/config.json`
      （复用 keymap 的手写 JSON 路径）
- [ ] G2 新增 `palette` 模块：集中定义画布底色、默认描边色、默认文字色、
      选中框/手柄色等 keyed 颜色，按主题取值；清理散落硬编码
- [ ] G3 UI chrome：切换 egui `Visuals::light()/dark()`
- [ ] G4 画布语义：主题同时决定**画布底色**与**新建元素默认颜色**（Excalidraw 语义：
      Light 默认黑描边，Dark 默认白描边）
- [ ] G5 设置面板/工具栏加主题切换入口
- [ ] G6 旧 `.prz` 不受影响（元素颜色跟随 item 自身，不随主题重着色）

**非目标**：不做用户自定义调色板；不做跟随系统的 Auto 探测（字段预留，实现后补）。

## 3. Phase H — 选中弹出属性侧栏（中）

- [ ] H1 右侧 `SidePanel`（宽 ~260px），场景无选中时隐藏；单选/多选均支持
- [ ] H2 按 `ItemKind` 分节：
  - Shape：描边色/宽/线型（Solid/Dashed/Dotted）、填充（含"无填充"）、手绘风开关、
    端点箭头（线性对象）、闭合开关（线性对象）
  - Text：字号/颜色/背景色开关（消费既有 `background: Option<[u8;4]>` 预留字段）
  - Pixmap：透明度/灰度/裁剪入口
  - Frame：编号编辑入口
- [ ] H3 所有改动经命令入 undo 栈；滑块拖动等连续修改合并为单条命令
      （沿用既有拖拽固化的 skip_first_redo 模式）
- [ ] H4 多选：显示交集属性，可批量改；属性互斥的控件禁用
- [ ] H5 画完图形立即选中（已具备）→ 侧栏即弹，绘制→调属性零额外点击

**非目标**：不做浮动式工具条（Excalidraw 顶条形态）；不做属性动画过渡。

## 4. Phase I — 图形元素类型统一（大，动数据模型）

- [ ] I1 `CurveType { Straight, Curved }` 加入线性对象（`Polyline`）：Curved 用
      Catmull-Rom 穿过顶点，推广 `RoughStyler::closed_catmull_rom` 补**开曲线**版本；
      Clean/Rough 两个风格器同步支持
- [ ] I2 不规则多边形 = `closed: true` 的线性对象，**不新增类型**；直线 = 2 顶点特例
- [ ] I3 `ArrowHeadStyle` 扩展：`Arrow` / `Dot`；`Option<ArrowHeadStyle>` 语义不变
- [ ] I4 矩形族 `roundness: Option<f32>`（圆角半径，None = 直角）；椭圆忽略
- [ ] I5 填充第一版维持纯色；hachure 斜线阴影排后续（与手绘风观感配套时再做）
- [ ] I6 `.prz` serde 迁移：新字段全部 `#[serde(default)]`，旧文件无损加载
- [ ] I7 交互适配：端点拖拽对 Curved 同样成立（改 points 即改曲线形状）；
      命中检测 Curved 按采样折线近似

**非目标**：不做 elbow 折线（决策点 D4）；不做贝塞尔手柄编辑（Curved 无控制点暴露，
用户只拖顶点）。

## 5. 决策点（待拍板）

| # | 问题 | 选项 | 倾向 |
|---|---|---|---|
| D1 | 主题三态 | 仅 Light/Dark vs 增 `Auto`（跟随系统） | 字段预留 `Auto`，第一版只做手动切换 |
| D2 | 画布底色 | 跟主题走 vs 独立可设 | 跟主题走，独立设置留到有需求再说 |
| D3 | 多选属性面板 | 显示交集 vs 禁用面板 | 显示交集可批量改，Excalidraw 同款 |
| D4 | elbow 折线 | 本轮做 vs 排后 | 排后：路由算法复杂度不成比例 |
| D5 | hachure 填充 | 本轮做 vs 排后 | 排后：纯色先统一数据模型 |

## 6. 实施顺序与依赖

```
G（主题，独立）──┐
                 ├→ H（侧栏）── 侧栏属性项依赖 I 定型的字段集合
I（类型统一）───┘
```

推荐 **G → I → H**：先定型数据模型，侧栏一次到位；G 完全独立可先行。
若想先看到交互价值、接受 H 的小幅返工，可 G → H → I。

## 7. 验收清单（每 Phase 通用）

- [ ] `cargo fmt --check` / `cargo clippy -D warnings` / `cargo test --workspace` 全绿
- [ ] 旧 `.prz` 文件加载无损（I6）
- [ ] 全程 undo/redo 行为正确（H3/I1-I4）
- [ ] GUI 手工验收：明暗切换观感 / 侧栏改属性即见 / 直线↔曲线↔多边形互转
