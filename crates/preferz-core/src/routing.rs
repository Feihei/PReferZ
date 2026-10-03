//! elbow 连接器 A\* 路由（plan #24 阶段 C/D，DP-5/DP-6 拍板见 `.agents/plan.md`）。
//!
//! 分层模型（对齐 Excalidraw `elbowArrow.ts` master）：
//! - 任一端**绑定** → A\* 网格避障：障碍 = 两端绑定形状的旋转 AABB 四边膨胀
//!   [`ELBOW_PADDING`]（对齐 Excalidraw `BASE_PADDING`）；首/末段 = 沿锚点边
//!   法线的"插座腿"（dongle）。**不做全场景绕行**（Excalidraw master 亦未做，
//!   issue #8635 开放中）。
//! - 两端自由（且无障碍）→ 确定性 L/Z/S（`elbow_mid_offset` bar 语义，旧档
//!   视觉不变）。
//! - `fixed_segments`（阶段 D）：固定段按坐标锚定进路由——锚点间子路由用 A\*
//!   （有障碍）或确定性最小连接（无障碍）缝合；固定段本身原样插入。
//! - A\* 失败 / 退化 → 整体回退确定性规则。
//!
//! 网格是**非均匀网格**（Excalidraw `calculateGrid` 同思路）：坐标 = 两端虚拟
//! 节点 + 障碍边线 ± union 外扩，取 x/y 笛卡尔交点为节点，规模 O(k²)（典型
//! <150 节点），每帧重算免缓存——纯函数，同源不变量（命中/渲染/手柄/导出）
//! 免费保持。输出恒为正交折线、首末点与输入端点一致。

use std::collections::BinaryHeap;

use serde::{Deserialize, Serialize};

use crate::item::elbow_polyline_offset;
use crate::shape::ElbowAxis;

/// 绑定端首/末"插座腿"长度（画布 px，对齐 Excalidraw `BASE_PADDING`）：
/// 离开/进入绑定形状时沿锚点边法线先走这一段，再进入自由路由。
pub const ELBOW_PADDING: f32 = 40.0;

/// 路由坐标精度阈值（与 `elbow_polyline_offset` 同量级）。
const EPS: f32 = 1e-3;

/// elbow 固定段（plan #24 阶段 D，DP-5）：用户拖动某中间段后固化的走线，
/// `start`/`end` 为 **item 局部坐标**（随 item 平移一致移动）。与 Excalidraw
/// 的 `{index, start, end}` 刻意不同：index 是对上一帧路由的引用、重路由后
/// 漂移，需要 renormalize；坐标锚定自洽，顺序在构建路由时贪心最近锚点链接
/// 得出（[`chain_stations`]）。首/末段不可固定（绑定端恒垂直进出边界）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ElbowFixedSegment {
    pub start: (f32, f32),
    pub end: (f32, f32),
}

impl ElbowFixedSegment {
    /// 段轴向（固定段恒正交）。
    pub fn axis(&self) -> ElbowAxis {
        axis_of((self.end.0 - self.start.0, self.end.1 - self.start.1))
    }
}

/// A\* 路由输入（全部消费方——命中/渲染/手柄/导出——传同一份输入得同一条
/// 路径）。坐标空间由调用方选定（画布或局部），输入输出同空间。
pub struct ElbowRouteInput<'a> {
    pub start: (f32, f32),
    pub end: (f32, f32),
    /// 首段离开端点的方向（单位轴向，有符号）：绑定端 = 锚点边外法线，自由
    /// 端 = 存储取向朝另一端。恒提供。
    pub start_heading: (f32, f32),
    /// 末段进入端点的方向（锚点边内法线 = -外法线）。恒提供。
    pub end_heading: (f32, f32),
    /// 端点是否绑定（绑定端加插座腿；绑定目标形状是障碍来源）。
    pub start_bound: bool,
    pub end_bound: bool,
    /// 无障碍无固定段时确定性 Z 的 bar 偏移（`elbow_mid_offset`，用户意图）。
    pub offset: f32,
    pub fixed_segments: &'a [ElbowFixedSegment],
    /// 障碍 AABB `(min_x, min_y, max_x, max_y)`（绑定形状膨胀后）。空 = 无避障。
    pub obstacles: &'a [(f32, f32, f32, f32)],
}

