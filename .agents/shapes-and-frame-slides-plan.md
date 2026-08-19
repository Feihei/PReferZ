# PReferZ 图形绘制 + Frame Slides 实施计划

> **For agentic workers:** 建议用 subagent-driven-development 或 executing-plans 按任务逐项执行。步骤用复选框（`- [ ]`）跟踪。
> 配套设计文档：`.agents/shapes-and-frame-slides-design.md`（计划发现快捷键冲突，已同步修订设计文档附录 A）。

**Goal:** 在现有参考图板架构上实现无限画布图形绘制（矩形/椭圆/菱形/直线/箭头）与基于 Frame 的全屏 slide 演示。

**Architecture:** `ItemKind` 增加 `Shape`/`Frame` variant（serde JSON 自动兼容，零 schema 迁移）；binary 层新增 `ShapeStyler` trait（`CleanStyler` 简洁实现，`RoughStyler` 手绘留待 Phase F）；绘制走现有"预览模式 + `AddItem` 命令"的 undo 语义；Slide 模式为 `AppMode` 状态机 + fit-to-screen 视口动画。核心几何/命中逻辑放 `preferz-core`，UI/渲染/风格器放 `preferz` binary，`preferz-core` 不依赖 egui。

**Tech Stack:** Rust 2021、egui/eframe 0.29、euclid、serde、rusqlite。**Phase A–E 零新依赖**（roughr 仅 Phase F 验证后引入）。

**执行顺序：A → B → C → D → E，每期独立合入 main。**

---

# Phase A：Shape 基础集（矩形/椭圆/菱形）

**交付物**：能在画布上绘制矩形/椭圆/菱形，可选中/移动/旋转/翻转/缩放/删除/undo/保存重开；带工具切换快捷键与样式面板。

**涉及文件**
- 新增：`crates/preferz-core/src/shape.rs`（ShapeType / DashStyle / StrokeStyle）
- 新增：`crates/preferz/src/ui/stylers.rs`（ShapeStyler trait + CleanStyler + ShapeData）
- 修改：`crates/preferz-core/src/item.rs`（ItemKind::Shape、Item::new_shape、base_size、命中、lib 导出）
- 修改：`crates/preferz-core/src/lib.rs`（导出新类型）
- 修改：`crates/preferz-core/src/spaces.rs`（无改动，确认 ScreenSpace 已导出）
- 修改：`crates/preferz/src/viewport.rs`（canvas_to_screen_transform()）
- 修改：`crates/preferz/src/preferz_app.rs`（Tool、DragState::CreatingShape、render_scene Shape 分支、begin/update/end_drag、handle_shortcuts、样式面板、工具栏、show_flip/show_rotate helper）
- 修改：`crates/preferz/src/ui/widgets/transform_handles.rs`（helper 函数放这里）
- 修改：`crates/preferz/src/i18n.rs`（新 T key）
- 修改：`crates/preferz-fileio/src/bee.rs`（item_kind_str 加 Shape）

### Task A1：core 数据模型

**Files:**
- Create: `crates/preferz-core/src/shape.rs`
- Modify: `crates/preferz-core/src/item.rs`
- Modify: `crates/preferz-core/src/lib.rs`

- [ ] **Step 1：新增 shape.rs，写核心类型 + 单测**

```rust
// crates/preferz-core/src/shape.rs
use serde::{Deserialize, Serialize};

/// 图形类型。Line/Arrow 为 Phase B 启用，本文件先定义枚举保证序列化稳定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapeType {
    Rectangle,
    Ellipse,
    Diamond,
    Line,
    Arrow,
}

/// 描边线型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DashStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

/// 描边样式（画布空间像素；颜色 RGBA）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StrokeStyle {
    pub color: [u8; 4],
    pub width: f32,
    pub dash: DashStyle,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            color: [255, 255, 255, 255],
            width: 2.0,
            dash: DashStyle::Solid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_style_serde_roundtrip() {
        let s = StrokeStyle {
            color: [10, 20, 30, 255],
            width: 3.5,
            dash: DashStyle::Dashed,
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: StrokeStyle = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn shape_type_serde_roundtrip() {
        for st in [ShapeType::Rectangle, ShapeType::Ellipse, ShapeType::Diamond] {
            let json = serde_json::to_string(&st).unwrap();
            let back: ShapeType = serde_json::from_str(&json).unwrap();
            assert_eq!(st, back);
        }
    }
}
```

- [ ] **Step 2：item.rs 增加 ItemKind::Shape + new_shape + base_size 分支**

在 `crates/preferz-core/src/item.rs` 顶部 import 增加：

```rust
use crate::shape::{ShapeType, StrokeStyle};
```

`ItemKind` 枚举新增 variant（放在 Text 之后）：

```rust
    Shape {
        shape_type: ShapeType,
        /// 局部空间尺寸（矩形族 = w×h；线类 = points 包围盒）。
        base_size: (f32, f32),
        /// 线类专用，局部坐标（A 期为空 Vec；B 期启用）。
        points: Vec<(f32, f32)>,
        stroke: StrokeStyle,
        fill: Option<[u8; 4]>, // RGBA；None = 透明
        /// 手绘风确定性噪声预留（Phase F 用），A 期固定 0。
        seed: u64,
    },
```

`impl Item` 新增构造器（放在 new_text 之后）：

```rust
    pub fn new_shape(
        shape_type: ShapeType,
        base_size: (f32, f32),
        pos_x: f32,
        pos_y: f32,
        stroke: StrokeStyle,
        fill: Option<[u8; 4]>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: ItemKind::Shape {
                shape_type,
                base_size,
                points: Vec::new(),
                stroke,
                fill,
                seed: 0,
            },
            transform: Transform::new(pos_x, pos_y, 1.0, 1.0),
            z: 0,
        }
    }
```

`base_size()` 的 match 增加分支（在 Text 分支之后、match 闭合前）：

```rust
            ItemKind::Shape { base_size, .. } => {
                CanvasVector::new(base_size.0.max(1.0), base_size.1.max(1.0))
            }
```

> 注意：当前 match 是穷尽匹配，加新 variant 后 compiler 会提示缺失分支；本 step 全部补完编译通过。

- [ ] **Step 3：lib.rs 导出**

