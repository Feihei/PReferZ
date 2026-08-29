use eframe::egui::{self, Color32, Pos2, Shape};
use preferz_core::item::{ItemKind, ItemLocalSpace};
use preferz_core::shape::{ArrowHeadStyle, DashStyle, SeededRng, ShapeType, StrokeStyle};
use preferz_core::spaces::ScreenSpace;

/// Item 局部 → 屏幕 的变换矩阵类型（与 core 的 `ItemLocalToScreen` 等价）。
type LocalToScreen = euclid::Transform2D<f32, ItemLocalSpace, ScreenSpace>;

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
    /// 是否闭合（仅 Polyline 使用）。闭合时首尾相连，可填充，不显示箭头。
    pub closed: bool,
    /// 手绘风抖动种子（Phase F）。同一 seed 恒得同一抖动；`CleanStyler` 忽略此字段。
    pub seed: u64,
}

/// 将 shape 局部几何转换为屏幕空间的 egui::Shape 列表。
/// 风格器自行把局部点经 to_screen 变换到屏幕；stroke 线宽按 zoom 缩放。
pub trait ShapeStyler {
    fn build_shapes(
        &self,
        shape: &ShapeData,
        stroke: &StrokeStyle,
        fill: Option<Color32>,
        to_screen: &LocalToScreen,
        zoom: f32,
    ) -> Vec<Shape>;
}

// ─────────────────────────── 共用工具 ───────────────────────────

fn color_from(c: [u8; 4]) -> Color32 {
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

/// 生成轮廓点的局部坐标。矩形族按 `base_size` 推导，Polyline 直接用 `points`。
///
/// `ellipse_segments` 控制椭圆近似精度：`CleanStyler` 用 64 段（边缘平滑），
/// `RoughStyler` 用 16 段（抖动本身会掩盖折线感，段数翻倍会让贝塞尔数量爆炸）。
fn outline_points(shape: &ShapeData, ellipse_segments: usize) -> Vec<(f32, f32)> {
    let (w, h) = shape.base_size;
    match shape.shape_type {
        ShapeType::Rectangle => vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)],
        ShapeType::Diamond => vec![(w / 2.0, 0.0), (w, h / 2.0), (w / 2.0, h), (0.0, h / 2.0)],
        ShapeType::Ellipse => {
            let (cx, cy, rx, ry) = (w / 2.0, h / 2.0, w / 2.0, h / 2.0);
            (0..ellipse_segments)
                .map(|i| {
                    let a = i as f32 / ellipse_segments as f32 * std::f32::consts::TAU;
                    (cx + rx * a.cos(), cy + ry * a.sin())
                })
                .collect()
        }
        ShapeType::Polyline => shape.points.clone(),
    }
}

/// 局部坐标点列 → 屏幕 Pos2 点列。
fn to_screen_points(pts: &[(f32, f32)], to_screen: &LocalToScreen) -> Vec<Pos2> {
    pts.iter()
        .map(|(x, y)| to_pos2(to_screen.transform_point(euclid::Point2D::new(*x, *y))))
        .collect()
}

/// 矩形族恒闭合；Polyline 由 `closed` 字段决定。
fn is_closed(shape: &ShapeData) -> bool {
    !matches!(shape.shape_type, ShapeType::Polyline) || shape.closed
}

/// 追加起/终点箭头（V 形两翼）。箭头头长 = 线宽 × 4，张角 ≈ ±50°。
///
/// 两个风格器共用：手绘风下**箭头不抖动** —— 抖动的箭头会认不出指向，
/// 且 Excalidraw 同样只在笔画上抖、箭头保持规整。
fn push_arrow_heads(
    out: &mut Vec<Shape>,
    pts: &[Pos2],
    egui_stroke: egui::Stroke,
    line_width: f32,
    start_arrow: Option<ArrowHeadStyle>,
    end_arrow: Option<ArrowHeadStyle>,
) {
    if pts.len() < 2 {
        return;
    }
    let half = std::f32::consts::FRAC_PI_2 * (5.0 / 9.0); // ≈50°

    // 起点箭头：尖端指向起点，两翼伸向线段体内（V 开口朝前）。
    if let Some(ArrowHeadStyle::Arrow) = start_arrow {
        let dir = pts[1] - pts[0];
        let len = dir.length();
        if len > 1e-3 {
            let dir = dir / len;
            let head_len = line_width * 4.0;
            let a1 = rotate_vec2(dir, half);
            let a2 = rotate_vec2(dir, -half);
            out.push(Shape::line(
                vec![pts[0], pts[0] + a1 * head_len],
                egui_stroke,
            ));
            out.push(Shape::line(
                vec![pts[0], pts[0] + a2 * head_len],
                egui_stroke,
            ));
        }
    }

    // 终点箭头：尖端指向终点，两翼伸向线段体内（V 开口朝后，箭头朝前）。
    if let Some(ArrowHeadStyle::Arrow) = end_arrow {
        let last = pts[pts.len() - 1];
        let dir = last - pts[pts.len() - 2];
        let len = dir.length();
        if len > 1e-3 {
            let dir = dir / len;
            let head_len = line_width * 4.0;
            let a1 = rotate_vec2(dir, half);
            let a2 = rotate_vec2(dir, -half);
            out.push(Shape::line(vec![last, last - a1 * head_len], egui_stroke));
            out.push(Shape::line(vec![last, last - a2 * head_len], egui_stroke));
        }
    }
}

