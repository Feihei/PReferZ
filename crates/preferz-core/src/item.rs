use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shape::{
    ArrowHeadStyle, CurveType, FillStyle, FontFamily, ShapeType, Sloppiness, StrokeStyle,
    TextAlignH, TextAlignV, TextStyle,
};
use crate::spaces::{CanvasPoint, CanvasRect, CanvasVector};
use crate::transform::Transform;

pub type ItemId = Uuid;

/// 端点绑定（plan #5/#14）：线端点钉在目标图形上。
///
/// `target` 为被吸附图形；`anchor` 为端点钉在目标**局部坐标系**的表面点
/// （吸附命中时记录）。目标图形移动/缩放/旋转时按锚点重算端点画布位置，
/// 端点钉在同一表面点不沿边缘滑动。`anchor: None` 表示旧存档迁移（只有
/// 目标），`resolve_bindings` 回退“另一端点的最近轮廓点”旧行为。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EndpointBinding {
    pub target: ItemId,
    #[serde(default)]
    pub anchor: Option<(f32, f32)>,
}

/// 旧存档兼容：`start_binding`/`end_binding` 曾是 `Option<ItemId>`（JSON 中为
/// uuid 字符串或 null）。反序列化时接受 uuid 字符串（→ 无锚点绑定）或
/// `{target, anchor}` 对象，均归一为 `Option<EndpointBinding>`。
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum EndpointBindingRepr {
    Legacy(ItemId),
    Full {
        target: ItemId,
        #[serde(default)]
        anchor: Option<(f32, f32)>,
    },
}

pub fn deserialize_endpoint_binding_opt<'de, D>(
    deserializer: D,
) -> Result<Option<EndpointBinding>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let repr = Option::<EndpointBindingRepr>::deserialize(deserializer)?;
    Ok(repr.map(|r| match r {
        EndpointBindingRepr::Legacy(target) => EndpointBinding {
            target,
            anchor: None,
        },
        EndpointBindingRepr::Full { target, anchor } => EndpointBinding { target, anchor },
    }))
}

/// Item 局部坐标空间。原点为 item 的 `transform.pos`，未旋转/缩放前的左上角。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemLocalSpace;

/// Item 局部 → 画布 变换矩阵。
pub type ItemLocalToCanvas = euclid::Transform2D<f32, ItemLocalSpace, crate::spaces::CanvasSpace>;
/// 画布 → Item 局部 变换矩阵（用于 hit-test）。
pub type CanvasToItemLocal = euclid::Transform2D<f32, crate::spaces::CanvasSpace, ItemLocalSpace>;
/// Item 局部 → 屏幕 变换矩阵（画布 → 屏幕 再叠加 ItemLocal → Canvas）。
pub type ItemLocalToScreen = euclid::Transform2D<f32, ItemLocalSpace, crate::spaces::ScreenSpace>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ItemKind {
    Pixmap {
        texture_id: u64,
        filename: Option<String>,
        original_size: (u32, u32),
        opacity: f32,
        grayscale: bool,
        crop: Option<CropRect>,
    },
    Text {
        content: String,
        font_size: f32,
        color: [u8; 4], // RGBA
        editing: bool,
        /// UI 层用 egui 实际测量的文字尺寸（画布空间像素）。
        /// `base_size()` 优先用此值，使变换边框与实际渲染一致（修 B6）。
        /// None 时退回到字符宽度估算。
        measured_size: Option<(f32, f32)>,
        /// 绑定的容器形状 id（None = 自由文本）。绑定文本随容器联动（Phase C）。
        /// `#[serde(default)]`：旧存档无此字段也能加载。
        #[serde(default)]
        container_id: Option<ItemId>,
        /// 文字背景色（RGBA）。`None` = **全透明**（默认，与 Excalidraw 一致）。
        ///
        /// 保留此字段是为了将来侧栏能按需给文字加底色；默认不画任何背景矩形，
        /// 避免出现"文字自带灰色底"的观感。
        /// `#[serde(default)]`：旧存档无此字段视为透明。
        #[serde(default)]
        background: Option<[u8; 4]>,
        /// 水平对齐（plan #1）。仅绑定文字生效；默认 `Center`（旧存档自动居中）。
        #[serde(default)]
        align_h: TextAlignH,
        /// 垂直对齐（plan #1）。仅绑定文字生效；默认 `Middle` 同上。
        #[serde(default)]
        align_v: TextAlignV,
        /// 字体族（plan #1）：黑体 / 伪手写。默认黑体。
        #[serde(default)]
        font_family: FontFamily,
    },
    Shape {
        shape_type: ShapeType,
        /// 局部空间尺寸（矩形族 = w×h；线类 = points 包围盒）。
        base_size: (f32, f32),
        /// 线性对象顶点，局部坐标（矩形族为空 Vec；N ≥ 2）。
        points: Vec<(f32, f32)>,
        stroke: StrokeStyle,
        fill: Option<[u8; 4]>, // RGBA；None = 无填充（Excalidraw "transparent"）
        /// 填充样式（仅闭合图形生效；fill 为 None 时无意义）。
        /// `#[serde(default)]`：旧存档缺该字段按 Solid 加载（历史 fill=Some 即纯色）。
        #[serde(default)]
        fill_style: FillStyle,
        /// 起点箭头样式（仅 Polyline 使用；矩形族忽略）。
        start_arrow: Option<ArrowHeadStyle>,
        /// 终点箭头样式（仅 Polyline 使用；矩形族忽略）。
        end_arrow: Option<ArrowHeadStyle>,
        /// 是否闭合（仅 Polyline 使用；矩形族忽略）。闭合时首尾相连，可填充。
        /// `#[serde(default)]`：旧存档无此字段按 `false`（开放）加载（Phase I 子阶段 D 补）。
        #[serde(default)]
        closed: bool,
        /// 曲线模式（Phase I）。仅 Polyline 使用；`Curved` 经 Catmull-Rom 插值。
        /// `#[serde(default)]`：旧存档无此字段按 `Straight` 加载。
        #[serde(default)]
        curve_type: CurveType,
        /// 矩形族圆角比例 0..1（Phase I）。仅矩形族使用；`radius = min(w,h) * roundness`。
        /// `#[serde(default)]`：旧存档无此字段按 0.0（直角）加载。
        #[serde(default)]
        roundness: f32,
        /// 手绘风描边抖动种子（Phase F）。同种子恒得同一抖动，保证重绘/存盘后形状不变。
        seed: u64,
        /// 手绘风抖动档位（plan #3，取代旧 `rough: bool`）。
        /// `alias = "rough"` 兼容旧存档的 bool 字段（false→Off、true→Artist），
        /// 见 [`crate::shape::deserialize_sloppiness`]。
        /// `#[serde(default)]`：旧存档无此字段（未开手绘风）按 `Off` 加载。
        #[serde(
            default,
            alias = "rough",
            deserialize_with = "crate::shape::deserialize_sloppiness"
        )]
        sloppiness: Sloppiness,
        /// 端点绑定（吸附语义，仅 Polyline 使用）：`points[0]` = `start_binding`，
        /// `points[last]` = `end_binding`。绑定记录**目标 + 锚点**（锚点为端点钉在
        /// 目标局部坐标系的位置），目标图形移动/缩放/旋转时由
        /// `Scene::resolve_bindings` 按锚点重算端点，端点钉在同一表面点不滑动
        /// （Excalidraw 语义）。`anchor=None`（旧存档迁移）回退“最近轮廓点”重算。
        #[serde(default, deserialize_with = "deserialize_endpoint_binding_opt")]
        start_binding: Option<EndpointBinding>,
        /// 见 `start_binding`（终点）。
        #[serde(default, deserialize_with = "deserialize_endpoint_binding_opt")]
        end_binding: Option<EndpointBinding>,
    },
    /// 幻灯片画框（Phase D）。不旋转/不翻转；仅边框点选命中，内容区域点击穿透。
    /// 渲染恒在其它 item 之下（创建时 z 置为最小）。
    Frame {
        /// 局部空间尺寸（宽×高）。
        base_size: (f32, f32),
        /// 画框编号（决定演示翻页顺序）。
        number: u32,
        /// 可选名称。
        #[serde(default)]
        name: Option<String>,
    },
}

impl ItemKind {
    pub fn pixmap_original_width(&self) -> Option<f32> {
        match self {
            ItemKind::Pixmap { original_size, .. } => Some(original_size.0 as f32),
            _ => None,
        }
    }

    pub fn pixmap_original_height(&self) -> Option<f32> {
        match self {
            ItemKind::Pixmap { original_size, .. } => Some(original_size.1 as f32),
            _ => None,
        }
    }

    /// 更新线性对象（Polyline）端点，并把 `base_size` 同步为 points 的 AABB，
    /// 保证持久化与 `base_size()` 推导一致。非线类调用无副作用。
    pub fn set_line_points(&mut self, points: Vec<(f32, f32)>) {
        if let ItemKind::Shape {
            shape_type,
            base_size,
            points: pts,
            ..
        } = self
        {
            *pts = points;
            if matches!(shape_type, ShapeType::Polyline) && !pts.is_empty() {
                let mut min_x = f32::MAX;
                let mut min_y = f32::MAX;
                let mut max_x = f32::MIN;
                let mut max_y = f32::MIN;
                for (x, y) in pts.iter() {
                    min_x = min_x.min(*x);
                    min_y = min_y.min(*y);
                    max_x = max_x.max(*x);
                    max_y = max_y.max(*y);
                }
                *base_size = (max_x - min_x, max_y - min_y);
            }
        }
    }