```rust
pub mod shape;
...
pub use shape::{DashStyle, ShapeType, StrokeStyle};
```

- [ ] **Step 4：验证**

Run: `cargo test -p preferz-core`
Expected: `shape` 相关单测 PASS；`cargo fmt --all --check` 通过（先 `cargo fmt`）。

- [ ] **Step 5：Commit**

```bash
git add crates/preferz-core/src/shape.rs crates/preferz-core/src/item.rs crates/preferz-core/src/lib.rs
git commit -m "feat(core): add Shape item kind with ShapeType/StrokeStyle"
```

### Task A2：viewport 屏幕变换 + ItemLocalToScreen

**Files:**
- Modify: `crates/preferz/src/viewport.rs`
- Modify: `crates/preferz-core/src/item.rs`

- [ ] **Step 1：item.rs 定义 ItemLocalToScreen 别名**

在 `ItemLocalToCanvas` 类型别名附近加：

```rust
/// Item 局部 → 屏幕 变换矩阵（画布 → 屏幕 再叠加 ItemLocal → Canvas）。
pub type ItemLocalToScreen =
    euclid::Transform2D<f32, ItemLocalSpace, crate::spaces::ScreenSpace>;
```

- [ ] **Step 2：viewport.rs 增加 canvas_to_screen_transform()**

在 `canvas_to_screen_rect` 方法之后加：

```rust
    /// 构造 Canvas → Screen 的仿射矩阵（与 [`canvas_to_screen`] 严格一致）。
    pub fn canvas_to_screen_transform(&self) -> CanvasToScreen {
        let center = self.screen_rect.center();
        CanvasToScreen::identity()
            .then_scale(self.zoom, self.zoom)
            .then_translate(euclid::Vector2D::new(
                center.x - self.pan.x * self.zoom,
                center.y - self.pan.y * self.zoom,
            ))
    }
```

> 数学验证：`screen = canvas * zoom + (center - pan * zoom)` 与 `canvas_to_screen()` 的
> `center + (canvas - pan) * zoom` 恒等。

- [ ] **Step 3：验证**

Run: `cargo check --workspace`
Expected: 编译通过。`viewport.rs` 需 `use preferz_core::spaces::CanvasToScreen;`。

- [ ] **Step 4：Commit**

```bash
git add crates/preferz/src/viewport.rs crates/preferz-core/src/item.rs
git commit -m "feat(core,viewport): add ItemLocalToScreen alias and canvas_to_screen_transform"
```

### Task A3：ShapeStyler trait + CleanStyler

**Files:**
- Create: `crates/preferz/src/ui/stylers.rs`
- Modify: `crates/preferz/src/ui/mod.rs`

- [ ] **Step 1：写 stylers.rs**

```rust
// crates/preferz/src/ui/stylers.rs
use eframe::egui::{self, epaint::PathStroke, Color32, Pos2, Shape};
use preferz_core::shape::{DashStyle, ShapeType, StrokeStyle};
use preferz_core::spaces::ScreenSpace;

/// 风格器输入：shape 的局部空间几何。
pub struct ShapeData {
    pub shape_type: ShapeType,
    pub base_size: (f32, f32),
    pub points: Vec<(f32, f32)>,
}

/// 将 shape 局部几何转换为屏幕空间的 egui::Shape 列表。
/// 风格器自行把局部点经 to_screen 变换到屏幕；stroke 线宽按 zoom 缩放。
pub trait ShapeStyler {
    fn build_shapes(
        &self,
        shape: &ShapeData,
        stroke: &StrokeStyle,
        fill: Option<Color32>,
        to_screen: &euclid::Transform2D<f32, preferz_core::item::ItemLocalSpace, ScreenSpace>,
        zoom: f32,
    ) -> Vec<Shape>;
}

/// Phase 1 简洁实现：实线/虚线/圆点，epaint 原生。
pub struct CleanStyler;

impl CleanStyler {
    fn color(c: [u8; 4]) -> Color32 {
        Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
    }
}

impl ShapeStyler for CleanStyler {
    fn build_shapes(
        &self,
        shape: &ShapeData,
        stroke: &StrokeStyle,
        fill: Option<Color32>,
        to_screen: &euclid::Transform2D<f32, preferz_core::item::ItemLocalSpace, ScreenSpace>,
        zoom: f32,
    ) -> Vec<Shape> {
        let (w, h) = shape.base_size;
        // 局部空间顶点 → 屏幕
        let mut pts: Vec<Pos2> = match shape.shape_type {
            ShapeType::Rectangle => {
                let corners = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
                corners
                    .iter()
                    .map(|(x, y)| to_screen.transform_point(euclid::Point2D::new(*x, *y)))
                    .collect()
            }
            ShapeType::Diamond => {
                let corners = [(w / 2.0, 0.0), (w, h / 2.0), (w / 2.0, h), (0.0, h / 2.0)];
                corners
                    .iter()
                    .map(|(x, y)| to_screen.transform_point(euclid::Point2D::new(*x, *y)))
                    .collect()
            }
            ShapeType::Ellipse => {
                let cx = w / 2.0;
                let cy = h / 2.0;
                let rx = w / 2.0;
                let ry = h / 2.0;
                let seg = 64usize;
                (0..seg)
                    .map(|i| {
                        let a = i as f32 / seg as f32 * std::f32::consts::TAU;
                        to_screen.transform_point(euclid::Point2D::new(
                            cx + rx * a.cos(),
                            cy + ry * a.sin(),
                        ))
                    })
                    .collect()
            }
            // Phase B 启用
            ShapeType::Line | ShapeType::Arrow => return Vec::new(),
        };

        let stroke_color = Self::color(stroke.color);
        let line_width = stroke.width * zoom;
        let stroke = PathStroke::new(line_width, stroke_color);

        let fill = fill.unwrap_or(Color32::TRANSPARENT);
        match stroke.dash {
            DashStyle::Solid => {
                vec![Shape::convex_polygon(pts, fill, stroke)]
            }
            DashStyle::Dashed | DashStyle::Dotted => {
                let mut out = vec![Shape::convex_polygon(pts.clone(), fill, PathStroke::NONE)];
                // 闭合路径：首点追加到末尾
                pts.push(pts[0]);
                let (dash_len, gap_len) = match stroke.dash {
                    DashStyle::Dotted => (1.5 * zoom, line_width * 2.0),
                    _ => (line_width * 4.0, line_width * 2.0),
                };
                out.push(Shape::dashed_line(pts, stroke, dash_len, gap_len));
                out
            }
        }
    }
}

/// 便捷入口：Item 局部 → 屏幕 的变换（供 render_scene 使用）。
pub fn item_local_to_screen(
    item: &preferz_core::Item,
    viewport: &crate::viewport::ViewportState,
) -> euclid::Transform2D<f32, preferz_core::item::ItemLocalSpace, ScreenSpace> {
    viewport
        .canvas_to_screen_transform()
        .then(&item.local_to_canvas())
}
```

