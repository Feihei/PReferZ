use eframe::egui;
use preferz_core::item::elbow_polyline_offset;
use preferz_core::shape::{CurveType, ShapeType};
use preferz_core::{Item, ItemKind};

use crate::ui::stylers::item_local_to_screen;
use crate::viewport::{ViewportEgui, ViewportState};

/// 是否显示翻转手柄（仅 Pixmap 支持；Shape 无镜像、Frame 无翻转）。
pub fn should_show_flip(item: &Item) -> bool {
    matches!(item.kind, ItemKind::Pixmap { .. })
}

/// 是否显示旋转手柄（仅 Pixmap 支持；Shape 无旋转、Frame 无旋转）。
pub fn should_show_rotate(item: &Item) -> bool {
    matches!(item.kind, ItemKind::Pixmap { .. })
}

/// 变换手柄种类。命中优先级：角点 > 旋转 > 翻转边。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    None,
    ResizeTopLeft,
    ResizeTopRight,
    ResizeBottomLeft,
    ResizeBottomRight,
    Rotate,
    /// 水平翻转手柄（左/右边中点，spec L239「翻转边」）。
    FlipH,
    /// 垂直翻转手柄（上/下边中点）。
    FlipV,
    /// 线性对象顶点控制点（Polyline，局部坐标 points 的下标；开放折线/闭合多边形共用）。
    Endpoint(usize),
    /// 线性对象段中点手柄（拖拽即在段中间插入顶点并进入端点拖拽）。
    /// `usize` 为段起始顶点在 points 中的下标：段 i 连接 points[i] → points[i+1]
    /// （闭合多边形的收尾段 i = n-1 连接 points[n-1] → points[0]）。
    SegmentMid(usize),
    /// elbow 直角折线的中间 bar 手柄（plan #16 E1）：拖拽平移中间正交段，
    /// 改 `elbow_mid_offset`（交叉轴偏移），不增删顶点。仅 `curve_type=Elbow`
    /// 且展开后 4 点（非退化/非 clamp 贴端点）时存在。
    ElbowBar,
}

/// 是否为线性对象（Polyline）：选中态用顶点控制点，而非变换边框。
fn is_line(item: &Item) -> bool {
    matches!(
        item.kind,
        ItemKind::Shape {
            shape_type: ShapeType::Polyline,
            ..
        }
    )
}

/// 是否为两点 elbow 线（plan #19：仅两点线压制段中点手柄、暴露 bar 手柄）。
/// 多顶点 elbow 放行段中点（加顶点后与相邻顶点正交连接）；bar 手柄仅两点线有意义
/// （多段居中展开不消费 `elbow_mid_offset`）。
fn is_two_point_elbow_line(item: &Item) -> bool {
    matches!(
        item.kind,
        ItemKind::Shape {
            shape_type: ShapeType::Polyline,
            curve_type: CurveType::Elbow,
            ref points,
            ..
        } if points.len() == 2
    )
}

/// elbow bar 手柄命中容差（屏幕像素）。沿中间正交段全长做带状命中，垂直容差取
/// `max(本常量, 屏幕线宽)`——细线时 8px 保证点得中，粗线时不小于线宽。
const ELBOW_BAR_HIT_PX: f32 = 8.0;

/// elbow 线展开后中间 bar 段的屏幕两端点。仅 4 点正常情形返回；退化（直线）或
/// clamp 贴端点去重为 3 点时无独立 bar → None（此时无可拖 bar）。
fn elbow_bar_screen_segment(
    item: &Item,
    viewport: &ViewportState,
) -> Option<(egui::Pos2, egui::Pos2)> {
    let ItemKind::Shape {
        points,
        curve_type,
        elbow_mid_offset,
        ..
    } = &item.kind
    else {
        return None;
    };
    if !matches!(curve_type, CurveType::Elbow) || points.len() != 2 {
        return None;
    }
    let expanded = elbow_polyline_offset(points, *elbow_mid_offset);
    if expanded.len() != 4 {
        return None;
    }
    let to_screen = item_local_to_screen(item, viewport);
    let p1 = to_screen.transform_point(euclid::Point2D::new(expanded[1].0, expanded[1].1));
    let p2 = to_screen.transform_point(euclid::Point2D::new(expanded[2].0, expanded[2].1));
    Some((egui::pos2(p1.x, p1.y), egui::pos2(p2.x, p2.y)))
}

