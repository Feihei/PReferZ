//! 悬浮 bar 外壳样式（Excalidraw 风格固定位置浮层 + 圆角倒角）。
//!
//! 工具栏 / 属性栏 / HUD 共用 [`floating_bar_frame`] 构造圆角 + 柔和阴影的
//! `egui::Frame`；集中定义避免各调用点硬编码圆角 / 间距 / 阴影导致视觉不一致。

use egui::{Color32, CornerRadius, Frame, Margin, Shadow, Style};

/// 悬浮 bar 圆角半径（px）。
pub const BAR_RADIUS: u8 = 10;
/// 悬浮 bar 与窗口边缘留白（px）。
pub const BAR_MARGIN: f32 = 10.0;
/// 悬浮 bar 内边距（px）。
pub const BAR_INNER_MARGIN: i8 = 6;
/// 工具按钮尺寸（px）。
pub const TOOL_BTN_SIZE: f32 = 32.0;
/// 工具按钮间距（px）。
pub const TOOL_BTN_GAP: f32 = 4.0;
/// 属性栏固定宽度（px）。
pub const PROPS_BAR_WIDTH: f32 = 230.0;

/// 构造悬浮 bar 的 `Frame`：圆角 10 + 柔和阴影 + 主题化填充/描边。
///
/// 以 `Frame::popup(style)` 为基底（自带主题化背景 + 描边），覆盖圆角与阴影，
/// 并设置统一内边距。调用方如需不同内边距可在返回值上再链 `.inner_margin(...)`。
pub fn floating_bar_frame(style: &Style) -> Frame {
    Frame::popup(style)
        .inner_margin(Margin::same(BAR_INNER_MARGIN))
        .corner_radius(CornerRadius::same(BAR_RADIUS))
        .shadow(Shadow {
            offset: [0, 2],
            blur: 8,
            spread: 0,
            color: Color32::from_black_alpha(46),
        })
}

/// Bottom breathing room kept under the panel (px).
const PANEL_VIEW_BOTTOM_FUDGE: f32 = 8.0;

/// Scroll-view height for a full-height side panel (props / defaults).
///
/// Anchored [`egui::Area`]s are content-sized, so a `ScrollArea` inside one can
/// never exceed the area's own (previous-frame) height — with `auto_shrink(false)`
/// the view height equals the available height, content fills it exactly, and the
/// panel stays frozen at egui's `default_area_size` (400px), scrolling whenever
/// content overflows instead of filling the window. Callers must derive the view
/// height from the screen rect explicitly and pin it with `allocate_ui`.
///
/// `header_used` = height consumed by the title row(s) above the scroll view
/// (`ui.cursor().top() - ui.min_rect().top()`).
pub fn panel_view_height(screen_height: f32, header_used: f32) -> f32 {
    (screen_height
        - 2.0 * BAR_MARGIN
        - 2.0 * BAR_INNER_MARGIN as f32
        - header_used
        - PANEL_VIEW_BOTTOM_FUDGE)
        .max(64.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_view_height_fills_window_and_clamps() {
        // 1080p: 上下 BAR_MARGIN + 内边距 + 标题行 + 底部余量之外全部留给视图
        let h = panel_view_height(1080.0, 40.0);
        assert_eq!(h, 1080.0 - 20.0 - 12.0 - 40.0 - 8.0);

        // 极小窗口：钳到最小可用高度，不产生负尺寸分配
        assert_eq!(panel_view_height(80.0, 60.0), 64.0);
    }
}
