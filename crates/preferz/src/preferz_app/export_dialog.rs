//! export_dialog — 导出选区对话框（plan：Excalidraw 同款 `Ctrl+Shift+E`）。
//!
//! 流程：快捷键捕获选区 id → 弹窗实时预览（临时视口 + 复用 `draw_item_visual`）
//! → 透明背景开关 → 导出 PNG（离屏栅格化，后台线程）/ SVG（矢量生成，同步）/
//! 复制到剪贴板（光栅化后 UI 线程写 arboard）。无选中时快捷键只弹提示不弹窗。

use super::*;
use preferz_core::spaces::{ScreenPoint, ScreenRect};
use std::path::PathBuf;

/// 导出分辨率 = 画布尺寸 × 2（Excalidraw 选区导出同款 @2x）。
const EXPORT_SCALE: f32 = 2.0;
/// 选区包围盒外边距（画布像素），防边缘裁切。
const SELECTION_PADDING: f32 = 8.0;
/// 预览区最大尺寸（屏幕像素）。
const PREVIEW_MAX_W: f32 = 480.0;
const PREVIEW_MAX_H: f32 = 320.0;
/// 预览棋盘格底纹的格子边长（屏幕像素）。
const CHECKER_CELL: f32 = 8.0;

/// 后台光栅化 → 剪贴板位图（UI 线程收货后写 arboard，见 poll_background）。
pub(crate) struct ClipboardImageOutcome {
    pub(crate) result: Result<(Vec<u8>, u32, u32), String>,
}

/// 导出选区对话框状态。
#[derive(Default)]
pub(crate) struct ExportDialogState {
    /// 对话框是否打开。
    pub(crate) open: bool,
    /// 背景透明（默认开，Excalidraw 同款）。
    pub(crate) transparent: bool,
    /// 打开对话框时捕获的选中项 id（渲染/导出按这些 id 过滤，之后改选不影响）。
    pub(crate) item_ids: Vec<ItemId>,
    /// 预览区透明底纹（懒生成）。
    checker: Option<egui::TextureHandle>,
}

impl ExportDialogState {
    pub(crate) fn open_with(&mut self, scene: &Scene) {
        self.item_ids = expanded_export_ids(scene);
        self.open = true;
    }
}

