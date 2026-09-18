//! present — 从 preferz_app 拆出的视图/交互方法段（架构整固 Step 3b）。
//! 方法体逐字搬迁；`use super::*;` 取 mod.rs 的模块级词汇与私有项（子模块可访问父模块私有）。
use super::*;

impl PReferZApp {
    /// 进入演示：收集尺寸达标的画框快照（按编号升序）+ 预计算成员，记录视口，进入全屏。
    pub(crate) fn enter_present(&mut self, ctx: &egui::Context) {
        let slides: Vec<ItemId> = self
            .scene
            .frames_by_number()
            .into_iter()
            .filter(|id| {
                self.scene.get_item(id).is_some_and(|it| {
                    let s = it.base_size();
                    s.x >= 10.0 && s.y >= 10.0
                })
            })
            .collect();
        if slides.is_empty() {
            self.flash(t(self.lang, T::PresentNoFrames));
            return;
        }
        // 进入时预计算每帧成员快照（翻页不重算）。
        let members: Vec<Vec<ItemId>> = slides
            .iter()
            .map(|id| self.scene.frame_members(*id))
            .collect();
        let saved_pan = self.viewport.pan;
        let saved_zoom = self.viewport.zoom;
        self.app_mode = AppMode::Present {
            slides,
            members,
            index: 0,
            saved_pan,
            saved_zoom,
        };
        self.present_anim = None;
        self.drag = DragState::Idle;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        ctx.request_repaint();
    }

    /// 退出演示：恢复视口与窗口（全屏关闭）。
    pub(crate) fn exit_present(&mut self, ctx: &egui::Context) {
        if let AppMode::Present {
            saved_pan,
            saved_zoom,
            ..
        } = &self.app_mode
        {
            self.viewport.pan = *saved_pan;
            self.viewport.zoom = *saved_zoom;
        }
        self.app_mode = AppMode::Edit;
        self.present_anim = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        ctx.request_repaint();
    }

    /// 计算 Present 模式下把 `frame_rect` 适配到 `screen_rect` 的目标视口 `(zoom, pan)`。
    /// 按渲染标准和设计取 95% 填充；Present 临时放宽 max_zoom（不 clamp 上限）。
    pub(crate) fn present_compute_fit(
        &self,
        screen_rect: egui::Rect,
        frame_rect: CanvasRect,
    ) -> (f32, CanvasVector) {
        let fw = frame_rect.width().max(1.0);
        let fh = frame_rect.height().max(1.0);
        let sw = screen_rect.width().max(1.0);
        let sh = screen_rect.height().max(1.0);
        let zoom = (sw / fw).min(sh / fh) * 0.95;
        let zoom = zoom.max(self.viewport.min_zoom);
        (zoom, frame_rect.center().to_vector())
    }

    /// 每帧把视口渐进趋近目标（翻页过渡 / Present 实时适配 DPI）。
    pub(crate) fn present_apply_fit(
        &mut self,
        ctx: &egui::Context,
        screen_rect: egui::Rect,
        frame_rect: CanvasRect,
    ) {
        let (tz, tp) = match self.present_anim {
            Some(t) => t,
            None => self.present_compute_fit(screen_rect, frame_rect),
        };
        if self.present_anim.is_some() {
            // 指数插值，约 200ms 收敛
            let dt = ctx.input(|i| i.unstable_dt).clamp(0.0, 0.05);
            let k = 1.0 - (-dt / 0.18).exp();
            self.viewport.zoom += (tz - self.viewport.zoom) * k;
            self.viewport.pan.x += (tp.x - self.viewport.pan.x) * k;
            self.viewport.pan.y += (tp.y - self.viewport.pan.y) * k;
            let pan_close = (self.viewport.pan - tp).length() < 0.5;
            if (self.viewport.zoom - tz).abs() < 0.001 && pan_close {
                self.viewport.zoom = tz;
                self.viewport.pan = tp;
                self.present_anim = None;
            } else {
                ctx.request_repaint();
            }
        } else {
            self.viewport.zoom = tz;
            self.viewport.pan = tp;
        }
    }

    pub(crate) fn present_slide_count(&self) -> usize {
        match &self.app_mode {
            AppMode::Present { slides, .. } => slides.len(),
            AppMode::Edit => 0,
        }
    }

