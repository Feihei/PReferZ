//! 坐标空间标签类型，用于 euclid 泛型参数，避免裸 f32 跨空间换算。
//!
//! 约定：
//! - `CanvasSpace`：画布世界坐标（场景内 item 的 transform.pos 即此空间）
//! - `ScreenSpace`：屏幕像素坐标（egui::Pos2 / egui::Vec2 即此空间）
//!
//! 严禁裸 f32 在两个空间之间直接换算；必须走 [`crate::viewport`] 的转换函数
//! 或 `euclid::Transform2D<f32, CanvasSpace, ScreenSpace>`。

/// 画布世界坐标空间。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CanvasSpace;

/// 屏幕像素坐标空间。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScreenSpace;

/// 画布空间下的 2D 点。
pub type CanvasPoint = euclid::Point2D<f32, CanvasSpace>;
/// 画布空间下的 2D 向量。
pub type CanvasVector = euclid::Vector2D<f32, CanvasSpace>;
/// 画布空间下的矩形。
pub type CanvasRect = euclid::Rect<f32, CanvasSpace>;
/// 画布空间下的尺寸。
pub type CanvasSize = euclid::Size2D<f32, CanvasSpace>;

/// 屏幕空间下的 2D 点。
pub type ScreenPoint = euclid::Point2D<f32, ScreenSpace>;
/// 屏幕空间下的 2D 向量。
pub type ScreenVector = euclid::Vector2D<f32, ScreenSpace>;
/// 屏幕空间下的矩形。
pub type ScreenRect = euclid::Rect<f32, ScreenSpace>;

/// 画布 → 屏幕 变换矩阵。
pub type CanvasToScreen = euclid::Transform2D<f32, CanvasSpace, ScreenSpace>;
/// 屏幕 → 画布 变换矩阵。
pub type ScreenToCanvas = euclid::Transform2D<f32, ScreenSpace, CanvasSpace>;
