//! 滚动输入的语义判定：同一份 `smooth_scroll_delta` 在不同设备上该缩放还是平移。
//!
//! - 鼠标滚轮 → **缩放**。这是 PReferZ 一贯的手感，Windows / Linux 上也以滚轮为主，
//!   不能改。
//! - 触控板双指滑动 → **平移**。macOS 用户默认只有触控板，双指滚动缩放会让
//!   画布既跳又糊，Excalidraw / Figma 等白板工具一律是滚动=平移、捏合=缩放。
//!
//! 不能按平台一刀切：macOS 上插鼠标的用户滚轮也该缩放。egui 0.37 的
//! [`egui::MouseWheelSource`] 正好提供设备来源——只有 macOS 的 winit 后端会区分
//! `Wheel` / `Trackpad` / `Momentum`，其它平台一律 `Unknown`，于是自然落回
//! 「缩放」这一既有行为，非 macOS 平台零回归。

/// 本帧滚动输入应当做什么。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScrollIntent {
    /// 缩放（鼠标滚轮、无法判定来源的平台）。
    Zoom,
    /// 平移（触控板双指滑动及其惯性滚动）。
    Pan,
}

/// 判定本帧的滚动意图：只要帧内任一滚轮事件来自触控板（或惯性阶段）就按平移处理。
///
/// 混合来源（同一帧既有滚轮又有触控板）极罕见，取「平移」是安全的一侧——
/// 平移不会跳变缩放比例，用户再滑一下就能纠正。
pub fn scroll_intent(events: &[egui::Event]) -> ScrollIntent {
    let from_trackpad = events.iter().any(|event| {
        matches!(
            event,
            egui::Event::MouseWheel { source, .. }
                if matches!(
                    source,
                    egui::MouseWheelSource::Trackpad | egui::MouseWheelSource::Momentum
                )
        )
    });
    if from_trackpad {
        ScrollIntent::Pan
    } else {
        ScrollIntent::Zoom
    }
}

#[cfg(test)]
mod tests {
    use super::{scroll_intent, ScrollIntent};
    use egui::{Event, Modifiers, MouseWheelSource, TouchPhase, Vec2};

    fn wheel(source: MouseWheelSource) -> Event {
        Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, -20.0),
            phase: TouchPhase::Move,
            source,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn mouse_wheel_zooms() {
        assert_eq!(
            scroll_intent(&[wheel(MouseWheelSource::Wheel)]),
            ScrollIntent::Zoom
        );
        // 非 macOS 平台没有来源信息，必须落回缩放（既有行为）
        assert_eq!(
            scroll_intent(&[wheel(MouseWheelSource::Unknown)]),
            ScrollIntent::Zoom
        );
    }

    #[test]
    fn trackpad_and_momentum_pan() {
        assert_eq!(
            scroll_intent(&[wheel(MouseWheelSource::Trackpad)]),
            ScrollIntent::Pan
        );
        // 惯性滚动（手指已离开触控板）要接着平移，不能忽然变成缩放
        assert_eq!(
            scroll_intent(&[wheel(MouseWheelSource::Momentum)]),
            ScrollIntent::Pan
        );
    }

    #[test]
    fn no_events_zoom_is_default() {
        // 没有滚轮事件时本函数不会被调用，但返回值必须是既有行为，避免将来
        // 有人无条件调用时把滚轮缩放静默关掉。
        assert_eq!(scroll_intent(&[]), ScrollIntent::Zoom);
    }
}
