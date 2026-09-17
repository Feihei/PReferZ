//! 视口的 egui↔core 桥接层。
//!
//! [`ViewportState`] 本体在 `preferz-core`（见 ADR-0008），只吃 euclid 的
//! `ScreenSpace`/`CanvasSpace` 标签类型。本模块做两件事：
//! 1. 重导出 `ViewportState`，保持 `crate::viewport::ViewportState` 既有引用路径不变；
//! 2. 提供 [`ViewportEgui`] 扩展 trait——本地 trait 实现给 foreign 类型合法（绕开
//!    orphan 规则），把 `egui::Pos2`/`egui::Rect`/`egui::Vec2` 一对一转成 core 的
//!    `ScreenPoint`/`ScreenRect`/`ScreenVector` 再走纯几何路径。
//!
//! 于是"egui 世界"到"core 世界"的坐标边界**收口在这一处**：调用点要么用 core 原生
//! 方法（`screen_to_canvas(ScreenPoint)`），要么用本 trait 的 egui 口味方法，二者语义
//! 一致，只是入/出坐标类型不同。

use eframe::egui;
use preferz_core::spaces::{CanvasPoint, CanvasRect, ScreenPoint, ScreenRect, ScreenVector};

pub use preferz_core::viewport::ViewportState;

/// [`ViewportState`] 的 egui 口味便捷层：把 egui 输入/输出类型转成 core `ScreenSpace`
/// 标签类型再委托给纯几何方法。仅在 app 边界使用；core 侧不出现 egui。
pub trait ViewportEgui {
    /// `egui::Pos2` → 画布点。
    fn pos2_to_canvas(&self, p: egui::Pos2) -> CanvasPoint;
    /// 画布点 → `egui::Pos2`（供绘制）。
    fn canvas_to_pos2(&self, c: CanvasPoint) -> egui::Pos2;
    /// 画布矩形 → `egui::Rect`（供绘制/命中）。
    fn canvas_rect_to_egui(&self, r: CanvasRect) -> egui::Rect;
    /// 用 `egui::Rect` 设置画布面板屏幕矩形（CentralPanel 每帧调用）。
    fn set_screen_rect_egui(&mut self, r: egui::Rect);
    /// 按 `egui::Vec2` 屏幕位移平移视口。
    fn pan_by_screen_egui(&mut self, d: egui::Vec2);
    /// 以 `egui::Pos2` 锚点缩放。
    fn zoom_at_egui(&mut self, delta: f32, p: egui::Pos2);
}

impl ViewportEgui for ViewportState {
    fn pos2_to_canvas(&self, p: egui::Pos2) -> CanvasPoint {
        self.screen_to_canvas(ScreenPoint::new(p.x, p.y))
    }

    fn canvas_to_pos2(&self, c: CanvasPoint) -> egui::Pos2 {
        let s = self.canvas_to_screen(c);
        egui::Pos2::new(s.x, s.y)
    }

    fn canvas_rect_to_egui(&self, r: CanvasRect) -> egui::Rect {
        let s = self.canvas_to_screen_rect(r);
        egui::Rect::from_min_max(
            egui::Pos2::new(s.min().x, s.min().y),
            egui::Pos2::new(s.max().x, s.max().y),
        )
    }

    fn set_screen_rect_egui(&mut self, r: egui::Rect) {
        self.set_screen_rect(ScreenRect::new(
            ScreenPoint::new(r.min.x, r.min.y),
            euclid::Size2D::new(r.width(), r.height()),
        ));
    }

    fn pan_by_screen_egui(&mut self, d: egui::Vec2) {
        self.pan_by_screen(ScreenVector::new(d.x, d.y));
    }

    fn zoom_at_egui(&mut self, delta: f32, p: egui::Pos2) {
        self.zoom_at(delta, ScreenPoint::new(p.x, p.y));
    }
}
