//! text_edit — 从 preferz_app 拆出的视图/交互方法段（架构整固 Step 3b）。
//! 方法体逐字搬迁；`use super::*;` 取 mod.rs 的模块级词汇与私有项（子模块可访问父模块私有）。
use super::*;

impl PReferZApp {
    /// 对指定 item 开启文本编辑（Excalidraw 语义），双击与 Enter 快捷键共用：
    /// - `Text` item → 编辑其内容；
    /// - 封闭 `Shape` → 编辑已有绑定文本，没有则新建一个空的；
    /// - 其它（图片 / 未闭合折线 / 画框）→ 不处理并返回 false，由调用方兜底。
    ///
    /// 已在编辑中时直接返回 true（不打断当前输入）。
    pub(crate) fn start_text_edit(&mut self, id: ItemId) -> bool {
        if self.editing_text.is_some() {
            return true;
        }
        let Some(kind) = self.scene.get_item(&id).map(|it| it.kind.clone()) else {
            return false;
        };
        match kind {
            ItemKind::Text {
                content,
                font_size,
                color,
                container_id,
                font_family,
                ..
            } => {
                let canvas_pos = self
                    .scene
                    .get_item(&id)
                    .map(|it| it.transform.pos)
                    .map(|p| CanvasPoint::new(p.x, p.y))
                    .unwrap_or_else(|| CanvasPoint::new(f32::MIN, f32::MIN));
                self.editing_text = Some(EditingText {
                    editing_item_id: Some(id),
                    canvas_pos,
                    buffer: content,
                    font_size,
                    color,
                    first_frame: true,
                    container_id,
                    font_family,
                });
            }
            ref k if is_text_container(k) => {
                let center = self
                    .scene
                    .get_item(&id)
                    .map(|it| it.bounding_rect().center())
                    .map(|p| CanvasPoint::new(p.x, p.y));
                let Some(center) = center else {
                    return false;
                };
                let existing = self.scene.texts_bound_to(id).into_iter().next();
                // 新建绑定文字时初值取容器描边色（默认"跟随边框"，创建即观感正确）。
                let container_stroke = self
                    .scene
                    .get_item(&id)
                    .and_then(|it| match &it.kind {
                        ItemKind::Shape { stroke, .. } => Some(stroke.color),
                        _ => None,
                    })
                    .unwrap_or([255; 4]);
                // 已有绑定文本 → 编辑；否则提交时才写入容器 id 新建一个
                let (editing_item_id, buffer, font_size, color, font_family) =
                    match existing.and_then(|tid| self.scene.get_item(&tid)) {
                        Some(it) => match &it.kind {
                            ItemKind::Text {
                                content,
                                font_size,
                                color,
                                font_family,
                                ..
                            } => (existing, content.clone(), *font_size, *color, *font_family),
                            _ => (
                                None,
                                String::new(),
                                18.0,
                                container_stroke,
                                FontFamily::Normal,
                            ),
                        },
                        None => (
                            None,
                            String::new(),
                            18.0,
                            container_stroke,
                            FontFamily::Normal,
                        ),
                    };
                self.editing_text = Some(EditingText {
                    editing_item_id,
                    canvas_pos: center,
                    buffer,
                    font_size,
                    color,
                    first_frame: true,
                    container_id: Some(id),
                    font_family,
                });
            }
            _ => return false,
        }
        true
    }

    /// 选中项恰好一个时返回其 id（多选返回 None，此时不做"编辑文本"这类单选语义的操作）。
    pub(crate) fn single_selected_id(&self) -> Option<ItemId> {
        if self.scene.selection.len() == 1 {
            self.scene.selection.iter().next().copied()
        } else {
            None
        }
    }

