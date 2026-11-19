//! 主题色空间：基色存储 + 暗色绘制滤镜 + 旧存档颜色迁移。
//!
//! 元素颜色统一存**基色空间**（浅色主题下的颜色，Excalidraw 的存储语义）；
//! 暗色主题在**绘制/导出时**才套 [`apply_dark_mode_filter`]
//! （Excalidraw 同款 `invert(93%) hue-rotate(180deg)`），图片不套。
//!
//! 旧版 `.prz`（v0.6 及以前）存的是"创作时主题下的显示色"：深色主题下选的
//! 色板颜色已经过滤镜，默认描边色是 `[240,240,240]`。加载时 [`migrate_legacy_scene`]
//! 按"颜色离基色色板近还是离暗色滤镜色板近"判定创作色空间，深色空间整档转回基色。
//! 判定按整档多数投票（色板/默认墨色精确命中优先），混合主题的文档按多数处理。

use crate::item::{Item, ItemKind};
use crate::scene::Scene;

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

/// 描边 top picks（Excalidraw `DEFAULT_ELEMENT_STROKE_PICKS`）；填充共用。
pub const PICKS: [&str; 5] = ["#1e1e1e", "#e03131", "#2f9e44", "#1971c2", "#f08c00"];

/// 新建元素默认墨色（基色空间，浅色下近黑、暗色下经滤镜呈近白）。
pub const DEFAULT_INK_RGBA: [u8; 4] = [17, 17, 17, 255];

/// 旧版深色主题的默认墨色（D2 时期硬编码，非滤镜输出），迁移时映射回基色墨。
const LEGACY_DARK_INK_RGB: [u8; 3] = [240, 240, 240];

/// `#rrggbb` → `[u8; 3]`。非法输入返回黑色（色板数据为编译期常量，不会发生）。
pub fn color_from_hex(hex: &str) -> [u8; 3] {
    let h = hex.trim_start_matches('#');
    let pair = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0);
    [pair(0), pair(2), pair(4)]
}

/// Excalidraw 暗色模式颜色滤镜：`invert(93%) hue-rotate(180deg)`。
///
/// 逐式移植自 `.ref/excalidraw` colors.ts 的 `cssInvert` + `cssHueRotate`，
/// 输入输出均为不透明 RGB。
pub fn apply_dark_mode_filter(rgb: [u8; 3]) -> [u8; 3] {
    let t = invert_input(rgb);
    apply_hue_matrix(t)
}

/// [`apply_dark_mode_filter`] 的逆变换（旧档迁移用）。
///
/// 滤镜是仿射变换 + 逐通道 clamp；对色板颜色二者可近似往返（clamp 丢信息
/// 的极端色除外，测试钉住容差）。
pub fn inverse_dark_mode_filter(rgb: [u8; 3]) -> [u8; 3] {
    let y = [
        rgb[0] as f32 / 255.0,
        rgb[1] as f32 / 255.0,
        rgb[2] as f32 / 255.0,
    ];
    let m = hue_matrix();
    let inv = invert3(m);
    let mut t = [0.0f32; 3];
    for (row, t_row) in inv.iter().zip(t.iter_mut()) {
        *t_row = (row[0] * y[0] + row[1] * y[1] + row[2] * y[2]).clamp(0.0, 1.0);
    }
    let out = t.map(|v| ((0.93 - v) / 0.86).clamp(0.0, 1.0) * 255.0);
    [
        out[0].round() as u8,
        out[1].round() as u8,
        out[2].round() as u8,
    ]
}

/// 主题化颜色：暗色主题套滤镜，alpha 原样保留。
pub fn themed_rgba(c: [u8; 4], dark: bool) -> [u8; 4] {
    if !dark {
        return c;
    }
    let [r, g, b] = apply_dark_mode_filter([c[0], c[1], c[2]]);
    [r, g, b, c[3]]
}

/// 滤镜第一步：`invert(93%)`，输出 0..1。
fn invert_input(rgb: [u8; 3]) -> [f32; 3] {
    rgb.map(|c| 0.93 - 0.86 * (c as f32 / 255.0))
}

/// 滤镜第二步：`hue-rotate(180deg)` 矩阵（CSS filter 规范同款），输出 RGB。
fn apply_hue_matrix(t: [f32; 3]) -> [u8; 3] {
    let m = hue_matrix();
    let mut out = [0u8; 3];
    for (row, o) in m.iter().zip(out.iter_mut()) {
        let v = (row[0] * t[0] + row[1] * t[1] + row[2] * t[2]).clamp(0.0, 1.0);
        *o = (v * 255.0).round() as u8;
    }
    out
}

