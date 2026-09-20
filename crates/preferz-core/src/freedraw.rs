//! 徒手绘制（freedraw）的纯几何（L1）：把一串采样点变成一条**速度锥形**的
//! 可变宽墨迹——中心线点 + 逐点笔宽。渲染层（L3）据此逐段描边画墨迹（非填充轮廓）。
//!
//! 全部为无副作用、egui-free、可无头测试的函数（core-sink 纪律）：
//! - egui 无指针压感输入，宽度由**相邻点间距**（局部速度代理）模拟：运笔快→点稀→细，
//!   慢→点密→粗；再叠加首尾收笔锥形（pen lift）。间距换算考虑 zoom，使同一手部速度
//!   在不同缩放下得到一致的笔形（手感稳定、可复现）。
//! - 数据只存最终 `points` + `widths`（画布单位），不存时间戳、不存中间速度，落盘/重开
//!   形状恒定。

/// 「中速」参考间距（**屏幕**像素/采样）：笔宽映射以此为中点。运笔快慢相对它增减。
const REFERENCE_SCREEN_SPEED: f32 = 14.0;
/// 笔宽相对基准的最小比例（最快运笔 ≈ 基准 × 此值，收笔端亦用此值）。
const MIN_WIDTH_FACTOR: f32 = 0.28;
/// 收笔锥形作用的端点采样数（首尾各这么多点线性渐细到 [`MIN_WIDTH_FACTOR`]）。
const END_TAPER_SAMPLES: usize = 3;

/// 由采样中心线点求每点笔宽（画布单位）。
///
/// - `points`：局部/画布坐标（同一空间即可，函数只用相邻间距）。
/// - `base_width`：慢速运笔的目标笔宽（画布单位，通常取当前描边宽度）。
/// - `zoom`：当前视口缩放，用于把「手部屏幕速度」折算成画布间距参考，使笔形与缩放无关。
///
/// 返回与 `points` 等长；`points.len() < 2` 时全部退化为 `base_width`（单点无速度可言）。
pub fn widths_from_spacing(points: &[(f32, f32)], base_width: f32, zoom: f32) -> Vec<f32> {
    let n = points.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![base_width.max(0.5)];
    }

    // 参考间距（画布单位）= 屏幕参考 / zoom：zoom 越大，同样的手部速度采到的画布间距越小，
    // 参考随之缩小，速度→宽度映射保持稳定。
    let ref_canvas = REFERENCE_SCREEN_SPEED / zoom.max(1e-3);

    // 逐点瞬时速度代理：到前一点的距离（首点用「下一点」）。
    let mut speed = vec![0.0f32; n];
    for i in 0..n {
        let (a, b) = if i == 0 {
            (points[0], points[1])
        } else {
            (points[i - 1], points[i])
        };
        speed[i] = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    }

    // 三点滑动平均：抑制单帧抖动造成的笔宽毛刺。
    let smooth: Vec<f32> = (0..n)
        .map(|i| {
            let lo = i.saturating_sub(1);
            let hi = (i + 1).min(n - 1);
            let sum: f32 = (lo..=hi).map(|j| speed[j]).sum();
            sum / (hi - lo + 1) as f32
        })
        .collect();

    // 速度→宽度：t = s/(s+ref) ∈ [0,1) 单调递增；宽度 = base × (1 − (1−MIN)·t)。
    // 慢（s→0）→ base，快（s→∞）→ base × MIN_WIDTH_FACTOR。
    let taper = 1.0 - MIN_WIDTH_FACTOR; // 快慢之间的可调行程
    let widths: Vec<f32> = smooth
        .iter()
        .map(|&s| {
            let t = s / (s + ref_canvas);
            base_width * (1.0 - taper * t)
        })
        .collect();

    apply_end_taper(&widths, base_width)
}

/// 把首尾若干采样渐细（pen lift 收笔），中段保持给定宽度。单点/两点不额外处理。
fn apply_end_taper(widths: &[f32], base_width: f32) -> Vec<f32> {
    let n = widths.len();
    if n < 3 {
        return widths
            .iter()
            .map(|w| (*w).max(base_width * MIN_WIDTH_FACTOR))
            .collect();
    }
    let k = END_TAPER_SAMPLES.min(n / 2);
    let mut out = widths.to_vec();
    for i in 0..k {
        // 端点最细（MIN 比例），向内部线性回到全宽。
        let ramp = (i + 1) as f32 / (k + 1) as f32; // ∈ (0,1)
        let factor = MIN_WIDTH_FACTOR + (1.0 - MIN_WIDTH_FACTOR) * ramp;
        let floor = base_width * factor;
        out[i] = out[i].min(floor);
        out[n - 1 - i] = out[n - 1 - i].min(floor);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_pts(n: usize, step: f32) -> Vec<(f32, f32)> {
        (0..n).map(|i| (i as f32 * step, 0.0)).collect()
    }

    #[test]
    fn widths_handles_empty_single_and_matches_len() {
        assert!(widths_from_spacing(&[], 2.0, 1.0).is_empty());
        assert_eq!(widths_from_spacing(&[(0.0, 0.0)], 2.0, 1.0), vec![2.0]);
        let w = widths_from_spacing(&line_pts(10, 5.0), 3.0, 1.0);
        assert_eq!(w.len(), 10);
        assert!(w.iter().all(|&x| x > 0.0 && x.is_finite()));
    }

    #[test]
    fn faster_motion_is_thinner() {
        // 同样点数：大步距（快）整体应比小步距（慢）细（除首尾收笔外取中段比较）。
        let slow = widths_from_spacing(&line_pts(12, 2.0), 4.0, 1.0);
        let fast = widths_from_spacing(&line_pts(12, 40.0), 4.0, 1.0);
        let mid = |w: &[f32]| w[5..7].iter().sum::<f32>() / 2.0;
        assert!(
            mid(&fast) < mid(&slow),
            "快运笔中段笔宽应更细：fast={} slow={}",
            mid(&fast),
            mid(&slow)
        );
    }

    #[test]
    fn widths_are_zoom_stable() {
        // 同一手部屏幕速度：画布间距 = 屏幕间距 / zoom，不同 zoom 下应得几乎相同笔宽。
        let screen_step = 20.0;
        let at1 = widths_from_spacing(&line_pts(12, screen_step / 1.0), 4.0, 1.0);
        let at2 = widths_from_spacing(&line_pts(12, screen_step / 2.0), 4.0, 2.0);
        let mid = |w: &[f32]| w[5..7].iter().sum::<f32>() / 2.0;
        assert!(
            (mid(&at1) - mid(&at2)).abs() < 1e-2,
            "笔宽应随缩放保持稳定：{} vs {}",
            mid(&at1),
            mid(&at2)
        );
    }

    #[test]
    fn ends_taper_thinner_than_middle() {
        // 均匀慢速直线：中段接近基准宽，两端收笔更细。
        let w = widths_from_spacing(&line_pts(15, 1.0), 5.0, 1.0);
        assert!(w[0] < w[7]);
        assert!(w[w.len() - 1] < w[7]);
        // 首尾对称。
        assert!((w[0] - w[w.len() - 1]).abs() < 1e-3);
    }
}