/// 屏幕空间点到线段距离（命中容差判定用）。
fn dist_point_segment_screen(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 < 1e-6 {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

/// item 描边在屏幕空间的线宽（局部线宽 × 局部→屏幕缩放）。
fn stroke_screen_width(item: &Item, viewport: &ViewportState) -> f32 {
    let to_screen = item_local_to_screen(item, viewport);
    let scale = to_screen
        .transform_vector(
            euclid::Vector2D::<f32, preferz_core::item::ItemLocalSpace>::new(1.0, 0.0),
        )
        .length();
    let w = match &item.kind {
        ItemKind::Shape { stroke, .. } => stroke.width,
        _ => 0.0,
    };
    w * scale
}

#[derive(Debug, Clone)]
pub struct TransformHandles {
    pub active_handle: Handle,
    pub hover_handle: Handle,
    pub is_dragging: bool,
}

impl Default for TransformHandles {
    fn default() -> Self {
        Self {
            active_handle: Handle::None,
            hover_handle: Handle::None,
            is_dragging: false,
        }
    }
}

impl TransformHandles {
    pub fn new() -> Self {
        Self::default()
    }

    /// 拖拽释放后清理"拖拽中"标志，但保留 active_handle 让下一帧 hover 能继续。
    pub fn end_drag(&mut self) {
        self.is_dragging = false;
    }

    pub fn handle_size() -> f32 {
        10.0
    }

    /// 手柄在屏幕上的位置，顺序：
    /// [TL, TR, BL, BR, Rotate, top_mid, bottom_mid, left_mid, right_mid]
    /// 旋转手柄始终在视觉上方（翻转后不跑到下方）。
    fn handle_screen_positions(item: &Item, viewport: &ViewportState) -> [egui::Pos2; 9] {
        let corners = item.canvas_corners();
        let tl = viewport.canvas_to_pos2(corners[0]);
        let tr = viewport.canvas_to_pos2(corners[1]);
        let bl = viewport.canvas_to_pos2(corners[2]);
        let br = viewport.canvas_to_pos2(corners[3]);

        let top_mid = (tr + tl.to_vec2()) * 0.5;
        let bottom_mid = (br + bl.to_vec2()) * 0.5;
        let left_mid = (tl + bl.to_vec2()) * 0.5;
        let right_mid = (tr + br.to_vec2()) * 0.5;

        // 旋转手柄：始终在视觉上方（翻转后不跑到下方）。
        // 取屏幕空间 y 最小的两个角点的中点作为视觉顶边中点。
        let mut sorted = [tl, tr, bl, br];
        sorted.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap());
        let visual_top_mid = egui::pos2(
            (sorted[0].x + sorted[1].x) * 0.5,
            (sorted[0].y + sorted[1].y) * 0.5,
        );
        let rotate = visual_top_mid + egui::Vec2::new(0.0, -20.0);

        [
            tl, tr, bl, br, rotate, top_mid, bottom_mid, left_mid, right_mid,
        ]
    }

    /// 视觉顶边中点（屏幕空间 y 最小的两角点中点），用于旋转手柄连线。
    fn visual_top_mid(
        tl: egui::Pos2,
        tr: egui::Pos2,
        bl: egui::Pos2,
        br: egui::Pos2,
    ) -> egui::Pos2 {
        let mut sorted = [tl, tr, bl, br];
        sorted.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap());
        egui::pos2(
            (sorted[0].x + sorted[1].x) * 0.5,
            (sorted[0].y + sorted[1].y) * 0.5,
        )
    }

    /// 线性对象各顶点的屏幕位置（points 经局部→画布→屏幕变换）。
    fn line_endpoint_screen_positions(item: &Item, viewport: &ViewportState) -> Vec<egui::Pos2> {
        let to_screen = item_local_to_screen(item, viewport);
        let mut out = Vec::new();
        if let ItemKind::Shape { points, .. } = &item.kind {
            for (x, y) in points.iter() {
                let p = to_screen.transform_point(euclid::Point2D::new(*x, *y));
                out.push(egui::pos2(p.x, p.y));
            }
        }
        out
    }

    /// 仅更新 hover_handle。**不**改 active_handle / drag_start_*（修 B6：
    /// 旧实现 hover 时就改写 active_handle 把状态机拆散了）。active_handle
    /// 由 click 处理逻辑在按下时设置。
    pub fn update_hover(
        &mut self,
        screen_pos: egui::Pos2,
        selected_items: &[Item],
        viewport: &ViewportState,
    ) {
        let mut found = Handle::None;
        // 从顶层（最后一个）往底层查
        for item in selected_items.iter().rev() {
            let show_flip = should_show_flip(item);
            // 文字元素不需要旋转，去掉旋转手柄
            let show_rotate = should_show_rotate(item);
            let h = self.hit_test(screen_pos, item, viewport, show_flip, show_rotate);
            if h != Handle::None {
                found = h;
                break;
            }
        }
        self.hover_handle = found;
    }

    /// 屏幕点是否命中某 item 的手柄。命中后返回对应 Handle，否则 None。
    pub fn hit_test(
        &self,
        screen_pos: egui::Pos2,
        item: &Item,
        viewport: &ViewportState,
        show_flip: bool,
        show_rotate: bool,
    ) -> Handle {
        // 线类：顶点控制点优先，其次段中点（拖拽加点）
        if is_line(item) {
            let eps = Self::line_endpoint_screen_positions(item, viewport);
            let handle_size = Self::handle_size() * 2.0;
            for (i, p) in eps.iter().enumerate() {
                let r = egui::Rect::from_center_size(*p, egui::Vec2::splat(handle_size));
                if r.contains(screen_pos) {
                    return Handle::Endpoint(i);
                }
            }
            // 段中点：开放折线 n-1 段，闭合多边形含收尾段。两点 elbow 线不给加点手柄
            // （见 `is_two_point_elbow_line`：对角弦中点会破坏两点不变量）；多顶点 elbow 放行（plan #19）。
            let n = eps.len();
            if n >= 2 && !is_two_point_elbow_line(item) {
                let closed = matches!(item.kind, ItemKind::Shape { closed: true, .. });
                let seg_count = if closed { n } else { n - 1 };
                let mid_size = Self::handle_size() * 1.4;
                for i in 0..seg_count {
                    let a = eps[i];
                    let b = eps[(i + 1) % n];
                    let m = (a + b.to_vec2()) * 0.5;
                    let r = egui::Rect::from_center_size(m, egui::Vec2::splat(mid_size));
                    if r.contains(screen_pos) {
                        return Handle::SegmentMid(i);
                    }
                }
            }
            // elbow bar：沿中间正交段全长做带状命中（plan #16 E1，仅两点线）。
            if is_two_point_elbow_line(item) {
                if let Some((p1, p2)) = elbow_bar_screen_segment(item, viewport) {
                    let half = ELBOW_BAR_HIT_PX.max(stroke_screen_width(item, viewport));
                    if dist_point_segment_screen(screen_pos, p1, p2) <= half {
                        return Handle::ElbowBar;
                    }
                }
            }
            return Handle::None;
        }

        let positions = Self::handle_screen_positions(item, viewport);
        let handle_size = Self::handle_size() * 2.0;

        // 角点优先
        let corners = [
            Handle::ResizeTopLeft,
            Handle::ResizeTopRight,
            Handle::ResizeBottomLeft,
            Handle::ResizeBottomRight,
        ];
        for (i, handle) in corners.iter().enumerate() {
            let r = egui::Rect::from_center_size(positions[i], egui::Vec2::splat(handle_size));
            if r.contains(screen_pos) {
                return *handle;
            }
        }
        // 旋转手柄（仅在 show_rotate 时检测，文本元素不旋转）
        if show_rotate {
            let rotate_r =
                egui::Rect::from_center_size(positions[4], egui::Vec2::splat(handle_size));
            if rotate_r.contains(screen_pos) {
                return Handle::Rotate;
            }
        }
        // 翻转边手柄（仅在 show_flip 时检测，文本元素无翻转）
        if show_flip {
            // FlipV：上边中点 [5] / 下边中点 [6]
            for i in [5, 6] {
                let r = egui::Rect::from_center_size(positions[i], egui::Vec2::splat(handle_size));
                if r.contains(screen_pos) {
                    return Handle::FlipV;
                }
            }
            // FlipH：左边中点 [7] / 右边中点 [8]
            for i in [7, 8] {
                let r = egui::Rect::from_center_size(positions[i], egui::Vec2::splat(handle_size));
                if r.contains(screen_pos) {
                    return Handle::FlipH;
                }
            }
        }
        Handle::None
    }

    /// 渲染 item 的选中框 + 手柄。使用 item 的真实角点（旋转后正确）。
    pub fn render(
        &self,
        item: &Item,
        painter: &egui::Painter,
        viewport: &ViewportState,
        show_flip: bool,
        show_rotate: bool,
    ) {
        // 线类：顶点控制点（黄色方块）+ 段中点手柄（小号浅黄，提示可拖拽加点）
        if is_line(item) {
            let eps = Self::line_endpoint_screen_positions(item, viewport);
            let handle_size = Self::handle_size();
            let fill = egui::Color32::YELLOW;
            for p in &eps {
                let r = egui::Rect::from_center_size(*p, egui::Vec2::splat(handle_size));
                painter.rect_filled(r, egui::CornerRadius::same(1), fill);
            }
            let n = eps.len();
            if n >= 2 && !is_two_point_elbow_line(item) {
                let closed = matches!(item.kind, ItemKind::Shape { closed: true, .. });
                let seg_count = if closed { n } else { n - 1 };
                let mid_fill = egui::Color32::from_rgb(255, 225, 130);
                for i in 0..seg_count {
                    let a = eps[i];
                    let b = eps[(i + 1) % n];
                    let m = (a + b.to_vec2()) * 0.5;
                    let r = egui::Rect::from_center_size(m, egui::Vec2::splat(6.0));
                    painter.rect_filled(r, egui::CornerRadius::same(1), mid_fill);
                }
            }
            // elbow bar 手柄：bar 中点小方块（plan #16 E1，仅两点线；提示可拖拽平移走线）。
            if is_two_point_elbow_line(item) {
                if let Some((p1, p2)) = elbow_bar_screen_segment(item, viewport) {
                    let m = egui::pos2((p1.x + p2.x) * 0.5, (p1.y + p2.y) * 0.5);
                    let mid_fill = egui::Color32::from_rgb(255, 225, 130);
                    let r = egui::Rect::from_center_size(m, egui::Vec2::splat(6.0));
                    painter.rect_filled(r, egui::CornerRadius::same(1), mid_fill);
                }
            }
            return;
        }

        let positions = Self::handle_screen_positions(item, viewport);
        let [tl, tr, bl, br, rotate, top_mid, bottom_mid, left_mid, right_mid] = positions;

        // 选中框：用 4 个真实角点画 polygon
        let stroke = egui::Stroke::new(1.5_f32, egui::Color32::YELLOW);
        painter.line_segment([tl, tr], stroke);
        painter.line_segment([tr, br], stroke);
        painter.line_segment([br, bl], stroke);
        painter.line_segment([bl, tl], stroke);

        // 旋转手柄连线（从视觉顶边中点到旋转手柄），仅 Pixmap 显示
        let visual_top = Self::visual_top_mid(tl, tr, bl, br);
        if show_rotate {
            painter.line_segment([visual_top, rotate], stroke);
        }

        // 4 个角点方块（缩放）
        let handle_size = Self::handle_size();
        let fill = egui::Color32::YELLOW;
        for p in [tl, tr, bl, br] {
            let r = egui::Rect::from_center_size(p, egui::Vec2::splat(handle_size));
            painter.rect_filled(r, egui::CornerRadius::same(1), fill);
        }
        // 旋转手柄圆（仅 Pixmap 显示）
        if show_rotate {
            painter.circle_filled(rotate, handle_size / 2.0, fill);
        }

        // 翻转边手柄（青色方块 + H/V 标识，区别于缩放角点）
        if show_flip {
            let flip_fill = egui::Color32::from_rgb(80, 200, 255);
            // FlipV（上/下边中点）
            for p in [top_mid, bottom_mid] {
                let r = egui::Rect::from_center_size(p, egui::Vec2::splat(handle_size));
                painter.rect_filled(r, egui::CornerRadius::same(1), flip_fill);
                painter.text(
                    p,
                    egui::Align2::CENTER_CENTER,
                    "V",
                    egui::FontId::proportional(handle_size * 0.7),
                    egui::Color32::BLACK,
                );
            }
            // FlipH（左/右边中点）
            for p in [left_mid, right_mid] {
                let r = egui::Rect::from_center_size(p, egui::Vec2::splat(handle_size));
                painter.rect_filled(r, egui::CornerRadius::same(1), flip_fill);
                painter.text(
                    p,
                    egui::Align2::CENTER_CENTER,
                    "H",
                    egui::FontId::proportional(handle_size * 0.7),
                    egui::Color32::BLACK,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use preferz_core::shape::{ArrowHeadStyle, StrokeStyle};

    fn line(curve: CurveType, n: usize) -> Item {
        let pts: Vec<(f32, f32)> = (0..n).map(|i| (i as f32 * 10.0, 0.0)).collect();
        let bs = (((n.saturating_sub(1)) as f32 * 10.0).max(1.0), 1.0);
        let mut it = Item::new_polyline(
            pts,
            bs,
            None,
            Some(ArrowHeadStyle::Arrow),
            false,
            0.0,
            0.0,
            StrokeStyle::default(),
        );
        if let ItemKind::Shape { curve_type, .. } = &mut it.kind {
            *curve_type = curve;
        }
        it
    }

    #[test]
    fn only_two_point_elbow_suppresses_segment_mid_handle() {
        // 两点 elbow 线：压制段中点（对角弦中点会破坏两点不变量）、暴露 bar 手柄。
        let elbow = line(CurveType::Elbow, 2);
        assert!(is_line(&elbow));
        assert!(is_two_point_elbow_line(&elbow));
        // 多顶点 elbow 线（plan #19）：放行段中点（加顶点后与相邻顶点正交连接）。
        let multi = line(CurveType::Elbow, 3);
        assert!(is_line(&multi));
        assert!(!is_two_point_elbow_line(&multi));
        // 尖角 / 圆滑折线：仍保留段中点加点手柄。
        for c in [CurveType::Straight, CurveType::Curved] {
            let l = line(c, 3);
            assert!(is_line(&l));
            assert!(!is_two_point_elbow_line(&l));
        }
    }
}