- [ ] **Step 2：ui/mod.rs 导出**

```rust
pub mod stylers;
pub mod widgets;
```

- [ ] **Step 3：验证**

Run: `cargo check --workspace`
Expected: 编译通过（若 `CanvasVector` import 未用到，删除该 import 与 `_keep_canvas_vector` 占位，保持 clippy 零警告）。

> 注意：`cargo clippy --workspace --all-targets -- -D warnings` 必须零警告，不要用 `#[allow]` 掩盖真实问题；上述 `#[allow(dead_code)]` 占位在无用时直接删除整个 helper。

- [ ] **Step 4：Commit**

```bash
git add crates/preferz/src/ui/stylers.rs crates/preferz/src/ui/mod.rs
git commit -m "feat(ui): add ShapeStyler trait with CleanStyler (rect/ellipse/diamond)"
```

### Task A4：Tool 枚举 + 应用状态字段 + show_flip/show_rotate helper

**Files:**
- Modify: `crates/preferz/src/preferz_app.rs`
- Modify: `crates/preferz/src/ui/widgets/transform_handles.rs`

- [ ] **Step 1：preferz_app.rs 定义 Tool 枚举与字段**

在 `enum DragState` 之前加：

```rust
/// 当前激活工具。Select = 现有选择/框选行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Shape(ShapeType),
    /// Phase D 启用
    Frame,
}
```

`PReferZApp` struct 加字段（`transform_handles` 之后）：

```rust
    /// 当前激活工具。
    tool: Tool,
    /// 绘制形状默认描边样式（样式面板可调）。
    default_stroke: StrokeStyle,
    /// 绘制形状默认填充色（None = 透明）。
    default_fill: Option<[u8; 4]>,
```

`PReferZApp::new()` 初始化：

```rust
            tool: Tool::Select,
            default_stroke: StrokeStyle::default(),
            default_fill: None,
```

文件头 import 增加：

```rust
use preferz_core::shape::{DashStyle, ShapeType, StrokeStyle};
```

- [ ] **Step 2：transform_handles.rs 增加 helper 并替换内联 matches**

新增（放 `Handle` 枚举之前）：

```rust
/// 是否显示翻转手柄（Pixmap 与 Shape 支持，Text/Frame 不显示）。
pub fn should_show_flip(item: &Item) -> bool {
    matches!(item.kind, ItemKind::Pixmap { .. } | ItemKind::Shape { .. })
}

/// 是否显示旋转手柄（同上）。
pub fn should_show_rotate(item: &Item) -> bool {
    matches!(item.kind, ItemKind::Pixmap { .. } | ItemKind::Shape { .. })
}
```

把 `update_hover` 内两处：

```rust
            let show_flip = matches!(item.kind, ItemKind::Pixmap { .. });
            // 文字元素不需要旋转，去掉旋转手柄
            let show_rotate = matches!(item.kind, ItemKind::Pixmap { .. });
```

替换为：

```rust
            let show_flip = should_show_flip(item);
            let show_rotate = should_show_rotate(item);
```

- [ ] **Step 3：preferz_app.rs 替换全部 show_flip/show_rotate 内联 matches**

4 处位置（用 Grep 定位 `matches!(item.kind, ItemKind::Pixmap { .. })`）：
- `update()` 光标逻辑（约 L716 区域）
- `begin_drag()` 手柄检测（约 L897-898）
- `render_scene()` 手柄渲染（约 L1281-1282）

统一替换为：

```rust
                let show_flip = should_show_flip(item);
                let show_rotate = should_show_rotate(item);
```

并在文件头 import：`use crate::ui::widgets::transform_handles::should_show_flip;` 等。

> 注意：Frame（D 期）不显示旋转/翻转；当前 `should_show_*` 对 Frame 返回 false 已满足。

- [ ] **Step 4：验证**

Run: `cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: 编译通过、零警告。

- [ ] **Step 5：Commit**

```bash
git add crates/preferz/src/preferz_app.rs crates/preferz/src/ui/widgets/transform_handles.rs
git commit -m "feat(ui): add Tool enum and extend flip/rotate handles to Shape items"
```

### Task A5：DragState::CreatingShape + 绘制流程

**Files:**
- Modify: `crates/preferz/src/preferz_app.rs`

- [ ] **Step 1：DragState 增加变体**

```rust
    /// 用绘制工具拖拽创建 shape（两点式：start → current）。
    CreatingShape {
        shape_type: ShapeType,
        start: CanvasPoint,
        current: CanvasPoint,
    },
```

- [ ] **Step 2：begin_drag 开头拦截绘制工具**

在 `begin_drag` 函数体最前面（`editing_text` 检查之后）加：

```rust
        // 绘制工具激活：直接进入创建拖拽（不处理手柄/命中/框选）
        if let Tool::Shape(shape_type) = self.tool {
            let start_canvas = self.viewport.screen_to_canvas(screen_pos);
            self.drag = DragState::CreatingShape {
                shape_type,
                start: start_canvas,
                current: start_canvas,
            };
            return;
        }
```

> 说明：现有 `begin_drag` 前置分支（crop_mode / color_picker）在绘制工具下不会同时激活，工具切换会清空这些模式（见 Task A7），此处拦截顺序安全。

- [ ] **Step 3：update_drag_preview 更新 current**

在 `match &self.drag` 的 `DragState::BoxSelect { .. } => {}` 分支前加：

```rust
            DragState::CreatingShape { current, .. } => {
                let _ = current;
            }
