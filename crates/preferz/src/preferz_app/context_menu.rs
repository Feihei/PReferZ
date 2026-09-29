//! context_menu — 从 preferz_app 拆出的视图/交互方法段（架构整固 Step 3b）。
//! 方法体逐字搬迁；`use super::*;` 取 mod.rs 的模块级词汇与私有项（子模块可访问父模块私有）。
use super::*;

impl PReferZApp {
    pub(crate) fn render_context_menu(&mut self, ctx: &egui::Context) {
        let menu_id = egui::Id::new("context_menu");
        let pos = self.context_menu_pos;
        let has_selection = !self.scene.selection.is_empty();
        let primary_pressed = ctx.input(|i| i.pointer.primary_pressed());
        let pointer_pos = ctx.input(|i| i.pointer.latest_pos());

        // egui::Area + 手动按钮。返回菜rect 用于检测点击外部（B4
        let area_response = egui::Area::new(menu_id)
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                let frame = egui::Frame::popup(ui.style());
                frame.show(ui, |ui| {
                    ui.set_max_width(180.0);

                    if ui
                        .button(format!("\u{1F195} {}", t(self.lang, T::NewCanvas)))
                        .clicked()
                    {
                        self.new_canvas(ctx);
                        self.context_menu_open = false;
                    }
                    ui.separator();
                    if ui
                        .button(format!(
                            "\u{25B6} {}",
                            self.shortcut_hint(T::Present, &[Action::TogglePresent])
                        ))
                        .clicked()
                    {
                        self.enter_present(ctx);
                        self.context_menu_open = false;
                    }
                    ui.separator();
                    if ui
                        .button(format!("\u{1F4C2} {}", t(self.lang, T::OpenProject)))
                        .clicked()
                    {
                        self.open_project_file(ctx);
                        self.context_menu_open = false;
                    }
                    if ui
                        .button(format!("\u{1F5BC} {}", t(self.lang, T::LoadImage)))
                        .clicked()
                    {
                        self.import_image_file(ctx);
                        self.context_menu_open = false;
                    }
                    if ui
                        .button(format!(
                            "\u{1F4CB} {}",
                            self.shortcut_hint(T::PasteImage, &[Action::Paste])
                        ))
                        .clicked()
                    {
                        self.paste_from_clipboard(ctx);
                        self.context_menu_open = false;
                    }
                    // mermaid 流程图输入弹窗（plan #9）
                    if ui
                        .button(format!("\u{1F537} {}", t(self.lang, T::MenuMermaid)))
                        .clicked()
                    {
                        self.mermaid_open = true;
                        self.context_menu_open = false;
                    }
                    ui.separator();
                    if ui
                        .button(format!("\u{1F4BE} {}", t(self.lang, T::Save)))
                        .clicked()
                    {
                        self.save_file(ctx);
                        self.context_menu_open = false;
                    }
                    if ui
                        .button(format!("\u{1F4C4} {}", t(self.lang, T::SaveAs)))
                        .clicked()
                    {
                        self.save_file_as(ctx);
                        self.context_menu_open = false;
                    }
                    ui.menu_button(
                        format!("\u{1F4F7} {}", t(self.lang, T::ExportScene)),
                        |ui| {
                            if ui.button(t(self.lang, T::ExportPngAll)).clicked() {
                                self.start_export_dialog(ctx, ExportFormat::Png, false);
                                self.context_menu_open = false;
                            }
                            if ui.button(t(self.lang, T::ExportJpgAll)).clicked() {
                                self.start_export_dialog(ctx, ExportFormat::Jpeg, false);
                                self.context_menu_open = false;
                            }
                            if has_selection {
                                ui.separator();
                                if ui.button(t(self.lang, T::ExportPngSelection)).clicked() {
                                    self.start_export_dialog(ctx, ExportFormat::Png, true);
                                    self.context_menu_open = false;
                                }
                                if ui.button(t(self.lang, T::ExportJpgSelection)).clicked() {
                                    self.start_export_dialog(ctx, ExportFormat::Jpeg, true);
                                    self.context_menu_open = false;
                                }
                            }
                        },
                    );
                    ui.menu_button(
                        format!("\u{1F4E9} {}", t(self.lang, T::ExportImagesToDir)),
                        |ui| {
                            if ui.button(t(self.lang, T::ExportAllImages)).clicked() {
                                self.start_export_images_dialog(ctx, false);
                                self.context_menu_open = false;
                            }
                            if has_selection
                                && ui.button(t(self.lang, T::ExportSelectionImages)).clicked()
                            {
                                self.start_export_images_dialog(ctx, true);
                                self.context_menu_open = false;
                            }
                        },
                    );
                    ui.separator();

                    if has_selection {
                        if ui
                            .button(format!("\u{1F4CB} {}", t(self.lang, T::Copy)))
                            .clicked()
                        {
                            self.copy_selected();
                            self.context_menu_open = false;
                        }
                        if ui
                            .button(format!("\u{2702} {}", t(self.lang, T::Cut)))
                            .clicked()
                        {
                            self.cut_selected();
                            self.context_menu_open = false;
                        }
                        if ui
                            .button(format!("\u{1F5D1} {}", t(self.lang, T::DeleteSelected)))
                            .clicked()
                        {
                            self.delete_selected();
                            self.context_menu_open = false;
                        }
                        // z-order：Frame 恒在最底，选区全为 Frame 时禁用
                        let reorderable = self
                            .scene
                            .selection
                            .iter()
                            .any(|id| !self.scene.get_item(id).is_some_and(|i| i.is_frame()));
                        if ui
                            .add_enabled(
                                reorderable,
                                egui::Button::new(format!(
                                    "\u{2191} {}",
                                    t(self.lang, T::MoveForward)
                                )),
                            )
                            .clicked()
                        {
                            self.move_forward();
                            self.context_menu_open = false;
                        }
                        if ui
                            .add_enabled(
                                reorderable,
                                egui::Button::new(format!(
                                    "\u{23EB} {}",
                                    t(self.lang, T::BringToFront)
                                )),
                            )
                            .clicked()
                        {
                            self.bring_to_front();
                            self.context_menu_open = false;
                        }
                        if ui
                            .add_enabled(
                                reorderable,
                                egui::Button::new(format!(
                                    "\u{2193} {}",
                                    t(self.lang, T::MoveBackward)
                                )),
                            )
                            .clicked()
                        {
                            self.move_backward();
                            self.context_menu_open = false;
                        }
                        if ui
                            .add_enabled(
                                reorderable,
                                egui::Button::new(format!(
                                    "\u{23EC} {}",
                                    t(self.lang, T::SendToBack)
                                )),
                            )
                            .clicked()
                        {
                            self.send_to_back();
                            self.context_menu_open = false;
                        }
                        // 编组/解组（plan #13）：编组需 ≥2 项；解组需选区内有已编组项
                        if self.scene.selection.len() >= 2
                            && ui
                                .button(format!("\u{1F510} {}", t(self.lang, T::MenuGroup)))
                                .clicked()
                        {
                            self.group_selected();
                            self.context_menu_open = false;
                        }
                        if ui
                            .button(format!("\u{1F513} {}", t(self.lang, T::MenuUngroup)))
                            .clicked()
                        {
                            self.ungroup_selected();
                            self.context_menu_open = false;
                        }
                        // elbow → 多段线烘焙（plan #24 DP-3）：当前路由固化为自由
                        // 顶点（视觉不变），此后顶点自由可编辑。仅选区内含 elbow 时显示。
                        if self.selected_elbow_count() > 0
                            && ui
                                .button(format!(
                                    "\u{2796} {}",
                                    t(self.lang, T::MenuConvertToPolyline)
                                ))
                                .clicked()
                        {
                            self.convert_elbows_to_polyline();
                            self.context_menu_open = false;
                        }
                        ui.separator();

                        // Phase 5：灰度/透明度/裁剪（仅 Pixmap 单选时）
                        if self.selected_pixmap_count() == 1 {
                            let is_gray = self.selected_pixmap_grayscale();
                            let gray_label = if is_gray {
                                format!("\u{1F3A8} {}", t(self.lang, T::CancelGrayscale))
                            } else {
                                format!("\u{1F3A8} {}", t(self.lang, T::ToggleGrayscale))
                            };
                            if ui.button(gray_label).clicked() {
                                self.toggle_grayscale_selected();
                                self.context_menu_open = false;
                            }
                            if ui
                                .button(format!("\u{1F4CF} {}", t(self.lang, T::CropMode)))
                                .clicked()
                            {
                                self.enter_crop_mode();
                                self.context_menu_open = false;
                            }
                            ui.separator();
                        }

                        // Phase 5：归一化尺寸（Pixmap 多选）
                        if self.selected_pixmap_count() >= 2 {
                            ui.menu_button(
                                format!("\u{1F4D0} {}", t(self.lang, T::NormalizeSize)),
                                |ui| {
                                    if ui.button(t(self.lang, T::NormalizeByWidth)).clicked() {
                                        self.normalize_selected(
                                            preferz_core::commands::NormalizeMode::Width,
                                        );
                                        self.context_menu_open = false;
                                    }
                                    if ui.button(t(self.lang, T::NormalizeByHeight)).clicked() {
                                        self.normalize_selected(
                                            preferz_core::commands::NormalizeMode::Height,
                                        );
                                        self.context_menu_open = false;
                                    }
                                    if ui.button(t(self.lang, T::NormalizeByArea)).clicked() {
                                        self.normalize_selected(
                                            preferz_core::commands::NormalizeMode::Area,
                                        );
                                        self.context_menu_open = false;
                                    }
                                },
                            );
                        }

                        // Phase 5：批量排列（≥ 2 项）
                        if self.scene.selection.len() >= 2 {
                            ui.menu_button(
                                format!("\u{1F9ED} {}", t(self.lang, T::Arrange)),
                                |ui| {
                                    if ui.button(t(self.lang, T::ArrangeLinear)).clicked() {
                                        self.arrange_selected(ArrangeMode::Linear);
                                        self.context_menu_open = false;
                                    }
                                    if ui.button(t(self.lang, T::ArrangeGrid)).clicked() {
                                        self.arrange_selected(ArrangeMode::Grid);
                                        self.context_menu_open = false;
                                    }
                                    if ui.button(t(self.lang, T::ArrangeOptimal)).clicked() {
                                        self.arrange_selected(ArrangeMode::Optimal);
                                        self.context_menu_open = false;
                                    }
                                    // 对齐 / 分布（plan #6）：与属性栏同一套动作
                                    ui.separator();
                                    ui.menu_button(
                                        format!("\u{21D4} {}", t(self.lang, T::PropsSectionAlign)),
                                        |ui| {
                                            for (mode, key) in [
                                                (AlignMode::Left, T::AlignLeft),
                                                (AlignMode::HCenter, T::AlignHCenter),
                                                (AlignMode::Right, T::AlignRight),
                                                (AlignMode::Top, T::AlignTop),
                                                (AlignMode::VCenter, T::AlignVCenter),
                                                (AlignMode::Bottom, T::AlignBottom),
                                            ] {
                                                if ui.button(t(self.lang, key)).clicked() {
                                                    self.align_selected(mode);
                                                    self.context_menu_open = false;
                                                }
                                            }
                                        },
                                    );
                                    ui.menu_button(
                                        format!("\u{22EF} {}", t(self.lang, T::Distribute)),
                                        |ui| {
                                            let enabled = self.scene.selection.len() >= 3;
                                            for (axis, dist_mode, key) in [
                                                (
                                                    DistributeAxis::Horizontal,
                                                    DistributeMode::Gap,
                                                    T::DistributeHGap,
                                                ),
                                                (
                                                    DistributeAxis::Horizontal,
                                                    DistributeMode::Centers,
                                                    T::DistributeHCenters,
                                                ),
                                                (
                                                    DistributeAxis::Vertical,
                                                    DistributeMode::Gap,
                                                    T::DistributeVGap,
                                                ),
                                                (
                                                    DistributeAxis::Vertical,
                                                    DistributeMode::Centers,
                                                    T::DistributeVCenters,
                                                ),
                                            ] {
                                                let resp = ui.add_enabled(
                                                    enabled,
                                                    egui::Button::new(t(self.lang, key)),
                                                );
                                                if resp.clicked() {
                                                    self.distribute_selected(axis, dist_mode);
                                                    self.context_menu_open = false;
                                                } else if !enabled {
                                                    resp.on_disabled_hover_text(t(
                                                        self.lang,
                                                        T::FlashDistributeNeedThree,
                                                    ));
                                                }
                                            }
                                        },
                                    );
                                },
                            );
                        }

                        ui.separator();
                    }

                    // 颜色采样模式（spec §2.2）
                    let picker_label = if self.color_picker_active {
                        format!("\u{1F3A8} {}", t(self.lang, T::ExitColorPicker))
                    } else {
                        format!("\u{1F3A8} {}", t(self.lang, T::ColorPickerMode))
                    };
                    if ui.button(picker_label).clicked() {
                        self.color_picker_active = !self.color_picker_active;
                        self.context_menu_open = false;
                    }

                    if ui
                        .button(format!("\u{1F527} {}", t(self.lang, T::Settings)))
                        .clicked()
                    {
                        self.settings_open = true;
                        self.context_menu_open = false;
                    }

                    if ui
                        .button(format!("\u{1F50D} {}", t(self.lang, T::FitToCanvas)))
                        .clicked()
                    {
                        self.fit_to_screen();
                        self.context_menu_open = false;
                    }
                    if ui
                        .button(format!("\u{1F504} {}", t(self.lang, T::ResetZoom)))
                        .clicked()
                    {
                        self.viewport.reset();
                        self.flash(t(self.lang, T::FlashResetZoom).to_string());
                        self.context_menu_open = false;
                    }
                    ui.separator();
                    if ui
                        .button(format!("\u{274C} {}", t(self.lang, T::Exit)))
                        .clicked()
                    {
                        // 触发 close_requested 流程；dirty 时由 update 顶部检测弹保存提示
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        self.context_menu_open = false;
                    }
                });
                ui.min_rect()
            });

        // 点击菜单外部 关闭菜单（修 B4
        if primary_pressed {
            if let Some(p) = pointer_pos {
                if !area_response.inner.contains(p) {
                    self.context_menu_open = false;
                }
            }
        }
    }
}
