use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shape::{
    ArrowHeadStyle, CurveType, FillStyle, FontFamily, ShapeType, Sloppiness, StrokeStyle,
    TextAlignH, TextAlignV, TextStyle,
};
use crate::spaces::{CanvasPoint, CanvasRect, CanvasVector};
use crate::transform::Transform;

pub type ItemId = Uuid;

/// 图表默认尺寸（局部空间宽×高，plan #8）。
pub const CHART_DEFAULT_SIZE: (f32, f32) = (440.0, 300.0);
/// 图表默认系列颜色（Excalidraw 蓝色档）。
pub const CHART_DEFAULT_COLOR: [u8; 4] = [0x19, 0x71, 0xc2, 0xff];

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

/// `ItemKind::Text::follow_stroke` 的 serde 默认值：`true`。
/// 旧存档无此字段时，绑定文字默认跟随容器描边色（保持历史行为）。
fn default_follow_stroke() -> bool {
    true
}

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
        /// 文字色是否跟随容器描边色（仅绑定文字有意义）。
        /// `#[serde(default = "default_follow_stroke")]` 默认 `true`：旧存档的绑定
        /// 文字保持"跟随边框"语义（与历史"改描边即覆盖文字色"行为一致）；用户在
        /// 调色板手选文字色后置 `false`，此后改边框不再影响该文字。
        #[serde(default = "default_follow_stroke")]
        follow_stroke: bool,
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
        /// 填充色是否跟随本形状描边色（"背景跟随形状"）。
        /// `#[serde(default = "default_follow_stroke")]` 默认 `true`：改描边色时填充 RGB
        /// 跟随（保留 alpha，与历史行为一致）；用户在填充调色板手选色后置 `false`（独立）。
        #[serde(default = "default_follow_stroke")]
        fill_follow_stroke: bool,
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
        /// elbow 直角折线的中间 bar 交叉轴偏移（plan #16 E1）。仅 `curve_type=Elbow`
        /// 的两点 Polyline 使用；矩形族/其它曲线模式忽略。值为**短轴偏移**：几何层
        /// 沿垂直于两端点主导轴的方向，把中间正交段从中点平移此有符号距离（局部
        /// 坐标）；渲染/命中/导出三处经同一 `elbow_polyline_offset` 消费，clamp 保证
        /// bar 不越过任一端点。端点/绑定重算后偏移保持（用户意图），超界由 clamp
        /// 兜底。`#[serde(default)]`：旧存档无此字段按 0（居中）加载，`.prz` 零迁移。
        #[serde(default)]
        elbow_mid_offset: f32,
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
        /// 边标签（仅线性对象 Polyline 使用；矩形族忽略）。mermaid `-->|文本|` /
        /// `-- 文本 -->` 生成的连线文字，渲染期每帧画在当前曲线中点，随端点重路由
        /// 自动跟随（plan #17 DP2）。`#[serde(default)]`：旧存档无此字段按 `None` 加载。
        #[serde(default)]
        label: Option<String>,
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
        /// 是否跟随全局画框比例（设置面板）：跟随中的画框在全局比例变更时
        /// 联动重算尺寸；侧栏套用预设即解除跟随（覆盖全局）。
        /// `#[serde(default)]`：旧存档无此字段按 `false`（不联动，行为不变）。
        #[serde(default)]
        follow_global_ratio: bool,
    },
    /// 徒手绘制墨迹（plan #10）。中心线点（局部坐标，AABB 左上角为原点）+ 逐点**相对
    /// 笔宽**（pressures ∈ (0,1]，速度锥形/收笔形状）+ 一个**基准笔宽** `stroke_width`
    /// （画布单位）+ 颜色。实际每点笔宽 = `stroke_width × pressures[i]`。把「形状
    /// (pressures)」与「粗细 (stroke_width)」分离存，改粗细只动一个标量、不改形状，
    /// 且属性 undo 快照是 Copy（不必携带点级 Vec）。渲染成逐段描边的可变宽墨迹
    /// （见 `crate::freedraw` 与 binary `stylers`）。与线性对象共用「points 相对 AABB
    /// 左上角、整体位置交给 transform.pos」的约定。
    Freedraw {
        /// 墨迹中心线点，局部坐标（N ≥ 2）。
        points: Vec<(f32, f32)>,
        /// 与 `points` 一一对应的相对笔宽乘子，取值 (0,1]（慢/中段 = 1，快/收笔 < 1）。
        pressures: Vec<f32>,
        /// 基准笔宽（画布单位，慢速运笔即此宽度）。
        stroke_width: f32,
        /// 墨迹颜色 RGBA。
        color: [u8; 4],
    },
    /// 单系列图表（plan #8）：两列剪贴板数据粘贴生成。数据存 item（.prz 随
    /// kind JSON 落盘），渲染层逐段矢量绘制（binary `draw_chart_item`）。
    /// 变换框走 `base_size`（同 Frame），可移动/缩放。
    Chart {
        chart_type: ChartType,
        /// 局部空间尺寸（宽×高）。
        base_size: (f32, f32),
        /// 类别标签（与 `values` 一一对应）。
        labels: Vec<String>,
        /// 数值（单系列，支持负值：零线随数据范围浮动）。
        values: Vec<f32>,
        /// 柱/折线颜色 RGBA。
        color: [u8; 4],
        /// 折线宽（Line）/ 柱描边宽（Bar），画布单位。
        stroke_width: f32,
    },
}