```

在函数末尾的 BoxSelect 更新块旁边加（复用同一模式）：

```rust
        if let DragState::CreatingShape { current, .. } = &mut self.drag {
            *current = self.viewport.screen_to_canvas(screen_pos);
        }
```

- [ ] **Step 4：end_drag 完成创建**

在 `end_drag` 的 `match prev` 中 `DragState::BoxSelect` 分支之后加：

```rust
            DragState::CreatingShape {
                shape_type,
                start,
                current,
            } => {
                self.finish_create_shape(shape_type, start, current);
            }
```

新增私有方法（`impl PReferZApp` 块内，`end_drag` 之后）：

```rust
    fn finish_create_shape(&mut self, shape_type: ShapeType, start: CanvasPoint, current: CanvasPoint) {
        let w = (current.x - start.x).abs();
        let h = (current.y - start.y).abs();
        // 误触：小于 3 画布像素丢弃
        if w < 3.0 || h < 3.0 {
            return;
        }
        let min_x = start.x.min(current.x);
        let min_y = start.y.min(current.y);
        // Shift 锁正方形
        let shift = self
            .current_modifiers_shift();
        let (bw, bh) = if shift {
            let side = w.max(h);
            (side, side)
        } else {
            (w, h)
        };
        let stroke = self.default_stroke;
        let fill = self.default_fill;
        let item = Item::new_shape(
            shape_type,
            (bw, bh),
            min_x,
            min_y,
            stroke,
            fill,
        );
        let cmd = AddItem::new(item);
        self.push_cmd(Box::new(cmd));
        self.flash("已创建图形");
        // 默认回 Select（Shift 保持工具在 begin_drag 中处理）
        self.tool = Tool::Select;
    }

    fn current_modifiers_shift(&self) -> bool {
        // egui 无全局修饰符读取，改由调用方传参更干净——见 Step 5 调整
        false
    }
```

> 注意：egui 修饰符在 `ctx.input()` 内读取，`finish_create_shape` 没有 ctx 参数。按 Step 5 调整：给 `finish_create_shape` 增加 `shift: bool` 参数，由 `end_drag` 调用处从 ctx 读取传入。

- [ ] **Step 5：修正修饰符读取（改写 Step 4 实现）**

`end_drag` 分支改为：

```rust
            DragState::CreatingShape {
                shape_type,
                start,
                current,
            } => {
                // egui 修饰符需在 ctx.input 内读取，故在调用前捕获
                let shift = self.capture_shift_for_commit();
                self.finish_create_shape(shape_type, start, current, shift);
            }
```

`finish_create_shape` 签名改为带 `shift: bool`，去掉 `current_modifiers_shift`。

由于 `end_drag()` 本身没有 ctx 参数，最干净的方案是**在 begin_drag 时就捕获 shift**：在 `DragState::CreatingShape` 变体中增加 `shift: bool` 字段，`begin_drag` 里从已有 `additive`（=shift）参数传入。`begin_drag` 签名已有 `additive: bool`（即 Shift 按下），直接复用：

```rust
    CreatingShape {
        shape_type: ShapeType,
        start: CanvasPoint,
        current: CanvasPoint,
        shift: bool,
    },
```

`begin_drag` 中：

```rust
        if let Tool::Shape(shape_type) = self.tool {
            let start_canvas = self.viewport.screen_to_canvas(screen_pos);
            self.drag = DragState::CreatingShape {
                shape_type,
                start: start_canvas,
                current: start_canvas,
                shift: additive,
            };
            return;
        }
```

`end_drag` 匹配带 `shift`：

```rust
            DragState::CreatingShape {
                shape_type,
                start,
                current,
                shift,
            } => {
                self.finish_create_shape(shape_type, start, current, shift);
            }
```

`finish_create_shape`（去掉 Step 4 的占位 helper）：

```rust
    fn finish_create_shape(
        &mut self,
        shape_type: ShapeType,
        start: CanvasPoint,
        current: CanvasPoint,
        shift: bool,
    ) {
        let w = (current.x - start.x).abs();
        let h = (current.y - start.y).abs();
        if w < 3.0 || h < 3.0 {
            return;
        }
        let min_x = start.x.min(current.x);
        let min_y = start.y.min(current.y);
        let (bw, bh) = if shift {
            let side = w.max(h);
            (side, side)
        } else {
            (w, h)
        };
        let item = Item::new_shape(
            shape_type,
            (bw, bh),
            min_x,
            min_y,
            self.default_stroke,
            self.default_fill,
        );
        let cmd = AddItem::new(item);
        self.push_cmd(Box::new(cmd));
        self.flash("已创建图形");
        self.tool = Tool::Select;
    }
```

- [ ] **Step 6：渲染创建预览**

在 `update()` 的 CentralPanel 内、`render_scene` 之后加（与 BoxSelect 预览并列）：

```rust
            // 绘制工具拖拽预览
            if let DragState::CreatingShape {
                shape_type,
                start,
                current,
                ..
            } = &self.drag
            {
                let min_x = start.x.min(current.x);
                let min_y = start.y.min(current.y);
                let w = (current.x - start.x).abs();
                let h = (current.y - start.y).abs();
                let rect = CanvasRect::new(
                    CanvasPoint::new(min_x, min_y),
                    CanvasSize::new(w, h),
                );
                let screen_rect = self.viewport.canvas_to_screen_rect(rect);
                let stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(100, 200, 255));
                match shape_type {
                    ShapeType::Rectangle | ShapeType::Diamond => {
                        ui.painter().rect_stroke(screen_rect, 0.0, stroke);
                    }
                    ShapeType::Ellipse => {
                        ui.painter().circle_stroke(screen_rect.center(), screen_rect.width() / 2.0, stroke);
                    }
                    ShapeType::Line | ShapeType::Arrow => {}
                }
            }
```

> 说明：椭圆预览用宽度一半近似圆（不精确但作为创建反馈足够）；正式形状渲染在 render_scene 走 styler（Task A6）。Diamond 预览用 rect 近似，可接受。

- [ ] **Step 7：验证 + 手工验收**

Run: `cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Run: `cargo run -p preferz`（手工：按 R 拖拽画矩形、Esc 取消、undo/redo）

