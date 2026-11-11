//! elbow 连接器 A\* 路由（plan #24 阶段 C/D，DP-5/DP-6 拍板见 `.agents/plan.md`）。
//!
//! 分层模型（对齐 Excalidraw `elbowArrow.ts` master）：
//! - 任一端**绑定** → A\* 网格避障：障碍 = 两端绑定形状的旋转 AABB 四边膨胀
//!   有效 padding（标称 [`ELBOW_PADDING`]，对齐 Excalidraw `BASE_PADDING`；
//!   两端间隙不足时按实际间距自适应收缩，见 [`effective_padding`]）；首/末段
//!   = 沿锚点边法线的"插座腿"（dongle），长度恒等于有效 padding。**不做全场景
//!   绕行**（Excalidraw master 亦未做，issue #8635 开放中）。
//! - 两端自由（且无障碍）→ 确定性 L/Z/S（`elbow_mid_offset` bar 语义，旧档
//!   视觉不变）。
//! - `fixed_segments`（阶段 D）：固定段按坐标锚定进路由——锚点间子路由用 A\*
//!   （有障碍）或确定性最小连接（无障碍）缝合；固定段本身原样插入。
//! - A\* 失败 / 退化 → 回退确定性规则，**但保留插座腿**（缝合站点取 sv/ev）。
//! - **跳变根治（2026-11-11）**：固定 40px 腿长在两端间隙 < 80px 时把虚拟
//!   节点送进对方膨胀 AABB，A\* 必败并静默回退成丢弃插座腿的 Z —— 拖动跨过
//!   80px 即整条换形态（用户可见 180° 掉头）。现 padding 与腿长同步随间隙
//!   连续收缩，虚拟节点恒落在自身障碍边界线上、两障碍间恒留通道；间隙充足
//!   时 padding 仍为标称值，输出与旧实现逐点一致。
//!
//! 网格是**非均匀网格**（Excalidraw `calculateGrid` 同思路）：坐标 = 两端虚拟
//! 节点 + 障碍边线 ± union 外扩，取 x/y 笛卡尔交点为节点，规模 O(k²)（典型
//! <150 节点），每帧重算免缓存——纯函数，同源不变量（命中/渲染/手柄/导出）
//! 免费保持。输出恒为正交折线、首末点与输入端点一致。

use std::collections::BinaryHeap;

use serde::{Deserialize, Serialize};

use crate::item::elbow_polyline_offset;
use crate::shape::ElbowAxis;

/// 绑定端首/末"插座腿"长度与障碍膨胀量的**标称上限**（画布 px，对齐
/// Excalidraw `BASE_PADDING`）：离开/进入绑定形状时沿锚点边法线先走这一段，
/// 再进入自由路由。两端绑定形状靠得比 2× 本值更近时，路由按实际间隙收缩该值
/// （[`effective_padding`]）；间隙充足时恒为本值，旧档视觉不变。
pub const ELBOW_PADDING: f32 = 40.0;

