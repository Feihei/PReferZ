//! plan #7 流程图节点落位（core L1）：沿某方向新建节点时，**主轴**固定在源节点
//! 边界外一个 `gap`（永远相对源，而非相对邻居），**交叉轴**在该列带内把新节点滑到
//! 离源中心最近的空位——从而在同向已有下一节点时自动错开成**分叉**，而非重叠。
//!
//! 移植 Excalidraw `packages/element/src/flowchart.ts::placeCluster`（count=1 退化
//! 成单节点：`primaryStart` / `mergeIntervals` / `findNearestFreeSlot`）。障碍集由
//! 调用方（binary 层遍历场景端点绑定取同一连通子图的节点包围盒）传入，对齐 Excalidraw
//! `getConnectedFlowchartNodes`（#8518）。纯函数、无副作用、egui-free、可无头单测。

use crate::spaces::{CanvasPoint, CanvasRect};

/// 新建节点的方向（core 侧纯几何用）。binary 层从其按键语义 `FlowDir` 映射到此。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowDir {
    Up,
    Right,
    Down,
    Left,
}

impl FlowDir {
    /// 主轴是否为水平方向（Right/Left：主轴=x，交叉轴=y）。
    fn horizontal(self) -> bool {
        matches!(self, FlowDir::Right | FlowDir::Left)
    }
}

/// 交叉轴上的一段占用区间（已按 `cross_gap` 外扩）。
#[derive(Debug, Clone, Copy)]
struct Interval {
    start: f32,
    end: f32,
}

/// 合并重叠/相接的区间（入参无序，内部排序）。移植 `mergeIntervals`。
fn merge_intervals(mut intervals: Vec<Interval>) -> Vec<Interval> {
    intervals.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut merged: Vec<Interval> = Vec::new();
    for iv in intervals {
        match merged.last_mut() {
            // `<=` 让首尾相接也合并，与 Excalidraw 一致
            Some(last) if iv.start <= last.end => {
                last.end = last.end.max(iv.end);
            }
            _ => merged.push(iv),
        }
    }
    merged
}

/// 一段 `[start, start+size]` 是否与所有占用区间都不交。移植 `intervalIsFree`。
fn interval_is_free(start: f32, size: f32, occupied: &[Interval]) -> bool {
    occupied
        .iter()
        .all(|o| start + size <= o.start || start >= o.end)
}

fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    v.max(lo).min(hi)
}

/// 在占用区间之外，找离 `ideal` 最近的、能容纳 `[start, start+size]` 的落点，
/// 两侧同时搜索、平票偏向正方向（`<=` 保证后者覆盖前者）。移植 `findNearestFreeSlot`。
///
/// 前置：`occupied` 已 [`merge_intervals`] 且按 start 升序、彼此不相交。据此可把
/// 「缝隙」直接表示为 `[前一个 end, 后一个 start]` 的相邻对，两端各补 ±∞。
fn nearest_free_slot(ideal: f32, size: f32, occupied: &[Interval]) -> f32 {
    if interval_is_free(ideal, size, occupied) {
        return ideal;
    }

    // gap_starts = [-∞, o0.end, o1.end, ...]，gap_ends = [o0.start, o1.start, ..., +∞]，
    // 二者一一对应成第 i 个缝隙 [gap_starts[i], gap_ends[i]]。
    let mut gap_starts = Vec::with_capacity(occupied.len() + 1);
    let mut gap_ends = Vec::with_capacity(occupied.len() + 1);
    gap_starts.push(f32::NEG_INFINITY);
    for o in occupied {
        gap_starts.push(o.end);
        gap_ends.push(o.start);
    }
    gap_ends.push(f32::INFINITY);

    let mut best = ideal;
    let mut best_dist = f32::INFINITY;
    for i in 0..gap_starts.len() {
        if gap_ends[i] - gap_starts[i] < size {
            continue;
        }
        let start = clamp(ideal, gap_starts[i], gap_ends[i] - size);
        let dist = (start - ideal).abs();
        if dist <= best_dist {
            best = start;
            best_dist = dist;
        }
    }
    best
}

