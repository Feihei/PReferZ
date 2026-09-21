use serde::{Deserialize, Serialize};

/// 图形类型。Polyline 为线性对象：直线 = 2 顶点，未来多段线 / 曲线同用此类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapeType {
    Rectangle,
    Ellipse,
    Diamond,
    Polyline,
}

/// 曲线模式（Phase I）。
///
/// `Straight` = 顶点直线相连；`Curved` = Catmull-Rom 插值（开/闭曲线，
/// 见 [`crate::item`] 的采样辅助函数）。仅 Polyline 生效，矩形族忽略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CurveType {
    /// 折线（默认）。
    #[default]
    Straight,
    /// 平滑曲线。
    Curved,
}

/// 端点箭头样式。`Option<ArrowHeadStyle>` 表示"该端无箭头"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArrowHeadStyle {
    /// 标准三角箭头。
    Arrow,
    /// 圆点（Phase I 新增）。
    Dot,
}

/// 描边线型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DashStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

/// 手绘风抖动档位（plan #3，取代旧 `rough: bool`）。
///
/// `Off` = 关闭手绘风（CleanStyler，即旧 `rough: false` 语义）；其余三档对齐
/// Excalidraw 的 sloppiness 选择器，抖动幅度依次增大，`Artist` ≈ 旧
/// `rough: true` 的观感。幅度乘数见 [`Sloppiness::amp_scale`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Sloppiness {
    /// 关闭手绘风（默认；旧 `rough: false`）。
    #[default]
    Off,
    /// 建筑师（轻微抖动）。
    Architect,
    /// 画师（中等抖动，旧 `rough: true` 观感）。
    Artist,
    /// 卡通（夸张抖动）。
    Cartoonist,
}

impl Sloppiness {
    /// 抖动幅度乘数（1.0 = 旧 `rough: true` 观感，Phase F 基准）。
    pub fn amp_scale(self) -> f32 {
        match self {
            Sloppiness::Off => 0.0,
            Sloppiness::Architect => 0.5,
            Sloppiness::Artist => 1.0,
            Sloppiness::Cartoonist => 1.8,
        }
    }
}

/// `Sloppiness` 字段的兼容反序列化（plan #3）。
///
/// 旧存档此字段名为 `rough` 且存 bool：`false` → `Off`、`true` → `Artist`
/// （保住手绘观感）；新存档为小写字符串。未知值一律降级 `Off`，宁缺勿炸。
/// 字段缺失时由 `#[serde(default)]` 走 `Default`（= `Off`），不经本函数。
pub fn deserialize_sloppiness<'de, D>(deserializer: D) -> Result<Sloppiness, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Repr {
        Bool(bool),
        Str(String),
    }
    Ok(match Repr::deserialize(deserializer) {
        Ok(Repr::Bool(true)) => Sloppiness::Artist,
        Ok(Repr::Str(s)) => match s.as_str() {
            "architect" => Sloppiness::Architect,
            "artist" => Sloppiness::Artist,
            "cartoonist" => Sloppiness::Cartoonist,
            _ => Sloppiness::Off,
        },
        _ => Sloppiness::Off,
    })
}

/// 填充样式（Excalidraw 同款四态中的三种有填充样式；"无填充"由
/// `fill: None` 表达）。仅闭合图形生效。
///
/// serde 默认值为 `Solid`：旧存档只有 `fill`（Some = 纯色填充）没有本字段，
/// 缺省按 `Solid` 加载即与历史行为一致；新建元素 `fill` 恒为 `None`，
/// 字段值在用户选择填充样式时才被写入。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FillStyle {
    /// 纯色填充。
    #[default]
    Solid,
    /// 斜线填充（rough.js 同款 -41° 平行线）。
    Hachure,
    /// 交叉线填充（斜线两遍，第二遍旋转 90°）。
    CrossHatch,
}

/// 描边样式（画布空间像素；颜色 RGBA）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StrokeStyle {
    pub color: [u8; 4],
    pub width: f32,
    pub dash: DashStyle,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            color: [255, 255, 255, 255],
            width: 2.0,
            dash: DashStyle::Solid,
        }
    }
}

/// 水平对齐（plan #1）。仅**绑定文字**生效（自由文本单行无框，恒 top-left）；
/// 旧存档缺字段按 `Center` 加载（与历史自动居中行为一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextAlignH {
    Left,
    #[default]
    Center,
    Right,
}

/// 垂直对齐（plan #1）。仅**绑定文字**生效；默认 `Middle` 同上。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextAlignV {
    Top,
    #[default]
    Middle,
    Bottom,
}

/// 文字字体族（plan #1）。
///
/// `Handwriting` 使用内嵌 851远星夜行手写体（基于 851手写杂书体改作，
/// 作者 Lakejason0 / 原作者 8:51:22 pm，免费商用许可——详见 assets/FONT_LICENSES.md）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FontFamily {
    /// 黑体（内嵌思源黑体，默认）。
    #[default]
    Normal,
    /// 手写（内嵌 851远星夜行手写体）。
    Handwriting,
}

