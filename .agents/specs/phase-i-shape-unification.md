# PReferZ 图形类型统一（Phase I）设计

- 日期：2026-09-01
- 状态：已拍板待实施（决策见 [../plan.md](../plan.md) §Phase I 决策点 I1–I4）
- 配套：本文在 [linear-object-design.md](linear-object-design.md) 已落地成果之上扩展。
  该文已完成 `Line/Arrow → Polyline` + 起终点 `ArrowHeadStyle` 统一；本文新增
  **曲线（straight/curved）、闭合多边形（closed Polyline）、Dot 箭头头、矩形圆角**，
  并把 `.prz` 持久化补成带默认字段。

## 1. 范围

### 纳入

- **曲线**：per-item `CurveType { Straight, Curved }`，Catmull-Rom 开/闭曲线采样
- **不规则多边形** = 闭合 Polyline（`closed: bool` 标记，不新增 `ShapeType`）；新增「多边形」工具（点击加点、双击/回车闭合）
- **ArrowHeadStyle 增 `Dot`**：终点/起点可设 无 / 箭头 / 圆点
- **矩形族 roundness**：`roundness: f32`（0..1 比例），仅矩形族生效
- **`.prz` 迁移**：新字段全部 `#[serde(default)]`，`USER_VERSION` 不变，旧文件缺字段自动取默认

### 排后（明确不纳入 Phase I）

- D4 elbow 折线（直角拐点连接器）
- D5 hachure 填充（仅纯色，数据模型先统一）

## 2. 数据模型（preferz-core）

### 2.1 `shape.rs`

```rust
/// 曲线模式。Straight = 折线相连；Curved = Catmull-Rom 插值（开/闭）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CurveType {
    #[default]
    Straight,
    Curved,
}

/// 端点箭头样式。None（在 ItemKind 里用 Option 表达）= 该端无箭头。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArrowHeadStyle {
    Arrow,   // 标准三角箭头（现有）
    Dot,     // 圆点（新增）
}
```

`ItemKind::Shape` 在现有字段基础上新增三项（`#[serde(default)]`）：

```rust
ItemKind::Shape {
    shape_type: ShapeType,
    base_size: (f32, f32),
    points: Vec<(f32, f32)>,
    curve_type: CurveType,   // 新增，默认 Straight；仅 Polyline 生效，矩形族忽略
    closed: bool,            // 新增，默认 false；Polyline 闭合（多边形）时 true
    roundness: f32,          // 新增，默认 0.0；仅矩形族，0..1 比例（radius = min(w,h)*roundness）
    stroke: StrokeStyle,
    fill: Option<[u8; 4]>,
    start_arrow: Option<ArrowHeadStyle>,
    end_arrow: Option<ArrowHeadStyle>,
    seed: u64,
}
```

- `ArrowHeadStyle` 移除 `#[default]`（历史 `Arrow` 是唯一种，现在用 `Option` 表达无箭头，语义更清晰；旧 `.prz` 无此字段，靠 `#[serde(default)]` 给 `None`）。
- 矩形族 roundness：渲染 `radius = min(w, h) * roundness`；`0.0` = 直角（与 Excalidraw 比例语义一致）。

### 2.2 几何 / 命中（`item.rs`）

- **Polyline 采样**：
  - `Straight`：`points` 直线相连（N-1 段）。
  - `Curved`：经 Catmull-Rom 插值。`!closed` 开曲线用 phantom 控制点（首/末点各复制一次）；`closed` 用环绕索引（已有 `closed_catmull_rom` 思路，本项补**开曲线**版 `open_catmull_rom`）。
- **闭合多边形命中**：点在多边形内部（ray casting）**或** 到任一边距离 < 阈值（`max(stroke_width,6px)/zoom`）→ 选中；`!closed` 仍按线段距离。
- **`base_size()`**：矩形族 = `base_size`；Polyline = `points` AABB（不受 `curve_type` 影响，AABB 由控制点决定）。
- **`is_text_container`**：矩形/椭圆/菱形恒可；Polyline 仅 `closed` 时可（容器需封闭形状）。

### 2.3 命令（`commands.rs`）

- 复用 Phase B 的 `SetArrowHeads { item_id, start, end }`（Arrow/Dot 同走，存储 `Option<ArrowHeadStyle>`）。
- 新增 `SetCurveType { item_id, old: CurveType, new: CurveType }`。
- 新增 `SetClosed { item_id, old: bool, new: bool }`。
- 新增 `SetRoundness { item_id, old: f32, new: f32 }`。
- **多选批量**（D3 拍板「显示交集可批量改」）：上述命令对每个选中项逐个 push（或单命令持 `Vec<(id, old, new)>`）；面板显示交集值。

## 3. 渲染（binary，`stylers.rs`）

- **Polyline 路径**：
  - `Straight` + `!closed` → `PathShape::line`
  - `Straight` + `closed` → `PathShape::closed`（带 `fill`）
  - `Curved` → 采样点经 Catmull-Rom；`closed` 则闭合路径 + `fill`
