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

    /// 自动保存恢复提示（plan #5）：打开 `.prz` 时检测到较新的
    /// `.prz.autosave` 弹出。恢复=载入备份内容（current_file 仍指原文件、
    /// 内容视作未保存）；忽略=保留备份文件不删。
    pub(crate) fn render_autosave_restore_prompt(&mut self, ctx: &egui::Context) {
        let mut restore = false;
        let mut dismiss = false;
        egui::Window::new("autosave_restore")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(320.0);
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label(t(self.lang, T::AutosaveRestorePrompt));
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button(t(self.lang, T::AutosaveRestore)).clicked() {
                            restore = true;
                        }
                        if ui.button(t(self.lang, T::AutosaveDismiss)).clicked() {
                            dismiss = true;
                        }
                    });
                    ui.add_space(8.0);
                });
            });
        if restore {
            let Some(auto) = self.pending_autosave_restore.take() else {
                return;
            };
            let original = self.current_file.clone();
            self.restoring_from_autosave = original;
            self.bg_ops.start_load(ctx, auto, self.lang);
        } else if dismiss {
            self.pending_autosave_restore = None;
        }
    }

    /// 把当前配置（语言 + 主题 + 自动保存 + 全局画框比例 + 默认风格）落盘到
    /// `~/.preferz/config.json`。
    /// keymap 不再持久化（改绑入口已随 D6 移除，见 `PReferZApp::new`）。
    pub(crate) fn persist_config(&self) {
        save_config(&UserConfig {
            lang: self.lang,
            keymap: KeymapMap::default(),
            theme: self.theme,
            autosave_enabled: self.autosave_enabled,
            autosave_interval: self.autosave_interval,
            frame_ratio: self.frame_ratio,
            default_style: self.default_style,
        });
    }

    /// 应用全局画框比例（设置面板变更入口）：持久化到 config.json，并把所有
    /// 「跟随全局」的画框按新比例重算尺寸（保持有效长边、中心锚定，一条 undo）。
    /// `None`（自由）只解除新建锁定，不动已有画框——没有目标比例可联动。
    pub(crate) fn apply_global_frame_ratio(&mut self, ratio: Option<(u32, u32)>) {
        self.frame_ratio = ratio;
        self.persist_config();
        let Some(r) = ratio else {
            return;
        };
        let rf = (r.0 as f32, r.1 as f32);
        let mut items: Vec<(ItemId, FrameGeom, FrameGeom)> = Vec::new();
        for it in self.scene.items_by_z_order() {
            if !it.frame_follows_global_ratio() {
                continue;
            }
            let g = match &it.kind {
                ItemKind::Frame { base_size, .. } => FrameGeom {
                    pos: (it.transform.pos.x, it.transform.pos.y),
                    base: *base_size,
                    scale: (it.transform.scale.x, it.transform.scale.y),
                },
                _ => continue,
            };
            let new = frame_geom_for_ratio(g, rf);
            if new == g {
                continue; // 已是目标比例：跳过，避免产生空 undo
            }
            items.push((it.id, g, new));
        }
        let count = items.len();
        if count == 0 {
            return;
        }
        self.push_cmd(Box::new(SetFrameSize::new_batch(items)));
        self.flash(fill(
            t(self.lang, T::FlashGlobalRatioApplied),
            &[count.to_string()],
        ));
    }

    /// 渲染设置面板（spec §2.3 简化版：排列间距 + 窗口形态 + 语言 + 主题 + 快捷键）。
    pub(crate) fn render_settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.settings_open;
        // 局部副本，闭包内修改；changed 时记录，闭包外发 ViewportCommand
        let mut always_on_top = self.always_on_top;
        let mut frameless = self.frameless;
        let mut lang = self.lang;
        let mut theme = self.theme;
        let mut autosave_enabled = self.autosave_enabled;
        let mut autosave_interval = self.autosave_interval;
        let mut new_frame_ratio = self.frame_ratio;
        let mut frame_ratio_changed = false;
        let mut top_changed = false;
        let mut frame_changed = false;
        let mut lang_changed = false;
        let mut theme_changed = false;
        let mut autosave_changed = false;
        egui::Window::new(t(self.lang, T::SettingsTitle))
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                // 默认风格总开关（2026-09-28）：草绘 = 中档手绘抖动 + 手写体，
                // 规整 = 精确线条 + 黑体。切换一键覆盖当前新建默认档并持久化；
                // 会话内仍可在「新建元素默认样式」侧栏微调 sloppiness。
                ui.label(t(self.lang, T::SettingsDefaultStyle));
                ui.horizontal(|ui| {
                    for (preset, label) in [
                        (DefaultStylePreset::Sketch, T::DefaultStyleSketch),
                        (DefaultStylePreset::Clean, T::DefaultStyleClean),
                    ] {
                        let selected = self.default_style == preset;
                        if ui.selectable_label(selected, t(self.lang, label)).clicked() && !selected
                        {
                            self.default_style = preset;
                            preset.apply_to(
                                &mut self.default_sloppiness,
                                &mut self.default_font_family,
                            );
                            self.persist_config();
                        }
                    }
                });
                ui.separator();

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
                // 背景不透明度：5 档 + 输入框（下限 0.15，避免窗口过于不可见）。
                // egui 0.36 换成 glow 后端后为单层线性 alpha 合成，同值比旧 wgpu 更透，故下限由
                // 0.1 抬到 0.15。用百分比中间变量，档位 [15,30,50,75,100]%。
                ui.label(t(self.lang, T::SettingsBgAlpha));
                let op_labels = [
                    t(self.lang, T::OpMin),
                    t(self.lang, T::OpLow),
                    t(self.lang, T::OpMed),
                    t(self.lang, T::OpHigh),
                    t(self.lang, T::OpMax),
                ];
                let mut bg_pct = (self.bg_alpha * 100.0).round();
                if stepper(
                    ui,
                    &mut bg_pct,
                    &[15.0, 30.0, 50.0, 75.0, 100.0],
                    &op_labels,
                    15.0..=100.0,
                    Some("%"),
                    0.5,
                ) {
                    self.bg_alpha = bg_pct / 100.0;
                }
                ui.separator();

                // 自动保存（plan #5）：开关 + debounce 间隔（最小 10s）
                ui.label(t(self.lang, T::SettingsAutosave));
                if ui
                    .checkbox(
                        &mut autosave_enabled,
                        t(self.lang, T::SettingsAutosaveEnabled),
                    )
                    .changed()
                {
                    autosave_changed = true;
                }
                ui.add_enabled(
                    autosave_enabled,
                    egui::Slider::new(&mut autosave_interval, 10..=300)
                        .text(t(self.lang, T::SettingsAutosaveInterval)),
                );
                if autosave_changed {
                    // 立即持久化（与语言/主题同惯例）
                    self.autosave_enabled = autosave_enabled;
                    self.autosave_interval = autosave_interval;
                    self.persist_config();
                    autosave_changed = false;
                }
                ui.separator();

                // 全局画框比例：新建画框拖拽时锁定比例（Shift 临时自由）；
                // 「跟随全局」的画框在比例变更时联动重算尺寸（一条 undo）。
                ui.label(t(self.lang, T::SettingsFrameRatio));
                let is_custom = new_frame_ratio.is_some_and(|r| !FRAME_RATIO_PRESETS.contains(&r));
                let selected_text = match new_frame_ratio {
                    None => t(self.lang, T::SettingsFrameRatioFree).to_string(),
                    Some(r) if !is_custom => format!("{}:{}", r.0, r.1),
                    Some(_) => t(self.lang, T::SettingsFrameRatioCustom).to_string(),
                };
                egui::ComboBox::from_id_salt("settings_frame_ratio")
                    .selected_text(selected_text)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(
                                new_frame_ratio.is_none(),
                                t(self.lang, T::SettingsFrameRatioFree),
                            )
                            .clicked()
                        {
                            new_frame_ratio = None;
                            frame_ratio_changed = true;
                        }
                        for (w, h) in FRAME_RATIO_PRESETS {
                            let r = Some((w, h));
                            if ui
                                .selectable_label(new_frame_ratio == r, format!("{w}:{h}"))
                                .clicked()
                            {
                                new_frame_ratio = r;
                                frame_ratio_changed = true;
                            }
                        }
                        if ui
                            .selectable_label(is_custom, t(self.lang, T::SettingsFrameRatioCustom))
                            .clicked()
                        {
                            // 从自由进入自定义：16:9 起步，随后可用下方 W/H 微调
                            new_frame_ratio = new_frame_ratio.or(Some((16, 9)));
                            frame_ratio_changed = true;
                        }
                    });
                if let Some((w, h)) = &mut new_frame_ratio {
                    ui.horizontal(|ui| {
                        ui.label("W");
                        if ui.add(egui::DragValue::new(w).range(1..=10_000)).changed() {
                            frame_ratio_changed = true;
                        }
                        ui.label("H");
                        if ui.add(egui::DragValue::new(h).range(1..=10_000)).changed() {
                            frame_ratio_changed = true;
                        }
                    });
                }
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
        if frame_ratio_changed {
            // 全局比例变更：持久化 + 联动更新「跟随全局」的画框（一条 undo）。
            self.apply_global_frame_ratio(new_frame_ratio);
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
