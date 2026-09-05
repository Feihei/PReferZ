use crate::item::{Item, ItemId};
use crate::scene::Scene;
use crate::spaces::{CanvasRect, CanvasVector};
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArrangeMode {
    Linear,
    Optimal,
    Grid,
}

/// 一次移动：`(item_id, old_pos, new_pos)`。
type ItemMove = (ItemId, CanvasVector, CanvasVector);

/// 位置容差（画布单位）：小于此值的位移视为「无需移动」，不产生 undo 条目。
const MOVE_EPSILON: f32 = 1e-4;

/// 多元素对齐方式（6 向）。
///
/// 参考系为**选中项整体包围盒**（`plan.md` #6 决策点）：所有参与项贴向同一个框的
/// 对应边/中线，而不是各自贴向不同的锚。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignMode {
    /// 左对齐：各元素 AABB 左边界对齐到选区框左边界。
    Left,
    /// 水平居中：各元素 AABB 水平中心对齐到选区框水平中心。
    HCenter,
    /// 右对齐。
    Right,
    /// 顶对齐。
    Top,
    /// 垂直居中。
    VCenter,
    /// 底对齐。
    Bottom,
}

/// 多元素分布的轴向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistributeAxis {
    Horizontal,
    Vertical,
}

/// 多元素分布的基准（`plan.md` #6 决策点：两种都做）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistributeMode {
    /// 等间距：相邻元素的**边界空隙**相等（首尾元素不动）。
    /// Excalidraw / PowerPoint「横向分布」同款，元素大小不一时视觉最自然。
    Gap,
    /// 等中心距：相邻元素的**中心点**间距相等（首尾元素不动）。
    /// 元素大小差异大时中心节奏均匀，但视觉空隙不均。
    Centers,
}

/// 一次排列产生的移动列表 `(item_id, old_pos, new_pos)`。
/// UI 层应将其包装为 [`crate::commands::ArrangeItems`] 命令 push 到 undo 栈，
/// 而不是直接改 item.transform.pos（AGENTS.md: "All item mutations go through
/// the undo stack"）。
pub fn plan_arrange(
    scene: &Scene,
    mode: ArrangeMode,
    spacing: f32,
) -> Vec<(ItemId, CanvasVector, CanvasVector)> {
    // 按 z 序排列，保证顺序稳定
    let items: Vec<&Item> = scene.items_by_z_order();
    match mode {
        ArrangeMode::Linear => arrange_linear(&items, spacing),
        ArrangeMode::Optimal => arrange_optimal(&items, spacing),
        ArrangeMode::Grid => arrange_grid(&items, spacing),
    }
}

// ─────────────────────────── 对齐 / 分布 ───────────────────────────

/// 规划一次多元素**对齐**（`plan.md` #6）。
///
/// 参考系 = 参与项 AABB 的并集（选区包围盒）；`mode` 决定贴向哪条边/中线。
/// 返回 `(id, old_pos, new_pos)`，UI 层应包成
/// [`crate::commands::ArrangeItems`] 入 undo 栈。
///
/// 少于 2 个可对齐项时返回空（1 个元素对齐没有意义，不该产生 undo 条目）。
pub fn plan_align(scene: &Scene, ids: &[ItemId], mode: AlignMode) -> Vec<ItemMove> {
    let entries = alignable_entries(scene, ids);
    if entries.len() < 2 {
        return Vec::new();
    }
    let rects: Vec<CanvasRect> = entries.iter().map(|(_, r)| *r).collect();
    let Some(bounds) = union_rects(&rects) else {
        return Vec::new();
    };

    let mut moves = Vec::with_capacity(entries.len());
    for (item, r) in entries {
        let delta = match mode {
            AlignMode::Left => CanvasVector::new(bounds.min_x() - r.min_x(), 0.0),
            AlignMode::HCenter => CanvasVector::new(bounds.center().x - r.center().x, 0.0),
            AlignMode::Right => CanvasVector::new(bounds.max_x() - r.max_x(), 0.0),
            AlignMode::Top => CanvasVector::new(0.0, bounds.min_y() - r.min_y()),
            AlignMode::VCenter => CanvasVector::new(0.0, bounds.center().y - r.center().y),
            AlignMode::Bottom => CanvasVector::new(0.0, bounds.max_y() - r.max_y()),
        };
        push_move(&mut moves, item, delta);
    }
    moves
}

