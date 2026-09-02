//! Excalidraw 式预设调色板控件（替代连续取色器）。
//!
//! 颜色数据与暗色变换移植自 `.ref/excalidraw` 的
//! `packages/common/src/colors.ts`：
//! - 色板取 open-color 五档（权重 50/200/400/600/800）；
//! - 暗色主题套用 Excalidraw 同款滤镜 `invert(93%) hue-rotate(180deg)`
//!   （纯数学变换，见 [`apply_dark_mode_filter`]），保证暗色下观感一致。

use eframe::egui;
use preferz_core::shape::FillStyle;

use crate::i18n::{t, Lang, T};

/// 单色系五档（明 → 暗），对应 Excalidraw `COLOR_PALETTE` 的 shade 元组。
pub type ColorTuple = [&'static str; 5];

/// 色板行：每行一个色系五档。顺序即展示顺序（Excalidraw common shades 同序）。
pub const PALETTE_ROWS: [(&str, ColorTuple); 11] = [
    (
        "gray",
        ["#f8f9fa", "#e9ecef", "#ced4da", "#868e96", "#343a40"],
    ),
    (
        "red",
        ["#fff5f5", "#ffc9c9", "#ff8787", "#fa5252", "#e03131"],
    ),
    (
        "pink",
        ["#fff0f6", "#fcc2d7", "#f783ac", "#e64980", "#c2255c"],
    ),
    (
        "grape",
        ["#f8f0fc", "#eebefa", "#da77f2", "#be4bdb", "#9c36b5"],
    ),
    (
        "violet",
        ["#f3f0ff", "#d0bfff", "#9775fa", "#7950f2", "#6741d9"],
    ),
    (
        "blue",
        ["#e7f5ff", "#a5d8ff", "#4dabf7", "#228be6", "#1971c2"],
    ),
    (
        "cyan",
        ["#e3fafc", "#99e9f2", "#3bc9db", "#15aabf", "#0c8599"],
    ),
    (
        "teal",
        ["#e6fcf5", "#96f2d7", "#38d9a9", "#12b886", "#099268"],
    ),
    (
        "green",
        ["#ebfbee", "#b2f2bb", "#69db7c", "#40c057", "#2f9e44"],
    ),
    (
        "yellow",
        ["#fff9db", "#ffec99", "#ffd43b", "#fab005", "#f08c00"],
    ),
    (
        "orange",
        ["#fff4e6", "#ffd8a8", "#ffa94d", "#fd7e14", "#e8590c"],
    ),
];

/// 描边 top picks（Excalidraw `DEFAULT_ELEMENT_STROKE_PICKS`）：
/// 黑 + 各色系第 5 档（600/800 权重档）。
const STROKE_PICKS: [&str; 5] = ["#1e1e1e", "#e03131", "#2f9e44", "#1971c2", "#f08c00"];

/// 填充 top picks（Excalidraw `DEFAULT_ELEMENT_BACKGROUND_PICKS`）：
/// transparent + 各色系第 2 档（200 权重档，柔和底色）。
const FILL_PICKS: [&str; 4] = ["#ffc9c9", "#b2f2bb", "#a5d8ff", "#ffec99"];

/// `#rrggbb` → `[u8; 3]`。非 hex 输入返回黑色（数据为编译期常量，不会发生）。
fn hex_to_rgb(hex: &str) -> [u8; 3] {
    let h = hex.trim_start_matches('#');
    [
        u8::from_str_radix(&h[0..2], 16).unwrap_or(0),
        u8::from_str_radix(&h[2..4], 16).unwrap_or(0),
        u8::from_str_radix(&h[4..6], 16).unwrap_or(0),
    ]
}

/// Excalidraw 暗色模式颜色滤镜：`invert(93%) hue-rotate(180deg)`。
///
/// 从 `.ref/excalidraw` colors.ts 的 `cssInvert` + `cssHueRotate` 逐式移植，
/// 输入输出均为不透明 RGB。
pub fn apply_dark_mode_filter(rgb: [u8; 3]) -> [u8; 3] {
    const INVERT_PERCENT: f32 = 93.0;
    const HUE_ROTATE_DEG: f32 = 180.0;

    let p = INVERT_PERCENT / 100.0;
    let invert = |c: u8| -> f32 {
        let c = c as f32;
        (c * (1.0 - p) + (255.0 - c) * p).clamp(0.0, 255.0)
    };
    let r = invert(rgb[0]) / 255.0;
    let g = invert(rgb[1]) / 255.0;
    let b = invert(rgb[2]) / 255.0;

    let a = HUE_ROTATE_DEG.to_radians();
    let (s, c) = a.sin_cos();
    // hue-rotate 变换矩阵（CSS filter 规范同款）
    let m = [
        0.213 + c * 0.787 - s * 0.213,
        0.715 - c * 0.715 - s * 0.715,
        0.072 - c * 0.072 + s * 0.928,
        0.213 - c * 0.213 + s * 0.143,
        0.715 + c * 0.285 + s * 0.14,
        0.072 - c * 0.072 - s * 0.283,
        0.213 - c * 0.213 - s * 0.787,
        0.715 - c * 0.715 + s * 0.715,
        0.072 + c * 0.928 + s * 0.072,
    ];
    let apply = |r: f32, g: f32, b: f32| -> u8 {
        ((r * m[0] + g * m[1] + b * m[2]).clamp(0.0, 1.0) * 255.0).round() as u8
    };
    [apply(r, g, b), apply(r, m[4], b), apply(r, g, m[8])]
}

/// 主题化 hex → Color32：暗色主题套 Excalidraw 滤镜。
fn themed_color(hex: &str, dark: bool) -> egui::Color32 {
    let [r, g, b] = if dark {
        apply_dark_mode_filter(hex_to_rgb(hex))
    } else {
        hex_to_rgb(hex)
    };
    egui::Color32::from_rgb(r, g, b)
}

/// 单个色板格子（小方块，选中描白框）。
fn swatch(ui: &mut egui::Ui, color: egui::Color32, selected: bool, size: f32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    if resp.clicked() {
        return true;
    }
    let painter = ui.painter();
    painter.rect_filled(rect.expand(1.0), 3.0, color);
    if selected {
        painter.rect_stroke(
            rect.expand(1.0),
            3.0,
            egui::Stroke::new(2.0_f32, egui::Color32::WHITE),
        );
        painter.rect_stroke(
            rect.expand(1.0),
            3.0,
            egui::Stroke::new(1.0_f32, egui::Color32::BLACK),
        );
    } else {
        painter.rect_stroke(
            rect.expand(1.0),
            3.0,
            egui::Stroke::new(0.5_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );
    }
    false
}

/// 调色板弹层：top picks 行 + 全色板网格 + 自定义取色。
fn palette_popup(
    ui: &mut egui::Ui,
    current: egui::Color32,
    dark: bool,
    picks: &[&'static str],
) -> Option<egui::Color32> {
    let mut picked = None;
    let sw = 20.0;

    // top picks
    ui.horizontal(|ui| {
        for hex in picks {
            let c = themed_color(hex, dark);
            if swatch(ui, c, approx_eq(c, current), sw) {
                picked = Some(c);
            }
        }
    });

    ui.separator();

    // 全色板：每行一个色系五档
    for (_name, row) in PALETTE_ROWS {
        ui.horizontal(|ui| {
            for hex in row {
                let c = themed_color(hex, dark);
                if swatch(ui, c, approx_eq(c, current), sw) {
                    picked = Some(c);
                }
            }
        });
    }

    ui.separator();

    // 自定义色：保留系统取色器入口（Excalidraw 的 hex 输入框的等价物）
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("…").weak());
        let mut custom = current;
        if ui.color_edit_button_srgba(&mut custom).changed() {
            picked = Some(custom);
        }
    });

    picked
}

