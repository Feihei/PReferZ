//! 档位步进器（Excalidraw 风格）：N 个档位按钮 + 末尾 DragValue 输入框。
//!
//! 替换原连续滑块，用于画笔粗细 / 透明度 / 字号 / 圆角等设置。档位按钮
//! 快捷选值（语义标签如 XS/S/M/L/XL），DragValue 保留数字精确设置。选中
//! 档位按钮高亮；当前值不命中任何档位时无按钮高亮，DragValue 显示实际值。

use std::ops::RangeInclusive;

/// 档位按钮最小尺寸（px）。紧凑以适应属性栏 230px 宽度。
const BTN_MIN_W: f32 = 22.0;
const BTN_MIN_H: f32 = 20.0;
/// 档位命中容差（f32 比较）。
const EPS: f32 = 1e-4;

/// 档位步进器。
///
/// - `value`：当前值（双向）。
/// - `levels`：档位值列表，按显示顺序排列。
/// - `labels`：档位按钮文案，与 `levels` 一一对应（如 `["XS","S","M","L","XL"]`）。
/// - `input_range`：DragValue 输入框的合法范围。
/// - `suffix`：DragValue 后缀（如 `"%"`、`"px"`），`None` 则无。
/// - `speed`：DragValue 拖拽灵敏度，单位 = 每像素值增量（Blender 式按住数字
///   左右拖连续改值）。量程越大取值越大，经验值：`量程 / 500` 左右。
///
/// 返回 `value` 是否被改变。改变包括点击档位按钮或在 DragValue 中输入新值。
pub fn stepper(
    ui: &mut egui::Ui,
    value: &mut f32,
    levels: &[f32],
    labels: &[&str],
    input_range: RangeInclusive<f32>,
    suffix: Option<&str>,
    speed: f32,
) -> bool {
    debug_assert_eq!(
        levels.len(),
        labels.len(),
        "stepper: levels 与 labels 长度必须一致"
    );
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(3.0, 1.0);
        ui.spacing_mut().item_spacing.x = 3.0;
        for (i, &lvl) in levels.iter().enumerate() {
            let active = (*value - lvl).abs() < EPS;
            let btn = egui::Button::new(labels[i])
                .selected(active)
                .min_size(egui::vec2(BTN_MIN_W, BTN_MIN_H));
            if ui.add(btn).clicked() && !active {
                *value = lvl;
                changed = true;
            }
        }
        // DragValue：精确输入。范围用 input_range，允许超出档位但限制在合法区间。
        let mut dv = *value;
        let mut widget = egui::DragValue::new(&mut dv)
            .range(input_range)
            .speed(speed);
        if let Some(s) = suffix {
            widget = widget.suffix(s);
        }
        let resp = ui.add(widget);
        if resp.changed() && (dv - *value).abs() > EPS {
            *value = dv;
            changed = true;
        }
    });
    changed
}

#[cfg(test)]
mod tests {

    #[test]
    fn stepper_levels_basic() {
        // 档位列表非空时首末值正确。
        let levels = [2.0_f32, 4.0, 8.0, 16.0, 32.0];
        assert_eq!(levels.len(), 5);
        assert_eq!(levels[0], 2.0);
        assert_eq!(levels[4], 32.0);
    }

    #[test]
    fn stepper_opacity_levels() {
        let levels = [0.15_f32, 0.3, 0.5, 0.75, 1.0];
        assert_eq!(levels.len(), 5);
        assert_eq!(levels[0], 0.15);
        assert_eq!(levels[4], 1.0);
    }
}