/// 规划一次多元素**分布**（`plan.md` #6）。
///
/// - `Gap`：首尾外边界之间等分空隙（相邻边界间隙相等）；
/// - `Centers`：首尾中心之间等分中心距。
///
/// 两种模式都**保持首尾元素不动**，仅调整中间项，符合 Excalidraw 行为。
/// 少于 3 个可分布项时返回空（2 个元素无需"分布"）。
pub fn plan_distribute(
    scene: &Scene,
    ids: &[ItemId],
    axis: DistributeAxis,
    mode: DistributeMode,
) -> Vec<ItemMove> {
    let mut entries = alignable_entries(scene, ids);
    if entries.len() < 3 {
        return Vec::new();
    }
    // 沿轴向按起点排序，确定谁是"首"、谁是"尾"
    entries.sort_by(|a, b| {
        let (ka, kb) = (axis_min(a.1, axis), axis_min(b.1, axis));
        ka.partial_cmp(&kb).unwrap_or(Ordering::Equal)
    });

    let n = entries.len();
    let mut moves = Vec::with_capacity(n);
    match mode {
        DistributeMode::Gap => {
            // 首尾外边界固定，中间按 (span - Σ尺寸) / (n-1) 的间隙铺开
            let start = axis_min(entries[0].1, axis);
            let end = axis_max(entries[n - 1].1, axis);
            let sum_size: f32 = entries.iter().map(|(_, r)| axis_size(*r, axis)).sum();
            let gap = (end - start - sum_size) / (n - 1) as f32;
            let mut cursor = start;
            for (item, r) in entries {
                let delta = axis_delta(axis, axis_min(r, axis) - cursor);
                push_move(&mut moves, item, delta);
                cursor += axis_size(r, axis) + gap;
            }
        }
        DistributeMode::Centers => {
            // 首尾中心固定，中心距等分
            let c0 = axis_center(entries[0].1, axis);
            let c1 = axis_center(entries[n - 1].1, axis);
            let step = (c1 - c0) / (n - 1) as f32;
            for (i, (item, r)) in entries.iter().enumerate() {
                let target = c0 + step * i as f32;
                let delta = axis_delta(axis, axis_center(*r, axis) - target);
                push_move(&mut moves, item, delta);
            }
        }
    }
    moves
}

/// 参与对齐/分布的项：`ids` 中真实存在、且**不是绑定文本**的 item。
///
/// 绑定文本（容器封闭 Shape 上的文字）位置由容器实时决定，独立平移会与容器错位
/// （Phase C 语义：不可独立选中/移动），因此一律排除——包括不参与包围盒计算。
fn alignable_entries<'a>(scene: &'a Scene, ids: &[ItemId]) -> Vec<(&'a Item, CanvasRect)> {
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(item) = scene.get_item(id) {
            if scene.is_bound_text(item) {
                continue;
            }
            out.push((item, item.bounding_rect()));
        }
    }
    out
}

fn union_rects(rects: &[CanvasRect]) -> Option<CanvasRect> {
    let mut acc: Option<CanvasRect> = None;
    for r in rects {
        acc = Some(match acc {
            Some(b) => b.union(r),
            None => *r,
        });
    }
    acc
}

fn axis_min(r: CanvasRect, axis: DistributeAxis) -> f32 {
    match axis {
        DistributeAxis::Horizontal => r.min_x(),
        DistributeAxis::Vertical => r.min_y(),
    }
}

fn axis_max(r: CanvasRect, axis: DistributeAxis) -> f32 {
    match axis {
        DistributeAxis::Horizontal => r.max_x(),
        DistributeAxis::Vertical => r.max_y(),
    }
}

fn axis_size(r: CanvasRect, axis: DistributeAxis) -> f32 {
    match axis {
        DistributeAxis::Horizontal => r.width(),
        DistributeAxis::Vertical => r.height(),
    }
}

fn axis_center(r: CanvasRect, axis: DistributeAxis) -> f32 {
    match axis {
        DistributeAxis::Horizontal => r.center().x,
        DistributeAxis::Vertical => r.center().y,
    }
}

/// 把「沿轴向的偏差量」转成带符号位移：`offset` 为 当前值 - 目标值。
fn axis_delta(axis: DistributeAxis, offset: f32) -> CanvasVector {
    match axis {
        DistributeAxis::Horizontal => CanvasVector::new(-offset, 0.0),
        DistributeAxis::Vertical => CanvasVector::new(0.0, -offset),
    }
}