- [ ] **Step 8：Commit**

```bash
git add crates/preferz/src/preferz_app.rs
git commit -m "feat(ui): add CreatingShape drag state with preview and AddItem commit"
```

### Task A6：render_scene Shape 分支

**Files:**
- Modify: `crates/preferz/src/preferz_app.rs`

- [ ] **Step 1：render_scene 的 match 增加 Shape 分支**

在 `ItemKind::Text` 分支之后、match 闭合前加：

```rust
                ItemKind::Shape {
                    shape_type,
                    base_size,
                    points,
                    stroke,
                    fill,
                    seed: _,
                } => {
                    let data = crate::ui::stylers::ShapeData {
                        shape_type: *shape_type,
                        base_size: *base_size,
                        points: points.clone(),
                    };
                    let to_screen = crate::ui::stylers::item_local_to_screen(item, &self.viewport);
                    let fill_color = fill.map(|c| {
                        egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
                    });
                    let shapes = crate::ui::stylers::CleanStyler.build_shapes(
                        &data,
                        stroke,
                        fill_color,
                        &to_screen,
                        self.viewport.zoom,
                    );
                    ui.painter().extend(shapes);
                }
```

> 注意：`item` 在此处是 `&&Item`（来自 `&items` 迭代），解引用时用 `*item` 或直接 `item`（Deref 自动）。如 borrow 冲突，先 clone 必要字段（与 Text 分支同模式）。

- [ ] **Step 2：验证 + 手工验收**

Run: `cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Run: `cargo run -p preferz`（画矩形/椭圆/菱形，验证旋转后形状正确渲染、命中、缩放）

- [ ] **Step 3：Commit**

```bash
git add crates/preferz/src/preferz_app.rs
git commit -m "feat(ui): render Shape items via CleanStyler"
```

### Task A7：工具快捷键 + 快捷键冲突处理

**Files:**
- Modify: `crates/preferz/src/preferz_app.rs`

> **背景**：现有单键 `R`=线形排列、`G`=网格、`O`=最优排列（仅多选时）。本任务把这些低频排列快捷键**移除单键绑定**（右键菜单 Arrange 子菜单仍保留），把 `R/O` 释放给绘制工具。`C`=裁剪、`I`=取色、`F`=fit 保留，但仅 Select 工具下生效。

- [ ] **Step 1：新增工具切换处理**

在 `handle_shortcuts` 开头（`editing_text` 检查之后、crop_mode 检查之前）加：

```rust
        // 工具切换快捷键（始终生效，即使当前工具非 Select）
        let tool_switch = self.tool_switch_shortcut(ctx);
        if let Some(new_tool) = tool_switch {
            self.drag = DragState::Idle; // 取消进行中的绘制
            if self.tool != new_tool {
                self.tool = new_tool;
            } else {
                self.tool = Tool::Select; // 再次按同键回 Select
            }
            return;
        }
        // 绘制工具激活：屏蔽其它场景快捷键，Esc 回 Select
        if self.tool != Tool::Select {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.tool = Tool::Select;
            }
            return;
        }
```

新增方法：

```rust
    fn tool_switch_shortcut(&self, ctx: &egui::Context) -> Option<Tool> {
        let no_mod = ctx.input(|i| !i.modifiers.ctrl && !i.modifiers.shift && !i.modifiers.alt);
        if !no_mod {
            return None;
        }
        let pressed = |key| ctx.input(|i| i.key_pressed(key));
        if pressed(egui::Key::V) {
            Some(Tool::Select)
        } else if pressed(egui::Key::R) {
            Some(Tool::Shape(ShapeType::Rectangle))
        } else if pressed(egui::Key::O) {
            Some(Tool::Shape(ShapeType::Ellipse))
        } else if pressed(egui::Key::D) {
            Some(Tool::Shape(ShapeType::Diamond))
        } else {
            None
        }
    }
```

> `V/R/O/D` 本期实现；`A/L`（线类）与 `M`（Frame）在 B/D 期加入此函数。

- [ ] **Step 2：移除 R/G/O 单键排列**

删除 `handle_shortcuts` 中：

```rust
            // R = 线形排列
            if ctx.input(|i| i.key_pressed(egui::Key::R)) && self.scene.selection.len() >= 2 {
                self.arrange_selected(ArrangeMode::Linear);
            }
            // G = 网格排列
            if ctx.input(|i| i.key_pressed(egui::Key::G)) && self.scene.selection.len() >= 2 {
                self.arrange_selected(ArrangeMode::Grid);
            }
            // O = 最优装箱
            if ctx.input(|i| i.key_pressed(egui::Key::O)) && self.scene.selection.len() >= 2 {
                self.arrange_selected(ArrangeMode::Optimal);
            }
```

保留 `C` 裁剪分支（仅 `selected_pixmap_count() == 1`，且现在外层 `tool != Select` 已由 Step 1 屏蔽，安全）。

> 排列功能保留在右键菜单 `Arrange` 子菜单（`render_context_menu` 无需改动）。

- [ ] **Step 3：验证**

Run: `cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Run: `cargo run -p preferz`（V/R/O/D 切换、Esc 回 Select、多选后右键 Arrange 仍可用）

- [ ] **Step 4：Commit**

```bash
git add crates/preferz/src/preferz_app.rs
git commit -m "feat(ui): add tool shortcut keys, move arrange to context menu only"
```

### Task A8：样式面板 + 工具栏

**Files:**
- Modify: `crates/preferz/src/preferz_app.rs`
- Modify: `crates/preferz/src/i18n.rs`

- [ ] **Step 1：i18n 新增 T key**

在 `i18n.rs` 的 `T` 枚举（`// ── 右键菜单 ──` 区块）追加：

```rust
    // ── 绘制工具/样式面板 ──
    ToolSelect,
    ToolRectangle,
    ToolEllipse,
    ToolDiamond,
    ToolLine,
    ToolArrow,
    ToolFrame,
    StyleStrokeColor,
    StyleStrokeWidth,
    StyleDashSolid,
    StyleDashDashed,
    StyleDashDotted,
    StyleFillNone,
```

在 `translate` 函数中为 `Lang::En` / `Lang::Zh` 补对应文案（示例，中文分支）：