/// 图表类型（plan #8）。首轮单系列柱状/折线。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChartType {
    Bar,
    Line,
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
                follow_stroke,
                ..
            } => Some(TextStyle {
                font_size: *font_size,
                color: *color,
                background: *background,
                align_h: *align_h,
                align_v: *align_v,
                font_family: *font_family,
                follow_stroke: *follow_stroke,
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
            follow_stroke,
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
            *follow_stroke = style.follow_stroke;
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
                // 自由文本无容器可跟随，恒为独立色。
                follow_stroke: false,
            },
            transform: Transform::new(pos_x, pos_y, 1.0, 1.0),
            z: 0,
            group_id: None,
        }
    }

    /// 新建单系列图表（plan #8）。位置以左上角锚定（调用方自行居中换算），
    /// 颜色/线宽取默认常量，后续如需属性面板再暴露。
    pub fn new_chart(
        chart_type: ChartType,
        labels: Vec<String>,
        values: Vec<f32>,
        pos_x: f32,
        pos_y: f32,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: ItemKind::Chart {
                chart_type,
                base_size: CHART_DEFAULT_SIZE,
                labels,
                values,
                color: CHART_DEFAULT_COLOR,
                stroke_width: 2.0,
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
                // 绑定文字默认跟随容器描边色（用户手选文字色后脱离）。
                follow_stroke: true,
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
                follow_global_ratio: true,
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

    /// 画框是否跟随全局比例（非画框返回 `false`）。
    pub fn frame_follows_global_ratio(&self) -> bool {
        match &self.kind {
            ItemKind::Frame {
                follow_global_ratio,
                ..
            } => *follow_global_ratio,
            _ => false,
        }
    }

    /// 修改画框「跟随全局比例」状态（非画框无副作用）。
    pub fn set_frame_follow_global_ratio(&mut self, follow: bool) {
        if let ItemKind::Frame {
            follow_global_ratio,
            ..
        } = &mut self.kind
        {
            *follow_global_ratio = follow;
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
                fill_follow_stroke: true,
                start_arrow: None,
                end_arrow: None,
                closed: false,
                curve_type: CurveType::Straight,
                elbow_mid_offset: 0.0,
                roundness: 0.0,
                seed: 0,
                sloppiness: Sloppiness::Off,
                start_binding: None,
                end_binding: None,
                label: None,
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
                fill_follow_stroke: true,
                start_arrow,
                end_arrow,
                closed,
                curve_type: CurveType::Straight,
                elbow_mid_offset: 0.0,
                roundness: 0.0,
                seed: 0,
                sloppiness: Sloppiness::Off,
                start_binding: None,
                end_binding: None,
                label: None,
            },
            transform: Transform::new(pos_x, pos_y, 1.0, 1.0),
            z: 0,
            group_id: None,
        }
    }

    /// builder：设置线性对象的边标签（plan #17 DP2）。非 Shape 无副作用。
    pub fn with_label(mut self, label: Option<String>) -> Self {
        if let ItemKind::Shape { label: l, .. } = &mut self.kind {
            *l = label.filter(|s| !s.is_empty());
        }
        self
    }

    /// 线性对象的边标签（非 Shape / 无标签返回 `None`）。
    pub fn label(&self) -> Option<&str> {
        match &self.kind {
            ItemKind::Shape { label, .. } => label.as_deref(),
            _ => None,
        }
    }

    /// 墨迹（Freedraw）构造器：入参为**画布坐标**中心线点 + 逐点相对笔宽 `pressures`
    /// (0,1] + 基准 `stroke_width`（画布单位）+ 颜色。内部把中心线归一化成「AABB 左上角
    /// 为原点」的局部点，整体位置交给 `transform.pos`——与线性对象/多边形创建共用同一
    /// 约定，后续移动/缩放/命中都走既有点集代码路径。
    pub fn new_freedraw(
        canvas_points: &[(f32, f32)],
        pressures: &[f32],
        stroke_width: f32,
        color: [u8; 4],
    ) -> Self {
        let (min_x, min_y) = canvas_points
            .iter()
            .fold((f32::MAX, f32::MAX), |(mx, my), &(x, y)| {
                (mx.min(x), my.min(y))
            });
        let (min_x, min_y) = (
            if min_x == f32::MAX { 0.0 } else { min_x },
            if min_y == f32::MAX { 0.0 } else { min_y },
        );
        let points: Vec<(f32, f32)> = canvas_points
            .iter()
            .map(|&(x, y)| (x - min_x, y - min_y))
            .collect();
        Self {
            id: Uuid::new_v4(),
            kind: ItemKind::Freedraw {
                points,
                pressures: pressures.to_vec(),
                stroke_width,
                color,
            },
            transform: Transform::new(min_x, min_y, 1.0, 1.0),
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
            ItemKind::Chart { base_size, .. } => {
                CanvasVector::new(base_size.0.max(1.0), base_size.1.max(1.0))
            }
            // 墨迹：base_size 取中心线点 AABB（与线性对象同约定；笔宽超出部分由
            // bounding_rect/hit-test 的 slop 容忍，变换框略小于最粗处可接受）。
            ItemKind::Freedraw { points, .. } => {
                if points.is_empty() {
                    return CanvasVector::new(1.0, 1.0);
                }
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
                CanvasVector::new((max_x - min_x).max(1.0), (max_y - min_y).max(1.0))
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
        // 墨迹：命中 = 到中心线任一段的距离 ≤ 该段半宽 + 少量容差（细笔也点得中）。
        // 半宽 = stroke_width × 相邻两点较大乘子 × 0.5；逆变换已消化旋转/缩放/翻转。
        if let ItemKind::Freedraw {
            points,
            pressures,
            stroke_width,
            ..
        } = &self.kind
        {
            if points.len() < 2 {
                // 退化：单点墨迹按点距命中。
                return points
                    .first()
                    .map(|&p| dist_point_segment(local, p, p) <= HIT_SLOP)
                    .unwrap_or(false);
            }
            let half_at = |i: usize| -> f32 {
                pressures
                    .get(i)
                    .copied()
                    .map(|pr| pr * stroke_width * 0.5)
                    .unwrap_or(0.0)
            };
            for i in 0..points.len() - 1 {
                let half = half_at(i).max(half_at(i + 1));
                if dist_point_segment(local, points[i], points[i + 1]) <= half + HIT_SLOP {
                    return true;
                }
            }
            return false;
        }
        if let ItemKind::Shape {
            shape_type,
            points,
            stroke,
            closed,
            curve_type,
            elbow_mid_offset,
            ..
        } = &self.kind
        {
            if matches!(shape_type, ShapeType::Polyline) {
                // 线类命中：点到任一（采样后）线段距离 ≤ max(线宽, 6.0)。
                // 旋转/缩放由逆变换处理；曲线先经 Catmull-Rom 采样成折线再测距。
                let threshold = stroke.width.max(6.0);
                if points.len() >= 2 {
                    let pts = match curve_type {
                        CurveType::Curved => catmull_rom_polyline(points, *closed, CURVE_SAMPLES),
                        CurveType::Elbow => {
                            if points.len() == 2 {
                                elbow_polyline_offset(points, *elbow_mid_offset)
                            } else {
                                elbow_vertex_polyline(points, *closed)
                            }
                        }
                        CurveType::Straight => points.clone(),
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

/// 墨迹命中容差（画布单位）：在逐点半宽之外额外放宽，保证细笔/收笔处也点得中。
const HIT_SLOP: f32 = 3.0;

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
    // 验收反馈 #4-1：自动闭合是"端点吸附至起点重合 + closed 标志"（保留两个端点
    // 供拖开解闭合），但闭合样条按重合两点采样会在接缝处产生零长段 → 反向环。
    // 首尾精确重合时视作**一个点**：去掉重复尾点再采样。
    if closed && pts[0] == pts[n - 1] {
        return catmull_rom_polyline(&pts[..n - 1], true, samples);
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

/// elbow 直角折线展开（居中）：`elbow_polyline_offset` 偏移为 0 的薄封装。
pub fn elbow_polyline(pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
    elbow_polyline_offset(pts, 0.0)
}

/// 两端点 → 正交（直角折线 / elbow）路径点列。命中测试与渲染共用（与
/// [`catmull_rom_polyline`] 同策——两边必须算出同一条路径，否则"看着在线上"却点不中）。
/// 把两端点展开成「短腿 → 中间正交 bar → 短腿」的 3 段正交折线，bar 沿**短轴**
/// （垂直于两端点主导轴的方向）从连线中点平移 `offset`（局部坐标，有符号）。
///
/// 启发式：先沿较小 Δ 的那条轴走一段"短腿"，再垂直转折，末段与首段平行进入另一端。
/// 对流程图绑定连线恒正确：主轴间距固定为 gap、交叉轴间距总是 ≥ 一节点宽 + gap（更大），
/// 故"较小 Δ 轴"= 主轴 = 两端朝向彼此的那条边界法线，首末段正好沿法线离开/进入
/// （右/左连线→水平先走、上/下连线→垂直先走）。对任意手动画也是一条干净的对称 Z。
///
/// `offset` 是拖 bar 的用户意图（存储原值不 clamp）：端点移动 / 绑定重算后 bar 相对
/// 位置保持；仅当 bar 越过任一端点时由 clamp 收到两端点之间，走线恒为干净 Z 形、
/// 不出回钩。`offset = 0` 退化为居中 Z（与历史 [`elbow_polyline`] 完全一致）。
///
/// 仅两点线性对象有意义（其余长度原样返回）；首/末点保持与输入一致，退化（近水平/
/// 垂直）时直接返回直线，避免零长段干扰箭头方向与命中。
pub fn elbow_polyline_offset(pts: &[(f32, f32)], offset: f32) -> Vec<(f32, f32)> {
    if pts.len() != 2 {
        return pts.to_vec();
    }
    let (a, b) = (pts[0], pts[1]);
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    const EPS: f32 = 1e-3;
    if dx.abs() < EPS || dy.abs() < EPS {
        return vec![a, b];
    }
    if dx.abs() <= dy.abs() {
        // 水平先走：bar 为垂直段（x = 中点 x + offset），clamp 在两端点 x 之间。
        let mx = bar_axis((a.0 + b.0) * 0.5, offset, a.0, b.0);
        let mut out = vec![a, (mx, a.1), (mx, b.1), b];
        out.dedup(); // clamp 贴端点时可能产生重合点，去零长段（防箭头方向 NaN）
        out
    } else {
        // 垂直先走：bar 为水平段（y = 中点 y + offset），clamp 在两端点 y 之间。
        let my = bar_axis((a.1 + b.1) * 0.5, offset, a.1, b.1);
        let mut out = vec![a, (a.0, my), (b.0, my), b];
        out.dedup();
        out
    }
}

/// 多顶点 elbow「顶点锚定 bar」正交展开（plan #21，取代 #19 的逐段居中 Z 展开）。
/// 每个中间顶点锚定一根正交 bar：bar 过顶点本身、取向**垂直于 (前邻, 后邻) 的主导轴**
/// （`|dx| <= |dy|` → 竖 bar，与 [`elbow_polyline_offset`] 的短轴优先启发式一致）；
/// 跑段在相邻 bar 之间垂直连接，端点处跑段沿轴进入。路径是 `points` 的纯函数——
/// 拖动顶点即平移整根 bar（仅垂直于 bar 的一个自由度改变走线），相邻坐标对齐时
/// 零长段被 [`Vec::dedup`] 吃掉、折线自动直化（对齐合并不需要额外簿记）。
///
/// `closed=true` 时末段连回首点保持视觉闭环（推导按开放链处理，首尾重合由 dedup 收口）。
/// 两点开放链等价 [`elbow_polyline`]（居中）；退化（<2 点）原样返回。恒有：首末点
/// 保留、每段严格横/竖。注意：顶点恒在 bar 所在直线上，但 bar 的**遍历范围**由相邻
/// 几何决定——顶点序严重"倒挂"（锚点坐标越过两侧邻居）时锚点可落在绘制段之外，
/// 此时沿 bar 轴拖动顶点走线不变（单自由度语义的极端表现），属可接受退化。
pub fn elbow_vertex_polyline(pts: &[(f32, f32)], closed: bool) -> Vec<(f32, f32)> {
    let mut chain: Vec<(f32, f32)> = pts.to_vec();
    if chain.len() < 2 {
        return chain;
    }
    if closed {
        chain.push(pts[0]);
    }
    if chain.len() == 2 {
        return elbow_polyline_offset(&chain, 0.0);
    }
    let mut out: Vec<(f32, f32)> = vec![chain[0]];
    let mut q = chain[0];
    for i in 1..chain.len() - 1 {
        let (px, py) = chain[i - 1];
        let (nx, ny) = chain[i + 1];
        let (x_i, y_i) = chain[i];
        // bar 取向：垂直于 (前邻, 后邻) 主导轴；平局按竖 bar（与两点线一致）。
        let vertical = (nx - px).abs() <= (ny - py).abs();
        // 到达 bar：垂直于 bar 进入（竖 bar → 横跑到 x=x_i；横 bar → 竖跑到 y=y_i）。
        // 与上一离开角重合时（前后 bar 垂直）推入重复点，由 dedup 收口。
        q = if vertical { (x_i, q.1) } else { (q.0, y_i) };
        out.push(q);
        // 沿 bar 继续到与下一锚点（chain[i+1]，可为中间顶点或终点）前进方向的交汇处：
        // 竖 bar → 拐角取下一锚点的 y；横 bar → 取其 x。三种下一锚点形态
        // （竖 bar/横 bar/终点）统一成立，跑段恒垂直于两根相接的 bar。
        q = if vertical { (x_i, ny) } else { (nx, y_i) };
        out.push(q);
    }
    out.push(chain[chain.len() - 1]);
    out.dedup();
    out
}

/// 多顶点 elbow 一个段的双击插入候选（plan #21 DP-A）。
#[derive(Debug, Clone, PartialEq)]
pub struct ElbowInsertCandidate {
    /// 段起始顶点在 points 中的下标（闭合线的收尾段 `seg = n-1`）。
    pub seg: usize,
    /// 通过校验的插入点（在该段路径上，插入后视觉不变）。
    pub point: (f32, f32),
    /// 该段在推导路径上覆盖的完整折线（跑段 + bar 半段）——UI 用它做**带状命中**，
    /// 双击线上任意处即插入 `point`（不必瞄准小方块）。
    pub span: Vec<(f32, f32)>,
}

/// 多顶点 elbow 的「双击插入」候选（plan #21 DP-A）：每个可插入段一条候选，
/// 插入点取该段在推导路径上覆盖的**笔直小段**（跑段 / bar 半段）上的采样点，且必须
/// 通过两条硬性校验，否则换小段 / 换采样点 / 放弃：
/// 1. **视觉不变**：插入后 [`elbow_vertex_polyline`] 的共线简化与原路径一致
///    （双击落点即线上的锚点，绝不允许"点一下整条线改道"——插入顶点会改变
///    相邻 bar 的取向判定，任意落点未必安全）；
/// 2. **可拖动**：新顶点小幅移动会改变路径（bar 取向与所在小段平行时插入点
///    才有自由度；垂直时是拖不动的惰性顶点，不暴露）。
///
/// `click`（与 `pts` 同一局部坐标系）是**采样偏好**：小段按离落点的远近排序，优先试
/// 落点在该小段上的投影（"双击哪儿就长在哪儿"），投影不安全时退回中点 / 偏侧点。
/// 传 `None` 得规范化的中点优先结果（手柄几何用，须与鼠标无关）。
///
/// 两点线 / 退化（<3 点）无候选。注意：候选随几何动态变化，取向不利的段可能
/// 整段无候选（该段无双击靶区，属可接受覆盖缺口，见 plan #21）。
pub fn elbow_insert_candidates(
    pts: &[(f32, f32)],
    closed: bool,
    click: Option<(f32, f32)>,
) -> Vec<ElbowInsertCandidate> {
    const EPS: f32 = 1e-3;
    let n = pts.len();
    if n < 3 {
        return Vec::new();
    }
    let base = orthogonal_simplify(&elbow_vertex_polyline(pts, closed));
    let mut chain: Vec<(f32, f32)> = pts.to_vec();
    if closed {
        chain.push(pts[0]);
    }
    let m = chain.len();
    let vertical = |i: usize| -> bool {
        let (px, py) = chain[i - 1];
        let (nx, ny) = chain[i + 1];
        (nx - px).abs() <= (ny - py).abs()
    };
    // 推导路径 + 段归属标注（与 elbow_vertex_polyline 同式展开）：锚点顶点把
    // 自己的 bar 一分为二——顶点前半属上一段（seg i-1）、后半属下一段（seg i），
    // 跑段离开角属当前段。标注决定候选点插入到哪个段。
    let mut annotated: Vec<((f32, f32), usize)> = vec![(chain[0], 0)];
    let mut q = chain[0];
    for i in 1..m - 1 {
        let v = vertical(i);
        let (x_i, y_i) = chain[i];
        q = if v { (x_i, q.1) } else { (q.0, y_i) };
        annotated.push((q, i - 1));
        annotated.push((chain[i], i - 1));
        annotated.push((chain[i], i));
        q = if v {
            (x_i, chain[i + 1].1)
        } else {
            (chain[i + 1].0, y_i)
        };
        annotated.push((q, i));
    }
    annotated.push((chain[m - 1], m - 2));

    let mut out = Vec::new();
    for seg in 0..m - 1 {
        let group: Vec<(f32, f32)> = annotated
            .iter()
            .filter(|(_, s)| *s == seg)
            .map(|(p, _)| *p)
            .collect();
        // 笔直小段（方向变化处切开，连续重复点先压缩）
        let mut pieces: Vec<((f32, f32), (f32, f32))> = Vec::new();
        let mut g: Vec<(f32, f32)> = Vec::new();
        for p in group {
            if g.last()
                .is_none_or(|c| (c.0 - p.0).abs() > EPS || (c.1 - p.1).abs() > EPS)
            {
                g.push(p);
            }
        }
        let dir = |a: (f32, f32), b: (f32, f32)| if (a.1 - b.1).abs() < EPS { 0u8 } else { 1 };
        let mut start = match g.first() {
            Some(&p) => p,
            None => continue,
        };
        for i in 1..g.len() {
            if i >= 2 && dir(g[i - 2], g[i - 1]) != dir(g[i - 1], g[i]) {
                pieces.push((start, g[i - 1]));
                start = g[i - 1];
            }
        }
        if let Some(&last) = g.last() {
            pieces.push((start, last));
        }
        // 采样偏好：离落点近的小段先试
        if let Some(c) = click {
            let seg_dist = |a: &(f32, f32), b: &(f32, f32)| -> f32 {
                let (ax, ay) = *a;
                let (bx, by) = *b;
                let (dx, dy) = (bx - ax, by - ay);
                let len2 = dx * dx + dy * dy;
                if len2 < 1e-6 {
                    return ((c.0 - ax).powi(2) + (c.1 - ay).powi(2)).sqrt();
                }
                let t = (((c.0 - ax) * dx + (c.1 - ay) * dy) / len2).clamp(0.0, 1.0);
                let (px, py) = (ax + dx * t, ay + dy * t);
                ((c.0 - px).powi(2) + (c.1 - py).powi(2)).sqrt()
            };
            pieces.sort_by(|p, q| {
                seg_dist(&p.0, &p.1)
                    .partial_cmp(&seg_dist(&q.0, &q.1))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        let mut chosen = None;
        for (a, b) in pieces {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            if dx.abs() < 1.0 && dy.abs() < 1.0 {
                continue; // 过短小段无可抓取的插入点
            }
            // 采样顺序：落点投影优先（"双击哪儿就长在哪儿"），再中点 / 偏侧点。
            // 投影只允许落在小段**内部** [0.15, 0.85]——点在小段端点（=拐角）外侧时
            // clamp 到 0/1 会把锚点正好摆在角上，拖起来就成了角点而非线上滑点。
            let mut fracs: Vec<f32> = Vec::new();
            if let Some(c) = click {
                let len2 = dx * dx + dy * dy;
                if len2 > 1e-6 {
                    let t = (((c.0 - a.0) * dx + (c.1 - a.1) * dy) / len2).clamp(0.15, 0.85);
                    fracs.push(t);
                }
            }
            fracs.extend([0.5f32, 0.25, 0.75]);
            for f in fracs {
                let p = (a.0 + dx * f, a.1 + dy * f);
                let mut inserted = pts.to_vec();
                inserted.insert(seg + 1, p);
                if !orthogonal_path_eq(
                    &orthogonal_simplify(&elbow_vertex_polyline(&inserted, closed)),
                    &base,
                ) {
                    continue; // 插入改道（相邻 bar 取向被翻转）：换采样点
                }
                let mut moved_h = inserted.clone();
                moved_h[seg + 1].0 += 5.0;
                let mut moved_v = inserted.clone();
                moved_v[seg + 1].1 += 5.0;
                let live = !orthogonal_path_eq(
                    &orthogonal_simplify(&elbow_vertex_polyline(&moved_h, closed)),
                    &base,
                ) || !orthogonal_path_eq(
                    &orthogonal_simplify(&elbow_vertex_polyline(&moved_v, closed)),
                    &base,
                );
                if !live {
                    break; // 惰性顶点与采样点位置无关，整段放弃
                }
                chosen = Some(p);
                break;
            }
            if chosen.is_some() {
                break;
            }
        }
        if let Some(p) = chosen {
            out.push(ElbowInsertCandidate {
                seg,
                point: p,
                span: g,
            });
        }
    }
    out
}

/// 多顶点 elbow 拖拽顶点的落点约束（plan #21）：顶点是它所锚定 bar 的锚点，只有
/// **垂直于 bar** 的分量被推导消费（=平移整根 bar），沿 bar 轴的分量不落地，放任它
/// 手柄会被拖到走线之外、看着像"锚点离线漂浮"。这里把沿轴分量钳进该 bar 的实际绘制
/// 区间 `[entry, exit]`——即锚点只能沿自己的 bar 滑动、不许离线。
///
/// `probe` = 未约束的目标位置。不约束的情形原样返回：两点线 / 退化（<3 点）、
/// 无 bar 的顶点（开放线首末点、闭合线的接缝点 0——它们本身就是路径端点，恒在线上）。
pub fn clamp_elbow_vertex_drag(
    pts: &[(f32, f32)],
    closed: bool,
    idx: usize,
    probe: (f32, f32),
) -> (f32, f32) {
    let n = pts.len();
    // 无 bar 的顶点：开放线首末点；闭合线只有接缝点 0（chain 里它同时是首末点，
    // 其余点包括 n-1 都是 chain 的中间顶点，各有一根 bar）。
    let seam_like = if closed {
        idx == 0
    } else {
        idx == 0 || idx == n - 1
    };
    if n < 3 || seam_like {
        return probe;
    }
    let mut chain: Vec<(f32, f32)> = pts.to_vec();
    if closed {
        chain.push(pts[0]);
    }
    chain[idx] = probe;
    // bar 取向（与 elbow_vertex_polyline 同式：垂直于邻居对主导轴，平局取竖 bar）
    let vertical_at = |i: usize| -> bool {
        let (px, py) = chain[i - 1];
        let (nx, ny) = chain[i + 1];
        (nx - px).abs() <= (ny - py).abs()
    };
    // 走一遍上游拿进入本 bar 前的当前点 q。推导里「离开角」= (x_i, 下一邻的轴坐标)
    // 不依赖上游，故只需逐点推进它（进入角由它即时决定，不必保留）。
    let mut q = chain[0];
    for i in 1..idx {
        let v = vertical_at(i);
        let (x_i, y_i) = chain[i];
        let (nx, ny) = chain[i + 1];
        q = if v { (x_i, ny) } else { (nx, y_i) };
    }
    let (nx, ny) = chain[idx + 1];
    let (entry, exit) = if vertical_at(idx) {
        ((probe.0, q.1), (probe.0, ny))
    } else {
        ((q.0, probe.1), (nx, probe.1))
    };
    if vertical_at(idx) {
        (
            probe.0,
            probe.1.clamp(entry.1.min(exit.1), entry.1.max(exit.1)),
        )
    } else {
        (
            probe.0.clamp(entry.0.min(exit.0), entry.0.max(exit.0)),
            probe.1,
        )
    }
}

/// 共线简化：移除位于水平/垂直直线中间的冗余点（插入 no-op 校验用——推导会在
/// 笔直 run 上推入共线的中间点，视觉等价但点列不同）。
fn orthogonal_simplify(pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
    const EPS: f32 = 1e-3;
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    for w in pts.windows(3) {
        let x_line = (w[0].0 - w[1].0).abs() < EPS && (w[1].0 - w[2].0).abs() < EPS;
        let y_line = (w[0].1 - w[1].1).abs() < EPS && (w[1].1 - w[2].1).abs() < EPS;
        if !x_line && !y_line {
            out.push(w[1]);
        }
    }
    out.push(pts[pts.len() - 1]);
    out
}

/// 正交点列的容差相等（逐点比较，容差 1e-3）。
fn orthogonal_path_eq(a: &[(f32, f32)], b: &[(f32, f32)]) -> bool {
    const EPS: f32 = 1e-3;
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(p, r)| (p.0 - r.0).abs() < EPS && (p.1 - r.1).abs() < EPS)
}

/// bar 轴坐标 = 中点 + 偏移，clamp 在两端点（`p`/`q`）之间——bar 恒落在端点内侧，
/// 折线不出回钩。端点重算后偏移存原值、超界部分由这里兜底。
fn bar_axis(mid: f32, offset: f32, p: f32, q: f32) -> f32 {
    (mid + offset).clamp(p.min(q), p.max(q))
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

/// 把画框几何调整为目标宽高比 `ratio = (w, h)`（侧栏比例预设与全局比例联动共用）。
/// 保持**有效长边**长度不变、另一边按比例缩放；以画框中心为锚点重算左上角，
/// scale 归一到 1、尺寸写进 base_size，使结果确定且后续手柄缩放从干净状态开始。
pub fn frame_geom_for_ratio(
    g: crate::commands::FrameGeom,
    ratio: (f32, f32),
) -> crate::commands::FrameGeom {
    let (ew, eh) = (g.base.0 * g.scale.0, g.base.1 * g.scale.1);
    let (cx, cy) = (g.pos.0 + ew / 2.0, g.pos.1 + eh / 2.0);
    let (rw, rh) = ratio;
    let long = ew.max(eh);
    let (nw, nh) = if rw >= rh {
        (long, long * rh / rw)
    } else {
        (long * rw / rh, long)
    };
    crate::commands::FrameGeom {
        pos: (cx - nw / 2.0, cy - nh / 2.0),
        base: (nw, nh),
        scale: (1.0, 1.0),
    }
}

/// 拖拽创建画框时的比例钳制：长边跟拖拽主方向，另一边按 `ratio = (w, h)` 锁定。
/// 保留拖拽方向（起点到终点可指向任意象限，含反向拖动）。
pub fn constrain_drag_to_ratio(
    start: CanvasPoint,
    current: CanvasPoint,
    ratio: (f32, f32),
) -> CanvasPoint {
    let (rw, rh) = ratio;
    let dx = current.x - start.x;
    let dy = current.y - start.y;
    let (dx, dy) = if dx.abs() >= dy.abs() {
        (dx, dx * rh / rw)
    } else {
        (dy * rw / rh, dy)
    };
    CanvasPoint::new(start.x + dx, start.y + dy)
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
    fn elbow_steps_orthogonally_choosing_smaller_delta_axis() {
        // dx<dy → 水平先走（首末段水平）；拐点在 x 中点。
        let out = elbow_polyline(&[(0.0, 0.0), (40.0, 100.0)]);
        assert_eq!(
            out,
            vec![(0.0, 0.0), (20.0, 0.0), (20.0, 100.0), (40.0, 100.0)]
        );
        // dx>dy → 垂直先走（首末段垂直）；拐点在 y 中点。
        let out = elbow_polyline(&[(0.0, 0.0), (100.0, 40.0)]);
        assert_eq!(
            out,
            vec![(0.0, 0.0), (0.0, 20.0), (100.0, 20.0), (100.0, 40.0)]
        );
    }

    #[test]
    fn elbow_is_axis_aligned_and_endpoint_preserving() {
        let pts = [(130.0, 50.0), (230.0, 230.0)]; // 流程图右向分叉的实际两端
        let out = elbow_polyline(&pts);
        assert_eq!(out.first().unwrap(), &pts[0]);
        assert_eq!(out.last().unwrap(), &pts[1]);
        // 每一段都必须是水平或垂直（正交折线）。
        for w in out.windows(2) {
            let axis_aligned = (w[0].0 - w[1].0).abs() < 1e-3 || (w[0].1 - w[1].1).abs() < 1e-3;
            assert!(axis_aligned, "段 {:?}→{:?} 非正交", w[0], w[1]);
        }
    }

    #[test]
    fn elbow_aligned_or_non_two_point_falls_back() {
        // 近水平/近垂直 → 直接直线两点。
        assert_eq!(
            elbow_polyline(&[(0.0, 0.0), (100.0, 0.0)]),
            vec![(0.0, 0.0), (100.0, 0.0)]
        );
        // 非两点（多点/退化）→ 原样返回（不做正交化）。
        let tri = vec![(0.0, 0.0), (10.0, 5.0), (20.0, 0.0)];
        assert_eq!(elbow_polyline(&tri), tri);
    }

    #[test]
    fn elbow_offset_shifts_bar_on_short_axis() {
        // dx>dy → 垂直先走、bar 为水平段 y = 中点 + offset；offset=0 与居中版一致。
        let pts = [(0.0, 0.0), (100.0, 40.0)];
        assert_eq!(elbow_polyline_offset(&pts, 0.0), elbow_polyline(&pts));
        assert_eq!(
            elbow_polyline_offset(&pts, 10.0),
            vec![(0.0, 0.0), (0.0, 30.0), (100.0, 30.0), (100.0, 40.0)]
        );
        // dx<dy → bar 为垂直段 x = 中点 + offset，负偏移向 x-。
        let pts = [(0.0, 0.0), (40.0, 100.0)];
        assert_eq!(
            elbow_polyline_offset(&pts, -8.0),
            vec![(0.0, 0.0), (12.0, 0.0), (12.0, 100.0), (40.0, 100.0)]
        );
    }

    #[test]
    fn elbow_offset_clamped_between_endpoints_no_hook() {
        // 偏移存原值；几何层把 bar 收在两端点之间（不出回钩），贴端点时去零长段。
        let pts = [(0.0, 0.0), (100.0, 40.0)];
        // 越界 +500 → bar 贴 b.y=40，末段与端点重合被去掉（3 点）。
        assert_eq!(
            elbow_polyline_offset(&pts, 500.0),
            vec![(0.0, 0.0), (0.0, 40.0), (100.0, 40.0)]
        );
        // 越界 -500 → bar 贴 a.y=0。
        assert_eq!(
            elbow_polyline_offset(&pts, -500.0),
            vec![(0.0, 0.0), (100.0, 0.0), (100.0, 40.0)]
        );
        // 仍保持正交 + 端点不变量。
        let out = elbow_polyline_offset(&pts, 500.0);
        assert_eq!(out.first().unwrap(), &pts[0]);
        assert_eq!(out.last().unwrap(), &pts[1]);
    }

    #[test]
    fn elbow_mid_offset_serde_roundtrip_and_legacy_default() {
        let mut item = make_line(vec![(0.0, 0.0), (80.0, 40.0)], 10.0, 20.0);
        if let ItemKind::Shape {
            curve_type,
            elbow_mid_offset,
            ..
        } = &mut item.kind
        {
            *curve_type = CurveType::Elbow;
            *elbow_mid_offset = 30.0;
        }
        let json = serde_json::to_string(&item).unwrap();
        let back: Item = serde_json::from_str(&json).unwrap();
        match &back.kind {
            ItemKind::Shape {
                elbow_mid_offset, ..
            } => assert!((*elbow_mid_offset - 30.0).abs() < 1e-6),
            _ => panic!("kind 往返变形"),
        }
        // 旧存档兼容：kind JSON 无该字段 → 按 0（居中）加载，`.prz` 零迁移。
        // ItemKind 是 externally-tagged enum（`{"Shape": {...}}`），删字段走 Shape 子键。
        let mut v: serde_json::Value = serde_json::to_value(&item).unwrap();
        v["kind"]["Shape"]
            .as_object_mut()
            .unwrap()
            .remove("elbow_mid_offset");
        let legacy: Item = serde_json::from_value(v).unwrap();
        match &legacy.kind {
            ItemKind::Shape {
                elbow_mid_offset, ..
            } => assert_eq!(*elbow_mid_offset, 0.0),
            _ => panic!("kind 往返变形"),
        }
    }

    #[test]
    fn elbow_offset_affects_contains_canvas_point() {
        // bar 平移后命中跟随（与渲染同源）：pos=(10,20)、scale=1 → 画布 = 局部 + pos。
        let mut item = make_line(vec![(0.0, 0.0), (100.0, 40.0)], 10.0, 20.0);
        if let ItemKind::Shape {
            curve_type,
            elbow_mid_offset,
            ..
        } = &mut item.kind
        {
            *curve_type = CurveType::Elbow;
            *elbow_mid_offset = 10.0; // 局部 bar y=30 → 画布 y=50
        }
        // offset 后 bar 上：局部 (50,30) → 画布 (60,50)，距 0 ≤ 阈值。
        assert!(item.contains_canvas_point(CanvasPoint::new(60.0, 50.0)));
        // 居中 bar 位置（画布 y=40）：距偏移后 bar 10 > 阈值 6，不命中。
        assert!(!item.contains_canvas_point(CanvasPoint::new(60.0, 50.0 - 10.0)));
        // 对照：无偏移 item 命中居中 bar（画布 y=40）。
        let plain = make_line(vec![(0.0, 0.0), (100.0, 40.0)], 10.0, 20.0);
        assert!(plain.contains_canvas_point(CanvasPoint::new(60.0, 40.0)));
        assert!(!plain.contains_canvas_point(CanvasPoint::new(60.0, 50.0)));
    }

    #[test]
    fn elbow_vertex_degenerate_returns_as_is() {
        assert_eq!(elbow_vertex_polyline(&[], false), vec![] as Vec<(f32, f32)>);
        let single = vec![(5.0, 7.0)];
        assert_eq!(elbow_vertex_polyline(&single, false), single);
        assert_eq!(elbow_vertex_polyline(&single, true), single);
    }

    #[test]
    fn elbow_vertex_two_points_equals_centered_elbow_polyline() {
        let pts = [(0.0, 0.0), (40.0, 100.0)];
        // 开放两点：等价居中 elbow_polyline。
        assert_eq!(elbow_vertex_polyline(&pts, false), elbow_polyline(&pts));
        // 闭合两点：末段连回首点（首尾重合、正交往返），不等于开放两点。
        let closed = elbow_vertex_polyline(&pts, true);
        assert_eq!(closed.first().unwrap(), &pts[0]);
        assert_eq!(closed.last().unwrap(), &pts[0]);
        assert!(closed.len() >= 3);
    }

    #[test]
    fn elbow_vertex_v_shape_bridge_midpoint() {
        // V 形穿点：P1(50,40) 锚定横 bar（邻居对 (0,0)-(100,0) 水平主导），
        // 路径成"桥"，P1 恰为 bar 中点；3 顶点只出 3 段（对比逐段展开的 7 段）。
        let pts = [(0.0, 0.0), (50.0, 40.0), (100.0, 0.0)];
        let out = elbow_vertex_polyline(&pts, false);
        assert_eq!(
            out,
            vec![(0.0, 0.0), (0.0, 40.0), (100.0, 40.0), (100.0, 0.0)]
        );
    }

    #[test]
    fn elbow_vertex_stair_vertical_bar() {
        // 台阶：对角平局按竖 bar（与两点线短轴优先一致），P1(50,50) 恰为 bar 中点。
        let pts = [(0.0, 0.0), (50.0, 50.0), (100.0, 100.0)];
        let out = elbow_vertex_polyline(&pts, false);
        assert_eq!(
            out,
            vec![(0.0, 0.0), (50.0, 0.0), (50.0, 100.0), (100.0, 100.0)]
        );
    }

    #[test]
    fn elbow_vertex_aligned_vertices_auto_straighten() {
        // 对齐自动合并：顶点拖到与两端共线 → 零长段被 dedup 吃掉，退化成直线。
        let pts = [(0.0, 0.0), (50.0, 0.0), (100.0, 0.0)];
        assert_eq!(
            elbow_vertex_polyline(&pts, false),
            vec![(0.0, 0.0), (100.0, 0.0)]
        );
    }

    #[test]
    fn elbow_vertex_preserves_vertices_and_is_orthogonal() {
        // 顶点保留 + 每段正交 + 首末点不动（混合横竖 bar 的 4 顶点链）。
        let pts = [(0.0, 0.0), (50.0, 20.0), (60.0, 40.0), (100.0, 0.0)];
        let out = elbow_vertex_polyline(&pts, false);
        assert_eq!(out.first().unwrap(), &pts[0]);
        assert_eq!(out.last().unwrap(), &pts[3]);
        for w in out.windows(2) {
            let axis_aligned = (w[0].0 - w[1].0).abs() < 1e-3 || (w[0].1 - w[1].1).abs() < 1e-3;
            assert!(axis_aligned, "段 {:?}→{:?} 非正交", w[0], w[1]);
        }
        // P1(50,20) 在横 bar y=20 上、P2(60,40) 是拐角，顶点均落在路径上。
        assert!(out.contains(&pts[2]));
        let on_path = |p: (f32, f32)| {
            out.windows(2).any(|w| {
                (w[0].0 <= p.0 && p.0 <= w[1].0 || w[1].0 <= p.0 && p.0 <= w[0].0)
                    && (w[0].1 <= p.1 && p.1 <= w[1].1 || w[1].1 <= p.1 && p.1 <= w[0].1)
                    && ((w[0].0 - w[1].0).abs() < 1e-3 && (w[0].0 - p.0).abs() < 1e-3
                        || (w[0].1 - w[1].1).abs() < 1e-3 && (w[0].1 - p.1).abs() < 1e-3)
            })
        };
        assert!(on_path(pts[1]), "P1 {:?} 不在路径上", pts[1]);
        assert_eq!(
            out,
            vec![
                (0.0, 0.0),
                (0.0, 20.0),
                (60.0, 20.0),
                (60.0, 40.0),
                (100.0, 40.0),
                (100.0, 0.0)
            ]
        );
    }

    #[test]
    fn elbow_vertex_closed_adds_closing_segment_and_dedups() {
        // 闭合三角形：末段连回首点（视觉闭环，首尾重合保留），全段正交。
        let pts = [(0.0, 0.0), (40.0, 100.0), (80.0, 0.0)];
        let out = elbow_vertex_polyline(&pts, true);
        assert_eq!(out.first().unwrap(), &pts[0]);
        assert_eq!(out.last().unwrap(), &pts[0]);
        assert_eq!(
            out,
            vec![
                (0.0, 0.0),
                (0.0, 100.0),
                (80.0, 100.0),
                (80.0, 0.0),
                (0.0, 0.0)
            ]
        );
        for w in out.windows(2) {
            let axis_aligned = (w[0].0 - w[1].0).abs() < 1e-3 || (w[0].1 - w[1].1).abs() < 1e-3;
            assert!(axis_aligned, "段 {:?}→{:?} 非正交", w[0], w[1]);
        }
    }

    #[test]
    fn elbow_vertex_collinear_segment_passes_through() {
        // 含共轴段：(0,0)→(40,0) 与 (40,0)→(40,80) 均退化直线，路径无新增转折。
        let pts = [(0.0, 0.0), (40.0, 0.0), (40.0, 80.0)];
        let out = elbow_vertex_polyline(&pts, false);
        assert_eq!(out, vec![(0.0, 0.0), (40.0, 0.0), (40.0, 80.0)]);
    }

    #[test]
    fn elbow_insert_candidates_two_points_and_degenerate_empty() {
        // 两点线走 bar 手柄、退化无意义：候选恒空。
        assert!(elbow_insert_candidates(&[], false, None).is_empty());
        assert!(elbow_insert_candidates(&[(1.0, 2.0)], false, None).is_empty());
        assert!(elbow_insert_candidates(&[(0.0, 0.0), (40.0, 80.0)], false, None).is_empty());
        assert!(elbow_insert_candidates(&[(0.0, 0.0), (40.0, 80.0)], true, None).is_empty());
    }

    #[test]
    fn elbow_insert_candidates_v_shape_on_path_and_live() {
        // V 形 [(0,0),(50,100),(100,0)]：base = [(0,0),(0,100),(100,100),(100,0)]。
        // seg0 首个小段（竖跑段）中点 (0,50)、seg1 末个小段（竖跑段）中点 (100,50)
        // 均通过「视觉不变 + 可拖动」校验。
        let pts = [(0.0, 0.0), (50.0, 100.0), (100.0, 0.0)];
        let base = elbow_vertex_polyline(&pts, false);
        let cands = elbow_insert_candidates(&pts, false, None);
        let got: Vec<(usize, (f32, f32))> = cands.iter().map(|c| (c.seg, c.point)).collect();
        assert_eq!(got, vec![(0usize, (0.0, 50.0)), (1, (100.0, 50.0))]);
        // 带状命中靶区 = 该段覆盖的完整推导路径（seg0：竖跑段 + bar 前半）
        assert_eq!(cands[0].span, vec![(0.0, 0.0), (0.0, 100.0), (50.0, 100.0)]);
        // 插入任一候选后路径不变（no-op），拖动后路径改变（live）——直接复核校验语义
        for c in &cands {
            let mut inserted = pts.to_vec();
            inserted.insert(c.seg + 1, c.point);
            assert_eq!(
                orthogonal_simplify(&elbow_vertex_polyline(&inserted, false)),
                orthogonal_simplify(&base),
                "插入 {:?} 后路径改变",
                c.point
            );
            let mut moved = inserted.clone();
            moved[c.seg + 1].0 += 5.0;
            let mut moved_v = inserted.clone();
            moved_v[c.seg + 1].1 += 5.0;
            assert!(
                elbow_vertex_polyline(&moved, false) != base
                    || elbow_vertex_polyline(&moved_v, false) != base,
                "候选 {:?} 惰性",
                c.point
            );
        }
    }

    fn on_orthogonal_path(path: &[(f32, f32)], p: (f32, f32)) -> bool {
        const EPS: f32 = 1e-3;
        path.windows(2).any(|w| {
            let (a, b) = (w[0], w[1]);
            let (ux, uy) = (b.0 - a.0, b.1 - a.1);
            let len = (ux * ux + uy * uy).sqrt();
            let within = p.0 >= a.0.min(b.0) - EPS
                && p.0 <= a.0.max(b.0) + EPS
                && p.1 >= a.1.min(b.1) - EPS
                && p.1 <= a.1.max(b.1) + EPS;
            if len < EPS {
                return within && ((p.0 - a.0).abs() < EPS && (p.1 - a.1).abs() < EPS);
            }
            let cross = (ux * (p.1 - a.1) - uy * (p.0 - a.0)).abs() / len;
            cross < EPS && within
        })
    }

    #[test]
    fn clamp_elbow_vertex_drag_keeps_anchor_on_path() {
        // ∩ 形 [(0,0),(100,40),(200,0)]：顶点 1 锚横 bar（y=40，x∈[0,200]）。
        // 沿 bar 轴（x）拖出区间应钳回区间端；垂直轴（y）自由 = 平移整根 bar。
        let pts = [(0.0, 0.0), (100.0, 40.0), (200.0, 0.0)];
        assert_eq!(
            clamp_elbow_vertex_drag(&pts, false, 1, (300.0, 40.0)),
            (200.0, 40.0)
        );
        assert_eq!(
            clamp_elbow_vertex_drag(&pts, false, 1, (-50.0, 40.0)),
            (0.0, 40.0)
        );
        assert_eq!(
            clamp_elbow_vertex_drag(&pts, false, 1, (100.0, 120.0)),
            (100.0, 120.0),
            "垂直于 bar 的分量不该被动"
        );
        // 不约束的情形：原样返回
        assert_eq!(
            clamp_elbow_vertex_drag(&pts, false, 0, (999.0, -1.0)),
            (999.0, -1.0),
            "端点无 bar"
        );
        let two = [(0.0, 0.0), (40.0, 80.0)];
        assert_eq!(
            clamp_elbow_vertex_drag(&two, false, 1, (7.0, 9.0)),
            (7.0, 9.0)
        );
        // 性质：任意探针经约束后，锚点必落在重推导的走线上
        for probe in [
            (300.0, 40.0),
            (-20.0, 200.0),
            (60.0, -30.0),
            (150.0, 150.0),
            (100.0, 40.0),
        ] {
            let fixed = clamp_elbow_vertex_drag(&pts, false, 1, probe);
            let mut next = pts.to_vec();
            next[1] = fixed;
            assert!(
                on_orthogonal_path(&elbow_vertex_polyline(&next, false), fixed),
                "探针 {:?} → 约束后 {:?} 仍离线",
                probe,
                fixed
            );
        }
    }

    #[test]
    fn clamp_elbow_vertex_drag_property_over_multi_vertex_shapes() {
        // 多组几何 × 多方向探针：约束后的锚点恒在重推导路径上（含闭合三角形）
        let shapes: Vec<(Vec<(f32, f32)>, bool)> = vec![
            (vec![(0.0, 0.0), (50.0, 100.0), (100.0, 0.0)], false),
            (
                vec![(0.0, 0.0), (200.0, 50.0), (40.0, 100.0), (120.0, 180.0)],
                false,
            ),
            (vec![(0.0, 0.0), (100.0, 0.0), (50.0, 80.0)], true),
        ];
        for (pts, closed) in shapes {
            let n = pts.len();
            for idx in 0..n {
                for dx in [-220.0f32, -70.0, 0.0, 95.0, 260.0] {
                    for dy in [-140.0f32, -35.0, 0.0, 60.0, 190.0] {
                        let probe = (pts[idx].0 + dx, pts[idx].1 + dy);
                        let fixed = clamp_elbow_vertex_drag(&pts, closed, idx, probe);
                        let mut next = pts.clone();
                        next[idx] = fixed;
                        assert!(
                            on_orthogonal_path(&elbow_vertex_polyline(&next, closed), fixed),
                            "pts={:?} closed={} idx={} probe={:?} → fixed={:?} 离线",
                            pts,
                            closed,
                            idx,
                            probe,
                            fixed
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn elbow_insert_candidates_click_beyond_corner_lands_inside_segment() {
        // 点在小段端点（拐角）外侧：投影须被限制在小段内部，锚点不得正好落在角点上
        // （否则新锚点表现成"角点拖动"，丢掉 bar 语义）。
        let pts = [(0.0, 0.0), (100.0, 40.0), (200.0, 0.0)];
        let base = elbow_vertex_polyline(&pts, false);
        let cands = elbow_insert_candidates(&pts, false, Some((260.0, 40.0)));
        assert!(!cands.is_empty(), "应有候选");
        for c in &cands {
            let corners = base.windows(2).map(|w| w[0]).collect::<Vec<_>>();
            assert!(
                !corners.contains(&c.point),
                "候选 {:?} 正好落在拐角上",
                c.point
            );
            assert!(
                on_orthogonal_path(&base, c.point),
                "候选 {:?} 不在走线上",
                c.point
            );
        }
    }

    #[test]
    fn elbow_insert_candidates_click_prefers_nearby_sample() {
        // 落点偏好：双击哪儿就长在哪儿——同一小段上，投影点若安全则优先于中点。
        let pts = [(0.0, 0.0), (50.0, 100.0), (100.0, 0.0)];
        let base = elbow_vertex_polyline(&pts, false);
        let plain = elbow_insert_candidates(&pts, false, None);
        assert_eq!(plain[0].point, (0.0, 50.0), "无落点时取中点");
        let near_top = elbow_insert_candidates(&pts, false, Some((30.0, 80.0)));
        assert_eq!(near_top[0].seg, 0);
        assert!(
            near_top[0].point.1 > 60.0,
            "落点靠上时应贴着落点采样，实际 {:?}",
            near_top[0].point
        );
        assert_eq!(near_top[0].point, (0.0, 80.0));
        // 无论取哪个采样点，插入后路径都视觉不变
        let mut inserted = pts.to_vec();
        inserted.insert(1, near_top[0].point);
        assert_eq!(
            orthogonal_simplify(&elbow_vertex_polyline(&inserted, false)),
            orthogonal_simplify(&base)
        );
    }

    #[test]
    fn elbow_insert_candidates_skips_rerouting_pieces() {
        // 回钩形 [(0,0),(200,50),(40,100)]：seg1 笔直小段的中点 (120,100) 插入会
        // 翻转 seg0 bar 取向导致改道——被校验淘汰、换采样点，0.75 处 (80,100)
        // 相邻 bar 取向不变、通过校验；seg0 取水平跑段中点 (100,0)。
        let pts = [(0.0, 0.0), (200.0, 50.0), (40.0, 100.0)];
        let cands = elbow_insert_candidates(&pts, false, None);
        let got: Vec<(usize, (f32, f32))> = cands.iter().map(|c| (c.seg, c.point)).collect();
        assert_eq!(got, vec![(0usize, (100.0, 0.0)), (1, (80.0, 100.0))]);
        // 复核两个候选均为 no-op
        for c in &cands {
            let mut inserted = pts.to_vec();
            inserted.insert(c.seg + 1, c.point);
            assert_eq!(
                orthogonal_simplify(&elbow_vertex_polyline(&inserted, false)),
                orthogonal_simplify(&elbow_vertex_polyline(&pts, false))
            );
        }
    }

    #[test]
    fn elbow_insert_candidates_closed_triangle() {
        // 闭合三角形 [(0,0),(100,0),(50,80)]：seg0 水平跑段中点 (50,0) 插入
        // 后路径不变（顶点保留、视觉闭环）。
        let pts = [(0.0, 0.0), (100.0, 0.0), (50.0, 80.0)];
        let cands = elbow_insert_candidates(&pts, true, None);
        let c0 = cands
            .iter()
            .find(|c| c.seg == 0)
            .unwrap_or_else(|| panic!("实际候选：{:?}", cands));
        assert_eq!(c0.point, (50.0, 0.0));
        // 插入候选后仍为闭环（首尾重合）且路径不变
        let mut inserted = pts.to_vec();
        inserted.insert(1, c0.point);
        assert_eq!(
            orthogonal_simplify(&elbow_vertex_polyline(&inserted, true)),
            orthogonal_simplify(&elbow_vertex_polyline(&pts, true))
        );
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
    fn catmull_rom_closed_merged_endpoint_samples_as_single_point() {
        // 验收反馈 #4-1：自动闭合的"合并点"（首尾精确重合）必须视作一个点采样，
        // 否则闭合侧零长段产生反向环（圆滑模式接缝处的反向曲线）。
        let tri = vec![(0.0, 0.0), (50.0, 0.0), (0.0, 50.0)];
        let merged = vec![(0.0, 0.0), (50.0, 0.0), (0.0, 50.0), (0.0, 0.0)];
        assert_eq!(
            catmull_rom_polyline(&merged, true, 8),
            catmull_rom_polyline(&tri, true, 8),
            "首尾重合与不重合的闭合采样必须一致"
        );
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
    fn text_follow_stroke_defaults_by_binding() {
        // 绑定文字默认跟随边框；自由文字恒独立。
        let bound = Item::new_text_in("x".to_string(), 0.0, 0.0, 16.0, [255; 4], Uuid::new_v4());
        assert!(
            bound.kind.text_style().unwrap().follow_stroke,
            "绑定文字默认跟随"
        );
        let free = Item::new_text("x".to_string(), 0.0, 0.0, 16.0, [255; 4]);
        assert!(
            !free.kind.text_style().unwrap().follow_stroke,
            "自由文字不跟随"
        );
        // set_text_style 应把 follow_stroke 写回（undo 依赖整份快照还原）
        let mut it = bound;
        let mut st = it.kind.text_style().unwrap();
        st.follow_stroke = false;
        it.set_text_style(st);
        assert!(!it.kind.text_style().unwrap().follow_stroke);
    }

    #[test]
    fn text_serde_backward_compat_follow_stroke_true() {
        // 旧存档 Text JSON 无 follow_stroke 字段 → 默认 true（保持历史"改边框覆盖文字色"语义）。
        let json = r#"{
            "Text": {
                "content": "abc",
                "font_size": 16.0,
                "color": [255,255,255,255],
                "editing": false,
                "measured_size": null,
                "container_id": "6b1f0c2e-0000-0000-0000-000000000000"
            }
        }"#;
        let kind: ItemKind = serde_json::from_str(json).unwrap();
        match &kind {
            ItemKind::Text { follow_stroke, .. } => {
                assert!(*follow_stroke, "旧存档绑定文字默认跟随")
            }
            _ => panic!("expected Text kind"),
        }
    }

    #[test]
    fn shape_fill_follow_stroke_defaults_true_for_legacy() {
        // 新建形状：填充默认跟随描边。
        let it = Item::new_shape(
            ShapeType::Rectangle,
            (10.0, 10.0),
            0.0,
            0.0,
            StrokeStyle::default(),
            Some([1, 2, 3, 4]),
        );
        assert!(
            matches!(
                it.kind,
                ItemKind::Shape {
                    fill_follow_stroke: true,
                    ..
                }
            ),
            "新建形状填充默认跟随"
        );
        // 旧存档 Shape JSON 无 fill_follow_stroke 字段 → 默认 true（保持历史"改边框覆盖填充"语义）。
        let mut v = serde_json::to_value(&it.kind).unwrap();
        v["Shape"]
            .as_object_mut()
            .unwrap()
            .remove("fill_follow_stroke");
        let kind: ItemKind = serde_json::from_value(v).unwrap();
        match kind {
            ItemKind::Shape {
                fill_follow_stroke, ..
            } => assert!(fill_follow_stroke, "旧存档填充默认跟随"),
            _ => panic!("expected Shape kind"),
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
                follow_global_ratio,
            } => {
                assert_eq!(*number, 7);
                assert_eq!(name.as_deref(), Some("封面"));
                assert_eq!(*base_size, (300.0, 200.0));
                // new_frame 构造的画框默认跟随全局比例，roundtrip 后保持
                assert!(*follow_global_ratio);
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

    #[test]
    fn frame_serde_without_follow_global_ratio_loads_as_false() {
        // 旧存档无 follow_global_ratio 字段 → 按 false 加载（不联动全局，行为不变）。
        let json = r#"{
            "Frame": {
                "base_size": [400.0, 300.0],
                "number": 3
            }
        }"#;
        let kind: ItemKind = serde_json::from_str(json).unwrap();
        match &kind {
            ItemKind::Frame {
                follow_global_ratio,
                ..
            } => assert!(!follow_global_ratio),
            _ => panic!("expected Frame kind"),
        }
    }

    #[test]
    fn frame_geom_for_ratio_keeps_long_edge_and_center() {
        use crate::commands::FrameGeom;
        // 200×100（横长）套 16:9：长边 200 不变，短边 = 200*9/16 = 112.5
        let g = FrameGeom {
            pos: (0.0, 0.0),
            base: (200.0, 100.0),
            scale: (1.0, 1.0),
        };
        let n = frame_geom_for_ratio(g, (16.0, 9.0));
        assert!((n.base.0 - 200.0).abs() < 1e-4);
        assert!((n.base.1 - 112.5).abs() < 1e-4);
        // 中心锚定：原中心 (100, 50) 不动
        let cx = n.pos.0 + n.base.0 / 2.0;
        let cy = n.pos.1 + n.base.1 / 2.0;
        assert!((cx - 100.0).abs() < 1e-4 && (cy - 50.0).abs() < 1e-4);
        // scale 归一
        assert_eq!(n.scale, (1.0, 1.0));
        // scale≠1 的旧几何按有效尺寸（base × scale）参与计算
        let g2 = FrameGeom {
            pos: (0.0, 0.0),
            base: (100.0, 100.0),
            scale: (2.0, 2.0), // 有效 200×200
        };
        let n2 = frame_geom_for_ratio(g2, (1.0, 1.0));
        assert!((n2.base.0 - 200.0).abs() < 1e-4 && (n2.base.1 - 200.0).abs() < 1e-4);
    }

    #[test]
    fn constrain_drag_to_ratio_follows_dominant_axis() {
        // 横向主导：终点 x 保持，y 按比例重算（16:9 → h = w*9/16）
        let start = CanvasPoint::new(0.0, 0.0);
        let c = constrain_drag_to_ratio(start, CanvasPoint::new(160.0, 50.0), (16.0, 9.0));
        assert!((c.x - 160.0).abs() < 1e-4);
        assert!((c.y - 90.0).abs() < 1e-4);
        // 纵向主导：终点 y 保持，x 按比例重算
        let c2 = constrain_drag_to_ratio(start, CanvasPoint::new(30.0, 90.0), (16.0, 9.0));
        assert!((c2.x - 160.0).abs() < 1e-4);
        assert!((c2.y - 90.0).abs() < 1e-4);
        // 反向拖（起点向左上）方向保留
        let c3 = constrain_drag_to_ratio(start, CanvasPoint::new(-160.0, -20.0), (16.0, 9.0));
        assert!((c3.x + 160.0).abs() < 1e-4);
        assert!((c3.y + 90.0).abs() < 1e-4);
    }

    #[test]
    fn frame_follow_global_ratio_helpers() {
        let mut f = Item::new_frame(1, (100.0, 100.0), 0.0, 0.0, None);
        assert!(f.frame_follows_global_ratio());
        f.set_frame_follow_global_ratio(false);
        assert!(!f.frame_follows_global_ratio());
        // 非画框无副作用
        let mut s = Item::new_shape(
            crate::shape::ShapeType::Rectangle,
            (50.0, 50.0),
            0.0,
            0.0,
            crate::shape::StrokeStyle::default(),
            None,
        );
        s.set_frame_follow_global_ratio(true);
        assert!(!s.frame_follows_global_ratio());
    }

    // ── 墨迹（Freedraw，plan #10）──

    fn make_freedraw() -> Item {
        // 画布点：一条从 (100,50) 向右下走的短斜线，基准宽 4、恒压（等宽）。
        let canvas = vec![(100.0, 50.0), (140.0, 50.0), (160.0, 90.0)];
        Item::new_freedraw(&canvas, &[1.0, 1.0, 1.0], 4.0, [10, 20, 30, 255])
    }

    #[test]
    fn freedraw_normalizes_points_to_aabb_origin() {
        let it = make_freedraw();
        // transform.pos 落在点集 AABB 左上角 (100,50)。
        assert!((it.transform.pos.x - 100.0).abs() < 1e-3);
        assert!((it.transform.pos.y - 50.0).abs() < 1e-3);
        match &it.kind {
            ItemKind::Freedraw { points, .. } => {
                assert_eq!(points[0], (0.0, 0.0));
                assert_eq!(points[1], (40.0, 0.0));
            }
            _ => panic!("expected Freedraw kind"),
        }
        // base_size = 点集 AABB 宽高 (60,40)。
        let size = it.base_size();
        assert!((size.x - 60.0).abs() < 1e-3);
        assert!((size.y - 40.0).abs() < 1e-3);
    }

    #[test]
    fn freedraw_hit_uses_variable_half_width() {
        let it = make_freedraw();
        // 第一段中心线中点上方 1px（半宽 2 + 容差 3 = 5 阈值内）→ 命中。
        assert!(it.contains_canvas_point(CanvasPoint::new(120.0, 51.0)));
        // 远离所有段（右上方 40px）→ 不命中。
        assert!(!it.contains_canvas_point(CanvasPoint::new(140.0, 5.0)));
    }

    #[test]
    fn freedraw_serde_roundtrip() {
        let it = make_freedraw();
        let s = serde_json::to_string(&it).unwrap();
        assert!(s.contains("\"Freedraw\""));
        let back: Item = serde_json::from_str(&s).unwrap();
        assert_eq!(back.id, it.id);
        match (&it.kind, &back.kind) {
            (
                ItemKind::Freedraw {
                    points: pa,
                    pressures: wa,
                    stroke_width: sa,
                    color: ca,
                },
                ItemKind::Freedraw {
                    points: pb,
                    pressures: wb,
                    stroke_width: sb,
                    color: cb,
                },
            ) => {
                assert_eq!(pa, pb);
                assert_eq!(wa, wb);
                assert_eq!(*sa, *sb);
                assert_eq!(ca, cb);
            }
            _ => panic!("expected Freedraw on both"),
        }
    }
}
