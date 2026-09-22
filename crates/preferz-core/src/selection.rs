//! 框选（marquee）模式的纯决策：窗口 vs 交叉，及 item 命中判定。
//!
//! CAD 语义的拖拽版：起点到终点的**水平方向**决定模式——
//! - 左→右 = 窗口（Window）：仅完全包含于选框的 item 命中；
//! - 右→左 = 交叉（Crossing）：与选框相交或落在其中的 item 命中。
//!
//! 竖直方向不影响模式。纯竖直拖拽（x 相等）按窗口处理——零宽矩形本就
//! 无法"包含"任何 item，模式对结果等价，视觉也稳定不抖动。

use crate::spaces::{CanvasPoint, CanvasRect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxSelectMode {
    Window,
    Crossing,
}

impl BoxSelectMode {
    /// 依据起点与当前点的水平方向决定模式。
    pub fn from_drag(start: CanvasPoint, current: CanvasPoint) -> Self {
        if current.x >= start.x {
            BoxSelectMode::Window
        } else {
            BoxSelectMode::Crossing
        }
    }

    /// 判断 item 的 AABB 在给定选框下是否命中。`sel` 必须是已归一化
    /// （min ≤ max）的矩形；调用方从 `start`/`current` 组 rect 时自行 min/max。
    pub fn hits(self, sel: &CanvasRect, item_aabb: &CanvasRect) -> bool {
        match self {
            BoxSelectMode::Window => sel.contains_rect(item_aabb),
            BoxSelectMode::Crossing => item_aabb.intersects(sel),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spaces::CanvasSize;

    fn r(min_x: f32, min_y: f32, w: f32, h: f32) -> CanvasRect {
        CanvasRect::new(CanvasPoint::new(min_x, min_y), CanvasSize::new(w, h))
    }

    #[test]
    fn from_drag_left_to_right_is_window() {
        let s = CanvasPoint::new(0.0, 0.0);
        let c = CanvasPoint::new(10.0, -5.0); // 竖直方向不影响
        assert_eq!(BoxSelectMode::from_drag(s, c), BoxSelectMode::Window);
    }

    #[test]
    fn from_drag_right_to_left_is_crossing() {
        let s = CanvasPoint::new(10.0, 0.0);
        let c = CanvasPoint::new(0.0, 5.0);
        assert_eq!(BoxSelectMode::from_drag(s, c), BoxSelectMode::Crossing);
    }

    #[test]
    fn from_drag_pure_vertical_defaults_to_window() {
        let s = CanvasPoint::new(5.0, 0.0);
        let c = CanvasPoint::new(5.0, 100.0);
        assert_eq!(BoxSelectMode::from_drag(s, c), BoxSelectMode::Window);
    }

    #[test]
    fn window_requires_full_containment() {
        let sel = r(0.0, 0.0, 100.0, 100.0);
        let inside = r(10.0, 10.0, 20.0, 20.0);
        let straddling_right = r(90.0, 10.0, 20.0, 20.0); // 越右边界
        assert!(BoxSelectMode::Window.hits(&sel, &inside));
        assert!(!BoxSelectMode::Window.hits(&sel, &straddling_right));
    }

    #[test]
    fn crossing_accepts_overlap_only() {
        let sel = r(0.0, 0.0, 100.0, 100.0);
        let inside = r(10.0, 10.0, 20.0, 20.0);
        let straddling = r(90.0, 10.0, 20.0, 20.0);
        let outside = r(200.0, 200.0, 10.0, 10.0);
        assert!(BoxSelectMode::Crossing.hits(&sel, &inside));
        assert!(BoxSelectMode::Crossing.hits(&sel, &straddling));
        assert!(!BoxSelectMode::Crossing.hits(&sel, &outside));
    }
}
