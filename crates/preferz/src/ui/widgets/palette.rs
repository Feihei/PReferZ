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

/// 填充默认不透明度（plan #2 拍板 2026-09-10）：50%。
/// 从色板格子选中填充色时应用（自定义取色保留用户 alpha；滑块可显式覆盖）。
pub const FILL_DEFAULT_ALPHA: u8 = 128;

/// 填充 top picks：与描边共用同组 5 色（plan #2 拍板，Excalidraw 描边/填充共用调色板）。
/// 视觉上靠 `FILL_DEFAULT_ALPHA` 半透明呈现，不再用浅色 200 档模拟。
const FILL_PICKS: [&str; 5] = STROKE_PICKS;

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
    // 矩阵行点乘（修复：此前误把 m[4]/m[8] 当颜色分量传入，导致 dark 滤镜
    // 输出错乱、色板与主题不符）。CSS 规范：newG 用第 2 行、newB 用第 3 行。
    let apply_row = |r: f32, g: f32, b: f32, row: usize| -> u8 {
        ((r * m[row] + g * m[row + 1] + b * m[row + 2]).clamp(0.0, 1.0) * 255.0).round() as u8
    };
    [
        apply_row(r, g, b, 0),
        apply_row(r, g, b, 3),
        apply_row(r, g, b, 6),
    ]
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
            egui::StrokeKind::Middle,
        );
        painter.rect_stroke(
            rect.expand(1.0),
            3.0,
            egui::Stroke::new(1.0_f32, egui::Color32::BLACK),
            egui::StrokeKind::Middle,
        );
    } else {
        painter.rect_stroke(
            rect.expand(1.0),
            3.0,
            egui::Stroke::new(0.5_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
            egui::StrokeKind::Middle,
        );
    }
    false
}

/// 色系代表色在 5 档中的下标（第 5 档 = 最饱和的 open-color 600/800 权重档）。
/// 「Colors」段每色系只展示这一档作为代表，明暗微调交给「Shades」段。
const FAMILY_REPRESENTATIVE: usize = 4;

/// 忽略 alpha 的 RGB 相等判断（色板格子是否"当前选中"用颜色本身，不比对 alpha）。
fn rgb_eq(a: egui::Color32, b: egui::Color32) -> bool {
    a.r() == b.r() && a.g() == b.g() && a.b() == b.b()
}

/// `Color32` → 小写 `#rrggbb`（hex 输入框初值）。
fn color_to_hex(c: egui::Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

/// 解析用户输入的 hex（接受 `rgb` / `rrggbb`，可选前导 `#`）。非法返回 `None`。
fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let h = s.trim().trim_start_matches('#');
    let expand = |digits: &[u8]| -> Option<[u8; 3]> {
        let nib = |b: u8| u8::from_str_radix((b as char).to_string().as_str(), 16).ok();
        Some([nib(digits[0])?, nib(digits[1])?, nib(digits[2])?])
    };
    match h.len() {
        3 => {
            let v = expand(h.as_bytes())?;
            // #abc → #aabbcc
            Some([v[0] << 4 | v[0], v[1] << 4 | v[1], v[2] << 4 | v[2]])
        }
        6 => {
            let pair = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
            Some([pair(0)?, pair(2)?, pair(4)?])
        }
        _ => None,
    }
}

/// RGB 平方距离（忽略 alpha），用于「最近色系」归属。
fn dist2(a: egui::Color32, b: egui::Color32) -> u32 {
    let d = |x: u8, y: u8| (x as i32 - y as i32).pow(2) as u32;
    d(a.r(), b.r()) + d(a.g(), b.g()) + d(a.b(), b.b())
}

