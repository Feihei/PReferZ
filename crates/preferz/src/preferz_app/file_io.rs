//! file_io — 从 preferz_app 拆出的方法段（架构整固 Step 3c）。方法体逐字搬迁。
//! `use super::*;` 取 mod.rs 的模块级词汇与私有项（子模块可访问父模块私有）。
use super::*;

impl PReferZApp {
    pub(crate) fn finish_import(&mut self, ctx: &egui::Context, outcome: ImportOutcome) {
        if let Some(e) = outcome.error {
            self.flash(fill(
                t(self.lang, T::FlashImportFailed),
                &[outcome.path.display().to_string(), e.to_string()],
            ));
            return;
        }
        let texture_id = self.next_texture_id;
        self.next_texture_id += 1;

        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            [outcome.width as usize, outcome.height as usize],
            &outcome.rgba,
        );
        let texture_handle = ctx.load_texture(
            format!("img_{}", texture_id),
            color_image,
            Default::default(),
        );
        self.texture_cache.insert(texture_id, texture_handle);
        // 缓存原始字节，保存时写入 sqlar
        self.image_data_cache
            .insert(texture_id, outcome.bytes.clone());
        // 缓存 RGBA 像素（懒生成灰度纹理 + 颜色采样用）
        self.rgba_pixel_cache
            .insert(texture_id, outcome.rgba.clone());
        self.rgba_size_cache
            .insert(texture_id, (outcome.width, outcome.height));

        // 初始位置：视口中心对应的画布
        let center_canvas = self
            .viewport
            .screen_to_canvas(self.viewport.screen_rect.center());
        let item = Item::new_pixmap(
            texture_id,
            Some(outcome.path.to_string_lossy().to_string()),
            (outcome.width, outcome.height),
            center_canvas.x - outcome.width as f32 / 2.0,
            center_canvas.y - outcome.height as f32 / 2.0,
            1.0,
            1.0,
        );