/// 虚线/点线的 dash 与 gap 长度（屏幕像素），两个风格器共用同一套视觉参数。
fn dash_lengths(dash: DashStyle, zoom: f32, line_width: f32) -> (f32, f32) {
    match dash {
        DashStyle::Dotted => (1.5 * zoom, line_width * 2.0),
        _ => (line_width * 4.0, line_width * 2.0),
    }
}

// ─────────────────────────── CleanStyler ───────────────────────────

/// 简洁实现：精确几何 + 实线/虚线/圆点，epaint 原生。
pub struct CleanStyler;

impl CleanStyler {
    /// 椭圆近似段数。
    const ELLIPSE_SEGMENTS: usize = 64;
}

impl ShapeStyler for CleanStyler {
    fn build_shapes(
        &self,
        shape: &ShapeData,
        stroke: &StrokeStyle,
        fill: Option<Color32>,
        to_screen: &LocalToScreen,
        zoom: f32,
    ) -> Vec<Shape> {
        let mut pts = to_screen_points(&outline_points(shape, Self::ELLIPSE_SEGMENTS), to_screen);
        if pts.len() < 2 {
            return Vec::new();
        }

        let stroke_color = color_from(stroke.color);
        let line_width = stroke.width * zoom;
        let egui_stroke = egui::Stroke::new(line_width, stroke_color);
        let closed = is_closed(shape);

        // 开放折线：直接画线 + 箭头，无填充概念。
        if !closed {
            let mut out = Vec::new();
            match stroke.dash {
                DashStyle::Solid => out.push(Shape::line(pts.clone(), egui_stroke)),
                DashStyle::Dashed | DashStyle::Dotted => {
                    let (d, g) = dash_lengths(stroke.dash, zoom, line_width);
                    out.extend(Shape::dashed_line(&pts, egui_stroke, d, g));
                }
            }
            push_arrow_heads(
                &mut out,
                &pts,
                egui_stroke,
                line_width,
                shape.start_arrow,
                shape.end_arrow,
            );
            return out;
        }

        let fill_color = fill.unwrap_or(Color32::TRANSPARENT);
        let path_stroke = egui::epaint::PathStroke::new(line_width, stroke_color);
        match stroke.dash {
            DashStyle::Solid => vec![Shape::convex_polygon(pts, fill_color, path_stroke)],
            DashStyle::Dashed | DashStyle::Dotted => {
                let mut out = vec![Shape::convex_polygon(
                    pts.clone(),
                    fill_color,
                    egui::epaint::PathStroke::NONE,
                )];
                // 闭合路径：首点追加到末尾
                pts.push(pts[0]);
                let (d, g) = dash_lengths(stroke.dash, zoom, line_width);
                out.extend(Shape::dashed_line(&pts, egui_stroke, d, g));
                out
            }
        }
    }
}

// ─────────────────────────── RoughStyler ───────────────────────────

/// 手绘风实现（Phase F）：把每条边换成抖动的三次贝塞尔，每条边描两遍形成笔触。
///
/// 算法移植自 rough.js `_line`（Excalidraw 同款）：端点与控制点按种子做确定性抖动，
/// 控制点沿边分布并由 bowing 垂直于边撑开，得到"手一抖画歪了"的观感。
/// 抖动幅度以**画布像素**计量再乘 zoom，故放大画布时抖动同步放大，
/// 与真实手绘稿被放大的观感一致（也和 `stroke.width * zoom` 的缩放语义一致）。
pub struct RoughStyler;

