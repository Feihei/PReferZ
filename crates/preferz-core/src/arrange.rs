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
        let size = item.base_size();
        let new_pos = CanvasVector::new(x, 0.0);
        moves.push((item.id, item.transform.pos, new_pos));
        x += size.x + spacing;
    }
    moves
}

fn arrange_optimal(items: &[&Item], spacing: f32) -> Vec<(ItemId, CanvasVector, CanvasVector)> {
    // 简化：按"行宽达到首项宽度即换行"的策略装箱
    let mut x = 0.0_f32;
    let mut y = 0.0_f32;
    let mut row_max_h = 0.0_f32;
    let mut row_width = 0.0_f32;
    let mut container_width = 0.0_f32;
    let mut moves = Vec::with_capacity(items.len());

    for item in items {
        let size = item.base_size();
        if row_width + size.x + spacing > container_width && row_width > 0.0 {
            x = 0.0;
            y += row_max_h + spacing;
            row_max_h = 0.0;
            row_width = 0.0;
            container_width = size.x + spacing;
        }
        let new_pos = CanvasVector::new(x, y);
        moves.push((item.id, item.transform.pos, new_pos));

        row_width += size.x + spacing;
        if size.y > row_max_h {
            row_max_h = size.y;
        }
        x += size.x + spacing;
    }
    moves
}

fn arrange_grid(items: &[&Item], spacing: f32) -> Vec<(ItemId, CanvasVector, CanvasVector)> {
    let n = items.len();
    let cols = (n as f32).sqrt().ceil() as usize;
    let mut moves = Vec::with_capacity(n);

    // 用首行高度/宽度作为格子尺寸，简化
    for (i, item) in items.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let size = item.base_size();
        let new_pos = CanvasVector::new(
            col as f32 * (size.x + spacing),
            row as f32 * (size.y + spacing),
        );
        moves.push((item.id, item.transform.pos, new_pos));
    }
    moves
}
