# PReferZ 线性对象统一重构设计（Line/Arrow → Polyline + 端点箭头）

- 日期：2026-08-17
- 状态：已评审（设计讨论定稿，待实施）
- 配套：`.agents/shapes-and-frame-slides-design.md`（本文档取代其中 §3.1 / §4 / §5 关于 Line/Arrow 两变体的描述）
- 关键决策：**不向后兼容**（移除 `ShapeType::Arrow`，旧 `.prz`/`.bee` 文件不做 serde 迁移）

## 1. 背景与目标

### 现状（Phase B）

- `ShapeType::Line` 与 `ShapeType::Arrow` 是两个独立变体
- 箭头只出现在终点，且写死在渲染逻辑里（`stylers.rs` 判断 `shape_type == Arrow`）
- `points` 固定 2 个顶点

### 目标

- **直线和箭头是同一类对象**：直线 = 只有 2 个顶点的 Polyline
- **箭头是对象属性**：起点 / 终点各自独立可配（`start_arrow` / `end_arrow`）
- **为多段线 / 曲线铺路**：`points` 为顶点列表（N ≥ 2），未来多段线（N 顶点）与曲线（顶点带曲线句柄 / 曲线模式）并入此类

### 非目标

- 多点线编辑 UI（加点 / 拖点）→ 后续多段线阶段
- 曲线渲染 → 后续阶段（模型已预留）
- 描边 / 填充改为编辑选中项 → 不在本次范围（仅箭头做编辑选中项，见 §5）

## 2. 数据模型（preferz-core）

### 2.1 `shape.rs`

```rust
/// 图形类型。Polyline 为线性对象：直线 = 2 顶点，未来多段线 / 曲线同用此类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapeType {
    Rectangle,
    Ellipse,
    Diamond,
    Polyline,          // 取代 Line + Arrow
}

/// 端点箭头样式。`Option<ArrowHeadStyle>` 表示"该端无箭头"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArrowHeadStyle {
    Arrow,             // 标准三角箭头
}
```

### 2.2 `item.rs` — `ItemKind::Shape` 新增字段

```rust
ItemKind::Shape {
    shape_type: ShapeType,
    base_size: (f32, f32),               // 矩形族 = w×h；Polyline = points 包围盒
    points: Vec<(f32, f32)>,             // Polyline 顶点，局部坐标，N ≥ 2
    stroke: StrokeStyle,
    fill: Option<[u8; 4]>,
    start_arrow: Option<ArrowHeadStyle>, // 新增：起点箭头（矩形族忽略）
    end_arrow: Option<ArrowHeadStyle>,   // 新增：终点箭头（矩形族忽略）
    seed: u64,
}
```

- 现有所有 `matches!(shape_type, ShapeType::Line | ShapeType::Arrow)` 判断（`set_line_points`、`base_size()`、命中检测、`transform_handles::is_line`）统一改为 `ShapeType::Polyline`。
- `Item::new_shape_line(...)` 改为接受起止箭头参数的构造器（`new_polyline(points, base_size, start_arrow, end_arrow, ...)`）。
- `EditShapePoints` 命令不变（通用 points 编辑）。

### 2.3 命中检测（多段线距离）

Polyline 命中 = 点到**全部连续线段**（N-1 段）距离的最小值，阈值 = `max(stroke_width, 6px) / zoom`。2 顶点时退化为现有点到线段距离，N 顶点时即为多段线命中——顺带为多段线阶段铺路。

## 3. 渲染（binary 层）

### 3.1 `stylers.rs` — `build_line_shapes`

- **折线绘制**：`points` 依次连成 polyline（N-1 段），不再区分 Line / Arrow 变体。
- **起点箭头**：在 `points[0]` 处沿首段方向**反向**后退的头部，仅当 `start_arrow == Some(Arrow)` 时绘制。
- **终点箭头**：在 `points[N-1]` 处沿末段方向**正向**前进的头部，仅当 `end_arrow == Some(Arrow)` 时绘制。
- 箭头几何沿用现有实现（终点 ±50° 两短线段，`head_len` 按 stroke 宽度 / 缩放）。

### 3.2 创建预览（`preferz_app.rs`）

拖拽创建时的临时预览同样按工具的默认箭头绘制（Line 工具无箭头，Arrow 工具终点箭头），与正式渲染一致。