/// 位移超过容差才记录，避免给"已经在位"的元素塞无意义 undo 条目。
fn push_move(moves: &mut Vec<ItemMove>, item: &Item, delta: CanvasVector) {
    if delta.length() <= MOVE_EPSILON {
        return;
    }
    moves.push((item.id, item.transform.pos, item.transform.pos + delta));
}

fn arrange_linear(items: &[&Item], spacing: f32) -> Vec<(ItemId, CanvasVector, CanvasVector)> {
    let mut x = 0.0_f32;
    let mut moves = Vec::with_capacity(items.len());
    for item in items {
        // 用画布空间实际占用宽度（应用 scale/rotation 后的 AABB），
        // 否则缩放过的 item 排列会留出大量空隙。
        let w = item.bounding_rect().size.width;
        // y 对齐到 0（item 的 pos 即其局部原点，最终 y 由调用方视情况调整）
        let new_pos = CanvasVector::new(x, 0.0);
        moves.push((item.id, item.transform.pos, new_pos));
        x += w + spacing;
    }
    moves
}

/// MaxRects 装箱算法（spec §2.2 批量操作：最优排列）。
///
/// 把所有 item 装入一个动态扩张的容器（高度随装填增长，宽度固定为最长 item 的宽度）。
/// MaxRects 维护"空闲矩形"列表，每个新 item 用 BSSF（Best Short Side Fit）启发式
/// 选择最佳空闲矩形：放入后剩余的短边最小。
///
/// 参考：Jukka Jylänki, "A Thousand Ways to Pack the Bin" (2009)。
fn arrange_optimal(items: &[&Item], spacing: f32) -> Vec<(ItemId, CanvasVector, CanvasVector)> {
    if items.is_empty() {
        return Vec::new();
    }

    // 用画布空间实际占用尺寸（应用 scale/rotation 后的 AABB），
    // 而非 base_size（未应用 scale 的原始尺寸），否则缩放过的 item 装箱会留出大量空隙。
    let item_sizes: Vec<(f32, f32)> = items
        .iter()
        .map(|it| {
            let b = it.bounding_rect();
            (b.size.width, b.size.height)
        })
        .collect();

    // 容器宽度：取所有 item 的最大宽度 + spacing
    let container_w = item_sizes
        .iter()
        .map(|(w, _)| *w + spacing)
        .fold(0.0_f32, f32::max)
        .max(64.0);

    // 初始空闲矩形：从 (0, 0) 开始，高度无限（向下扩张）
    let mut free_rects: Vec<FreeRect> = vec![FreeRect {
        x: 0.0,
        y: 0.0,
        w: container_w,
        h: f32::INFINITY,
    }];

    let mut moves = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let (w, h) = item_sizes[i];

        // BSSF：找最佳空闲矩形
        let best = choose_best_free_rect(&free_rects, w, h, spacing);
        let placed = match best {
            Some((idx, rotate)) => {
                let fr = &free_rects[idx];
                let (pw, ph) = if rotate { (h, w) } else { (w, h) };
                // placed 占用 = item 尺寸 + 1 倍 spacing（spacing 留作右侧/下方间隙）
                // 注意：choose_best_free_rect 已经按 w+spacing × h+spacing 判断能否装下，
                // 这里 placed.w/h 必须与之一致，否则会重复加 spacing 导致间距翻倍。
                PlacedRect {
                    x: fr.x,
                    y: fr.y,
                    w: pw + spacing,
                    h: ph + spacing,
                }
            }
            None => {
                // 装不下：在容器底部新开一行（高度扩张）
                let max_y = free_rects
                    .iter()
                    .filter(|fr| fr.h.is_finite())
                    .map(|fr| fr.y + fr.h)
                    .fold(0.0_f32, f32::max);
                PlacedRect {
                    x: 0.0,
                    y: max_y,
                    w: w + spacing,
                    h: h + spacing,
                }
            }
        };

        // 新位置（扣除 spacing，让 spacing 留在右下作为间隙）
        let new_pos = CanvasVector::new(placed.x, placed.y);
        moves.push((item.id, item.transform.pos, new_pos));

        // 更新空闲矩形：split 所有与 placed 相交的空闲矩形
        let mut new_free: Vec<FreeRect> = Vec::new();
        for fr in free_rects.iter() {
            if !rects_intersect(fr, &placed) {
                new_free.push(*fr);
                continue;
            }
            // 生成最多 4 个子矩形（左/右/上/下）
            // 上
            if fr.y < placed.y {
                new_free.push(FreeRect {
                    x: fr.x,
                    y: fr.y,
                    w: fr.w,
                    h: placed.y - fr.y,
                });
            }
            // 下
            if fr.y + fr.h > placed.y + placed.h {
                new_free.push(FreeRect {
                    x: fr.x,
                    y: placed.y + placed.h,
                    w: fr.w,
                    h: fr.y + fr.h - (placed.y + placed.h),
                });
            }
            // 左
            if fr.x < placed.x {
                new_free.push(FreeRect {
                    x: fr.x,
                    y: fr.y,
                    w: placed.x - fr.x,
                    h: fr.h,
                });
            }
            // 右
            if fr.x + fr.w > placed.x + placed.w {
                new_free.push(FreeRect {
                    x: placed.x + placed.w,
                    y: fr.y,
                    w: fr.x + fr.w - (placed.x + placed.w),
                    h: fr.h,
                });
            }
        }
        // 去除被其他空闲矩形包含的小矩形（MaxRects prune）
        prune_contained(&mut new_free);
        free_rects = new_free;
    }

    moves
}

