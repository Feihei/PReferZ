use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shape::{ArrowHeadStyle, ShapeType, StrokeStyle};
use crate::spaces::{CanvasPoint, CanvasRect, CanvasVector};
use crate::transform::Transform;

pub type ItemId = Uuid;

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
    },
    Shape {
        shape_type: ShapeType,
        /// 局部空间尺寸（矩形族 = w×h；线类 = points 包围盒）。
        base_size: (f32, f32),
        /// 线性对象顶点，局部坐标（矩形族为空 Vec；N ≥ 2）。
        points: Vec<(f32, f32)>,
        stroke: StrokeStyle,
        fill: Option<[u8; 4]>, // RGBA；None = 透明
        /// 起点箭头样式（仅 Polyline 使用；矩形族忽略）。
        start_arrow: Option<ArrowHeadStyle>,
        /// 终点箭头样式（仅 Polyline 使用；矩形族忽略）。
        end_arrow: Option<ArrowHeadStyle>,
        /// 是否闭合（仅 Polyline 使用；矩形族忽略）。闭合时首尾相连，可填充。
        closed: bool,
        /// 手绘风描边抖动种子（Phase F）。同种子恒得同一抖动，保证重绘/存盘后形状不变。
        seed: u64,
        /// 手绘风描边开关（Phase F）。true 时由 RoughStyler 渲染抖动描边。
        /// `#[serde(default)]`：旧存档无此字段时按 false 加载。
        #[serde(default)]
        rough: bool,
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
            },
            transform: Transform::new(pos_x, pos_y, 1.0, 1.0),
            z: 0,
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
            },
            transform: Transform::new(pos_x, pos_y, 1.0, 1.0),
            z: 0,
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
        let inside_x = canvas_pos.x >= min.x - threshold && canvas_pos.x <= max.x + threshold;
        let inside_y = canvas_pos.y >= min.y - threshold && canvas_pos.y <= max.y + threshold;
        if !(inside_x && inside_y) {
            return false;
        }
        let dx = if canvas_pos.x < min.x {
            min.x - canvas_pos.x
        } else if canvas_pos.x > max.x {
            canvas_pos.x - max.x
        } else {
            0.0
        };
        let dy = if canvas_pos.y < min.y {
            min.y - canvas_pos.y
        } else if canvas_pos.y > max.y {
            canvas_pos.y - max.y
        } else {
            0.0
        };
        dx <= threshold && dy <= threshold
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
                start_arrow: None,
                end_arrow: None,
                closed: false,
                seed: 0,
                rough: false,
            },
            transform: Transform::new(pos_x, pos_y, 1.0, 1.0),
            z: 0,
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
                start_arrow,
                end_arrow,
                closed,
                seed: 0,
                rough: false,
            },
            transform: Transform::new(pos_x, pos_y, 1.0, 1.0),
            z: 0,
        }
    }

    /// 是否启用手绘风描边（非 Shape 恒为 false）。
    pub fn rough(&self) -> bool {
        matches!(self.kind, ItemKind::Shape { rough: true, .. })
    }

    /// builder：设置手绘风开关。开启时若 `seed` 仍为 0，则生成一个随机种子，
    /// 避免所有手绘图形抖出一模一样的轮廓。
    pub fn with_rough(mut self, rough: bool) -> Self {
        self.set_rough(rough);
        self
    }

    /// 设置手绘风开关（供 `SetRough` 命令使用），语义同 [`Item::with_rough`]。
    pub fn set_rough(&mut self, rough: bool) {
        if let ItemKind::Shape { rough: r, seed, .. } = &mut self.kind {
            *r = rough;
            if rough && *seed == 0 {
                *seed = Uuid::new_v4().as_u128() as u64;
            }
        }
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
            ..
        } = &self.kind
        {
            if matches!(shape_type, ShapeType::Polyline) {
                // 线类：点到任一线段距离 ≤ max(线宽, 6.0) 视为命中
                // （局部单位；旋转/缩放由逆变换处理）。闭合时补首尾闭合线段。
                let threshold = stroke.width.max(6.0);
                if points.len() >= 2 {
                    let open_hit = points
                        .windows(2)
                        .any(|seg| dist_point_segment(local, seg[0], seg[1]) <= threshold);
                    if open_hit {
                        return true;
                    }
                    if *closed {
                        return dist_point_segment(local, points[points.len() - 1], points[0])
                            <= threshold;
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
    fn shape_serde_backward_compat_without_rough() {
        // 旧版本 Shape JSON 无 rough 字段（Phase F 新增），应加载为 false（不报错）。
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
            ItemKind::Shape { rough, .. } => assert!(!*rough),
            _ => panic!("expected Shape kind"),
        }
    }

    #[test]
    fn shape_rough_serde_roundtrip() {
        let item = Item::new_shape(
            ShapeType::Rectangle,
            (10.0, 10.0),
            0.0,
            0.0,
            StrokeStyle::default(),
            None,
        )
        .with_rough(true);
        assert!(item.rough());
        let s = serde_json::to_string(&item).unwrap();
        let back: Item = serde_json::from_str(&s).unwrap();
        assert!(back.rough());
        // seed 一并持久化，保证重开后抖动形状不变
        match (&item.kind, &back.kind) {
            (ItemKind::Shape { seed: a, .. }, ItemKind::Shape { seed: b, .. }) => assert_eq!(a, b),
            _ => panic!("expected Shape kind"),
        }
    }

    #[test]
    fn with_rough_assigns_seed_only_when_zero() {
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

        let rough = item.clone().with_rough(true);
        let seed = seed_of(&rough);
        assert_ne!(seed, 0, "开启手绘应生成非 0 种子");
        // 二次切换保留原种子：重绘时轮廓不跳变
        assert_eq!(seed_of(&rough.clone().with_rough(true)), seed);
        assert_eq!(seed_of(&rough.clone().with_rough(false)), seed);

        // 关闭手绘不改 seed（再次打开沿用同一抖动）
        let off = rough.with_rough(false);
        assert!(!off.rough());
        assert_eq!(seed_of(&off), seed);
    }

    #[test]
    fn rough_is_false_for_non_shape_items() {
        let txt = Item::new_text("x".to_string(), 0.0, 0.0, 16.0, [255; 4]);
        assert!(!txt.rough());
        let frame = Item::new_frame(1, (10.0, 10.0), 0.0, 0.0, None);
        assert!(!frame.rough());
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
