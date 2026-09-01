# PReferZ 图形绘制 + Frame Slides 设计

- 日期：2026-08-17
- 状态：已评审（设计讨论定稿，待实施）
- 参考：`.ref/excalidraw`（Excalidraw OSS 源码调研笔记见本文附录 B）

## 1. 目标与非目标

### 目标

1. **图形绘制**：在无限画布上绘制基础图形（矩形 / 椭圆 / 菱形 / 直线 / 箭头），用于标注参考图。
2. **文本入形**：文本可绑定到封闭形状内部，随形状移动 / 缩放重排。
3. **Frame 画框**：可创建带编号的 frame，动态收纳画布上的 items。
4. **Slide 演示**：按 frame 编号全屏播放（F5），方向键翻页，Esc 退出。

### 非目标（第一版明确不做）

- 手绘（roughjs）视觉风格 → 保留 trait 抽象，Phase F 实现
- 多点线编辑（加点 / 拖点 / 折线）→ points 字段预留
- freedraw 自由画笔
- 箭头绑定吸附（arrow binding 到形状边缘）
- 协同编辑 / fractional indexing

## 2. 核心决策记录

| 决策点 | 结论 | 理由 |
|---|---|---|
| 视觉风格 | 先简洁后手绘：`ShapeStyler` trait 抽象，Phase 1 简洁实现 | 零新依赖跑通全流程，手绘皮肤可后叠 |
| 图形范围 | 基础集：矩形/椭圆/菱形 + 两点式直线/箭头 | 覆盖标注场景 90%，状态机简单 |
| 文本绑定 | 支持：`Text.container_id`，双击形状进入编辑 | slide 标题框是高频需求，字段现在就建避免持久化迁移 |
| Frame 归属 | 动态包含判定：包围盒完全落入即属于，不存字段 | 零 schema 改动，"画框即一页"心智直观 |
| Slide 顺序 | Frame 显式 `number` 属性，可交互编辑 | 显式可控，避免空间排序的歧义和 z 序语义纠缠 |

## 3. 数据模型（preferz-core）

### 3.1 `ItemKind` 新增 variant

```rust
// item.rs

pub enum ItemKind {
    Pixmap { ... },
    Text {
        content: String,
        font_size: f32,
        color: [u8; 4],
        editing: bool,
        measured_size: Option<(f32, f32)>,
        container_id: Option<ItemId>,   // 新增：绑定的容器形状（None = 独立便签）
    },
    Shape {
        shape_type: ShapeType,          // Rectangle / Ellipse / Diamond / Line / Arrow
        base_size: (f32, f32),          // 局部空间尺寸（矩形族 = w×h；线类 = points 包围盒）
        points: Vec<(f32, f32)>,        // 线类专用，局部坐标；矩形族为空
        stroke: StrokeStyle,
        fill: Option<[u8; 4]>,          // RGBA；None = 透明
        seed: u64,                      // 手绘风确定性噪声预留（Phase F 用）
    },
    Frame {
        number: u32,                    // slide 播放序号，可编辑
        name: Option<String>,
    },
}

pub struct StrokeStyle {
    pub color: [u8; 4],
    pub width: f32,                     // 画布空间像素，默认 2.0
    pub dash: DashStyle,                // Solid / Dashed / Dotted
}
```

### 3.2 设计要点

- **完全复用 `Transform`**：shape 的旋转 / 缩放 / 翻转走现有矩阵体系，`local_to_canvas()` / `contains_canvas_point()` 的 OBB 逻辑直接生效。
- **`base_size` 语义与 Pixmap/Text 对齐**：`Item::base_size()` 分发到各 variant，矩形族返回 `base_size`，线类返回 points 的 AABB——与现有命中检测、变换手柄代码无缝衔接。
- **线类 points 存局部坐标**（Excalidraw 同款）：`points[0]` 相对 `transform.pos`，拖动整体 = 改 pos，与 MoveItems 命令兼容。
- **Frame 无旋转**：frame 的 transform.rotation 恒为 0（创建时锁定，旋转手柄对 frame 隐藏），保证动态包含判定始终是 AABB。
- **`seed` 现在就建字段**：手绘风需要 per-element 确定性噪声（Excalidraw 同款机制），后补字段要动持久化兼容，现在建好序列化，Phase F 才用。

### 3.3 Frame 动态包含判定

```rust
// scene.rs 新增
impl Scene {
    /// item 的画布空间 AABB 是否完全落入 frame 矩形（各含 margin 容差）。
    /// frame 自身、绑定文本跟随容器判定，不单独算。
    pub fn frame_members(&self, frame_id: ItemId) -> Vec<ItemId>;

    /// frame 编号排序的列表（播放顺序）
    pub fn frames_by_number(&self) -> Vec<ItemId>;
}
```

