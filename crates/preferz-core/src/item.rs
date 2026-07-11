use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
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

    pub fn new_text(content: String, pos_x: f32, pos_y: f32, font_size: f32, color: [u8; 4]) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: ItemKind::Text {
                content,
                font_size,
                color,
                editing: false,
                measured_size: None,
            },
            transform: Transform::new(pos_x, pos_y, 1.0, 1.0),
            z: 0,
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
            ItemKind::Text { content, font_size, measured_size, .. } => {
                // 优先用 UI 层 egui 实际测量的尺寸（修 B6：边框与渲染内容一致）
                if let Some((w, h)) = measured_size {
                    return CanvasVector::new(w.max(1.0), h.max(1.0));
                }
                // 估算文本宽度：CJK 字符约 1.0 * font_size，ASCII 约 0.6 * font_size。
                let width: f32 = content.chars().map(|c| {
                    if c.is_ascii() && !c.is_ascii_control() { 0.6 } else { 1.0 }
                }).sum::<f32>() * font_size;
                let height = *font_size * 1.2;
                CanvasVector::new(width.max(1.0), height.max(1.0))
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

    /// 画布点是否落在 item 内（OBB 命中，正确处理旋转/翻转/缩放）。
    pub fn contains_canvas_point(&self, canvas_pos: CanvasPoint) -> bool {
        let inv = match self.local_to_canvas().inverse() {
            Some(inv) => inv,
            None => return false,
        };
        let local = inv.transform_point(canvas_pos);
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