/// `hue-rotate(180deg)` 的 3×3 矩阵（行主序）。
fn hue_matrix() -> [[f32; 3]; 3] {
    let a = 180.0f32.to_radians();
    let (s, c) = a.sin_cos();
    [
        [
            0.213 + c * 0.787 - s * 0.213,
            0.715 - c * 0.715 - s * 0.715,
            0.072 - c * 0.072 + s * 0.928,
        ],
        [
            0.213 - c * 0.213 + s * 0.143,
            0.715 + c * 0.285 + s * 0.14,
            0.072 - c * 0.072 - s * 0.283,
        ],
        [
            0.213 - c * 0.213 - s * 0.787,
            0.715 - c * 0.715 + s * 0.715,
            0.072 + c * 0.928 + s * 0.072,
        ],
    ]
}

/// 3×3 矩阵求逆（伴随矩阵；hue 矩阵可逆）。
fn invert3(m: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let inv_det = 1.0 / det;
    [
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inv_det,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv_det,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv_det,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inv_det,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv_det,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv_det,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inv_det,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv_det,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv_det,
        ],
    ]
}

fn palette_base_colors() -> Vec<[u8; 3]> {
    let mut out = Vec::with_capacity(11 * 5 + PICKS.len() + 1);
    for (_name, row) in PALETTE_ROWS {
        for hex in row {
            out.push(color_from_hex(hex));
        }
    }
    for hex in PICKS {
        out.push(color_from_hex(hex));
    }
    out.push(DEFAULT_INK_RGBA[..3].try_into().expect("RGB"));
    out
}

fn rgb_eq(a: [u8; 3], b: [u8; 3]) -> bool {
    a == b
}

fn dist2(a: [u8; 3], b: [u8; 3]) -> u32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (*x as i32 - *y as i32).pow(2) as u32)
        .sum()
}

/// 遍历 item 的每个颜色槽位（不可变）。
fn item_colors(item: &Item) -> Vec<[u8; 4]> {
    match &item.kind {
        ItemKind::Text {
            color, background, ..
        } => {
            let mut out = vec![*color];
            out.extend(background);
            out
        }
        ItemKind::Shape { stroke, fill, .. } => {
            let mut out = vec![stroke.color];
            out.extend(fill);
            out
        }
        ItemKind::Freedraw { color, .. } => vec![*color],
        ItemKind::Chart { color, .. } => vec![*color],
        ItemKind::Pixmap { .. } | ItemKind::Frame { .. } => Vec::new(),
    }
}

/// 遍历 item 的每个颜色槽位（可变）。
fn item_colors_mut(item: &mut Item) -> Vec<&mut [u8; 4]> {
    match &mut item.kind {
        ItemKind::Text {
            color, background, ..
        } => {
            let mut out: Vec<&mut [u8; 4]> = vec![color];
            out.extend(background.as_mut());
            out
        }
        ItemKind::Shape { stroke, fill, .. } => {
            let mut out: Vec<&mut [u8; 4]> = vec![&mut stroke.color];
            out.extend(fill.as_mut());
            out
        }
        ItemKind::Freedraw { color, .. } => vec![color],
        ItemKind::Chart { color, .. } => vec![color],
        ItemKind::Pixmap { .. } | ItemKind::Frame { .. } => Vec::new(),
    }
}

/// 判定整档颜色是否创作于深色主题：精确命中色板/默认墨色多者胜；
/// 无精确命中时按每色"离基色色板 vs 离暗色滤镜色板"的最近距离多数投票。
fn looks_dark_authored(scene: &Scene) -> bool {
    let base = palette_base_colors();
    let dark: Vec<([u8; 3], [u8; 3])> = base
        .iter()
        .map(|&b| (apply_dark_mode_filter(b), b))
        .chain(std::iter::once((
            LEGACY_DARK_INK_RGB,
            DEFAULT_INK_RGBA[..3].try_into().expect("RGB"),
        )))
        .collect();

    let mut exact_base = 0usize;
    let mut exact_dark = 0usize;
    let mut near_dark = 0usize;
    let mut near_base = 0usize;
    for item in &scene.items {
        for c in item_colors(item) {
            let rgb = [c[0], c[1], c[2]];
            if base.iter().any(|&b| rgb_eq(b, rgb)) {
                exact_base += 1;
            } else if dark.iter().any(|&(d, _)| rgb_eq(d, rgb)) {
                exact_dark += 1;
            } else {
                let d_base = base
                    .iter()
                    .map(|&b| dist2(b, rgb))
                    .min()
                    .unwrap_or(u32::MAX);
                let d_dark = dark
                    .iter()
                    .map(|&(d, _)| dist2(d, rgb))
                    .min()
                    .unwrap_or(u32::MAX);
                if d_dark < d_base {
                    near_dark += 1;
                } else {
                    near_base += 1;
                }
            }
        }
    }
    if exact_dark != exact_base {
        exact_dark > exact_base
    } else {
        near_dark > near_base
    }
}