- 判定规则：`item_canvas_aabb ⊆ frame_rect`（含 0px 容差，严格包含）。
- **绑定文本跟随容器**：`container_id` 指向某 shape 的 text 不单独判定，容器在 frame 内则文本视为在 frame 内。
- 编号冲突处理：编辑编号时若与现有 frame 重复，自动将冲突方及后续 frame 顺移 +1（类似 PPT 行为）。
- frame 可以无内容（空 slide），也允许 frame 嵌套不判定（frame 不属于 frame，播放时各自独立）。

## 4. 风格生成器（binary 层，core 不依赖 egui）

```rust
// ui/stylers.rs

/// 将 shape 几何转换为 epaint Shape 列表。
/// 输入为局部坐标几何 + 最终的 local→screen 变换（风格器自行决定是否在局部空间生成后变换）。
trait ShapeStyler {
    fn build_shapes(&self, shape: &ShapeGeom, style: &StrokeStyle, fill: Option<Color32>,
                    to_screen: &ItemLocalToScreen) -> Vec<egui::Shape>;
}

struct CleanStyler;   // Phase 1：epaint 原生 PathShape / 圆角矩形 / 椭圆 / 虚线 stroke
struct RoughStyler;   // Phase F：roughr 或自实现 seeded 抖动，多段折线模拟
```

- **CleanStyler 实现要点**：
  - 矩形：`egui::Shape::rect_stroke`（fill 时 `rect_filled` 叠加），圆角 = `width * 2`（有 roundness 开关）
  - 椭圆：`epaint::CircleShape` 变体或 `PathShape` 用 `ellipse` 辅助函数
  - 菱形：四个顶点 `PathShape::closed`
  - 直线 / 箭头：`PathShape::line`；箭头头部 = 末端方向 ±25° 两条短线（长 = `width * 4`）
  - 虚线：`epaint::Stroke { width, color }` + `Shape::dashed_line`（epaint 已提供）
- **每个 shape 每帧重建**：矢量绘制成本远低于图片 blit，数十个 shape 无性能问题；RoughStyler 未来按 `(seed, size)` 缓存点列。

## 5. 工具状态机（preferz binary）

### 5.1 工具枚举与切换

```rust
// preferz_app.rs

enum Tool {
    Select,                     // 现有行为（默认）
    Shape(ShapeType),           // R / O / D / A / L 切换
    Frame,                      // M 切换
}
```

- 快捷键：`V` 选择、`R` 矩形、`O` 椭圆、`D` 菱形、`A` 箭头、`L` 直线、`M` frame（F 已被 fit 占用）。
- Esc / 提交后自动回 Select（Excalidraw 是保持工具，PReferZ 场景低频绘制，回 Select 更符合参考图板习惯；Shift 按住提交后保持工具）。
- 工具栏：左侧竖排图标按钮（矢量图标用 epaint 路径自绘，不引入图标库）+ 底部样式面板（描边色 / 宽度 / 线型 / 填充色）。

### 5.2 DragState 扩展

```rust
enum DragState {
    Idle,
    HandleTransform { ... },        // 现有
    MoveItems { ... },              // 现有
    MarqueeSelect { ... },          // 现有
    CreatingShape {                  // 新增：矩形族 + 线类统一（两点式）
        shape_type: ShapeType,
        start: CanvasPoint,
        current: CanvasPoint,
    },
    CreatingFrame {                  // 新增
        start: CanvasPoint,
        current: CanvasPoint,
    },
}
```

- **创建流程（与现有预览模式语义一致）**：pointer down 记 start → move 更新 current 画临时预览（painter 直接画，不入 scene）→ pointer up：
  - 尺寸 < 3px 画布像素视为误触，丢弃
  - 归一化（拖反向时 min/max 修正）；Shift 锁正方形 / 45° 角（矩形族锁 1:1，线类锁 0/45/90°）
  - 计算初始 `Transform{ pos, scale }`（base_size 为 1 时 scale 即尺寸）或直接存 base_size + scale=1（**采用后者**：创建时 base_size = 拖拽尺寸，scale = 1.0，与 Pixmap 语义一致）
  - `AddItem` 命令 push undo 栈
- **线类反向拖拽**：两点式不需要归一化，`points[0] = (0,0)`、`points[1] = (dx, dy)` 相对 pos。

### 5.3 命中检测扩展

- 封闭形状：现有 OBB `contains_canvas_point` 直接可用。
- 线类：新增点到线段距离检测（画布空间），阈值 = `max(stroke_width, 6px) / zoom`，考虑 transform 旋转（逆变换到局部空间后测局部线段，阈值用局部宽度）。
- Frame 命中：优先命中**边框线**（内容区域点击穿透到下层 item），边框命中阈值 6px——避免 frame 挡住其内容的点选。

### 5.4 选择与变换