/// 自适应收缩时留出的通道余量（画布 px）：两障碍膨胀后至少留 2× 本值的空隙，
/// 避免浮点边界判定把通道吃掉、A\* 首/末步再次被判成落在障碍内部。
const ELBOW_GAP_CLEARANCE: f32 = 1.0;

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
    /// 障碍 AABB `(min_x, min_y, max_x, max_y)`：**调用方按 [`ELBOW_PADDING`]
    /// 膨胀后**的禁入区（唯一生产者 `Scene::elbow_route_local`）。路由层按有效
    /// padding 重新膨胀（[`reinflate_obstacles`]）——标称值生效时是恒等变换。
    /// 空 = 无避障。
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
    // 自由端存储取向可能与端点连线垂直（如 VerticalFirst + 纯水平连接），
    // 此时 heading = (0,0)，A\* 的方向编码为 0、末步约束整体失效（末段可能
    // 从任意方向进入端点）。用 start→end 主方向兜底；确定性层仍消费原
    // heading，旧档视觉不变。
    let main = unit_dir((end.0 - start.0, end.1 - start.1));
    let sh = if dir_code(input.start_heading) == 0 {
        main
    } else {
        input.start_heading
    };
    let eh = if dir_code(input.end_heading) == 0 {
        main
    } else {
        input.end_heading
    };
    // 有效 padding 与插座腿同步随两端间隙收缩：sv/ev 恒落在自身障碍边界线
    // 上、两障碍之间恒留通道，不再有"间隙 < 2×40 就 A\* 必败"的硬阈值。
    let padding = effective_padding(input.obstacles);
    let inflated = reinflate_obstacles(input.obstacles, padding);
    let obstacles: &[(f32, f32, f32, f32)] = &inflated;
    let sv = virtual_node(start, sh, input.start_bound, padding, 1.0);
    let ev = virtual_node(end, eh, input.end_bound, padding, -1.0);
    // 所有失败/退化路径统一回退到"保留插座腿"的缝合层，不再整条退化成 Z。
    let fallback = || join_chain_with_sockets(input, sv, ev, sh, eh);
    if manhattan(sv, ev) < EPS {
        return fallback();
    }
    let Some(stations) = chain_stations(start, input.fixed_segments, ev) else {
        return fallback();
    };
    let mut out: Vec<(f32, f32)> = vec![start];
    if input.start_bound {
        out.push(sv);
    }
    // 逐站点 A\*：出发方向 = 上一段延续方向；终点进入方向 = 下一站约束。
    let mut cur = sv;
    let mut cur_heading = sh;
    let mut failed = false;
    for st in &stations {
        let (target, arrival) = match st {
            // 固定段进入端也要约束：到达方向若与段方向反平行，接入段本体
            // 就会形成 180° 回折（merge_collinear 还会静默删掉共线回折点）。
            Station::Anchor(a) => (*a, anchor_arrival(input.fixed_segments, *a)),
            Station::End(e) => (*e, Some(eh)),
        };
        match astar(cur, target, cur_heading, arrival, obstacles) {
            Some(mut seg) => {
                seg.remove(0); // 首点 == cur，避免重合
                out.extend(seg);
            }
            None => {
                failed = true;
                break;
            }
        }
        if let Station::Anchor(a) = st {
            // 插入固定段本体（a 到段另一端），继续沿段方向出发。
            let Some((from, to)) = fixed_segment_from(input.fixed_segments, *a) else {
                failed = true;
                break;
            };
            out.push(to);
            cur = to;
            cur_heading = unit_dir((to.0 - from.0, to.1 - from.1));
        }
    }
    if failed {
        return fallback();
    }
    if input.end_bound {
        out.push(ev);
        out.push(end);
    }
    let out = merge_collinear(out);
    // 端点不变量兜底（异常即回退，绝不输出变形路径）。
    if out.len() < 2 || out[0] != start || *out.last().unwrap() != end {
        return fallback();
    }
    out
}

/// 绑定端虚拟节点：`sign=1`（起点）沿 heading 外推 `padding`；`sign=-1`
/// （终点）沿进入 heading 反向内推（虚拟节点在端点后方 padding 处，恰落在
/// 自身膨胀障碍的边界线上——`padding` 与障碍膨胀量同源，这是无硬阈值的关键）。
fn virtual_node(
    p: (f32, f32),
    heading: (f32, f32),
    bound: bool,
    padding: f32,
    sign: f32,
) -> (f32, f32) {
    if bound {
        (
            p.0 + heading.0 * padding * sign,
            p.1 + heading.1 * padding * sign,
        )
    } else {
        p
    }
}

/// 有效 padding（跳变根治）：`obstacles` 是调用方按标称值膨胀后的禁入区，
/// 先收回原始形状 AABB 量出两两之间的分离量，再取 `min(标称, sep/2 - 余量)`。
/// 只有一个障碍（或没有）时不存在"对方形状"，恒用标称值；分离量连续变化
/// → padding 连续变化 → 路径不再跨阈值整条换形态。
fn effective_padding(obstacles: &[(f32, f32, f32, f32)]) -> f32 {
    let raw: Vec<(f32, f32, f32, f32)> = obstacles
        .iter()
        .map(|&(x0, y0, x1, y1)| {
            (
                x0 + ELBOW_PADDING,
                y0 + ELBOW_PADDING,
                x1 - ELBOW_PADDING,
                y1 - ELBOW_PADDING,
            )
        })
        .collect();
    let mut sep = f32::INFINITY;
    for (i, a) in raw.iter().enumerate() {
        for b in raw.iter().skip(i + 1) {
            // 两 AABB 在 x 或 y 任一轴上分离即可走线；取较宽松的一轴。
            let gx = (b.0 - a.2).max(a.0 - b.2);
            let gy = (b.1 - a.3).max(a.1 - b.3);
            sep = sep.min(gx.max(gy).max(0.0));
        }
    }
    if !sep.is_finite() {
        return ELBOW_PADDING;
    }
    ((sep - 2.0 * ELBOW_GAP_CLEARANCE) * 0.5).clamp(0.0, ELBOW_PADDING)
}

