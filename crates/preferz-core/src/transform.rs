use crate::spaces::CanvasVector;
use serde::{Deserialize, Serialize};

/// 一个 item 在画布空间下的仿射变换。
///
/// `pos` 与 `scale` 使用 [`CanvasVector`]（即 `Vector2D<f32, CanvasSpace>`），
/// 与屏幕空间的换算必须经 [`crate::spaces::CanvasToScreen`] 或 viewport 转换函数，
/// 严禁裸 `f32` 换算（AGENTS.md "Use euclid types — never cast between them with raw f32"）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub pos: CanvasVector,
    pub scale: CanvasVector,
    pub rotation: f32,
    pub flip_h: bool,
    pub flip_v: bool,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            pos: CanvasVector::zero(),
            scale: CanvasVector::new(1.0, 1.0),
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
        }
    }
}

impl Transform {
    pub fn new(pos_x: f32, pos_y: f32, scale_x: f32, scale_y: f32) -> Self {
        Self {
            pos: CanvasVector::new(pos_x, pos_y),
            scale: CanvasVector::new(scale_x, scale_y),
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
        }
    }

    pub fn scale_by(&mut self, factor: f32) {
        self.scale.x *= factor;
        self.scale.y *= factor;
    }

    pub fn rotate_by(&mut self, angle: f32) {
        self.rotation += angle;
    }

    pub fn flip_horizontal(&mut self) {
        self.flip_h = !self.flip_h;
    }

    pub fn flip_vertical(&mut self) {
        self.flip_v = !self.flip_v;
    }
}