        let cmd = AddItem::new(item);
        self.push_cmd(Box::new(cmd));
        self.flash(fill(
            t(self.lang, T::FlashImported),
            &[outcome.path.display().to_string()],
        ));
        ctx.request_repaint();
    }

    /// 后台加载完成：重scene + 上传纹理 + 重新映射 texture_id（由 BackgroundOps::poll 调用）
    pub(crate) fn finish_load(&mut self, ctx: &egui::Context, outcome: LoadOutcome) {
        match outcome.result {
            Ok((mut scene, images, viewport_meta)) => {
                // 清空当前状态（纹理/字节缓存/undo 栈）
                self.texture_cache.clear();
                self.grayscale_texture_cache.clear();
                self.image_data_cache.clear();
                self.rgba_pixel_cache.clear();
                self.rgba_size_cache.clear();
                self.undo_stack = UndoStack::new();

                // 为每Pixmap 重新分配 texture_id，解码上传纹理，重映item.texture_id
                let mut id_remap: HashMap<u64, u64> = HashMap::new();
                for item in &mut scene.items {
                    if let ItemKind::Pixmap { texture_id, .. } = &mut item.kind {
                        let old_id = *texture_id;
                        let new_id = self.next_texture_id;
                        self.next_texture_id += 1;
                        id_remap.insert(old_id, new_id);

                        let bytes = images.get(&old_id.to_string());
                        if let Some(bytes) = bytes {
                            // 解码字节上传纹理
                            match image::load_from_memory(bytes) {
                                Ok(img) => {
                                    let (w, h) = img.dimensions();
                                    let rgba = img.to_rgba8().into_vec();
                                    let color_image = egui::ColorImage::from_rgba_unmultiplied(
                                        [w as usize, h as usize],
                                        &rgba,
                                    );
                                    let handle = ctx.load_texture(
                                        format!("img_{}", new_id),
                                        color_image,
                                        Default::default(),
                                    );
                                    self.texture_cache.insert(new_id, handle);
                                    self.image_data_cache.insert(new_id, bytes.clone());
                                    self.rgba_pixel_cache.insert(new_id, rgba);
                                    self.rgba_size_cache.insert(new_id, (w, h));
                                }
                                Err(e) => {
                                    log::error!("解码图片 texture_id={} 失败: {}", old_id, e);
                                }
                            }
                        }
                        *texture_id = new_id;
                    }
                }

                // 应用视口元数据。zoom 需按当前钳制范围收敛：旧文件可能存着
                // 超出新 min/max（10%–1000%）的极端值，不 clamp 会让滚轮缩放在
                // 越界值上"空转"好几圈才有反应。
                self.viewport.pan = CanvasVector::new(viewport_meta.pan_x, viewport_meta.pan_y);
                self.viewport.zoom = viewport_meta
                    .zoom
                    .clamp(self.viewport.min_zoom, self.viewport.max_zoom);

                self.scene = scene;
                // 清理孤儿 container_id（容器已不存在则置 None），Phase C/Step 4
                self.scene.cleanup_orphan_containers();
                self.current_file = Some(outcome.path.clone());
                self.dirty = false;
                // 加载成功后加入最近文件列表
                add_recent_file(&mut self.recent_files, outcome.path.clone());
                self.flash(fill(
                    t(self.lang, T::FlashOpened),
                    &[outcome.path.display().to_string()],
                ));
                ctx.request_repaint();
            }
            Err(e) => {
                self.flash(fill(
                    t(self.lang, T::FlashOpenFailed),
                    std::slice::from_ref(&e),
                ));
            }
        }
    }

    /// 保存到当前文件；若没有则调用 save_file_as 弹出对话框
    pub(crate) fn save_file(&mut self, ctx: &egui::Context) {
        if let Some(path) = self.current_file.clone() {
            self.start_save(ctx, path);
        } else {
            self.save_file_as(ctx);
        }
    }

    /// 另存为：弹出对话框选择路径
    pub(crate) fn save_file_as(&mut self, ctx: &egui::Context) {
        let picked = rfd::FileDialog::new()
            .add_filter("PReferZ 项目", &["prz"])
            .set_file_name("untitled.prz")
            .save_file();
        if let Some(path) = picked {
            self.start_save(ctx, path);
        }
    }

    /// 新建空白画布：清scene / 纹理缓存 / undo / current_file / dirty    /// 调用前应已处理保存提示（由调用方负责）
    pub(crate) fn reset_canvas(&mut self, ctx: &egui::Context) {
        self.scene = Scene::new();
        self.texture_cache.clear();
        self.grayscale_texture_cache.clear();
        self.image_data_cache.clear();
        self.rgba_pixel_cache.clear();
        self.rgba_size_cache.clear();
        self.undo_stack = UndoStack::new();
        self.current_file = None;
        self.dirty = false;
        self.crop_mode = None;
        self.editing_text = None;
        self.color_picker_active = false;
        self.color_sample = None;
        self.pending_save_prompt = None;
        self.viewport.reset();
        self.flash(t(self.lang, T::FlashNewCanvas).to_string());
        ctx.request_repaint();
    }

    /// 触发新建画布流程：若 dirty 弹保存提示，否则直接 reset
    pub(crate) fn new_canvas(&mut self, ctx: &egui::Context) {
        if self.dirty {
            self.pending_save_prompt = Some(SavePromptAction::NewCanvas);
        } else {
            self.reset_canvas(ctx);
        }
    }

    /// 打开项目文件（.prz）。
    pub(crate) fn open_project_file(&mut self, ctx: &egui::Context) {
        let picked = rfd::FileDialog::new()
            .add_filter("PReferZ 项目", &["prz"])
            .pick_file();
        if let Some(path) = picked {
            self.add_recent_and_load(ctx, path);
        }
    }

    /// 记录最近文件并启动后台加载。
    pub(crate) fn add_recent_and_load(&mut self, ctx: &egui::Context, path: PathBuf) {
        add_recent_file(&mut self.recent_files, path.clone());
        self.bg_ops.start_load(ctx, path, self.lang);
    }

    /// 载入图片到当前画布。
    pub(crate) fn import_image_file(&mut self, _ctx: &egui::Context) {
        let picked = rfd::FileDialog::new()
            .add_filter("图片", &["png", "jpg", "jpeg", "gif", "bmp", "webp"])
            .pick_file();
        if let Some(path) = picked {
            self.pending_import.push(path);
        }
    }

    /// 从剪贴板粘贴图片到画布（spec §2.1 剪贴板粘贴）。
    /// UI 线程：读剪贴板（毫秒级，arboard 同步访问）。
    /// 后台线程：PNG 编码（几十~几百毫秒，大图会卡 UI）。
    /// 通过 bg_ops.import_rx 复用 finish_import 完成纹理上传 + item 创建。
    pub(crate) fn paste_from_clipboard(&mut self, ctx: &egui::Context) {
        let mut clipboard = match arboard::Clipboard::new() {
            Ok(c) => c,
            Err(e) => {
                self.flash(fill(
                    t(self.lang, T::FlashClipboardFailed),
                    &[e.to_string()],
                ));
                return;
            }
        };
        let img_data = match clipboard.get_image() {
            Ok(img) => img,
            Err(_) => {
                self.flash(t(self.lang, T::FlashNoClipboardImage).to_string());
                return;
            }
        };
        let (w, h) = (img_data.width as u32, img_data.height as u32);
        let rgba: Vec<u8> = img_data.bytes.into_owned();

        // 后台线程：PNG 编码（避免大图卡 UI）
        let (tx, rx) = mpsc::channel();
        self.bg_ops.import_rx = Some(rx);
        self.bg_ops.pending += 1;
        self.bg_ops.msg = Some(t(self.lang, T::FlashPasteImage).to_string());
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let outcome = (|| {
                let expected = (w as usize) * (h as usize) * 4;
                if rgba.len() != expected {
                    return ImportOutcome {
                        path: PathBuf::from("clipboard.png"),
                        bytes: Vec::new(),
                        width: 0,
                        height: 0,
                        rgba: Vec::new(),
                        error: Some(format!(
                            "剪贴板图片数据异常：{} 字节（预期 {}）",
                            rgba.len(),
                            expected
                        )),
                    };
                }
                let mut png_bytes: Vec<u8> = Vec::new();
                let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
                match image::ImageEncoder::write_image(
                    encoder,
                    &rgba,
                    w,
                    h,
                    image::ExtendedColorType::Rgba8,
                ) {
                    Ok(()) => ImportOutcome {
                        path: PathBuf::from("clipboard.png"),
                        bytes: png_bytes,
                        width: w,
                        height: h,
                        rgba,
                        error: None,
                    },
                    Err(e) => ImportOutcome {
                        path: PathBuf::from("clipboard.png"),
                        bytes: Vec::new(),
                        width: 0,
                        height: 0,
                        rgba: Vec::new(),
                        error: Some(format!("PNG 编码失败: {}", e)),
                    },
                }
            })();
            let _ = tx.send(outcome);
            ctx2.request_repaint();
        });
    }

    /// 启动后台保存
    pub(crate) fn start_save(&mut self, ctx: &egui::Context, path: PathBuf) {
        // 收集 image_data_cache（key 转字符串以匹sqlar name
        let mut images: HashMap<String, Vec<u8>> = HashMap::new();
        for item in &self.scene.items {
            if let ItemKind::Pixmap { texture_id, .. } = &item.kind {
                if let Some(bytes) = self.image_data_cache.get(texture_id) {
                    images.insert(texture_id.to_string(), bytes.clone());
                }
            }
        }
        let viewport = ViewportMeta {
            pan_x: self.viewport.pan.x,
            pan_y: self.viewport.pan.y,
            zoom: self.viewport.zoom,
        };
        self.bg_ops
            .start_save(ctx, path, self.scene.clone(), images, viewport, self.lang);
    }

    /// 启动后台导出（spec §2.3 导出）。
    /// 收集所有 Pixmap 的 RGBA + item 快照，后台线程逐像素合成 + 编码。
    /// 文本便签在导出中渲染为纯色矩形（MVP 简化，不渲染 glyph）。
    pub(crate) fn start_export(
        &mut self,
        ctx: &egui::Context,
        path: PathBuf,
        format: ExportFormat,
        selection_only: bool,
    ) {
        if self.scene.items.is_empty() {
            self.flash(t(self.lang, T::FlashCanvasEmptyNoExport).to_string());
            return;
        }

        // 按 selection_only 过滤 items
        let items_snapshot: Vec<Item> = if selection_only {
            let sel = &self.scene.selection;
            self.scene
                .items
                .iter()
                .filter(|it| sel.contains(&it.id))
                .cloned()
                .collect()
        } else {
            self.scene.items.clone()
        };
        if items_snapshot.is_empty() {
            self.flash(t(self.lang, T::FlashNoSelectionToExport).to_string());
            return;
        }

        // 收集 Pixmap 的 RGBA 像素和尺寸（按 texture_id 索引；只收集将用到的）
        let mut pixmaps: Vec<(u64, Vec<u8>, u32, u32)> = Vec::new();
        for item in &items_snapshot {
            if let ItemKind::Pixmap { texture_id, .. } = &item.kind {
                if let (Some(rgba), Some(size)) = (
                    self.rgba_pixel_cache.get(texture_id),
                    self.rgba_size_cache.get(texture_id),
                ) {
                    pixmaps.push((*texture_id, rgba.clone(), size.0, size.1));
                }
            }
        }

        let (tx, rx) = mpsc::channel();
        self.bg_ops.export_rx = Some(rx);
        self.bg_ops.pending += 1;
        self.bg_ops.msg = Some(format!(
            "{}: {}",
            t(self.lang, T::FlashExportProgress),
            path.display()
        ));
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let result = export_scene_to_file(&items_snapshot, &pixmaps, &path, format)
                .map(|_| String::new());
            let _ = tx.send(ExportOutcome { path, result });
            ctx2.request_repaint();
        });
    }

    /// 启动后台批量导出图片到目录（每个 Pixmap 一个 PNG 文件，原始分辨率）。
    /// `selection_only=true` 仅导出选中的 Pixmap。
    pub(crate) fn start_export_images_to_dir(
        &mut self,
        ctx: &egui::Context,
        dir: PathBuf,
        selection_only: bool,
    ) {
        // 过滤 Pixmap items
        let sel = &self.scene.selection;
        let pixmap_items: Vec<&Item> = self
            .scene
            .items
            .iter()
            .filter(|it| matches!(it.kind, ItemKind::Pixmap { .. }))
            .filter(|it| !selection_only || sel.contains(&it.id))
            .collect();

        if pixmap_items.is_empty() {
            self.flash(t(
                self.lang,
                if selection_only {
                    T::FlashNoSelectedImages
                } else {
                    T::FlashNoExportableImages
                },
            ));
            return;
        }

        // 收集 (filename, rgba, w, h)
        let mut entries: Vec<(String, Vec<u8>, u32, u32)> = Vec::new();
        let mut used_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut anon_index = 1usize;
        for item in &pixmap_items {
            if let ItemKind::Pixmap {
                texture_id,
                filename,
                ..
            } = &item.kind
            {
                let (Some(rgba), Some(size)) = (
                    self.rgba_pixel_cache.get(texture_id),
                    self.rgba_size_cache.get(texture_id),
                ) else {
                    continue;
                };

                // 文件名：优先 filename 的 stem，否则 image_N.png；确保目录内唯一
                let base = filename
                    .as_ref()
                    .and_then(|s| {
                        std::path::Path::new(s)
                            .file_stem()
                            .and_then(|x| x.to_str())
                            .map(|x| x.to_string())
                    })
                    .unwrap_or_else(|| {
                        let n = format!("image_{}", anon_index);
                        anon_index += 1;
                        n
                    });
                let mut name = format!("{}.png", base);
                let mut n = 1;
                while used_names.contains(&name) {
                    name = format!("{}_{}.png", base, n);
                    n += 1;
                }
                used_names.insert(name.clone());
                entries.push((name, rgba.clone(), size.0, size.1));
            }
        }

        if entries.is_empty() {
            self.flash(t(self.lang, T::FlashPixelDataNotReady).to_string());
            return;
        }

        let (tx, rx) = mpsc::channel();
        self.bg_ops.export_rx = Some(rx);
        self.bg_ops.pending += 1;
        self.bg_ops.msg = Some(format!(
            "{}: {}",
            t(self.lang, T::FlashExportImagesProgress),
            dir.display()
        ));
        let ctx2 = ctx.clone();
        let lang = self.lang;
        std::thread::spawn(move || {
            let total = entries.len();
            let result = export_pixmaps_to_dir(&entries, &dir).map(|_| {
                format!(
                    "{} {} {}: {}",
                    t(lang, T::FlashExportedNImages),
                    total,
                    t(lang, T::FlashExportImagesTo),
                    dir.display()
                )
            });
            let _ = tx.send(ExportOutcome {
                path: dir.clone(),
                result,
            });
            ctx2.request_repaint();
        });
    }

    /// 弹出目录选择器，确定目录后启动批量图片导出。
    pub(crate) fn start_export_images_dialog(&mut self, ctx: &egui::Context, selection_only: bool) {
        let picked = rfd::FileDialog::new()
            .set_title("选择导出目录")
            .pick_folder();
        if let Some(dir) = picked {
            self.start_export_images_to_dir(ctx, dir, selection_only);
        }
    }

    pub(crate) fn start_export_dialog(
        &mut self,
        ctx: &egui::Context,
        format: ExportFormat,
        selection_only: bool,
    ) {
        let ext = format.extension();
        let default_name = if selection_only { "selection" } else { "scene" };
        let picked = rfd::FileDialog::new()
            .add_filter(format.display_name(), &[ext])
            .set_file_name(format!("{}.{}", default_name, ext))
            .save_file();
        if let Some(mut path) = picked {
            // 确保扩展名正确
            if path.extension().and_then(|e| e.to_str()) != Some(ext) {
                path.set_extension(ext);
            }
            self.start_export(ctx, path, format, selection_only);
        }
    }
}