- Shape 选中后走现有 TransformHandles（四角缩放 + 旋转 + 翻转），零改动。
- Frame 选中：四角缩放改变 frame 大小（动态包含自动增减成员，实时可见）；拖动 = `MoveItems(frame + 完全包含的成员)`，预览模式直接改、松手 push。
- 删除 frame：只删 frame 自身，成员散落画布（动态归属模型下无孤儿问题，无需确认弹窗）。

## 6. 文本入形

### 6.1 交互流程

- **进入编辑**：双击封闭形状（矩形/椭圆/菱形，线类不支持容器）：
  - 已有绑定文本 → 进入编辑（复用现有 TextEdit overlay 逻辑）
  - 无绑定文本 → 创建 `Text{ container_id: Some(id) }`，初始尺寸 = 容器内接矩形（padding 8px）
- **换行布局**：egui `TextEdit::desired_width` = 容器宽 - 2×padding，`LayoutJob` 居中对齐（水平 + 垂直），`measured_size` 从 Galley 取。
- **resize 联动**：容器缩放时文本重排；文本高度超出容器时**容器自动向下撑高**（TransformItem 命令，预览模式）——Excalidraw 同款行为，避免文字溢出被裁剪。
- **跟随**：拖动 / 旋转 / 翻转容器时，文本作为独立 item 一起进 `MoveItems` / `TransformItem` 的成员集合（查询 `container_id` 反向索引）。
- **删除**：删除容器连带删除绑定文本（DeleteItems 已是多 item 命令，扩展成员收集逻辑）；删除文本不影响容器。
- **禁用**：绑定文本不可单独拖动（只能编辑），选中容器时文本作为容器的一部分展示。

### 6.2 数据一致性

- `container_id` 指向不存在的 item（容器被外部手段删除）→ 加载 / 删除时清理为 None，文本退化为独立便签。
- undo 容器删除时，连带删除的文本一并恢复（DeleteItems 快照机制已支持多 item）。

## 7. Frame 渲染（编辑态）

- **视觉**：1px 虚线边框（暗灰 #555）+ 左上角编号角标（"1"、"2"...，小圆角矩形底 + 数字）+ name（有值时角标右侧显示）。
- **编号编辑**：点击角标 → 弹出小输入框（egui TextEdit，数字 only）→ 回车提交，冲突顺移。
- **不裁剪内容**：编辑态成员可越界显示（Excalidraw 默认也不 clip，clip 是可选项）——第一版从简，裁剪只在 slide 模式生效。
- **z 序**：frame 恒在所有普通 item 之下（创建时 `z = min_z - 1`，ReorderItems 不作用于 frame），边框线不遮挡内容点选。

## 8. Slide 演示模式

### 8.1 模式状态

```rust
// preferz_app.rs

enum AppMode {
    Edit,
    Present {
        slides: Vec<ItemId>,     // 按 number 排序的 frame 列表（进入时快照）
        index: usize,
    },
}
```

### 8.2 进入 / 退出

- **进入**（F5 或菜单 View → Present）：
  - 收集全部 frame 按 `number` 排序 → `slides` 快照；无 frame 时提示不进入
  - `ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true))`
  - 恢复用：记录进入前的 pan / zoom / window rect
- **退出**（Esc / F5 再按）：
  - `Fullscreen(false)` + 恢复进入前的 pan / zoom

### 8.3 渲染

- 背景：暗色主题纯色（`egui::Visuals::dark` 的 panel bg），整个 CentralPanel 不画常规 UI（菜单 / 状态栏 / 工具栏全部隐藏）。
- **视口**：fit-to-screen——`zoom = min(sw / fw, sh / fh) * 0.95`（clamp 到 viewport 上下限之外，Present 模式临时放宽 max_zoom），`pan = frame 中心`。
- **内容裁剪**：`Painter::with_clip_rect(frame_screen_rect)` 只画完全包含的成员（进入 Present 前预计算每个 frame 的成员快照，翻页不重算）。
- **过渡动画**：翻页时 zoom / pan 用指数插值（约 200ms，每帧 `ctx.request_repaint()` 驱动），简单淡入淡出可后加。
- **页码指示**：右下角半透明 "3 / 12"。

### 8.4 导航

| 键 | 行为 |
|---|---|
| → / Space / PgDn | 下一页 |
| ← / PgUp | 上一页 |
| Home / End | 首页 / 末页 |
| Esc / F5 | 退出 |
| 鼠标滚轮 | 下一页 / 上一页（Present 模式滚轮不缩放） |

### 8.5 空态与边界

- **Present 模式为纯展示态**：所有编辑操作（工具切换、删除、undo/redo、右键菜单）不响应，仅导航键和鼠标滚轮有效。
- slides 为进入时的快照：退出后重新进入才会刷新 frame 列表。
- frame 尺寸为 0（误创建后未删）→ 进入时过滤 `width/height < 10px` 的 frame。