/// 当前色属于哪个色系：先精确命中色板 5 档；不在色板内（近黑 `#1e1e1e`、自定义 hex 等）
/// 则取 RGB 最近的一档所属色系——使「Shades」段恒显示（对齐 Excalidraw 按最近色系给
/// 明暗档的行为）。`PALETTE_ROWS` 非空，故恒返回 `Some`。
fn family_of_current(current: egui::Color32, dark: bool) -> Option<usize> {
    // 精确命中优先。
    if let Some(i) = PALETTE_ROWS.iter().enumerate().find_map(|(i, (_n, row))| {
        row.iter()
            .any(|hex| rgb_eq(themed_color(hex, dark), current))
            .then_some(i)
    }) {
        return Some(i);
    }
    // 否则取最近色系（该色系 5 档中距 current 最近者决定归属）。
    PALETTE_ROWS
        .iter()
        .enumerate()
        .map(|(i, (_n, row))| {
            let d = row
                .iter()
                .map(|hex| dist2(themed_color(hex, dark), current))
                .min()
                .unwrap_or(u32::MAX);
            (i, d)
        })
        .min_by_key(|&(_i, d)| d)
        .map(|(i, _d)| i)
}

/// 调色板弹层（Excalidraw 同款三段）：**Colors**（每色系代表色）+ **Shades**（当前色
/// 所属色系的五档明暗，恒显示，非色板色取最近色系）+ **Hex**（十六进制输入，等价旧取色器）。
/// 返回 `(颜色, 是否来自色板格子)`；色板格子返回 `true`（填充套默认 alpha），
/// hex 输入返回 `false`（保留 current 的 alpha）。
fn palette_popup(
    ui: &mut egui::Ui,
    current: egui::Color32,
    dark: bool,
    lang: Lang,
) -> Option<(egui::Color32, bool)> {
    let mut picked = None;
    let sw = 20.0;
    let section = |ui: &mut egui::Ui, text: &str| {
        ui.label(egui::RichText::new(text).weak().small());
    };

    // ── Colors：11 色系代表色（每色系第 5 档），每行 6 格自动换行 ──
    section(ui, t(lang, T::PaletteColors));
    let reps: Vec<egui::Color32> = PALETTE_ROWS
        .iter()
        .map(|(_n, row)| themed_color(row[FAMILY_REPRESENTATIVE], dark))
        .collect();
    for chunk in reps.chunks(6) {
        ui.horizontal(|ui| {
            for c in chunk {
                if swatch(ui, *c, rgb_eq(*c, current), sw) {
                    picked = Some((*c, true));
                }
            }
        });
    }
    ui.separator();

    // ── Shades：当前色系的五档明暗（仅当 current 命中某色系时）──
    if let Some(fi) = family_of_current(current, dark) {
        section(ui, t(lang, T::PaletteShades));
        ui.horizontal(|ui| {
            for hex in PALETTE_ROWS[fi].1 {
                let c = themed_color(hex, dark);
                if swatch(ui, c, rgb_eq(c, current), sw) {
                    picked = Some((c, true));
                }
            }
        });
        ui.separator();
    }

    // ── Hex：十六进制输入（Excalidraw 底部同款，取代旧 egui HSV 取色器）──
    section(ui, t(lang, T::PaletteHex));
    // 用 temp 数据按 popup id 缓存输入串：未聚焦时以 current 回填，聚焦（正在打字）
    // 时保留用户输入，避免每帧重置光标。popup 关闭后 temp 自然失效，重开时再回填。
    let id = ui.id().with("palette_hex");
    let focused_key = id.with("focus");
    let was_focused = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(focused_key))
        .unwrap_or(false);
    let mut buf = if was_focused {
        ui.ctx()
            .data(|d| d.get_temp::<String>(id))
            .unwrap_or_default()
    } else {
        color_to_hex(current)
    };
    let resp = ui.add(
        egui::TextEdit::singleline(&mut buf)
            .desired_width(90.0)
            .char_limit(7)
            .margin(egui::vec2(4.0, 2.0)),
    );
    let has_focus = resp.has_focus();
    ui.ctx().data_mut(|d| {
        d.insert_temp(id, buf.clone());
        d.insert_temp(focused_key, has_focus);
    });
    if resp.changed() || resp.lost_focus() {
        if let Some([r, g, b]) = parse_hex(&buf) {
            // 保留 current 的 alpha（填充半透明不被 hex 编辑破坏）
            picked = Some((
                egui::Color32::from_rgba_unmultiplied(r, g, b, current.a()),
                false,
            ));
        }
    }

    picked
}

