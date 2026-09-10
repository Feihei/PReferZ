use eframe::egui::{self, Color32, Pos2, Shape};
use preferz_core::item::{
    catmull_rom_polyline, ItemKind, ItemLocalSpace, CURVE_SAMPLES, ROUNDED_CORNER_SEGMENTS,
};
use preferz_core::shape::{
    ArrowHeadStyle, CurveType, DashStyle, FillStyle, SeededRng, ShapeType, Sloppiness, StrokeStyle,
};
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
    /// 曲线模式（Phase I）。仅 Polyline 使用；`Curved` 经 Catmull-Rom 插值。
    pub curve_type: CurveType,
    /// 矩形族圆角比例 0..1（Phase I）。仅矩形族使用。
    pub roundness: f32,
    /// 手绘风描边抖动种子（Phase F）。同一 seed 恒得同一抖动；`CleanStyler` 忽略此字段。
    pub seed: u64,
    /// 手绘风抖动档位（plan #3）。`Off` 时调用方应选 `CleanStyler`；
    /// `RoughStyler` 按 [`Sloppiness::amp_scale`] 缩放抖动幅度。
    pub sloppiness: Sloppiness,
}

/// 将 shape 局部几何转换为屏幕空间的 egui::Shape 列表。
/// 风格器自行把局部点经 to_screen 变换到屏幕；stroke 线宽按 zoom 缩放。
pub trait ShapeStyler {
    fn build_shapes(
        &self,
        shape: &ShapeData,
        stroke: &StrokeStyle,
        fill: Option<Color32>,
        fill_style: FillStyle,
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

/// 矩形族的圆角半径（局部坐标）：`min(w, h) * roundness`，且不超过短边的一半。
///
/// 超过一半会让两头的圆弧互相吃掉，退化成畸形轮廓，故显式夹紧。
fn roundness_radius(base_size: (f32, f32), roundness: f32) -> f32 {
    let (w, h) = base_size;
    let short = w.min(h);
    (short * roundness).clamp(0.0, short / 2.0)
}

/// 圆角矩形的轮廓点（顺时针，未闭合——闭合由调用方按 `is_closed` 处理）。
///
/// 每个圆角按 [`ROUNDED_CORNER_SEGMENTS`] 段圆弧采样，故整条轮廓是凸多边形：
/// CleanStyler 可直接填充，RoughStyler 逐边抖动也会得到"手画的圆角矩形"。
fn rounded_rect_points(w: f32, h: f32, r: f32) -> Vec<(f32, f32)> {
    if r <= 1e-3 {
        return vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
    }
    // 角心顺序 TL → TR → BR → BL；每段弧跨 90°（屏幕坐标 y 向下，故顺时针推进）
    let centers = [(r, r), (w - r, r), (w - r, h - r), (r, h - r)];
    let mut out = Vec::with_capacity(centers.len() * (ROUNDED_CORNER_SEGMENTS + 1));
    for (i, (cx, cy)) in centers.iter().enumerate() {
        let start = std::f32::consts::PI + i as f32 * std::f32::consts::FRAC_PI_2;
        for s in 0..=ROUNDED_CORNER_SEGMENTS {
            let a =
                start + (s as f32 / ROUNDED_CORNER_SEGMENTS as f32) * std::f32::consts::FRAC_PI_2;
            out.push((cx + r * a.cos(), cy + r * a.sin()));
        }
    }
    out
}

/// 生成轮廓点的局部坐标。矩形族按 `base_size` 推导，Polyline 直接用 `points`。
///
/// `ellipse_segments` 控制椭圆的采样点数：`CleanStyler` 用 64 段（边缘平滑），
/// `RoughStyler` 用 24 段——后者还会把采样点插值成光滑曲线，无需靠堆段数换圆度。
///
/// 两处会改写点列（Phase I）：
/// - 矩形 `roundness > 0` → 四角换成圆弧采样点；
/// - Polyline + `Curved` → 经 `catmull_rom_polyline` 插值（与命中测试同源）。
fn outline_points(shape: &ShapeData, ellipse_segments: usize) -> Vec<(f32, f32)> {
    let (w, h) = shape.base_size;
    match shape.shape_type {
        ShapeType::Rectangle => {
            let r = roundness_radius((w, h), shape.roundness);
            if r > 1e-3 {
                rounded_rect_points(w, h, r)
            } else {
                vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]
            }
        }
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
        ShapeType::Polyline => {
            // Phase I：`Curved` 经 Catmull-Rom 插值成折线（与命中测试同源，
            // 故"看着在曲线上"的点一定点得中）。两点曲线无中间控制点，插值无意义。
            if matches!(shape.curve_type, CurveType::Curved) {
                catmull_rom_polyline(&shape.points, shape.closed, CURVE_SAMPLES)
            } else {
                shape.points.clone()
            }
        }
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

/// 追加起/终点箭头（Arrow = V 形两翼，Dot = 实心圆点）。
///
/// 尺寸基准：Arrow 的头长 = 线宽 × 4；Dot 的半径 = 线宽 × 1.5（直径 3 倍线宽，
/// 与 Arrow 的视觉分量相当），圆心沿线段方向内缩一个半径，使圆点**相切于端点**
/// 而不是盖住它（Excalidraw 同款观感）。
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
    match start_arrow {
        Some(ArrowHeadStyle::Arrow) => {
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
        Some(ArrowHeadStyle::Dot) => {
            let dir = pts[1] - pts[0];
            let len = dir.length();
            if len > 1e-3 {
                let dir = dir / len;
                let radius = (line_width * 1.5).max(1.0);
                out.push(Shape::circle_filled(
                    pts[0] + dir * radius,
                    radius,
                    egui_stroke.color,
                ));
            }
        }
        None => {}
    }

    // 终点箭头：尖端指向终点，两翼伸向线段体内（V 开口朝后，箭头朝前）。
    let last = pts[pts.len() - 1];
    let dir = last - pts[pts.len() - 2];
    let len = dir.length();
    if len > 1e-3 {
        let dir = dir / len;
        match end_arrow {
            Some(ArrowHeadStyle::Arrow) => {
                let head_len = line_width * 4.0;
                let a1 = rotate_vec2(dir, half);
                let a2 = rotate_vec2(dir, -half);
                out.push(Shape::line(vec![last, last - a1 * head_len], egui_stroke));
                out.push(Shape::line(vec![last, last - a2 * head_len], egui_stroke));
            }
            Some(ArrowHeadStyle::Dot) => {
                let radius = (line_width * 1.5).max(1.0);
                out.push(Shape::circle_filled(
                    last - dir * radius,
                    radius,
                    egui_stroke.color,
                ));
            }
            None => {}
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

// ─────────────────────────── Hachure 填充（Excalidraw 同款） ───────────────────────────

/// 斜线填充的默认角度（度，屏幕坐标系）。rough.js / Excalidraw 同款默认值。
const HACHURE_ANGLE_DEG: f32 = -41.0;

/// 斜线填充的行距：rough.js 同款 `hachureGap = 4 × 线宽`，并给个下限防止细线贴死。
fn hachure_gap(line_width: f32) -> f32 {
    (line_width * 4.0).max(4.0)
}

/// 生成沿 `angle_deg` 方向的斜线填充线段（屏幕空间）。
///
/// 算法：把多边形旋转 `-angle`，使填充线方向变为水平；对每条水平扫描线求与
/// 多边形各边的交点横坐标，排序后两两配对（穿入/穿出交替），再旋回。
/// 对凸/凹简单多边形均适用（凹多边形一条扫描线可得 >2 个交点，配对后为多段）。
fn hachure_segments(pts: &[Pos2], angle_deg: f32, gap: f32) -> Vec<[Pos2; 2]> {
    if pts.len() < 3 || gap <= 1e-3 {
        return Vec::new();
    }
    let rad = angle_deg.to_radians();
    let (sin, cos) = rad.sin_cos();
    // 旋转 -angle：填充线方向变为水平 x 轴
    let fwd = |p: &Pos2| egui::pos2(p.x * cos + p.y * sin, -p.x * sin + p.y * cos);
    // 旋回 +angle
    let back = |p: &Pos2| egui::pos2(p.x * cos - p.y * sin, p.x * sin + p.y * cos);

    let rp: Vec<Pos2> = pts.iter().map(fwd).collect();
    let ymin = rp.iter().map(|p| p.y).fold(f32::MAX, f32::min);
    let ymax = rp.iter().map(|p| p.y).fold(f32::MIN, f32::max);

    let mut segs = Vec::new();
    let mut y = ymin + gap * 0.5;
    while y < ymax {
        let mut xs: Vec<f32> = Vec::new();
        for i in 0..rp.len() {
            let a = rp[i];
            let b = rp[(i + 1) % rp.len()];
            // 半开区间 [min, max) 避免顶点被相邻两条边重复计入
            if (a.y <= y && b.y > y) || (b.y <= y && a.y > y) {
                let t = (y - a.y) / (b.y - a.y);
                xs.push(a.x + (b.x - a.x) * t);
            }
        }
        xs.sort_by(f32::total_cmp);
        for pair in xs.chunks(2) {
            if let [x0, x1] = pair {
                segs.push([back(&egui::pos2(*x0, y)), back(&egui::pos2(*x1, y))]);
            }
        }
        y += gap;
    }
    segs
}

/// 斜线段列表 → egui 形状（颜色即填充色，线宽同描边）。
fn hachure_shapes(
    pts: &[Pos2],
    angle_deg: f32,
    gap: f32,
    line_width: f32,
    color: Color32,
) -> Vec<Shape> {
    let stroke = egui::Stroke::new(line_width, color);
    hachure_segments(pts, angle_deg, gap)
        .into_iter()
        .map(|seg| Shape::line_segment(seg, stroke))
        .collect()
}

/// 闭合多边形填充。走 epaint 的 PathShape（耳切三角化），凹多边形也正确——
/// `Shape::convex_polygon` 只适配凸形，多边形工具可产出凹形。
fn closed_filled_path(pts: Vec<Pos2>, fill: Color32) -> Shape {
    Shape::Path(egui::epaint::PathShape {
        points: pts,
        closed: true,
        fill,
        stroke: egui::epaint::PathStroke::NONE,
    })
}

/// 闭合多边形描边（不填充）。
fn closed_stroked_path(pts: Vec<Pos2>, stroke: egui::epaint::PathStroke) -> Shape {
    Shape::Path(egui::epaint::PathShape {
        points: pts,
        closed: true,
        fill: Color32::TRANSPARENT,
        stroke,
    })
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
        fill_style: FillStyle,
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

        let mut out = Vec::new();

        // 填充层（在轮廓之下）：纯色 = 实心多边形；斜线/交叉线 = 平行线段，
        // 不铺底色（Excalidraw 同款观感），颜色即填充色。无填充（None）时跳过。
        if let Some(fill_color) = fill {
            match fill_style {
                FillStyle::Solid => out.push(closed_filled_path(pts.clone(), fill_color)),
                FillStyle::Hachure => out.extend(hachure_shapes(
                    &pts,
                    HACHURE_ANGLE_DEG,
                    hachure_gap(line_width),
                    line_width,
                    fill_color,
                )),
                FillStyle::CrossHatch => {
                    for angle in [HACHURE_ANGLE_DEG, HACHURE_ANGLE_DEG + 90.0] {
                        out.extend(hachure_shapes(
                            &pts,
                            angle,
                            hachure_gap(line_width),
                            line_width,
                            fill_color,
                        ));
                    }
                }
            }
        }

        // 轮廓层
        match stroke.dash {
            DashStyle::Solid => out.push(closed_stroked_path(
                pts,
                egui::epaint::PathStroke::new(line_width, stroke_color),
            )),
            DashStyle::Dashed | DashStyle::Dotted => {
                // 闭合路径：首点追加到末尾
                pts.push(pts[0]);
                let (d, g) = dash_lengths(stroke.dash, zoom, line_width);
                out.extend(Shape::dashed_line(&pts, egui_stroke, d, g));
            }
        }
        out
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
    /// 椭圆近似的采样点数：`RoughStyler` 采样后还要经
    /// [`RoughStyler::closed_catmull_rom`] 插值成光滑曲线，故点数只需够圆即可。
    const ELLIPSE_SEGMENTS: usize = 24;
    /// dash 模式下把每条贝塞尔采样为折线的点数。
    const DASH_SAMPLES: usize = 16;

    /// 该轮廓是否为**曲线类**：抖动后需连成光滑曲线，而非逐边画抖动的直线段。
    ///
    /// 两类属于曲线：
    /// - 椭圆：由采样点近似，直接连段会露出多边形折角（手绘风下尤其刺眼）；
    /// - `Curved` 的 Polyline：Catmull-Rom 采样后本身就是曲线控制点。
    ///
    /// 矩形 / 菱形 / `Straight` 折线本来就是直边，保持逐边抖动。
    fn is_smooth(shape: &ShapeData) -> bool {
        match shape.shape_type {
            ShapeType::Ellipse => true,
            ShapeType::Polyline => matches!(shape.curve_type, CurveType::Curved),
            ShapeType::Rectangle | ShapeType::Diamond => false,
        }
    }

    /// 整圈抖动：给每个采样点加独立偏移，得到"手抖画歪"的轮廓点环。
    ///
    /// 与逐边抖动（[`RoughStyler::sketch_edge`]）的区别是——这里先抖动顶点、
    /// 再用光滑曲线穿过它们，因此曲线类轮廓（椭圆）不会出现直线段拼接的折角。
    fn jitter_points(rng: &mut SeededRng, pts: &[Pos2], amp: f32) -> Vec<Pos2> {
        pts.iter()
            .map(|p| *p + egui::vec2(rng.signed() * amp, rng.signed() * amp))
            .collect()
    }

    /// 把闭合点环转成 C1 连续的三次贝塞尔序列（均匀 Catmull-Rom，张力 0.5）。
    ///
    /// 段 i 从 `p1` 到 `p2`，控制点由前后邻居推出：
    /// `c1 = p1 + (p2 - p0) / 6`、`c2 = p2 - (p3 - p1) / 6`。
    /// 相邻两段在共享端点处切线方向相同，故整条闭合曲线无可见折角。
    fn closed_catmull_rom(pts: &[Pos2]) -> Vec<[Pos2; 4]> {
        let n = pts.len();
        if n < 3 {
            return Vec::new();
        }
        (0..n)
            .map(|i| {
                let p0 = pts[(i + n - 1) % n];
                let p1 = pts[i];
                let p2 = pts[(i + 1) % n];
                let p3 = pts[(i + 2) % n];
                [p1, p1 + (p2 - p0) / 6.0, p2 - (p3 - p1) / 6.0, p2]
            })
            .collect()
    }

    /// 同 [`RoughStyler::closed_catmull_rom`]，但用于**开曲线**：两端用重复点外推
    /// （`p0 = p1`、`p3 = p2`），故首末点不被邻居拉偏。
    fn open_catmull_rom(pts: &[Pos2]) -> Vec<[Pos2; 4]> {
        let n = pts.len();
        if n < 2 {
            return Vec::new();
        }
        (0..n - 1)
            .map(|i| {
                let p0 = pts[i.saturating_sub(1)];
                let p1 = pts[i];
                let p2 = pts[i + 1];
                let p3 = pts[(i + 2).min(n - 1)];
                [p1, p1 + (p2 - p0) / 6.0, p2 - (p3 - p1) / 6.0, p2]
            })
            .collect()
    }

    /// 抖动贝塞尔序列：闭合轮廓用环绕式，开放曲线用端点外推式。
    fn catmull_rom_beziers(pts: &[Pos2], closed: bool) -> Vec<[Pos2; 4]> {
        if closed {
            Self::closed_catmull_rom(pts)
        } else {
            Self::open_catmull_rom(pts)
        }
    }

    /// 闭合点环的抖动幅度（屏幕像素）：取相邻点平均间距换算回画布像素后乘
    /// [`RoughStyler::OFFSET_RATIO`]，再受 `MAX_OFFSET_CANVAS` 约束——
    /// 与 [`RoughStyler::sketch_edge`] 同一尺度，保证直线与曲线抖动观感一致。
    ///
    /// 开放曲线的周长不含"末点 → 首点"那一段（它并不存在），否则平均间距被
    /// 虚增，抖动幅度会偏大。
    fn curve_jitter_amp(pts: &[Pos2], zoom: f32, closed: bool, amp_scale: f32) -> f32 {
        let n = pts.len();
        if n < 2 {
            return 0.0;
        }
        let zoom = zoom.max(1e-3);
        let seg_count = if closed { n } else { n - 1 };
        let perimeter: f32 = (0..seg_count)
            .map(|i| (pts[i] - pts[(i + 1) % n]).length())
            .sum();
        let avg_canvas = perimeter / seg_count as f32 / zoom;
        // 不再额外打五折：此前 ×0.5 加上采样密集导致曲线抖动仅零点几像素，
        // 各 Sloppiness 档位肉眼无差别（用户反馈"曲线手绘样式都一样"）。
        // 现在与逐边抖动同一尺度，档位差异（0.5/1/1.8）可感知。
        (avg_canvas * Self::OFFSET_RATIO).min(Self::MAX_OFFSET_CANVAS) * zoom * amp_scale
    }

    /// 按 dash 样式把一条抖动贝塞尔落到 egui 形状列表。
    ///
    /// 实线直接用 `CubicBezier`（tessellator 自适应细分，放大也不露折角）；
    /// dash 模式下 `PathStroke` 不支持虚线，只能采样成点列交给 `Shape::dashed_line`。
    fn push_edge(
        out: &mut Vec<Shape>,
        bez: [Pos2; 4],
        stroke: &StrokeStyle,
        line_width: f32,
        stroke_color: Color32,
        zoom: f32,
    ) {
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
                out.extend(Shape::dashed_line(
                    &sampled,
                    egui::Stroke::new(line_width, stroke_color),
                    d,
                    g,
                ));
            }
        }
    }

    /// 生成一条抖动边 `a → b` 的三次贝塞尔控制点 `[p0, c1, c2, p3]`。
    ///
    /// 退化边（长度 ≈ 0）直接返回零抖动直线，避免除以 0 得到 NaN。
    fn sketch_edge(rng: &mut SeededRng, a: Pos2, b: Pos2, zoom: f32, amp_scale: f32) -> [Pos2; 4] {
        let d = b - a;
        let len = d.length();
        if len < 1e-3 {
            return [a, a, b, b];
        }

        // 抖动幅度以画布像素计量，再换算回屏幕像素（见类型注释）；
        // 乘档位系数得到 Architect/Artist/Cartoonist 三档观感（plan #3）。
        let len_canvas = len / zoom;
        let max_offset =
            (len_canvas * Self::OFFSET_RATIO).min(Self::MAX_OFFSET_CANVAS) * zoom * amp_scale;
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
        fill_style: FillStyle,
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

        // 填充层：纯色 = 精确实心多边形（与 CleanStyler 一致）；斜线/交叉线 =
        // 端点抖动的斜线段（手绘观感），抖动用独立种子的 rng，与描边抖动解耦。
        if closed {
            if let Some(f) = fill {
                match fill_style {
                    FillStyle::Solid => out.push(closed_filled_path(pts.clone(), f)),
                    FillStyle::Hachure | FillStyle::CrossHatch => {
                        let angles: &[f32] = match fill_style {
                            FillStyle::CrossHatch => &[HACHURE_ANGLE_DEG, HACHURE_ANGLE_DEG + 90.0],
                            _ => &[HACHURE_ANGLE_DEG],
                        };
                        let mut fill_rng = SeededRng::new(shape.seed ^ 0x6841_4355_4C4C_5F53); // "hACULL_S"
                        let amp = hachure_gap(line_width) * 0.15;
                        for angle in angles {
                            let segs = hachure_segments(&pts, *angle, hachure_gap(line_width));
                            for [a, b] in segs {
                                for _ in 0..Self::PASSES {
                                    let a = a + egui::vec2(
                                        fill_rng.signed() * amp,
                                        fill_rng.signed() * amp,
                                    );
                                    let b = b + egui::vec2(
                                        fill_rng.signed() * amp,
                                        fill_rng.signed() * amp,
                                    );
                                    out.push(Shape::line_segment(
                                        [a, b],
                                        egui::Stroke::new(line_width, f),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut rng = SeededRng::new(shape.seed);
        let amp_scale = shape.sloppiness.amp_scale();

        if Self::is_smooth(shape) {
            // 曲线类轮廓（椭圆、Curved 折线）：整圈抖动后连成光滑曲线。
            // 若沿用逐边直线抖动，采样段之间的折角会非常明显。
            let amp = Self::curve_jitter_amp(&pts, zoom, closed, amp_scale);
            for _ in 0..Self::PASSES {
                let jittered = Self::jitter_points(&mut rng, &pts, amp);
                for bez in Self::catmull_rom_beziers(&jittered, closed) {
                    Self::push_edge(&mut out, bez, stroke, line_width, stroke_color, zoom);
                }
            }
        } else {
            // 直线类轮廓（矩形 / 菱形 / 折线）：逐边抖动，闭合图形多一条 n-1 → 0 的收尾边。
            let seg_count = if closed { pts.len() } else { pts.len() - 1 };
            for i in 0..seg_count {
                let a = pts[i];
                let b = pts[(i + 1) % pts.len()];
                for _ in 0..Self::PASSES {
                    let bez = Self::sketch_edge(&mut rng, a, b, zoom, amp_scale);
                    Self::push_edge(&mut out, bez, stroke, line_width, stroke_color, zoom);
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

/// 依据 Shape 的 `sloppiness` 档位选择风格器，构建 egui 形状列表。
///
/// `render_scene` 与 Present 模式共用，避免两处各组装一遍 `ShapeData`。
pub fn build_shape_visuals(kind: &ItemKind, to_screen: &LocalToScreen, zoom: f32) -> Vec<Shape> {
    let ItemKind::Shape {
        shape_type,
        base_size,
        points,
        stroke,
        fill,
        fill_style,
        start_arrow,
        end_arrow,
        closed,
        curve_type,
        roundness,
        seed,
        sloppiness,
        ..
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
        curve_type: *curve_type,
        roundness: *roundness,
        seed: *seed,
        sloppiness: *sloppiness,
    };
    let fill_color = fill.map(color_from);

    if *sloppiness != Sloppiness::Off {
        RoughStyler.build_shapes(&data, stroke, fill_color, *fill_style, to_screen, zoom)
    } else {
        CleanStyler.build_shapes(&data, stroke, fill_color, *fill_style, to_screen, zoom)
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
            curve_type: CurveType::Straight,
            roundness: 0.0,
            seed,
            sloppiness: Sloppiness::Off,
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
            curve_type: CurveType::Straight,
            roundness: 0.0,
            seed: 42,
            sloppiness: Sloppiness::Off,
        }
    }

    fn debug(shapes: &[Shape]) -> String {
        format!("{shapes:?}")
    }

    #[test]
    fn rough_styler_is_deterministic_for_same_seed() {
        let d = rect(0xABCD);
        let stroke = StrokeStyle::default();
        let a = RoughStyler.build_shapes(&d, &stroke, None, FillStyle::Solid, &identity(), 1.0);
        let b = RoughStyler.build_shapes(&d, &stroke, None, FillStyle::Solid, &identity(), 1.0);
        assert_eq!(debug(&a), debug(&b), "同 seed 必须得到同一抖动");
    }

    #[test]
    fn rough_styler_differs_for_different_seed() {
        let stroke = StrokeStyle::default();
        let a =
            RoughStyler.build_shapes(&rect(1), &stroke, None, FillStyle::Solid, &identity(), 1.0);
        let b =
            RoughStyler.build_shapes(&rect(2), &stroke, None, FillStyle::Solid, &identity(), 1.0);
        assert_ne!(debug(&a), debug(&b), "不同 seed 应得到不同抖动");
    }

    #[test]
    fn rough_styler_rect_emits_two_passes_per_edge() {
        let stroke = StrokeStyle::default();
        // 矩形 4 条边 × 2 passes = 8 条贝塞尔；无填充故不加凸多边形。
        let shapes =
            RoughStyler.build_shapes(&rect(7), &stroke, None, FillStyle::Solid, &identity(), 1.0);
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
            FillStyle::Solid,
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
        let shapes = RoughStyler.build_shapes(
            &open_line(),
            &stroke,
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
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
        let shapes =
            RoughStyler.build_shapes(&rect(7), &stroke, None, FillStyle::Solid, &identity(), 1.0);
        // dash 模式下每条边采样为折线，产生 ≥1 个 shape；不出现贝塞尔
        assert!(!shapes.is_empty());
        assert!(shapes.iter().all(|s| !matches!(s, Shape::CubicBezier(_))));
    }

    fn ellipse(size: f32, seed: u64) -> ShapeData {
        ShapeData {
            shape_type: ShapeType::Ellipse,
            base_size: (size, size),
            points: Vec::new(),
            start_arrow: None,
            end_arrow: None,
            closed: false,
            curve_type: CurveType::Straight,
            roundness: 0.0,
            seed,
            sloppiness: Sloppiness::Artist,
        }
    }

    /// 取出所有贝塞尔的控制点数组（非贝塞尔形状直接 panic，便于定位）。
    fn beziers(shapes: &[Shape]) -> Vec<[Pos2; 4]> {
        shapes
            .iter()
            .map(|s| match s {
                Shape::CubicBezier(b) => b.points,
                other => panic!("expected CubicBezier, got {other:?}"),
            })
            .collect()
    }

    #[test]
    fn rough_styler_ellipse_is_smooth_curve() {
        let stroke = StrokeStyle::default();
        let shapes = RoughStyler.build_shapes(
            &ellipse(100.0, 3),
            &stroke,
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        // 24 段 × 2 passes = 48 条贝塞尔；曲线轮廓不得退化成直线段拼接
        assert_eq!(shapes.len(), 48);
        assert!(shapes.iter().all(|s| matches!(s, Shape::CubicBezier(_))));

        // 每条 pass 内部：相邻两段首尾重合且切线共线（C1 连续），否则会看到折角
        let bez = beziers(&shapes);
        for pass in bez.chunks(24) {
            for i in 0..pass.len() {
                let cur = pass[i];
                let next = pass[(i + 1) % pass.len()];
                assert_eq!(cur[3], next[0], "相邻段必须在端点处相接");
                let in_tangent = cur[3] - cur[2];
                let out_tangent = next[1] - next[0];
                let cross = in_tangent.x * out_tangent.y - in_tangent.y * out_tangent.x;
                assert!(cross.abs() < 1e-3, "接缝处切线不共线，会露出折角");
                assert!(
                    (in_tangent - out_tangent).length() < 1e-3,
                    "接缝处切线长度应相等"
                );
            }
        }
    }

    #[test]
    fn rough_styler_ellipse_stays_deterministic_and_seed_sensitive() {
        let stroke = StrokeStyle::default();
        let a = RoughStyler.build_shapes(
            &ellipse(100.0, 9),
            &stroke,
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        let b = RoughStyler.build_shapes(
            &ellipse(100.0, 9),
            &stroke,
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        assert_eq!(debug(&a), debug(&b));
        let c = RoughStyler.build_shapes(
            &ellipse(100.0, 10),
            &stroke,
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        assert_ne!(debug(&a), debug(&c));
    }

    #[test]
    fn rough_styler_ellipse_jitter_is_bounded() {
        // 抖动幅度受 MAX_OFFSET_CANVAS 约束：既不会抖成一团，也不会抖飞出圆周太远。
        // 局部坐标经 identity 变换直接落到屏幕，故圆心 (200,200)、半径 200。
        let stroke = StrokeStyle::default();
        let shapes = RoughStyler.build_shapes(
            &ellipse(400.0, 11),
            &stroke,
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        let center = egui::pos2(200.0, 200.0);
        for p in beziers(&shapes).iter().flat_map(|seg| seg.iter()) {
            let d = (*p - center).length();
            assert!((100.0..=300.0).contains(&d), "抖动越界: {p:?} d={d}");
        }
    }

    #[test]
    fn rough_styler_ellipse_dash_sampled_no_nan() {
        let stroke = StrokeStyle {
            color: [255, 255, 255, 255],
            width: 2.0,
            dash: DashStyle::Dashed,
        };
        let shapes = RoughStyler.build_shapes(
            &ellipse(100.0, 4),
            &stroke,
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        assert!(!shapes.is_empty());
        assert!(shapes.iter().all(|s| !matches!(s, Shape::CubicBezier(_))));
    }

    #[test]
    fn rough_styler_degenerate_ellipse_does_not_produce_nan() {
        // 零尺寸椭圆：所有采样点重合，Catmull-Rom 退化，不应产生 NaN
        let shapes = RoughStyler.build_shapes(
            &ellipse(0.0, 5),
            &StrokeStyle::default(),
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        for b in beziers(&shapes) {
            for p in b {
                assert!(p.x.is_finite() && p.y.is_finite(), "NaN/Inf 坐标: {p:?}");
            }
        }
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
            curve_type: CurveType::Straight,
            roundness: 0.0,
            seed: 5,
            sloppiness: Sloppiness::Artist,
        };
        let shapes = RoughStyler.build_shapes(
            &degenerate,
            &StrokeStyle::default(),
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
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
        let shapes = CleanStyler.build_shapes(
            &rect(0),
            &StrokeStyle::default(),
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        assert_eq!(shapes.len(), 1);
    }

    // ── Phase I-B：曲线 / 圆角 / Dot 箭头 ──

    /// 三点开放折线，`Curved` + 未闭合。
    fn curved_open_line() -> ShapeData {
        ShapeData {
            shape_type: ShapeType::Polyline,
            base_size: (100.0, 40.0),
            points: vec![(0.0, 40.0), (50.0, 0.0), (100.0, 40.0)],
            start_arrow: None,
            end_arrow: None,
            closed: false,
            curve_type: CurveType::Curved,
            roundness: 0.0,
            seed: 42,
            sloppiness: Sloppiness::Off,
        }
    }

    #[test]
    fn roundness_radius_is_clamped_to_half_of_short_side() {
        // 比例语义：radius = min(w, h) * roundness
        assert_eq!(roundness_radius((100.0, 40.0), 0.0), 0.0);
        assert!((roundness_radius((100.0, 40.0), 0.5) - 20.0).abs() < 1e-5);
        // 超过 0.5 会两头相吃，必须夹到短边一半
        assert!((roundness_radius((100.0, 40.0), 1.0) - 20.0).abs() < 1e-5);
        assert!((roundness_radius((100.0, 40.0), 0.25) - 10.0).abs() < 1e-5);
    }

    #[test]
    fn rounded_rect_outline_is_convex_and_stays_inside_bounds() {
        // 100×40 矩形、圆角比例 0.5 → 半径 20。轮廓必须全部落在 [0,w]×[0,h] 内，
        // 且四角被"削掉"（(0,0) / (w,0) / (w,h) / (0,h) 不再出现在轮廓上）。
        let pts = rounded_rect_points(100.0, 40.0, 20.0);
        assert!(pts.len() > 4, "圆角轮廓必须比直角矩形点更多");
        for (x, y) in &pts {
            assert!((-1e-3..=100.0 + 1e-3).contains(x), "x 越界: {x}");
            assert!((-1e-3..=40.0 + 1e-3).contains(y), "y 越界: {y}");
        }
        let corners = [(0.0, 0.0), (100.0, 0.0), (100.0, 40.0), (0.0, 40.0)];
        for c in corners {
            assert!(
                !pts.iter()
                    .any(|p| (p.0 - c.0).abs() < 1e-3 && (p.1 - c.1).abs() < 1e-3),
                "圆角矩形不应还包含直角点 {c:?}"
            );
        }
    }

    #[test]
    fn rounded_rect_zero_radius_falls_back_to_plain_rect() {
        assert_eq!(
            rounded_rect_points(100.0, 40.0, 0.0),
            vec![(0.0, 0.0), (100.0, 0.0), (100.0, 40.0), (0.0, 40.0)]
        );
    }

    #[test]
    fn clean_styler_rect_roundness_changes_outline_point_count() {
        let mut plain = rect(0);
        let plain_shapes = CleanStyler.build_shapes(
            &plain,
            &StrokeStyle::default(),
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        plain.roundness = 0.5;
        let round_shapes = CleanStyler.build_shapes(
            &plain,
            &StrokeStyle::default(),
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        assert_eq!(plain_shapes.len(), 1);
        assert_eq!(round_shapes.len(), 1);
        let count = |s: &Shape| match s {
            Shape::Path(p) => p.points.len(),
            other => panic!("expected Path, got {other:?}"),
        };
        assert_eq!(count(&plain_shapes[0]), 4);
        assert!(count(&round_shapes[0]) > 4, "圆角应产生更多轮廓点");
    }

    #[test]
    fn curved_polyline_is_sampled_into_more_points() {
        // Curved 折线在采样后点远多于控制点；Straight 原样返回
        let curved = curved_open_line();
        let mut straight = curved_open_line();
        straight.curve_type = CurveType::Straight;

        let curved_pts = outline_points(&curved, 64);
        let straight_pts = outline_points(&straight, 64);
        assert_eq!(straight_pts.len(), 3, "Straight 直接用控制点");
        assert_eq!(curved_pts.len(), 2 * CURVE_SAMPLES + 1);

        // 端点必须落在原控制点上（Catmull-Rom 端点不外推）
        assert!((curved_pts[0].0 - 0.0).abs() < 1e-3 && (curved_pts[0].1 - 40.0).abs() < 1e-3);
        let last = curved_pts.last().unwrap();
        assert!((last.0 - 100.0).abs() < 1e-3 && (last.1 - 40.0).abs() < 1e-3);
    }

    #[test]
    fn curved_polyline_bulges_away_from_the_straight_chord() {
        // 中点 (50,40) 在 Straight 下贴着弦；Curved 应把它推向控制点 (50,0)
        let curved = curved_open_line();
        let pts = outline_points(&curved, 64);
        let mid = pts[pts.len() / 2];
        assert!(mid.1 < 20.0, "曲线中段应明显偏离弦，实际 y={}", mid.1);
    }

    #[test]
    fn dot_arrow_head_emits_a_filled_circle_at_each_end() {
        let mut d = open_line();
        d.start_arrow = Some(ArrowHeadStyle::Dot);
        d.end_arrow = Some(ArrowHeadStyle::Dot);
        let shapes = CleanStyler.build_shapes(
            &d,
            &StrokeStyle::default(),
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        // 1 条线 + 2 个圆点
        assert_eq!(shapes.len(), 3);
        let circles: Vec<_> = shapes
            .iter()
            .filter(|s| matches!(s, Shape::Circle(_)))
            .collect();
        assert_eq!(circles.len(), 2, "两端各一个圆点");
    }

    #[test]
    fn dot_arrow_circle_sits_inside_the_segment() {
        // 圆心沿线段内缩一个半径，故圆点相切于端点而不是盖住它：
        // 起点 (0,0) → 终点 (100,0)，半径 = 线宽 2 × 1.5 = 3
        let mut d = open_line();
        d.start_arrow = Some(ArrowHeadStyle::Dot);
        d.end_arrow = Some(ArrowHeadStyle::Dot);
        let shapes = CleanStyler.build_shapes(
            &d,
            &StrokeStyle::default(),
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        let centers: Vec<Pos2> = shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Circle(c) => Some(c.center),
                _ => None,
            })
            .collect();
        assert_eq!(centers.len(), 2);
        assert!(
            (centers[0].x - 3.0).abs() < 1e-3,
            "起点圆心内缩: {:?}",
            centers[0]
        );
        assert!(
            (centers[1].x - 97.0).abs() < 1e-3,
            "终点圆心内缩: {:?}",
            centers[1]
        );
    }

    #[test]
    fn rough_styler_curved_polyline_is_smooth_and_deterministic() {
        let stroke = StrokeStyle::default();
        let d = curved_open_line();
        let a = RoughStyler.build_shapes(&d, &stroke, None, FillStyle::Solid, &identity(), 1.0);
        let b = RoughStyler.build_shapes(&d, &stroke, None, FillStyle::Solid, &identity(), 1.0);
        assert_eq!(debug(&a), debug(&b), "同 seed 必须得到同一抖动");
        // 开放曲线：段数 = (采样点数 - 1) × passes
        let sampled = outline_points(&d, 24).len();
        assert_eq!(a.len(), (sampled - 1) * RoughStyler::PASSES);
        assert!(a.iter().all(|s| matches!(s, Shape::CubicBezier(_))));
        for bez in beziers(&a) {
            for p in bez {
                assert!(p.x.is_finite() && p.y.is_finite(), "NaN/Inf 坐标: {p:?}");
            }
        }
    }

    #[test]
    fn rough_styler_curved_polyline_endpoints_stay_put() {
        // 开放 Catmull-Rom 的端点用重复点外推，首末点不得被邻居拉偏
        let d = curved_open_line();
        let shapes = RoughStyler.build_shapes(
            &d,
            &StrokeStyle::default(),
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        let bez = beziers(&shapes);
        let first = bez[0][0];
        let last = bez[bez.len() - 1][3];
        // 抖动幅度上限 8 画布像素（zoom = 1），端点只在该范围内漂移
        assert!((first.x - 0.0).abs() < 8.0 && (first.y - 40.0).abs() < 8.0);
        assert!((last.x - 100.0).abs() < 8.0 && (last.y - 40.0).abs() < 8.0);
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

        let rough = base.with_sloppiness(Sloppiness::Artist);
        let sketched = build_shape_visuals(&rough.kind, &identity(), 1.0);
        assert_eq!(sketched.len(), 8, "开启手绘 → RoughStyler 抖动边");
    }

    #[test]
    fn build_shape_visuals_returns_empty_for_non_shape() {
        let txt = Item::new_text("x".to_string(), 0.0, 0.0, 16.0, [255; 4]);
        assert!(build_shape_visuals(&txt.kind, &identity(), 1.0).is_empty());
    }
}
