//! 明暗主题（Phase G）。
//!
//! - [`ThemeMode`]：用户可选项 `Light` / `Dark` / `Auto`（`Auto` 跟随系统，
//!   第一版仅作为可选值持久化，运行时解析为具体模式）。
//! - 画布底色与新建元素默认前景色随主题翻转（D1/D2 拍板）。
//! - egui chrome（Visuals）随主题切换；`bg_alpha` 透明度仍施加在 panel/window/faint
//!   上，保证无边框 + 置顶的悬浮看图效果在明暗两套主题下都不受影响。

use egui::{Color32, Context, Visuals};
use serde::{Deserialize, Serialize};

/// 主题模式。
///
/// `Auto` 在运行时解析为具体的 `Light`/`Dark`（见 [`ThemeMode::resolve`]）。
/// 字段预留 `Auto` 以便日后在设置里加「跟随系统」勾选而不必改数据模型（D1 拍板：
/// 第一版只做手动切换，但字段先预留）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// 暗色（默认，与历史行为一致）。
    #[default]
    Dark,
    /// 亮色。
    Light,
    /// 跟随系统（运行时解析为 Light/Dark）。
    Auto,
}

impl ThemeMode {
    /// 把 `Auto` 解析成具体模式；系统主题不可用（部分平台返回 `None`）时回退 `Dark`。
    pub fn resolve(self, ctx: &Context) -> ThemeMode {
        match self {
            ThemeMode::Auto => match ctx.system_theme() {
                Some(egui::Theme::Dark) => ThemeMode::Dark,
                Some(egui::Theme::Light) => ThemeMode::Light,
                None => ThemeMode::Dark,
            },
            other => other,
        }
    }

    /// 当前是否为暗色（已解析 `Auto`）。
    pub fn is_dark(self, ctx: &Context) -> bool {
        matches!(self.resolve(ctx), ThemeMode::Dark)
    }

    /// 画布背景底色（RGB）；`bg_alpha` 不透明度由调用方叠加。
    /// `Auto` 走系统主题，做到实时跟随。
    pub fn canvas_bg(self, ctx: &Context) -> [u8; 3] {
        if self.is_dark(ctx) {
            [45, 45, 48]
        } else {
            [250, 250, 250]
        }
    }

    /// 新建元素默认描边色（RGBA），随主题翻转（D2 拍板「跟主题走」）。
    /// 仅在切换主题时应用到 `default_stroke`，用户仍可手动改色。
    pub fn default_stroke_color(self, ctx: &Context) -> [u8; 4] {
        if self.is_dark(ctx) {
            [240, 240, 240, 255]
        } else {
            [17, 17, 17, 255]
        }
    }

    /// 无 `ctx` 时的默认描边色（`Auto` 按 `Dark` 处理）；用于 `PReferZApp::new()`
    /// 启动初始化——此时还拿不到系统主题，仅作合理兜底。
    pub fn default_stroke_color_static(self) -> [u8; 4] {
        match self {
            ThemeMode::Light => [17, 17, 17, 255],
            ThemeMode::Dark | ThemeMode::Auto => [240, 240, 240, 255],
        }
    }

    /// 主文字色（标题 / 强调），随主题反色保证对比度。
    pub fn text_primary(self, ctx: &Context) -> Color32 {
        if self.is_dark(ctx) {
            Color32::from_rgb(228, 228, 236)
        } else {
            Color32::from_rgb(28, 28, 36)
        }
    }

    /// 次级文字（副标题 / 区块标题）。
    pub fn text_secondary(self, ctx: &Context) -> Color32 {
        if self.is_dark(ctx) {
            Color32::from_rgb(150, 150, 160)
        } else {
            Color32::from_rgb(96, 96, 108)
        }
    }

    /// 三级文字（提示 / 弱信息）。
    pub fn text_tertiary(self, ctx: &Context) -> Color32 {
        if self.is_dark(ctx) {
            Color32::from_rgb(140, 140, 150)
        } else {
            Color32::from_rgb(120, 120, 130)
        }
    }

    /// 设置面板显示名。
    pub fn display_name(self) -> &'static str {
        match self {
            ThemeMode::Dark => "Dark",
            ThemeMode::Light => "Light",
            ThemeMode::Auto => "Auto (system)",
        }
    }
}

/// 构造指定主题的 egui `Visuals`，并把 `bg_alpha` 重新施加到 chrome 填充色
/// （panel / window / faint），使透明窗口效果在明暗两套主题下都生效。
pub fn build_visuals(mode: ThemeMode, bg_alpha: f32, ctx: &Context) -> Visuals {
    let dark = mode.is_dark(ctx);
    let mut visuals = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let alpha = (bg_alpha * 255.0).round() as u8;
    let (panel, window, faint) = if dark {
        ([45, 45, 48], [30, 30, 32], [30, 30, 32])
    } else {
        ([235, 235, 240], [250, 250, 252], [250, 250, 252])
    };
    visuals.panel_fill = Color32::from_rgba_unmultiplied(panel[0], panel[1], panel[2], alpha);
    visuals.window_fill = Color32::from_rgba_unmultiplied(window[0], window[1], window[2], alpha);
    visuals.faint_bg_color = Color32::from_rgba_unmultiplied(faint[0], faint[1], faint[2], alpha);
    visuals
}
