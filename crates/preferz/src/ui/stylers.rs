use eframe::egui::{self, Color32, Pos2, Shape};
use preferz_core::item::{
    catmull_rom_polyline, orthogonal_outline_segments, round_orthogonal_corners, roundness_radius,
    ItemKind, ItemLocalSpace, OutlineSeg, CURVE_SAMPLES, ROUNDED_CORNER_SEGMENTS,
};
use preferz_core::shape::{
    ArrowHeadStyle, CurveType, DashStyle, ElbowAxis, FillStyle, SeededRng, ShapeType, Sloppiness,
    StrokeStyle,
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
    /// elbow 中间 bar 交叉轴偏移（plan #16 E1 / #24）。仅 `ShapeType::Elbow`
    /// 消费；与命中/导出经同一 `elbow_polyline_offset` 同源。
    pub elbow_mid_offset: f32,
    /// elbow 取向滞回状态（plan #21 DP-B）。`None` = 旧档按几何推断；
    /// 与命中经同一 `elbow_axis_effective` 解析。
    pub elbow_axis: Option<ElbowAxis>,
    /// elbow 完整派生路由（局部坐标，`Scene::elbow_route_local` 产物）。仅
    /// `ShapeType::Elbow` 消费；非 elbow 恒空。渲染与命中/手柄/导出同源
    /// （plan #24 阶段 C：调用方注入，本模块不再自行派生）。
    pub elbow_route: Vec<(f32, f32)>,
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

/// RGBA → egui 颜色。
///
/// egui painter 使用**预乘 alpha** 语义：存储的 (255,0,0,128) 若按非预乘构建，
/// 红色分量 255 > alpha 128，混合结果接近不透明（用户反馈"填充 50% 看不出透明"）。
/// 此处显式预乘 RGB 后交给 egui。
fn color_from(c: [u8; 4]) -> Color32 {
    let a = c[3] as u32;
    Color32::from_rgba_premultiplied(
        (c[0] as u32 * a / 255) as u8,
        (c[1] as u32 * a / 255) as u8,
        (c[2] as u32 * a / 255) as u8,
        c[3],
    )
}

fn to_pos2(p: euclid::Point2D<f32, ScreenSpace>) -> Pos2 {
    egui::pos2(p.x, p.y)
}

/// 顺时针旋转屏幕向量 angle（弧度）。
fn rotate_vec2(v: egui::Vec2, angle: f32) -> egui::Vec2 {
    let (s, c) = angle.sin_cos();
    egui::vec2(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// 矩形族的圆角半径（局部坐标）：`min(w, h) × roundness × 0.5`，且不超过短边的一半。
///
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
/// `RoughStyler` 用 rough.js `generateEllipseParams` 的自适应段数。
/// `ellipse_phase` 为椭圆采样起始角（弧度）：`CleanStyler` 恒 0，
/// `RoughStyler` 按 rough.js `radOffset` 给随档位缩放的随机相位。
///
/// 两处会改写点列（Phase I）：
/// - 矩形 `roundness > 0` → 四角换成圆弧采样点；
/// - Polyline + `Curved` → 经 `catmull_rom_polyline` 插值（与命中测试同源）。
fn outline_points(
    shape: &ShapeData,
    ellipse_segments: usize,
    ellipse_phase: f32,
) -> Vec<(f32, f32)> {
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
                    let a =
                        ellipse_phase + i as f32 / ellipse_segments as f32 * std::f32::consts::TAU;
                    (cx + rx * a.cos(), cy + ry * a.sin())
                })
                .collect()
        }
        ShapeType::Polyline => {
            // Phase I：`Curved` 经 Catmull-Rom 插值成折线（与命中测试同源，
            // 故"看着在曲线上"的点一定点得中）。两点曲线无中间控制点，插值无意义。
            match shape.curve_type {
                CurveType::Curved => {
                    catmull_rom_polyline(&shape.points, shape.closed, CURVE_SAMPLES)
                }
                CurveType::Straight => shape.points.clone(),
            }
        }
        // elbow 连接器（plan #24）：路径 = `elbow_polyline_offset` 从两端点 + 偏移
        // + 有效取向（滞回解析，plan #21 DP-B）推导（与命中测试同源）；倒角
        // （plan #23）与矩形族共用 roundness 字段与半径语义，派生路径的每个直角
        // 拐角换圆弧采样。
        ShapeType::Elbow => {
            // 路由 = 调用方注入的完整派生路径（plan #24 阶段 C 同源不变量）。
            let base = shape.elbow_route.clone();
            let r = roundness_radius(shape.base_size, shape.roundness);
            if r > 1e-3 {
                round_orthogonal_corners(&base, false, r)
            } else {
                base
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

/// 矩形族恒闭合；Polyline 由 `closed` 字段决定；Elbow 连接器恒开放。
fn is_closed(shape: &ShapeData) -> bool {
    !matches!(shape.shape_type, ShapeType::Polyline | ShapeType::Elbow) || shape.closed
}

/// 追加起/终点箭头（四样式：Arrow / Triangle / TriangleOutline / Dot，样式表见
/// [`push_one_arrowhead`]）。
///
/// 尺寸基准（[`ArrowHeadStyle::HEAD_LEN_MULT`] / [`ArrowHeadStyle::HALF_ANGLE`]）：
/// 头长 = 线宽 × 4，半张角 30°（总张开 60°）。
///
/// 本函数画**规整**箭头（`CleanStyler` 用）；`RoughStyler` 走
/// [`RoughStyler::push_arrow_heads_rough`]——DP3 拍板跟随 Excalidraw：箭头是
/// rough polygon 会抖（此前注释"Excalidraw 箭头保持规整"与事实不符，已更正）。
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

    // 起点箭头：尖端在起点，`inward` 指向线段体内。
    let start_dir = pts[1] - pts[0];
    if start_dir.length() > 1e-3 {
        push_one_arrowhead(
            out,
            pts[0],
            start_dir / start_dir.length(),
            start_arrow,
            egui_stroke,
            line_width,
        );
    }

    // 终点箭头：尖端在终点，`inward` = -outward（指向线段体内）。
    let last = pts[pts.len() - 1];
    let end_dir = last - pts[pts.len() - 2];
    if end_dir.length() > 1e-3 {
        push_one_arrowhead(
            out,
            last,
            -(end_dir / end_dir.length()),
            end_arrow,
            egui_stroke,
            line_width,
        );
    }
}

/// 空心三角（TriangleOutline）端头的主线裁剪：线端内收到三角底边中点，
/// 避免线穿入三角内部（V 箭头尖端锚在线端、实心三角 / 圆点盖住线端，无需裁剪）。
/// 起点 / 终点按各自端头样式独立处理；裁剪量 = 头长在线方向上的投影
/// （头长 × cos30°），且不超过该段长度的一半——极短线宁可重叠也不把线裁没了。
fn trimmed_line_points(
    pts: &[Pos2],
    start: Option<ArrowHeadStyle>,
    end: Option<ArrowHeadStyle>,
    line_width: f32,
) -> Vec<Pos2> {
    let mut out = pts.to_vec();
    if out.len() < 2 {
        return out;
    }
    let trim = line_width * ArrowHeadStyle::HEAD_LEN_MULT * ArrowHeadStyle::HALF_ANGLE.cos();
    if start == Some(ArrowHeadStyle::TriangleOutline) {
        let d = out[1] - out[0];
        let len = d.length();
        if len > 1e-3 {
            out[0] += d / len * trim.min(len * 0.5);
        }
    }
    if end == Some(ArrowHeadStyle::TriangleOutline) {
        let n = out.len();
        let d = out[n - 1] - out[n - 2];
        let len = d.length();
        if len > 1e-3 {
            out[n - 1] -= d / len * trim.min(len * 0.5);
        }
    }
    out
}

/// 单端箭头（四样式）：`inward` 为尖端指向线段体内的单位向量。
///
/// - Arrow：V 形两翼，半张角 30°（总张开 60°）
/// - Triangle：描边色实心三角
/// - TriangleOutline：空心三角（仅描边不填充；主线经 [`trimmed_line_points`]
///   裁到三角底边，不穿入三角）
/// - Dot：实心圆点，半径 = 线宽 × [`ArrowHeadStyle::DOT_RADIUS_MULT`]，
///   圆心内缩一个半径**相切于端点**
fn push_one_arrowhead(
    out: &mut Vec<Shape>,
    tip: Pos2,
    inward: egui::Vec2,
    style: Option<ArrowHeadStyle>,
    egui_stroke: egui::Stroke,
    line_width: f32,
) {
    let Some(style) = style else { return };
    let half = ArrowHeadStyle::HALF_ANGLE; // 30°
    let head_len = line_width * ArrowHeadStyle::HEAD_LEN_MULT;
    match style {
        ArrowHeadStyle::Arrow => {
            let a1 = rotate_vec2(inward, half);
            let a2 = rotate_vec2(inward, -half);
            out.push(Shape::line(vec![tip, tip + a1 * head_len], egui_stroke));
            out.push(Shape::line(vec![tip, tip + a2 * head_len], egui_stroke));
        }
        ArrowHeadStyle::Triangle => {
            let b1 = tip + rotate_vec2(inward, half) * head_len;
            let b2 = tip + rotate_vec2(inward, -half) * head_len;
            out.push(closed_filled_path(vec![tip, b1, b2], egui_stroke.color));
        }
        ArrowHeadStyle::TriangleOutline => {
            let b1 = tip + rotate_vec2(inward, half) * head_len;
            let b2 = tip + rotate_vec2(inward, -half) * head_len;
            out.push(closed_stroked_path(
                vec![tip, b1, b2],
                egui::epaint::PathStroke::new(line_width, egui_stroke.color),
            ));
        }
        ArrowHeadStyle::Dot => {
            let radius = (line_width * ArrowHeadStyle::DOT_RADIUS_MULT).max(1.0);
            out.push(Shape::circle_filled(
                tip + inward * radius,
                radius,
                egui_stroke.color,
            ));
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

/// 斜线填充的默认角度（度，屏幕坐标系）。
///
/// rough.js 的 `hachureAngle` 默认 -41 会在 `scan-line-hachure.ts` 里 **+90** 后
/// 使用（有效 49° 仰角）；本常量直接取换算后的线方向，故为 -49。
/// cross-hatch 第二组 +90 → +41，与 rough.js 两组方向一致。
const HACHURE_ANGLE_DEG: f32 = -49.0;

/// 斜线填充的行距：rough.js 同款 `hachureGap = 4 × 线宽`，gap 下限按
/// rough.js `round(max(gap, 0.1))`（plan #20 批次3，取代旧的 4px 下限）。
fn hachure_gap(stroke_width: f32, zoom: f32) -> f32 {
    (stroke_width * 4.0).max(0.1).round() * zoom
}

/// 生成沿 `angle_deg` 方向的斜线填充线段（屏幕空间）。
///
/// 算法：把多边形旋转 `-angle`，使填充线方向变为水平；对每条水平扫描线求与
/// 多边形各边的交点横坐标，排序后两两配对（穿入/穿出交替），再旋回。
/// 对凸/凹简单多边形均适用（凹多边形一条扫描线可得 >2 个交点，配对后为多段）。
///
/// `skip_first_line`：rough.js `scan-line-hachure.ts` 在 roughness≥1 时约 30% 概率
/// 把扫描起点后移一整行（"跳首线"的随机相位）；扫描步距恒为 `gap`，故跳首线的
/// 结果恰等于不跳时的线表去掉第一条。
fn hachure_segments(
    pts: &[Pos2],
    angle_deg: f32,
    gap: f32,
    skip_first_line: bool,
) -> Vec<[Pos2; 2]> {
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
    let mut y = ymin + gap * 0.5 + if skip_first_line { gap } else { 0.0 };
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

/// 斜线段列表 → egui 形状（颜色即填充色）。
///
/// 填充线宽取描边的一半（Excalidraw `fillWeight = strokeWidth / 2`，
/// plan #20 批次3；调用方传 `line_width * 0.5`）。
fn hachure_shapes(
    pts: &[Pos2],
    angle_deg: f32,
    gap: f32,
    line_width: f32,
    color: Color32,
) -> Vec<Shape> {
    let stroke = egui::Stroke::new(line_width, color);
    hachure_segments(pts, angle_deg, gap, false)
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
        let mut pts = to_screen_points(
            &outline_points(shape, Self::ELLIPSE_SEGMENTS, 0.0),
            to_screen,
        );
        if pts.len() < 2 {
            return Vec::new();
        }

        let stroke_color = color_from(stroke.color);
        let line_width = stroke.width * zoom;
        let egui_stroke = egui::Stroke::new(line_width, stroke_color);
        let closed = is_closed(shape);
        let fill_width = line_width * 0.5; // Excalidraw fillWeight = strokeWidth / 2

        // 开放折线：直接画线 + 箭头，无填充概念。空心三角端头把主线裁到三角底边
        //（见 [`trimmed_line_points`]），箭头几何仍用原始端点。
        if !closed {
            let mut out = Vec::new();
            let line_pts =
                trimmed_line_points(&pts, shape.start_arrow, shape.end_arrow, line_width);
            match stroke.dash {
                DashStyle::Solid => out.push(Shape::line(line_pts, egui_stroke)),
                DashStyle::Dashed | DashStyle::Dotted => {
                    let (d, g) = dash_lengths(stroke.dash, zoom, line_width);
                    out.extend(Shape::dashed_line(&line_pts, egui_stroke, d, g));
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
                    hachure_gap(stroke.width, zoom),
                    fill_width,
                    fill_color,
                )),
                FillStyle::CrossHatch => {
                    for angle in [HACHURE_ANGLE_DEG, HACHURE_ANGLE_DEG + 90.0] {
                        out.extend(hachure_shapes(
                            &pts,
                            angle,
                            hachure_gap(stroke.width, zoom),
                            fill_width,
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
/// [`RoughStyler::outline_kind`] 的三路渲染路线。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutlineKind {
    /// 采样间距均匀的曲线轮廓（椭圆 / Curved 折线）：整圈抖动 + Catmull-Rom。
    Smooth,
    /// 圆角矩形 / 倒角 elbow："直边 + 单条角弧"分段（core `OutlineSeg`）。
    Segmented,
    /// 纯直边轮廓：逐边抖动。
    Edged,
}

pub struct RoughStyler;

impl RoughStyler {
    /// 每条边重复描边的次数（rough.js 为 2，形成双线笔触）。
    const PASSES: usize = 2;
    /// 弓形系数：控制贝塞尔控制点垂直于边的位移强度。
    const BOWING: f32 = 1.0;
    /// rough.js `maxRandomnessOffset` 默认值（画布像素）：underlay 端点/控制点
    /// 抖动幅度基准（plan #20 批次1，取代旧的 `min(边长6%, 8px)`）。
    const MAX_RANDOMNESS_OFFSET: f32 = 2.0;
    /// rough.js `curveStepCount` 默认值：椭圆自适应采样数的下限。
    const CURVE_STEP_COUNT: f32 = 9.0;
    /// dash 模式下把每条贝塞尔采样为折线的点数。
    const DASH_SAMPLES: usize = 16;

    /// rough.js `_line` 的 `roughnessGain`：按**画布**边长衰减——<200 → 1、
    /// >500 → 0.4、中间线性（plan #20 批次1）。
    fn roughness_gain(len_canvas: f32) -> f32 {
        if len_canvas < 200.0 {
            1.0
        } else if len_canvas > 500.0 {
            0.4
        } else {
            -0.0016668 * len_canvas + 1.233334
        }
    }

    /// Excalidraw `adjustRoughness` 的小图衰减系数（与 amp_scale **相乘**，DP4）。
    /// max 边 <10 → ÷3、<20 → ÷2；三种例外不衰减（与 Excalidraw 同条件）：
    /// 两边都够大（min≥20 且 max≥50）、带圆角的矩形（min≥15）、线性元素够长（max≥50）。
    fn small_size_roughness_scale(shape: &ShapeData) -> f32 {
        let (w, h) = shape.base_size;
        let (max_s, min_s) = (w.max(h), w.min(h));
        let round_eligible =
            matches!(shape.shape_type, ShapeType::Rectangle) && shape.roundness > 0.0;
        if (min_s >= 20.0 && max_s >= 50.0)
            || (min_s >= 15.0 && round_eligible)
            || (matches!(shape.shape_type, ShapeType::Polyline | ShapeType::Elbow) && max_s >= 50.0)
        {
            return 1.0;
        }
        if max_s < 10.0 {
            1.0 / 3.0
        } else {
            0.5
        }
    }

    /// rough.js `generateEllipseParams` 的椭圆采样段数（`curveStepCount`=9、
    /// Excalidraw 对 ellipse 设 `curveFitting=1` 故半径不加随机抖动）：
    /// `ceil(max(9, 9/√200 · √(2π·√((rx²+ry²)/2))))`，小圆 9 段、大圆按周长增长。
    fn ellipse_step_count(w: f32, h: f32) -> usize {
        let (rx, ry) = (w * 0.5, h * 0.5);
        let psq = (std::f32::consts::TAU * ((rx * rx + ry * ry) * 0.5).sqrt()).sqrt();
        (Self::CURVE_STEP_COUNT.max(Self::CURVE_STEP_COUNT / 200.0f32.sqrt() * psq)).ceil() as usize
    }

    /// 轮廓渲染路线（2026-10-01 重构，对齐 rough.js/Excalidraw）：
    ///
    /// - [`OutlineKind::Smooth`]：采样间距均匀的曲线轮廓（椭圆、Curved 折线）——
    ///   整圈抖动 + Catmull-Rom 平滑连线，采样点等距故控制柄不会过冲；
    /// - [`OutlineKind::Segmented`]：圆角矩形 / 倒角 elbow——core
    ///   `orthogonal_outline_segments` 拆成"直边 + 单条角弧"分段渲染（Excalidraw
    ///   圆角矩形即 `path()` 的 4×L + 4×Q，`shape.ts:776`）。此前圆角也走 Smooth：
    ///   直边 1 长段 + 弧 8 短段喂均匀 CR，交界点控制柄被直边撑到弧段长的数倍，
    ///   甩出鼓包/回头环（2026-10-01 验收反馈：断裂/跳动，小尺寸尤甚）；
    /// - [`OutlineKind::Edged`]：纯直边轮廓（矩形 / 菱形 / Straight 折线 /
    ///   无倒角 elbow），逐边抖动。
    fn outline_kind(shape: &ShapeData) -> OutlineKind {
        let rounded = roundness_radius(shape.base_size, shape.roundness) > 1e-3;
        match shape.shape_type {
            ShapeType::Ellipse => OutlineKind::Smooth,
            ShapeType::Rectangle if rounded => OutlineKind::Segmented,
            ShapeType::Polyline if matches!(shape.curve_type, CurveType::Curved) => {
                OutlineKind::Smooth
            }
            ShapeType::Elbow if rounded => OutlineKind::Segmented,
            _ => OutlineKind::Edged,
        }
    }

    /// 分段轮廓的局部坐标段列表（圆角矩形 / 倒角 elbow）。
    ///
    /// 与命中测试同源：elbow 的基础折线来自 `elbow_polyline_offset`，圆角半径
    /// 语义与矩形族一致（`roundness_radius`），拐角推导全部在 core 完成。
    fn outline_segments(shape: &ShapeData) -> Vec<OutlineSeg> {
        let (w, h) = shape.base_size;
        let r = roundness_radius((w, h), shape.roundness);
        match shape.shape_type {
            ShapeType::Rectangle => {
                orthogonal_outline_segments(&[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)], true, r)
            }
            ShapeType::Elbow => orthogonal_outline_segments(&shape.elbow_route, false, r),
            _ => Vec::new(),
        }
    }

    /// 整圈抖动：给每个采样点加独立偏移，得到"手抖画歪"的轮廓点环。
    ///
    /// 与逐边抖动（[`RoughStyler::sketch_edge`]）的区别是——这里先抖动顶点、
    /// 再用光滑曲线穿过它们，因此曲线类轮廓（椭圆）不会出现直线段拼接的折角。
    /// 仅用于**采样间距均匀**的轮廓（椭圆 / Curved 折线）：圆角矩形已于
    /// 2026-10-01 改走"直边 + 角弧"分段渲染——间距不均的点环会让均匀
    /// Catmull-Rom 在直边↔弧交界处控制柄过冲甩鼓包。
    ///
    /// 幅度逐点取 `min(rough_amp, 0.35 × 较短相邻段)`：全局平均间距会在
    /// "局部密集 + 局部稀疏"的轮廓上失守，相邻点仍可能被推过邻居打成结。
    /// 0.35 ≈ 0.5/√2：x/y 两轴独立抽样时欧氏位移可达 amp×√2，
    /// 收紧后相邻两点位移之和恒小于段长，结构上不可能互越。
    fn jitter_points(rng: &mut SeededRng, pts: &[Pos2], closed: bool, rough_amp: f32) -> Vec<Pos2> {
        let n = pts.len();
        pts.iter()
            .enumerate()
            .map(|(i, p)| {
                let prev = if i > 0 || closed {
                    Some(pts[(i + n - 1) % n])
                } else {
                    None
                };
                let next = if i + 1 < n || closed {
                    Some(pts[(i + 1) % n])
                } else {
                    None
                };
                let local = match (prev, next) {
                    (Some(a), Some(b)) => (*p - a).length().min((b - *p).length()) * 0.35,
                    (Some(a), None) | (None, Some(a)) => (*p - a).length() * 0.35,
                    (None, None) => rough_amp,
                };
                let amp = rough_amp.min(local);
                *p + egui::vec2(rng.signed() * amp, rng.signed() * amp)
            })
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

    /// 曲线（椭圆 / Curved 折线）采样点的抖动幅度（屏幕像素）。
    ///
    /// 基准 = rough.js `curve()` 的逐点偏移 `(1+amp_scale×0.2)×amp_scale × zoom`——
    /// 与曲线尺寸/采样密度无关。另加**采样间距上限**（≤ 相邻采样点平均间距的一半）：
    /// 相邻点距只有几像素时，大幅独立抖动会让相邻点互越、再被 Catmull-Rom 放大
    /// 成乱线小环；间距上限从结构上杜绝自交。
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
        let avg_spacing = perimeter / seg_count as f32;
        let rough = (1.0 + amp_scale * 0.2) * amp_scale * zoom;
        rough.min(avg_spacing * 0.5)
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
    /// 公式对齐 rough.js `_line`（plan #20 批次1/2）：抖动幅度 =
    /// `maxRandomnessOffset(2 画布px) × roughnessGain(边长) × amp_scale × zoom`，
    /// 边长 <20 画布px 时衰减为 `len/10`；bowing 幅度 = `2·len/200 × gain × amp_scale`
    /// （不饱和），**符号随机**；`ctx.preserve_vertices`（Excalidraw 同名选项，
    /// Architect/Artist 档生效）时端点不抖。
    ///
    /// 退化边（长度 ≈ 0）直接返回零抖动直线，避免除以 0 得到 NaN。
    fn sketch_edge(rng: &mut SeededRng, a: Pos2, b: Pos2, ctx: &RoughCtx) -> [Pos2; 4] {
        let d = b - a;
        let len = d.length();
        if len < 1e-3 {
            return [a, a, b, b];
        }

        let zoom = ctx.zoom;
        let amp_scale = ctx.amp_scale;
        let preserve_vertices = ctx.preserve_vertices;
        let len_canvas = len / zoom;
        let gain = Self::roughness_gain(len_canvas);
        let mut offset = Self::MAX_RANDOMNESS_OFFSET;
        if offset * offset * 100.0 > len_canvas * len_canvas {
            offset = len_canvas / 10.0;
        }
        let max_offset = offset * gain * amp_scale * zoom;

        // 弓形位移：垂直于边，rough.js 不饱和（/200 线性），符号+幅度随机
        let bow = Self::BOWING * Self::MAX_RANDOMNESS_OFFSET * len_canvas / 200.0
            * gain
            * amp_scale
            * zoom;
        let mid_disp = (egui::vec2(-d.y, d.x) / len) * bow * rng.signed();

        // 控制点沿边的位置（0.2~0.4），rough.js 的 divergePoint。
        let diverge = 0.2 + rng.next_f32() * 0.2;
        let jitter =
            |rng: &mut SeededRng| egui::vec2(rng.signed() * max_offset, rng.signed() * max_offset);

        let p0 = if preserve_vertices {
            a
        } else {
            a + jitter(rng)
        };
        let p3 = if preserve_vertices {
            b
        } else {
            b + jitter(rng)
        };
        let m1 = a + d * diverge;
        let m2 = a + d * (2.0 * diverge);
        let c1 = m1 + mid_disp + jitter(rng);
        let c2 = m2 + mid_disp + jitter(rng);
        [p0, c1, c2, p3]
    }

    /// 圆角的"单条角弧"贝塞尔：二次近似弧（控制点 = 直角顶点 `vertex`）转三次，
    /// **端点钉死、只抖控制点**——对齐 rough.js `_bezierTo`（Excalidraw 的圆角
    /// 矩形/倒角 elbow 是连续 path，`preserveVertices` 恒真，`shape.ts:223`）：
    /// 曲线只在顶点附近轻微鼓出，与相邻直边在切点处共用端点坐标，连接无缝。
    /// 此前整圈独立抖动 + 均匀 CR 会在此处过冲甩鼓包（2026-10-01 验收反馈）。
    ///
    /// 两笔 pass 幅度按 rough.js 取 `maxRandomnessOffset` 与 `+0.3`（overlay 略大）；
    /// `_bezierTo` 不乘 `roughnessGain`（短弧不衰减——小尺寸由
    /// `small_size_roughness_scale` 统一兜底）。
    fn corner_bezier(
        rng: &mut SeededRng,
        p_in: Pos2,
        vertex: Pos2,
        p_out: Pos2,
        ctx: &RoughCtx,
        pass: usize,
    ) -> [Pos2; 4] {
        let base = Self::MAX_RANDOMNESS_OFFSET + if pass > 0 { 0.3 } else { 0.0 };
        let amp = base * ctx.amp_scale * ctx.zoom;
        let c1 = p_in
            + (vertex - p_in) * (2.0 / 3.0)
            + egui::vec2(rng.signed() * amp, rng.signed() * amp);
        let c2 = p_out
            + (vertex - p_out) * (2.0 / 3.0)
            + egui::vec2(rng.signed() * amp, rng.signed() * amp);
        [p_in, c1, c2, p_out]
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

    /// 手绘风箭头：走 [`RoughStyler::sketch_edge`] 完整双线抖动
    /// （plan #20 批次4，DP3 拍板跟随 Excalidraw——其箭头是 rough polygon，
    /// roughness 封顶 `min(1, roughness)`，见 `shape.ts:339/422`）。
    /// Dot 端头保持规整实心圆。
    fn push_arrow_heads_rough(
        out: &mut Vec<Shape>,
        pts: &[Pos2],
        ctx: &RoughCtx,
        rng: &mut SeededRng,
        start_arrow: Option<ArrowHeadStyle>,
        end_arrow: Option<ArrowHeadStyle>,
    ) {
        if pts.len() < 2 {
            return;
        }
        let amp_scale = ctx.amp_scale.min(1.0);
        let wing_ctx = RoughCtx {
            zoom: ctx.zoom,
            amp_scale,
            preserve_vertices: ctx.preserve_vertices,
            line_width: ctx.line_width,
            stroke_color: ctx.stroke_color,
        };
        let path_stroke = egui::epaint::PathStroke::new(ctx.line_width, ctx.stroke_color);
        let mut push_seg = |out: &mut Vec<Shape>, a: Pos2, b: Pos2| {
            for _ in 0..Self::PASSES {
                let bez = Self::sketch_edge(rng, a, b, &wing_ctx);
                out.push(Shape::CubicBezier(
                    egui::epaint::CubicBezierShape::from_points_stroke(
                        bez,
                        false,
                        Color32::TRANSPARENT,
                        path_stroke.clone(),
                    ),
                ));
            }
        };

        // 起点箭头：`inward` 指向线段体内。
        let start_dir = pts[1] - pts[0];
        if start_dir.length() > 1e-3 {
            Self::push_one_arrowhead_rough(
                &mut push_seg,
                out,
                pts[0],
                start_dir / start_dir.length(),
                start_arrow,
                ctx,
            );
        }

        // 终点箭头：`inward` = -outward。修复：此前翅膀沿 +outward 伸出线外，
        // 导致 V 尖朝线内、箭头方向反了（CleanStyler 一直是对的）。
        let last = pts[pts.len() - 1];
        let end_dir = last - pts[pts.len() - 2];
        if end_dir.length() > 1e-3 {
            Self::push_one_arrowhead_rough(
                &mut push_seg,
                out,
                last,
                -(end_dir / end_dir.length()),
                end_arrow,
                ctx,
            );
        }
    }

    /// 单端手绘箭头（四样式），几何与 CleanStyler 的
    /// [`push_one_arrowhead`] 同源（尖端 + 内向单位向量）。Dot 端头保持规整实心圆。
    fn push_one_arrowhead_rough(
        push_seg: &mut dyn FnMut(&mut Vec<Shape>, Pos2, Pos2),
        out: &mut Vec<Shape>,
        tip: Pos2,
        inward: egui::Vec2,
        style: Option<ArrowHeadStyle>,
        ctx: &RoughCtx,
    ) {
        let Some(style) = style else { return };
        let half = ArrowHeadStyle::HALF_ANGLE; // 30°
        let head_len = ctx.line_width * ArrowHeadStyle::HEAD_LEN_MULT;
        match style {
            ArrowHeadStyle::Arrow => {
                push_seg(out, tip, tip + rotate_vec2(inward, half) * head_len);
                push_seg(out, tip, tip + rotate_vec2(inward, -half) * head_len);
            }
            ArrowHeadStyle::Triangle => {
                let b1 = tip + rotate_vec2(inward, half) * head_len;
                let b2 = tip + rotate_vec2(inward, -half) * head_len;
                // 实心三角：描边色填充 + 三条 rough 边
                out.push(closed_filled_path(vec![tip, b1, b2], ctx.stroke_color));
                push_seg(out, tip, b1);
                push_seg(out, tip, b2);
                push_seg(out, b1, b2);
            }
            ArrowHeadStyle::TriangleOutline => {
                let b1 = tip + rotate_vec2(inward, half) * head_len;
                let b2 = tip + rotate_vec2(inward, -half) * head_len;
                // 空心三角：只描三条 rough 边，不填充
                push_seg(out, tip, b1);
                push_seg(out, tip, b2);
                push_seg(out, b1, b2);
            }
            ArrowHeadStyle::Dot => {
                let radius = (ctx.line_width * ArrowHeadStyle::DOT_RADIUS_MULT).max(1.0);
                out.push(Shape::circle_filled(
                    tip + inward * radius,
                    radius,
                    ctx.stroke_color,
                ));
            }
        }
    }
}

/// `RoughStyler` 单次 build 的共享派生参数（避免逐函数长参数列）。
#[derive(Clone, Copy)]
struct RoughCtx {
    zoom: f32,
    /// `amp_scale() × adjustRoughness` 小图衰减（DP4 相乘）。
    amp_scale: f32,
    /// Excalidraw `preserveVertices`：Architect/Artist 端点不抖。
    preserve_vertices: bool,
    line_width: f32,
    stroke_color: Color32,
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
        // Excalidraw generateRoughOptions：非实线 dash → disableMultiStroke（单笔）
        // + strokeWidth+0.5 补回观感粗细（plan #20 批次4）；fillWeight/hachureGap
        // 显式独立计算，不跟随加宽后的描边。
        let solid_dash = stroke.dash == DashStyle::Solid;
        let passes = if solid_dash { Self::PASSES } else { 1 };
        let line_width = (stroke.width + if solid_dash { 0.0 } else { 0.5 }) * zoom;
        let closed = is_closed(shape);
        let mut out = Vec::new();

        let ctx = RoughCtx {
            zoom,
            amp_scale: shape.sloppiness.amp_scale() * Self::small_size_roughness_scale(shape),
            // Excalidraw preserveVertices = roughness < cartoonist：Architect/Artist
            // 档端点不抖（plan #20 批次2；DP1 拍板维持 PReferZ 三档幅度映射）。
            preserve_vertices: shape.sloppiness != Sloppiness::Cartoonist,
            line_width,
            stroke_color: color_from(stroke.color),
        };

        // 椭圆：rough.js generateEllipseParams 自适应段数 + radOffset 随机起始相位
        let ellipse_segments = Self::ellipse_step_count(shape.base_size.0, shape.base_size.1);
        let ellipse_phase = if matches!(shape.shape_type, ShapeType::Ellipse) {
            SeededRng::new(shape.seed ^ 0x5EED_11C5).signed() * ctx.amp_scale * 0.5
        } else {
            0.0
        };
        let pts = to_screen_points(
            &outline_points(shape, ellipse_segments, ellipse_phase),
            to_screen,
        );
        if pts.len() < 2 {
            return Vec::new();
        }

        // 填充层：solid = 顶点抖动的实心多边形（DP2 拍板跟随 rough.js
        // solidFillPolygon：顶点 ±2 画布px × amp_scale）；hachure/cross-hatch =
        // 斜线段走 sketch_edge 完整双线抖动（rough.js hachure-filler 的
        // doubleLineOps），线宽减半（fillWeight = strokeWidth/2），roughness≥1 时
        // ~30% 概率跳首线（scan-line-hachure）。抖动用独立种子的 rng，与描边解耦。
        if closed {
            if let Some(f) = fill {
                match fill_style {
                    FillStyle::Solid => {
                        let mut solid_rng = SeededRng::new(shape.seed ^ 0x5011_DF11);
                        let amp = Self::MAX_RANDOMNESS_OFFSET * ctx.amp_scale * zoom;
                        let jittered: Vec<Pos2> = pts
                            .iter()
                            .map(|p| {
                                *p + egui::vec2(solid_rng.signed() * amp, solid_rng.signed() * amp)
                            })
                            .collect();
                        out.push(closed_filled_path(jittered, f));
                    }
                    FillStyle::Hachure | FillStyle::CrossHatch => {
                        let angles: &[f32] = match fill_style {
                            FillStyle::CrossHatch => &[HACHURE_ANGLE_DEG, HACHURE_ANGLE_DEG + 90.0],
                            _ => &[HACHURE_ANGLE_DEG],
                        };
                        let mut fill_rng = SeededRng::new(shape.seed ^ 0x6841_4355_4C4C_5F53); // "hACULL_S"
                        let skip_first = ctx.amp_scale >= 1.0 && fill_rng.next_f32() > 0.7;
                        let gap = hachure_gap(stroke.width, zoom);
                        let path_stroke = egui::epaint::PathStroke::new(line_width * 0.5, f);
                        for angle in angles {
                            let segs = hachure_segments(&pts, *angle, gap, skip_first);
                            for [a, b] in segs {
                                for _ in 0..Self::PASSES {
                                    let bez = Self::sketch_edge(&mut fill_rng, a, b, &ctx);
                                    out.push(Shape::CubicBezier(
                                        egui::epaint::CubicBezierShape::from_points_stroke(
                                            bez,
                                            false,
                                            Color32::TRANSPARENT,
                                            path_stroke.clone(),
                                        ),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut rng = SeededRng::new(shape.seed);

        // 开放折线：空心三角端头把主线裁到三角底边（见 [`trimmed_line_points`]），
        // 箭头几何仍用原始端点；闭合图形不裁。
        let stroke_pts = if closed {
            pts.clone()
        } else {
            trimmed_line_points(&pts, shape.start_arrow, shape.end_arrow, line_width)
        };

        match Self::outline_kind(shape) {
            OutlineKind::Smooth => {
                // 曲线类轮廓（椭圆、Curved 折线）：整圈抖动后连成光滑曲线。
                // 若沿用逐边直线抖动，采样段之间的折角会非常明显。
                let amp = Self::curve_jitter_amp(&stroke_pts, zoom, closed, ctx.amp_scale);
                for _ in 0..passes {
                    let jittered = Self::jitter_points(&mut rng, &stroke_pts, closed, amp);
                    for bez in Self::catmull_rom_beziers(&jittered, closed) {
                        Self::push_edge(&mut out, bez, stroke, line_width, ctx.stroke_color, zoom);
                    }
                }
            }
            OutlineKind::Segmented => {
                // 圆角矩形 / 倒角 elbow：直边 + 单条角弧分段渲染（对齐 Excalidraw
                // 的 `path()` 路线）。连续 path 的端点恒钉死（Excalidraw 对
                // continuousPath 不分档位，`shape.ts:223`），只有边/弧的控制点抖，
                // 相邻段共用端点坐标 → 连接处无缝（2026-10-01 验收反馈）。
                let segs = Self::outline_segments(shape);
                let seg_ctx = RoughCtx {
                    preserve_vertices: true,
                    ..ctx
                };
                let scr = |p: (f32, f32)| -> Pos2 {
                    to_pos2(to_screen.transform_point(euclid::Point2D::new(p.0, p.1)))
                };
                for pass in 0..passes {
                    for seg in &segs {
                        let bez = match *seg {
                            OutlineSeg::Line { a, b } => {
                                Self::sketch_edge(&mut rng, scr(a), scr(b), &seg_ctx)
                            }
                            OutlineSeg::Corner {
                                p_in,
                                vertex,
                                p_out,
                                ..
                            } => Self::corner_bezier(
                                &mut rng,
                                scr(p_in),
                                scr(vertex),
                                scr(p_out),
                                &seg_ctx,
                                pass,
                            ),
                        };
                        Self::push_edge(&mut out, bez, stroke, line_width, ctx.stroke_color, zoom);
                    }
                }
            }
            OutlineKind::Edged => {
                // 直线类轮廓（矩形 / 菱形 / 折线）：逐边抖动，闭合图形多一条 n-1 → 0 的收尾边。
                let seg_count = if closed {
                    stroke_pts.len()
                } else {
                    stroke_pts.len() - 1
                };
                for i in 0..seg_count {
                    let a = stroke_pts[i];
                    let b = stroke_pts[(i + 1) % stroke_pts.len()];
                    for _ in 0..passes {
                        let bez = Self::sketch_edge(&mut rng, a, b, &ctx);
                        Self::push_edge(&mut out, bez, stroke, line_width, ctx.stroke_color, zoom);
                    }
                }
            }
        }

        if !closed {
            Self::push_arrow_heads_rough(
                &mut out,
                &pts,
                &ctx,
                &mut rng,
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
pub fn build_shape_visuals(
    kind: &ItemKind,
    elbow_route: &[(f32, f32)],
    to_screen: &LocalToScreen,
    zoom: f32,
) -> Vec<Shape> {
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
        elbow_mid_offset,
        elbow_axis,
        roundness,
        seed,
        sloppiness,
        ..
    } = kind
    else {
        return Vec::new();
    };

    // 验收反馈 #4-1：闭合且首尾重合（自动闭合的"合并点"）视作一个点——去掉
    // 重复尾点，直线/手绘路径不再画零长收尾段（抖出一个脏点），曲线与 core
    // `catmull_rom_polyline` 的同款去重保持一致（命中与渲染同一条曲线）。
    let mut pts = points.clone();
    if *closed && pts.len() >= 2 && pts[0] == pts[pts.len() - 1] {
        pts.pop();
    }

    let data = ShapeData {
        shape_type: *shape_type,
        base_size: *base_size,
        points: pts,
        start_arrow: *start_arrow,
        end_arrow: *end_arrow,
        closed: *closed,
        curve_type: *curve_type,
        elbow_mid_offset: *elbow_mid_offset,
        elbow_axis: *elbow_axis,
        elbow_route: elbow_route.to_vec(),
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

/// 墨迹描边形状：屏幕中心线点 + 每点**屏幕**笔宽 → 逐段按端点均宽画描边线 +
/// 每点补一个圆帽（半径 = 该点半宽）。
///
/// **用描边而非填充 ribbon 轮廓**：填充闭合轮廓在笔迹弯曲/回环处两侧偏移边界会
/// 自交，被 epaint 非零环绕三角化填成一整块实心区域（正是「画出区域而非笔」的
/// 根因）；逐段描边结构上不可能填内部，且段缝/转角靠圆帽自然圆滑。速度锥形仍保留
/// （段宽取相邻两点均值）。终稿渲染与实时预览共用此函数。
pub fn freedraw_stroke_shapes(
    screen_pts: &[Pos2],
    screen_widths: &[f32],
    color: Color32,
) -> Vec<Shape> {
    let n = screen_pts.len();
    if n < 2 || screen_widths.len() != n {
        return Vec::new();
    }
    // 每点半宽（含最小可见宽度 0.4px，防细笔/收笔处消失）。
    let halves: Vec<f32> = screen_widths.iter().map(|&w| w.max(0.8) * 0.5).collect();
    let mut out = Vec::with_capacity(2 * n - 1);
    for i in 0..n - 1 {
        let w = halves[i] + halves[i + 1];
        out.push(Shape::line_segment(
            [screen_pts[i], screen_pts[i + 1]],
            egui::Stroke::new(w, color),
        ));
    }
    // 圆帽/圆角连接：每点补一个半径 = 该点半宽的实心圆，段缝与转角自然圆滑。
    for (&p, &h) in screen_pts.iter().zip(&halves) {
        out.push(Shape::circle_filled(p, h, color));
    }
    out
}

/// 墨迹（plan #10）终稿渲染：局部中心线点 + 逐点相对笔宽（pressures）+ 基准笔宽
/// （画布单位）→ 变换到屏幕，绝对笔宽 = `stroke_width × pressure × scale`（scale 含
/// item scale + 视口 zoom）→ 逐段描边。
pub fn build_freedraw_visuals(kind: &ItemKind, to_screen: &LocalToScreen) -> Vec<Shape> {
    let ItemKind::Freedraw {
        points,
        pressures,
        stroke_width,
        color,
    } = kind
    else {
        return Vec::new();
    };
    if points.len() < 2 || pressures.len() != points.len() {
        return Vec::new();
    }
    // 屏幕像素 / 局部单位：取 x 方向像元长度（含缩放，旋转下均匀故取其模长）。
    let v = to_screen.transform_vector(euclid::Vector2D::<f32, ItemLocalSpace>::new(1.0, 0.0));
    let scale = v.length().max(1e-4);
    let screen_pts = to_screen_points(points, to_screen);
    let base = stroke_width * scale;
    let screen_widths: Vec<f32> = pressures.iter().map(|pr| base * pr).collect();
    freedraw_stroke_shapes(&screen_pts, &screen_widths, color_from(*color))
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
            elbow_mid_offset: 0.0,
            elbow_axis: None,
            elbow_route: Vec::new(),
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
            elbow_mid_offset: 0.0,
            elbow_axis: None,
            elbow_route: Vec::new(),
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

    fn rounded_rect(seed: u64, size: (f32, f32), sloppiness: Sloppiness) -> ShapeData {
        ShapeData {
            shape_type: ShapeType::Rectangle,
            base_size: size,
            points: Vec::new(),
            start_arrow: None,
            end_arrow: None,
            closed: false,
            curve_type: CurveType::Straight,
            elbow_mid_offset: 0.0,
            elbow_axis: None,
            elbow_route: Vec::new(),
            roundness: 0.5,
            seed,
            sloppiness,
        }
    }

    #[test]
    fn rough_styler_rounded_rect_is_segmented_and_seamless() {
        let stroke = StrokeStyle::default();
        let d = rounded_rect(7, (100.0, 60.0), Sloppiness::Cartoonist);
        let shapes =
            RoughStyler.build_shapes(&d, &stroke, None, FillStyle::Solid, &identity(), 1.0);
        // 4 直边 + 4 角弧 = 8 段 × 2 passes = 16 条贝塞尔，无多余断段
        let bez = beziers(&shapes);
        assert_eq!(bez.len(), 16);
        // 同一 pass 内相邻段共享端点、环收回首段（接缝处零脱开——此前整圈
        // 独立抖动 + 均匀 CR 在交界处甩鼓包，2026-10-01 验收反馈）
        for pass in 0..2 {
            let segs = &bez[pass * 8..(pass + 1) * 8];
            for i in 0..8 {
                let end = segs[i][3];
                let next_start = segs[(i + 1) % 8][0];
                assert_eq!(end, next_start, "pass{pass} 段{i} 与下一段在接缝处脱开");
            }
        }
    }

    #[test]
    fn rough_styler_rounded_rect_junctions_match_ideal_outline() {
        // 端点钉死（preserveVertices 恒真）：所有段端点必须精确落在理想轮廓的
        // 8 个切点/顶点上，Cartoonist 也不例外（Excalidraw continuousPath 语义）
        let stroke = StrokeStyle::default();
        let d = rounded_rect(99, (100.0, 60.0), Sloppiness::Cartoonist);
        let shapes =
            RoughStyler.build_shapes(&d, &stroke, None, FillStyle::Solid, &identity(), 1.0);
        let r = 15.0_f32; // min(100,60) × 0.5 × 0.5
        let junctions: Vec<Pos2> = [
            (r, 0.0),
            (100.0 - r, 0.0),
            (100.0, r),
            (100.0, 60.0 - r),
            (100.0 - r, 60.0),
            (r, 60.0),
            (0.0, 60.0 - r),
            (0.0, r),
        ]
        .iter()
        .map(|(x, y)| egui::pos2(*x, *y))
        .collect();
        for bez in beziers(&shapes) {
            for p in [bez[0], bez[3]] {
                assert!(junctions.contains(&p), "段端点 {p:?} 不在理想切点上");
            }
        }
    }

    #[test]
    fn rough_styler_small_rounded_rect_corner_stays_near_ideal_outline() {
        // 回归：30×20 r=2 的小圆角矩形（旧方案零抖动下交界鼓包已达角半径 42%）。
        // 新方案角弧是单条贝塞尔：控制点抖幅 ≤ 2px × amp_scale，曲线偏离理想
        // 轮廓必须有界（旧方案 CR 控制柄被直边撑到弧段的 11 倍，无此上界）。
        let stroke = StrokeStyle::default();
        let d = rounded_rect(3, (30.0, 20.0), Sloppiness::Architect);
        let shapes =
            RoughStyler.build_shapes(&d, &stroke, None, FillStyle::Solid, &identity(), 1.0);
        let r = 5.0_f32; // min(30,20) × 0.5 × 0.5
        let centers = [(r, r), (30.0 - r, r), (30.0 - r, 20.0 - r), (r, 20.0 - r)];
        for bez in beziers(&shapes) {
            // 角弧段（起止端点都在同一角的两条切点上）采样 9 点测径向偏差
            let start = bez[0];
            let end = bez[3];
            let Some(center) = centers
                .iter()
                .map(|c| egui::pos2(c.0, c.1))
                .find(|c| (start - *c).length() - r <= 1e-3 && (end - *c).length() - r <= 1e-3)
            else {
                continue; // 直边段不测
            };
            for p in RoughStyler::sample_bezier(&bez, 8) {
                let dev = ((p - center).length() - r).abs();
                assert!(
                    dev <= 2.0 * 1.5 + 1.0,
                    "角弧偏离理想轮廓 {dev:.2}px（r={r}）：{p:?}"
                );
            }
        }
    }

    #[test]
    fn core_rounded_rect_segments_flatten_to_same_ring() {
        // 分段与点列扁平化必须一致（命中测试 / CleanStyler 依赖点环）：
        // 100×60 r=15 的闭合矩形环 = 4 角 × (SEG+1) 点，从角 0 切入点开始，
        // 相邻采样点间距有限（无跳变）。
        let ring = round_orthogonal_corners(
            &[(0.0, 0.0), (100.0, 0.0), (100.0, 60.0), (0.0, 60.0)],
            true,
            15.0,
        );
        assert_eq!(ring.len(), 4 * (ROUNDED_CORNER_SEGMENTS + 1));
        assert_eq!(ring[0], (0.0, 15.0), "环从角 0 切入点开始");
        for w in ring.windows(2) {
            let d = (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
            // 直边是单段长跳（最长 70px），弧段 ~6px；只要非零即无跳变断裂
            assert!(d > 0.0 && d < 80.0, "相邻采样点间距异常: {d}");
        }
    }

    #[test]
    fn rough_styler_fill_emits_jittered_solid_polygon() {
        let stroke = StrokeStyle::default();
        let mut d = rect(7);
        d.sloppiness = Sloppiness::Artist;
        let shapes = RoughStyler.build_shapes(
            &d,
            &stroke,
            Some(Color32::from_rgb(10, 20, 30)),
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        // 1 个实心多边形填充（DP2：顶点抖动，但仍是单个 Path）+ 8 条抖动边
        assert_eq!(shapes.len(), 9);
        assert!(matches!(shapes[0], Shape::Path(_)));
        // DP2：Artist 档（amp_scale 2.0）solid 填充顶点抖动 ≤ ±2 画布px × amp
        let Shape::Path(p) = &shapes[0] else {
            panic!("expected Path");
        };
        let corners = [
            egui::pos2(0.0, 0.0),
            egui::pos2(100.0, 0.0),
            egui::pos2(100.0, 60.0),
            egui::pos2(0.0, 60.0),
        ];
        for (actual, want) in p.points.iter().zip(corners) {
            assert!(
                (*actual - want).length() <= 2.0 * Sloppiness::Artist.amp_scale() + 1e-3,
                "solid 填充顶点抖动越界: {actual:?} vs {want:?}"
            );
        }
    }

    #[test]
    fn rough_styler_arrow_heads_are_sketched_with_capped_roughness() {
        let stroke = StrokeStyle::default();
        let shapes = RoughStyler.build_shapes(
            &open_line(),
            &stroke,
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        // 1 条边 × 2 passes + 终点箭头 2 翼 × 2 passes（DP3 箭头改抖）= 6，全为贝塞尔
        assert_eq!(shapes.len(), 6);
        assert!(shapes.iter().all(|s| matches!(s, Shape::CubicBezier(_))));
        // 翼尖仍锚在几何端点（Artist 档 preserveVertices：端点不抖）
        let bez = beziers(&shapes);
        let wing1 = bez[2];
        let wing2 = bez[4];
        assert!((wing1[0] - egui::pos2(100.0, 0.0)).length() < 1e-3);
        assert!((wing2[0] - egui::pos2(100.0, 0.0)).length() < 1e-3);
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
            elbow_mid_offset: 0.0,
            elbow_axis: None,
            elbow_route: Vec::new(),
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
        // 自适应段数（rough.js generateEllipseParams）× 2 passes；曲线轮廓不得退化成直线段拼接
        let segs = RoughStyler::ellipse_step_count(100.0, 100.0);
        assert_eq!(segs, 12);
        assert_eq!(shapes.len(), segs * 2);
        assert!(shapes.iter().all(|s| matches!(s, Shape::CubicBezier(_))));

        // 每条 pass 内部：相邻两段首尾重合且切线共线（C1 连续），否则会看到折角
        let bez = beziers(&shapes);
        for pass in bez.chunks(segs) {
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
    fn ellipse_step_count_follows_roughjs_generate_ellipse_params() {
        // 小圆取 curveStepCount 下限 9；Ø400 → 23；Ø800 → 32
        assert_eq!(RoughStyler::ellipse_step_count(10.0, 10.0), 9);
        assert_eq!(RoughStyler::ellipse_step_count(400.0, 400.0), 23);
        assert_eq!(RoughStyler::ellipse_step_count(800.0, 800.0), 32);
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
            elbow_mid_offset: 0.0,
            elbow_axis: None,
            elbow_route: Vec::new(),
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
            elbow_mid_offset: 0.0,
            elbow_axis: None,
            elbow_route: Vec::new(),
            roundness: 0.0,
            seed: 42,
            sloppiness: Sloppiness::Off,
        }
    }

    #[test]
    fn roundness_radius_is_clamped_to_half_of_short_side() {
        // 比例语义：radius = min(w, h) * roundness * 0.5（1.0 = 短边全圆弧）
        assert_eq!(roundness_radius((100.0, 40.0), 0.0), 0.0);
        assert!((roundness_radius((100.0, 40.0), 0.5) - 10.0).abs() < 1e-5);
        // 1.0 → 短边一半（两头的圆弧恰好相接），即最大圆角
        assert!((roundness_radius((100.0, 40.0), 1.0) - 20.0).abs() < 1e-5);
        assert!((roundness_radius((100.0, 40.0), 0.25) - 5.0).abs() < 1e-5);
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

        let curved_pts = outline_points(&curved, 64, 0.0);
        let straight_pts = outline_points(&straight, 64, 0.0);
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
        let pts = outline_points(&curved, 64, 0.0);
        let mid = pts[pts.len() / 2];
        assert!(mid.1 < 20.0, "曲线中段应明显偏离弦，实际 y={}", mid.1);
    }

    #[test]
    fn end_arrow_wings_point_into_the_segment() {
        // 回归：rough 终点箭头曾沿 +dir 伸出线外（V 尖朝线内、方向反了）。
        // (0,0)→(100,0)、线宽 2、头长 12（1.5× 后）、半张角 30°：
        // 翼尖 ≈ (100 - 12·cos30°, ±12·sin30°) = (89.61, ±6)。
        let stroke = StrokeStyle {
            width: 2.0,
            ..StrokeStyle::default()
        };
        let mut d = open_line();
        d.sloppiness = Sloppiness::Artist;
        let shapes =
            RoughStyler.build_shapes(&d, &stroke, None, FillStyle::Solid, &identity(), 1.0);
        let bez = beziers(&shapes);
        // 边 2 passes 后紧跟终点箭头 2 翼 × 2 passes
        for wing in &bez[2..6] {
            assert!(
                (wing[0].x - 100.0).abs() < 1e-3,
                "翼根必须锚在终点: {:?}",
                wing[0]
            );
            assert!(
                wing[3].x < 100.0 && wing[3].x > 85.0,
                "翼尖必须伸进线段体内（方向不得反）: {:?}",
                wing[3]
            );
            assert!(
                wing[3].y.abs() > 3.5 && wing[3].y.abs() < 8.5,
                "翼尖应按 30° 半张角张开约 ±6px: {:?}",
                wing[3]
            );
        }
    }

    #[test]
    fn clean_styler_arrow_uses_60_degree_total_opening() {
        // V 形箭头半张角 30°（总张开 60°）：线宽 2 → head_len 12（1.5× 后），
        // 翼尖 = (100 - 12·cos30°, ±12·sin30°) = (89.61, ±6.00)。
        let stroke = StrokeStyle {
            width: 2.0,
            ..StrokeStyle::default()
        };
        let shapes = CleanStyler.build_shapes(
            &open_line(),
            &stroke,
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        // 1 条主线 + 2 条翼线（Shape::line 生成 Path）
        assert_eq!(shapes.len(), 3);
        let paths: Vec<egui::epaint::PathShape> = shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Path(p) => Some(p.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(paths.len(), 3, "主线与两翼都是 Path");
        for wing in &paths[1..] {
            assert_eq!(wing.points[0], egui::pos2(100.0, 0.0), "翼根在终点");
            assert!(
                (wing.points[1].x - 89.608).abs() < 1e-2,
                "翼尖 x ≈ 100 - 12·cos30°: {:?}",
                wing.points[1]
            );
            assert!(
                (wing.points[1].y.abs() - 6.0).abs() < 1e-2,
                "翼尖 |y| ≈ 6: {:?}",
                wing.points[1]
            );
        }
    }

    #[test]
    fn clean_styler_filled_and_hollow_triangle_heads() {
        // 线宽 2：head_len = 12（1.5× 后），底边两点 ≈ (89.61, ±6.00)。
        let stroke = StrokeStyle {
            width: 2.0,
            ..StrokeStyle::default()
        };
        let mut d = open_line();
        d.end_arrow = Some(ArrowHeadStyle::Triangle);
        let shapes =
            CleanStyler.build_shapes(&d, &stroke, None, FillStyle::Solid, &identity(), 1.0);
        // 1 条线 + 1 个实心三角 Path
        assert_eq!(shapes.len(), 2);
        let Shape::Path(tri) = &shapes[1] else {
            panic!("Triangle 应为 Path 填充: {:?}", shapes[1]);
        };
        assert_eq!(tri.points.len(), 3);
        assert_eq!(tri.points[0], egui::pos2(100.0, 0.0), "尖端在终点");
        assert!(tri.fill.a() > 0, "三角必须实心填充描边色");
        for p in &tri.points[1..] {
            assert!(
                p.x < 100.0 && p.x > 85.0 && p.y.abs() > 3.0,
                "底边两点应在终点内侧张开: {p:?}"
            );
        }
        // 实心三角盖住线端，主线不裁——末端仍在 (100, 0)
        let Shape::Path(main) = &shapes[0] else {
            panic!("主线应为 Path: {:?}", shapes[0]);
        };
        assert_eq!(main.points.last().unwrap(), &egui::pos2(100.0, 0.0));

        let mut d = open_line();
        d.end_arrow = Some(ArrowHeadStyle::TriangleOutline);
        let shapes =
            CleanStyler.build_shapes(&d, &stroke, None, FillStyle::Solid, &identity(), 1.0);
        // 1 条线 + 1 个空心三角 Path（描边线宽 = 线宽，不填充）
        assert_eq!(shapes.len(), 2);
        let Shape::Path(hollow) = &shapes[1] else {
            panic!("TriangleOutline 应为 Path 描边: {:?}", shapes[1]);
        };
        assert_eq!(hollow.points.len(), 3);
        assert!(hollow.fill.a() == 0, "空心三角不得填充: {:?}", hollow.fill);
        assert_eq!(hollow.stroke.width, 2.0, "描边线宽应等于主线线宽");
        assert_eq!(hollow.points[0], egui::pos2(100.0, 0.0), "尖端在终点");
        // 空心三角：主线末端裁剪到三角底边中点 (100 - 12·cos30°, 0) = (89.61, 0)
        let Shape::Path(main) = &shapes[0] else {
            panic!("主线应为 Path: {:?}", shapes[0]);
        };
        assert!(
            ((main.points.last().unwrap().x - 89.608).abs() < 1e-2),
            "主线末端应裁剪到三角底边: {:?}",
            main.points.last()
        );
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
        // 起点 (0,0) → 终点 (100,0)，半径 = 线宽 2 × 2.25（1.5× 后）= 4.5
        let mut d = open_line();
        d.start_arrow = Some(ArrowHeadStyle::Dot);
        d.end_arrow = Some(ArrowHeadStyle::Dot);
        // 用固定 width=2 而非 Default，避免默认档调整破坏几何断言。
        let stroke = StrokeStyle {
            width: 2.0,
            ..StrokeStyle::default()
        };
        let shapes =
            CleanStyler.build_shapes(&d, &stroke, None, FillStyle::Solid, &identity(), 1.0);
        let centers: Vec<Pos2> = shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Circle(c) => Some(c.center),
                _ => None,
            })
            .collect();
        assert_eq!(centers.len(), 2);
        assert!(
            (centers[0].x - 4.5).abs() < 1e-3,
            "起点圆心内缩: {:?}",
            centers[0]
        );
        assert!(
            (centers[1].x - 95.5).abs() < 1e-3,
            "终点圆心内缩: {:?}",
            centers[1]
        );
    }

    #[test]
    fn rough_styler_triangle_head_fills_and_sketches_edges() {
        let mut d = open_line();
        d.end_arrow = Some(ArrowHeadStyle::Triangle);
        let shapes = RoughStyler.build_shapes(
            &d,
            &StrokeStyle::default(),
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        // 1 边 × 2 passes + 1 实心三角填充 + 3 条三角边 × 2 passes = 9
        assert_eq!(shapes.len(), 9);
        assert!(matches!(shapes[2], Shape::Path(_)), "先填充后描边");
        assert!(shapes[3..]
            .iter()
            .all(|s| matches!(s, Shape::CubicBezier(_))));
    }

    #[test]
    fn rough_styler_hollow_triangle_head_sketches_without_fill() {
        // 用固定 width=2 让裁剪量确定：trim = 12·cos30° ≈ 10.39。
        let stroke = StrokeStyle {
            width: 2.0,
            ..StrokeStyle::default()
        };
        let mut d = open_line();
        d.end_arrow = Some(ArrowHeadStyle::TriangleOutline);
        let shapes =
            RoughStyler.build_shapes(&d, &stroke, None, FillStyle::Solid, &identity(), 1.0);
        // 1 边 × 2 passes + 3 条三角边 × 2 passes = 8，无填充 Path
        assert_eq!(shapes.len(), 8);
        assert!(
            shapes.iter().all(|s| matches!(s, Shape::CubicBezier(_))),
            "空心三角只描边不填充"
        );
        // 主线末端裁剪到三角底边中点（amp=0 时贝塞尔端点即几何点）
        let bez = beziers(&shapes);
        assert!(
            (bez[0][3].x - 89.608).abs() < 1e-2,
            "主线末端应裁剪到三角底边: {:?}",
            bez[0][3]
        );
        assert!(
            (bez[1][3].x - 89.608).abs() < 1e-2,
            "第二笔同样裁剪: {:?}",
            bez[1][3]
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
        let sampled = outline_points(&d, 24, 0.0).len();
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
        let clean = build_shape_visuals(&base.kind, &[], &identity(), 1.0);
        assert_eq!(clean.len(), 1, "未开启手绘 → CleanStyler 单多边形");

        let rough = base.with_sloppiness(Sloppiness::Artist);
        let sketched = build_shape_visuals(&rough.kind, &[], &identity(), 1.0);
        assert_eq!(sketched.len(), 8, "开启手绘 → RoughStyler 抖动边");
    }

    #[test]
    fn build_shape_visuals_returns_empty_for_non_shape() {
        let txt = Item::new_text("x".to_string(), 0.0, 0.0, 16.0, [255; 4]);
        assert!(build_shape_visuals(&txt.kind, &[], &identity(), 1.0).is_empty());
    }

    #[test]
    fn freedraw_renders_as_strokes_not_filled_region() {
        // 回归（用户反馈）：墨迹必须是**逐段描边**，不能是填充闭合轮廓——后者在
        // 弯曲/回环处会自交、被非零环绕三角化填成实心区域。
        let item = Item::new_freedraw(
            &[(0.0, 0.0), (10.0, 2.0), (20.0, 0.0), (30.0, 3.0)],
            &[1.0, 0.8, 0.6, 0.4],
            4.0,
            [10, 10, 10, 255],
        );
        let shapes = build_freedraw_visuals(&item.kind, &identity());
        let n = 4;
        assert_eq!(shapes.len(), 2 * n - 1, "应产出 (n-1) 段描边线 + n 个圆帽");
        let segs = shapes
            .iter()
            .filter(|s| matches!(s, Shape::LineSegment { .. }))
            .count();
        let discs = shapes
            .iter()
            .filter(|s| matches!(s, Shape::Circle { .. }))
            .count();
        assert_eq!(segs, n - 1);
        assert_eq!(discs, n);
        assert!(
            !shapes
                .iter()
                .any(|s| matches!(s, Shape::Path(p) if p.closed)),
            "绝不能再出现闭合填充轮廓（那正是区域填充 bug）"
        );
    }

    #[test]
    fn freedraw_visuals_empty_for_non_freedraw_or_degenerate() {
        let txt = Item::new_text("x".to_string(), 0.0, 0.0, 16.0, [255; 4]);
        assert!(build_freedraw_visuals(&txt.kind, &identity()).is_empty());
        // 单点墨迹（<2）→ 空。
        let dot = Item::new_freedraw(&[(5.0, 5.0)], &[1.0], 3.0, [0, 0, 0, 255]);
        assert!(build_freedraw_visuals(&dot.kind, &identity()).is_empty());
    }

    // ── plan #20：RoughStyler 对齐 rough.js / Excalidraw 公式 ──

    fn straight_line(size: f32, sloppiness: Sloppiness) -> ShapeData {
        ShapeData {
            shape_type: ShapeType::Polyline,
            base_size: (size, 0.0),
            points: vec![(0.0, 0.0), (size, 0.0)],
            start_arrow: None,
            end_arrow: None,
            closed: false,
            curve_type: CurveType::Straight,
            elbow_mid_offset: 0.0,
            elbow_axis: None,
            elbow_route: Vec::new(),
            roundness: 0.0,
            seed: 42,
            sloppiness,
        }
    }

    #[test]
    fn roughness_gain_matches_roughjs_piecewise_formula() {
        assert_eq!(RoughStyler::roughness_gain(100.0), 1.0);
        assert!((RoughStyler::roughness_gain(200.0) - 0.9).abs() < 1e-3);
        assert!((RoughStyler::roughness_gain(300.0) - 0.7333).abs() < 1e-3);
        assert!((RoughStyler::roughness_gain(500.0) - 0.4).abs() < 1e-3);
        assert!((RoughStyler::roughness_gain(1000.0) - 0.4).abs() < 1e-3);
    }

    #[test]
    fn small_size_roughness_scale_matches_excalidraw_adjust_roughness() {
        let mut d = rect(1);
        // 100×60：min 20 且 max 50 → 不衰减
        assert_eq!(RoughStyler::small_size_roughness_scale(&d), 1.0);
        // 8×8：max <10 → ÷3
        d.base_size = (8.0, 8.0);
        assert!((RoughStyler::small_size_roughness_scale(&d) - 1.0 / 3.0).abs() < 1e-6);
        // 15×15：max <20 → ÷2
        d.base_size = (15.0, 15.0);
        assert_eq!(RoughStyler::small_size_roughness_scale(&d), 0.5);
        // 15×60 带圆角矩形：round 例外 → 不衰减
        d.base_size = (15.0, 60.0);
        d.roundness = 0.3;
        assert_eq!(RoughStyler::small_size_roughness_scale(&d), 1.0);
        d.roundness = 0.0;
        assert_eq!(RoughStyler::small_size_roughness_scale(&d), 0.5);
        // 线性元素 max ≥ 50 → 不衰减
        let line = straight_line(100.0, Sloppiness::Artist);
        assert_eq!(RoughStyler::small_size_roughness_scale(&line), 1.0);
    }

    #[test]
    fn cartoonist_endpoint_jitter_bounded_by_roughjs_formula() {
        // 端点位移 ≤ 2 画布px × roughnessGain(边长) × amp_scale（zoom=1）
        for (size, gain) in [(100.0, 1.0), (300.0, 0.7333), (1000.0, 0.4)] {
            let d = straight_line(size, Sloppiness::Cartoonist); // amp_scale = 3.6
            let shapes = RoughStyler.build_shapes(
                &d,
                &StrokeStyle::default(),
                None,
                FillStyle::Solid,
                &identity(),
                1.0,
            );
            let bez = beziers(&shapes);
            let bound = 2.0 * gain * Sloppiness::Cartoonist.amp_scale() + 1e-3;
            let a = egui::pos2(0.0, 0.0);
            let b = egui::pos2(size, 0.0);
            // rough.js 端点 x/y 各自独立 ±offset，逐轴断言（plan #20 质量门）
            let p0 = bez[0][0];
            let p3 = bez[bez.len() - 1][3];
            assert!(
                (p0.x - a.x).abs() <= bound && (p0.y - a.y).abs() <= bound,
                "size={size} 起点抖动越界: {p0:?}"
            );
            assert!(
                (p3.x - b.x).abs() <= bound && (p3.y - b.y).abs() <= bound,
                "size={size} 终点抖动越界: {p3:?}"
            );
        }
    }

    #[test]
    fn architect_and_artist_preserve_vertices() {
        // rough.js preserveVertices：roughness < cartoonist 的档位端点精确
        // （绑定/拼接接头不因端点抖动脱开）
        for s in [Sloppiness::Architect, Sloppiness::Artist] {
            let d = straight_line(100.0, s);
            let shapes = RoughStyler.build_shapes(
                &d,
                &StrokeStyle::default(),
                None,
                FillStyle::Solid,
                &identity(),
                1.0,
            );
            let bez = beziers(&shapes);
            for seg in &bez {
                assert_eq!(seg[0], egui::pos2(0.0, 0.0), "{s:?} 起点必须精确");
                assert_eq!(seg[3], egui::pos2(100.0, 0.0), "{s:?} 终点必须精确");
            }
        }
    }

    #[test]
    fn bowing_sign_is_randomized_across_seeds() {
        // 同一条 100px 边、不同 seed：贝塞尔控制点相对弦的垂直偏移必须出现
        // 正负两种符号（否则矩形四边一致外凸成"吹气感"，rough.js 无此行为）
        let mut has_pos = false;
        let mut has_neg = false;
        for seed in 0..32u64 {
            let mut d = straight_line(100.0, Sloppiness::Artist);
            d.seed = seed;
            let shapes = RoughStyler.build_shapes(
                &d,
                &StrokeStyle::default(),
                None,
                FillStyle::Solid,
                &identity(),
                1.0,
            );
            let dy = beziers(&shapes)[0][1].y;
            if dy > 1e-3 {
                has_pos = true;
            } else if dy < -1e-3 {
                has_neg = true;
            }
        }
        assert!(has_pos && has_neg, "bowing 符号必须随机");
    }

    #[test]
    fn hachure_skip_first_line_shifts_scan_by_exactly_one_gap() {
        // 跳首线 = 扫描起点后移一整行 → 线表恰为不跳时去掉第一条
        let pts = vec![
            egui::pos2(0.0, 0.0),
            egui::pos2(100.0, 0.0),
            egui::pos2(100.0, 100.0),
            egui::pos2(0.0, 100.0),
        ];
        let normal = hachure_segments(&pts, HACHURE_ANGLE_DEG, 10.0, false);
        let skipped = hachure_segments(&pts, HACHURE_ANGLE_DEG, 10.0, true);
        assert_eq!(skipped.len(), normal.len() - 1);
        for (s, n) in skipped.iter().zip(normal.iter().skip(1)) {
            assert_eq!(s[0], n[0]);
            assert_eq!(s[1], n[1]);
        }
    }

    #[test]
    fn hachure_lines_run_at_roughjs_effective_49_degree() {
        // rough.js hachureAngle(-41)+90 = 49° 仰角；屏幕 y 向下 → 方向 (cos49°, -sin49°)
        let pts = vec![
            egui::pos2(0.0, 0.0),
            egui::pos2(100.0, 0.0),
            egui::pos2(100.0, 100.0),
            egui::pos2(0.0, 100.0),
        ];
        let segs = hachure_segments(&pts, HACHURE_ANGLE_DEG, 10.0, false);
        assert!(!segs.is_empty());
        let [a, b] = segs[0];
        let d = (b - a) / (b - a).length();
        let rad = 49.0f32.to_radians();
        let want = egui::vec2(rad.cos(), -rad.sin());
        assert!(
            (d - want).length() < 1e-3 || (d + want).length() < 1e-3,
            "斜线方向 {d:?} 偏离 49° 仰角"
        );
    }

    #[test]
    fn rough_styler_rounded_rect_is_segmented_not_dense_ring() {
        // plan #20 批次1 曾把圆角矩形并入"整圈抖动 + Catmull-Rom"；2026-10-01
        // 验收反馈：交界控制柄过冲甩鼓包（小尺寸尤甚），改走"直边 + 单条角弧"
        // 分段路线（对齐 Excalidraw `path()`）——段数与轮廓采样密度解耦。
        let mut d = rect(3);
        d.roundness = 0.5; // 半径 = 40 × 0.5 × 0.5 = 10
        let shapes = RoughStyler.build_shapes(
            &d,
            &StrokeStyle::default(),
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        // 4 直边 + 4 角弧 = 8 段 × 2 passes；不再随 ROUNDED_CORNER_SEGMENTS 增长
        assert_eq!(shapes.len(), 8 * RoughStyler::PASSES);
        assert!(shapes.iter().all(|s| matches!(s, Shape::CubicBezier(_))));
    }

    #[test]
    fn curve_jitter_amp_never_exceeds_half_sample_spacing() {
        // plan #20 验收反馈：小矩形 + Cartoonist 圆角出乱线——固定密度采样下
        // 相邻点距只有几像素，抖动不得超过间距一半（否则相邻点互越、被
        // Catmull-Rom 放大成自交小环）。
        let mut d = rect(3);
        d.roundness = 0.5;
        d.base_size = (40.0, 30.0);
        d.sloppiness = Sloppiness::Cartoonist;
        let pts = to_screen_points(&outline_points(&d, 12, 0.0), &identity());
        let amp = RoughStyler::curve_jitter_amp(
            &pts,
            1.0,
            true,
            d.sloppiness.amp_scale() * RoughStyler::small_size_roughness_scale(&d),
        );
        let n = pts.len();
        let perimeter: f32 = (0..n).map(|i| (pts[i] - pts[(i + 1) % n]).length()).sum();
        let spacing = perimeter / n as f32;
        // rough.js 公式本身（Cartoonist ×小图衰减）远大于间距上限，被夹住
        assert!(
            (1.0 + 1.8 * 0.2) * 1.8 > spacing * 0.5,
            "测试前提：rough 公式应超出间距上限才有效"
        );
        assert!(
            amp <= spacing * 0.5 + 1e-3,
            "amp {amp} 超过间距一半 {spacing}"
        );
        assert!(amp > 0.0, "不应衰减到 0");
    }

    #[test]
    fn smooth_jitter_respects_local_sample_spacing_on_ellipse() {
        // plan #20 验收反馈二次：全局平均间距失守时逐点幅度必须 ≤ 0.35 × 较短
        // 相邻段，否则相邻点互越、被 Catmull-Rom 放大成自交小环。圆角矩形已于
        // 2026-10-01 改走分段渲染，整圈抖动只剩椭圆（采样均匀）——直接对
        // jitter_points 验证逐点钳制性质。
        let mut d = ellipse(80.0, 3);
        d.base_size = (80.0, 60.0);
        d.sloppiness = Sloppiness::Cartoonist;
        let seg_count = RoughStyler::ellipse_step_count(80.0, 60.0);
        let pts = to_screen_points(&outline_points(&d, seg_count, 0.0), &identity());
        let amp = RoughStyler::curve_jitter_amp(
            &pts,
            1.0,
            true,
            d.sloppiness.amp_scale() * RoughStyler::small_size_roughness_scale(&d),
        );
        let mut rng = SeededRng::new(3);
        let jittered = RoughStyler::jitter_points(&mut rng, &pts, true, amp);
        let n = pts.len();
        for i in 0..n {
            let prev = pts[(i + n - 1) % n];
            let next = pts[(i + 1) % n];
            let local = (pts[i] - prev).length().min((next - pts[i]).length()) * 0.35;
            let disp = (jittered[i] - pts[i]).length();
            assert!(
                disp <= local * std::f32::consts::SQRT_2 + 1e-3,
                "点 {i} 位移 {disp:.2} 超过局部间距上限 {local:.2}（会打成结）"
            );
        }
    }

    #[test]
    fn rough_styler_elbow_roundness_chains_without_gaps() {
        // plan #23 验收反馈（2026-09-28）：倒角弧段不得脱开断线。2026-10-01 起
        // elbow 倒角走"直边 + 单条角弧"分段渲染——段端点钉死（preserveVertices
        // 恒真，Excalidraw continuousPath 语义），每个 pass 内部逐段共享端点。
        let mut d = open_line();
        d.shape_type = ShapeType::Elbow;
        d.points = vec![(0.0, 0.0), (200.0, 100.0)];
        d.base_size = (200.0, 100.0);
        d.end_arrow = None;
        d.roundness = 0.5; // 半径 = min(200,100) × 0.5 × 0.5 = 25，拐角有弧
        d.sloppiness = Sloppiness::Cartoonist; // 旧逐边路线下此档会脱开
                                               // 阶段 C 起渲染消费注入路由（同源）：这里用 fallback 路由等价构造。
        d.elbow_route = preferz_core::item::elbow_route_fallback(
            &d.points,
            d.elbow_mid_offset,
            preferz_core::item::elbow_axis_infer(&d.points),
            &[],
        );
        let shapes = RoughStyler.build_shapes(
            &d,
            &StrokeStyle::default(),
            None,
            FillStyle::Solid,
            &identity(),
            1.0,
        );
        assert!(shapes.iter().all(|s| matches!(s, Shape::CubicBezier(_))));
        let bez = beziers(&shapes);
        // 段数与 core 分段同源：注入路由（fallback 构造）的直边数 + 拐角数
        let segs = orthogonal_outline_segments(
            &d.elbow_route,
            false,
            roundness_radius(d.base_size, d.roundness),
        )
        .len();
        assert!(segs >= 3, "测试前提：S 形路由至少 3 段");
        assert_eq!(bez.len(), segs * RoughStyler::PASSES);
        // 逐 pass 检查链式连续：段 i 的终点 = 段 i+1 的起点
        for p in 0..RoughStyler::PASSES {
            let pass = &bez[p * segs..(p + 1) * segs];
            for w in pass.windows(2) {
                assert_eq!(
                    w[0][3], w[1][0],
                    "elbow 倒角段脱开: {:?} -> {:?}",
                    w[0][3], w[1][0]
                );
            }
        }
        // 路径两端点是 elbow 端点、不倒角（不抖）
        assert_eq!(bez[0][0], egui::pos2(0.0, 0.0));
        assert_eq!(bez[segs - 1][3], egui::pos2(200.0, 100.0));
    }

    #[test]
    fn rough_hachure_fill_lines_use_half_stroke_width() {
        let stroke = StrokeStyle {
            color: [0, 0, 0, 255],
            width: 2.0,
            dash: DashStyle::Solid,
        };
        let with_fill = RoughStyler.build_shapes(
            &rect(7),
            &stroke,
            Some(Color32::RED),
            FillStyle::Hachure,
            &identity(),
            1.0,
        );
        let without_fill = RoughStyler.build_shapes(
            &rect(7),
            &stroke,
            None,
            FillStyle::Hachure,
            &identity(),
            1.0,
        );
        // 填充层在轮廓层之前：前 (with - without) 个 shape 即填充线
        let fill_count = with_fill.len() - without_fill.len();
        assert!(fill_count > 0, "hachure 填充必须产出线段");
        for s in &with_fill[..fill_count] {
            let Shape::CubicBezier(b) = s else {
                panic!("hachure 填充线现在走 sketch_edge（贝塞尔），got {s:?}");
            };
            assert!(
                (b.stroke.width - 1.0).abs() < 1e-3,
                "填充线宽应为 strokeWidth/2 = 1.0，got {}",
                b.stroke.width
            );
        }
    }
}