impl RoughStyler {
    /// 每条边重复描边的次数（rough.js 为 2，形成双线笔触）。
    const PASSES: usize = 2;
    /// 弓形系数：控制贝塞尔控制点垂直于边的位移强度。
    const BOWING: f32 = 1.0;
    /// 抖动幅度上限（画布像素）：防止长边抖成波浪。
    const MAX_OFFSET_CANVAS: f32 = 8.0;
    /// 抖动幅度占边长的比例。
    const OFFSET_RATIO: f32 = 0.06;
    /// 椭圆近似段数（降采样以控制贝塞尔数量）。
    const ELLIPSE_SEGMENTS: usize = 16;
    /// dash 模式下把每条贝塞尔采样为折线的点数。
    const DASH_SAMPLES: usize = 16;

    /// 生成一条抖动边 `a → b` 的三次贝塞尔控制点 `[p0, c1, c2, p3]`。
    ///
    /// 退化边（长度 ≈ 0）直接返回零抖动直线，避免除以 0 得到 NaN。
    fn sketch_edge(rng: &mut SeededRng, a: Pos2, b: Pos2, zoom: f32) -> [Pos2; 4] {
        let d = b - a;
        let len = d.length();
        if len < 1e-3 {
            return [a, a, b, b];
        }

        // 抖动幅度以画布像素计量，再换算回屏幕像素（见类型注释）。
        let len_canvas = len / zoom;
        let max_offset = (len_canvas * Self::OFFSET_RATIO).min(Self::MAX_OFFSET_CANVAS) * zoom;
        let half = max_offset * 0.5;

        // 弓形位移：垂直于边，强度随边长增长（200 画布像素处饱和）。
        let bow_k = (len_canvas / 200.0).min(1.0);
        let bow = Self::BOWING * max_offset * bow_k / len;
        let mid_disp = egui::vec2(-d.y * bow, d.x * bow);

        // 控制点沿边的位置（0.2~0.4 与 0.4~0.8），rough.js 的 divergePoint。
        let diverge = 0.2 + rng.next_f32() * 0.2;
        let jitter = |rng: &mut SeededRng| egui::vec2(rng.signed() * half, rng.signed() * half);

        let p0 = a + jitter(rng);
        let p3 = b + jitter(rng);
        let m1 = a + d * diverge;
        let m2 = a + d * (2.0 * diverge);
        let c1 = m1 + mid_disp + jitter(rng);
        let c2 = m2 + mid_disp + jitter(rng);
        [p0, c1, c2, p3]
    }

    /// 三次贝塞尔采样为折线。dash 模式下 `PathStroke` 不支持虚线，
    /// 只能打散成点列交给 `Shape::dashed_line`。
    fn sample_bezier(pts: &[Pos2; 4], n: usize) -> Vec<Pos2> {
        (0..=n)
            .map(|i| {
                let t = i as f32 / n as f32;
                let u = 1.0 - t;
                let b0 = u * u * u;
                let b1 = 3.0 * u * u * t;
                let b2 = 3.0 * u * t * t;
                let b3 = t * t * t;
                egui::pos2(
                    pts[0].x * b0 + pts[1].x * b1 + pts[2].x * b2 + pts[3].x * b3,
                    pts[0].y * b0 + pts[1].y * b1 + pts[2].y * b2 + pts[3].y * b3,
                )
            })
            .collect()
    }
}

impl ShapeStyler for RoughStyler {
    fn build_shapes(
        &self,
        shape: &ShapeData,
        stroke: &StrokeStyle,
        fill: Option<Color32>,
        to_screen: &LocalToScreen,
        zoom: f32,
    ) -> Vec<Shape> {
        let pts = to_screen_points(&outline_points(shape, Self::ELLIPSE_SEGMENTS), to_screen);
        if pts.len() < 2 {
            return Vec::new();
        }

        let stroke_color = color_from(stroke.color);
        let line_width = stroke.width * zoom;
        let egui_stroke = egui::Stroke::new(line_width, stroke_color);
        let closed = is_closed(shape);
        let mut out = Vec::new();

        // 填充保持精确几何：本轮只手绘化描边，填充仍是与 CleanStyler 一致的凸多边形。
        if closed {
            if let Some(f) = fill {
                out.push(Shape::convex_polygon(
                    pts.clone(),
                    f,
                    egui::epaint::PathStroke::NONE,
                ));
            }
        }

        let mut rng = SeededRng::new(shape.seed);
        // 闭合图形多一条 n-1 → 0 的收尾边。
        let seg_count = if closed { pts.len() } else { pts.len() - 1 };
        for i in 0..seg_count {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            for _ in 0..Self::PASSES {
                let bez = Self::sketch_edge(&mut rng, a, b, zoom);
                match stroke.dash {
                    DashStyle::Solid => out.push(Shape::CubicBezier(
                        egui::epaint::CubicBezierShape::from_points_stroke(
                            bez,
                            false,
                            Color32::TRANSPARENT,
                            egui::epaint::PathStroke::new(line_width, stroke_color),
                        ),
                    )),
                    DashStyle::Dashed | DashStyle::Dotted => {
                        let (d, g) = dash_lengths(stroke.dash, zoom, line_width);
                        let sampled = Self::sample_bezier(&bez, Self::DASH_SAMPLES);
                        out.extend(Shape::dashed_line(&sampled, egui_stroke, d, g));
                    }
                }
            }
        }

        if !closed {
            push_arrow_heads(
                &mut out,
                &pts,
                egui_stroke,
                line_width,
                shape.start_arrow,
                shape.end_arrow,
            );
        }
        out
    }
}

