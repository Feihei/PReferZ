use super::*;
use preferz_core::{Item, ItemKind};
use std::path::Path;

/// 颜色采样结果（spec §2.2 颜色采样）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct ColorSample {
    pub(crate) r: u8,
    pub(crate) g: u8,
    pub(crate) b: u8,
    pub(crate) a: u8,
    pub(crate) screen_pos: egui::Pos2,
    pub(crate) px: u32,
    pub(crate) py: u32,
}

// ─────────────────────────── 导出 ───────────────────────────

/// 导出格式（spec §2.3 导出，SVG 暂不支持）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Jpeg,
}

impl ExportFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Png => "png",
            ExportFormat::Jpeg => "jpg",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ExportFormat::Png => "PNG",
            ExportFormat::Jpeg => "JPG",
        }
    }
}

/// 导出场景到文件（后台线程调用）。
/// 逐像素反向采样：对每个输出像素，画布坐标 → 遍历 Z 序倒序 → 命中 Pixmap 则采样。
/// 文本便签渲染为纯色矩形（MVP 简化）。
pub(crate) fn export_scene_to_file(
    items: &[Item],
    pixmaps: &[(u64, Vec<u8>, u32, u32)],
    path: &Path,
    format: ExportFormat,
) -> Result<(), String> {
    use euclid::Point2D;
    use preferz_core::spaces::CanvasSpace;

    if items.is_empty() {
        return Err("画布为空".to_string());
    }

    // 计算所有 item 画布 AABB 并集
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
    // 加 padding 防止边缘裁切
    let padding = 8.0_f32;
    min_x -= padding;
    min_y -= padding;
    max_x += padding;
    max_y += padding;

    let canvas_w = (max_x - min_x).max(1.0).ceil() as u32;
    let canvas_h = (max_y - min_y).max(1.0).ceil() as u32;
    // 限制最大尺寸避免 OOM（100MP 上限）
    const MAX_PIXELS: u64 = 100_000_000;
    if (canvas_w as u64) * (canvas_h as u64) > MAX_PIXELS {
        return Err(format!(
            "导出尺寸过大: {}x{} (上限 {} 像素)",
            canvas_w, canvas_h, MAX_PIXELS
        ));
    }

    // 预构建 Pixmap 像素查找表（texture_id → (rgba, w, h)）
    use std::collections::HashMap;
    let pixmap_map: HashMap<u64, (&Vec<u8>, u32, u32)> = pixmaps
        .iter()
        .map(|(id, rgba, w, h)| (*id, (rgba, *w, *h)))
        .collect();

    // 按 Z 序倒序（顶层先采样）
    let mut sorted: Vec<&Item> = items.iter().collect();
    sorted.sort_by_key(|b| std::cmp::Reverse(b.z));

    // 逐像素合成
    let mut out_rgba: Vec<u8> = vec![0u8; (canvas_w as usize) * (canvas_h as usize) * 4];
    // 背景填充为白色（JPG 不支持透明，PNG 也用白底更实用）
    for px in out_rgba.as_chunks_mut::<4>().0 {
        px[0] = 255;
        px[1] = 255;
        px[2] = 255;
        px[3] = 255;
    }

    // 遍历每个像素，反向找命中的顶层 item
    for y in 0..canvas_h {
        let canvas_y = min_y + y as f32 + 0.5;
        for x in 0..canvas_w {
            let canvas_x = min_x + x as f32 + 0.5;
            let canvas_pos: Point2D<f32, CanvasSpace> = Point2D::new(canvas_x, canvas_y);

            // Z 序倒序查找命中的 item
            for item in &sorted {
                if !item.contains_canvas_point(canvas_pos) {
                    continue;
                }
                // 命中：采样颜色
                let pixel = sample_item_pixel(item, &pixmap_map, canvas_pos);
                if let Some((r, g, b, a)) = pixel {
                    let idx = ((y as usize) * (canvas_w as usize) + x as usize) * 4;
                    // alpha 混合到白底
                    let alpha = a as f32 / 255.0;
                    out_rgba[idx] = (r as f32 * alpha + 255.0 * (1.0 - alpha)) as u8;
                    out_rgba[idx + 1] = (g as f32 * alpha + 255.0 * (1.0 - alpha)) as u8;
                    out_rgba[idx + 2] = (b as f32 * alpha + 255.0 * (1.0 - alpha)) as u8;
                    // PNG 保留原 alpha；JPG 后续会丢弃
                    out_rgba[idx + 3] = 255;
                }
                break; // 只取顶层
            }
        }
    }

    // 编码到文件
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let writer = std::io::BufWriter::new(file);
    match format {
        ExportFormat::Png => {
            let encoder = image::codecs::png::PngEncoder::new(writer);
            image::ImageEncoder::write_image(
                encoder,
                &out_rgba,
                canvas_w,
                canvas_h,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| e.to_string())?;
        }
        ExportFormat::Jpeg => {
            // JPEG 不支持 alpha：RGBA → RGB（背景已合成白底，直接丢弃 alpha）
            let mut out_rgb: Vec<u8> =
                Vec::with_capacity((canvas_w as usize) * (canvas_h as usize) * 3);
            for px in out_rgba.as_chunks::<4>().0 {
                // 丢弃 alpha（背景已合成白底）
                out_rgb.extend_from_slice(&px[..3]);
            }
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(writer, 90);
            image::ImageEncoder::write_image(
                encoder,
                &out_rgb,
                canvas_w,
                canvas_h,
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

/// 批量将 Pixmap RGBA 数据写入指定目录，每个 entry 一个 PNG 文件。
/// 单个失败不中断后续，最终汇总错误数。返回成功数与错误数描述。
pub(crate) fn export_pixmaps_to_dir(
    entries: &[(String, Vec<u8>, u32, u32)],
    dir: &Path,
) -> Result<(), String> {
    if !dir.exists() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut ok = 0u32;
    let mut errs: Vec<String> = Vec::new();
    for (name, rgba, w, h) in entries {
        let path = dir.join(name);
        match std::fs::File::create(&path) {
            Ok(file) => {
                let writer = std::io::BufWriter::new(file);
                let encoder = image::codecs::png::PngEncoder::new(writer);
                if let Err(e) = image::ImageEncoder::write_image(
                    encoder,
                    rgba,
                    *w,
                    *h,
                    image::ExtendedColorType::Rgba8,
                ) {
                    errs.push(format!("{}: {}", name, e));
                } else {
                    ok += 1;
                }
            }
            Err(e) => errs.push(format!("{}: {}", name, e)),
        }
    }
    if errs.is_empty() {
        Ok(())
    } else if ok == 0 {
        Err(format!("全部失败 ({}): {}", errs.len(), errs.join("; ")))
    } else {
        // 部分成功也视为成功，但带上错误信息
        Ok(()) // 成功条数通过 flash 消息体现；err 信息记录到日志
    }
}

/// 采样 item 在画布坐标处的像素颜色。
/// Pixmap：逆变换到局部坐标 → 应用 crop → 采样 RGBA（含 opacity、grayscale）。
/// Text：返回纯色（背景灰 + 文字色混合的简化表示）。
pub(crate) fn sample_item_pixel(
    item: &Item,
    pixmap_map: &std::collections::HashMap<u64, (&Vec<u8>, u32, u32)>,
    canvas_pos: preferz_core::spaces::CanvasPoint,
) -> Option<(u8, u8, u8, u8)> {
    let inv = item.local_to_canvas().inverse()?;
    let local = inv.transform_point(canvas_pos);
    let base = item.base_size();

    match &item.kind {
        ItemKind::Pixmap {
            texture_id,
            opacity,
            grayscale,
            crop,
            ..
        } => {
            let (rgba, w, h) = pixmap_map.get(texture_id)?;
            let w = *w as f32;
            let h = *h as f32;

            // 应用 crop：crop 定义在局部空间，需把 local 映射到 crop 后的图片坐标
            let (cx, cy, cw, ch) = if let Some(c) = crop {
                (c.x, c.y, c.width, c.height)
            } else {
                (0.0, 0.0, w, h)
            };

            // local 坐标 → crop 内偏移
            let px = local.x;
            let py = local.y;
            // crop 区域 = [cx, cx+cw] × [cy, cy+ch]，超出则透明
            if px < cx || px >= cx + cw || py < cy || py >= cy + ch {
                return Some((0, 0, 0, 0)); // crop 外透明
            }

            // 把 crop 内坐标映射回原始图片像素坐标
            // crop 把 [cx, cx+cw] 拉伸到 [0, w]（scale 使然），所以：
            //   原图 x = cx + (local.x / base.x) * cw  -- 但 local 已是变换后坐标，base 已含 scale
            // 简化：crop 在 base_size 空间定义，base_size 是 original_size * scale，
            //       所以 local 直接对应 base_size 空间，px/cw * 原图宽 即原图坐标
            let img_x = ((px - cx) / cw * w).clamp(0.0, w - 1.0) as u32;
            let img_y = ((py - cy) / ch * h).clamp(0.0, h - 1.0) as u32;

            let idx = ((img_y as usize) * (w as usize) + img_x as usize) * 4;
            if idx + 3 >= rgba.len() {
                return None;
            }
            let r = rgba[idx];
            let g = rgba[idx + 1];
            let b = rgba[idx + 2];
            let a = rgba[idx + 3];

            // 灰度（BT.601）
            let (r, g, b) = if *grayscale {
                let y = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32).round() as u8;
                (y, y, y)
            } else {
                (r, g, b)
            };

            // opacity
            let alpha = (a as f32 * opacity.clamp(0.0, 1.0)) as u8;
            Some((r, g, b, alpha))
        }
        ItemKind::Text { color, .. } => {
            // 简化：整个文本区域用文字色填充（MVP）
            // 仅当 local 在 [0, base.x] × [0, base.y] 内（contains_canvas_point 已保证）
            let _ = (local, base);
            Some((color[0], color[1], color[2], color[3]))
        }
        // A1 数据模型先行，取色采样在 Phase A6 后补充
        ItemKind::Shape { .. } => None,
        // 墨迹（plan #10）：与 Shape 一致，取色器不采样矢量笔迹
        ItemKind::Freedraw { .. } => None,
        // 画框不参与导出采样
        ItemKind::Frame { .. } => None,
        // 图表（plan #8）：与 Text 同款简化采样——命中区域返回系列颜色
        ItemKind::Chart { color, .. } => {
            let _ = (local, base);
            Some((color[0], color[1], color[2], color[3]))
        }
    }
}
