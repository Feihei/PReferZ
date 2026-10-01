//! 工具栏手绘图标：嵌入字体（思源黑体）无对应字形的工具用 painter 手绘，
//! 走字符 fallback 会渲染成豆腐块或与其他图标风格不一致（同 HUD 主题按钮惯例）。

use egui::epaint::{PathShape, PathStroke};
use egui::{vec2, Color32, Painter, Pos2, Rect, Shape, Stroke};

/// 工具栏按钮图标：字形字符或手绘形状。
pub enum ToolIcon {
    /// 字形字符（须在内嵌字体 cmap 中有字形，见 render_toolbar 注释）。
    Glyph(&'static str),
    /// 选择工具：鼠标光标箭头。
    SelectCursor,
    /// elbow 工具：单线九十度拐弯加箭头。
    ElbowArrow,
}

impl ToolIcon {
    /// 字形图标的按钮文案；手绘图标按钮留空（形状画在按钮 rect 上）。
    pub fn label(&self) -> &'static str {
        match self {
            ToolIcon::Glyph(g) => g,
            ToolIcon::SelectCursor | ToolIcon::ElbowArrow => "",
        }
    }
}

/// 在按钮 rect 上绘制非字形图标；字形图标由 egui Button 自行渲染。
pub fn draw(painter: &Painter, rect: Rect, icon: &ToolIcon, color: Color32) {
    match icon {
        ToolIcon::Glyph(_) => {}
        ToolIcon::SelectCursor => select_cursor(painter, rect, color),
        ToolIcon::ElbowArrow => elbow_arrow(painter, rect, color),
    }
}

/// 选择工具：经典鼠标光标箭头。轮廓的鳍是凹的，走 epaint 的 PathShape
/// （耳切三角化）填充——`Shape::convex_polygon` 只适配凸形。
fn select_cursor(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let o = Pos2::new(c.x - 4.6, c.y - 8.5);
    let pts: Vec<Pos2> = [
        (0.0, 0.0),
        (0.0, 14.0),
        (3.6, 10.8),
        (5.8, 17.0),
        (7.6, 16.1),
        (5.6, 10.0),
        (9.6, 10.0),
    ]
    .into_iter()
    .map(|(dx, dy)| o + vec2(dx, dy))
    .collect();
    painter.add(Shape::Path(PathShape {
        points: pts,
        closed: true,
        fill: color,
        stroke: PathStroke::NONE,
    }));
}

/// elbow 工具：一条正交折线（先横后竖的九十度拐弯）加终点箭头头部，
/// 端头规格与画布箭头一致（半张角 30°，头长按图标比例取 6px）。
fn elbow_arrow(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let start = Pos2::new(c.x - 8.0, c.y - 7.0);
    let corner = Pos2::new(c.x + 4.0, c.y - 7.0);
    let tip = Pos2::new(c.x + 4.0, c.y + 7.0);
    let stroke = Stroke::new(1.8, color);
    painter.line_segment([start, corner], stroke);
    painter.line_segment([corner, tip], stroke);
    let head_len = 6.0;
    let (sin, cos) = std::f32::consts::FRAC_PI_6.sin_cos();
    let b1 = tip - vec2(head_len * sin, head_len * cos);
    let b2 = tip - vec2(-head_len * sin, head_len * cos);
    painter.line_segment([tip, b1], stroke);
    painter.line_segment([tip, b2], stroke);
}