```rust
    T::ToolSelect => ("Select", "选择"),
    T::ToolRectangle => ("Rectangle", "矩形"),
    T::ToolEllipse => ("Ellipse", "椭圆"),
    T::ToolDiamond => ("Diamond", "菱形"),
    T::ToolLine => ("Line", "直线"),
    T::ToolArrow => ("Arrow", "箭头"),
    T::ToolFrame => ("Frame", "画框"),
    T::StyleStrokeColor => ("Stroke color", "描边颜色"),
    T::StyleStrokeWidth => ("Stroke width", "描边宽度"),
    T::StyleDashSolid => ("Solid", "实线"),
    T::StyleDashDashed => ("Dashed", "虚线"),
    T::StyleDashDotted => ("Dotted", "点线"),
    T::StyleFillNone => ("No fill", "无填充"),
```

> 先读 `i18n.rs` 的 `translate` 实际签名与 `TRANSLATIONS` 表结构，按现有模式追加（避免两分支返回值数量不匹配导致编译错）。

- [ ] **Step 2：工具栏（左侧 SidePanel）**

在 `update()` 中 `CentralPanel` 之前插入（TopBottomPanel 之前或之后均可，需在 CentralPanel 前）：

```rust
        egui::SidePanel::left("tool_panel")
            .exact_width(44.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                let tools = [
                    (Tool::Select, "↖", T::ToolSelect),
                    (Tool::Shape(ShapeType::Rectangle), "▭", T::ToolRectangle),
                    (Tool::Shape(ShapeType::Ellipse), "◯", T::ToolEllipse),
                    (Tool::Shape(ShapeType::Diamond), "◇", T::ToolDiamond),
                ];
                for (tool, icon, key) in tools {
                    let is_active = self.tool == tool;
                    let btn = egui::Button::new(icon)
                        .min_size(egui::vec2(30.0, 30.0))
                        .fill(if is_active {
                            ui.visuals().selection.bg_fill
                        } else {
                            ui.visuals().widgets.inactive.bg_fill
                        });
                    if ui.add(btn).on_hover_text(t(self.lang, key)).clicked() {
                        self.tool = tool;
                        self.drag = DragState::Idle;
                    }
                    ui.add_space(4.0);
                }
            });
```

> 说明：`↖/▭/◯/◇` 是 Unicode 字符作图标，不引入图标库（设计 §5.1）。i18n tooltip 用 `t(self.lang, key)`。

- [ ] **Step 3：样式面板（底部，仅 Shape 工具激活时显示）**

在 `update()` 的 CentralPanel 之后、状态栏之前（`TopBottomPanel::bottom("status_bar")` 之前）插入：

```rust
        if self.tool != Tool::Select {
            egui::TopBottomPanel::bottom("style_panel").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(t(self.lang, T::StyleStrokeColor));
                    let mut c = self.default_stroke.color;
                    let mut col = egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
                    if ui.color_edit_button_srgba(&mut col).changed() {
                        c = [col.r(), col.g(), col.b(), col.a()];
                        self.default_stroke.color = c;
                    }
                    ui.separator();
                    ui.label(t(self.lang, T::StyleStrokeWidth));
                    ui.add(egui::Slider::new(&mut self.default_stroke.width, 0.5..=12.0).logarithmic(true));
                    ui.separator();
                    for (dash, label) in [
                        (DashStyle::Solid, T::StyleDashSolid),
                        (DashStyle::Dashed, T::StyleDashDashed),
                        (DashStyle::Dotted, T::StyleDashDotted),
                    ] {
                        let active = self.default_stroke.dash == dash;
                        if ui
                            .selectable_label(active, t(self.lang, label))
                            .clicked()
                        {
                            self.default_stroke.dash = dash;
                        }
                    }
                    ui.separator();
                    ui.label("填充");
                    let mut fill_checked = self.default_fill.is_some();
                    if ui.checkbox(&mut fill_checked, "F").changed() {
                        self.default_fill = if fill_checked {
                            Some([100, 180, 255, 60])
                        } else {
                            None
                        };
                    }
                });
            });
        }
```

> 说明：填充色 MVP 用"开关 + 固定半透明蓝"，避免引入复杂色板；后续可加色块选择。

- [ ] **Step 4：验证 + 手工验收**

Run: `cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Run: `cargo run -p preferz`（点工具栏切工具、改样式、绘制验证样式生效）

- [ ] **Step 5：Commit**

```bash
git add crates/preferz/src/preferz_app.rs crates/preferz/src/i18n.rs
git commit -m "feat(ui): add tool sidebar and shape style panel"
```

### Task A9：fileio item_kind_str + 持久化验证

**Files:**
- Modify: `crates/preferz-fileio/src/bee.rs`

- [ ] **Step 1：item_kind_str 加 Shape**

在 `bee.rs` 的 `item_kind_str` 函数（约 L320）的 match 中加：

```rust
        ItemKind::Shape { .. } => "shape",
```

- [ ] **Step 2：保存/加载验证**

Run: `cargo test --workspace`
Run: `cargo run -p preferz`（画若干形状 → Ctrl+S 保存 → 重新打开 → 形状完整还原）
Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings`

- [ ] **Step 3：Commit**

```bash
git add crates/preferz-fileio/src/bee.rs
git commit -m "feat(fileio): persist Shape items"
```

**Phase A 完成标准**：矩形/椭圆/菱形可画可改可存；`cargo test --workspace` 全绿；fmt + clippy 零警告。

---

# Phase B：线类（两点式直线/箭头）

**交付物**：直线/箭头工具，两点式创建，Shift 锁 45°，可命中/编辑/旋转/缩放/删除/undo/持久化。

**涉及文件**：同 Phase A + `stylers.rs`（线类绘制）、`item.rs`（base_size 线类分支、contains 线类距离命中）。

### Task B1：线类绘制（stylers.rs）