/// 展开箭头按钮：手绘一个「›」chevron（不依赖字体字形，避免豆腐块），点击弹完整调色板。
fn expand_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
    let visuals = ui.visuals();
    let bg = if resp.hovered() {
        visuals.widgets.hovered.bg_fill
    } else if resp.clicked() {
        visuals.widgets.active.bg_fill
    } else {
        visuals.widgets.inactive.bg_fill
    };
    let stroke = visuals.widgets.inactive.bg_stroke;
    let fg = visuals.text_color();
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, bg);
    painter.rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Middle);
    // 「›」：两条线段构成的右尖括号
    let c = rect.center();
    let chevron = egui::Stroke::new(1.4_f32, fg);
    painter.line_segment(
        [egui::pos2(c.x - 2.0, c.y - 4.0), egui::pos2(c.x + 2.5, c.y)],
        chevron,
    );
    painter.line_segment(
        [egui::pos2(c.x + 2.5, c.y), egui::pos2(c.x - 2.0, c.y + 4.0)],
        chevron,
    );
    resp
}

/// 「跟随形状」格子的上下文：`on` = 当前是否跟随，`border` = 形状描边色（格子底色，
/// 也是开启跟随时颜色吸附的目标）。仅文字/填充控件传入；描边/墨迹为 `None`（不显示该格）。
#[derive(Debug, Clone, Copy)]
pub struct FollowCtx {
    pub on: bool,
    pub border: egui::Color32,
}

/// 调色板控件一次交互的结果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorPick {
    /// 无操作。
    None,
    /// 选了一个颜色（来自内联格子 / 弹层格子 / hex）。调用方应据此把"跟随"置为关闭。
    Color([u8; 4]),
    /// 点了「跟随形状」格子，参数为切换后的新状态（true=开启，颜色应吸附到边框；
    /// false=关闭，保持当前颜色）。
    Follow(bool),
}

/// 手绘「链环」图标（两个交叠圆环），叠在跟随格上以区别于普通色格。
fn draw_link_icon(painter: &egui::Painter, rect: egui::Rect, fg: egui::Color32) {
    let c = rect.center();
    let r = 3.0;
    let s = egui::Stroke::new(1.1_f32, fg);
    painter.circle_stroke(c + egui::vec2(-2.4, 2.4), r, s);
    painter.circle_stroke(c + egui::vec2(2.4, -2.4), r, s);
}

/// 「跟随形状」格子：底色=形状边框色，叠一个链环图标，跟随时描选中环。返回是否被点击。
fn follow_swatch(ui: &mut egui::Ui, border: egui::Color32, active: bool, lang: Lang) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
    let clicked = resp.clicked();
    let painter = ui.painter();
    painter.rect_filled(rect.expand(1.0), 3.0, border);
    // 图标前景按底色亮度取黑/白，保证任意边框色下都看得见
    let lum = (border.r() as u32 * 299 + border.g() as u32 * 587 + border.b() as u32 * 114) / 1000;
    let fg = if lum > 140 {
        egui::Color32::BLACK
    } else {
        egui::Color32::WHITE
    };
    draw_link_icon(painter, rect, fg);
    if active {
        painter.rect_stroke(
            rect.expand(1.0),
            3.0,
            egui::Stroke::new(2.0_f32, egui::Color32::WHITE),
            egui::StrokeKind::Middle,
        );
        painter.rect_stroke(
            rect.expand(1.0),
            3.0,
            egui::Stroke::new(1.0_f32, egui::Color32::BLACK),
            egui::StrokeKind::Middle,
        );
    } else {
        painter.rect_stroke(
            rect.expand(1.0),
            3.0,
            egui::Stroke::new(0.5_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
            egui::StrokeKind::Middle,
        );
    }
    resp.on_hover_text(t(lang, T::PaletteFollowShape));
    clicked
}

