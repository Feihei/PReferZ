use eframe::egui;
use preferz_core::spaces::{CanvasPoint, CanvasRect, CanvasToScreen, CanvasVector};

/// 视口状态：把画布世界坐标 (`CanvasSpace`) 映射到屏幕像素 (`ScreenSpace`)。
///
/// 变换公式（严格互逆）：
/// ```text
/// screen = (canvas - pan) * zoom + screen_center
/// canvas = (screen - screen_center) / zoom + pan
/// ```
/// 其中 `pan` 是"视口中心在画布空间的坐标"，`screen_center` 是
/// `screen_rect.center()`。两个函数必须用同一个中心点（修 W3）。
#[derive(Debug, Clone)]
pub struct ViewportState {
    /// 视口中心在画布空间的坐标。
    pub pan: CanvasVector,
    /// 缩放比例（屏幕像素 / 画布像素）。
    pub zoom: f32,
    pub min_zoom: f32,
    pub max_zoom: f32,
    /// 画布面板在屏幕上的矩形（由 CentralPanel 每帧更新）。
    pub screen_rect: egui::Rect,
    /// 画布面板矩形是否已从占位默认值初始化过。
    /// 首帧直接采用实际矩形、不补偿 pan，避免启动时内容被平移。
    rect_initialized: bool,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            pan: CanvasVector::zero(),
            zoom: 1.0,
            min_zoom: 0.01,
            max_zoom: 100.0,
            screen_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(800.0, 600.0)),
            rect_initialized: false,
        }
    }
}

impl ViewportState {
    /// 设置画布面板的屏幕矩形（CentralPanel 每帧调用）。
    ///
    /// 当矩形中心因侧栏开合/窗口缩放而移动时，反向平移 `pan` 抵消，
    /// 使画布内容的**可见位置**保持不变（修：无侧栏点击元素后侧栏弹出、
    /// 内容整体平移）。首帧（占位默认值→实际矩形）不补偿，避免启动跳变。
    pub fn set_screen_rect(&mut self, rect: egui::Rect) {
        if self.rect_initialized {
            let old_center = self.screen_rect.center();
            let new_center = rect.center();
            let d = new_center - old_center;
            // 中心移动 d 时，pan 同向移动 d/zoom 可令同一画布点的屏幕坐标不变：
            // screen(W) = (W - pan)·zoom + center ⇒ 抵消 center 的位移。
            self.pan.x += d.x / self.zoom;
            self.pan.y += d.y / self.zoom;
        }
        self.screen_rect = rect;
        self.rect_initialized = true;
    }

    /// 屏幕中心点（egui 坐标）。
    fn screen_center(&self) -> egui::Pos2 {
        self.screen_rect.center()
    }

    // ─────────────── 互逆的坐标转换 ───────────────

    /// 屏幕点 → 画布点。与 [`canvas_to_screen`] 严格互逆。
    pub fn screen_to_canvas(&self, screen_pos: egui::Pos2) -> CanvasPoint {
        let center = self.screen_center();
        let unscaled = (screen_pos - center) / self.zoom;
        CanvasPoint::new(unscaled.x + self.pan.x, unscaled.y + self.pan.y)
    }

    /// 画布点 → 屏幕点。与 [`screen_to_canvas`] 严格互逆。
    pub fn canvas_to_screen(&self, canvas_pos: CanvasPoint) -> egui::Pos2 {
        let center = self.screen_center();
        let scaled = (canvas_pos.to_vector() - self.pan) * self.zoom;
        egui::Pos2::new(center.x + scaled.x, center.y + scaled.y)
    }

    /// 画布矩形 → 屏幕矩形。
    pub fn canvas_to_screen_rect(&self, canvas_rect: CanvasRect) -> egui::Rect {
        let min = self.canvas_to_screen(canvas_rect.origin);
        let max = self.canvas_to_screen(canvas_rect.origin + canvas_rect.size);
        egui::Rect::from_min_max(min, max)
    }

