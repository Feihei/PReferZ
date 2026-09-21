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