- [ ] **Step 1**：`ShapeData.points` 已有；`CleanStyler::build_shapes` 的 `Line | Arrow` 分支实现：
  - 局部空间两点 `points[0]=(0,0)`、`points[1]=(dx,dy)` 变换到屏幕。
  - 直线：`Shape::line([p0, p1], stroke)`（dash 用 `Shape::dashed_line`）。
  - 箭头：在终点按方向向量 ±25° 画两条短线（长 = `stroke.width * 4`）：
    ```rust
    let dir = (p1 - p0).normalized();
    let head_len = line_width * 4.0;
    let a1 = dir.rotate(std::f32::consts::FRAC_PI_2 * (5.0/9.0)); // ≈50°
    let a2 = dir.rotate(-std::f32::consts::FRAC_PI_2 * (5.0/9.0));
    // 箭头线 = p1 → p1 + a1*head_len, p1 → p1 + a2*head_len
    ```
  - 线宽 = `stroke.width * zoom`（与矩形一致）。
- [ ] **Step 2**：`item.rs` `base_size()` Shape 分支：Line/Arrow 返回 points 的 AABB（`points.iter()` min/max），保证 `local_to_canvas`/变换手柄正确。
- [ ] **Step 3**：`Item::contains_canvas_point` 对 Line/Arrow 改为点到线段距离（`points` 各段，阈值 `max(stroke.width, 6.0)` 局部单位，旋转由逆变换处理）——在 `item.rs` 加 `contains_local_point` 或直接改 `contains_canvas_point` 分发。

### Task B2：线类创建流程

- [ ] **Step 1**：`tool_switch_shortcut` 加 `A` → `Arrow`、`L` → `Line`；工具栏加 2 个按钮。
- [ ] **Step 2**：`finish_create_shape` 对 Line/Arrow 走不同路径：`points = [(0,0), (dx,dy)]`，`base_size = |dx|,|dy|`，pos = `(min_x, min_y)`；Shift 锁 45°（`angle = round(atan2(dy,dx) / (PI/4)) * PI/4`）。
- [ ] **Step 3**：渲染预览支持线（画 start→current 线段）。

### Task B3：测试 + 持久化

- [ ] **Step 1**：core 单测：线类 `base_size`、`contains_canvas_point` 距离命中（含旋转）、serde 往返。
- [ ] **Step 2**：`cargo test --workspace` + 手工验收（画线/箭头、命中、undo、保存重开）。
- [ ] **Step 3**：Commit（`feat: add line/arrow two-point drawing`）。

---

# Phase C：文本入形

**交付物**：双击封闭形状创建/编辑绑定文本，随形状移动缩放重排，容器 resize 联动，删除连带。

**涉及文件**：`item.rs`（Text.container_id）、`commands.rs`（容器 resize 联动命令或复用 TransformItem）、`preferz_app.rs`（双击分发、文本布局、跟随移动、删除连带）、`i18n.rs`。

### Task C1：数据模型

- [ ] **Step 1**：`ItemKind::Text` 加 `container_id: Option<ItemId>`（`#[serde(default)]`）。
- [ ] **Step 2**：core 单测：serde 往返（旧文本 JSON 无 container_id 字段也能加载）。

### Task C2：双击创建/编辑 + 换行布局

- [ ] **Step 1**：`update()` 双击逻辑：命中封闭 Shape（Rectangle/Ellipse/Diamond）→ 查找 `container_id == Some(shape_id)` 的 Text；有则进入编辑，无则创建 `Text{ container_id: Some(shape_id) }`（初始位置 = 容器左上角 + padding 8px）。
- [ ] **Step 2**：`render_text_editor`：编辑时 `TextEdit::desired_width` = 容器宽 - 2×padding，居中 `Align2`；提交时 push `AddItem`（新建）或 `EditTextContent`（现有）。编辑结束刷新 `measured_size`。
- [ ] **Step 3**：渲染绑定文本：在容器中心位置绘制（Galley 水平/垂直居中），字号 = `font_size * scale * zoom`。

### Task C3：联动与跟随

- [ ] **Step 1**：`MoveItems`/`TransformItem` 的成员集合收集时，若容器在集合内，把 `container_id` 指向它的 Text 也加入（在 `begin_drag` 收集 `start_transforms` 处扩展，以及删除时）。
- [ ] **Step 2**：容器缩放（HandleTransform）时文本重排：在 `end_drag` 提交后或预览中按新容器尺寸重排文本 `measured_size`；文本高度超出 → 容器向下撑高（预览模式改容器 transform，随 `TransformItem` 提交）。
- [ ] **Step 3**：删除容器 → 连带删除绑定文本（`delete_selected` 扩展成员收集）。
- [ ] **Step 4**：`container_id` 指向不存在 item → 加载/删除时清为 None。

### Task C4：测试 + 验收

- [ ] core 单测：绑定文本跟随查询、删除连带、container_id 清理。
- [ ] 手工验收：双击形状写字、resize 重排、高度撑开、拖动/删除容器文本跟随、undo 恢复。

---

# Phase D：Frame

**交付物**：可创建带编号 frame，动态收纳完全包含的 items，拖动连带，编号编辑，边框点选命中。

**涉及文件**：`item.rs`（ItemKind::Frame）、`scene.rs`（frame_members / frames_by_number / 编号冲突顺移）、`preferz_app.rs`（Tool::Frame、CreatingFrame、frame 渲染、边框命中、拖动连带）、`transform_handles.rs`（frame 无旋转/翻转——`should_show_*` 已对 Frame 返回 false）、`bee.rs`。

### Task D1：数据模型

- [x] **Step 1**：`ItemKind::Frame { number: u32, name: Option<String> }` + `Item::new_frame` + `base_size` 分支 + serde 单测。
- [x] **Step 2**：`scene.rs`：`frame_members(frame_id)`（item 的 `bounding_rect ⊆ frame` 的 canvas 矩形，frame 自身排除，绑定文本跟随容器）、`frames_by_number()`。
- [x] **Step 3**：编号冲突顺移：编辑编号与现有重复时，冲突及后续 `number += 1`（core 函数 + 单测）。

### Task D2：创建与渲染

- [x] **Step 1**：`Tool::Frame` + `M` 快捷键 + 工具栏按钮 + `DragState::CreatingFrame`（同 CreatingShape 模式）。
- [x] **Step 2**：`Item::new_frame`：transform 不旋转（创建即 rotation=0，`should_show_rotate` 已 false）；创建时 `z = min_z - 1`（帧恒在最底）。
- [x] **Step 3**：render_scene Frame 分支：虚线边框（灰 #555）+ 左上角编号角标（圆角矩形底 + number）+ name；不裁剪内容。
- [x] **Step 4**：`should_show_flip/rotate` 对 Frame 保持 false；四角缩放手柄仍可用（现有 render 角点 always 显示）。

