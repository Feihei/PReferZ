//! 徒手绘制（freedraw）的纯几何（L1）：把一串采样点变成一条**速度锥形**墨迹的
//! 逐点**相对笔宽**（pressures，取值 (0,1]）。渲染层（L3）用
//! `绝对笔宽 = stroke_width × pressures[i]` 逐段描边（非填充轮廓）。
//!
//! 全部为无副作用、egui-free、可无头测试的函数（core-sink 纪律）：
//! - egui 无指针压感输入，宽度由**相邻点间距**（局部速度代理）模拟：运笔快→点稀→细，
//!   慢→点密→粗；再叠加首尾收笔锥形（pen lift）。间距换算考虑 zoom，使同一手部速度
//!   在不同缩放下得到一致的笔形（手感稳定、可复现）。
//! - 形状（pressures）与粗细（stroke_width）分离：改粗细只动一个标量、不改形状。
//!   pressures 是无量纲乘子，落盘/重开形状恒定。

/// 「中速」参考间距（**屏幕**像素/采样）：笔宽映射以此为中点。运笔快慢相对它增减。
const REFERENCE_SCREEN_SPEED: f32 = 14.0;
/// 相对笔宽的最小值（最快运笔 ≈ 此值，收笔端亦收敛到它附近）。
const MIN_PRESSURE: f32 = 0.28;
/// 收笔锥形作用的端点采样数（首尾各这么多点线性渐细到 [`MIN_PRESSURE`]）。
const END_TAPER_SAMPLES: usize = 3;

/// 由采样中心线点求每点**相对笔宽**乘子（∈ (MIN_PRESSURE, 1]）。
///
/// - `points`：中心线点（与调用方同一坐标空间即可，函数只用相邻间距）。
/// - `zoom`：当前视口缩放，用于把「手部屏幕速度」折算成该空间的参考间距，使笔形与
///   缩放无关。
///
/// 返回与 `points` 等长；`points.len() < 2` 时全部退化为 `1.0`（无速度可言）。
pub fn pressures_from_spacing(points: &[(f32, f32)], zoom: f32) -> Vec<f32> {
    let n = points.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![1.0];
    }

    // 参考间距（点空间）= 屏幕参考 / zoom：zoom 越大，同样手部速度采到的点间距越小，
    // 参考随之缩小，速度→宽度映射保持稳定。
    let ref_spacing = REFERENCE_SCREEN_SPEED / zoom.max(1e-3);

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

    // 速度→相对笔宽：t = s/(s+ref) ∈ [0,1) 单调递增；pressure = 1 − (1−MIN)·t。
    // 慢（s→0）→ 1，快（s→∞）→ MIN_PRESSURE。
    let travel = 1.0 - MIN_PRESSURE; // 快慢之间的可调行程
    let p: Vec<f32> = smooth
        .iter()
        .map(|&s| {
            let t = s / (s + ref_spacing);
            1.0 - travel * t
        })
        .collect();

    apply_end_taper(&p)
}

/// 把首尾若干采样渐细（pen lift 收笔，收敛到接近 [`MIN_PRESSURE`]），中段保持给定值。
fn apply_end_taper(pressures: &[f32]) -> Vec<f32> {
    let n = pressures.len();
    if n < 3 {
        return pressures.iter().map(|&p| p.max(MIN_PRESSURE)).collect();
    }
    let k = END_TAPER_SAMPLES.min(n / 2);
    let mut out = pressures.to_vec();
    for i in 0..k {
        // 端点最细（≈ MIN_PRESSURE），向内部线性回到全宽。
        let ramp = (i + 1) as f32 / (k + 1) as f32; // ∈ (0,1)
        let floor = MIN_PRESSURE + (1.0 - MIN_PRESSURE) * ramp;
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
    fn pressures_handles_empty_single_and_matches_len() {
        assert!(pressures_from_spacing(&[], 1.0).is_empty());
        assert_eq!(pressures_from_spacing(&[(0.0, 0.0)], 1.0), vec![1.0]);
        let w = pressures_from_spacing(&line_pts(10, 5.0), 1.0);
        assert_eq!(w.len(), 10);
        assert!(w.iter().all(|&x| x > 0.0 && x <= 1.0 + 1e-6));
    }

    #[test]
    fn faster_motion_is_thinner() {
        // 同样点数：大步距（快）整体应比小步距（慢）细（除首尾收笔外取中段比较）。
        let slow = pressures_from_spacing(&line_pts(12, 2.0), 1.0);
        let fast = pressures_from_spacing(&line_pts(12, 40.0), 1.0);
        let mid = |w: &[f32]| w[5..7].iter().sum::<f32>() / 2.0;
        assert!(
            mid(&fast) < mid(&slow),
            "快运笔中段相对笔宽应更小：fast={} slow={}",
            mid(&fast),
            mid(&slow)
        );
    }

    #[test]
    fn pressures_are_zoom_stable() {
        // 同一手部屏幕速度：点间距 = 屏幕间距 / zoom，不同 zoom 下应得几乎相同乘子。
        let screen_step = 20.0;
        let at1 = pressures_from_spacing(&line_pts(12, screen_step / 1.0), 1.0);
        let at2 = pressures_from_spacing(&line_pts(12, screen_step / 2.0), 2.0);
        let mid = |w: &[f32]| w[5..7].iter().sum::<f32>() / 2.0;
        assert!(
            (mid(&at1) - mid(&at2)).abs() < 1e-2,
            "相对笔宽应随缩放保持稳定：{} vs {}",
            mid(&at1),
            mid(&at2)
        );
    }

    #[test]
    fn ends_taper_thinner_than_middle() {
        // 均匀慢速直线：中段乘子接近 1，两端收笔更小。
        let w = pressures_from_spacing(&line_pts(15, 1.0), 1.0);
        assert!(w[0] < w[7]);
        assert!(w[w.len() - 1] < w[7]);
        // 首尾对称。
        assert!((w[0] - w[w.len() - 1]).abs() < 1e-3);
    }

    #[test]
    fn pressures_bounded_in_range() {
        // 任意速度下乘子恒在 [MIN_PRESSURE, 1]。
        for step in [0.0f32, 0.5, 3.0, 15.0, 100.0, 1000.0] {
            let w = pressures_from_spacing(&line_pts(9, step), 1.0);
            assert!(w
                .iter()
                .all(|&p| (MIN_PRESSURE - 1e-4..=1.0 + 1e-4).contains(&p)));
        }
    }
}