/// elbow 完整路由（分层模型总入口）。输出首/末点恒与 `start`/`end` 一致，
/// 中间点全部正交。
pub fn elbow_route(input: &ElbowRouteInput) -> Vec<(f32, f32)> {
    let (start, end) = (input.start, input.end);
    let aligned = (end.0 - start.0).abs() < EPS || (end.1 - start.1).abs() < EPS;
    // 退化（共线）直接直线——但**有障碍 / 有固定段时不短路**：直线可能穿障
    // 或漏掉用户固定段，分别交给 A\* 与缝合层（失败再由回退层处理）。
    if aligned && input.obstacles.is_empty() && input.fixed_segments.is_empty() {
        return vec![start, end];
    }
    // 无障碍：A\* 无用武之地，固定段用确定性缝合、其余走 Z/L（旧档行为）。
    if input.obstacles.is_empty() {
        return join_chain_deterministic(input);
    }
    // A\* 层：绑定端加插座腿——起点沿出发 heading 外推、终点沿进入 heading
    // 内推（`end_heading` 指向端点，虚拟节点在其后方 P 处，落在膨胀边界外）。
    let sv = virtual_node(start, input.start_heading, input.start_bound, 1.0);
    let ev = virtual_node(end, input.end_heading, input.end_bound, -1.0);
    if manhattan(sv, ev) < EPS {
        return join_chain_deterministic(input);
    }
    let Some(stations) = chain_stations(start, input.fixed_segments, ev) else {
        return join_chain_deterministic(input);
    };
    let mut out: Vec<(f32, f32)> = vec![start];
    if input.start_bound {
        out.push(sv);
    }
    // 逐站点 A\*：出发方向 = 上一段延续方向；终点进入方向 = 下一站约束。
    let mut cur = sv;
    let mut cur_heading = input.start_heading;
    let mut failed = false;
    for st in &stations {
        let (target, arrival) = match st {
            Station::Anchor(a) => (*a, None),
            Station::End(e) => (*e, Some(input.end_heading)),
        };
        match astar(cur, target, cur_heading, arrival, input.obstacles) {
            Some(mut seg) => {
                seg.remove(0); // 首点 == cur，避免重合
                out.extend(seg);
            }
            None => {
                failed = true;
                break;
            }
        }
        match st {
            Station::Anchor(a) => {
                // 插入固定段本体（a 到段另一端），继续沿段方向出发。
                let seg = input
                    .fixed_segments
                    .iter()
                    .find(|s| s.start == *a || s.end == *a)
                    .copied();
                let Some(seg) = seg else {
                    failed = true;
                    break;
                };
                let (from, to) = if seg.start == *a {
                    (seg.start, seg.end)
                } else {
                    (seg.end, seg.start)
                };
                out.push(to);
                cur = to;
                cur_heading = unit_dir((to.0 - from.0, to.1 - from.1));
            }
            Station::End(_) => {}
        }
    }
    if failed {
        return join_chain_deterministic(input);
    }
    if input.end_bound {
        out.push(ev);
        out.push(end);
    }
    let out = merge_collinear(out);
    // 端点不变量兜底（异常即回退，绝不输出变形路径）。
    if out.len() < 2 || out[0] != start || *out.last().unwrap() != end {
        return join_chain_deterministic(input);
    }
    out
}

/// 绑定端虚拟节点：`sign=1`（起点）沿 heading 外推插座腿长度；`sign=-1`
/// （终点）沿进入 heading 反向内推（虚拟节点在端点后方 P 处、膨胀边界外）。
fn virtual_node(p: (f32, f32), heading: (f32, f32), bound: bool, sign: f32) -> (f32, f32) {
    if bound {
        (
            p.0 + heading.0 * ELBOW_PADDING * sign,
            p.1 + heading.1 * ELBOW_PADDING * sign,
        )
    } else {
        p
    }
}