/// 删除被其他空闲矩形包含的矩形（MaxRects prune 步骤）。
fn prune_contained(rects: &mut Vec<FreeRect>) {
    let mut to_remove: Vec<usize> = Vec::new();
    for i in 0..rects.len() {
        for j in 0..rects.len() {
            if i == j || to_remove.contains(&i) {
                continue;
            }
            let a = &rects[i];
            let b = &rects[j];
            // a 被 b 包含 → 移除 a
            if b.x <= a.x && b.y <= a.y && b.x + b.w >= a.x + a.w && b.y + b.h >= a.y + a.h {
                to_remove.push(i);
                break;
            }
        }
    }
    // 倒序移除以保持索引稳定
    for i in to_remove.into_iter().rev() {
        rects.remove(i);
    }
}

#[derive(Debug, Clone, Copy)]
struct FreeRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[derive(Debug, Clone, Copy)]
struct PlacedRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn rects_intersect(fr: &FreeRect, pr: &PlacedRect) -> bool {
    fr.x < pr.x + pr.w && fr.x + fr.w > pr.x && fr.y < pr.y + pr.h && fr.y + fr.h > pr.y
}

/// BSSF（Best Short Side Fit）：在所有空闲矩形中找最适合放置 (w, h) 的，
/// 返回 (空闲矩形索引, 是否旋转)。允许旋转 90° 以更紧凑装箱。
fn choose_best_free_rect(
    free_rects: &[FreeRect],
    w: f32,
    h: f32,
    spacing: f32,
) -> Option<(usize, bool)> {
    let pw = w + spacing;
    let ph = h + spacing;
    let mut best: Option<(usize, bool, f32, f32)> = None;
    for (i, fr) in free_rects.iter().enumerate() {
        // 不旋转
        if fr.w >= pw && fr.h >= ph {
            let leftover_w = fr.w - pw;
            let leftover_h = fr.h - ph;
            let short = leftover_w.min(leftover_h);
            let long = leftover_w.max(leftover_h);
            match best {
                None => best = Some((i, false, short, long)),
                Some((_, _, bs, bl)) if (short, long) < (bs, bl) => {
                    best = Some((i, false, short, long))
                }
                _ => {}
            }
        }
        // 旋转 90°
        if fr.w >= ph && fr.h >= pw {
            let leftover_w = fr.w - ph;
            let leftover_h = fr.h - pw;
            let short = leftover_w.min(leftover_h);
            let long = leftover_w.max(leftover_h);
            match best {
                None => best = Some((i, true, short, long)),
                Some((_, _, bs, bl)) if (short, long) < (bs, bl) => {
                    best = Some((i, true, short, long))
                }
                _ => {}
            }
        }
    }
    best.map(|(i, r, _, _)| (i, r))
}