### Task D3：交互

- [x] **Step 1**：命中：Frame 优先命中边框线（6px 阈值，`contains_canvas_point` 分发或单独 `contains_frame_border`），内容区域点击穿透到下层 item。
- [x] **Step 2**：拖动 = `MoveItems(frame + frame_members)`；缩放改变 frame 大小实时增减成员。
- [x] **Step 3**：删除 frame 只删自身，成员散落（无需弹窗）。
- [x] **Step 4**：编号编辑：点击角标 → 小输入框（egui `TextEdit`，数字 only）→ Enter 提交，冲突顺移。

### Task D4：测试 + 验收

- [x] core 单测：frame_members 包含/相切、绑定文本跟随、frames_by_number 排序、冲突顺移。
- [x] 手工验收：画 frame、拖入拖出自动收纳、编号编辑、拖动连带、删除散落、保存重开。

---

# Phase E：Slide 演示模式

**交付物**：F5 全屏演示，按 frame 编号翻页，fit-to-screen，Esc 恢复。

**涉及文件**：`preferz_app.rs`（AppMode、Present 渲染、导航、全屏）、`viewport.rs`（fit_to_frame 或复用 fit_to_content）、`i18n.rs`。

### Task E1：模式状态与进入/退出

- [x] **Step 1**：`enum AppMode { Edit, Present { slides, members, index, saved_pan, saved_zoom } }`，`PReferZApp` 加 `app_mode: AppMode`。
- [x] **Step 2**：F5 / 右键菜单 Present 进入：`frames_by_number()` 过滤 `size >= 10px` → `slides` 快照 + 预计算 `members`；无 frame flash 提示 `PresentNoFrames`；记录进入前 pan/zoom；`ViewportCommand::Fullscreen(true)`。
- [x] **Step 3**：Esc / F5 退出：`Fullscreen(false)` + 恢复 pan/zoom。翻页期间不重算 members（进入时固化）。

### Task E2：Present 渲染

- [x] **Step 1**：Present 模式跳过菜单栏/工具栏/状态栏/欢迎页/右键菜单（`update()` 顶部 `return` 分支统一处理）。
- [x] **Step 2**：`render_present`：纯色背景 → `present_compute_fit(frame_rect)`（每帧按当前 `screen_rect` 重算，`* 0.95`）→ `set_clip_rect(frame_screen_rect)` 内只画该帧成员（`draw_item_visual` 限定 item 集 + 视口剔除）。
- [x] **Step 3**：过渡：翻页时 `present_anim` 存目标 `(zoom, pan)`，指数插值 ~200ms（`ctx.request_repaint()` 每帧驱动）。
- [x] **Step 4**：右下角页码指示 "n / m"。

### Task E3：导航 + 空态

- [x] **Step 1**：Present 输入：`→/Space/PgDn` 下一页、`←/PgUp` 上一页、`Home/End` 首末页、`Esc/F5` 退出、滚轮 = 翻页（不缩放）。
- [x] **Step 2**：Present 为纯展示态：`update()` 早返回 + `handle_shortcuts` F5 分支，编辑快捷键/右键/undo 均被屏蔽。
- [x] **Step 3**：single slide 时 `present_goto` 直接返回（导航无操作）。

### Task E4：测试 + 验收

- [x] `cargo check / fmt / clippy -D warnings / cargo test --workspace` 全部通过（24 core + 4 fileio 测试全绿）。
- [ ] 手工验收：F5 → 翻页 → 过渡动画 → Esc 恢复窗口与视口；中途删除 frame 不影响本次演示。

---

# 快捷键总表（实施后，含冲突解决）

| 键 | 上下文 | 行为 | 备注 |
|---|---|---|---|
| V | Edit | 选择工具 | 新增 |
| R / O / D | Edit | 矩形 / 椭圆 / 菱形工具 | 新增（原"排列"单键移除） |
| A / L | Edit | 箭头 / 直线工具（Phase B） | 新增 |
| M | Edit | Frame 工具（Phase D） | 新增 |
| Shift（按住） | 绘制中 | 正方形 / 45° 锁定 | 新增 |
| C | Edit（Select） | 进入裁剪 | 保留（原行为） |
| I | Edit（Select） | 取色器 | 保留 |
| F | Edit（Select） | fit 画布 | 保留 |
| R / G / O | Edit（多选） | 排列 | **移除单键**，改走右键菜单 Arrange |
| F5 | Edit | 进入 Present（Phase E） | 新增 |
| →/←/Space/PgDn/PgUp/Home/End | Present | 翻页 | 新增 |
| Esc | Present / 绘制中 / 工具激活 | 退出 / 取消 / 回 Select | 新增 |

> 设计文档 `.agents/shapes-and-frame-slides-design.md` 附录 A 已同步此表。

---

# 测试与质量门槛（每期通用）

- `cargo fmt --all --check` 必须通过（提交前 `cargo fmt`）
- `cargo clippy --workspace --all-targets -- -D warnings` 必须零警告，不用 `#[allow]` 掩盖
- `cargo test --workspace` 全绿
- 每期完成后手工验收清单见各期

# 风险与备注

- **egui `Shape::convex_polygon`** 只支持凸多边形——矩形/菱形/椭圆（64 段近似）均凸，OK；如未来做星形等凹形需换 `PathShape::line` 填充。
- **椭圆命中**：MVP 用 OBB（`contains_canvas_point`）保守近似，边缘点选略宽松；如体验差可在 `item.rs` 对 Ellipse 做精确椭圆命中（点满足 `(dx/rx)^2+(dy/ry)^2 <= 1`）。
- **dash 线宽**：`Shape::dashed_line` 的 dash 段长度按 zoom 缩放，缩太小时可能几乎连续——可接受。
- **Phase C 高度撑开**涉及预览中改容器 transform，与现有 `TransformItem`（单 item）语义不同，需注意 undo 一致性（容器+文本两条命令或一条复合命令）。