/// 确定性缝合链（无障碍层 / A\* 失败回退）：固定段原样插入，锚点间用最小
/// 正交连接（Z/L）；无固定段 = 现行 `elbow_polyline_offset`（旧档视觉不变）。
fn join_chain_deterministic(input: &ElbowRouteInput) -> Vec<(f32, f32)> {
    let (start, end) = (input.start, input.end);
    let start_axis = axis_of(input.start_heading);
    if input.fixed_segments.is_empty() {
        return elbow_polyline_offset(&[start, end], input.offset, start_axis);
    }
    let Some(stations) = chain_stations(start, input.fixed_segments, end) else {
        return elbow_polyline_offset(&[start, end], input.offset, start_axis);
    };
    let mut out: Vec<(f32, f32)> = vec![start];
    let mut prev = start;
    let mut prev_axis = start_axis;
    for st in &stations {
        match st {
            Station::Anchor(a) => {
                let seg = input
                    .fixed_segments
                    .iter()
                    .find(|s| s.start == *a || s.end == *a)
                    .copied();
                let Some(seg) = seg else { break };
                let (from, to) = if seg.start == *a {
                    (seg.start, seg.end)
                } else {
                    (seg.end, seg.start)
                };
                out.extend(minimal_join(prev, from, prev_axis, Some(seg.axis())));
                out.push(to);
                prev = to;
                prev_axis = seg.axis();
            }
            Station::End(e) => {
                out.extend(minimal_join(
                    prev,
                    *e,
                    prev_axis,
                    Some(axis_of(input.end_heading)),
                ));
                prev = *e;
            }
        }
    }
    merge_collinear(out)
}

/// 子路由最小正交连接：出发轴 `from_axis`、到达轴 `to_axis`。同轴 → Z（bar
/// 居中）；异轴 → L（单转折，转折点取使首末段各沿其轴的角）。
fn minimal_join(
    a: (f32, f32),
    b: (f32, f32),
    from_axis: ElbowAxis,
    to_axis: Option<ElbowAxis>,
) -> Vec<(f32, f32)> {
    if (b.0 - a.0).abs() < EPS || (b.1 - a.1).abs() < EPS {
        return vec![a, b];
    }
    let to = to_axis.unwrap_or(from_axis);
    if from_axis == to {
        elbow_polyline_offset(&[a, b], 0.0, from_axis)
    } else {
        let bend = if from_axis == ElbowAxis::HorizontalFirst {
            (b.0, a.1)
        } else {
            (a.0, b.1)
        };
        vec![a, bend, b]
    }
}

/// 站点：路由需依次经过的锚点/终点。`Anchor(a)` 的 `a` 是固定段上被链接的
/// 进入端（段本体在 [`elbow_route`] / [`join_chain_deterministic`] 里原样插入）。
enum Station {
    Anchor((f32, f32)),
    End((f32, f32)),
}

/// 贪心最近锚点链接（DP-5）：从 `start` 出发，反复取「离当前点最近的固定段
/// 端点」作为下一站，遍历完所有固定段后到 `end`。并列时先见者胜（遍历顺序
/// 固定 → 确定性）。
fn chain_stations(
    start: (f32, f32),
    fixed: &[ElbowFixedSegment],
    end: (f32, f32),
) -> Option<Vec<Station>> {
    let mut cur = start;
    let mut remaining: Vec<ElbowFixedSegment> = fixed.to_vec();
    let mut out = Vec::with_capacity(remaining.len() + 1);
    while !remaining.is_empty() {
        let mut best: Option<(usize, (f32, f32))> = None;
        for (i, s) in remaining.iter().enumerate() {
            for p in [s.start, s.end] {
                if best.is_none() || manhattan(cur, p) < manhattan(cur, best.unwrap().1) {
                    best = Some((i, p));
                }
            }
        }
        let (idx, entry) = best?;
        let seg = remaining.remove(idx);
        out.push(Station::Anchor(entry));
        cur = if seg.start == entry {
            seg.end
        } else {
            seg.start
        };
    }
    out.push(Station::End(end));
    Some(out)
}