/// 调色板控件（共用实现）：**〔可选「跟随形状」格〕 + 内联 5 个 top picks + 展开箭头**。
/// 点内联格子直接选色（若带跟随格则自动脱离跟随）；点箭头弹 [`palette_popup`]
/// （Colors / Shades / Hex 三段）。`follow` 为 `Some` 时最前面渲染「跟随形状」格。
/// `swatch_alpha`：`Some(a)` 时，从色板格子选中的颜色强制写 alpha = `a`（hex 输入不受影响）。
fn palette_button_inner(
    ui: &mut egui::Ui,
    current: &[u8; 4],
    dark: bool,
    lang: Lang,
    picks: &[&'static str],
    swatch_alpha: Option<u8>,
    follow: Option<FollowCtx>,
) -> ColorPick {
    let shown =
        egui::Color32::from_rgba_unmultiplied(current[0], current[1], current[2], current[3]);
    let mut out = ColorPick::None;
    ui.horizontal(|ui| {
        // 跟随形状格（最前）：点它切换跟随。
        let mut click_follow: Option<bool> = None;
        if let Some(fc) = follow {
            if follow_swatch(ui, fc.border, fc.on, lang) {
                click_follow = Some(!fc.on);
            }
        }
        // 内联 top picks：跟随时不高亮（当前色=边框，避免与某个 pick 混淆）。
        let pick_sel_active = follow.map(|f| !f.on).unwrap_or(true);
        let mut click_color: Option<[u8; 4]> = None;
        for hex in picks {
            let c = themed_color(hex, dark);
            if swatch(ui, c, pick_sel_active && rgb_eq(c, shown), 22.0) {
                let mut rgba = [c.r(), c.g(), c.b(), c.a()];
                if let Some(a) = swatch_alpha {
                    rgba[3] = a;
                }
                click_color = Some(rgba);
            }
        }
        // 展开箭头：弹完整调色板。
        let mut popup_color: Option<[u8; 4]> = None;
        let resp = expand_button(ui).on_hover_text(t(lang, T::PaletteMore));
        egui::Popup::from_toggle_button_response(&resp)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                if let Some((c, from_swatch)) = palette_popup(ui, shown, dark, lang) {
                    let mut rgba = [c.r(), c.g(), c.b(), c.a()];
                    if from_swatch {
                        if let Some(a) = swatch_alpha {
                            rgba[3] = a;
                        }
                    }
                    popup_color = Some(rgba);
                }
            });
        // 合并（同一帧只会有一个动作）：选色优先于切跟随。
        if let Some(c) = click_color.or(popup_color) {
            out = ColorPick::Color(c);
        } else if let Some(on) = click_follow {
            out = ColorPick::Follow(on);
        }
    });
    out
}

/// 描边 / 墨迹调色板按钮（无跟随格）：内联 picks 为 `STROKE_PICKS`。
/// 返回 `true` 表示选了新颜色（就地写回 `current`）。
pub fn color_palette_button(
    ui: &mut egui::Ui,
    current: &mut [u8; 4],
    dark: bool,
    lang: Lang,
) -> bool {
    match palette_button_inner(ui, current, dark, lang, &STROKE_PICKS, None, None) {
        ColorPick::Color(c) => {
            *current = c;
            true
        }
        _ => false,
    }
}

/// 填充样式未启用时的填充色按钮（默认样式面板用，无跟随格）。
pub fn fill_color_palette_button(
    ui: &mut egui::Ui,
    current: &mut [u8; 4],
    dark: bool,
    lang: Lang,
) -> bool {
    match palette_button_inner(
        ui,
        current,
        dark,
        lang,
        &FILL_PICKS,
        Some(FILL_DEFAULT_ALPHA),
        None,
    ) {
        ColorPick::Color(c) => {
            *current = c;
            true
        }
        _ => false,
    }
}

/// 文字色调色板按钮（带「跟随形状」格）。返回事件，调用方负责写回颜色与跟随态。
pub fn color_palette_button_follow(
    ui: &mut egui::Ui,
    current: &[u8; 4],
    dark: bool,
    lang: Lang,
    follow: FollowCtx,
) -> ColorPick {
    palette_button_inner(ui, current, dark, lang, &STROKE_PICKS, None, Some(follow))
}