    /// 测量所有 Text item 的实际文字尺寸并更新 `measured_size`（修 B6）。
    /// 仅在 `measured_size` 为 None 时测量（content 变化会清空 measured_size）；
    /// 绑定文本例外——它随容器宽度换行，需每帧重测。
    pub(crate) fn update_text_measured_sizes(&mut self, ctx: &egui::Context) {
        let zoom = self.viewport.zoom;
        let mut updates: Vec<(ItemId, (f32, f32))> = Vec::new();
        for item in &self.scene.items {
            if let ItemKind::Text {
                content,
                font_size,
                measured_size,
                container_id,
                font_family,
                ..
            } = &item.kind
            {
                let font_id = |size: f32| {
                    if *font_family == FontFamily::Handwriting {
                        egui::FontId::new(
                            size,
                            egui::FontFamily::Name(HANDWRITING_FONT_FAMILY.into()),
                        )
                    } else {
                        egui::FontId::proportional(size)
                    }
                };
                if let Some(cid) = container_id {
                    // 绑定文本：换行到容器宽度，随容器 resize 每帧重测。
                    if let Some(container) = self.scene.get_item(cid) {
                        let cw = self
                            .viewport
                            .canvas_to_screen_rect(container.bounding_rect())
                            .width();
                        let wrap = (cw - 12.0).max(20.0);
                        let job = egui::text::LayoutJob::simple(
                            content.clone(),
                            font_id(*font_size * zoom),
                            egui::Color32::WHITE,
                            wrap,
                        );
                        let gal = ctx.fonts_mut(|f| f.layout_job(job));
                        updates.push((item.id, (gal.size().x / zoom, gal.size().y / zoom)));
                        continue;
                    }
                }
                if measured_size.is_none() {
                    // 自由文本：仅在未测量时测（content 变化会清空触发重测）。
                    let gal = ctx.fonts_mut(|fonts| {
                        fonts.layout_no_wrap(
                            content.clone(),
                            font_id(*font_size),
                            egui::Color32::WHITE,
                        )
                    });
                    updates.push((item.id, (gal.size().x, gal.size().y)));
                }
            }
        }
        for (id, (w, h)) in updates {
            if let Some(item) = self.scene.get_item_mut(&id) {
                if let ItemKind::Text { measured_size, .. } = &mut item.kind {
                    *measured_size = Some((w, h));
                }
            }
        }
    }