fn manhattan(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

/// 任意方向向量 → 主轴。
fn axis_of(d: (f32, f32)) -> ElbowAxis {
    if d.0.abs() >= d.1.abs() {
        ElbowAxis::HorizontalFirst
    } else {
        ElbowAxis::VerticalFirst
    }
}

/// 非零方向向量 → 有符号单位轴向。
fn unit_dir(d: (f32, f32)) -> (f32, f32) {
    if d.0.abs() >= d.1.abs() {
        (d.0.signum(), 0.0)
    } else {
        (0.0, d.1.signum())
    }
}

/// 绑定锚点 → 单位 heading（4 向，与旋转无关——Excalidraw heading 同为 4 向）：
/// 锚点在目标局部 AABB 哪条边，heading 即该边**外法线**（离开形状的方向）。
/// 终点的进入方向取其相反数（调用方处理）。边判定与 `elbow_axis_from_anchor`
/// 一致（到四边最近者）。
pub fn elbow_heading_from_anchor(size: (f32, f32), anchor: (f32, f32)) -> (f32, f32) {
    let dist = (anchor.0, size.0 - anchor.0, anchor.1, size.1 - anchor.1);
    if dist.0.min(dist.1) <= dist.2.min(dist.3) {
        if dist.0 <= dist.1 {
            (-1.0, 0.0)
        } else {
            (1.0, 0.0)
        }
    } else if dist.2 <= dist.3 {
        (0.0, -1.0)
    } else {
        (0.0, 1.0)
    }
}

/// 合并共线相邻段、去零长点（路由输出的最后整形：拐点唯一、无重复点）。
fn merge_collinear(pts: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = Vec::with_capacity(pts.len());
    for p in pts {
        if let Some(last) = out.last() {
            if (last.0 - p.0).abs() < EPS && (last.1 - p.1).abs() < EPS {
                continue;
            }
        }
        out.push(p);
    }
    let mut i = 1;
    while out.len() > 2 && i < out.len() - 1 {
        let (a, m, b) = (out[i - 1], out[i], out[i + 1]);
        let collinear = (a.0 - m.0).abs() < EPS && (m.0 - b.0).abs() < EPS
            || (a.1 - m.1).abs() < EPS && (m.1 - b.1).abs() < EPS;
        if collinear {
            out.remove(i);
        } else {
            i += 1;
        }
    }
    out
}

// ─────────────────────────── A\*（非均匀网格） ───────────────────────────

/// 非均匀网格 A\*：网格坐标 = 两端点 + 障碍边线 + union 外扩一档（保证能绕到
/// 外侧）；障碍边恒在网格线上 → 相邻节点段的中点落障判据**精确**。首步限
/// `start_heading`、末步限 `end_heading`（Some）；禁止立即反向；转弯罚 =
/// 曼哈顿距离³（对齐 Excalidraw bendPenalty 量级：最少转弯优先、路程次之）。
/// 确定性：邻居固定顺序展开、堆并列按插入序破平。节点数 >4096 视为异常放弃
/// （上层回退确定性规则）。
fn astar(
    start: (f32, f32),
    end: (f32, f32),
    start_heading: (f32, f32),
    end_heading: Option<(f32, f32)>,
    obstacles: &[(f32, f32, f32, f32)],
) -> Option<Vec<(f32, f32)>> {
    if obstacles.is_empty() {
        return None; // 无障碍不该进 A\*（上层已分流）
    }
    let (mut xs, mut ys) = (vec![start.0, end.0], vec![start.1, end.1]);
    let (mut u_min_x, mut u_min_y) = (start.0.min(end.0), start.1.min(end.1));
    let (mut u_max_x, mut u_max_y) = (start.0.max(end.0), start.1.max(end.1));
    for &(x0, y0, x1, y1) in obstacles {
        xs.push(x0);
        xs.push(x1);
        ys.push(y0);
        ys.push(y1);
        u_min_x = u_min_x.min(x0);
        u_min_y = u_min_y.min(y0);
        u_max_x = u_max_x.max(x1);
        u_max_y = u_max_y.max(y1);
    }
    xs.push(u_min_x - ELBOW_PADDING);
    xs.push(u_max_x + ELBOW_PADDING);
    ys.push(u_min_y - ELBOW_PADDING);
    ys.push(u_max_y + ELBOW_PADDING);
    xs.sort_by(|a, b| a.total_cmp(b));
    ys.sort_by(|a, b| a.total_cmp(b));
    xs.dedup_by(|a, b| (*a - *b).abs() < EPS);
    ys.dedup_by(|a, b| (*a - *b).abs() < EPS);
    let nx = xs.len();
    let ny = ys.len();
    if nx.saturating_mul(ny) > 4096 {
        return None;
    }
    let (Some(si), Some(sj)) = (
        xs.iter().position(|&v| (v - start.0).abs() < EPS),
        ys.iter().position(|&v| (v - start.1).abs() < EPS),
    ) else {
        return None;
    };
    let (Some(ei), Some(ej)) = (
        xs.iter().position(|&v| (v - end.0).abs() < EPS),
        ys.iter().position(|&v| (v - end.1).abs() < EPS),
    ) else {
        return None;
    };

    let midpoint_blocked = |a: (f32, f32), b: (f32, f32)| -> bool {
        let m = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
        obstacles.iter().any(|&(x0, y0, x1, y1)| {
            m.0 > x0 + EPS && m.0 < x1 - EPS && m.1 > y0 + EPS && m.1 < y1 - EPS
        })
    };

    // 方向编码：0=无（起点），1..4 = +x/-x/+y/-y。
    let dir_code = |d: (f32, f32)| -> u8 {
        if d.0.abs() >= d.1.abs() {
            if d.0 > 0.0 {
                1
            } else if d.0 < 0.0 {
                2
            } else {
                0
            }
        } else if d.1 > 0.0 {
            3
        } else if d.1 < 0.0 {
            4
        } else {
            0
        }
    };
    let start_dir = dir_code(start_heading);
    let end_dir = end_heading.map(dir_code);
    let is_reverse = |a: u8, b: u8| matches!((a, b), (1, 2) | (2, 1) | (3, 4) | (4, 3));

    #[derive(Clone, PartialEq)]
    struct State {
        f: f64,
        seq: u64,
        i: usize,
        j: usize,
        dir: u8,
    }
    impl Eq for State {}
    impl PartialOrd for State {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for State {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            // BinaryHeap 是最大堆：反比较取最小 f；同 f 按插入序（seq 小者先）。
            other.f.total_cmp(&self.f).then(other.seq.cmp(&self.seq))
        }
    }

    let m = ((end.0 - start.0).abs() + (end.1 - start.1).abs()) as f64;
    let bend_penalty = m * m * m;
    // 状态 = (节点, 进入方向)：转弯罚依赖 last_dir，同一节点不同方向是不同
    // 状态（否则目标点可能被错误方向的路径占坑）。dir 0 仅起点。
    let stride = nx * ny;
    let mut dist = vec![f64::INFINITY; 5 * stride];
    let mut came = vec![None::<(u8, usize, usize)>; 5 * stride];
    let mut done = vec![false; 5 * stride];
    let mut heap = BinaryHeap::new();
    let mut seq: u64 = 0;
    dist[sj * nx + si] = 0.0;
    heap.push(State {
        f: 0.0,
        seq: 0,
        i: si,
        j: sj,
        dir: 0,
    });

    while let Some(State { i, j, dir, .. }) = heap.pop() {
        let key = dir as usize * stride + j * nx + i;
        if done[key] {
            continue; // 陈旧堆条目（惰性删除）
        }
        done[key] = true;
        if (i, j) == (ei, ej) && (end_dir.is_none() || end_dir == Some(dir)) {
            let mut pts = Vec::new();
            let mut cur = (dir, i, j);
            loop {
                pts.push((xs[cur.1], ys[cur.2]));
                match came[cur.0 as usize * stride + cur.2 * nx + cur.1] {
                    Some(prev) => cur = prev,
                    None => break,
                }
            }
            pts.reverse();
            return Some(pts);
        }
        let cur_g = dist[key];
        for (dx, dy) in [(1usize, 0usize), (usize::MAX, 0), (0, 1), (0, usize::MAX)] {
            let (ni, nj) = match (dx, dy) {
                (1, 0) => (i + 1, j),
                (usize::MAX, 0) => (i.wrapping_sub(1), j),
                (0, 1) => (i, j + 1),
                _ => (i, j.wrapping_sub(1)),
            };
            if ni >= nx || nj >= ny || (ni == i && nj == j) {
                continue;
            }
            let d = dir_code((xs[ni] - xs[i], ys[nj] - ys[j]));
            if dir == 0 {
                if d != start_dir {
                    continue; // 首步必须沿出发 heading
                }
            } else if is_reverse(d, dir) {
                continue; // 禁止立即反向
            }
            let a = (xs[i], ys[j]);
            let b = (xs[ni], ys[nj]);
            if midpoint_blocked(a, b) {
                continue;
            }
            let step_len = ((b.0 - a.0).abs() + (b.1 - a.1).abs()) as f64;
            let turn = if dir != 0 && d != dir {
                bend_penalty
            } else {
                0.0
            };
            let ng = cur_g + step_len + turn;
            let nkey = d as usize * stride + nj * nx + ni;
            if ng + 1e-9 < dist[nkey] {
                dist[nkey] = ng;
                came[nkey] = Some((dir, i, j));
                let h = ((end.0 - b.0).abs() + (end.1 - b.1).abs()) as f64;
                seq += 1;
                heap.push(State {
                    f: ng + h,
                    seq,
                    i: ni,
                    j: nj,
                    dir: d,
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试便捷入口：`bounds = (start_bound, end_bound)`。
    fn route(
        start: (f32, f32),
        end: (f32, f32),
        sh: (f32, f32),
        eh: (f32, f32),
        bounds: (bool, bool),
        fixed: &[ElbowFixedSegment],
        obstacles: &[(f32, f32, f32, f32)],
    ) -> Vec<(f32, f32)> {
        elbow_route(&ElbowRouteInput {
            start,
            end,
            start_heading: sh,
            end_heading: eh,
            start_bound: bounds.0,
            end_bound: bounds.1,
            offset: 0.0,
            fixed_segments: fixed,
            obstacles,
        })
    }

    fn assert_orthogonal_endpoint_preserving(pts: &[(f32, f32)], a: (f32, f32), b: (f32, f32)) {
        assert_eq!(pts.first(), Some(&a), "首点 = 起点");
        assert_eq!(pts.last(), Some(&b), "末点 = 终点");
        for w in pts.windows(2) {
            let axis_aligned = (w[0].0 - w[1].0).abs() < 1e-3 || (w[0].1 - w[1].1).abs() < 1e-3;
            assert!(axis_aligned, "段 {:?}→{:?} 非正交", w[0], w[1]);
        }
    }

    fn assert_avoids(pts: &[(f32, f32)], obstacles: &[(f32, f32, f32, f32)]) {
        for seg in pts.windows(2) {
            let m = ((seg[0].0 + seg[1].0) * 0.5, (seg[0].1 + seg[1].1) * 0.5);
            for &(x0, y0, x1, y1) in obstacles {
                assert!(
                    !(m.0 > x0 + 1e-3 && m.0 < x1 - 1e-3 && m.1 > y0 + 1e-3 && m.1 < y1 - 1e-3),
                    "段 {:?}→{:?} 中点 ({:.1},{:.1}) 落入障碍",
                    seg[0],
                    seg[1],
                    m.0,
                    m.1
                );
            }
        }
    }

    #[test]
    fn unbound_no_obstacles_matches_historic_z() {
        // 两端自由、无障碍、无固定段 = 现行确定性 Z（旧档视觉不变承诺）。
        let pts = [(0.0, 0.0), (100.0, 40.0)];
        // dx=100 > dy=40 → 存储取向 VerticalFirst → heading 沿 y 朝另一端。
        let out = route(
            pts[0],
            pts[1],
            (0.0, 1.0),
            (0.0, 1.0),
            (false, false),
            &[],
            &[],
        );
        assert_eq!(
            out,
            crate::item::elbow_polyline_offset(&pts, 0.0, ElbowAxis::VerticalFirst)
        );
    }

    #[test]
    fn astar_routes_around_obstacle_with_dongle_legs() {
        // 两端绑定：插座腿（40px 沿法线）+ A* 绕开中间障碍。
        let obstacles = [(50.0, -10.0, 250.0, 10.0)];
        let out = route(
            (0.0, 0.0),
            (300.0, 0.0),
            (1.0, 0.0),
            (1.0, 0.0),
            (true, true),
            &[],
            &obstacles,
        );
        assert_orthogonal_endpoint_preserving(&out, (0.0, 0.0), (300.0, 0.0));
        assert_avoids(&out, &obstacles);
        // 直线被障 → 路由离开中线，至少沿膨胀边界（|y| = 10，即 padding 边界）
        // 或绕到其外——与 Excalidraw 同款"贴 padding 边界绕行"观感。
        assert!(out.iter().any(|&(_, y)| y.abs() >= 10.0 - 1e-3));
        // 首段沿 heading 离开起点（插座腿直行时与后续共线合并，不保留冗余点）。
        assert_eq!(out[1].1, 0.0);
        assert!(out[1].0 > 0.0);
    }

    #[test]
    fn astar_deterministic_for_same_input() {
        let obstacles = [(50.0, -10.0, 250.0, 10.0)];
        let a = route(
            (0.0, 0.0),
            (300.0, 0.0),
            (1.0, 0.0),
            (1.0, 0.0),
            (true, true),
            &[],
            &obstacles,
        );
        let b = route(
            (0.0, 0.0),
            (300.0, 0.0),
            (1.0, 0.0),
            (1.0, 0.0),
            (true, true),
            &[],
            &obstacles,
        );
        assert_eq!(a, b, "同输入必须同输出（同源不变量前提）");
    }

    #[test]
    fn astar_failure_falls_back_to_deterministic() {
        // 障碍包围起点（首步即被堵死）→ A* 无解 → 回退确定性 Z。
        let obstacles = [(39.0, -50.0, 41.0, 50.0)];
        let out = route(
            (0.0, 0.0),
            (300.0, 0.0),
            (1.0, 0.0),
            (1.0, 0.0),
            (true, false),
            &[],
            &obstacles,
        );
        assert_orthogonal_endpoint_preserving(&out, (0.0, 0.0), (300.0, 0.0));
        // 回退层不再保证避障（回退是"有路可走"的底线）。
        assert!(out.len() >= 2);
    }

    #[test]
    fn fixed_segments_stitch_into_route() {
        // 无障碍 + 固定段：路由按坐标锚定穿过固定段（DP-5）。
        let fixed = [ElbowFixedSegment {
            start: (0.0, 20.0),
            end: (100.0, 20.0),
        }];
        let out = route(
            (0.0, 0.0),
            (200.0, 0.0),
            (1.0, 0.0),
            (1.0, 0.0),
            (false, false),
            &fixed,
            &[],
        );
        assert_orthogonal_endpoint_preserving(&out, (0.0, 0.0), (200.0, 0.0));
        // 固定段几何必须在路径上（首点、末点或共线覆盖）。
        assert!(out.contains(&(0.0, 20.0)) || out.contains(&(100.0, 20.0)));
        assert!(
            out.iter().any(|&(_, y)| (y - 20.0).abs() < 1e-3),
            "路径过 y=20 走线"
        );
    }

    #[test]
    fn fixed_segments_chain_ordering_is_greedy_nearest() {
        // 两个固定段：链接顺序 = 贪心最近端点（DP-5 不存 index）。
        let fixed = [
            ElbowFixedSegment {
                start: (200.0, 100.0),
                end: (300.0, 100.0),
            },
            ElbowFixedSegment {
                start: (0.0, 50.0),
                end: (50.0, 50.0),
            },
        ];
        let out = route(
            (0.0, 0.0),
            (400.0, 200.0),
            (1.0, 0.0),
            (1.0, 0.0),
            (false, false),
            &fixed,
            &[],
        );
        assert_orthogonal_endpoint_preserving(&out, (0.0, 0.0), (400.0, 200.0));
        // 两个固定段的 y 都应出现在路径上。
        assert!(out.iter().any(|&(_, y)| (y - 50.0).abs() < 1e-3));
        assert!(out.iter().any(|&(_, y)| (y - 100.0).abs() < 1e-3));
    }

    #[test]
    fn aligned_or_degenerate_returns_straight_line() {
        let out = route(
            (0.0, 0.0),
            (100.0, 0.0),
            (1.0, 0.0),
            (1.0, 0.0),
            (false, false),
            &[],
            &[],
        );
        assert_eq!(out, vec![(0.0, 0.0), (100.0, 0.0)]);
        let out = route(
            (0.0, 0.0),
            (0.0, 100.0),
            (0.0, 1.0),
            (0.0, 1.0),
            (false, false),
            &[],
            &[],
        );
        assert_eq!(out, vec![(0.0, 0.0), (0.0, 100.0)]);
    }

    #[test]
    fn fixed_segment_serde_roundtrip() {
        let seg = ElbowFixedSegment {
            start: (1.5, 2.5),
            end: (3.5, 2.5),
        };
        let json = serde_json::to_string(&seg).unwrap();
        let back: ElbowFixedSegment = serde_json::from_str(&json).unwrap();
        assert_eq!(seg, back);
    }
}