fn approx_eq(a: egui::Color32, b: egui::Color32) -> bool {
    a.r() == b.r() && a.g() == b.g() && a.b() == b.b() && a.a() == b.a()
}

/// 调色板按钮（共用实现）：按钮本体显示当前颜色，点击弹出色板。
/// `picks` 决定弹层顶部的 top picks 行。返回 `true` 表示用户选了新颜色。
fn palette_button_with_picks(
    ui: &mut egui::Ui,
    current: &mut [u8; 4],
    dark: bool,
    picks: &[&'static str],
) -> bool {
    let shown =
        egui::Color32::from_rgba_unmultiplied(current[0], current[1], current[2], current[3]);
    let btn = egui::Button::new(egui::RichText::new("⬤").size(11.0))
        .min_size(egui::vec2(26.0, 20.0))
        .fill(shown)
        .stroke(egui::Stroke::new(
            1.0_f32,
            ui.visuals().widgets.inactive.bg_stroke.color,
        ));
    let resp = ui.add(btn);
    let popup_id = resp.id.with("palette_popup");
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }
    let mut changed = false;
    egui::popup_below_widget(
        ui,
        popup_id,
        &resp,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            if let Some(c) = palette_popup(ui, shown, dark, picks) {
                *current = [c.r(), c.g(), c.b(), c.a()];
                changed = true;
            }
        },
    );
    changed
}

