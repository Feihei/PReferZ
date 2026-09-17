//! 端点吸附几何：计算 item 轮廓在画布空间下的线段集合，并求查询点到轮廓的最近点。
//!
//! 用于 plan #5「直线/箭头端点吸附图形边缘」：拖端点时吸附到邻近 Shape 轮廓，
//! 以及形状移动时联动重算绑定端点位置。轮廓统一离散为多段线后求最近点，
//! 矩形/菱形用 4 边、椭圆用 64 边采样、线性对象用其自身顶点。

use crate::item::{Item, ItemKind, ItemLocalSpace};
use crate::shape::ShapeType;
use crate::spaces::{CanvasPoint, CanvasVector};

/// 把一段线段离散为 `n` 条边（首尾相连）。
fn ellipse_ring(r: crate::spaces::CanvasRect, n: usize) -> Vec<(CanvasPoint, CanvasPoint)> {
    let (x0, y0, x1, y1) = (r.min_x(), r.min_y(), r.max_x(), r.max_y());
    let cx = (x0 + x1) * 0.5;
    let cy = (y0 + y1) * 0.5;
    let rx = (x1 - x0) * 0.5;
    let ry = (y1 - y0) * 0.5;
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let theta = 2.0 * std::f32::consts::PI * (i as f32) / (n as f32);
        pts.push(CanvasPoint::new(
            cx + rx * theta.cos(),
            cy + ry * theta.sin(),
        ));
    }
    let mut segs = Vec::with_capacity(n);
    for w in pts.windows(2) {
        segs.push((w[0], w[1]));
    }
    segs.push((pts[n - 1], pts[0]));
    segs
}

/// 矩形（轴对齐 bounding rect）四边。
fn rect_ring(r: crate::spaces::CanvasRect) -> Vec<(CanvasPoint, CanvasPoint)> {
    let (x0, y0, x1, y1) = (r.min_x(), r.min_y(), r.max_x(), r.max_y());
    let tl = CanvasPoint::new(x0, y0);
    let tr = CanvasPoint::new(x1, y0);
    let br = CanvasPoint::new(x1, y1);
    let bl = CanvasPoint::new(x0, y1);
    vec![(tl, tr), (tr, br), (br, bl), (bl, tl)]
}

/// 菱形（矩形内切菱形，四边）。
fn diamond_ring(r: crate::spaces::CanvasRect) -> Vec<(CanvasPoint, CanvasPoint)> {
    let (x0, y0, x1, y1) = (r.min_x(), r.min_y(), r.max_x(), r.max_y());
    let cx = (x0 + x1) * 0.5;
    let cy = (y0 + y1) * 0.5;
    let top = CanvasPoint::new(cx, y0);
    let right = CanvasPoint::new(x1, cy);
    let bottom = CanvasPoint::new(cx, y1);
    let left = CanvasPoint::new(x0, cy);
    vec![(top, right), (right, bottom), (bottom, left), (left, top)]
}

/// item 在画布空间下的轮廓，离散为一组线段 `(起点, 终点)`。
///
/// - 矩形 / 画框 / 图片 / 文本：bounding rect 四边
/// - 椭圆：64 边采样
/// - 菱形：内切菱形四边
/// - 线性对象（Polyline）：其顶点变换到画布空间后的连续线段（闭合时首尾相连）
pub fn outline_segments(item: &Item) -> Vec<(CanvasPoint, CanvasPoint)> {
    match &item.kind {
        ItemKind::Shape {
            shape_type,
            points,
            closed,
            ..
        } => {
            let rect = item.bounding_rect();
            match shape_type {
                ShapeType::Rectangle => rect_ring(rect),
                ShapeType::Ellipse => ellipse_ring(rect, 64),
                ShapeType::Diamond => diamond_ring(rect),
                ShapeType::Polyline => {
                    let to_c = item.local_to_canvas();
                    let pts: Vec<CanvasPoint> = points
                        .iter()
                        .map(|(x, y)| {
                            to_c.transform_point(euclid::Point2D::<f32, ItemLocalSpace>::new(
                                *x, *y,
                            ))
                        })
                        .collect();
                    if pts.len() < 2 {
                        return Vec::new();
                    }
                    let mut segs = Vec::new();
                    for w in pts.windows(2) {
                        segs.push((w[0], w[1]));
                    }
                    if *closed {
                        segs.push((pts[pts.len() - 1], pts[0]));
                    }
                    segs
                }
            }
        }
        // 非 Shape（Pixmap / Text / Frame）：取其 bounding rect 四边。
        _ => rect_ring(item.bounding_rect()),
    }
}