    /// 渲染文本便签编辑 overlay（spec L243 P2-5）    /// 创建中的文本不在 scene 中；Enter/失焦时提交（非空→AddItem），Esc 取消
    pub(crate) fn render_text_editor(&mut self, ctx: &egui::Context) {
        let mut edit = match self.editing_text.take() {
            Some(e) => e,
            None => return,
        };
        let screen_pos = self.viewport.canvas_to_pos2(edit.canvas_pos);
        // 绑定文本：获取容器屏幕矩形，用于居中 + 定宽（换行）。
        let container_screen_rect = edit.container_id.and_then(|cid| {
            self.scene
                .get_item(&cid)
                .map(|it| self.viewport.canvas_rect_to_egui(it.bounding_rect()))
        });
        let mut commit = false;
        let mut cancel = false;

        // 编辑 overlay 的字体族跟随 item 的 font_family
        let font_id = if edit.font_family == FontFamily::Handwriting {
            egui::FontId::new(
                edit.font_size,
                egui::FontFamily::Name(HANDWRITING_FONT_FAMILY.into()),
            )
        } else {
            egui::FontId::proportional(edit.font_size)
        };

        let mut area =
            egui::Area::new(egui::Id::new("text_edit_area")).order(egui::Order::Foreground);
        if let Some(r) = container_screen_rect {
            // 绑定文本：居中于容器（先按预估高度定位，渲染后再由 min_width 撑开）。
            let w = (r.width() - 16.0).max(60.0);
            let est_h = edit.font_size.max(16.0) * 2.0 + 16.0;
            area = area.fixed_pos(r.center() - egui::vec2(w / 2.0, est_h / 2.0));
        } else {
            area = area.fixed_pos(screen_pos);
        }
        area.show(ctx, |ui| {
            // 弹层背景跟随主题（D2）：去掉硬编码暗色 fill，用 Frame::popup 的主题默认。
            let frame = egui::Frame::popup(ui.style()).stroke(egui::Stroke::new(
                1.0_f32,
                egui::Color32::from_rgb(100, 200, 255),
            ));
            frame.show(ui, |ui| {
                if let Some(r) = container_screen_rect {
                    // 绑定文本编辑：宽度受容器约束，支持换行，居中。
                    let w = (r.width() - 16.0).max(60.0);
                    ui.set_min_width(w);
                    let response = ui.add(
                        egui::TextEdit::multiline(&mut edit.buffer)
                            .desired_width(w)
                            .hint_text("输入文本...")
                            .font(font_id.clone())
                            .text_color(egui::Color32::from_rgba_premultiplied(
                                edit.color[0],
                                edit.color[1],
                                edit.color[2],
                                edit.color[3],
                            )),
                    );
                    if edit.first_frame {
                        response.request_focus();
                        edit.first_frame = false;
                    }
                    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
                        cancel = true;
                    } else if response.lost_focus() {
                        commit = true;
                    }
                } else {
                    ui.set_min_width(120.0);
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut edit.buffer)
                            .desired_width(160.0)
                            .hint_text("输入文本...")
                            .font(font_id)
                            .text_color(egui::Color32::from_rgba_premultiplied(
                                edit.color[0],
                                edit.color[1],
                                edit.color[2],
                                edit.color[3],
                            )),
                    );
                    if edit.first_frame {
                        response.request_focus();
                        edit.first_frame = false;
                    }
                    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
                        cancel = true;
                    } else if response.lost_focus() {
                        commit = true;
                    }
                }
            });
        });

        if cancel {
            self.editing_text = None;
            return;
        }
        if commit {
            match edit.editing_item_id {
                None => {
                    // 创建模式：空内容丢弃，非push AddItem
                    if !edit.buffer.trim().is_empty() {
                        let item = match edit.container_id {
                            Some(cid) => Item::new_text_in(
                                edit.buffer,
                                edit.canvas_pos.x,
                                edit.canvas_pos.y,
                                edit.font_size,
                                edit.color,
                                cid,
                            ),
                            None => Item::new_text(
                                edit.buffer,
                                edit.canvas_pos.x,
                                edit.canvas_pos.y,
                                edit.font_size,
                                edit.color,
                            ),
                        };
                        self.push_cmd(Box::new(AddItem::new(item)));
                        self.flash(t(self.lang, T::FlashTextCreated).to_string());
                    }
                }
                Some(id) => {
                    // 编辑模式：空内容不修改原 item（避免误删）；非空且变化push EditTextContent
                    if !edit.buffer.trim().is_empty() {
                        let old_content = self.scene.get_item(&id).and_then(|item| {
                            if let ItemKind::Text { content, .. } = &item.kind {
                                Some(content.clone())
                            } else {
                                None
                            }
                        });
                        if let Some(old) = old_content {
                            if old != edit.buffer {
                                let cmd = EditTextContent::new(id, old, edit.buffer);
                                self.push_cmd(Box::new(cmd));
                                self.flash(t(self.lang, T::FlashTextUpdated).to_string());
                            }
                        }
                    }
                }
            }
            self.editing_text = None;
            return;
        }
        self.editing_text = Some(edit);
    }

    /// 画框编号编辑小窗（Phase D）。点击角标触发；Enter 提交（数字 only），
    /// 冲突时经 [`Scene::plan_frame_renumber`] 自动顺移，走 undo 命令。
    pub(crate) fn render_frame_number_editor(&mut self, ctx: &egui::Context) {
        let Some(frame_id) = self.editing_frame_number else {
            return;
        };
        let Some(frame) = self.scene.get_item(&frame_id).filter(|it| it.is_frame()) else {
            self.editing_frame_number = None;
            return;
        };
        let old_number = frame.frame_number().unwrap_or(1);
        let sr = self.viewport.canvas_rect_to_egui(frame.bounding_rect());
        let mut commit = false;
        let mut cancel = false;

        let mut buf = self.frame_number_buf.clone();
        egui::Area::new(egui::Id::new("frame_number_edit_area"))
            .order(egui::Order::Foreground)
            .fixed_pos(sr.min)
            .show(ctx, |ui| {
                // 弹层背景跟随主题（D2）：去掉硬编码暗色 fill，用 Frame::popup 的主题默认。
                egui::Frame::popup(ui.style())
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        egui::Color32::from_rgb(100, 200, 255),
                    ))
                    .show(ui, |ui| {
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut buf)
                                .desired_width(40.0)
                                .hint_text("#")
                                .char_limit(6)
                                .font(egui::FontId::proportional(10.0)),
                        );
                        resp.request_focus();
                        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if esc {
                            cancel = true;
                        } else if enter || resp.lost_focus() {
                            commit = true;
                        }
                    });
            });
        self.frame_number_buf = buf;

        if cancel {
            self.editing_frame_number = None;
            return;
        }
        if !commit {
            return;
        }
        self.editing_frame_number = None;
        let Some(new_number) = self
            .frame_number_buf
            .trim()
            .parse::<u32>()
            .ok()
            .map(|n| n.max(1))
        else {
            return;
        };
        if new_number != old_number {
            let plan = self.scene.plan_frame_renumber(frame_id, new_number);
            let cmd = RenumberFrame::new(plan);
            self.push_cmd(Box::new(cmd));
            self.flash(fill(
                t(self.lang, T::FlashFrameRenumbered),
                &[new_number.to_string()],
            ));
        }
    }
}