/// 填充色调色板按钮（带「跟随形状」格）。色板格子选中套默认 50% alpha。
pub fn fill_color_palette_button_follow(
    ui: &mut egui::Ui,
    current: &[u8; 4],
    dark: bool,
    lang: Lang,
    follow: FollowCtx,
) -> ColorPick {
    palette_button_inner(
        ui,
        current,
        dark,
        lang,
        &FILL_PICKS,
        Some(FILL_DEFAULT_ALPHA),
        Some(follow),
    )
}

/// 填充样式六态选择器（无 / 纯色 / 斜线 / 交叉线 / 之字线 / 圆点；前三态为
/// Excalidraw 同款，后两态自移植 rough.js，plan #20）。
///
/// 返回 `Some(new_style)` 表示用户点了新的样式：`new_style` 为 `None` 即
/// 选中"无填充"。`current` 为 `None` 时代表当前无填充（"无"态）。
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
        (Some(FillStyle::Zigzag), T::StyleFillZigzag),
        (Some(FillStyle::Dots), T::StyleFillDots),
    ];
    ui.horizontal(|ui| {
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
    })
    .inner
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
    painter.rect_stroke(inner, 1.5, stroke, egui::StrokeKind::Middle);
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
        Some(FillStyle::Zigzag) => {
            // 三折人字线（↯ 观感）
            let x0 = inner.left() + 2.0;
            let x1 = inner.right() - 2.0;
            let y_top = inner.top() + 2.5;
            let y_bot = inner.bottom() - 2.5;
            let xm = (x0 + x1) * 0.5;
            let ym = (y_top + y_bot) * 0.5;
            painter.line_segment([egui::pos2(x0, y_bot), egui::pos2(xm, y_top)], stroke);
            painter.line_segment([egui::pos2(xm, y_top), egui::pos2(xm + 1.0, ym)], stroke);
            painter.line_segment([egui::pos2(xm + 1.0, ym), egui::pos2(x1, y_bot)], stroke);
        }
        Some(FillStyle::Dots) => {
            // 2×2 圆点
            let r = 1.1;
            for (dx, dy) in [(4.5, 4.5), (12.0, 4.5), (4.5, 12.0), (12.0, 12.0)] {
                painter.circle_filled(egui::pos2(inner.left() + dx, inner.top() + dy), r, fg);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 黑 → 亮灰（Excalidraw dark 下黑色元素显示为近白，非滤镜错乱后的中间色）。
    #[test]
    fn dark_filter_lightens_black() {
        let [r, g, b] = apply_dark_mode_filter([0x1e, 0x1e, 0x1e]);
        assert!(r > 200 && g > 200 && b > 200, "got ({r},{g},{b})");
    }

    /// 白 → 暗灰（滤镜互逆方向）。
    #[test]
    fn dark_filter_darkens_white() {
        let [r, g, b] = apply_dark_mode_filter([255, 255, 255]);
        assert!(r < 60 && g < 60 && b < 60, "got ({r},{g},{b})");
    }

    /// 灰度色 hue-rotate 后仍是灰度（R=G=B）——此前矩阵行错位时该性质被破坏。
    #[test]
    fn dark_filter_preserves_grayscale() {
        let [r, g, b] = apply_dark_mode_filter([100, 100, 100]);
        assert_eq!(r, g);
        assert_eq!(g, b);
    }

    /// 纯红变亮红（用户反馈：dark picks 应为"白红绿蓝橙，比 light 亮一点"）。
    #[test]
    fn dark_filter_keeps_hue_and_lightens_red() {
        let [r, g, b] = apply_dark_mode_filter([0xe0, 0x31, 0x31]);
        assert!(r > g && r > b, "hue lost: got ({r},{g},{b})");
        assert!(r > 0xe0, "not lightened: got ({r},{g},{b})");
    }
}