    /// Text 样式快照（plan #1 扩展对齐与字体族）；非 Text 返回 `None`。
    pub fn text_style(&self) -> Option<TextStyle> {
        match self {
            ItemKind::Text {
                font_size,
                color,
                background,
                align_h,
                align_v,
                font_family,
                ..
            } => Some(TextStyle {
                font_size: *font_size,
                color: *color,
                background: *background,
                align_h: *align_h,
                align_v: *align_v,
                font_family: *font_family,
            }),
            _ => None,
        }
    }

    /// 整份写入 Text 样式（供 `SetTextStyle` 命令使用）；
    /// 同时作废测量尺寸缓存（字号变化后变换框须重测）。非 Text 调用无副作用。
    pub fn set_text_style(&mut self, style: TextStyle) {
        if let ItemKind::Text {
            font_size,
            color,
            background,
            align_h,
            align_v,
            font_family,
            measured_size,
            ..
        } = self
        {
            *font_size = style.font_size;
            *color = style.color;
            *background = style.background;
            *align_h = style.align_h;
            *align_v = style.align_v;
            *font_family = style.font_family;
            *measured_size = None;
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl CropRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// 限制 crop 在 [0, w] × [0, h] 范围内并保证 width/height > 0。
    pub fn clamp_to(self, w: f32, h: f32) -> Self {
        let x = self.x.clamp(0.0, w.max(0.0));
        let y = self.y.clamp(0.0, h.max(0.0));
        let max_w = (w - x).max(0.0);
        let max_h = (h - y).max(0.0);
        let width = self.width.min(max_w).max(1.0);
        let height = self.height.min(max_h).max(1.0);
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: ItemId,
    pub kind: ItemKind,
    pub transform: Transform,
    pub z: i32,
    /// 所属编组 id（plan #13，单组模型：一个元素至多属一组）。
    /// `None` = 未编组。同组元素点击命中扩展为整组选中（G2）。
    /// `#[serde(default)]`：旧存档无此字段按未编组加载。
    #[serde(default)]
    pub group_id: Option<Uuid>,
}

impl Item {
    pub fn new_pixmap(
        texture_id: u64,
        filename: Option<String>,
        original_size: (u32, u32),
        pos_x: f32,
        pos_y: f32,
        scale_x: f32,
        scale_y: f32,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: ItemKind::Pixmap {
                texture_id,
                filename,
                original_size,
                opacity: 1.0,
                grayscale: false,
                crop: None,
            },
            transform: Transform::new(pos_x, pos_y, scale_x, scale_y),
            z: 0,
            group_id: None,
        }
    }

    pub fn new_text(
        content: String,
        pos_x: f32,
        pos_y: f32,
        font_size: f32,
        color: [u8; 4],
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: ItemKind::Text {
                content,
                font_size,
                color,
                editing: false,
                measured_size: None,
                container_id: None,
                background: None,
                align_h: TextAlignH::Center,
                align_v: TextAlignV::Middle,
                font_family: FontFamily::Normal,
            },
            transform: Transform::new(pos_x, pos_y, 1.0, 1.0),
            z: 0,
            group_id: None,
        }
    }

    /// 绑定到容器（封闭形状）的文本构造器。`pos` 通常为容器中心（Phase C）。
    /// 由调用方负责后续随容器联动（移动/缩放/删除连带）。
    pub fn new_text_in(
        content: String,
        pos_x: f32,
        pos_y: f32,
        font_size: f32,
        color: [u8; 4],
        container_id: ItemId,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: ItemKind::Text {
                content,
                font_size,
                color,
                editing: false,
                measured_size: None,
                container_id: Some(container_id),
                background: None,
                align_h: TextAlignH::Center,
                align_v: TextAlignV::Middle,
                font_family: FontFamily::Normal,
            },
            transform: Transform::new(pos_x, pos_y, 1.0, 1.0),
            z: 0,
            group_id: None,
        }
    }

    /// 该 item 是否可作为文本容器（封闭形状）。矩形/椭圆/菱形恒可；多段线仅闭合时可。
    pub fn shape_can_host_text(&self) -> bool {
        if let ItemKind::Shape {
            shape_type, closed, ..
        } = &self.kind
        {
            !matches!(shape_type, ShapeType::Polyline) || *closed
        } else {
            false
        }
    }

    /// 文字背景色（`None` = 透明）。非 Text item 恒为 `None`。
    pub fn text_background(&self) -> Option<[u8; 4]> {
        match &self.kind {
            ItemKind::Text { background, .. } => *background,
            _ => None,
        }
    }

    /// 设置文字背景色。非 Text item 无副作用。
    pub fn set_text_background(&mut self, background: Option<[u8; 4]>) {
        if let ItemKind::Text { background: bg, .. } = &mut self.kind {
            *bg = background;
        }
    }