    pub(crate) fn present_goto(&mut self, idx: usize, ctx: &egui::Context) {
        let (slides, index) = match &self.app_mode {
            AppMode::Present { slides, index, .. } => (slides.clone(), *index),
            AppMode::Edit => return,
        };
        let n = slides.len();
        if n <= 1 {
            return;
        }
        let idx = idx.min(n - 1);
        if idx == index {
            return;
        }
        if let AppMode::Present { index: slot, .. } = &mut self.app_mode {
            *slot = idx;
        }
        if let Some(frame) = self.scene.get_item(&slides[idx]) {
            let (z, p) = self.present_compute_fit(ctx.content_rect(), frame.bounding_rect());
            self.present_anim = Some((z, p));
        }
        ctx.request_repaint();
    }

    pub(crate) fn render_present(&mut self, ui: &mut egui::Ui) {
        let ctx = &ui.ctx().clone();
        let (slides, members, index) = match &self.app_mode {
            AppMode::Present {
                slides,
                members,
                index,
                ..
            } => (slides.clone(), members.clone(), *index),
            AppMode::Edit => return,
        };
        if slides.is_empty() {
            return;
        }
        let screen_rect = ctx.content_rect();
        self.viewport.set_screen_rect_egui(screen_rect);

        let Some(frame) = self.scene.get_item(&slides[index]) else {
            return;
        };
        let frame_rect = frame.bounding_rect();
        self.present_apply_fit(ctx, screen_rect, frame_rect);
        let frame_screen_rect = self.viewport.canvas_rect_to_egui(frame_rect);

        // 画布背景跟随主题（D2）；Present 为不透明演示背景。
        let present_bg = self.theme.canvas_bg(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(egui::Color32::from_rgb(
                present_bg[0],
                present_bg[1],
                present_bg[2],
            )))
            .show(ui, |ui| {
                let original_clip = ui.clip_rect();
                // 只绘制当前帧成员，且裁剪到帧矩形，形成独立"幻灯片"画布。
                ui.set_clip_rect(frame_screen_rect);
                for id in &members[index] {
                    let Some(item) = self.scene.get_item(id).cloned() else {
                        continue;
                    };
                    let cull = self.viewport.canvas_rect_to_egui(item.bounding_rect());
                    if screen_rect.intersects(cull) {
                        self.draw_item_visual(ui, &item, None);
                    }
                }
                ui.set_clip_rect(original_clip);
            });

        // 页码指示（右下角）
        egui::Area::new(egui::Id::new("present_page_indicator"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-28.0, -20.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let label = format!("{} / {}", index + 1, slides.len());
                // 页码颜色随主题反色（Light 下浅灰背景上需深色字）。
                let page_text_color = if self.theme.is_dark(ctx) {
                    egui::Color32::from_gray(210)
                } else {
                    egui::Color32::from_gray(40)
                };
                ui.label(egui::RichText::new(label).size(16.0).color(page_text_color));
            });
    }

    pub(crate) fn handle_present_input(&mut self, ctx: &egui::Context) {
        let n = self.present_slide_count();
        let index = match &self.app_mode {
            AppMode::Present { index, .. } => *index,
            AppMode::Edit => return,
        };

        // 退出：取消键 / 再次按演示切换键 / Esc 硬兜底（与 cancel_pressed 同策略）
        let exit = self.keymap.pressed(Action::Cancel, ctx)
            || self.keymap.pressed(Action::TogglePresent, ctx)
            || ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if exit {
            self.exit_present(ctx);
            return;
        }

        let mut delta: i32 = 0;
        if self.keymap.pressed(Action::PresentNext, ctx) {
            delta = 1;
        } else if self.keymap.pressed(Action::PresentPrev, ctx) {
            delta = -1;
        }
        if self.keymap.pressed(Action::PresentFirst, ctx) {
            delta = i32::MIN;
        } else if self.keymap.pressed(Action::PresentLast, ctx) {
            delta = i32::MAX;
        }
        let scroll = ctx.input(|i| i.smooth_scroll_delta).y;
        if scroll != 0.0 {
            // Present 模式滚轮 = 翻页（滚轮向下 → 下一页）
            delta = if scroll < 0.0 { 1 } else { -1 };
        }
        if delta != 0 {
            let target = (index as i32 + delta).clamp(0, n as i32 - 1) as usize;
            self.present_goto(target, ctx);
        }
    }
}