/// 计算沿 `dir` 从 `source` 新建一个**同尺寸**节点时，新节点的**左上角**（画布空间）。
///
/// - `source`：源节点包围盒；新节点尺寸取 `source` 的宽高（克隆语义）。
/// - `gap`：主轴间距（源远边 → 新节点近边）。
/// - `cross_gap`：交叉轴上与其他节点保持的最小间隙。
/// - `obstacles`：同一连通流程图里其它节点的包围盒（含源也无妨——源的投影落在带外，
///   被带过滤自然剔除）。
///
/// 无（或不交带内的）障碍时，交叉轴对齐源中心（= 原始单链行为）。
pub fn place_node(
    source: CanvasRect,
    dir: FlowDir,
    gap: f32,
    cross_gap: f32,
    obstacles: &[CanvasRect],
) -> CanvasPoint {
    let horizontal = dir.horizontal();
    let (node_primary, node_cross) = if horizontal {
        (source.width(), source.height())
    } else {
        (source.height(), source.width())
    };

    // 主轴：永远相对源远边 + gap（Right/Down 取 max 侧、Left/Up 取 min 侧）。
    let source_primary_min = if horizontal {
        source.min().x
    } else {
        source.min().y
    };
    let source_cross_center = if horizontal {
        source.center().y
    } else {
        source.center().x
    };
    let primary_start = match dir {
        FlowDir::Right | FlowDir::Down => source_primary_min + node_primary + gap,
        FlowDir::Left | FlowDir::Up => source_primary_min - gap - node_primary,
    };

    // 交叉轴占用：只取主轴投影与新带 [primary_start, +node_primary] 有交集的障碍，
    // 其交叉轴跨度外扩 cross_gap 后合并成占用区间集。
    let occupied = merge_intervals(
        obstacles
            .iter()
            .filter(|b| {
                let (bs, be) = if horizontal {
                    (b.min().x, b.max().x)
                } else {
                    (b.min().y, b.max().y)
                };
                bs < primary_start + node_primary && be > primary_start
            })
            .map(|b| {
                let (cs, ce) = if horizontal {
                    (b.min().y, b.max().y)
                } else {
                    (b.min().x, b.max().x)
                };
                Interval {
                    start: cs - cross_gap,
                    end: ce + cross_gap,
                }
            })
            .collect(),
    );

    let ideal = source_cross_center - node_cross / 2.0;
    let cross_start = nearest_free_slot(ideal, node_cross, &occupied);

    if horizontal {
        CanvasPoint::new(primary_start, cross_start)
    } else {
        CanvasPoint::new(cross_start, primary_start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spaces::{CanvasPoint, CanvasRect};

    fn rect(x: f32, y: f32, w: f32, h: f32) -> CanvasRect {
        CanvasRect::new(CanvasPoint::new(x, y), euclid::Size2D::new(w, h))
    }

    #[test]
    fn empty_band_centers_on_source_edge_plus_gap() {
        // 无邻居：主轴 = 源右边界 + gap，交叉轴对齐源中心（等价原始单链行为）。
        let src = rect(10.0, 10.0, 120.0, 80.0);
        let p = place_node(src, FlowDir::Right, 100.0, 100.0, &[]);
        // max.x = 130 + gap 100 = 230；center.y = 50 - h/2 40 = 10
        assert_eq!((p.x, p.y), (230.0, 10.0));
    }

    #[test]
    fn occupied_slot_pushes_to_next_free_below() {
        // 该列带内已有节点占据「居中」位 → 新节点滑到其下（正方向平票侧），形成分叉。
        let src = rect(10.0, 10.0, 120.0, 80.0);
        let neighbor = rect(230.0, 10.0, 120.0, 80.0); // B，正落在理想居中位
        let p = place_node(src, FlowDir::Right, 100.0, 100.0, &[neighbor]);
        // 主轴仍 230；占用交叉轴 [10-100,90+100]=[-90,190]，理想 10 被占 →
        // 两侧各距 180，平票取正 → cross_start = 190。
        assert_eq!((p.x, p.y), (230.0, 190.0));
    }

    #[test]
    fn third_sibling_takes_nearest_free_slot_above() {
        // A 已有两右子：B 占 y[10,90]、C 占 y[190,270]（外扩 cross_gap 后合并成
        // 占用段 [−90,370]）。理想居中位 10 被吞，最近空位在**上方** −170
        // （上距 180 < 下距 360），故第三个错到 B 之上——忠实复刻 findNearestFreeSlot。
        let src = rect(10.0, 10.0, 120.0, 80.0);
        let b = rect(230.0, 10.0, 120.0, 80.0);
        let c = rect(230.0, 190.0, 120.0, 80.0);
        let p = place_node(src, FlowDir::Right, 100.0, 100.0, &[b, c]);
        assert_eq!((p.x, p.y), (230.0, -170.0));
    }

    #[test]
    fn obstacle_outside_band_is_ignored() {
        // 障碍主轴投影落在带外（x 太靠左，够不到 primary 列）→ 不参与避让。
        let src = rect(10.0, 10.0, 120.0, 80.0);
        let far = rect(-500.0, 10.0, 120.0, 80.0);
        let p = place_node(src, FlowDir::Right, 100.0, 100.0, &[far]);
        assert_eq!((p.x, p.y), (230.0, 10.0)); // 回到居中理想位
    }

    #[test]
    fn up_direction_uses_min_side_and_forks_left() {
        // 向上：主轴 = 源 min.y - gap - h；占用后交叉轴（此处为 x）平票取正方向。
        let src = rect(100.0, 100.0, 80.0, 60.0);
        let p = place_node(src, FlowDir::Up, 100.0, 100.0, &[]);
        // primary = 100 - 100 - 60 = -60（y）；cross = center.x 140 - w/2 40 = 100（x）
        assert_eq!((p.x, p.y), (100.0, -60.0));
    }
}