## 4. 工具与交互（preferz_app.rs）

- 工具栏**保留 Line / Arrow 两个按钮**（已确认）：
  - **Line 按钮** → 创建 Polyline，`start=None, end=None`
  - **Arrow 按钮** → 创建 Polyline，`start=None, end=Some(Arrow)`
- `Tool` 枚举调整：`ShapeType::Arrow` 移除后，线类工具需携带默认终点箭头：
  - `Tool::Shape(ShapeType)` 保留矩形族；线类用 `Tool::Linear { end_arrow: Option<ArrowHeadStyle> }`（Line = None，Arrow = Some(Arrow)）
  - 键盘 `L` / `A` 行为不变
- `finish_create_shape` 线类分支改为 Polyline 分支，按工具箭头默认创建。
- 端点控制 widget：选中态显示两端点控制点（`transform_handles.rs` `is_line` 判断改为 `ShapeType::Polyline`），行为不变。

## 5. 样式面板：箭头开关（编辑选中项）

- 样式面板新增 **起点箭头 / 终点箭头** 两个开关（无 / 箭头），作用于**选中的线性对象**（per-item），支持已有线改箭头——满足"起点终点都可添加"。
- 新增 undo 命令 `SetArrowHeads { item_id, start: Option<ArrowHeadStyle>, end: Option<ArrowHeadStyle> }`，保存 old / new 快照，可 undo / redo。
- **范围说明**：现有描边 / 填充面板只设新建默认值、不编辑选中项；本次仅把箭头做成编辑选中项（用户需求），描边 / 填充保持现状。

## 6. 向后兼容

**不需要**（用户确认）。移除 `ShapeType::Arrow` 后，旧文件中含 `Arrow` 的 item 加载失败可接受（版本内开发阶段）。

## 7. 测试

- **core 单测**（co-located）：
  - `ArrowHeadStyle` serde 往返
  - Polyline `base_size()` = points AABB；`set_line_points` 同步
  - 构造器：无箭头 / 终点箭头 / 双向箭头
  - 命中：多段线距离（含 2 顶点退化）
- **fileio 单测**：`bee.rs` 原 Arrow 往返测试改为 Polyline + `end_arrow` 落库往返
- **手工验收清单**：
  - Line 工具画直线（两端无箭头），Arrow 工具画箭头（终点箭头）
  - 样式面板对选中线开关起点 / 终点箭头，undo / redo 正确
  - 端点控制点拖拽、移动、删除、保存重开

## 8. 涉及文件清单

| 文件 | 改动 |
|---|---|
| `crates/preferz-core/src/shape.rs` | 移除 `ShapeType::Arrow`，新增 `ArrowHeadStyle`；单测 |
| `crates/preferz-core/src/item.rs` | Shape 加 `start_arrow`/`end_arrow`；`Line|Arrow` 判断改 `Polyline`；`new_shape_line` 改构造器；命中改多段线距离；单测 |
| `crates/preferz-core/src/lib.rs` | 导出 `ArrowHeadStyle` |
| `crates/preferz-core/src/commands.rs` | 新增 `SetArrowHeads` 命令 |
| `crates/preferz/src/ui/stylers.rs` | `build_line_shapes` 改 polyline + 起终点箭头；去掉 `ShapeType::Arrow` 分支 |
| `crates/preferz/src/ui/widgets/transform_handles.rs` | `is_line` 判断改 `ShapeType::Polyline` |
| `crates/preferz/src/preferz_app.rs` | `Tool` 调整；工具栏 / 快捷键映射；创建预览；`finish_create_shape`；样式面板箭头开关 |
| `crates/preferz/src/i18n.rs` | 新增起点 / 终点箭头 T key |
| `crates/preferz-fileio/src/bee.rs` | Arrow 往返测试改 Polyline + end_arrow |

## 9. 与现有设计文档的关系

- 本文档取代 `shapes-and-frame-slides-design.md` §3.1（ShapeType / Shape variant）、§4（箭头头部基于 shape_type 判断）、§5（Tool 枚举 / 创建流程）中关于 Line/Arrow 的描述。
- 原设计文档 §1 非目标"多点线编辑 → points 字段预留"仍成立：模型支持 N 顶点，编辑 UI 留待多段线阶段。
