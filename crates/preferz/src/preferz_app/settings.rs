//! settings — 从 preferz_app 拆出的方法段（架构整固 Step 3c）。方法体逐字搬迁。
//! `use super::*;` 取 mod.rs 的模块级词汇与私有项（子模块可访问父模块私有）。
use super::*;

impl PReferZApp {
    /// 渲染保存提示对话框（关闭/新建时若 dirty 弹出）    /// 按钮    /// - 保存：触发保存流程，首次保存弹系统文件选择器；保存完成后由 poll_background 执行 pending action
    /// - 放弃：不保存，直接执pending action（关闭窗/ 新建画布    /// - 取消：什么都不做，保留当前画布状
    pub(crate) fn render_save_prompt(&mut self, ctx: &egui::Context) {
        if self.pending_save_prompt.is_none() {
            return;
        }
        // 保存进行中：等待完成（poll_background 会自动执pending action
        if self.bg_ops.save_rx.is_some() {
            return;
        }

        let mut save_clicked = false;
        let mut discard_clicked = false;
        let mut cancel_clicked = false;

        egui::Window::new(t(self.lang, T::SavePromptMessage))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(300.0);
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label(t(self.lang, T::SavePromptMessage));
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button(t(self.lang, T::SavePromptSave)).clicked() {
                            save_clicked = true;
                        }
                        if ui.button(t(self.lang, T::SavePromptDiscard)).clicked() {
                            discard_clicked = true;
                        }
                        if ui.button(t(self.lang, T::SavePromptCancel)).clicked() {
                            cancel_clicked = true;
                        }
                    });
                    ui.add_space(8.0);
                });
            });

        if save_clicked {
            // 触发保存流程：保存完成时 poll_background 会执pending action
            self.save_file(ctx);
        } else if discard_clicked {
            // 不保存，直接执行 pending action
            // 关键：把 dirty 置为 false，避免下一帧 close_requested 检测又弹保存提示
            if let Some(action) = self.pending_save_prompt.take() {
                match action {
                    SavePromptAction::Close => {
                        self.dirty = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    SavePromptAction::NewCanvas => {
                        self.reset_canvas(ctx);
                    }
                }
            }
        } else if cancel_clicked {
            self.pending_save_prompt = None;
        }
    }

    /// 把当前配置（语言 + 主题）落盘到 `~/.preferz/config.json`。
    /// keymap 不再持久化（改绑入口已随 D6 移除，见 `PReferZApp::new`）。
    pub(crate) fn persist_config(&self) {
        save_config(&UserConfig {
            lang: self.lang,
            keymap: KeymapMap::default(),
            theme: self.theme,
        });
    }

    /// 渲染设置面板（spec §2.3 简化版：排列间距 + 窗口形态 + 语言 + 主题 + 快捷键）。
    pub(crate) fn render_settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.settings_open;
        // 局部副本，闭包内修改；changed 时记录，闭包外发 ViewportCommand
        let mut always_on_top = self.always_on_top;
        let mut frameless = self.frameless;
        let mut lang = self.lang;
        let mut theme = self.theme;
        let mut top_changed = false;
        let mut frame_changed = false;
        let mut lang_changed = false;
        let mut theme_changed = false;
        egui::Window::new(t(self.lang, T::SettingsTitle))
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(t(self.lang, T::SettingsArrange));
                ui.add(
                    egui::Slider::new(&mut self.arrange_spacing, 0.0..=200.0)
                        .text(t(self.lang, T::SettingsSpacing)),
                );
                ui.separator();

                ui.label(t(self.lang, T::SettingsWindow));
                if ui
                    .checkbox(&mut always_on_top, t(self.lang, T::SettingsAlwaysOnTop))
                    .changed()
                {
                    top_changed = true;
                }
                if ui
                    .checkbox(&mut frameless, t(self.lang, T::SettingsFrameless))
                    .changed()
                {
                    frame_changed = true;
                }
                // 背景透明度：0.15~1.0，配无边框+置顶可作悬浮看图板。
                // 限制最小 0.15 避免空场景下窗口过于不可见（spec §2.3 透明背景注意事项）；
                // egui 0.36 换成 glow 后端后为单层线性 alpha 合成，同值比旧 wgpu 更透，故下限由
                // 0.1 抬到 0.15。
                ui.add(
                    egui::Slider::new(&mut self.bg_alpha, 0.15..=1.0)
                        .text(t(self.lang, T::SettingsBgAlpha))
                        .fixed_decimals(2),
                );
                ui.separator();

                // 语言切换
                ui.label(t(self.lang, T::SettingsLanguage));
                egui::ComboBox::from_id_salt("settings_language")
                    .selected_text(lang.display_name())
                    .show_ui(ui, |ui| {
                        for option in [Lang::En, Lang::Zh] {
                            if ui
                                .selectable_label(lang == option, option.display_name())
                                .clicked()
                            {
                                lang = option;
                                lang_changed = true;
                            }
                        }
                    });
                ui.separator();

                // 主题切换（Phase G）：Light / Dark / Auto（跟随系统）。
                ui.label(t(self.lang, T::SettingsTheme));
                egui::ComboBox::from_id_salt("settings_theme")
                    .selected_text(theme.display_name())
                    .show_ui(ui, |ui| {
                        for option in [ThemeMode::Dark, ThemeMode::Light, ThemeMode::Auto] {
                            if ui
                                .selectable_label(theme == option, option.display_name())
                                .clicked()
                            {
                                theme = option;
                                theme_changed = true;
                            }
                        }
                    });
                ui.separator();

                // 注：键鼠改绑设置入口已按 ADR-0007 / D6 移除（不做用户自定义）。
                // `Action`/`Keymap` 派发架构保留，默认键位由 Phase K 对齐 Excalidraw。
            });
        self.settings_open = open;
        // 应用窗口形态切换
        if top_changed {
            self.always_on_top = always_on_top;
            let level = if always_on_top {
                egui::viewport::WindowLevel::AlwaysOnTop
            } else {
                egui::viewport::WindowLevel::Normal
            };
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
        }
        if frame_changed {
            self.frameless = frameless;
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!frameless));
        }
        if lang_changed {
            self.lang = lang;
            // 持久化到 config.json
            self.persist_config();
        }
        if theme_changed {
            self.theme = theme;
            // 切换主题时把新建元素默认色翻到该主题（D2）；用户仍可手动改色。
            self.default_stroke.color = theme.default_stroke_color(ctx);
            self.persist_config();
        }
    }

    /// 渲染颜色采样 overlay（在鼠标附近显示 RGB/HEX）
    pub(crate) fn render_color_picker_overlay(&self, ctx: &egui::Context) {
        let pos = ctx.input(|i| i.pointer.latest_pos());
        let sample = match self.color_sample.as_ref() {
            Some(s) => s,
            None => {
                // 显示模式提示
                if let Some(p) = pos {
                    egui::Area::new(egui::Id::new("color_picker_hint"))
                        .order(egui::Order::Foreground)
                        .fixed_pos(p + egui::vec2(16.0, 16.0))
                        .show(ctx, |ui| {
                            let frame = egui::Frame::popup(ui.style());
                            frame.show(ui, |ui| {
                                ui.label(
                                    self.shortcut_hint(T::FlashColorPickerHint, &[Action::Cancel]),
                                );
                            });
                        });
                }
                return;
            }
        };
        let pos = pos.unwrap_or(sample.screen_pos);
        egui::Area::new(egui::Id::new("color_picker_overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos + egui::vec2(16.0, 16.0))
            .show(ctx, |ui| {
                let frame = egui::Frame::popup(ui.style());
                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let color = egui::Color32::from_rgba_unmultiplied(
                            sample.r, sample.g, sample.b, sample.a,
                        );
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, color);
                        ui.painter().rect_stroke(
                            rect,
                            2.0,
                            egui::Stroke::new(1.0_f32, egui::Color32::BLACK),
                            egui::StrokeKind::Middle,
                        );
                        ui.vertical(|ui| {
                            ui.label(format!("RGB: {}, {}, {}", sample.r, sample.g, sample.b));
                            ui.label(format!("Alpha: {}", sample.a));
                            ui.label(format!(
                                "HEX: #{:02X}{:02X}{:02X}",
                                sample.r, sample.g, sample.b
                            ));
                            ui.label(format!("位置: ({}, {})", sample.px, sample.py));
                        });
                    });
                });
            });
    }
}