/// 旧存档颜色迁移：整档判定为深色创作时，把所有元素颜色转回基色空间。
///
/// 精确命中"暗色滤镜色"的颜色直接映射到对应基色；其余（自定义 hex）走
/// [`inverse_dark_mode_filter`]。返回是否发生了转换（false = 本来就是基色空间）。
pub fn migrate_legacy_scene(scene: &mut Scene) -> bool {
    if !looks_dark_authored(scene) {
        return false;
    }
    let base = palette_base_colors();
    let dark_map: Vec<([u8; 3], [u8; 3])> = base
        .iter()
        .map(|&b| (apply_dark_mode_filter(b), b))
        .chain(std::iter::once((
            LEGACY_DARK_INK_RGB,
            DEFAULT_INK_RGBA[..3].try_into().expect("RGB"),
        )))
        .collect();

    for item in &mut scene.items {
        for slot in item_colors_mut(item) {
            let rgb = [slot[0], slot[1], slot[2]];
            let base_rgb = dark_map
                .iter()
                .find(|&&(d, _)| rgb_eq(d, rgb))
                .map(|&(_, b)| b)
                .unwrap_or_else(|| inverse_dark_mode_filter(rgb));
            slot[0] = base_rgb[0];
            slot[1] = base_rgb[1];
            slot[2] = base_rgb[2];
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;
    use crate::shape::StrokeStyle;

    fn text_item(color: [u8; 4]) -> Item {
        Item::new_text("hi".into(), 0.0, 0.0, 20.0, color)
    }

    fn shape_item(stroke: [u8; 4], fill: Option<[u8; 4]>) -> Item {
        let mut item = Item::new_shape(
            crate::shape::ShapeType::Rectangle,
            (10.0, 10.0),
            0.0,
            0.0,
            StrokeStyle {
                color: stroke,
                ..StrokeStyle::default()
            },
            fill,
        );
        if let ItemKind::Shape { fill: f, .. } = &mut item.kind {
            *f = fill;
        }
        item
    }

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

    /// 灰度色 hue-rotate 后仍是灰度（R=G=B）。
    #[test]
    fn dark_filter_preserves_grayscale() {
        let [r, g, b] = apply_dark_mode_filter([100, 100, 100]);
        assert_eq!(r, g);
        assert_eq!(g, b);
    }

    /// 纯红变亮红，色相保持。
    #[test]
    fn dark_filter_keeps_hue_and_lightens_red() {
        let [r, g, b] = apply_dark_mode_filter([0xe0, 0x31, 0x31]);
        assert!(r > g && r > b, "hue lost: got ({r},{g},{b})");
        assert!(r > 0xe0, "not lightened: got ({r},{g},{b})");
    }

    /// 逆滤镜对中间调（不触发 clamp 的颜色）应精确往返；迁移里色板色本身走
    /// 精确映射，逆变换只服务自定义 hex。
    #[test]
    fn inverse_filter_recovers_midtones() {
        for c in [[100, 100, 100], [100, 140, 180], [60, 90, 120]] {
            let back = inverse_dark_mode_filter(apply_dark_mode_filter(c));
            let max_err = c
                .iter()
                .zip(back.iter())
                .map(|(a, b)| (*a as i32 - *b as i32).abs())
                .max()
                .unwrap();
            assert!(max_err <= 8, "{c:?} → {back:?} (最大误差 {max_err})");
        }
    }

    /// 深色创作整档（滤镜后的色板色 + 旧深色默认墨）→ 转回基色。
    #[test]
    fn migrates_dark_authored_scene_to_base() {
        let red_dark = apply_dark_mode_filter(color_from_hex("#e03131"));
        let mut scene = Scene::new();
        scene.add_item(text_item([red_dark[0], red_dark[1], red_dark[2], 255]));
        scene.add_item(shape_item(
            [
                LEGACY_DARK_INK_RGB[0],
                LEGACY_DARK_INK_RGB[1],
                LEGACY_DARK_INK_RGB[2],
                255,
            ],
            Some([red_dark[0], red_dark[1], red_dark[2], 128]),
        ));
        assert!(migrate_legacy_scene(&mut scene));
        let text = item_colors(&scene.items[0]);
        assert_eq!(text[0], [0xe0, 0x31, 0x31, 255], "文本红应还原基色");
        let shape = item_colors(&scene.items[1]);
        assert_eq!(shape[0], DEFAULT_INK_RGBA, "深色默认墨应还原基色墨");
        assert_eq!(
            [shape[1][0], shape[1][1], shape[1][2]],
            [0xe0, 0x31, 0x31],
            "填充红应还原基色"
        );
        assert_eq!(shape[1][3], 128, "alpha 保留");
    }

    /// 浅色创作整档（基色色板 + 基色墨）原样保留。
    #[test]
    fn keeps_light_authored_scene_untouched() {
        let mut scene = Scene::new();
        scene.add_item(shape_item([0x1e, 0x1e, 0x1e, 255], None));
        scene.add_item(text_item([0xe0, 0x31, 0x31, 255]));
        let before: Vec<[u8; 4]> = scene.items.iter().flat_map(item_colors).collect();
        assert!(!migrate_legacy_scene(&mut scene));
        let after: Vec<[u8; 4]> = scene.items.iter().flat_map(item_colors).collect();
        assert_eq!(before, after);
    }
}
