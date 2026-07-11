use eframe::egui;
use preferz_core::{Item, ItemKind};

use crate::viewport::ViewportState;

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
        let tl = viewport.canvas_to_screen(corners[0]);
        let tr = viewport.canvas_to_screen(corners[1]);
        let bl = viewport.canvas_to_screen(corners[2]);
        let br = viewport.canvas_to_screen(corners[3]);

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

        [tl, tr, bl, br, rotate, top_mid, bottom_mid, left_mid, right_mid]
    }

    /// 视觉顶边中点（屏幕空间 y 最小的两角点中点），用于旋转手柄连线。
    fn visual_top_mid(tl: egui::Pos2, tr: egui::Pos2, bl: egui::Pos2, br: egui::Pos2) -> egui::Pos2 {
        let mut sorted = [tl, tr, bl, br];
        sorted.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap());
        egui::pos2(
            (sorted[0].x + sorted[1].x) * 0.5,
            (sorted[0].y + sorted[1].y) * 0.5,
        )
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
            let show_flip = matches!(item.kind, ItemKind::Pixmap { .. });
            let h = self.hit_test(screen_pos, item, viewport, show_flip);
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
    ) -> Handle {
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
        // 旋转手柄
        let rotate_r = egui::Rect::from_center_size(positions[4], egui::Vec2::splat(handle_size));
        if rotate_r.contains(screen_pos) {
            return Handle::Rotate;
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
    ) {
        let positions = Self::handle_screen_positions(item, viewport);
        let [tl, tr, bl, br, rotate, top_mid, bottom_mid, left_mid, right_mid] = positions;

        // 选中框：用 4 个真实角点画 polygon
        let stroke = egui::Stroke::new(1.5, egui::Color32::YELLOW);
        painter.line_segment([tl, tr], stroke);
        painter.line_segment([tr, br], stroke);
        painter.line_segment([br, bl], stroke);
        painter.line_segment([bl, tl], stroke);

        // 旋转手柄连线（从视觉顶边中点到旋转手柄）
        let visual_top = Self::visual_top_mid(tl, tr, bl, br);
        painter.line_segment([visual_top, rotate], stroke);

        // 4 个角点方块（缩放）
        let handle_size = Self::handle_size();
        let fill = egui::Color32::YELLOW;
        for p in [tl, tr, bl, br] {
            let r = egui::Rect::from_center_size(p, egui::Vec2::splat(handle_size));
            painter.rect_filled(r, egui::Rounding::same(1.0), fill);
        }
        // 旋转手柄圆
        painter.circle_filled(rotate, handle_size / 2.0, fill);

        // 翻转边手柄（青色方块 + H/V 标识，区别于缩放角点）
        if show_flip {
            let flip_fill = egui::Color32::from_rgb(80, 200, 255);
            // FlipV（上/下边中点）
            for p in [top_mid, bottom_mid] {
                let r = egui::Rect::from_center_size(p, egui::Vec2::splat(handle_size));
                painter.rect_filled(r, egui::Rounding::same(1.0), flip_fill);
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
                painter.rect_filled(r, egui::Rounding::same(1.0), flip_fill);
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
