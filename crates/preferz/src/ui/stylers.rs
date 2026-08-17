use eframe::egui::{self, Color32, Pos2, Shape};
use preferz_core::shape::{DashStyle, ShapeType, StrokeStyle};
use preferz_core::spaces::ScreenSpace;

/// 风格器输入：shape 的局部空间几何。
pub struct ShapeData {
    pub shape_type: ShapeType,
    pub base_size: (f32, f32),
    /// 线类两点（局部坐标，Phase B 启用）。
    pub points: Vec<(f32, f32)>,
}

/// 将 shape 局部几何转换为屏幕空间的 egui::Shape 列表。
/// 风格器自行把局部点经 to_screen 变换到屏幕；stroke 线宽按 zoom 缩放。
pub trait ShapeStyler {
    fn build_shapes(
        &self,
        shape: &ShapeData,
        stroke: &StrokeStyle,
        fill: Option<Color32>,
        to_screen: &euclid::Transform2D<f32, preferz_core::item::ItemLocalSpace, ScreenSpace>,
        zoom: f32,
    ) -> Vec<Shape>;
}

/// Phase 1 简洁实现：实线/虚线/圆点，epaint 原生。
pub struct CleanStyler;

impl CleanStyler {
    fn color(c: [u8; 4]) -> Color32 {
        Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
    }

    fn to_pos2(p: euclid::Point2D<f32, ScreenSpace>) -> Pos2 {
        egui::pos2(p.x, p.y)
    }

    /// 顺时针旋转屏幕向量 angle（弧度）。
    fn rotate_vec2(v: egui::Vec2, angle: f32) -> egui::Vec2 {
        let (s, c) = angle.sin_cos();
        egui::vec2(v.x * c - v.y * s, v.x * s + v.y * c)
    }

    /// 线类渲染：直线（支持虚线）与箭头（终点 ±50° 两短线段）。
    /// 线宽按 zoom 缩放；箭头头长 = 线宽 × 4。
    fn build_line_shapes(
        shape: &ShapeData,
        stroke: &StrokeStyle,
        to_screen: &euclid::Transform2D<f32, preferz_core::item::ItemLocalSpace, ScreenSpace>,
        zoom: f32,
    ) -> Vec<Shape> {
        let (base_w, base_h) = shape.base_size;
        let p0 = shape.points.first().copied().unwrap_or((0.0, 0.0));
        let p1 = shape.points.get(1).copied().unwrap_or((base_w, base_h));
        let s0 = Self::to_pos2(to_screen.transform_point(euclid::Point2D::new(p0.0, p0.1)));
        let s1 = Self::to_pos2(to_screen.transform_point(euclid::Point2D::new(p1.0, p1.1)));
        let stroke_color = Self::color(stroke.color);
        let line_width = stroke.width * zoom;
        let egui_stroke = egui::Stroke::new(line_width, stroke_color);

        let mut out = Vec::new();
        match stroke.dash {
            DashStyle::Solid => {
                out.push(Shape::line(vec![s0, s1], egui_stroke));
            }
            DashStyle::Dashed | DashStyle::Dotted => {
                let (dash_len, gap_len) = match stroke.dash {
                    DashStyle::Dotted => (1.5 * zoom, line_width * 2.0),
                    _ => (line_width * 4.0, line_width * 2.0),
                };
                out.extend(Shape::dashed_line(
                    &[s0, s1],
                    egui_stroke,
                    dash_len,
                    gap_len,
                ));
            }
        }

        if shape.shape_type == ShapeType::Arrow {
            let dir = s1 - s0;
            let len = dir.length();
            if len > 1e-3 {
                let dir = dir / len;
                let head_len = line_width * 4.0;
                let half = std::f32::consts::FRAC_PI_2 * (5.0 / 9.0); // ≈50°
                let a1 = Self::rotate_vec2(dir, half);
                let a2 = Self::rotate_vec2(dir, -half);
                out.push(Shape::line(vec![s1, s1 + a1 * head_len], egui_stroke));
                out.push(Shape::line(vec![s1, s1 + a2 * head_len], egui_stroke));
            }
        }
        out
    }
}

impl ShapeStyler for CleanStyler {
    fn build_shapes(
        &self,
        shape: &ShapeData,
        stroke: &StrokeStyle,
        fill: Option<Color32>,
        to_screen: &euclid::Transform2D<f32, preferz_core::item::ItemLocalSpace, ScreenSpace>,
        zoom: f32,
    ) -> Vec<Shape> {
        let (w, h) = shape.base_size;
        // 局部空间顶点 → 屏幕
        let mut pts: Vec<Pos2> = match shape.shape_type {
            ShapeType::Rectangle => {
                let corners = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
                corners
                    .iter()
                    .map(|(x, y)| {
                        Self::to_pos2(to_screen.transform_point(euclid::Point2D::new(*x, *y)))
                    })
                    .collect()
            }
            ShapeType::Diamond => {
                let corners = [(w / 2.0, 0.0), (w, h / 2.0), (w / 2.0, h), (0.0, h / 2.0)];
                corners
                    .iter()
                    .map(|(x, y)| {
                        Self::to_pos2(to_screen.transform_point(euclid::Point2D::new(*x, *y)))
                    })
                    .collect()
            }
            ShapeType::Ellipse => {
                let cx = w / 2.0;
                let cy = h / 2.0;
                let rx = w / 2.0;
                let ry = h / 2.0;
                let seg = 64usize;
                (0..seg)
                    .map(|i| {
                        let a = i as f32 / seg as f32 * std::f32::consts::TAU;
                        Self::to_pos2(to_screen.transform_point(euclid::Point2D::new(
                            cx + rx * a.cos(),
                            cy + ry * a.sin(),
                        )))
                    })
                    .collect()
            }
            // 线类：两点式（局部空间 points[0]→points[1]），含箭头
            ShapeType::Line | ShapeType::Arrow => {
                return Self::build_line_shapes(shape, stroke, to_screen, zoom);
            }
        };

        let stroke_color = Self::color(stroke.color);
        let line_width = stroke.width * zoom;
        let fill = fill.unwrap_or(Color32::TRANSPARENT);

        match stroke.dash {
            DashStyle::Solid => {
                let path_stroke = egui::epaint::PathStroke::new(line_width, stroke_color);
                vec![Shape::convex_polygon(pts, fill, path_stroke)]
            }
            DashStyle::Dashed | DashStyle::Dotted => {
                let mut out = vec![Shape::convex_polygon(
                    pts.clone(),
                    fill,
                    egui::epaint::PathStroke::NONE,
                )];
                // 闭合路径：首点追加到末尾
                pts.push(pts[0]);
                let (dash_len, gap_len) = match stroke.dash {
                    DashStyle::Dotted => (1.5 * zoom, line_width * 2.0),
                    _ => (line_width * 4.0, line_width * 2.0),
                };
                let egui_stroke = egui::Stroke::new(line_width, stroke_color);
                out.extend(Shape::dashed_line(&pts, egui_stroke, dash_len, gap_len));
                out
            }
        }
    }
}

/// 便捷入口：Item 局部 → 屏幕 的变换（供 render_scene 使用）。
pub fn item_local_to_screen(
    item: &preferz_core::Item,
    viewport: &crate::viewport::ViewportState,
) -> euclid::Transform2D<f32, preferz_core::item::ItemLocalSpace, ScreenSpace> {
    item.local_to_canvas()
        .then(&viewport.canvas_to_screen_transform())
}