    /// 构造 Canvas → Screen 的仿射矩阵（与 [`canvas_to_screen`] 严格一致）。
    pub fn canvas_to_screen_transform(&self) -> CanvasToScreen {
        let center = self.screen_center();
        CanvasToScreen::identity()
            .then_scale(self.zoom, self.zoom)
            .then_translate(euclid::Vector2D::new(
                center.x - self.pan.x * self.zoom,
                center.y - self.pan.y * self.zoom,
            ))
    }

    // ─────────────── 视口操作 ───────────────

    /// 以指定屏幕点为锚点缩放。鼠标位置对应的画布点在缩放前后保持不变。
    ///
    /// 量纲：`screen_pos - screen_center` 是屏幕像素，
    /// `(1/old_zoom - 1/new_zoom)` 是 1/zoom，乘积为画布空间，与 `pan` 同空间。
    /// （修 W4 的量纲错误。）
    pub fn zoom_at(&mut self, delta: f32, screen_pos: egui::Pos2) {
        // delta 钳制防止 NaN/爆炸；基数 1.05 每单位 delta 缩放 5%（原 1.02 太慢）。
        let normalized_delta = delta.clamp(-2.0, 2.0);
        let zoom_factor = 1.05_f32.powf(normalized_delta);
        let new_zoom = (self.zoom * zoom_factor).clamp(self.min_zoom, self.max_zoom);
        if (new_zoom - self.zoom).abs() < 1e-9 {
            return;
        }

        let center = self.screen_center();
        let dx = screen_pos.x - center.x;
        let dy = screen_pos.y - center.y;
        // new_pan = old_pan + (screen_pos - screen_center) * (1/old_zoom - 1/new_zoom)
        let factor = 1.0 / self.zoom - 1.0 / new_zoom;
        self.pan.x += dx * factor;
        self.pan.y += dy * factor;

        self.zoom = new_zoom;
    }

    /// 按屏幕像素 delta 平移视口（delta 是屏幕空间，需除 zoom 转 canvas 空间）。
    pub fn pan_by_screen(&mut self, screen_delta: egui::Vec2) {
        self.pan.x -= screen_delta.x / self.zoom;
        self.pan.y -= screen_delta.y / self.zoom;
    }

    /// 适配内容矩形到视口中心（90% 填充）。
    pub fn fit_to_content(&mut self, content_rect: CanvasRect) {
        let content_w = content_rect.width().max(1.0);
        let content_h = content_rect.height().max(1.0);
        let screen_w = self.screen_rect.width().max(1.0);
        let screen_h = self.screen_rect.height().max(1.0);

        let scale_x = screen_w / content_w;
        let scale_y = screen_h / content_h;
        let scale = scale_x.min(scale_y) * 0.9;
        let scale = scale.clamp(self.min_zoom, self.max_zoom);

        self.zoom = scale;
        // pan = 内容中心
        self.pan = content_rect.center().to_vector();
    }

    /// 无内容时重置到原点。
    pub fn reset(&mut self) {
        self.zoom = 1.0;
        self.pan = CanvasVector::zero();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 右侧栏开合导致 CentralPanel 中心平移时，pan 应反向补偿，
    /// 使同一画布点的屏幕坐标保持不变（修：无侧栏点击元素后侧栏弹出、内容平移）。
    #[test]
    fn set_screen_rect_compensates_pan_on_center_shift() {
        let mut vp = ViewportState::default();
        let r0 = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 800.0));
        // 首帧：直接采用矩形、不补偿（避免启动跳变）
        vp.set_screen_rect(r0);
        assert_eq!(vp.pan.x, 0.0);
        assert_eq!(vp.pan.y, 0.0);

        // 宽度收窄 120（右侧栏出现）→ 中心左移 60
        let r1 = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(880.0, 800.0));
        let w = vp.screen_to_canvas(r0.center());
        vp.set_screen_rect(r1);

        // 同一画布点仍应渲染在原屏幕位置
        let after = vp.canvas_to_screen(w);
        assert!((after.x - r0.center().x).abs() < 1e-4);
        assert!((after.y - r0.center().y).abs() < 1e-4);
        // pan 被补偿 -60（zoom=1）
        assert!((vp.pan.x + 60.0).abs() < 1e-4);
    }
}