/// 将查询点投影到线段 `[a, b]` 上（t 钳制在 [0, 1]），返回最近点。
pub fn project_point_on_segment(p: CanvasPoint, a: CanvasPoint, b: CanvasPoint) -> CanvasPoint {
    let ab = b - a;
    let ap = p - a;
    let len2 = ab.x * ab.x + ab.y * ab.y;
    if len2 == 0.0 {
        return a;
    }
    let t = ((ap.x * ab.x + ap.y * ab.y) / len2).clamp(0.0, 1.0);
    a + ab * t
}

/// 查询点到一组线段的最近点及其距离。无线段时返回 `(query, +inf)`。
pub fn nearest_point_on_segments(
    query: CanvasPoint,
    segs: &[(CanvasPoint, CanvasPoint)],
) -> (CanvasPoint, f32) {
    let mut best: Option<(CanvasPoint, f32)> = None;
    for (a, b) in segs {
        let p = project_point_on_segment(query, *a, *b);
        let d = query.distance_to(p);
        if best.as_ref().is_none_or(|(_, bd)| d < *bd) {
            best = Some((p, d));
        }
    }
    best.unwrap_or((query, f32::MAX))
}

/// item 轮廓上离 `query` 最近的点（画布空间）。
pub fn nearest_outline_point(item: &Item, query: CanvasPoint) -> CanvasPoint {
    let segs = outline_segments(item);
    nearest_point_on_segments(query, &segs).0
}