/// Text item 的可编辑样式快照（Phase H；plan #1 扩展对齐与字体族）。
///
/// 单独成结构是为了让 `SetTextStyle` 命令用 `(old, new)` 一对值描述整次改动，
/// 而不是为字号 / 颜色 / 背景各开一条命令——侧栏里拖一次滑块只该产生一条 undo 记录。
/// 各字段在 UI 上分属不同控件，但命令层按"整份样式"快照，改哪一项都走同一条路径。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    /// 字号（屏幕像素，与 `ItemKind::Text::font_size` 同单位）。
    pub font_size: f32,
    /// 文字颜色 RGBA。
    pub color: [u8; 4],
    /// 文字背景色；`None` = 全透明（默认，与 Excalidraw 一致）。
    pub background: Option<[u8; 4]>,
    /// 水平对齐（仅绑定文字生效）。
    pub align_h: TextAlignH,
    /// 垂直对齐（仅绑定文字生效）。
    pub align_v: TextAlignV,
    /// 字体族（黑体 / 伪手写）。
    pub font_family: FontFamily,
}

/// Pixmap item 的可编辑样式快照（Phase H）。
///
/// 与 [`TextStyle`] 同理：不透明度与灰度在 UI 上是两个控件，但命令层按整份快照
/// 存 old/new，一次改动只产生一条 undo 记录。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixmapStyle {
    /// 不透明度 0..1（1.0 = 完全不透明）。
    pub opacity: f32,
    /// 是否以灰度显示。
    pub grayscale: bool,
}

/// 确定性伪随机数发生器（xorshift64\*）。
///
/// 手绘风描边的抖动必须**可复现**：同一 `seed` 恒产生同一序列，
/// 否则每次重绘（缩放、平移、存盘重开）图形轮廓都会跳变。
/// 放在 core 层（不依赖 egui）以便单测确定性。
#[derive(Debug, Clone, Copy)]
pub struct SeededRng {
    state: u64,
}

impl SeededRng {
    /// seed 为 0 时用黄金比例常数兜底：xorshift 在 state=0 时会永久输出 0。
    const FALLBACK_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { Self::FALLBACK_SEED } else { seed },
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        // xorshift64*
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        self.state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// [0, 1) 区间的 f32。
    pub fn next_f32(&mut self) -> f32 {
        // 取高 24 位，保证精度足够且严格小于 1.0
        let v = (self.next_u64() >> 40) as f32;
        v / ((1u64 << 24) as f32)
    }

    /// [-1, 1] 区间的 f32，用于对称抖动。
    pub fn signed(&mut self) -> f32 {
        self.next_f32() * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_rng_is_deterministic() {
        let mut a = SeededRng::new(12345);
        let mut b = SeededRng::new(12345);
        for _ in 0..64 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn seeded_rng_differs_by_seed() {
        let mut a = SeededRng::new(1);
        let mut b = SeededRng::new(2);
        // 两个不同 seed 不可能 16 次全部相同
        assert!((0..16).any(|_| a.next_u64() != b.next_u64()));
    }

    #[test]
    fn seeded_rng_zero_seed_does_not_stick() {
        let mut r = SeededRng::new(0);
        assert_ne!(r.next_u64(), 0);
    }

    #[test]
    fn seeded_rng_f32_ranges() {
        let mut r = SeededRng::new(0xDEAD_BEEF);
        for _ in 0..1024 {
            let v = r.next_f32();
            assert!((0.0..1.0).contains(&v), "next_f32 out of range: {v}");
            let s = r.signed();
            assert!((-1.0..=1.0).contains(&s), "signed out of range: {s}");
        }
    }

    #[test]
    fn stroke_style_serde_roundtrip() {
        let s = StrokeStyle {
            color: [10, 20, 30, 255],
            width: 3.5,
            dash: DashStyle::Dashed,
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: StrokeStyle = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn shape_type_serde_roundtrip() {
        for st in [
            ShapeType::Rectangle,
            ShapeType::Ellipse,
            ShapeType::Diamond,
            ShapeType::Polyline,
        ] {
            let json = serde_json::to_string(&st).unwrap();
            let back: ShapeType = serde_json::from_str(&json).unwrap();
            assert_eq!(st, back);
        }
    }

    #[test]
    fn arrow_head_style_serde_roundtrip() {
        for a in [ArrowHeadStyle::Arrow, ArrowHeadStyle::Dot] {
            let json = serde_json::to_string(&a).unwrap();
            let back: ArrowHeadStyle = serde_json::from_str(&json).unwrap();
            assert_eq!(a, back);
        }
    }

    #[test]
    fn curve_type_serde_roundtrip_and_default() {
        for c in [CurveType::Straight, CurveType::Curved] {
            let json = serde_json::to_string(&c).unwrap();
            let back: CurveType = serde_json::from_str(&json).unwrap();
            assert_eq!(c, back);
        }
        // 派生默认值为 Straight（旧文件缺该字段时按此加载）
        assert_eq!(CurveType::default(), CurveType::Straight);
    }

    #[test]
    fn fill_style_serde_roundtrip_and_default() {
        for f in [FillStyle::Solid, FillStyle::Hachure, FillStyle::CrossHatch] {
            let json = serde_json::to_string(&f).unwrap();
            let back: FillStyle = serde_json::from_str(&json).unwrap();
            assert_eq!(f, back);
        }
        // 旧存档缺 fill_style 字段时按 Solid 加载（历史 fill=Some 即纯色填充）
        assert_eq!(FillStyle::default(), FillStyle::Solid);
    }
}