- **箭头头**：
  - `Arrow` = 现有三角（`points[N-1]` 沿末段方向 ±~25° 两短线段）
  - `Dot` = 末端小实心圆（`head_len` 取 stroke 宽度 * 2~3 作半径，圆心沿末段方向退后半径距离）
  - 起点头方向取首段反向。
- **矩形圆角**：`egui::Shape::rect_stroke` / `rect_filled` 的 `rounding = radius`（CleanStyler）。
- **手绘风（RoughStyler）**：曲线用 `jitter_points` + Catmull-Rom（已有 ellipse 思路复用）；roundness 近似（暂不完美，留 TODO 注释）；Dot 头在 Rough 下用抖动点圆。

## 4. 工具与交互（`preferz_app.rs`）

### 4.1 多边形工具（I1）

- 新增 `Tool::Polygon`；`DragState` 新增 `CreatingPolygon { points: Vec<CanvasPoint>, current: CanvasPoint }`。
- 交互：pointer down 记一个顶点 → move 更新 `current` 画临时预览（已点顶点 + 当前指针连线，按 `curve_type` 默认 `Straight`）→ 再次 down 加点；
  - **双击** 或 **Enter** 闭合并 `finish`（≥3 点才可闭合；<3 点视为取消）；
  - **Esc** 取消。
  - Shift 锁 0/45/90° 角度（与线类一致）。
- `finish_create_polygon`：归一化顶点到局部坐标（相对 `transform.pos`），`AddItem(Polyline { closed: true, … })`。

### 4.2 样式面板（I2/I3）

- 选中**线性对象**时显示：
  - 「直线 / 曲线」分段控件 → `SetCurveType`（走 undo）；
  - 起点 / 终点箭头：**无 / 箭头 / 圆点** 三态 → `SetArrowHeads`；
  - 「闭合」勾选（仅 Polyline）→ `SetClosed`。
- 选中**矩形族**时显示「圆角」滑块（0..1）→ `SetRoundness`。
- 多选：显示交集值，修改批量应用（D3）。

### 4.3 工具栏 / 键位

- 新增「多边形」工具按钮；快捷键暂定 **`P`**（与现有 V/R/O/D/A/L/M/F 不冲突），最终在 Phase K 对齐 Excalidraw 时复核。
- 其余键位（L 直线 / A 箭头 / R 矩形 …）不变。

## 5. 测试

- **core 单测**（co-located）：
  - `CurveType` / `ArrowHeadStyle` serde 往返（含 `Dot`、`Curved`）
  - `Shape` 新字段 `#[serde(default)]`：构造器默认值；旧 JSON（缺字段）加载不报错
  - 闭合多边形 AABB + 命中（内部点 / 边上点 / 外部点）
  - `open_catmull_rom` 采样点数与端点对齐（首末点不被拉偏）
  - 命令 `old/new` 快照正确、undo/redo 还原
- **fileio 单测**：`.prz` 旧文件（无 `curve_type`/`closed`/`roundness`）加载默认；新文件落库往返
- **手工验收清单**：
  - 多边形工具：点击加点 → 双击闭合 → 选中 → 填充 → 保存重开
  - 选中直线 → 样式面板切「曲线」→ 预览变弯 + undo 还原
  - 箭头改「圆点」两端；矩形圆角滑块实时变化
  - 明暗主题下样式面板控件正常（复用 Phase G 主题）

## 6. 实施分期（建议提交粒度）

| 子阶段 | 内容 | 交付物 |
|---|---|---|
| A | core 数据模型：`CurveType`/`Dot`/`closed`/`roundness` + serde default + 几何/命中 + 命令 + 单测 | `cargo test` 绿，模型就绪 |
| B | 渲染：`stylers.rs` 曲线采样 + closed fill + Dot head + 圆角 | 现有图形渲染无回归 |
| C | 工具/UI：多边形工具 + 样式面板控件 + 快捷键 | 可画多边形/切曲线/改箭头点/调圆角 |
| D | i18n + fileio 旧文件加载校验 + 手工验收 | 全链路可交付 |

每子阶段独立可合入；A 先行（B/C/D 依赖其字段）。

## 7. 涉及文件清单

| 文件 | 改动 |
|---|---|
| `crates/preferz-core/src/shape.rs` | `ArrowHeadStyle` 加 `Dot`；新增 `CurveType`；单测 |
| `crates/preferz-core/src/item.rs` | `Shape` 新字段 + 构造/命中/容器判定；单测 |
| `crates/preferz-core/src/commands.rs` | `SetCurveType` / `SetClosed` / `SetRoundness` |
| `crates/preferz/src/ui/stylers.rs` | 曲线采样 + closed fill + Dot head + 圆角 |
| `crates/preferz/src/preferz_app.rs` | 多边形工具 + `DragState` + 预览 + 样式面板控件 + 快捷键 |
| `crates/preferz/src/i18n.rs` | 新增相关 T key |
| `crates/preferz-fileio/src/{bee,prz}.rs` | 旧文件默认加载校验 |