// ─────────────────────────── 渲染入口 ───────────────────────────

/// 依据 Shape 的 `rough` 开关选择风格器，构建 egui 形状列表。
///
/// `render_scene` 与 Present 模式共用，避免两处各组装一遍 `ShapeData`。
pub fn build_shape_visuals(kind: &ItemKind, to_screen: &LocalToScreen, zoom: f32) -> Vec<Shape> {
    let ItemKind::Shape {
        shape_type,
        base_size,
        points,
        stroke,
        fill,
        start_arrow,
        end_arrow,
        closed,
        seed,
        rough,
    } = kind
    else {
        return Vec::new();
    };

    let data = ShapeData {
        shape_type: *shape_type,
        base_size: *base_size,
        points: points.clone(),
        start_arrow: *start_arrow,
        end_arrow: *end_arrow,
        closed: *closed,
        seed: *seed,
    };
    let fill_color = fill.map(color_from);

    if *rough {
        RoughStyler.build_shapes(&data, stroke, fill_color, to_screen, zoom)
    } else {
        CleanStyler.build_shapes(&data, stroke, fill_color, to_screen, zoom)
    }
}

/// 便捷入口：Item 局部 → 屏幕 的变换（供 render_scene 使用）。
pub fn item_local_to_screen(
    item: &preferz_core::Item,
    viewport: &crate::viewport::ViewportState,
) -> LocalToScreen {
    item.local_to_canvas()
        .then(&viewport.canvas_to_screen_transform())
}

#[cfg(test)]
mod tests {
    use super::*;
    use preferz_core::Item;

    fn identity() -> LocalToScreen {
        euclid::Transform2D::identity()
    }

    fn rect(seed: u64) -> ShapeData {
        ShapeData {
            shape_type: ShapeType::Rectangle,
            base_size: (100.0, 60.0),
            points: Vec::new(),
            start_arrow: None,
            end_arrow: None,
            closed: false,
            seed,
        }
    }

    fn open_line() -> ShapeData {
        ShapeData {
            shape_type: ShapeType::Polyline,
            base_size: (100.0, 0.0),
            points: vec![(0.0, 0.0), (100.0, 0.0)],
            start_arrow: None,
            end_arrow: Some(ArrowHeadStyle::Arrow),
            closed: false,
            seed: 42,
        }
    }

    fn debug(shapes: &[Shape]) -> String {
        format!("{shapes:?}")
    }

    #[test]
    fn rough_styler_is_deterministic_for_same_seed() {
        let d = rect(0xABCD);
        let stroke = StrokeStyle::default();
        let a = RoughStyler.build_shapes(&d, &stroke, None, &identity(), 1.0);
        let b = RoughStyler.build_shapes(&d, &stroke, None, &identity(), 1.0);
        assert_eq!(debug(&a), debug(&b), "同 seed 必须得到同一抖动");
    }

    #[test]
    fn rough_styler_differs_for_different_seed() {
        let stroke = StrokeStyle::default();
        let a = RoughStyler.build_shapes(&rect(1), &stroke, None, &identity(), 1.0);
        let b = RoughStyler.build_shapes(&rect(2), &stroke, None, &identity(), 1.0);
        assert_ne!(debug(&a), debug(&b), "不同 seed 应得到不同抖动");
    }