/// 选区 → 导出集合：容器与绑定文本**双向联动**（同 delete/duplicate 的
/// `texts_bound_to` 语义）。绑定文本不进 `scene.selection`（它随容器联动，
/// 见 drag.rs 框选注释），直接快照 selection 会把图形上的文字漏在导出外。
///
/// - 正向：选中的封闭形状 → 带上其绑定文本（修「导出图形时绑定文字不可见」）；
/// - 反向：选中的绑定文本 → 带上其容器（Excalidraw 语义：标签随元素导出）。
pub(crate) fn expanded_export_ids(scene: &Scene) -> Vec<ItemId> {
    let base: Vec<ItemId> = scene.selection.iter().copied().collect();
    let mut ids = base.clone();
    for id in &base {
        ids.extend(scene.texts_bound_to(*id));
        if let Some(ItemKind::Text {
            container_id: Some(cid),
            ..
        }) = scene.get_item(id).map(|it| &it.kind)
        {
            ids.push(*cid);
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

impl PReferZApp {
    /// 打开导出选区对话框（快捷键入口；调用方已保证选区非空）。
    pub(crate) fn open_export_selection_dialog(&mut self) {
        self.export_dialog.open_with(&self.scene);
    }

    /// 当前仍存在的待导出项（Z 序，底层在前）。
    fn export_selection_items(&self) -> Vec<Item> {
        self.scene
            .items_by_z_order()
            .into_iter()
            .filter(|it| self.export_dialog.item_ids.contains(&it.id))
            .cloned()
            .collect()
    }

    /// 选区画布包围盒（含边距；纯文本 item 的包围盒可能退化，钳最小 1px）。
    fn selection_region_for(items: &[Item]) -> Option<CanvasRect> {
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for item in items {
            let bbox = item.bounding_rect();
            min_x = min_x.min(bbox.min().x);
            min_y = min_y.min(bbox.min().y);
            max_x = max_x.max(bbox.max().x);
            max_y = max_y.max(bbox.max().y);
        }
        if min_x > max_x || min_y > max_y {
            return None;
        }
        let region = CanvasRect::new(
            CanvasPoint::new(min_x - SELECTION_PADDING, min_y - SELECTION_PADDING),
            euclid::Size2D::new(
                (max_x - min_x + 2.0 * SELECTION_PADDING).max(1.0),
                (max_y - min_y + 2.0 * SELECTION_PADDING).max(1.0),
            ),
        );
        Some(region)
    }

    /// 渲染导出选区对话框（每帧调用）。
    pub(crate) fn render_export_selection_dialog(&mut self, ctx: &egui::Context) {
        if !self.export_dialog.open {
            return;
        }
        let items = self.export_selection_items();
        if items.is_empty() {
            self.export_dialog.open = false;
            return;
        }
        let Some(region) = Self::selection_region_for(&items) else {
            self.export_dialog.open = false;
            return;
        };

        let lang = self.lang;
        let transparent = self.export_dialog.transparent;
        let mut open = self.export_dialog.open;
        egui::Window::new(t(lang, T::ExportSelectionDialog))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                // ── 预览区 ──
                let fit = (PREVIEW_MAX_W / region.width())
                    .min(PREVIEW_MAX_H / region.height())
                    .clamp(0.01, 8.0);
                let size = egui::vec2(region.width() * fit, region.height() * fit);
                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                if transparent {
                    self.draw_checkerboard(ui, rect);
                } else {
                    ui.painter().rect_filled(rect, 0.0, egui::Color32::WHITE);
                }
                self.draw_preview_items(ctx, ui, rect, &items, region, fit);

                // ── 尺寸信息 ──
                let w = region.width().max(1.0).floor() as i64;
                let h = region.height().max(1.0).floor() as i64;
                ui.label(fill(
                    t(lang, T::ExportSizeLabel),
                    &[
                        w.to_string(),
                        h.to_string(),
                        ((w as f32 * EXPORT_SCALE) as i64).to_string(),
                        ((h as f32 * EXPORT_SCALE) as i64).to_string(),
                    ],
                ));

                ui.checkbox(
                    &mut self.export_dialog.transparent,
                    t(lang, T::ExportTransparentBg),
                );

                // ── 操作按钮 ──
                ui.horizontal(|ui| {
                    if ui.button(t(lang, T::ExportToPng)).clicked() {
                        let picked = rfd::FileDialog::new()
                            .add_filter("PNG", &["png"])
                            .set_file_name("selection.png")
                            .save_file();
                        if let Some(mut path) = picked {
                            path.set_extension("png");
                            self.start_selection_raster_export(ctx, Some(path));
                        }
                    }
                    if ui.button(t(lang, T::ExportToSvg)).clicked() {
                        let picked = rfd::FileDialog::new()
                            .add_filter("SVG", &["svg"])
                            .set_file_name("selection.svg")
                            .save_file();
                        if let Some(mut path) = picked {
                            path.set_extension("svg");
                            self.export_selection_svg(ctx, path);
                        }
                    }
                    if ui.button(t(lang, T::ExportCopyToClipboard)).clicked() {
                        self.start_selection_raster_export(ctx, None);
                    }
                });
            });
        self.export_dialog.open = open;
    }

    /// 预览绘制：临时视口把选区映射到预览矩形，复用 `draw_item_visual`。
    fn draw_preview_items(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        items: &[Item],
        region: CanvasRect,
        fit: f32,
    ) {
        self.ensure_pixmap_textures(ctx);
        self.ensure_grayscale_textures(ctx);

        let mut vp = ViewportState::default();
        vp.zoom = fit;
        vp.pan = region.center().to_vector();
        vp.set_screen_rect(ScreenRect::new(
            ScreenPoint::new(rect.min.x, rect.min.y),
            euclid::Size2D::new(rect.width(), rect.height()),
        ));
        let old = std::mem::replace(&mut self.viewport, vp);
        let mut child = ui.new_child(egui::UiBuilder {
            max_rect: Some(rect),
            ..egui::UiBuilder::default()
        });
        child.set_clip_rect(rect);
        for item in items {
            self.draw_item_visual(&mut child, item, None);
        }
        self.viewport = old;
    }

    /// 透明底纹：8px 白/浅灰棋盘格（纹理懒生成，mesh 平铺）。
    fn draw_checkerboard(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        let tex = self.export_dialog.checker.get_or_insert_with(|| {
            let cell = CHECKER_CELL as usize;
            let side = cell * 2;
            let mut px = Vec::with_capacity(side * side * 4);
            for y in 0..side {
                for x in 0..side {
                    let v = if ((x / cell) + (y / cell)).is_multiple_of(2) {
                        255
                    } else {
                        204
                    };
                    px.extend([v, v, v, 255]);
                }
            }
            let img = egui::ColorImage::from_rgba_unmultiplied([side, side], &px);
            ui.ctx()
                .load_texture("export_checker", img, egui::TextureOptions::default())
        });
        // 每格一个 quad（2×2 格纹理平铺）
        let cols = (rect.width() / (CHECKER_CELL * 2.0)).ceil().max(1.0) as i32;
        let rows = (rect.height() / (CHECKER_CELL * 2.0)).ceil().max(1.0) as i32;
        let mut mesh = egui::epaint::Mesh {
            texture_id: tex.id(),
            ..Default::default()
        };
        for iy in 0..rows {
            for ix in 0..cols {
                let x0 = rect.min.x + ix as f32 * CHECKER_CELL * 2.0;
                let y0 = rect.min.y + iy as f32 * CHECKER_CELL * 2.0;
                let x1 = (x0 + CHECKER_CELL * 2.0).min(rect.max.x);
                let y1 = (y0 + CHECKER_CELL * 2.0).min(rect.max.y);
                if x1 <= x0 || y1 <= y0 {
                    continue;
                }
                let uv0 = egui::pos2(0.0, 0.0);
                let uv1 = egui::pos2(1.0, 1.0);
                let white = egui::Color32::WHITE;
                let i = mesh.vertices.len() as u32;
                mesh.vertices.extend([
                    egui::epaint::Vertex {
                        pos: egui::pos2(x0, y0),
                        uv: uv0,
                        color: white,
                    },
                    egui::epaint::Vertex {
                        pos: egui::pos2(x1, y0),
                        uv: egui::pos2(uv1.x, uv0.y),
                        color: white,
                    },
                    egui::epaint::Vertex {
                        pos: egui::pos2(x1, y1),
                        uv: uv1,
                        color: white,
                    },
                    egui::epaint::Vertex {
                        pos: egui::pos2(x0, y1),
                        uv: egui::pos2(uv0.x, uv1.y),
                        color: white,
                    },
                ]);
                mesh.indices.extend([i, i + 1, i + 2, i, i + 2, i + 3]);
            }
        }
        ui.painter().add(egui::Shape::mesh(mesh));
    }

    /// 离屏收集 + tessellate + 后台光栅化。
    ///
    /// `Some(path)` → 编码 PNG 写文件（复用 ExportOutcome 通道）；`None` → 光栅
    /// 结果送回 UI 线程写系统剪贴板（`clipboard_rx`）。
    fn start_selection_raster_export(&mut self, ctx: &egui::Context, target: Option<PathBuf>) {
        let items = self.export_selection_items();
        let Some(region) = Self::selection_region_for(&items) else {
            return;
        };
        let transparent = self.export_dialog.transparent;
        let ids = self.export_dialog.item_ids.clone();
        let mut scene = self.collect_offscreen_scene(ctx, &ids, region, EXPORT_SCALE);
        Self::attach_font_atlas(&mut scene);
        let out_w = scene.rect.width().ceil() as usize;
        let out_h = scene.rect.height().ceil() as usize;
        let prims = scene.ctx.tessellate(scene.flat_shapes(), 1.0);
        let textures = scene.textures;
        let lang = self.lang;

        match target {
            Some(path) => {
                let (tx, rx) = mpsc::channel();
                self.bg_ops.export_rx = Some(rx);
                self.bg_ops.pending += 1;
                self.bg_ops.msg = Some(format!(
                    "{}: {}",
                    t(lang, T::FlashExportProgress),
                    path.display()
                ));
                let ctx2 = ctx.clone();
                std::thread::spawn(move || {
                    let result =
                        rasterize_primitives(&prims, &textures, out_w, out_h, !transparent)
                            .and_then(|rgba| encode_png(&rgba, out_w, out_h))
                            .and_then(|bytes| {
                                std::fs::write(&path, bytes).map_err(|e| e.to_string())
                            })
                            .map(|_| String::new());
                    let _ = tx.send(ExportOutcome { path, result });
                    ctx2.request_repaint();
                });
            }
            None => {
                let (tx, rx) = mpsc::channel();
                self.bg_ops.clipboard_rx = Some(rx);
                self.bg_ops.pending += 1;
                self.bg_ops.msg = Some(t(lang, T::FlashProcessing).to_string());
                let ctx2 = ctx.clone();
                std::thread::spawn(move || {
                    let result =
                        rasterize_primitives(&prims, &textures, out_w, out_h, !transparent)
                            .map(|rgba| (rgba, out_w as u32, out_h as u32));
                    let _ = tx.send(ClipboardImageOutcome { result });
                    ctx2.request_repaint();
                });
            }
        }
    }

    /// 生成选区 SVG 并写文件（同步：形状收集 + 字符串拼接均为毫秒级）。
    fn export_selection_svg(&mut self, ctx: &egui::Context, path: PathBuf) {
        let items = self.export_selection_items();
        let Some(region) = Self::selection_region_for(&items) else {
            return;
        };
        let ids = self.export_dialog.item_ids.clone();
        let scene = self.collect_offscreen_scene(ctx, &ids, region, 1.0);
        // Pixmap 内嵌用原始字节（无需重编码）
        let mut image_bytes: HashMap<u64, Vec<u8>> = HashMap::new();
        for item in &scene.items {
            if let ItemKind::Pixmap { texture_id, .. } = &item.kind {
                if let Some(bytes) = self.image_data_cache.get(texture_id) {
                    image_bytes.insert(*texture_id, bytes.clone());
                }
            }
        }
        let transparent = self.export_dialog.transparent;
        let svg = svg_export::build_selection_svg(
            &scene,
            &scene.items,
            region,
            transparent,
            &image_bytes,
        );
        match std::fs::write(&path, svg) {
            Ok(()) => self.flash(fill(
                t(self.lang, T::FlashExportedTo),
                &[path.display().to_string()],
            )),
            Err(e) => self.flash(format!("{}: {}", t(self.lang, T::FlashExportFailed), e)),
        }
    }
}

/// RGBA8 直通像素 → PNG 字节。
fn encode_png(rgba: &[u8], w: usize, h: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut out);
    image::ImageEncoder::write_image(
        encoder,
        rgba,
        w as u32,
        h as u32,
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use preferz_core::shape::DashStyle;

    fn shape_at(x: f32, y: f32) -> Item {
        Item::new_shape(
            ShapeType::Rectangle,
            (100.0, 60.0),
            x,
            y,
            StrokeStyle {
                color: [30, 30, 30, 255],
                width: 2.0,
                dash: DashStyle::Solid,
            },
            None,
        )
    }

    /// 选区 → 导出集合必须双向联动：选中形状带上绑定文本、选中绑定文本带上容器。
    /// 绑定文本不进 scene.selection（随容器联动），直接快照 selection 会漏文字。
    #[test]
    fn export_ids_expand_bound_text_both_directions() {
        let shape = shape_at(0.0, 0.0);
        let shape_id = shape.id;
        let text = Item::new_text_in("标签".into(), 50.0, 30.0, 16.0, [0, 0, 0, 255], shape_id);
        let text_id = text.id;
        let free = Item::new_text("自由".into(), 200.0, 200.0, 16.0, [0, 0, 0, 255]);
        let free_id = free.id;
        let mut scene = Scene::new();
        scene.add_item(shape);
        scene.add_item(text);
        scene.add_item(free);

        // 选中形状 → 绑定文本随行，自由文本不受牵连
        scene.selection.insert(shape_id);
        let ids = expanded_export_ids(&scene);
        assert!(ids.contains(&shape_id));
        assert!(ids.contains(&text_id), "选中形状应连带绑定文本");
        assert!(!ids.contains(&free_id));

        // 选中绑定文本 → 容器随行
        scene.selection.clear();
        scene.selection.insert(text_id);
        let ids = expanded_export_ids(&scene);
        assert!(ids.contains(&text_id));
        assert!(ids.contains(&shape_id), "选中绑定文本应连带其容器");

        // 选中自由文本 → 不联动任何 item
        scene.selection.clear();
        scene.selection.insert(free_id);
        let ids = expanded_export_ids(&scene);
        assert_eq!(ids, vec![free_id]);
    }
}
