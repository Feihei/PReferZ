use serde::{Deserialize, Serialize};

/// 图形类型。Line/Arrow 为 Phase B 启用，本文件先定义枚举保证序列化稳定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapeType {
    Rectangle,
    Ellipse,
    Diamond,
    Line,
    Arrow,
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

#[cfg(test)]
mod tests {
    use super::*;

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
        for st in [ShapeType::Rectangle, ShapeType::Ellipse, ShapeType::Diamond] {
            let json = serde_json::to_string(&st).unwrap();
            let back: ShapeType = serde_json::from_str(&json).unwrap();
            assert_eq!(st, back);
        }
    }
}