fn arrange_grid(items: &[&Item], spacing: f32) -> Vec<(ItemId, CanvasVector, CanvasVector)> {
    let n = items.len();
    let cols = (n as f32).sqrt().ceil() as usize;
    let mut moves = Vec::with_capacity(n);

    // 用画布空间实际占用尺寸（应用 scale/rotation 后的 AABB）
    for (i, item) in items.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let bbox = item.bounding_rect();
        let w = bbox.size.width;
        let h = bbox.size.height;
        let new_pos = CanvasVector::new(col as f32 * (w + spacing), row as f32 * (h + spacing));
        moves.push((item.id, item.transform.pos, new_pos));
    }
    moves
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;
    use crate::scene::Scene;

    fn make_pixmap_item(w: u32, h: u32, pos_x: f32, pos_y: f32) -> Item {
        Item::new_pixmap(1, None, (w, h), pos_x, pos_y, 1.0, 1.0)
    }

    #[test]
    fn linear_arrange_layouts_horizontally() {
        let mut scene = Scene::new();
        scene.add_item(make_pixmap_item(100, 80, 0.0, 0.0));
        scene.add_item(make_pixmap_item(50, 60, 0.0, 0.0));
        let moves = plan_arrange(&scene, ArrangeMode::Linear, 10.0);
        assert_eq!(moves.len(), 2);
        // 第一项 x=0
        assert_eq_float(moves[0].2.x, 0.0);
        // 第二项 x = 100 + 10 = 110
        assert_eq_float(moves[1].2.x, 110.0);
    }

    #[test]
    fn optimal_arrange_uses_maxrects() {
        let mut scene = Scene::new();
        // 3 个不同尺寸 item
        scene.add_item(make_pixmap_item(100, 100, 5.0, 5.0));
        scene.add_item(make_pixmap_item(50, 50, 10.0, 10.0));
        scene.add_item(make_pixmap_item(80, 30, 20.0, 20.0));
        let moves = plan_arrange(&scene, ArrangeMode::Optimal, 8.0);
        assert_eq!(moves.len(), 3);
        // 所有新位置应在第一象限（>=0）
        for (_, _, new) in &moves {
            assert!(new.x >= 0.0 && new.y >= 0.0, "new_pos 应为非负: {:?}", new);
        }
        // 不应重叠（每个 item 占用 w+spacing × h+spacing 的格子）
        let items: Vec<&Item> = scene.items_by_z_order();
        let mut placed: Vec<(f32, f32, f32, f32)> = Vec::new();
        for (id, _, new) in &moves {
            let item = items.iter().find(|i| i.id == *id).unwrap();
            let s = item.base_size();
            let r = (new.x, new.y, new.x + s.x + 8.0, new.y + s.y + 8.0);
            for p in &placed {
                let overlap = r.0 < p.2 && r.2 > p.0 && r.1 < p.3 && r.3 > p.1;
                assert!(!overlap, "排列后不应重叠: {:?} vs {:?}", r, p);
            }
            placed.push(r);
        }
    }

    /// 验证：缩放过的 item 装箱按实际占用尺寸（scale 后），而非原始尺寸。
    /// 否则缩小到 0.1x 的 item 仍按原尺寸 1000×800 装箱，会留出大量空隙。
    #[test]
    fn arrange_uses_scaled_size_not_original() {
        let mut scene = Scene::new();
        // 1000×800 的图，缩放到 0.1x → 实际 100×80
        let mut a = Item::new_pixmap(1, None, (1000, 800), 0.0, 0.0, 1.0, 1.0);
        a.transform.scale = crate::spaces::CanvasVector::new(0.1, 0.1);
        scene.add_item(a);
        let mut b = Item::new_pixmap(2, None, (1000, 800), 0.0, 0.0, 1.0, 1.0);
        b.transform.scale = crate::spaces::CanvasVector::new(0.1, 0.1);
        scene.add_item(b);

        let moves = plan_arrange(&scene, ArrangeMode::Linear, 8.0);
        assert_eq!(moves.len(), 2);
        // 第一项 x=0；第二项 x = 100（实际宽度）+ 8（spacing）= 108
        // 若错误使用 base_size（1000），第二项 x 会是 1008
        assert_eq_float(moves[0].2.x, 0.0);
        assert_eq_float(moves[1].2.x, 108.0);
    }

    fn assert_eq_float(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-5, "float mismatch: {} vs {}", a, b);
    }

    // ── 对齐 / 分布（plan #6）──

    fn apply_moves(scene: &mut Scene, moves: &[ItemMove]) {
        for (id, _old, new) in moves {
            if let Some(it) = scene.get_item_mut(id) {
                it.transform.pos = *new;
            }
        }
    }

    fn sorted_edges(scene: &Scene, ids: &[ItemId], axis: DistributeAxis) -> Vec<(f32, f32)> {
        let mut v: Vec<(f32, f32)> = ids
            .iter()
            .filter_map(|id| scene.get_item(id))
            .map(|it| {
                let r = it.bounding_rect();
                match axis {
                    DistributeAxis::Horizontal => (r.min_x(), r.max_x()),
                    DistributeAxis::Vertical => (r.min_y(), r.max_y()),
                }
            })
            .collect();
        v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        v
    }

    /// 三个不同宽度的 item 横向排开（起点 x = 0 / 30 / 200），用于验证分布。
    fn distribute_fixture() -> (Scene, Vec<ItemId>) {
        let mut scene = Scene::new();
        let a = make_pixmap_item(20, 20, 0.0, 0.0);
        let b = make_pixmap_item(60, 20, 30.0, 0.0);
        let c = make_pixmap_item(20, 20, 200.0, 0.0);
        let ids = vec![a.id, b.id, c.id];
        scene.add_item(a);
        scene.add_item(b);
        scene.add_item(c);
        (scene, ids)
    }

    #[test]
    fn align_left_moves_items_to_selection_left_edge() {
        let mut scene = Scene::new();
        let a = make_pixmap_item(100, 80, 0.0, 0.0);
        let b = make_pixmap_item(40, 40, 50.0, 10.0);
        let (a_id, b_id) = (a.id, b.id);
        scene.add_item(a);
        scene.add_item(b);

        let moves = plan_align(&scene, &[a_id, b_id], AlignMode::Left);
        // 只有 b 需要移动（a 已在选区左边界上）
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].0, b_id);
        assert_eq_float(moves[0].1.x, 50.0);
        assert_eq_float(moves[0].2.x, 0.0);
        // 纵向不动
        assert_eq_float(moves[0].2.y, 10.0);
    }

    #[test]
    fn align_right_and_hcenter_align_opposite_edges() {
        let mut scene = Scene::new();
        let a = make_pixmap_item(100, 80, 0.0, 0.0); // 右边界 100，中心 50
        let b = make_pixmap_item(40, 40, 50.0, 0.0); // 右边界 90，中心 70
        let (a_id, b_id) = (a.id, b.id);
        scene.add_item(a);
        scene.add_item(b);

        // 右对齐：选区右边界 100 → b 右移 10
        let moves = plan_align(&scene, &[a_id, b_id], AlignMode::Right);
        assert_eq!(moves.len(), 1);
        assert_eq_float(moves[0].2.x, 60.0);

        // 水平居中：选区中心 50 → b 中心 70 需左移 20
        let moves = plan_align(&scene, &[a_id, b_id], AlignMode::HCenter);
        assert_eq!(moves.len(), 1);
        assert_eq_float(moves[0].2.x, 30.0);
    }

    #[test]
    fn align_vcenter_uses_vertical_axis_only() {
        let mut scene = Scene::new();
        let a = make_pixmap_item(100, 80, 0.0, 0.0); // 纵向中心 40
        let b = make_pixmap_item(40, 40, 0.0, 100.0); // 纵向中心 120
        let (a_id, b_id) = (a.id, b.id);
        scene.add_item(a);
        scene.add_item(b);

        // 选区纵向 0..140 → 中心 70：a（中心 40）下移 30，b（中心 120）上移 50
        let moves = plan_align(&scene, &[a_id, b_id], AlignMode::VCenter);
        assert_eq!(moves.len(), 2);
        let new_a = moves.iter().find(|(id, _, _)| *id == a_id).unwrap().2;
        let new_b = moves.iter().find(|(id, _, _)| *id == b_id).unwrap().2;
        assert_eq_float(new_a.y, 30.0);
        assert_eq_float(new_b.y, 50.0);
        // 横向完全不动
        assert_eq_float(new_a.x, 0.0);
        assert_eq_float(new_b.x, 0.0);
    }

    #[test]
    fn align_requires_two_items() {
        let mut scene = Scene::new();
        let a = make_pixmap_item(100, 80, 7.0, 3.0);
        let a_id = a.id;
        scene.add_item(a);
        assert!(plan_align(&scene, &[a_id], AlignMode::Left).is_empty());
    }

    #[test]
    fn align_skips_bound_text() {
        use crate::shape::ShapeType;
        let mut scene = Scene::new();
        let container = Item::new_shape(
            ShapeType::Rectangle,
            (100.0, 60.0),
            0.0,
            0.0,
            crate::shape::StrokeStyle::default(),
            None,
        );
        let c_id = container.id;
        let text = Item::new_text_in("hi".into(), 10.0, 10.0, 20.0, [255; 4], c_id);
        let t_id = text.id;
        let other = make_pixmap_item(50, 50, 80.0, 0.0);
        let o_id = other.id;
        scene.add_item(container);
        scene.add_item(text);
        scene.add_item(other);

        let moves = plan_align(&scene, &[c_id, t_id, o_id], AlignMode::Top);
        assert!(
            !moves.iter().any(|(id, _, _)| *id == t_id),
            "绑定文本不参与对齐：其位置由容器决定"
        );
    }

    #[test]
    fn distribute_gap_equalizes_boundary_gaps_and_keeps_ends() {
        let (mut scene, ids) = distribute_fixture();
        let before = sorted_edges(&scene, &ids, DistributeAxis::Horizontal);
        assert_eq_float(before[0].0, 0.0);
        assert_eq_float(before[2].1, 220.0);

        let moves = plan_distribute(
            &scene,
            &ids,
            DistributeAxis::Horizontal,
            DistributeMode::Gap,
        );
        apply_moves(&mut scene, &moves);

        let after = sorted_edges(&scene, &ids, DistributeAxis::Horizontal);
        // 首尾不动
        assert_eq_float(after[0].0, 0.0);
        assert_eq_float(after[2].1, 220.0);
        // 中间空隙相等：(220 - 0 - (20+60+20)) / 2 = 60
        let gap0 = after[1].0 - after[0].1;
        let gap1 = after[2].0 - after[1].1;
        assert_eq_float(gap0, 60.0);
        assert_eq_float(gap1, 60.0);
    }

    #[test]
    fn distribute_centers_equalizes_center_spacing() {
        let (mut scene, ids) = distribute_fixture();
        let moves = plan_distribute(
            &scene,
            &ids,
            DistributeAxis::Horizontal,
            DistributeMode::Centers,
        );
        apply_moves(&mut scene, &moves);

        let mut centers: Vec<f32> = ids
            .iter()
            .filter_map(|id| scene.get_item(id))
            .map(|it| it.bounding_rect().center().x)
            .collect();
        centers.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // 首尾中心 10 / 210 → 等分步长 100
        assert_eq_float(centers[0], 10.0);
        assert_eq_float(centers[1] - centers[0], 100.0);
        assert_eq_float(centers[2] - centers[1], 100.0);
    }

    #[test]
    fn distribute_vertical_axis_moves_y_only() {
        let mut scene = Scene::new();
        let a = make_pixmap_item(20, 20, 0.0, 0.0);
        let b = make_pixmap_item(20, 60, 0.0, 30.0);
        let c = make_pixmap_item(20, 20, 0.0, 200.0);
        let ids = vec![a.id, b.id, c.id];
        scene.add_item(a);
        scene.add_item(b);
        scene.add_item(c);

        let moves = plan_distribute(&scene, &ids, DistributeAxis::Vertical, DistributeMode::Gap);
        apply_moves(&mut scene, &moves);
        let after = sorted_edges(&scene, &ids, DistributeAxis::Vertical);
        let gap0 = after[1].0 - after[0].1;
        let gap1 = after[2].0 - after[1].1;
        assert_eq_float(gap0, gap1);
        // 横向完全没动
        for id in &ids {
            assert_eq_float(scene.get_item(id).unwrap().bounding_rect().min_x(), 0.0);
        }
    }

    #[test]
    fn distribute_requires_three_items() {
        let (scene, ids) = distribute_fixture();
        let two = &ids[..2];
        assert!(
            plan_distribute(&scene, two, DistributeAxis::Horizontal, DistributeMode::Gap)
                .is_empty()
        );
        assert!(plan_distribute(
            &scene,
            two,
            DistributeAxis::Vertical,
            DistributeMode::Centers
        )
        .is_empty());
    }
}