/// 按有效 padding 重新膨胀障碍。标称值生效时直接返回原数组——恒等变换，
/// 避免 `x0 + P - P` 的浮点扰动渗进网格坐标（间隙充足的常规/旧档场景逐点
/// 保持改动前的输出）。
fn reinflate_obstacles(
    obstacles: &[(f32, f32, f32, f32)],
    padding: f32,
) -> Vec<(f32, f32, f32, f32)> {
    if (padding - ELBOW_PADDING).abs() < EPS {
        return obstacles.to_vec();
    }
    obstacles
        .iter()
        .map(|&(x0, y0, x1, y1)| {
            (
                x0 + ELBOW_PADDING - padding,
                y0 + ELBOW_PADDING - padding,
                x1 - ELBOW_PADDING + padding,
                y1 - ELBOW_PADDING + padding,
            )
        })
        .collect()
}

/// 固定段在锚点 `a` 处的进入/离开方向（供 A\* 末步约束）；段不存在返回 None。
fn anchor_arrival(fixed: &[ElbowFixedSegment], a: (f32, f32)) -> Option<(f32, f32)> {
    let (from, to) = fixed_segment_from(fixed, a)?;
    Some(unit_dir((to.0 - from.0, to.1 - from.1)))
}

/// 找出以 `a` 为端点的固定段，返回 `(a, 另一端)`（方向按路由行进方向）。
fn fixed_segment_from(
    fixed: &[ElbowFixedSegment],
    a: (f32, f32),
) -> Option<((f32, f32), (f32, f32))> {
    let seg = fixed.iter().find(|s| s.start == a || s.end == a).copied()?;
    Some(if seg.start == a {
        (seg.start, seg.end)
    } else {
        (seg.end, seg.start)
    })
}

/// 确定性缝合链（无障碍层）：固定段原样插入，锚点间用最小正交连接（Z/L）；
/// 无固定段 = 现行 `elbow_polyline_offset`（旧档视觉不变）。
fn join_chain_deterministic(input: &ElbowRouteInput) -> Vec<(f32, f32)> {
    deterministic_between(
        input.start,
        input.end,
        input,
        axis_of(input.start_heading),
        axis_of(input.end_heading),
    )
}

/// A\* 失败 / 退化时的回退：**保留插座腿**。确定性缝合只在两个虚拟节点之间
/// 进行，再首尾各接一段 `start→sv` / `ev→end`。旧实现直接对整条 start→end
/// 走 Z，把绑定端的垂直进出段全丢了——间隙跨过 80px 时首段方向突然反转，
/// 正是用户看到的 180° 掉头。
fn join_chain_with_sockets(
    input: &ElbowRouteInput,
    sv: (f32, f32),
    ev: (f32, f32),
    sh: (f32, f32),
    eh: (f32, f32),
) -> Vec<(f32, f32)> {
    let middle = deterministic_between(sv, ev, input, axis_of(sh), axis_of(eh));
    let mut out: Vec<(f32, f32)> = vec![input.start];
    if input.start_bound {
        out.push(sv);
    }
    out.extend(middle.iter().skip(1)); // middle[0] == sv
    if input.end_bound {
        out.push(ev); // middle.last() == ev，重复点由 merge 去掉
    }
    out.push(input.end);
    let out = merge_collinear(out);
    // 两形状贴在一起且 padding→0 时全部点重合，merge 会压成单点；单点会让
    // 箭头方向取 NaN。保底输出两点（与历史退化行为一致）。
    if out.len() < 2 {
        vec![input.start, input.end]
    } else {
        out
    }
}