/// 描边色调色板按钮：top picks 为 `STROKE_PICKS`（Excalidraw 描边默认行）。
pub fn color_palette_button(ui: &mut egui::Ui, current: &mut [u8; 4], dark: bool) -> bool {
    palette_button_with_picks(ui, current, dark, &STROKE_PICKS)
}

/// 填充色调色板按钮：top picks 为 `FILL_PICKS`（Excalidraw 背景默认行）。
pub fn fill_color_palette_button(ui: &mut egui::Ui, current: &mut [u8; 4], dark: bool) -> bool {
    palette_button_with_picks(ui, current, dark, &FILL_PICKS)
}

/// 填充样式四态选择器（无 / 纯色 / 斜线 / 交叉线），Excalidraw 同款图标。
///
/// 返回 `Some(new_style)` 表示用户点了新的样式：`new_style` 为 `None` 即
/// 选中"无填充"。`current` 为 `None` 时代表当前无填充（四态中的"无"）。
pub fn fill_style_picker(
    ui: &mut egui::Ui,
    lang: Lang,
    current: Option<FillStyle>,
) -> Option<Option<FillStyle>> {
    let options = [
        (None, T::StyleFillNone),
        (Some(FillStyle::Solid), T::StyleFillSolid),
        (Some(FillStyle::Hachure), T::StyleFillHachure),
        (Some(FillStyle::CrossHatch), T::StyleFillCrossHatch),
    ];
    let mut result: Option<Option<FillStyle>> = None;
    for (style, tip) in options {
        let selected = current == style;
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
        let fg = if selected {
            ui.visuals().selection.stroke.color
        } else {
            ui.visuals().text_color()
        };
        if resp.clicked() && current != style {
            result = Some(style);
        }
        resp.on_hover_text(t(lang, tip));
        let painter = ui.painter();
        if selected {
            painter.rect_filled(
                rect,
                3.0,
                ui.visuals().selection.bg_fill.gamma_multiply(0.4),
            );
        }
        draw_fill_icon(painter, rect, style, fg);
    }
    result
}

/// 在 `rect` 内绘制填充样式图标。
fn draw_fill_icon(
    painter: &egui::Painter,
    rect: egui::Rect,
    style: Option<FillStyle>,
    fg: egui::Color32,
) {
    let stroke = egui::Stroke::new(1.2_f32, fg);
    let inner = rect.shrink(3.0);
    // 外框
    painter.rect_stroke(inner, 1.5, stroke);
    match style {
        None => {}
        Some(FillStyle::Solid) => {
            painter.rect_filled(inner.shrink(1.5), 1.0, fg);
        }
        Some(FillStyle::Hachure) => {
            // 两条对角线
            let a = egui::pos2(inner.left() + 2.0, inner.bottom() - 2.0);
            let b = egui::pos2(inner.right() - 2.0, inner.top() + 2.0);
            painter.line_segment([a, b], stroke);
        }
        Some(FillStyle::CrossHatch) => {
            let a = egui::pos2(inner.left() + 2.0, inner.bottom() - 2.0);
            let b = egui::pos2(inner.right() - 2.0, inner.top() + 2.0);
            painter.line_segment([a, b], stroke);
            let c = egui::pos2(inner.left() + 2.0, inner.top() + 2.0);
            let d = egui::pos2(inner.right() - 2.0, inner.bottom() - 2.0);
            painter.line_segment([c, d], stroke);
        }
    }
}
