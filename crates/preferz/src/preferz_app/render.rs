//! render — 从 preferz_app 拆出的视图/交互方法段（架构整固 Step 3b）。
//! 方法体逐字搬迁；`use super::*;` 取 mod.rs 的模块级词汇与私有项（子模块可访问父模块私有）。
use super::*;

impl PReferZApp {
    /// 绘制 Text item：可选背景色 + 文字。Edit 与 Present 两条渲染路径共用。
    ///
    /// **背景默认全透明**：只有 item 显式设置了 `ItemKind::Text::background` 才画底色
    /// （为将来侧栏"文字背景"开关预留）。背景矩形取自**本次实际排版结果**——
    /// 自由文本跟随自身 transform，绑定文本跟随容器矩形，
    /// 故容器移动 / 缩放 / 换行时背景与文字始终同步，不会各自错位。
    pub(crate) fn draw_text_item(
        &self,
        ui: &mut egui::Ui,
        item: &Item,
        editing_id: Option<ItemId>,
    ) {
        let ItemKind::Text {
            content,
            font_size,
            color,
            container_id,
            background,
            align_h,
            align_v,
            font_family,
            ..
        } = &item.kind
        else {
            return;
        };
        // 编辑期间内容由 overlay 的 TextEdit 接管，此处不画，避免原文字与编辑框重影
        if editing_id == Some(item.id) {
            return;
        }

        let text_color =
            egui::Color32::from_rgba_premultiplied(color[0], color[1], color[2], color[3]);
        let zoom = self.viewport.zoom;
        // 字体族：Handwriting → 内嵌 851远星夜行手写体；Normal → Proportional（思源黑体）
        let make_font_id = |size: f32| {
            if *font_family == FontFamily::Handwriting {
                egui::FontId::new(size, egui::FontFamily::Name(HANDWRITING_FONT_FAMILY.into()))
            } else {
                egui::FontId::proportional(size)
            }
        };

        // 统一产出 (字形 galley, 绝对屏幕位置) 列表 + 文字内容包围矩形。
        let (pieces, content_rect) =
            match container_id.filter(|cid| self.scene.get_item(cid).is_some()) {
                // 绑定文本：换行到容器宽度，按对齐枚举定位（plan #1；默认居中 = 历史行为）
                Some(cid) => {
                    let container = self.scene.get_item(&cid).expect("filter 已保证存在");
                    let cr = self.viewport.canvas_rect_to_egui(container.bounding_rect());
                    let wrap = (cr.width() - 12.0).max(20.0);
                    let pad = 6.0 * zoom;
                    let galley = ui.ctx().fonts_mut(|f| {
                        f.layout_job(egui::text::LayoutJob::simple(
                            content.clone(),
                            make_font_id(*font_size * zoom),
                            text_color,
                            wrap,
                        ))
                    });
                    let size = galley.size();
                    let dx = match align_h {
                        TextAlignH::Left => pad,
                        TextAlignH::Center => (cr.width() - size.x) * 0.5,
                        TextAlignH::Right => cr.width() - size.x - pad,
                    };
                    let dy = match align_v {
                        TextAlignV::Top => pad,
                        TextAlignV::Middle => (cr.height() - size.y) * 0.5,
                        TextAlignV::Bottom => cr.height() - size.y - pad,
                    };
                    let top_left = cr.min + egui::vec2(dx, dy);
                    (
                        vec![(galley, top_left)],
                        egui::Rect::from_min_size(top_left, size),
                    )
                }
                // 自由文本（或容器已丢失）：按自身 transform 定位，字号叠加 scale；
                // 对齐枚举不生效（单行无框，恒 top-left）
                None => {
                    let origin = self.viewport.canvas_to_pos2(item.canvas_corners()[0]);
                    // scale.x（等比缩放场景下 scale.y 相同；非等比 egui text 不支持非均匀缩放）
                    let effective_font_size = *font_size * item.transform.scale.x.abs() * zoom;
                    let galley = ui.ctx().fonts_mut(|f| {
                        f.layout_no_wrap(
                            content.clone(),
                            make_font_id(effective_font_size),
                            text_color,
                        )
                    });
                    let size = galley.size();
                    (
                        vec![(galley, origin)],
                        egui::Rect::from_min_size(origin, size),
                    )
                }
            };

        if let Some(bg) = background {
            let pad = 4.0 * zoom;
            ui.painter().rect_filled(
                content_rect.expand(pad),
                egui::CornerRadius::same(2),
                egui::Color32::from_rgba_unmultiplied(bg[0], bg[1], bg[2], bg[3]),
            );
        }
        for (galley, pos) in pieces {
            ui.painter().galley(pos, galley, text_color);
        }
    }