/// `a`→`b` 的确定性缝合，首尾轴由调用方给定（无障碍层 / 回退层共用）。
fn deterministic_between(
    a: (f32, f32),
    b: (f32, f32),
    input: &ElbowRouteInput,
    from_axis: ElbowAxis,
    to_axis: ElbowAxis,
) -> Vec<(f32, f32)> {
    if input.fixed_segments.is_empty() {
        return elbow_polyline_offset(&[a, b], input.offset, from_axis);
    }
    let Some(stations) = chain_stations(a, input.fixed_segments, b) else {
        return elbow_polyline_offset(&[a, b], input.offset, from_axis);
    };
    let mut out: Vec<(f32, f32)> = vec![a];
    let mut prev = a;
    let mut prev_axis = from_axis;
    for st in &stations {
        match st {
            Station::Anchor(anchor) => {
                let Some((from, to)) = fixed_segment_from(input.fixed_segments, *anchor) else {
                    break;
                };
                let seg_axis = axis_of((to.0 - from.0, to.1 - from.1));
                out.extend(minimal_join(prev, from, prev_axis, Some(seg_axis)));
                out.push(to);
                prev = to;
                prev_axis = seg_axis;
            }
            Station::End(e) => {
                out.extend(minimal_join(prev, *e, prev_axis, Some(to_axis)));
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

/// 方向编码：0 = 无（起点 / 零 heading），1..4 = +x/-x/+y/-y。A\* 的方向
/// 约束与 `elbow_route` 的零 heading 兜底共用同一套编码。
fn dir_code(d: (f32, f32)) -> u8 {
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
/// 外侧）；障碍边恒在网格线上 → 相邻节点段的中点落障判据**精确**。首步/末步
/// **禁止与 heading 相反**（直行延续与垂直转折均合法——插座腿关节处的拐弯是
/// 正常路径形状；180° 掉头非法）；途中禁止立即反向；转弯罚 = 曼哈顿距离³
/// （对齐 Excalidraw bendPenalty 量级：最少转弯优先、路程次之）。确定性：
/// 邻居固定顺序展开、堆并列按插入序破平。节点数 >4096 视为异常放弃（上层
/// 回退确定性规则）。
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
        // 末步约束：进入方向不得与 end_heading 相反（垂直转折 = 关节处拐弯
        // 合法；180° 掉头会与端点腿 ev→end 重叠成尖刺，非法）。
        if (i, j) == (ei, ej)
            && (end_dir.is_none() || dir == 0 || !is_reverse(dir, end_dir.unwrap()))
        {
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
                if is_reverse(d, start_dir) {
                    continue; // 首步不得与出发 heading 相反（掉头回穿过端点腿）
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

    /// 相邻两段不得反向重叠（180° 回折 / 尖刺）。
    fn assert_no_reversal(pts: &[(f32, f32)]) {
        for w in pts.windows(3) {
            let d1 = (w[1].0 - w[0].0, w[1].1 - w[0].1);
            let d2 = (w[2].0 - w[1].0, w[2].1 - w[1].1);
            assert!(d1.0 * d2.0 + d1.1 * d2.1 >= -1e-3, "180° 回折: {:?}", pts);
        }
    }

    /// 与 `Scene::elbow_route_local` 同款：形状 AABB 按标称值膨胀后的禁入区。
    fn inflated_rect(x: f32, y: f32, w: f32, h: f32) -> (f32, f32, f32, f32) {
        (
            x - ELBOW_PADDING,
            y - ELBOW_PADDING,
            x + w + ELBOW_PADDING,
            y + h + ELBOW_PADDING,
        )
    }

    /// 锚点边 → 外法线（终点进入方向 = 外法线取反）。
    fn edge_normal(edge: char) -> (f32, f32) {
        match edge {
            'L' => (-1.0, 0.0),
            'R' => (1.0, 0.0),
            'T' => (0.0, -1.0),
            _ => (0.0, 1.0),
        }
    }

    /// 100×60 形状上四条边的锚点。
    fn anchor_of(edge: char, ox: f32, oy: f32) -> (f32, f32) {
        match edge {
            'L' => (ox, oy + 30.0),
            'R' => (ox + 100.0, oy + 30.0),
            'T' => (ox + 50.0, oy),
            _ => (ox + 50.0, oy + 60.0),
        }
    }

    /// 两个 100×60 绑定形状：A 在原点，B 在 `(bx, by)`，两端按边取锚点。
    fn route_two_bound(edges: (char, char), bx: f32, by: f32) -> Vec<(f32, f32)> {
        let (a, b) = (anchor_of(edges.0, 0.0, 0.0), anchor_of(edges.1, bx, by));
        let n = edge_normal(edges.1);
        let obstacles = [
            inflated_rect(0.0, 0.0, 100.0, 60.0),
            inflated_rect(bx, by, 100.0, 60.0),
        ];
        route(
            a,
            b,
            edge_normal(edges.0),
            (-n.0, -n.1),
            (true, true),
            &[],
            &obstacles,
        )
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

    // ────────── 跳变根治（自适应 padding + 保留插座腿的回退） ──────────

    #[test]
    fn adaptive_padding_removes_80px_topology_jump() {
        // R→L 水平连接：旧实现在 gap = 2×ELBOW_PADDING 处 A\* 从"必败回退"
        // 翻到"成功绕行"，路径拓扑突变、首段方向反转。现在 padding 随间隙
        // 连续收缩，跨过 80px 时输出始终是干净的两点直线。
        for gap in [
            10.0, 20.0, 40.0, 60.0, 78.0, 79.0, 80.0, 81.0, 82.0, 100.0, 160.0, 200.0,
        ] {
            let out = route_two_bound(('R', 'L'), 100.0 + gap, 0.0);
            assert_eq!(out.len(), 2, "gap={gap} 路径拓扑跳变：{out:?}");
            assert_eq!(out[0], (100.0, 30.0));
            assert_eq!(out[1], (100.0 + gap, 30.0));
        }
    }

    #[test]
    fn narrow_gap_astar_keeps_dongle_legs() {
        // L→L：间隙 60 < 80，旧实现 ev 落进对方膨胀 AABB → A\* 必败、回退成
        // 穿过形状的 2 点直线。现在 padding 收缩到 29，A\* 有解且保留首段
        // 沿左边缘外法线（-x）离开的插座腿。
        let out = route_two_bound(('L', 'L'), 160.0, 0.0);
        assert_orthogonal_endpoint_preserving(&out, (0.0, 30.0), (160.0, 30.0));
        assert!(out.len() > 2, "窄间隙仍应走绕行路径：{out:?}");
        assert!(
            out[1].0 < 0.0 && (out[1].1 - 30.0).abs() < 1e-3,
            "首段应沿 -x 离开左边缘：{out:?}"
        );
        assert_no_reversal(&out);
    }

    #[test]
    fn effective_padding_tracks_gap_continuously() {
        let pad_at = |gap: f32| {
            effective_padding(&[
                inflated_rect(0.0, 0.0, 100.0, 60.0),
                inflated_rect(100.0 + gap, 0.0, 100.0, 60.0),
            ])
        };
        assert_eq!(pad_at(0.0), 0.0);
        assert_eq!(pad_at(40.0), 19.0);
        assert_eq!(pad_at(80.0), 39.0);
        assert_eq!(pad_at(82.0), ELBOW_PADDING);
        assert_eq!(pad_at(200.0), ELBOW_PADDING);
        // 0/1 个障碍时没有"对方形状"可撞，恒用标称值。
        assert_eq!(effective_padding(&[]), ELBOW_PADDING);
        assert_eq!(
            effective_padding(&[inflated_rect(0.0, 0.0, 100.0, 60.0)]),
            ELBOW_PADDING
        );
        // 连续性：间隙每变 1px，padding 最多变 0.5px 且单调不减。
        let mut prev = pad_at(0.0);
        for gap in 1..=200 {
            let p = pad_at(gap as f32);
            assert!((p - prev).abs() <= 0.5 + 1e-3, "gap={gap} padding 跳变");
            assert!(p >= prev, "gap={gap} padding 应单调不减");
            prev = p;
        }
    }

    #[test]
    fn nominal_padding_reinflation_is_identity() {
        // 间隙充足时障碍重膨胀必须是恒等变换——常规场景 / 旧档逐点不变。
        let obs = [
            inflated_rect(0.0, 0.0, 100.0, 60.0),
            inflated_rect(300.0, 0.0, 100.0, 60.0),
        ];
        let pad = effective_padding(&obs);
        assert_eq!(pad, ELBOW_PADDING);
        assert_eq!(reinflate_obstacles(&obs, pad), obs.to_vec());
    }

    #[test]
    fn astar_fallback_stitches_from_virtual_nodes() {
        // 单个薄障碍包住 sv → A\* 首步全堵。旧回退对 start→end 走 Z，
        // bar 在 x=200；新回退以 sv=(140,30) / ev=end(300,80) 缝合，
        // bar 移到 x=220（插座腿参与确定性 L/Z/S 的取轴与居中）。
        let obstacles = [(139.0, -20.0, 141.0, 100.0)];
        let out = route(
            (100.0, 30.0),
            (300.0, 80.0),
            (1.0, 0.0),
            (1.0, 0.0),
            (true, false),
            &[],
            &obstacles,
        );
        assert_orthogonal_endpoint_preserving(&out, (100.0, 30.0), (300.0, 80.0));
        assert!(
            out.iter().any(|&(x, _)| (x - 220.0).abs() < 1e-3),
            "回退应以 sv/ev 作缝合站点：{out:?}"
        );
        assert_no_reversal(&out);
    }

    #[test]
    fn degenerate_endpoints_keep_two_points() {
        // 两形状边界相接（sep=0 → padding=0）且首尾端点重合：merge 会把所有
        // 点压成单点，箭头方向将取 NaN。保底输出两点（与历史退化行为一致）。
        let obstacles = [
            inflated_rect(0.0, 0.0, 100.0, 60.0),
            inflated_rect(100.0, 0.0, 100.0, 60.0),
        ];
        let out = route(
            (100.0, 30.0),
            (100.0, 30.0),
            (1.0, 0.0),
            (1.0, 0.0),
            (true, true),
            &[],
            &obstacles,
        );
        assert_eq!(out, vec![(100.0, 30.0), (100.0, 30.0)]);
    }

    #[test]
    fn zero_free_end_heading_uses_main_direction() {
        // 自由端存储取向与连线垂直时会给出 (0,0)，A\* 的方向编码为 0 令
        // 末步约束整体失效。兜底后应与显式给主方向 heading 的结果一致。
        let obstacles = [inflated_rect(0.0, 0.0, 100.0, 60.0)];
        let zero = route(
            (100.0, 30.0),
            (300.0, 30.0),
            (1.0, 0.0),
            (0.0, 0.0),
            (true, false),
            &[],
            &obstacles,
        );
        let explicit = route(
            (100.0, 30.0),
            (300.0, 30.0),
            (1.0, 0.0),
            (1.0, 0.0),
            (true, false),
            &[],
            &obstacles,
        );
        assert_eq!(zero, explicit);
        assert_no_reversal(&zero);
    }

    #[test]
    fn anchor_sweep_has_no_reversal_and_preserves_invariants() {
        // 16 种锚点边组合 × 13 种水平间距 × 3 种垂直错位：端点、正交性、
        // 无 180° 回折的批量属性回归（对应跳变诊断时的网格扫描）。
        for ea in ['L', 'R', 'T', 'B'] {
            for eb in ['L', 'R', 'T', 'B'] {
                for bx in [
                    60.0, 80.0, 100.0, 120.0, 140.0, 160.0, 170.0, 180.0, 190.0, 200.0, 240.0,
                    300.0, 400.0,
                ] {
                    for by in [-120.0, 0.0, 120.0] {
                        let out = route_two_bound((ea, eb), bx, by);
                        assert!(out.len() >= 2, "{ea}{eb} bx={bx} by={by} 单点路径");
                        assert_orthogonal_endpoint_preserving(
                            &out,
                            anchor_of(ea, 0.0, 0.0),
                            anchor_of(eb, bx, by),
                        );
                        assert_no_reversal(&out);
                    }
                }
            }
        }
    }
}
