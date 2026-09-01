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

/// Text item 的可编辑样式快照（Phase H）。
///
/// 单独成结构是为了让 `SetTextStyle` 命令用 `(old, new)` 一对值描述整次改动，
/// 而不是为字号 / 颜色 / 背景各开一条命令——侧栏里拖一次滑块只该产生一条 undo 记录。
/// 三个字段在 UI 上分属不同控件，但命令层按"整份样式"快照，改哪一项都走同一条路径。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    /// 字号（屏幕像素，与 `ItemKind::Text::font_size` 同单位）。
    pub font_size: f32,
    /// 文字颜色 RGBA。
    pub color: [u8; 4],
    /// 文字背景色；`None` = 全透明（默认，与 Excalidraw 一致）。
    pub background: Option<[u8; 4]>,
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
}