## 9. 持久化兼容性

- `items.kind` 列为 serde JSON——`ItemKind` 新 variant **自动兼容**，`.prz` 无 schema 迁移（`USER_VERSION=3` 不变）。
- 旧版 PReferZ 读取新文件：serde 默认 `deny_unknown_fields` 未开启，未知 variant 会报错——**可接受**（向前兼容不做，版本内升级）。
- `.bee` 保存：新类型 item 落库格式与 `.prz` 相同（kind JSON），BeeRef 本身读不了但文件不损坏；读取旧 `.bee` 遇不到新类型（BeeRef 无 shape/frame），无冲突。
- `Text.container_id`、`Shape.seed` 均为带默认值字段（`#[serde(default)]`），旧文件加载不受影响。

## 10. 实施分期

| 期 | 内容 | 交付物 | 难度 |
|---|---|---|---|
| A | Shape 基础集：数据模型 + CleanStyler + 工具状态机 + 样式面板 + 快捷键 + 持久化 | 能画矩形/椭圆/菱形并保存 | ★★ |
| B | 线类：两点式直线/箭头 + 距离命中 + 箭头头部 + Shift 角度锁定 | 完整基础集 | ★★ |
| C | 文本入形：container_id + 换行布局 + resize 联动 + 跟随/删除连带 | 双击形状写字 | ★★★ |
| D | Frame：渲染 + 动态包含 + 编号编辑 + 拖动连带 + 点选边框命中 | 可组织画布分区 | ★★ |
| E | Slide 模式：AppMode + Present 渲染 + fit + 导航 + 全屏 | F5 演示 | ★★ |
| F | （后续）RoughStyler 手绘皮肤 / 多点线编辑 / freedraw | — | — |

每期独立可交付、可合入 main；A-C 无 frame 依赖可并行探索，D-E 严格顺序。

## 11. 测试策略

- **core 单测**（`#[cfg(test)]` co-located，遵循项目约定）：
  - Shape base_size / 命中（含旋转 OBB、线类距离）
  - Scene::frame_members 动态包含（含文本跟随容器、边界相切不包含）
  - frames_by_number 排序 + 编号冲突顺移
  - serde 往返：新 variant 读写 + 旧文件（无新字段的 JSON）加载
- **手工验收清单**（每期）：
  - A：画/选/移/旋/删/undo/保存重开
  - C：双击写字、resize 重排、高度撑开、删除连带恢复
  - E：F5 → 翻页 → Esc 恢复窗口和视口

## 12. 风险与缓解

| 风险 | 概率 | 缓解 |
|---|---|---|
| egui TextEdit 换行度量与 Galley 渲染不一致 | 中 | measured_size 统一从 Galley 取（现有 Text 已是此方案，修 B6 先例） |
| Frame 动态包含在拖动大 frame 时判定抖动 | 低 | 判定仅在 pointer up / 逐帧惰性重算，O(n) 遍历 items 对数百级无压力 |
| Present 模式全屏后 DPI 变化导致 fit 错位 | 中 | fit 每帧基于 `screen_rect` 重算（缓存 zoom 因子而非绝对值） |
| RoughStyler（roughr）不可用 | — | Phase F 才引入，届时验证；fallback = 自实现 seeded 抖动折线（工作量可控） |

## 附录 A. 快捷键总表（新增部分）

| 键 | 上下文 | 行为 |
|---|---|---|
| V | Edit | 选择工具 |
| R / O / D | Edit | 矩形 / 椭圆 / 菱形工具 |
| A / L | Edit | 箭头 / 直线工具 |
| M | Edit | Frame 工具 |
| Shift（按住） | 绘制中 | 正方形 / 45° 锁定；提交后保持工具 |
| F5 | Edit | 进入 Present |
| → / ← / Space / PgDn / PgUp / Home / End | Present | 翻页 |
| Esc | Present / 绘制中 | 退出 Present / 取消绘制回 Select |

## 附录 B. Excalidraw 调研要点（对照）

| Excalidraw 机制 | PReferZ 取舍 |
|---|---|
| 判别联合 ExcalidrawElement + 局部 points | 同构：ItemKind variant + 局部 points |
| roughjs ShapeCache + seed | trait 抽象 + seed 字段预留，Phase F |
| pointer down 创建 → move mutate → actionFinalize | 复用现有预览模式 + skip_first_redo |
| frameId 反向指针归属 | 改为动态包含判定（零 schema） |
| frameClip context.clip() | Painter::with_clip_rect（仅 Present 模式） |
| frames_to_slides presentation | OSS 无此功能（Excalidraw+ 付费），自研：number 排序 + fit + 导航 |
| containerId 双向绑定（boundElements 数组） | 单向 container_id + 反向查询（单机无协同，够用） |
| 文本高度溢出反向撑高容器 | 采纳同款行为 |