    /// 绘制单个 item 的视觉内容（不含选中手柄 / 多选外框 / 裁剪 overlay）。
    /// Edit 与 Present 模式共用的渲染原语，保证两处观感一致。
    pub(crate) fn draw_item_visual(
        &self,
        ui: &mut egui::Ui,
        item: &Item,
        editing_id: Option<ItemId>,
    ) {
        let canvas_bbox = item.bounding_rect();
        let item_screen_rect = self.viewport.canvas_rect_to_egui(canvas_bbox);
        let corners = item.canvas_corners();
        let screen_corners = [
            self.viewport.canvas_to_pos2(corners[0]),
            self.viewport.canvas_to_pos2(corners[1]),
            self.viewport.canvas_to_pos2(corners[2]),
            self.viewport.canvas_to_pos2(corners[3]),
        ];
        match &item.kind {
            ItemKind::Pixmap {
                texture_id,
                opacity,
                grayscale,
                crop,
                ..
            } => {
                let tex_id = *texture_id;
                let handle = if *grayscale {
                    self.grayscale_texture_cache
                        .get(&tex_id)
                        .or_else(|| self.texture_cache.get(&tex_id))
                } else {
                    self.texture_cache.get(&tex_id)
                };
                if let Some(handle) = handle {
                    let (u_min, u_max, v_min, v_max) = if let Some(c) = crop {
                        let base_w = item.base_size().x.max(1.0);
                        let base_h = item.base_size().y.max(1.0);
                        let cx0 = (c.x / base_w).clamp(0.0, 1.0);
                        let cx1 = ((c.x + c.width) / base_w).clamp(0.0, 1.0);
                        let cy0 = (c.y / base_h).clamp(0.0, 1.0);
                        let cy1 = ((c.y + c.height) / base_h).clamp(0.0, 1.0);
                        (cx0, cx1, cy0, cy1)
                    } else {
                        (0.0, 1.0, 0.0, 1.0)
                    };
                    let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
                    let tint = pixmap_tint(alpha);
                    let [tl, tr, bl, br] = screen_corners;
                    let verts = [
                        ([tl.x, tl.y], [u_min, v_min]),
                        ([tr.x, tr.y], [u_max, v_min]),
                        ([br.x, br.y], [u_max, v_max]),
                        ([bl.x, bl.y], [u_min, v_max]),
                    ];
                    let mut mesh = egui::epaint::Mesh {
                        texture_id: handle.id(),
                        ..Default::default()
                    };
                    for ([px, py], [u, v]) in verts {
                        mesh.vertices.push(egui::epaint::Vertex {
                            pos: [px, py].into(),
                            uv: [u, v].into(),
                            color: tint,
                        });
                    }
                    mesh.indices = vec![0, 1, 2, 0, 2, 3];
                    ui.painter().add(egui::epaint::Shape::mesh(mesh));
                } else {
                    ui.painter().rect_filled(
                        item_screen_rect,
                        egui::CornerRadius::same(0),
                        egui::Color32::from_rgb(70, 70, 70),
                    );
                }
            }
            // 文字：可选背景 + 排版绘制，两处渲染路径共用 draw_text_item
            ItemKind::Text { .. } => self.draw_text_item(ui, item, editing_id),
            // 风格器分发（CleanStyler / RoughStyler）在 build_shape_visuals 内按 sloppiness 档位决定。
            ItemKind::Shape { .. } => {
                let to_screen = item_local_to_screen(item, &self.viewport);
                let shapes = build_shape_visuals(&item.kind, &to_screen, self.viewport.zoom);
                ui.painter().extend(shapes);
                self.draw_edge_label(ui, item);
            }
            // 墨迹（plan #10）：速度锥形 ribbon 填充轮廓。
            ItemKind::Freedraw { .. } => {
                let to_screen = item_local_to_screen(item, &self.viewport);
                let shapes = build_freedraw_visuals(&item.kind, &to_screen);
                ui.painter().extend(shapes);
            }
            ItemKind::Frame { .. } => {
                let sr = self.viewport.canvas_rect_to_egui(item.bounding_rect());
                let border = egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(90, 90, 95));
                ui.painter()
                    .rect_stroke(sr, 0.0, border, egui::StrokeKind::Middle);
            }
            // 图表（plan #8）：矢量逐段绘制（坐标轴 + 柱/折线 + 文字标签）
            ItemKind::Chart { .. } => self.draw_chart_item(ui, item),
        }
    }
    // ─────────────────────────── 渲染 ───────────────────────────
    /// 图表（plan #8）矢量渲染：横向网格 + 数值刻度 + 左/下坐标轴 + 柱或
    /// 折线 + 底部类别标签。几何经 `to_screen` 变换（支持移动/缩放/旋转；
    /// 文字不随 item 旋转——首轮取舍，标签保持水平可读）。支持负值：
    /// 零线随数据范围浮动，柱从零线画到数值处。
    fn draw_chart_item(&self, ui: &mut egui::Ui, item: &Item) {
        let ItemKind::Chart {
            chart_type,
            labels,
            values,
            color,
            stroke_width,
            ..
        } = &item.kind
        else {
            return;
        };
        let n = values.len();
        if n == 0 {
            return;
        }
        let to_screen = item_local_to_screen(item, &self.viewport);
        // 屏幕像素 / 局部单位（含 item scale + 视口 zoom），用于线宽与字号。
        let unit =
            to_screen.transform_vector(euclid::Vector2D::<f32, ItemLocalSpace>::new(1.0, 0.0));
        let scale = unit.length().max(1e-4);

        let dark = ui.visuals().dark_mode;
        let axis_col = if dark {
            egui::Color32::from_rgb(0x9a, 0x9a, 0x9a)
        } else {
            egui::Color32::from_rgb(0x33, 0x33, 0x33)
        };
        let grid_col = if dark {
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 28)
        } else {
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 28)
        };
        let label_col = if dark {
            egui::Color32::from_rgb(0xaa, 0xaa, 0xaa)
        } else {
            egui::Color32::from_rgb(0x44, 0x44, 0x44)
        };
        let series = egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);

        let painter = ui.painter();
        // 局部 → 屏幕点
        let p = |x: f32, y: f32| -> egui::Pos2 {
            let q = to_screen.transform_point(euclid::point2(x, y));
            egui::Pos2::new(q.x, q.y)
        };

        let size = item.base_size();
        // 内边距（局部单位）：左侧留给数值刻度、底部留给类别标签。
        let (ml, mr, mt, mb) = (48.0_f32, 12.0_f32, 12.0_f32, 28.0_f32);
        let plot_x0 = ml;
        let plot_y0 = mt;
        let plot_x1 = (size.x - mr).max(ml + 1.0);
        let plot_y1 = (size.y - mb).max(mt + 1.0);
        let plot_w = plot_x1 - plot_x0;
        let plot_h = plot_y1 - plot_y0;

        let vmax = values.iter().cloned().fold(0.0_f32, f32::max);
        let vmin = values.iter().cloned().fold(0.0_f32, f32::min);
        let (lo, hi) = if (vmax - vmin).abs() < f32::EPSILON {
            (-1.0, 1.0)
        } else {
            (vmin, vmax)
        };
        let y_of = |val: f32| plot_y1 - (val - lo) / (hi - lo) * plot_h;

        // 横向网格线 + 左侧数值刻度（4 等分；网格先画，垫在柱/折线之下）
        for k in 0..=4 {
            let t = k as f32 / 4.0;
            let val = hi - t * (hi - lo);
            let ly = plot_y1 - t * plot_h;
            painter.line_segment(
                [p(plot_x0, ly), p(plot_x1, ly)],
                egui::Stroke::new(scale, grid_col),
            );
            let tick = if (val.fract()).abs() < 1e-6 {
                format!("{}", val as i64)
            } else {
                format!("{val:.1}")
            };
            let galley =
                painter.layout_no_wrap(tick, egui::FontId::proportional(10.0 * scale), label_col);
            let pos = p(plot_x0 - 6.0, ly) - egui::vec2(galley.size().x, galley.size().y * 0.5);
            painter.galley(pos, galley, label_col);
        }

        // 坐标轴（左 + 下）
        let axis_stroke = egui::Stroke::new(1.2 * scale, axis_col);
        painter.line_segment([p(plot_x0, plot_y0), p(plot_x0, plot_y1)], axis_stroke);
        painter.line_segment([p(plot_x0, plot_y1), p(plot_x1, plot_y1)], axis_stroke);

        let slot_w = plot_w / n as f32;
        let center_x = |i: usize| plot_x0 + (i as f32 + 0.5) * slot_w;

        // 底部类别标签（居中于各槽；空标签跳过）
        for (i, lb) in labels.iter().take(n).enumerate() {
            if lb.is_empty() {
                continue;
            }
            let galley = painter.layout_no_wrap(
                lb.clone(),
                egui::FontId::proportional(11.0 * scale),
                label_col,
            );
            let pos = p(center_x(i), plot_y1 + 6.0) - egui::vec2(galley.size().x * 0.5, 0.0);
            painter.galley(pos, galley, label_col);
        }

        let zero_y = y_of(0.0);
        match chart_type {
            preferz_core::ChartType::Bar => {
                let bar_w = slot_w * 0.6;
                for (i, val) in values.iter().enumerate() {
                    if *val == 0.0 {
                        continue;
                    }
                    let cx = center_x(i);
                    let yv = y_of(*val);
                    let corners = [
                        p(cx - bar_w * 0.5, zero_y),
                        p(cx + bar_w * 0.5, zero_y),
                        p(cx + bar_w * 0.5, yv),
                        p(cx - bar_w * 0.5, yv),
                    ];
                    painter.add(egui::Shape::convex_polygon(
                        corners.to_vec(),
                        series,
                        egui::Stroke::NONE,
                    ));
                }
            }
            preferz_core::ChartType::Line => {
                let pts: Vec<egui::Pos2> =
                    (0..n).map(|i| p(center_x(i), y_of(values[i]))).collect();
                if pts.len() >= 2 {
                    painter.add(egui::Shape::line(
                        pts.clone(),
                        egui::Stroke::new(stroke_width * scale, series),
                    ));
                }
                let r = (stroke_width * scale * 1.5).max(1.5);
                for pt in &pts {
                    painter.circle_filled(*pt, r, series);
                }
            }
        }
    }

    /// 线性对象边标签（plan #17 DP2）：把 mermaid `-->|文本|` / `-- 文本 -->` 生成的
    /// 连线文字画在当前线段中点的屏幕位置，随端点重路由自动跟随。带 `label` 的
    /// Polyline / Elbow 生效（mermaid 恒生成两点直边，故中点 = 包围盒中心；路由
    /// 改道后标签仍落在原包围盒中心，属可接受的小偏差）。
    fn draw_edge_label(&self, ui: &egui::Ui, item: &Item) {
        let ItemKind::Shape {
            shape_type, label, ..
        } = &item.kind
        else {
            return;
        };
        if !matches!(shape_type, ShapeType::Polyline | ShapeType::Elbow) {
            return;
        }
        let Some(text) = label else { return };
        if text.is_empty() {
            return;
        }
        let to_screen = item_local_to_screen(item, &self.viewport);
        let scale = to_screen
            .transform_vector(euclid::Vector2D::<f32, ItemLocalSpace>::new(1.0, 0.0))
            .length()
            .max(1e-4);
        let center = self.viewport.canvas_to_pos2(item.bounding_rect().center());
        let text_col = ui.visuals().text_color();
        let galley = ui.painter().layout_no_wrap(
            text.clone(),
            egui::FontId::proportional(15.0 * scale),
            text_col,
        );
        let pos = center - galley.size() * 0.5;
        ui.painter().galley(pos, galley, text_col);
    }

    pub(crate) fn render_scene(&mut self, ui: &mut egui::Ui) {
        let screen_rect = ui.max_rect();

        // 空场景：渲染欢迎页（spec §2.3 欢迎页 + 最近文件列表）
        if self.scene.items.is_empty() {
            self.render_welcome_page(ui, &screen_rect);
            return;
        }

        // 先测量所有 Text item 的实际尺寸，更新 measured_size（修 B6：边框与渲染一致）。
        // measured_size 为 None（新建/编辑/undo/redo 后）才重新测量，避免每帧重复计算。
        self.update_text_measured_sizes(ui.ctx());

        // 预生成所grayscale=true Pixmap 灰度纹理（避免渲染循环里 &mut self &self.scene 冲突
        self.ensure_grayscale_textures(ui.ctx());

        // Z 序渲染（W9）：底层先画，顶层后
        let items: Vec<&Item> = self.scene.items_by_z_order();
        let selection_count = self.scene.selection.len();
        let editing_id = self.editing_text.as_ref().and_then(|e| e.editing_item_id);
        let crop_item_id = self.crop_mode.as_ref().map(|c| c.item_id);
        for item in items {
            // 视口剔除（修 S9/M11）：用画AABB 转屏幕矩形，不相交则跳过
            let canvas_bbox = item.bounding_rect();
            let item_screen_rect = self.viewport.canvas_rect_to_egui(canvas_bbox);
            if !screen_rect.intersects(item_screen_rect) {
                continue;
            }

            let corners = item.canvas_corners();
            let screen_corners = [
                self.viewport.canvas_to_pos2(corners[0]),
                self.viewport.canvas_to_pos2(corners[1]),
                self.viewport.canvas_to_pos2(corners[2]),
                self.viewport.canvas_to_pos2(corners[3]),
            ];

            let is_selected = self.scene.selection_contains(&item.id);

            match &item.kind {
                ItemKind::Pixmap {
                    texture_id,
                    opacity,
                    grayscale,
                    crop,
                    ..
                } => {
                    let tex_id = *texture_id;
                    let opacity = *opacity;
                    let grayscale = *grayscale;
                    // 灰度选用灰度纹理，否则原纹理
                    let handle_opt = if grayscale {
                        self.grayscale_texture_cache
                            .get(&tex_id)
                            .or_else(|| self.texture_cache.get(&tex_id))
                    } else {
                        self.texture_cache.get(&tex_id)
                    };
                    if let Some(handle) = handle_opt {
                        // UV 计算说明                        // - flip local_to_canvas 的几何翻转实现（canvas_corners flip 后位置交换）                        //   因此 mesh quad screen_corners 即可呈现镜像，UV 不再翻转                        //   否则会与几何翻转抵消，导flip 后图片看起来不变                        // - crop 通过 UV 子矩形采样（item 局部空间，未应flip），
                        //   crop 区域的画布位置由 transform.scale 同步保证边框对齐
                        let (u_min, u_max, v_min, v_max) = if let Some(c) = crop {
                            let base_w = item.base_size().x.max(1.0);
                            let base_h = item.base_size().y.max(1.0);
                            let cx0 = (c.x / base_w).clamp(0.0, 1.0);
                            let cx1 = ((c.x + c.width) / base_w).clamp(0.0, 1.0);
                            let cy0 = (c.y / base_h).clamp(0.0, 1.0);
                            let cy1 = ((c.y + c.height) / base_h).clamp(0.0, 1.0);
                            (cx0, cx1, cy0, cy1)
                        } else {
                            (0.0, 1.0, 0.0, 1.0)
                        };
                        // 透明度：tint_color alpha = opacity（spec §2.2 透明度）
                        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
                        let tint = pixmap_tint(alpha);
                        // mesh quad 渲染，让图片真正跟着旋转/flip（screen_corners 已包含全部几何变换）
                        // screen_corners 顺序：[TL, TR, BL, BR]，重排为 [TL, TR, BR, BL] 顺时
                        let [tl, tr, bl, br] = screen_corners;
                        let verts = [
                            ([tl.x, tl.y], [u_min, v_min]),
                            ([tr.x, tr.y], [u_max, v_min]),
                            ([br.x, br.y], [u_max, v_max]),
                            ([bl.x, bl.y], [u_min, v_max]),
                        ];
                        let mut mesh = egui::epaint::Mesh {
                            texture_id: handle.id(),
                            ..Default::default()
                        };
                        for ([px, py], [u, v]) in verts {
                            mesh.vertices.push(egui::epaint::Vertex {
                                pos: [px, py].into(),
                                uv: [u, v].into(),
                                color: tint,
                            });
                        }
                        mesh.indices = vec![0, 1, 2, 0, 2, 3];
                        ui.painter().add(egui::epaint::Shape::mesh(mesh));
                    } else {
                        ui.painter().rect_filled(
                            item_screen_rect,
                            egui::CornerRadius::same(0),
                            if is_selected {
                                egui::Color32::from_rgb(80, 80, 40)
                            } else {
                                egui::Color32::from_rgb(70, 70, 70)
                            },
                        );
                    }
                }
                // 文字：可选背景 + 排版绘制，两处渲染路径共用 draw_text_item
                ItemKind::Text { .. } => self.draw_text_item(ui, item, editing_id),
                // Shape：风格器分发同 render_scene，sloppiness 档位决定 Clean 或手绘。
                ItemKind::Shape { .. } => {
                    let to_screen = item_local_to_screen(item, &self.viewport);
                    let shapes = build_shape_visuals(&item.kind, &to_screen, self.viewport.zoom);
                    ui.painter().extend(shapes);
                    self.draw_edge_label(ui, item);
                }
                // 墨迹（plan #10）：速度锥形 ribbon 填充轮廓。
                ItemKind::Freedraw { .. } => {
                    let to_screen = item_local_to_screen(item, &self.viewport);
                    let shapes = build_freedraw_visuals(&item.kind, &to_screen);
                    ui.painter().extend(shapes);
                }
                // 图表（plan #8）：矢量逐段绘制（坐标轴 + 柱/折线 + 文字标签）
                ItemKind::Chart { .. } => self.draw_chart_item(ui, item),
                // Frame：虚线边框 + 左上角编号角标 + 名称。不裁剪内容，仅作底框。
                ItemKind::Frame { number, name, .. } => {
                    // 边框矩形（画布 AABB 转屏幕：frame 不旋转，直接用 bounding_rect）。
                    let fc = item.bounding_rect();
                    let sr = self.viewport.canvas_rect_to_egui(fc);
                    let border = egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(90, 90, 95));
                    ui.painter()
                        .rect_stroke(sr, 0.0, border, egui::StrokeKind::Middle);
                    // 编号角标（左上角）
                    let label = format!("#{}", number);
                    let galley = ui.painter().layout_no_wrap(
                        label,
                        egui::FontId::proportional(10.0),
                        egui::Color32::WHITE,
                    );
                    let badge_size = galley.size() + egui::vec2(8.0, 4.0);
                    let badge_rect = egui::Rect::from_min_size(sr.min, badge_size);
                    ui.painter().rect_filled(
                        badge_rect,
                        egui::CornerRadius::same(3),
                        egui::Color32::from_rgba_unmultiplied(70, 110, 200, 255),
                    );
                    ui.painter().galley(
                        badge_rect.min + egui::vec2(4.0, 2.0),
                        galley,
                        egui::Color32::WHITE,
                    );
                    // 点击角标 → 进入编号编辑（Phase D：编号冲突自动顺移）。
                    let badge_id = egui::Id::new(("frame_badge", item.id));
                    if ui
                        .interact(badge_rect, badge_id, egui::Sense::click())
                        .clicked()
                        && self.editing_frame_number != Some(item.id)
                    {
                        self.editing_frame_number = Some(item.id);
                        self.frame_number_buf = number.to_string();
                    }
                    // 名称（角标右侧）
                    if let Some(nm) = name {
                        if !nm.is_empty() {
                            let ng = ui.painter().layout_no_wrap(
                                nm.clone(),
                                egui::FontId::proportional(12.0),
                                egui::Color32::from_rgb(160, 170, 180),
                            );
                            ui.painter().galley(
                                egui::pos2(sr.min.x + badge_size.x + 6.0, sr.min.y + 2.0),
                                ng,
                                egui::Color32::from_rgb(160, 170, 180),
                            );
                        }
                    }
                }
            }

            // 选中+ 手柄：单选时画单独手柄；多选时画统一外框（循环后            // 裁剪模式下手柄隐藏（避免与裁剪框冲突
            if is_selected && selection_count == 1 && crop_item_id != Some(item.id) {
                let show_flip = should_show_flip(item);
                let show_rotate = should_show_rotate(item);
                self.transform_handles.render(
                    item,
                    ui.painter(),
                    &self.viewport,
                    show_flip,
                    show_rotate,
                );
            }
        }

        // 多选统一外框（spec L241：多选时画一个统一 bbox
        if selection_count > 1 {
            if let Some(bbox) = self.scene.selection_bounding_rect() {
                let screen_bbox = self.viewport.canvas_rect_to_egui(bbox);
                let stroke = egui::Stroke::new(1.5_f32, egui::Color32::YELLOW);
                ui.painter()
                    .rect_stroke(screen_bbox, 0.0, stroke, egui::StrokeKind::Middle);
                // 4 角小方块标识
                let handle_size = TransformHandles::handle_size();
                let fill = egui::Color32::YELLOW;
                for p in [
                    screen_bbox.min,
                    egui::pos2(screen_bbox.max.x, screen_bbox.min.y),
                    egui::pos2(screen_bbox.min.x, screen_bbox.max.y),
                    screen_bbox.max,
                ] {
                    let r = egui::Rect::from_center_size(p, egui::Vec2::splat(handle_size));
                    ui.painter()
                        .rect_filled(r, egui::CornerRadius::same(1), fill);
                }
            }
        }

        // 端点吸附高亮（plan #5）：拖动线端点靠近某形状轮廓时，高亮该形状轮廓。
        // 仅在拖拽预览期间由 update_drag / update_drag_preview 设置，end_drag 时清空。
        if let Some(bid) = self.snap_highlight {
            if let Some(item) = self.scene.get_item(&bid) {
                let segs = snap::outline_segments(item);
                let stroke = egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(255, 170, 40));
                for (a, b) in segs {
                    let sa = self.viewport.canvas_to_pos2(a);
                    let sb = self.viewport.canvas_to_pos2(b);
                    ui.painter().line_segment([sa, sb], stroke);
                }
            }
        }

        // 裁剪模式 overlay（spec §2.2 裁剪
        self.render_crop_overlay(ui);
    }

    /// 懒生成所grayscale=true Pixmap 灰度纹理（spec §2.2 灰度）    /// 使用 ITU-R BT.601 亮度系数：Y = 0.299R + 0.587G + 0.114B（不引入 palette crate）
    pub(crate) fn ensure_grayscale_textures(&mut self, ctx: &egui::Context) {
        // 收集需要生成的 (texture_id, original_size) 列表
        let mut to_generate: Vec<(u64, (u32, u32))> = Vec::new();
        for item in &self.scene.items {
            if let ItemKind::Pixmap {
                texture_id,
                original_size,
                grayscale,
                ..
            } = &item.kind
            {
                if *grayscale && !self.grayscale_texture_cache.contains_key(texture_id) {
                    to_generate.push((*texture_id, *original_size));
                }
            }
        }
        for (tex_id, (w, h)) in to_generate {
            let rgba = match self.rgba_pixel_cache.get(&tex_id).cloned() {
                Some(b) => b,
                None => continue,
            };
            let mut gray = rgba;
            for chunk in gray.chunks_mut(4) {
                let r = chunk[0] as f32;
                let g = chunk[1] as f32;
                let b = chunk[2] as f32;
                let lum = (0.299 * r + 0.587 * g + 0.114 * b)
                    .round()
                    .clamp(0.0, 255.0) as u8;
                chunk[0] = lum;
                chunk[1] = lum;
                chunk[2] = lum;
            }
            let color_image =
                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &gray);
            let handle = ctx.load_texture(
                format!("img_gray_{}", tex_id),
                color_image,
                Default::default(),
            );
            self.grayscale_texture_cache.insert(tex_id, handle);
        }
    }

    /// 懒重建缺失的 Pixmap 纹理（修 .issues #1 灰方块）。
    ///
    /// 删除 Pixmap 时只驱逐 GPU 纹理句柄、保留 RGBA 像素缓存（见 `delete_selected`）：
    /// undo 恢复 item 后此处按缓存重新上传纹理；共享同一 texture_id 的副本/原件
    /// 也不会因为一次删除而集体变灰。每帧调用，缺纹理的场景通常为空，开销可忽略。
    pub(crate) fn ensure_pixmap_textures(&mut self, ctx: &egui::Context) {
        let missing: Vec<u64> = self
            .scene
            .items
            .iter()
            .filter_map(|it| match &it.kind {
                ItemKind::Pixmap { texture_id, .. } => Some(*texture_id),
                _ => None,
            })
            .filter(|tid| !self.texture_cache.contains_key(tid))
            .collect();
        for tid in missing {
            let Some(rgba) = self.rgba_pixel_cache.get(&tid).cloned() else {
                continue;
            };
            let Some((w, h)) = self.rgba_size_cache.get(&tid).copied() else {
                continue;
            };
            let color_image =
                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
            let handle = ctx.load_texture(format!("img_{}", tid), color_image, Default::default());
            self.texture_cache.insert(tid, handle);
        }
    }

    /// 渲染裁剪模式 overlay：在裁剪 item 上画可拖拽的裁剪矩形 + 4 角手+ 遮罩
    pub(crate) fn render_crop_overlay(&mut self, ui: &mut egui::Ui) {
        let crop_state = match self.crop_mode.as_ref() {
            Some(c) => c.clone(),
            None => return,
        };
        let item_id = crop_state.item_id;
        let item = match self.scene.get_item(&item_id) {
            Some(i) => i.clone(),
            None => {
                self.crop_mode = None;
                return;
            }
        };
        // Pixmap 支持裁剪
        let (texture_id, original_size) = match &item.kind {
            ItemKind::Pixmap {
                texture_id,
                original_size,
                ..
            } => (*texture_id, *original_size),
            _ => {
                self.crop_mode = None;
                return;
            }
        };
        let _ = (texture_id, original_size);

        // item 与 crop 的 4 角（屏幕空间，环形顺序 TL -> TR -> BR -> BL）。
        // 两者都完整走 item 的 local_to_canvas（含 flip/scale/rotate），因此旋转下
        // 裁剪框会跟随图片一起旋转。不能用画布 AABB 做线性插值定位——AABB 在旋转下
        // 会膨胀且不含旋转，裁剪框与图片会错位。
        let item_quad = item
            .canvas_corners_ring()
            .map(|p| self.viewport.canvas_to_pos2(p));
        let crop_quad = match item.crop_corners(crop_state.rect) {
            Some(q) => q.map(|p| self.viewport.canvas_to_pos2(p)),
            None => return,
        };

        // 遮罩（修旋转图闪烁黑影）：
        // 旧实现把「图片边→裁剪边」拆成 4 条梯形，每条边随裁剪框拖动而重排；
        // 旋转下这些对角线经 egui 抗锯齿后缝隙逐帧变化 → 影像周围闪黑。
        // 改为：① 单个凸多边形把整张图片压暗，外缘 = item_quad（拖拽时图片不动，
        //   遮罩外缘稳定不闪）；② 再把裁剪框内的图片以原始亮度重绘到 crop_quad 之上。
        // 裁剪框描边压住内部接缝，重绘的亮图与底层像素一致（同纹理同 UV），不引入新闪烁。
        let mask_color = egui::Color32::from_rgba_premultiplied(0, 0, 0, 120);
        ui.painter().add(egui::Shape::convex_polygon(
            item_quad.to_vec(),
            mask_color,
            egui::Stroke::NONE,
        ));

        // 裁剪框内图片以原始亮度重绘（需纹理句柄；缺失时退化为仅压暗整图）。
        let tex_handle = match &item.kind {
            ItemKind::Pixmap {
                texture_id,
                grayscale,
                ..
            } => {
                let tid = *texture_id;
                if *grayscale {
                    self.grayscale_texture_cache
                        .get(&tid)
                        .or_else(|| self.texture_cache.get(&tid))
                } else {
                    self.texture_cache.get(&tid)
                }
            }
            _ => None,
        };
        if let Some(handle) = tex_handle {
            // UV 必须与 crop_corners 的归一化基准（current_crop）一致，
            // 而非 base_size——否则已在裁剪过的图片会二次偏移。
            let base = item
                .current_crop()
                .unwrap_or(CropRect::new(0.0, 0.0, 1.0, 1.0));
            let r = crop_state.rect;
            let (u0, u1, v0, v1) = (
                ((r.x - base.x) / base.width).clamp(0.0, 1.0),
                ((r.x + r.width - base.x) / base.width).clamp(0.0, 1.0),
                ((r.y - base.y) / base.height).clamp(0.0, 1.0),
                ((r.y + r.height - base.y) / base.height).clamp(0.0, 1.0),
            );
            let opacity = match &item.kind {
                ItemKind::Pixmap { opacity, .. } => *opacity,
                _ => 1.0,
            };
            let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
            let tint = pixmap_tint(alpha);
            let [tl, tr, br, bl] = crop_quad;
            let verts = [
                ([tl.x, tl.y], [u0, v0]),
                ([tr.x, tr.y], [u1, v0]),
                ([br.x, br.y], [u1, v1]),
                ([bl.x, bl.y], [u0, v1]),
            ];
            let mut mesh = egui::epaint::Mesh {
                texture_id: handle.id(),
                ..Default::default()
            };
            for ([px, py], [u, v]) in verts {
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: [px, py].into(),
                    uv: [u, v].into(),
                    color: tint,
                });
            }
            mesh.indices = vec![0, 1, 2, 0, 2, 3];
            ui.painter().add(egui::epaint::Shape::mesh(mesh));
        }
        // 裁剪框：旋转四边形，用闭合折线描边而非轴对齐矩形
        let stroke = egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(100, 200, 255));
        ui.painter()
            .add(egui::Shape::closed_line(crop_quad.to_vec(), stroke));
        // 4 角手柄（落在旋转后的裁剪框角点上）
        let handle_size = TransformHandles::handle_size();
        let fill = egui::Color32::from_rgb(100, 200, 255);
        for p in crop_quad {
            let r = egui::Rect::from_center_size(p, egui::Vec2::splat(handle_size));
            ui.painter()
                .rect_filled(r, egui::CornerRadius::same(1), fill);
        }

        // 提示文字：挂在 item 屏幕包围盒左上角上方
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        for p in item_quad {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
        }
        ui.painter().text(
            egui::pos2(min_x, min_y) + egui::vec2(0.0, -18.0),
            egui::Align2::LEFT_BOTTOM,
            "裁剪模式：拖拽角点调整 · Enter 应用 · Esc 取消",
            egui::FontId::proportional(12.0),
            egui::Color32::from_rgb(100, 200, 255),
        );
    }
}
