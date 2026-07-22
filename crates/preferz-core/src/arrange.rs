use crate::item::{Item, ItemId};
use crate::scene::Scene;
use crate::spaces::CanvasVector;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArrangeMode {
    Linear,
    Optimal,
    Grid,
}

/// 一次排列产生的移动列表 `(item_id, old_pos, new_pos)`。
/// UI 层应将其包装为 [`crate::commands::ArrangeItems`] 命令 push 到 undo 栈，
/// 而不是直接改 item.transform.pos（AGENTS.md: "All item mutations go through
/// the undo stack"）。
pub fn plan_arrange(scene: &Scene, mode: ArrangeMode, spacing: f32) -> Vec<(ItemId, CanvasVector, CanvasVector)> {
    // 按 z 序排列，保证顺序稳定
    let items: Vec<&Item> = scene.items_by_z_order();
    match mode {
        ArrangeMode::Linear => arrange_linear(&items, spacing),
        ArrangeMode::Optimal => arrange_optimal(&items, spacing),
        ArrangeMode::Grid => arrange_grid(&items, spacing),
    }
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
    let item_sizes: Vec<(f32, f32)> = items.iter()
        .map(|it| {
            let b = it.bounding_rect();
            (b.size.width, b.size.height)
        })
        .collect();

    // 容器宽度：取所有 item 的最大宽度 + spacing
    let container_w = item_sizes.iter()
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
                let max_y = free_rects.iter()
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
            if b.x <= a.x && b.y <= a.y
                && b.x + b.w >= a.x + a.w
                && b.y + b.h >= a.y + a.h
            {
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
    fr.x < pr.x + pr.w
        && fr.x + fr.w > pr.x
        && fr.y < pr.y + pr.h
        && fr.y + fr.h > pr.y
}

/// BSSF（Best Short Side Fit）：在所有空闲矩形中找最适合放置 (w, h) 的，
/// 返回 (空闲矩形索引, 是否旋转)。允许旋转 90° 以更紧凑装箱。
fn choose_best_free_rect(free_rects: &[FreeRect], w: f32, h: f32, spacing: f32) -> Option<(usize, bool)> {
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
                Some((_, _, bs, bl)) if (short, long) < (bs, bl) => best = Some((i, false, short, long)),
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
                Some((_, _, bs, bl)) if (short, long) < (bs, bl) => best = Some((i, true, short, long)),
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
        let new_pos = CanvasVector::new(
            col as f32 * (w + spacing),
            row as f32 * (h + spacing),
        );
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
}