    /// 幻灯片画框构造器（Phase D）。创建时 transform 不旋转不翻转。
    pub fn new_frame(
        number: u32,
        base_size: (f32, f32),
        pos_x: f32,
        pos_y: f32,
        name: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: ItemKind::Frame {
                base_size,
                number,
                name,
            },
            transform: Transform::new(pos_x, pos_y, 1.0, 1.0),
            z: 0,
            group_id: None,
        }
    }

    /// 该 item 是否为画框。
    pub fn is_frame(&self) -> bool {
        matches!(self.kind, ItemKind::Frame { .. })
    }

    /// 画框编号（非画框返回 None）。
    pub fn frame_number(&self) -> Option<u32> {
        match &self.kind {
            ItemKind::Frame { number, .. } => Some(*number),
            _ => None,
        }
    }

    /// 画框名称（非画框返回 None）。
    pub fn frame_name(&self) -> Option<&str> {
        match &self.kind {
            ItemKind::Frame { name, .. } => name.as_deref(),
            _ => None,
        }
    }

    /// 画框是否在边界阈值内命中（仅边框；内容区域穿透）。`threshold` 为画布单位。
    pub fn frame_border_hit(&self, canvas_pos: CanvasPoint, threshold: f32) -> bool {
        if !self.is_frame() {
            return false;
        }
        let r = self.bounding_rect();
        let min = r.min();
        let max = r.max();
        // 扩展矩形（含阈值松弛）：点须落在此范围内。
        let in_x = canvas_pos.x >= min.x - threshold && canvas_pos.x <= max.x + threshold;
        let in_y = canvas_pos.y >= min.y - threshold && canvas_pos.y <= max.y + threshold;
        if !(in_x && in_y) {
            return false;
        }
        // 内缩矩形（阈值内缩）：点落在内缩矩形内部 = 远离边框 = 非边框命中。
        // 旧实现用 `dx <= threshold && dy <= threshold`，但内部点 dx=dy=0 也满足，
        // 导致整个 frame 内部都被判为边框命中，吞掉框内成员的点击（issue #1）。
        let inner_x = canvas_pos.x > min.x + threshold && canvas_pos.x < max.x - threshold;
        let inner_y = canvas_pos.y > min.y + threshold && canvas_pos.y < max.y - threshold;
        // 边框命中 = 在扩展矩形内 且 不在内缩矩形内。
        !(inner_x && inner_y)
    }

    /// 修改画框编号（非画框无副作用）。
    pub fn set_frame_number(&mut self, number: u32) {
        if let ItemKind::Frame { number: n, .. } = &mut self.kind {
            *n = number;
        }
    }

    /// 修改画框名称（非画框无副作用）。
    pub fn set_frame_name(&mut self, name: Option<String>) {
        if let ItemKind::Frame { name: nm, .. } = &mut self.kind {
            *nm = name;
        }
    }

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
                fill_style: FillStyle::Solid,
                start_arrow: None,
                end_arrow: None,
                closed: false,
                curve_type: CurveType::Straight,
                roundness: 0.0,
                seed: 0,
                sloppiness: Sloppiness::Off,
                start_binding: None,
                end_binding: None,
            },
            transform: Transform::new(pos_x, pos_y, 1.0, 1.0),
            z: 0,
            group_id: None,
        }
    }

    /// 线性对象（Polyline）构造器：points 为局部坐标（已对齐左上角 AABB），
    /// base_size 即 AABB 宽高；起点/终点箭头独立可配；closed 表示首尾相连闭合
    /// （闭合图形可填充，箭头无意义）；线类默认无填充。
    /// 参数均为独立数据位（无重叠可合并），故显式列出而非引入 builder 结构体。
    #[allow(clippy::too_many_arguments)]
    pub fn new_polyline(
        points: Vec<(f32, f32)>,
        base_size: (f32, f32),
        start_arrow: Option<ArrowHeadStyle>,
        end_arrow: Option<ArrowHeadStyle>,
        closed: bool,
        pos_x: f32,
        pos_y: f32,
        stroke: StrokeStyle,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: ItemKind::Shape {
                shape_type: ShapeType::Polyline,
                base_size,
                points,
                stroke,
                fill: None,
                fill_style: FillStyle::Solid,
                start_arrow,
                end_arrow,
                closed,
                curve_type: CurveType::Straight,
                roundness: 0.0,
                seed: 0,
                sloppiness: Sloppiness::Off,
                start_binding: None,
                end_binding: None,
            },
            transform: Transform::new(pos_x, pos_y, 1.0, 1.0),
            z: 0,
            group_id: None,
        }
    }

    /// 手绘风抖动档位（非 Shape 恒为 `Off`）。
    pub fn sloppiness(&self) -> Sloppiness {
        match &self.kind {
            ItemKind::Shape { sloppiness, .. } => *sloppiness,
            _ => Sloppiness::Off,
        }
    }

    /// builder：设置手绘风档位。非 `Off` 时若 `seed` 仍为 0，则生成一个随机种子，
    /// 避免所有手绘图形抖出一模一样的轮廓。
    pub fn with_sloppiness(mut self, sloppiness: Sloppiness) -> Self {
        self.set_sloppiness(sloppiness);
        self
    }

    /// 设置手绘风档位（供 `SetSloppiness` 命令使用），语义同 [`Item::with_sloppiness`]。
    pub fn set_sloppiness(&mut self, sloppiness: Sloppiness) {
        if let ItemKind::Shape {
            sloppiness: s,
            seed,
            ..
        } = &mut self.kind
        {
            *s = sloppiness;
            if sloppiness != Sloppiness::Off && *seed == 0 {
                *seed = Uuid::new_v4().as_u128() as u64;
            }
        }
    }

    /// 整份写入 Text 样式（plan #1：委托给 [`ItemKind::set_text_style`]）。
    pub fn set_text_style(&mut self, style: TextStyle) {
        self.kind.set_text_style(style);
    }

    /// 画布点 → item 局部坐标（逆 [`Item::local_to_canvas`]）。退化变换（缩放为 0）
    /// 时返回 `None`。吸附/联动换算端点位置用。
    pub fn canvas_to_local_point(&self, p: CanvasPoint) -> Option<(f32, f32)> {
        self.local_to_canvas().inverse().map(|inv| {
            let q = inv.transform_point(p);
            (q.x, q.y)
        })
    }

    /// item 局部坐标点 → 画布点（[`Item::local_to_canvas`] 正向变换）。
    /// 吸附查询端点位置时用。
    pub fn local_point_to_canvas(&self, p: (f32, f32)) -> CanvasPoint {
        self.local_to_canvas()
            .transform_point(euclid::Point2D::<f32, ItemLocalSpace>::new(p.0, p.1))
    }

    /// 起点（`points[0]`）绑定的目标 item id（仅 Polyline 有意义）。
    pub fn start_binding(&self) -> Option<EndpointBinding> {
        match &self.kind {
            ItemKind::Shape { start_binding, .. } => *start_binding,
            _ => None,
        }
    }

    /// 终点（`points[last]`）绑定的目标 item id（仅 Polyline 有意义）。
    pub fn end_binding(&self) -> Option<EndpointBinding> {
        match &self.kind {
            ItemKind::Shape { end_binding, .. } => *end_binding,
            _ => None,
        }
    }

    /// 设置起点绑定目标（非 Polyline 无副作用）。
    pub fn set_start_binding(&mut self, binding: Option<EndpointBinding>) {
        if let ItemKind::Shape { start_binding, .. } = &mut self.kind {
            *start_binding = binding;
        }
    }

    /// 设置终点绑定目标（非 Polyline 无副作用）。
    pub fn set_end_binding(&mut self, binding: Option<EndpointBinding>) {
        if let ItemKind::Shape { end_binding, .. } = &mut self.kind {
            *end_binding = binding;
        }
    }

    /// builder：设置填充样式（仅 Shape 生效）。颜色由 `fill` 字段独立表达，
    /// "无填充" = `fill: None`，本方法只写样式值。
    pub fn with_fill_style(mut self, style: FillStyle) -> Self {
        if let ItemKind::Shape { fill_style, .. } = &mut self.kind {
            *fill_style = style;
        }
        self
    }

    /// Item 未旋转/缩放前的原始尺寸（item 局部空间的宽高，单位：画布空间像素）。
    ///
    /// 注意：返回的是"未应用 scale"的尺寸；`scale` 由调用方通过
    /// [`local_to_canvas`] 自行应用到矩形。
    ///
    /// [`local_to_canvas`]: Item::local_to_canvas
    pub fn base_size(&self) -> CanvasVector {
        match &self.kind {
            ItemKind::Pixmap { original_size, .. } => {
                CanvasVector::new(original_size.0 as f32, original_size.1 as f32)
            }
            ItemKind::Text {
                content,
                font_size,
                measured_size,
                ..
            } => {
                // 优先用 UI 层 egui 实际测量的尺寸（修 B6：边框与渲染内容一致）
                if let Some((w, h)) = measured_size {
                    return CanvasVector::new(w.max(1.0), h.max(1.0));
                }
                // 估算文本宽度：CJK 字符约 1.0 * font_size，ASCII 约 0.6 * font_size。
                let width: f32 = content
                    .chars()
                    .map(|c| {
                        if c.is_ascii() && !c.is_ascii_control() {
                            0.6
                        } else {
                            1.0
                        }
                    })
                    .sum::<f32>()
                    * font_size;
                let height = *font_size * 1.2;
                CanvasVector::new(width.max(1.0), height.max(1.0))
            }
            ItemKind::Shape {
                shape_type,
                base_size,
                points,
                ..
            } => {
                // 线类：base_size 取 points 的 AABB（局部坐标已对齐左上角），
                // 保证 local_to_canvas / 变换手柄正确。
                if matches!(shape_type, ShapeType::Polyline) && !points.is_empty() {
                    let mut min_x = f32::MAX;
                    let mut min_y = f32::MAX;
                    let mut max_x = f32::MIN;
                    let mut max_y = f32::MIN;
                    for (x, y) in points.iter() {
                        min_x = min_x.min(*x);
                        min_y = min_y.min(*y);
                        max_x = max_x.max(*x);
                        max_y = max_y.max(*y);
                    }
                    return CanvasVector::new((max_x - min_x).max(1.0), (max_y - min_y).max(1.0));
                }
                CanvasVector::new(base_size.0.max(1.0), base_size.1.max(1.0))
            }
            ItemKind::Frame { base_size, .. } => {
                CanvasVector::new(base_size.0.max(1.0), base_size.1.max(1.0))
            }
        }
    }

    /// 构造 ItemLocal → Canvas 的仿射变换。
    ///
    /// 局部坐标系的原点 (0,0) 对应 item 的 `transform.pos`，正 X 向右、正 Y 向下。
    /// 该变换已应用 `flip`、`scale`、`rotation`，因此用其逆变换把画布点变到局部空间后，
    /// 用 `base_size()` 做轴对齐测试即可正确处理旋转后的命中。
    pub fn local_to_canvas(&self) -> ItemLocalToCanvas {
        // 顺序：先翻转/缩放（绕局部中点），再旋转（绕局部原点），再平移到 pos
        let base = self.base_size();
        let mut t = ItemLocalToCanvas::identity();

        // 1) flip（绕局部中点镜像：scale 后平移补偿 base，使翻转绕 (base/2) 进行）
        let sx = if self.transform.flip_h { -1.0 } else { 1.0 };
        let sy = if self.transform.flip_v { -1.0 } else { 1.0 };
        if sx != 1.0 || sy != 1.0 {
            t = t.then_scale(sx, sy);
            let tx = if self.transform.flip_h { base.x } else { 0.0 };
            let ty = if self.transform.flip_v { base.y } else { 0.0 };
            if tx != 0.0 || ty != 0.0 {
                t = t.then_translate(CanvasVector::new(tx, ty));
            }
        }

        // 2) scale（局部尺寸缩放）
        if self.transform.scale.x != 1.0 || self.transform.scale.y != 1.0 {
            t = t.then_scale(self.transform.scale.x, self.transform.scale.y);
        }

        // 3) 旋转（绕局部原点）
        if self.transform.rotation != 0.0 {
            t = t.then_rotate(euclid::Angle::radians(self.transform.rotation));
        }

        // 4) 平移到 pos
        t = t.then_translate(self.transform.pos);

        t
    }

    /// 画布点是否落在 item 内（OBB 命中，正确处理旋转/翻转/缩放；
    /// 线类改为点到线段距离命中）。
    pub fn contains_canvas_point(&self, canvas_pos: CanvasPoint) -> bool {
        // 画框不参与内容命中：仅边框命中（见 [`Item::frame_border_hit`]），内容穿透到下层。
        if self.is_frame() {
            return false;
        }
        let inv = match self.local_to_canvas().inverse() {
            Some(inv) => inv,
            None => return false,
        };
        let local = inv.transform_point(canvas_pos);
        if let ItemKind::Shape {
            shape_type,
            points,
            stroke,
            closed,
            curve_type,
            ..
        } = &self.kind
        {
            if matches!(shape_type, ShapeType::Polyline) {
                // 线类命中：点到任一（采样后）线段距离 ≤ max(线宽, 6.0)。
                // 旋转/缩放由逆变换处理；曲线先经 Catmull-Rom 采样成折线再测距。
                let threshold = stroke.width.max(6.0);
                if points.len() >= 2 {
                    let pts = if matches!(curve_type, CurveType::Curved) {
                        catmull_rom_polyline(points, *closed, CURVE_SAMPLES)
                    } else {
                        points.clone()
                    };
                    let seg_hit = pts
                        .windows(2)
                        .any(|seg| dist_point_segment(local, seg[0], seg[1]) <= threshold);
                    if seg_hit {
                        return true;
                    }
                    // 闭合图形：命中多边形内部也算（填充区域可点选）
                    if *closed && point_in_polygon((local.x, local.y), &pts) {
                        return true;
                    }
                }
                return false;
            }
        }
        let size = self.base_size();
        // 局部空间下的命中矩形：[0, size.x] x [0, size.y]
        local.x >= 0.0 && local.x <= size.x && local.y >= 0.0 && local.y <= size.y
    }

    /// Item 在画布空间下的轴对齐包围盒（用于粗剔除；旋转后的精确命中应走
    /// [`contains_canvas_point`]）。
    pub fn bounding_rect(&self) -> CanvasRect {
        let size = self.base_size();
        // 取 4 个局部角点变换到画布空间，再求 AABB
        let corners = [
            euclid::Point2D::<_, ItemLocalSpace>::origin(),
            euclid::Point2D::new(size.x, 0.0),
            euclid::Point2D::new(0.0, size.y),
            euclid::Point2D::new(size.x, size.y),
        ];
        let to_canvas = self.local_to_canvas();
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for c in corners.iter() {
            let p = to_canvas.transform_point(*c);
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
        CanvasRect::new(
            euclid::Point2D::new(min_x, min_y),
            euclid::Size2D::new(max_x - min_x, max_y - min_y),
        )
    }

    /// 4 个角点（画布空间），按 TopLeft / TopRight / BottomLeft / BottomRight 顺序。
    /// 用于变换手柄绘制与命中。
    ///
    /// ⚠️ **不是环形顺序**（第 3 个是 BL 而非 BR）：直接按此顺序连多边形会画成
    /// 交叉四边形。要画轮廓请用 [`Item::canvas_corners_ring`]。
    pub fn canvas_corners(&self) -> [CanvasPoint; 4] {
        let size = self.base_size();
        let to_canvas = self.local_to_canvas();
        [
            to_canvas.transform_point(euclid::Point2D::<_, ItemLocalSpace>::origin()),
            to_canvas.transform_point(euclid::Point2D::new(size.x, 0.0)),
            to_canvas.transform_point(euclid::Point2D::new(0.0, size.y)),
            to_canvas.transform_point(euclid::Point2D::new(size.x, size.y)),
        ]
    }

    /// 同 [`Item::canvas_corners`]，但重排为**环形顺序** TL → TR → BR → BL，
    /// 可直接用于连多边形（`Shape::convex_polygon` `PathShape::closed_line` 等）。
    pub fn canvas_corners_ring(&self) -> [CanvasPoint; 4] {
        let c = self.canvas_corners();
        // canvas_corners 是 [TL, TR, BL, BR] → 环形 [TL, TR, BR, BL]
        [c[0], c[1], c[3], c[2]]
    }

    /// 当前生效的裁剪区域（原图像素坐标）；未裁剪时为整图。
    ///
    /// 即画面上**实际可见**的那块原图区域。应用裁剪时 `transform.scale` 会同步按
    /// 裁剪比例缩小，因此 item 的显示尺寸始终等于该区域的像素尺寸。
    pub fn current_crop(&self) -> Option<CropRect> {
        let (ow, oh) = self.original_pixel_size()?;
        match &self.kind {
            ItemKind::Pixmap { crop: Some(c), .. } => Some(*c),
            ItemKind::Pixmap { crop: None, .. } => Some(CropRect::new(0.0, 0.0, ow, oh)),
            _ => None,
        }
    }

    /// 裁剪框 4 角（画布空间），按**环形顺序** TL → TR → BR → BL。
    ///
    /// `crop` 以原图像素为单位（见 [`CropRect`]），但归一化基准是
    /// [`Item::current_crop`]（**当前可见区域**）而非整图——应用裁剪后
    /// `transform.scale` 已按裁剪比例缩小，若仍以整图为基准会二次缩放，
    /// 裁剪框比图片小一圈。以当前可见区域为基准，`crop == current_crop()`
    /// 时裁剪框与图片重合。
    ///
    /// 归一化后再走完整的 [`Item::local_to_canvas`]，旋转/缩放/翻转下裁剪框
    /// 会**跟随 item 一起变换**。
    ///
    /// ⚠️ 不要用画布 AABB 对 crop 做线性插值来定位裁剪框——AABB 在旋转下会膨胀，
    /// 且轴对齐插值不含旋转，裁剪框会与图片错位（历史 bug）。
    ///
    /// 非 Pixmap 或尺寸退化时返回 `None`。
    pub fn crop_corners(&self, crop: CropRect) -> Option<[CanvasPoint; 4]> {
        let base = self.current_crop()?;
        let size = self.base_size();
        if base.width <= 0.0 || base.height <= 0.0 || size.x <= 0.0 || size.y <= 0.0 {
            return None;
        }
        let to_canvas = self.local_to_canvas();
        let local = |u: f32, v: f32| {
            to_canvas.transform_point(euclid::Point2D::<_, ItemLocalSpace>::new(
                u * size.x,
                v * size.y,
            ))
        };
        let (u0, v0) = (
            (crop.x - base.x) / base.width,
            (crop.y - base.y) / base.height,
        );
        let (u1, v1) = (
            (crop.x + crop.width - base.x) / base.width,
            (crop.y + crop.height - base.y) / base.height,
        );
        Some([local(u0, v0), local(u1, v0), local(u1, v1), local(u0, v1)])
    }

    /// 画布坐标 → 原图像素坐标（裁剪框拖拽用的逆变换）。
    ///
    /// 是 [`Item::crop_corners`] 的逆运算：先逆变换回 item 局部空间，按 `base_size`
    /// 归一化，再映射回 [`Item::current_crop`] 坐标系。结果可能落在当前可见区域
    /// 之外（拖到图外时），由调用方 clamp。
    pub fn canvas_to_crop_pixel(&self, canvas_pos: CanvasPoint) -> Option<(f32, f32)> {
        let base = self.current_crop()?;
        let inv = self.local_to_canvas().inverse()?;
        let size = self.base_size();
        if base.width <= 0.0 || base.height <= 0.0 || size.x <= 0.0 || size.y <= 0.0 {
            return None;
        }
        let local = inv.transform_point(canvas_pos);
        Some((
            base.x + local.x / size.x * base.width,
            base.y + local.y / size.y * base.height,
        ))
    }

    /// 应用裁剪后的 transform：使裁剪区域在画布上的**位置与尺寸保持不变**。
    ///
    /// 裁剪语义（与渲染端 UV 计算一致）：item 局部空间 `[0, size]²` 经 UV 映射到
    /// 原图的 `crop` 子区域，因此局部坐标 ℓ 对应原图像素
    /// `crop.x + ℓ / size * crop.width`。由此：
    /// - 新缩放 = 旧缩放 × (`crop` / 当前可见区域)，不是除以整图尺寸——
    ///   否则对**已裁剪过的图片**二次裁剪会再缩小一次（历史 bug）
    /// - 新 `pos` = 让局部原点落在 `crop` 左上角的画布位置（含旋转/翻转）
    ///
    /// 返回的新 transform 保持 `rotation` / `flip_*` 不变。非 Pixmap 或尺寸
    /// 退化时返回 `None`。
    pub fn transform_after_crop(&self, crop: CropRect) -> Option<Transform> {
        let base = self.current_crop()?;
        let size = self.base_size();
        if base.width <= 0.0
            || base.height <= 0.0
            || size.x <= 0.0
            || size.y <= 0.0
            || crop.width <= 0.0
            || crop.height <= 0.0
        {
            return None;
        }
        let old = self.transform;
        let new_scale = CanvasVector::new(
            old.scale.x * (crop.width / base.width),
            old.scale.y * (crop.height / base.height),
        );

        // crop 左上角在局部空间的坐标（注意基准是 base，不是整图）
        let u = (crop.x - base.x) / base.width * size.x;
        let v = (crop.y - base.y) / base.height * size.y;
        let crop_tl_canvas = self
            .local_to_canvas()
            .transform_point(euclid::Point2D::<_, ItemLocalSpace>::new(u, v));

        // 新 transform 下局部原点的画布位置（scale 已变、pos 置零）
        let mut probe = self.clone();
        probe.transform.scale = new_scale;
        probe.transform.pos = CanvasVector::zero();
        let new_origin_canvas = probe
            .local_to_canvas()
            .transform_point(euclid::Point2D::<_, ItemLocalSpace>::origin());

        Some(Transform {
            pos: CanvasVector::new(
                crop_tl_canvas.x - new_origin_canvas.x,
                crop_tl_canvas.y - new_origin_canvas.y,
            ),
            scale: new_scale,
            rotation: old.rotation,
            flip_h: old.flip_h,
            flip_v: old.flip_v,
        })
    }

    /// Pixmap 的原始像素尺寸 `(w, h)`；非 Pixmap 或尺寸退化时返回 `None`。
    pub fn original_pixel_size(&self) -> Option<(f32, f32)> {
        match &self.kind {
            ItemKind::Pixmap { original_size, .. } => {
                let (w, h) = (original_size.0 as f32, original_size.1 as f32);
                if w > 0.0 && h > 0.0 {
                    Some((w, h))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// 曲线（Curved）每段的采样点数；16 足够平滑且廉价。
pub const CURVE_SAMPLES: usize = 16;

/// 单段 Catmull-Rom 插值点（标准 α=0.5 的 centripetal 近似）。
fn catmull_rom_point(
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
    t: f32,
) -> (f32, f32) {
    let t2 = t * t;
    let t3 = t2 * t;
    let x = 0.5
        * ((2.0 * p1.0)
            + (-p0.0 + p2.0) * t
            + (2.0 * p0.0 - 5.0 * p1.0 + 4.0 * p2.0 - p3.0) * t2
            + (-p0.0 + 3.0 * p1.0 - 3.0 * p2.0 + p3.0) * t3);
    let y = 0.5
        * ((2.0 * p1.1)
            + (-p0.1 + p2.1) * t
            + (2.0 * p0.1 - 5.0 * p1.1 + 4.0 * p2.1 - p3.1) * t2
            + (-p0.1 + 3.0 * p1.1 - 3.0 * p2.1 + p3.1) * t3);
    (x, y)
}

/// 圆角矩形的每个角的采样段数。8 段在常见缩放下已看不出折角。
pub const ROUNDED_CORNER_SEGMENTS: usize = 8;

/// 把控制点采样成折线。开曲线端点用 clamp（首/末点复制）；闭曲线用环绕索引。
///
/// 命中测试（[`Item::contains_canvas_point`]）与渲染（`binary` 层 `ui::stylers`）
/// 共用此函数——两者必须采出同一条曲线，否则"看起来点在曲线上"却点不中。
pub fn catmull_rom_polyline(pts: &[(f32, f32)], closed: bool, samples: usize) -> Vec<(f32, f32)> {
    let n = pts.len();
    if n < 2 || (!closed && n == 2) {
        return pts.to_vec();
    }
    let seg_count = if closed { n } else { n - 1 };
    let mut out = Vec::with_capacity(seg_count * samples + 1);
    for i in 0..seg_count {
        let p0 = if closed {
            pts[(i as isize - 1).rem_euclid(n as isize) as usize]
        } else {
            pts[i.saturating_sub(1)]
        };
        let p1 = pts[i % n];
        let p2 = pts[(i + 1) % n];
        let p3 = if closed {
            pts[(i + 2) % n]
        } else {
            pts[(i + 2).min(n - 1)]
        };
        for s in 0..samples {
            let t = s as f32 / samples as f32;
            out.push(catmull_rom_point(p0, p1, p2, p3, t));
        }
    }
    out.push(pts[if closed { 0 } else { n - 1 }]);
    out
}

/// 射线法判断点是否在多边形内（局部坐标）。
fn point_in_polygon(p: (f32, f32), poly: &[(f32, f32)]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        if (a.1 > p.1) != (b.1 > p.1) {
            let x_intercept = a.0 + (p.1 - a.1) * (b.0 - a.0) / (b.1 - a.1);
            if p.0 < x_intercept {
                inside = !inside;
            }
        }
    }
    inside
}

/// 点到线段的最短距离（局部空间，供线类命中使用）。
fn dist_point_segment(
    p: euclid::Point2D<f32, ItemLocalSpace>,
    a: (f32, f32),
    b: (f32, f32),
) -> f32 {
    let a = euclid::Point2D::<_, ItemLocalSpace>::new(a.0, a.1);
    let b = euclid::Point2D::<_, ItemLocalSpace>::new(b.0, b.1);
    let ab = b - a;
    let ap = p - a;
    let len2 = ab.x * ab.x + ab.y * ab.y;
    if len2 <= f32::EPSILON {
        return ap.length();
    }
    let t = ((ap.x * ab.x + ap.y * ab.y) / len2).clamp(0.0, 1.0);
    let closest = a + ab * t;
    (p - closest).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个画框：transform.pos = (pos_x, pos_y)，base_size = (w, h)，scale 1。
    /// AABB = (pos_x, pos_y) .. (pos_x+w, pos_y+h)。
    fn make_frame(pos_x: f32, pos_y: f32, w: f32, h: f32) -> Item {
        Item::new_frame(1, (w, h), pos_x, pos_y, None)
    }

    #[test]
    fn frame_border_hit_only_on_border_band() {
        // 画框 AABB (100,100)-(300,250)，阈值 6。
        let frame = make_frame(100.0, 100.0, 200.0, 150.0);
        let t = 6.0;

        // 内部点（曾因 dx=dy=0 被旧实现误判为边框命中，导致框内成员无法选中）。
        assert!(
            !frame.frame_border_hit(CanvasPoint::new(200.0, 175.0), t),
            "内部点不应命中边框"
        );
        assert!(
            !frame.frame_border_hit(CanvasPoint::new(290.0, 240.0), t),
            "靠近角但仍在内部的点不应命中边框"
        );

        // 左边缘上的点 -> 命中。
        assert!(
            frame.frame_border_hit(CanvasPoint::new(100.0, 175.0), t),
            "左边缘点应命中边框"
        );
        // 边缘内侧 3px（阈值内）-> 命中。
        assert!(
            frame.frame_border_hit(CanvasPoint::new(103.0, 175.0), t),
            "边缘内侧阈值内应命中边框"
        );
        // 边缘外侧 5px（阈值内）-> 命中。
        assert!(
            frame.frame_border_hit(CanvasPoint::new(95.0, 175.0), t),
            "边缘外侧阈值内应命中边框"
        );
        // 完全在外侧且超出阈值 -> 不命中。
        assert!(
            !frame.frame_border_hit(CanvasPoint::new(50.0, 175.0), t),
            "框外远点不应命中边框"
        );
    }

    fn make_line(points: Vec<(f32, f32)>, pos_x: f32, pos_y: f32) -> Item {
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for (x, y) in points.iter() {
            min_x = min_x.min(*x);
            min_y = min_y.min(*y);
            max_x = max_x.max(*x);
            max_y = max_y.max(*y);
        }
        Item::new_polyline(
            points,
            (max_x - min_x, max_y - min_y),
            None,
            Some(ArrowHeadStyle::Arrow),
            false,
            pos_x,
            pos_y,
            StrokeStyle::default(),
        )
    }

    #[test]
    fn polyline_arrow_constructors() {
        // 无箭头
        let line = Item::new_polyline(
            vec![(0.0, 0.0), (80.0, 0.0)],
            (80.0, 0.0),
            None,
            None,
            false,
            0.0,
            0.0,
            StrokeStyle::default(),
        );
        match &line.kind {
            ItemKind::Shape {
                shape_type,
                start_arrow,
                end_arrow,
                closed,
                ..
            } => {
                assert_eq!(*shape_type, ShapeType::Polyline);
                assert_eq!(*start_arrow, None);
                assert_eq!(*end_arrow, None);
                assert!(!*closed);
            }
            _ => panic!("expected Shape kind"),
        }
        // 双向箭头
        let both = Item::new_polyline(
            vec![(0.0, 0.0), (80.0, 0.0)],
            (80.0, 0.0),
            Some(ArrowHeadStyle::Arrow),
            Some(ArrowHeadStyle::Arrow),
            false,
            0.0,
            0.0,
            StrokeStyle::default(),
        );
        match &both.kind {
            ItemKind::Shape {
                start_arrow,
                end_arrow,
                closed,
                ..
            } => {
                assert_eq!(*start_arrow, Some(ArrowHeadStyle::Arrow));
                assert_eq!(*end_arrow, Some(ArrowHeadStyle::Arrow));
                assert!(!*closed);
            }
            _ => panic!("expected Shape kind"),
        }
        // 闭合构造
        let closed = Item::new_polyline(
            vec![(0.0, 0.0), (80.0, 0.0)],
            (80.0, 0.0),
            None,
            None,
            true,
            0.0,
            0.0,
            StrokeStyle::default(),
        );
        match &closed.kind {
            ItemKind::Shape { closed, .. } => assert!(*closed),
            _ => panic!("expected Shape kind"),
        }
    }

    #[test]
    fn line_base_size_is_points_aabb() {
        let item = make_line(vec![(0.0, 0.0), (80.0, 40.0)], 100.0, 100.0);
        let size = item.base_size();
        assert!((size.x - 80.0).abs() < 1e-3);
        assert!((size.y - 40.0).abs() < 1e-3);
        // 负方向也归一化到 AABB
        let item = make_line(vec![(0.0, 0.0), (-60.0, -30.0)], 0.0, 0.0);
        let size = item.base_size();
        assert!((size.x - 60.0).abs() < 1e-3);
        assert!((size.y - 30.0).abs() < 1e-3);
    }

    #[test]
    fn line_hit_uses_point_to_segment_distance() {
        // 水平线 (0,0)→(80,0)，阈值 max(线宽 2, 6) = 6
        let item = make_line(vec![(0.0, 0.0), (80.0, 0.0)], 0.0, 0.0);
        // 距线段 3 像素 → 命中
        assert!(item.contains_canvas_point(CanvasPoint::new(40.0, 3.0)));
        // 端点附近 2 像素 → 命中
        assert!(item.contains_canvas_point(CanvasPoint::new(-2.0, 0.0)));
        // 距线段 10 像素 → 未命中
        assert!(!item.contains_canvas_point(CanvasPoint::new(40.0, 10.0)));
        // 端点延长线上 100 像素 → 未命中
        assert!(!item.contains_canvas_point(CanvasPoint::new(180.0, 0.0)));
    }

    #[test]
    fn line_hit_with_rotation() {
        // 水平线旋转 90° → 变成竖线 (0,0)→(0,100)
        let mut item = make_line(vec![(0.0, 0.0), (100.0, 0.0)], 0.0, 0.0);
        item.transform.rotate_by(std::f32::consts::FRAC_PI_2);
        // 距竖线 2 像素 → 命中
        assert!(item.contains_canvas_point(CanvasPoint::new(2.0, 50.0)));
        // 距竖线 50 像素 → 未命中
        assert!(!item.contains_canvas_point(CanvasPoint::new(50.0, 50.0)));
    }

    #[test]
    fn new_shape_defaults_curve_straight_roundness_zero() {
        let s = Item::new_shape(
            ShapeType::Rectangle,
            (10.0, 10.0),
            0.0,
            0.0,
            StrokeStyle::default(),
            None,
        );
        match &s.kind {
            ItemKind::Shape {
                curve_type,
                roundness,
                ..
            } => {
                assert_eq!(*curve_type, CurveType::Straight);
                assert_eq!(*roundness, 0.0);
            }
            _ => panic!("expected Shape kind"),
        }
    }

    #[test]
    fn closed_polygon_hits_interior_and_rejects_exterior() {
        // 闭合三角形 (0,0)-(80,0)-(40,60)，内部点 (40,30) 距任一边 > 阈值，
        // 仅靠 point-in-polygon 命中；外部点不命中。
        let tri = Item::new_polyline(
            vec![(0.0, 0.0), (80.0, 0.0), (40.0, 60.0)],
            (80.0, 60.0),
            None,
            None,
            true,
            0.0,
            0.0,
            StrokeStyle::default(),
        );
        assert!(tri.contains_canvas_point(CanvasPoint::new(40.0, 30.0)));
        assert!(!tri.contains_canvas_point(CanvasPoint::new(40.0, 100.0)));
    }

    #[test]
    fn catmull_rom_open_preserves_endpoints_and_samples() {
        let pts = vec![(0.0, 0.0), (10.0, 10.0), (20.0, 0.0)];
        let out = catmull_rom_polyline(&pts, false, 8);
        assert_eq!(out.len(), (pts.len() - 1) * 8 + 1);
        assert!((out[0].0 - 0.0).abs() < 1e-3 && (out[0].1 - 0.0).abs() < 1e-3);
        let last = out[out.len() - 1];
        assert!((last.0 - 20.0).abs() < 1e-3 && (last.1 - 0.0).abs() < 1e-3);
    }

    #[test]
    fn catmull_rom_closed_wraps_and_closes() {
        let pts = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let out = catmull_rom_polyline(&pts, true, 4);
        assert_eq!(out.len(), pts.len() * 4 + 1);
        // 闭合：末点回到首点
        assert!((out[out.len() - 1].0 - 0.0).abs() < 1e-3);
    }

    #[test]
    fn line_shape_serde_roundtrip() {
        let item = make_line(vec![(0.0, 0.0), (80.0, 40.0)], 10.0, 20.0);
        let json = serde_json::to_string(&item).unwrap();
        let back: Item = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, item.id);
        match &back.kind {
            ItemKind::Shape {
                shape_type,
                base_size,
                points,
                stroke,
                start_arrow,
                end_arrow,
                closed,
                ..
            } => {
                assert_eq!(*shape_type, ShapeType::Polyline);
                assert_eq!(*base_size, (80.0, 40.0));
                assert_eq!(points, &vec![(0.0, 0.0), (80.0, 40.0)]);
                assert_eq!(stroke.color, StrokeStyle::default().color);
                assert_eq!(*start_arrow, None);
                assert_eq!(*end_arrow, Some(ArrowHeadStyle::Arrow));
                assert!(!*closed);
            }
            _ => panic!("expected Shape kind"),
        }
    }

    #[test]
    fn closed_polyline_hits_closing_segment() {
        // 三角形：闭合边为 (50,50)→(0,0)。(25,25) 位于该闭合边上。
        let mut item = Item::new_polyline(
            vec![(0.0, 0.0), (100.0, 0.0), (50.0, 50.0)],
            (100.0, 50.0),
            None,
            None,
            true,
            0.0,
            0.0,
            StrokeStyle::default(),
        );
        // 闭合时：命中闭合边
        assert!(item.contains_canvas_point(CanvasPoint::new(25.0, 25.0)));
        // 打开时：该点远离两条开放边（≥35/25 > 阈值 6）→ 未命中
        if let ItemKind::Shape { closed, .. } = &mut item.kind {
            *closed = false;
        }
        assert!(!item.contains_canvas_point(CanvasPoint::new(25.0, 25.0)));
    }

    #[test]
    fn bound_text_has_container_id() {
        let container = Item::new_shape(
            ShapeType::Rectangle,
            (100.0, 60.0),
            0.0,
            0.0,
            StrokeStyle::default(),
            None,
        );
        let txt = Item::new_text_in(
            "hello".to_string(),
            50.0,
            30.0,
            16.0,
            [255; 4],
            container.id,
        );
        match &txt.kind {
            ItemKind::Text { container_id, .. } => assert_eq!(*container_id, Some(container.id)),
            _ => panic!("expected Text kind"),
        }
        // 自由文本 container_id 应为 None
        let free = Item::new_text("x".to_string(), 0.0, 0.0, 16.0, [255; 4]);
        match &free.kind {
            ItemKind::Text { container_id, .. } => assert_eq!(*container_id, None),
            _ => panic!("expected Text kind"),
        }
        // 矩形/椭圆/菱形可作容器；未闭合多段线不可
        assert!(container.shape_can_host_text());
        for st in [ShapeType::Ellipse, ShapeType::Diamond] {
            let s = Item::new_shape(st, (10.0, 10.0), 0.0, 0.0, StrokeStyle::default(), None);
            assert!(s.shape_can_host_text());
        }
        let open = Item::new_polyline(
            vec![(0.0, 0.0), (10.0, 0.0)],
            (10.0, 0.0),
            None,
            None,
            false,
            0.0,
            0.0,
            StrokeStyle::default(),
        );
        assert!(!open.shape_can_host_text());
        let closed = Item::new_polyline(
            vec![(0.0, 0.0), (10.0, 0.0), (5.0, 5.0)],
            (10.0, 5.0),
            None,
            None,
            true,
            0.0,
            0.0,
            StrokeStyle::default(),
        );
        assert!(closed.shape_can_host_text());
    }

    #[test]
    fn text_serde_backward_compat_without_container_id() {
        // 旧版本 Text JSON 无 container_id 字段，应加载为 None（不报错）。
        let json = r#"{
            "Text": {
                "content": "abc",
                "font_size": 16.0,
                "color": [255,255,255,255],
                "editing": false,
                "measured_size": null
            }
        }"#;
        let kind: ItemKind = serde_json::from_str(json).unwrap();
        match &kind {
            ItemKind::Text { container_id, .. } => assert_eq!(*container_id, None),
            _ => panic!("expected Text kind"),
        }
        // 新版本序列化应包含 container_id，roundtrip 保真
        let txt = Item::new_text_in("abc".to_string(), 0.0, 0.0, 16.0, [255; 4], Uuid::new_v4());
        let s = serde_json::to_string(&txt).unwrap();
        let back: Item = serde_json::from_str(&s).unwrap();
        assert_eq!(back.id, txt.id);
    }

    #[test]
    fn text_background_defaults_to_transparent() {
        let free = Item::new_text("x".to_string(), 0.0, 0.0, 16.0, [255; 4]);
        assert_eq!(free.text_background(), None, "默认必须无背景");
        let bound = Item::new_text_in("x".to_string(), 0.0, 0.0, 16.0, [255; 4], Uuid::new_v4());
        assert_eq!(bound.text_background(), None);

        let mut it = free;
        it.set_text_background(Some([10, 20, 30, 255]));
        assert_eq!(it.text_background(), Some([10, 20, 30, 255]));
        it.set_text_background(None);
        assert_eq!(it.text_background(), None);
    }

    #[test]
    fn text_serde_backward_compat_without_background() {
        // 旧版本 Text JSON 无 background 字段，应加载为 None（= 透明，不报错）。
        let json = r#"{
            "Text": {
                "content": "abc",
                "font_size": 16.0,
                "color": [255,255,255,255],
                "editing": false,
                "measured_size": null,
                "container_id": null
            }
        }"#;
        let kind: ItemKind = serde_json::from_str(json).unwrap();
        match &kind {
            ItemKind::Text { background, .. } => assert_eq!(*background, None),
            _ => panic!("expected Text kind"),
        }
    }

    #[test]
    fn shape_serde_backward_compat_without_sloppiness() {
        // 旧版本 Shape JSON 无手绘风字段（Phase F 起有 bool `rough`，plan #3 改枚举），
        // 缺字段应加载为 Off（不报错）。
        let json = r#"{
            "Shape": {
                "shape_type": "Rectangle",
                "base_size": [10.0, 20.0],
                "points": [],
                "stroke": {"color": [255,255,255,255], "width": 2.0, "dash": "Solid"},
                "fill": null,
                "start_arrow": null,
                "end_arrow": null,
                "closed": false,
                "seed": 0
            }
        }"#;
        let kind: ItemKind = serde_json::from_str(json).unwrap();
        match &kind {
            ItemKind::Shape { sloppiness, .. } => assert_eq!(*sloppiness, Sloppiness::Off),
            _ => panic!("expected Shape kind"),
        }
    }

    #[test]
    fn shape_serde_legacy_rough_bool_maps_to_sloppiness() {
        // 旧存档的 `"rough": bool`（Phase F–K）应被 alias 兼容加载：
        // false → Off（观感不变）、true → Artist（保住手绘观感）；
        // 新存档为小写字符串枚举，序列化后应能回读。
        let base = r#"{
            "Shape": {
                "shape_type": "Rectangle",
                "base_size": [10.0, 20.0],
                "points": [],
                "stroke": {"color": [255,255,255,255], "width": 2.0, "dash": "Solid"},
                "fill": null,
                "start_arrow": null,
                "end_arrow": null,
                "closed": false,
                "seed": 7,
                "rough": %s
            }
        }"#;
        let kind: ItemKind = serde_json::from_str(&base.replace("%s", "true")).unwrap();
        match &kind {
            ItemKind::Shape { sloppiness, .. } => assert_eq!(*sloppiness, Sloppiness::Artist),
            _ => panic!("expected Shape kind"),
        }
        let kind: ItemKind = serde_json::from_str(&base.replace("%s", "false")).unwrap();
        match &kind {
            ItemKind::Shape { sloppiness, .. } => assert_eq!(*sloppiness, Sloppiness::Off),
            _ => panic!("expected Shape kind"),
        }

        // 新格式：小写字符串，往返一致
        let json = r#"{
            "Shape": {
                "shape_type": "Rectangle",
                "base_size": [10.0, 20.0],
                "points": [],
                "stroke": {"color": [255,255,255,255], "width": 2.0, "dash": "Solid"},
                "fill": null,
                "start_arrow": null,
                "end_arrow": null,
                "closed": false,
                "seed": 7,
                "sloppiness": "cartoonist"
            }
        }"#;
        let kind: ItemKind = serde_json::from_str(json).unwrap();
        match &kind {
            ItemKind::Shape { sloppiness, .. } => assert_eq!(*sloppiness, Sloppiness::Cartoonist),
            _ => panic!("expected Shape kind"),
        }
        let out = serde_json::to_string(&kind).unwrap();
        assert!(out.contains("\"sloppiness\":\"cartoonist\""));
        assert!(!out.contains("\"rough\""), "新存档不应再写旧字段 rough");
    }

    #[test]
    fn shape_serde_backward_compat_without_phase_i_fields() {
        // Phase I 前的 Shape JSON 缺 curve_type / closed / roundness（I4 决策：新字段全 `#[serde(default)]`），
        // 应加载为 Straight / false / 0.0 而不报错——保证旧 .prz 直接打开。
        let json = r#"{
            "Shape": {
                "shape_type": "Rectangle",
                "base_size": [10.0, 20.0],
                "points": [],
                "stroke": {"color": [255,255,255,255], "width": 2.0, "dash": "Solid"},
                "fill": null,
                "start_arrow": null,
                "end_arrow": null,
                "seed": 0
            }
        }"#;
        let kind: ItemKind = serde_json::from_str(json).unwrap();
        match &kind {
            ItemKind::Shape {
                curve_type,
                closed,
                roundness,
                ..
            } => {
                assert_eq!(*curve_type, CurveType::Straight);
                assert!(!*closed);
                assert_eq!(*roundness, 0.0);
            }
            _ => panic!("expected Shape kind"),
        }
    }

    #[test]
    fn shape_sloppiness_serde_roundtrip() {
        let item = Item::new_shape(
            ShapeType::Rectangle,
            (10.0, 10.0),
            0.0,
            0.0,
            StrokeStyle::default(),
            None,
        )
        .with_sloppiness(Sloppiness::Artist);
        assert_eq!(item.sloppiness(), Sloppiness::Artist);
        let s = serde_json::to_string(&item).unwrap();
        let back: Item = serde_json::from_str(&s).unwrap();
        assert_eq!(back.sloppiness(), Sloppiness::Artist);
        // seed 一并持久化，保证重开后抖动形状不变
        match (&item.kind, &back.kind) {
            (ItemKind::Shape { seed: a, .. }, ItemKind::Shape { seed: b, .. }) => {
                assert_eq!(*a, *b)
            }
            _ => panic!("expected Shape kind"),
        }
    }

    #[test]
    fn with_sloppiness_assigns_seed_only_when_zero() {
        let item = Item::new_shape(
            ShapeType::Rectangle,
            (10.0, 10.0),
            0.0,
            0.0,
            StrokeStyle::default(),
            None,
        );
        let seed_of = |it: &Item| match &it.kind {
            ItemKind::Shape { seed, .. } => *seed,
            _ => panic!("expected Shape kind"),
        };
        assert_eq!(seed_of(&item), 0);

        let rough = item.clone().with_sloppiness(Sloppiness::Artist);
        let seed = seed_of(&rough);
        assert_ne!(seed, 0, "开启手绘应生成非 0 种子");
        // 二次切换保留原种子：重绘时轮廓不跳变
        assert_eq!(
            seed_of(&rough.clone().with_sloppiness(Sloppiness::Artist)),
            seed
        );
        assert_eq!(
            seed_of(&rough.clone().with_sloppiness(Sloppiness::Cartoonist)),
            seed
        );

        // 切档不改 seed（换档沿用同一抖动，观感连续）
        let other = rough.clone().with_sloppiness(Sloppiness::Architect);
        assert_eq!(other.sloppiness(), Sloppiness::Architect);
        assert_eq!(seed_of(&other), seed);

        // 关闭手绘不改 seed（再次打开沿用同一抖动）
        let off = rough.with_sloppiness(Sloppiness::Off);
        assert_eq!(off.sloppiness(), Sloppiness::Off);
        assert_eq!(seed_of(&off), seed);
    }

    #[test]
    fn sloppiness_is_off_for_non_shape_items() {
        let txt = Item::new_text("x".to_string(), 0.0, 0.0, 16.0, [255; 4]);
        assert_eq!(txt.sloppiness(), Sloppiness::Off);
        let frame = Item::new_frame(1, (10.0, 10.0), 0.0, 0.0, None);
        assert_eq!(frame.sloppiness(), Sloppiness::Off);
    }

    // ── crop 几何（旋转感知） ──

    /// 200×100 的图，放在 (10, 20)，scale 1.0。
    fn pixmap(rotation: f32) -> Item {
        let mut it = Item::new_pixmap(1, None, (200, 100), 10.0, 20.0, 1.0, 1.0);
        it.transform.rotation = rotation;
        it
    }

    const FULL: CropRect = CropRect {
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 100.0,
    };

    fn assert_pts_eq(a: &[CanvasPoint], b: &[(f32, f32)]) {
        assert_eq!(a.len(), b.len());
        for (p, (x, y)) in a.iter().zip(b) {
            assert!(
                (p.x - x).abs() < 1e-3 && (p.y - y).abs() < 1e-3,
                "期望 ({x}, {y})，实际 ({}, {})",
                p.x,
                p.y
            );
        }
    }

    /// 模拟 CropItems 效果：套用 transform_after_crop 并写入 crop 字段。
    fn apply_crop(item: &Item, crop: CropRect) -> Item {
        let mut out = item.clone();
        out.transform = item
            .transform_after_crop(crop)
            .expect("应能算出新 transform");
        if let ItemKind::Pixmap { crop: c, .. } = &mut out.kind {
            *c = Some(crop);
        } else {
            panic!("apply_crop 只用于 Pixmap");
        }
        out
    }

    fn assert_quads_eq(a: &[CanvasPoint], b: &[CanvasPoint], ctx: &str) {
        for (p, q) in a.iter().zip(b.iter()) {
            assert!(
                (p.x - q.x).abs() < 1e-2 && (p.y - q.y).abs() < 1e-2,
                "{ctx}: 期望 ({}, {})，实际 ({}, {})",
                q.x,
                q.y,
                p.x,
                p.y
            );
        }
    }

    #[test]
    fn crop_corners_without_rotation_matches_item_corners() {
        let it = pixmap(0.0);
        let c = it.crop_corners(FULL).expect("Pixmap 应有 crop 角点");
        // 满幅裁剪 = item 自身；环形顺序 TL → TR → BR → BL
        assert_pts_eq(
            &c,
            &[(10.0, 20.0), (210.0, 20.0), (210.0, 120.0), (10.0, 120.0)],
        );
        assert_pts_eq(
            &it.crop_corners(FULL).unwrap(),
            &it.canvas_corners_ring()
                .iter()
                .map(|p| (p.x, p.y))
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    fn crop_corners_follows_rotation() {
        // 90° 旋转（绕局部原点，即 item 左上角）：局部 (w,0) → 画布 (0,w)
        let it = pixmap(std::f32::consts::FRAC_PI_2);
        let c = it.crop_corners(FULL).expect("应有 crop 角点");
        assert_pts_eq(
            &c,
            &[
                (10.0, 20.0),   // TL 仍在旋转中心
                (10.0, 220.0),  // TR: +200 宽 → +200 y
                (-90.0, 220.0), // BR: 再 +100 高 → -100 x
                (-90.0, 20.0),  // BL
            ],
        );
    }

    #[test]
    fn crop_corners_is_subset_of_rotated_item() {
        // 45° 旋转 + 内缩裁剪：4 角必须落在 item 四边形内部
        // （crop 从 (50,25) 起而非 (0,0)，避免角点正好压在 item 边界上触发浮点判定抖动）
        let it = pixmap(std::f32::consts::FRAC_PI_4);
        let inner = CropRect::new(50.0, 25.0, 100.0, 50.0);
        let crop = it.crop_corners(inner).unwrap();
        for p in crop {
            assert!(
                it.contains_canvas_point(p),
                "crop 角点 ({}, {}) 应落在 item 内",
                p.x,
                p.y
            );
        }
    }

    #[test]
    fn crop_pixel_roundtrip_is_inverse_of_crop_corners() {
        let it = pixmap(0.7);
        let c = CropRect::new(20.0, 10.0, 120.0, 60.0);
        let corners = it.crop_corners(c).unwrap();
        // 4 角逆变换回原图像素，应还原 crop 的 4 角
        let expect = [(20.0, 10.0), (140.0, 10.0), (140.0, 70.0), (20.0, 70.0)];
        for (p, (x, y)) in corners.iter().zip(expect) {
            let (px, py) = it.canvas_to_crop_pixel(*p).expect("应可逆");
            assert!(
                (px - x).abs() < 1e-2 && (py - y).abs() < 1e-2,
                "期望 ({x}, {y})，实际 ({px}, {py})"
            );
        }
    }

    /// 已裁剪过的图片：item.crop = Some(..)，且 transform.scale 已按裁剪比例缩小
    /// （与 `CropItems` 命令一致）。返回 (item, 当前 crop)。
    fn cropped_pixmap(rotation: f32) -> (Item, CropRect) {
        let crop = CropRect::new(50.0, 25.0, 100.0, 50.0); // 原图右下半
        let mut it = Item::new_pixmap(1, None, (200, 100), 0.0, 0.0, 1.0, 1.0);
        if let ItemKind::Pixmap { crop: c, .. } = &mut it.kind {
            *c = Some(crop);
        }
        // 与 CropItems 一致：scale *= crop / original，使显示尺寸 = crop 像素尺寸
        it.transform.scale = CanvasVector::new(100.0 / 200.0, 50.0 / 100.0);
        it.transform.rotation = rotation;
        (it, crop)
    }

    #[test]
    fn crop_corners_matches_item_when_rect_equals_current_crop() {
        // 二次进入裁剪模式、未改动裁剪框时，裁剪框应与图片本身重合
        let (it, crop) = cropped_pixmap(0.0);
        let quad = it.crop_corners(crop).unwrap();
        let item = it.canvas_corners_ring();
        for (a, b) in quad.iter().zip(item.iter()) {
            assert!(
                (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3,
                "裁剪框 ({}, {}) 应与图片角点 ({}, {}) 重合",
                a.x,
                a.y,
                b.x,
                b.y
            );
        }
    }

    #[test]
    fn crop_corners_on_cropped_item_follows_rotation() {
        // 已裁剪 + 旋转 90°：裁剪框仍与图片重合（相对关系不受旋转影响）
        let (it, crop) = cropped_pixmap(std::f32::consts::FRAC_PI_2);
        let quad = it.crop_corners(crop).unwrap();
        let item = it.canvas_corners_ring();
        for (a, b) in quad.iter().zip(item.iter()) {
            assert!(
                (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3,
                "旋转下裁剪框仍应与图片重合：({}, {}) vs ({}, {})",
                a.x,
                a.y,
                b.x,
                b.y
            );
        }
    }

    #[test]
    fn crop_pixel_roundtrip_on_cropped_item() {
        // 已裁剪 + 旋转：在图片内再选一个子区域，往返应还原
        let (it, base) = cropped_pixmap(0.6);
        let inner = CropRect::new(base.x + 20.0, base.y + 10.0, 60.0, 30.0);
        let quad = it.crop_corners(inner).unwrap();
        let expect = [
            (base.x + 20.0, base.y + 10.0),
            (base.x + 80.0, base.y + 10.0),
            (base.x + 80.0, base.y + 40.0),
            (base.x + 20.0, base.y + 40.0),
        ];
        for (p, (x, y)) in quad.iter().zip(expect) {
            let (px, py) = it.canvas_to_crop_pixel(*p).unwrap();
            assert!(
                (px - x).abs() < 1e-2 && (py - y).abs() < 1e-2,
                "期望 ({x}, {y})，实际 ({px}, {py})"
            );
        }
    }

    /// 核心不变式：应用裁剪后，item 的 4 角应正好落在裁剪前裁剪框的 4 角上
    /// ——即裁剪只是把边框收缩到该区域，不改变区域在画布上的位置/尺寸/角度。
    #[test]
    fn transform_after_crop_keeps_region_in_place_when_rotated() {
        let mut it = pixmap(0.7);
        it.transform.scale = CanvasVector::new(1.3, 1.3);
        let c = CropRect::new(30.0, 20.0, 100.0, 50.0);

        let before = it.crop_corners(c).expect("应有 crop 角点");
        let after = apply_crop(&it, c);
        assert_quads_eq(
            &after.canvas_corners_ring(),
            &before,
            "旋转下裁剪后边框应对齐裁剪框",
        );
    }

    #[test]
    fn transform_after_crop_keeps_region_in_place_when_flipped() {
        let mut it = pixmap(-0.4);
        it.transform.scale = CanvasVector::new(0.8, 1.5);
        it.transform.flip_h = true;
        it.transform.flip_v = true;
        let c = CropRect::new(20.0, 15.0, 120.0, 60.0);

        let before = it.crop_corners(c).expect("应有 crop 角点");
        let after = apply_crop(&it, c);
        assert_quads_eq(
            &after.canvas_corners_ring(),
            &before,
            "翻转下裁剪后边框应对齐裁剪框",
        );
    }

    /// 回归：二次裁剪曾以「整图尺寸」而非「当前可见区域」为基准，导致图片被
    /// 再缩小一次。两步裁剪的结果必须与一步裁到最终区域完全一致。
    #[test]
    fn repeated_crop_is_equivalent_to_single_crop() {
        let mut it = pixmap(0.7);
        it.transform.scale = CanvasVector::new(1.3, 1.3);

        let c1 = CropRect::new(20.0, 10.0, 150.0, 80.0);
        let c2 = CropRect::new(50.0, 30.0, 60.0, 40.0); // c2 ⊂ c1

        let two_step = apply_crop(&apply_crop(&it, c1), c2);
        let one_step = apply_crop(&it, c2);

        assert_quads_eq(
            &two_step.canvas_corners_ring(),
            &one_step.canvas_corners_ring(),
            "二次裁剪应与一次裁到 c2 等价",
        );
    }

    #[test]
    fn transform_after_crop_preserves_rotation_and_flip() {
        let mut it = pixmap(0.7);
        it.transform.flip_h = true;
        let t = it
            .transform_after_crop(CropRect::new(10.0, 10.0, 50.0, 50.0))
            .expect("应能算出新 transform");
        assert!((t.rotation - 0.7).abs() < 1e-6);
        assert!(t.flip_h);
        assert!(!t.flip_v);
    }

    #[test]
    fn transform_after_crop_returns_none_for_degenerate_rect() {
        let it = pixmap(0.0);
        assert!(it
            .transform_after_crop(CropRect::new(0.0, 0.0, 0.0, 10.0))
            .is_none());
        assert!(it
            .transform_after_crop(CropRect::new(0.0, 0.0, 10.0, 0.0))
            .is_none());
    }

    #[test]
    fn crop_geometry_returns_none_for_non_pixmap() {
        let txt = Item::new_text("x".to_string(), 0.0, 0.0, 16.0, [255; 4]);
        assert!(txt.crop_corners(FULL).is_none());
        assert!(txt
            .canvas_to_crop_pixel(CanvasPoint::new(1.0, 1.0))
            .is_none());
        assert!(txt.original_pixel_size().is_none());
    }

    #[test]
    fn canvas_corners_ring_is_counter_clockwise_order() {
        // 环形顺序下，相邻点连线不应交叉：用叉积符号一致性验证（无旋转的矩形）
        let it = pixmap(0.0);
        let r = it.canvas_corners_ring();
        let mut sign = 0.0f32;
        for i in 0..4 {
            let a = r[i];
            let b = r[(i + 1) % 4];
            let c = r[(i + 2) % 4];
            let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
            assert!(cross.abs() > 1e-6, "相邻边不应共线");
            if sign == 0.0 {
                sign = cross.signum();
            } else {
                assert_eq!(sign, cross.signum(), "环形顺序应保持同一绕向");
            }
        }
    }

    #[test]
    fn frame_serde_roundtrip_preserves_fields() {
        let f = Item::new_frame(7, (300.0, 200.0), 10.0, 20.0, Some("封面".to_string()));
        let s = serde_json::to_string(&f).unwrap();
        let back: Item = serde_json::from_str(&s).unwrap();
        assert_eq!(back.id, f.id);
        assert!(back.is_frame());
        assert_eq!(back.frame_number(), Some(7));
        assert!(back.bounding_rect().width() >= 300.0);
        match &back.kind {
            ItemKind::Frame {
                number,
                name,
                base_size,
            } => {
                assert_eq!(*number, 7);
                assert_eq!(name.as_deref(), Some("封面"));
                assert_eq!(*base_size, (300.0, 200.0));
            }
            _ => panic!("expected Frame kind"),
        }
    }

    #[test]
    fn frame_serde_without_name_loads_as_none() {
        // 旧/外部 JSON 未提供 name 字段 → 加载为 None（serde default）。
        let json = r#"{
            "Frame": {
                "base_size": [400.0, 300.0],
                "number": 3
            }
        }"#;
        let kind: ItemKind = serde_json::from_str(json).unwrap();
        match &kind {
            ItemKind::Frame { number, name, .. } => {
                assert_eq!(*number, 3);
                assert!(name.is_none());
            }
            _ => panic!("expected Frame kind"),
        }
    }
}