    #[test]
    fn rough_styler_rect_emits_two_passes_per_edge() {
        let stroke = StrokeStyle::default();
        // 矩形 4 条边 × 2 passes = 8 条贝塞尔；无填充故不加凸多边形。
        let shapes = RoughStyler.build_shapes(&rect(7), &stroke, None, &identity(), 1.0);
        assert_eq!(shapes.len(), 8);
        assert!(shapes.iter().all(|s| matches!(s, Shape::CubicBezier(_))));
    }

    #[test]
    fn rough_styler_fill_stays_exact_polygon() {
        let stroke = StrokeStyle::default();
        let shapes = RoughStyler.build_shapes(
            &rect(7),
            &stroke,
            Some(Color32::from_rgb(10, 20, 30)),
            &identity(),
            1.0,
        );
        // 1 个精确凸多边形填充 + 8 条抖动边
        assert_eq!(shapes.len(), 9);
        assert!(matches!(shapes[0], Shape::Path(_)));
    }

    #[test]
    fn rough_styler_keeps_arrow_heads_crisp() {
        let stroke = StrokeStyle::default();
        let shapes = RoughStyler.build_shapes(&open_line(), &stroke, None, &identity(), 1.0);
        // 1 条边 × 2 passes + 2 笔箭头 = 4
        assert_eq!(shapes.len(), 4);
        assert_eq!(
            shapes
                .iter()
                .filter(|s| matches!(s, Shape::CubicBezier(_)))
                .count(),
            2
        );
    }

    #[test]
    fn rough_styler_dash_falls_back_to_sampled_polyline() {
        let stroke = StrokeStyle {
            color: [255, 255, 255, 255],
            width: 2.0,
            dash: DashStyle::Dashed,
        };
        let shapes = RoughStyler.build_shapes(&rect(7), &stroke, None, &identity(), 1.0);
        // dash 模式下每条边采样为折线，产生 ≥1 个 shape；不出现贝塞尔
        assert!(!shapes.is_empty());
        assert!(shapes.iter().all(|s| !matches!(s, Shape::CubicBezier(_))));
    }

    #[test]
    fn rough_styler_ellipse_is_downsampled() {
        let stroke = StrokeStyle::default();
        let ellipse = ShapeData {
            shape_type: ShapeType::Ellipse,
            base_size: (100.0, 100.0),
            points: Vec::new(),
            start_arrow: None,
            end_arrow: None,
            closed: false,
            seed: 3,
        };
        // 16 段 × 2 passes = 32 条贝塞尔（CleanStyler 的 64 段会翻倍到 128）
        let shapes = RoughStyler.build_shapes(&ellipse, &stroke, None, &identity(), 1.0);
        assert_eq!(shapes.len(), 32);
    }

    #[test]
    fn rough_styler_degenerate_edge_does_not_produce_nan() {
        // 零尺寸矩形：所有边退化为一个点，不应产生 NaN 坐标
        let degenerate = ShapeData {
            shape_type: ShapeType::Rectangle,
            base_size: (0.0, 0.0),
            points: Vec::new(),
            start_arrow: None,
            end_arrow: None,
            closed: false,
            seed: 5,
        };
        let shapes =
            RoughStyler.build_shapes(&degenerate, &StrokeStyle::default(), None, &identity(), 1.0);
        for s in &shapes {
            if let Shape::CubicBezier(b) = s {
                for p in b.points {
                    assert!(p.x.is_finite() && p.y.is_finite(), "NaN/Inf 坐标: {p:?}");
                }
            }
        }
    }

    #[test]
    fn clean_styler_rect_is_single_polygon() {
        let shapes =
            CleanStyler.build_shapes(&rect(0), &StrokeStyle::default(), None, &identity(), 1.0);
        assert_eq!(shapes.len(), 1);
    }

    #[test]
    fn build_shape_visuals_dispatches_on_rough_flag() {
        let base = Item::new_shape(
            ShapeType::Rectangle,
            (100.0, 60.0),
            0.0,
            0.0,
            StrokeStyle::default(),
            None,
        );
        let clean = build_shape_visuals(&base.kind, &identity(), 1.0);
        assert_eq!(clean.len(), 1, "未开启手绘 → CleanStyler 单多边形");

        let rough = base.with_rough(true);
        let sketched = build_shape_visuals(&rough.kind, &identity(), 1.0);
        assert_eq!(sketched.len(), 8, "开启手绘 → RoughStyler 抖动边");
    }

    #[test]
    fn build_shape_visuals_returns_empty_for_non_shape() {
        let txt = Item::new_text("x".to_string(), 0.0, 0.0, 16.0, [255; 4]);
        assert!(build_shape_visuals(&txt.kind, &identity(), 1.0).is_empty());
    }
}
