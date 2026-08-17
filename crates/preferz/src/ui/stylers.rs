use eframe::egui::{self, Color32, Pos2, Shape};
use preferz_core::shape::{ArrowHeadStyle, DashStyle, ShapeType, StrokeStyle};
use preferz_core::spaces::ScreenSpace;

/// 风格器输入：shape 的局部空间几何。
pub struct ShapeData {
    pub shape_type: ShapeType,
    pub base_size: (f32, f32),
    /// 线性对象顶点（局部坐标，N ≥ 2）。
    pub points: Vec<(f32, f32)>,
    /// 起点箭头样式（仅 Polyline 使用）。
    pub start_arrow: Option<ArrowHeadStyle>,
    /// 终点箭头样式（仅 Polyline 使用）。
    pub end_arrow: Option<ArrowHeadStyle>,
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

    /// 线性对象渲染：折线（支持虚线）与起/终点箭头。
    /// 线宽按 zoom 缩放；箭头头长 = 线宽 × 4，张角 ≈ ±50°。
    fn build_line_shapes(
        shape: &ShapeData,
        stroke: &StrokeStyle,
        start_arrow: Option<ArrowHeadStyle>,
        end_arrow: Option<ArrowHeadStyle>,
        to_screen: &euclid::Transform2D<f32, preferz_core::item::ItemLocalSpace, ScreenSpace>,
        zoom: f32,
    ) -> Vec<Shape> {
        let stroke_color = Self::color(stroke.color);
        let line_width = stroke.width * zoom;
        let egui_stroke = egui::Stroke::new(line_width, stroke_color);

        // 折线：所有顶点依次连接（N-1 段）
        let pts: Vec<Pos2> = shape
            .points
            .iter()
            .map(|(x, y)| Self::to_pos2(to_screen.transform_point(euclid::Point2D::new(*x, *y))))
            .collect();
        if pts.len() < 2 {
            return Vec::new();
        }

        let mut out = Vec::new();
        match stroke.dash {
            DashStyle::Solid => {
                out.push(Shape::line(pts.clone(), egui_stroke));
            }
            DashStyle::Dashed | DashStyle::Dotted => {
                let (dash_len, gap_len) = match stroke.dash {
                    DashStyle::Dotted => (1.5 * zoom, line_width * 2.0),
                    _ => (line_width * 4.0, line_width * 2.0),
                };
                out.extend(Shape::dashed_line(&pts, egui_stroke, dash_len, gap_len));
            }
        }

        // 起点箭头：沿首段方向反向后退（倒 V 指向起点）。
        if let Some(ArrowHeadStyle::Arrow) = start_arrow {
            let dir = pts[1] - pts[0];
            let len = dir.length();
            if len > 1e-3 {
                let dir = dir / len;
                let head_len = line_width * 4.0;
                let half = std::f32::consts::FRAC_PI_2 * (5.0 / 9.0); // ≈50°
                let a1 = Self::rotate_vec2(dir, half);
                let a2 = Self::rotate_vec2(dir, -half);
                out.push(Shape::line(
                    vec![pts[0], pts[0] - a1 * head_len],
                    egui_stroke,
                ));
                out.push(Shape::line(
                    vec![pts[0], pts[0] - a2 * head_len],
                    egui_stroke,
                ));
            }
        }

        // 终点箭头：沿末段方向正向前进（V 指向终点）。
        if let Some(ArrowHeadStyle::Arrow) = end_arrow {
            let last = pts[pts.len() - 1];
            let dir = last - pts[pts.len() - 2];
            let len = dir.length();
            if len > 1e-3 {
                let dir = dir / len;
                let head_len = line_width * 4.0;
                let half = std::f32::consts::FRAC_PI_2 * (5.0 / 9.0); // ≈50°
                let a1 = Self::rotate_vec2(dir, half);
                let a2 = Self::rotate_vec2(dir, -half);
                out.push(Shape::line(vec![last, last + a1 * head_len], egui_stroke));
                out.push(Shape::line(vec![last, last + a2 * head_len], egui_stroke));
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
            // 线性对象：折线 + 起/终点箭头
            ShapeType::Polyline => {
                return Self::build_line_shapes(
                    shape,
                    stroke,
                    shape.start_arrow,
                    shape.end_arrow,
                    to_screen,
                    zoom,
                );
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
