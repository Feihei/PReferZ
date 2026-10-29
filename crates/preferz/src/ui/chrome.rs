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

/// 线性混色：`t = 0` 得 `a`，`t = 1` 得 `b`（只混 RGB，alpha 取 `a`）。
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| ((x as f32) * (1.0 - t) + (y as f32) * t) as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

/// 悬浮控件统一 hover 底色：`window_fill` 向 `strong_text_color` 混 12%。
///
/// 浮层（工具栏 / 缩放 HUD / 侧栏开关）一律用**底色变化**表示 hover，替代
/// egui 默认的 `bg_stroke` 白边——白边在透明浮层上像多画了一圈框，观感脏。
pub fn hover_tint(ui: &egui::Ui) -> Color32 {
    mix(
        ui.visuals().window_fill,
        ui.visuals().strong_text_color(),
        0.12,
    )
}

/// 按下态底色（18%，比 hover 再深一档，保证点击有反馈）。
pub fn press_tint(ui: &egui::Ui) -> Color32 {
    mix(
        ui.visuals().window_fill,
        ui.visuals().strong_text_color(),
        0.18,
    )
}

/// 把当前 `ui` 作用域内交互控件的三态改为「底色高亮 + 无描边」，并让静止态
/// 透明（浮层专用：工具栏 / 缩放 HUD / 侧栏开关）。
///
/// 只覆盖 `inactive` / `hovered` / `active` 的填充与描边，其余视觉（文字色、
/// 圆角、expanding）沿用主题。按钮需**不显式 `fill()`** 才会透出 hover 底色
/// （`Button::fill` 会覆盖三态背景）。含 `TextEdit` 的局部请用 `ui.scope`
/// 收窄作用域，避免顺带抹掉输入框的边框。
pub fn use_tint_hover(ui: &mut egui::Ui) {
    tint_widgets(ui, true);
}

/// 同 [`use_tint_hover`]，但**保留静止态**的主题默认填充（普通面板专用）：
/// 面板里的按钮/下拉/滑块在静止时仍是有底色的控件，只把 hover / press 的
/// 白边换成底色变化，与浮层观感对齐。
pub fn use_tint_hover_keep_rest(ui: &mut egui::Ui) {
    tint_widgets(ui, false);
}

fn tint_widgets(ui: &mut egui::Ui, flat_rest: bool) {
    let hover = hover_tint(ui);
    let press = press_tint(ui);
    let w = &mut ui.style_mut().visuals.widgets;
    for v in [&mut w.inactive, &mut w.hovered, &mut w.active] {
        v.bg_stroke = egui::Stroke::NONE;
    }
    if flat_rest {
        w.inactive.bg_fill = Color32::TRANSPARENT;
        w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    }
    w.hovered.bg_fill = hover;
    w.hovered.weak_bg_fill = hover;
    w.active.bg_fill = press;
    w.active.weak_bg_fill = press;
}

/// 在已套用 [`use_tint_hover`] / [`use_tint_hover_keep_rest`] 的作用域里，为
/// `TextEdit` 局部恢复边框描边（egui 输入框的静止态本来无边框，靠 hovered /
/// active 的描边表示悬停与聚焦；统一 tint 会一并抹掉，焦点就看不出来了）。
pub fn restore_input_stroke(ui: &mut egui::Ui) {
    let v = ui.visuals();
    // 复刻各主题默认的 hovered 描边量级（dark gray150 / light gray105）：
    // 静止边框色向强文字色混 55%。
    let strong = v.strong_text_color();
    let hover = mix(v.widgets.noninteractive.bg_stroke.color, strong, 0.55);
    let w = &mut ui.style_mut().visuals.widgets;
    w.hovered.bg_stroke = egui::Stroke::new(1.0, hover);
    w.active.bg_stroke = egui::Stroke::new(1.0, strong);
}

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