/// 多边形/折线绘点时的 Shift 约束吸附（plan #4）：把 `from→to` 这条段的角度
/// 归到最近的 45° 方向、**保持段长不变**（等长转向，非投影）。`shift=false` 时
/// 原样返回 `to`；段长趋零时退化返回 `from`（无法定方向）。纯几何、只吃画布坐标。
pub fn snap_polygon_point(from: CanvasPoint, to: CanvasPoint, shift: bool) -> CanvasPoint {
    if !shift {
        return to;
    }
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-3 {
        return from;
    }
    let angle = (dy.atan2(dx) / std::f32::consts::FRAC_PI_4).round() * std::f32::consts::FRAC_PI_4;
    from + CanvasVector::new(len * angle.cos(), len * angle.sin())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;
    use crate::shape::ShapeType;
    use crate::spaces::{CanvasPoint, CanvasVector};

    fn rect_item(w: f32, h: f32) -> Item {
        Item::new_shape(
            ShapeType::Rectangle,
            (w, h),
            0.0,
            0.0,
            Default::default(),
            None,
        )
    }

    #[test]
    fn nearest_on_rect_outline() {
        // 100×100 矩形在原点。外部点 (150, 50) 最近轮廓点应为右边界 (100, 50)。
        let it = rect_item(100.0, 100.0);
        let q = CanvasPoint::new(150.0, 50.0);
        let p = nearest_outline_point(&it, q);
        assert!((p.x - 100.0).abs() < 1e-3, "x={}", p.x);
        assert!((p.y - 50.0).abs() < 1e-3, "y={}", p.y);
    }

    #[test]
    fn nearest_on_rect_corner_region() {
        // 点 (120, 120) 离右下角 (100,100) 最近。
        let it = rect_item(100.0, 100.0);
        let q = CanvasPoint::new(120.0, 120.0);
        let p = nearest_outline_point(&it, q);
        assert!((p.x - 100.0).abs() < 1e-3);
        assert!((p.y - 100.0).abs() < 1e-3);
    }

    #[test]
    fn ellipse_outline_close_to_boundary() {
        // 椭圆 200×100 内切于 [0,200]×[0,100]，右顶点在 (200, 50)。
        // 查询点 (300, 50) 位于长轴延长线上，最近点应≈右顶点 (200, 50)。
        let ellipse = Item::new_shape(
            ShapeType::Ellipse,
            (200.0, 100.0),
            0.0,
            0.0,
            Default::default(),
            None,
        );
        let q = CanvasPoint::new(300.0, 50.0);
        let p = nearest_outline_point(&ellipse, q);
        assert!((p.x - 200.0).abs() < 2.0, "ellipse x={}", p.x); // 64 边采样误差 < 2px
        assert!((p.y - 50.0).abs() < 2.0, "ellipse y={}", p.y);
    }

    #[test]
    fn diamond_outline_attaches_on_side() {
        // 菱形内切 100×100，右顶点在 bounding box 右侧中点 (100, 50)。
        // 查询点 (100, 50) 应命中右顶点。
        let diamond = Item::new_shape(
            ShapeType::Diamond,
            (100.0, 100.0),
            0.0,
            0.0,
            Default::default(),
            None,
        );
        let q = CanvasPoint::new(100.0, 50.0);
        let p = nearest_outline_point(&diamond, q);
        assert!((p.x - 100.0).abs() < 1e-3, "x={}", p.x);
        assert!((p.y - 50.0).abs() < 1e-3, "y={}", p.y);
    }

    #[test]
    fn project_clamps_to_segment() {
        let a = CanvasPoint::new(0.0, 0.0);
        let b = CanvasPoint::new(10.0, 0.0);
        // 超右端 → 钳制到 b
        let p = project_point_on_segment(CanvasPoint::new(20.0, 5.0), a, b);
        assert!((p.x - 10.0).abs() < 1e-5);
        assert!((p.y - 0.0).abs() < 1e-5);
        // 段中点正上方 → 中点
        let p2 = project_point_on_segment(CanvasPoint::new(5.0, 7.0), a, b);
        assert!((p2.x - 5.0).abs() < 1e-5);
        assert!(p2.y.abs() < 1e-5);
    }

    #[test]
    fn outline_respects_translation() {
        // 平移 30,40 后，外部点 (150+30, 50+40) 最近点应≈ (100+30, 50+40)
        let mut it = rect_item(100.0, 100.0);
        it.transform.pos = CanvasVector::new(30.0, 40.0);
        let q = CanvasPoint::new(180.0, 90.0);
        let p = nearest_outline_point(&it, q);
        assert!((p.x - 130.0).abs() < 1e-3);
        assert!((p.y - 90.0).abs() < 1e-3);
    }

    #[test]
    fn snap_polygon_point_without_shift_returns_target_unchanged() {
        let from = CanvasPoint::new(10.0, 10.0);
        let to = CanvasPoint::new(100.0, 37.0);
        assert_eq!(snap_polygon_point(from, to, false), to);
    }

    #[test]
    fn snap_polygon_point_with_shift_snaps_to_45_degrees_keeping_length() {
        // 约 11° → 吸附到 0°；长度保持（不是投影，是等长转向）
        let snapped = snap_polygon_point(
            CanvasPoint::new(0.0, 0.0),
            CanvasPoint::new(100.0, 20.0),
            true,
        );
        let len = (100.0f32 * 100.0 + 20.0f32 * 20.0).sqrt();
        assert!(
            (snapped.x - len).abs() < 1e-3 && snapped.y.abs() < 1e-3,
            "应吸附到 0° 且保持长度 {len}，实际 {snapped:?}"
        );
    }

    #[test]
    fn snap_polygon_point_snaps_near_45_to_exact_diagonal() {
        let snapped = snap_polygon_point(
            CanvasPoint::new(0.0, 0.0),
            CanvasPoint::new(100.0, 90.0),
            true,
        );
        assert!(
            (snapped.x - snapped.y).abs() < 1e-3,
            "应落在 45° 对角线上，实际 {snapped:?}"
        );
    }

    #[test]
    fn snap_polygon_point_degenerate_segment_returns_from() {
        let from = CanvasPoint::new(5.0, 7.0);
        assert_eq!(snap_polygon_point(from, from, true), from);
    }
}
