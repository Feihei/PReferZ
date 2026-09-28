//! offscreen — 导出选区的离屏场景收集与软件光栅化。
//!
//! 思路：建一个**离屏 `egui::Context`**（与应用主上下文同一份字体定义、同一主题），
//! 用临时 `ViewportState` 把选区包围盒映射到目标像素矩形，然后复用
//! [`PReferZApp::draw_item_visual`] 逐 item 收集 egui 形状。这份形状有两路去向：
//!
//! 1. `ctx.tessellate` → 三角网格 → 本模块的软件光栅化器（PNG / 剪贴板）；
//!    文字/手绘风曲线等所有类型都经 epaint 同一条细分路径，与屏上观感一致；
//! 2. 逐 item 的形状列表 → `svg_export.rs` 转成 SVG 元素（矢量导出）。
//!
//! 光栅化约定：epaint 全链路使用**预乘 alpha**——字体图集像素由 `from_white_alpha`
//! 生成（天然预乘），图片纹理在构建像素表时手动预乘，顶点色亦为预乘；混合走
//! `out = src + dst·(1−a_src)`，最后一步逆预乘转直通 alpha 输出 PNG/剪贴板。

use super::*;

/// egui 纹理 id → 预乘 RGBA 像素（宽、高）。字体图集放在 `TextureId::default()`。
pub(crate) type TexturePixels = HashMap<egui::TextureId, (Vec<u8>, usize, usize)>;

/// 离屏收集结果：逐 item 的形状列表（顺序 = 传入顺序，Z 序底层在前）。
pub(crate) struct OffscreenScene {
    pub(crate) ctx: egui::Context,
    /// 目标像素矩形（逻辑空间 = 输出像素空间）。
    pub(crate) rect: egui::Rect,
    pub(crate) textures: TexturePixels,
    /// 收集的 item 数据（与 [`Self::item_shapes`] 一一对应；SVG 的 Pixmap 内嵌需要）。
    pub(crate) items: Vec<Item>,
    pub(crate) item_shapes: Vec<Vec<egui::epaint::ClippedShape>>,
}

impl OffscreenScene {
    /// 所有 item 的形状展平（供整体 tessellate）。
    pub(crate) fn flat_shapes(&self) -> Vec<egui::epaint::ClippedShape> {
        self.item_shapes.iter().flatten().cloned().collect()
    }
}

impl PReferZApp {
    /// 离屏收集选区 item 的 egui 形状。
    ///
    /// `region`：选区画布包围盒（含边距）；`zoom`：画布像素 → 输出像素的比例
    /// （栅格导出用 `EXPORT_SCALE`=2，SVG 用 1）。必须在 UI 线程调用（临时换出
    /// `self.viewport`），成本为一次形状收集，无逐像素工作。
    pub(crate) fn collect_offscreen_scene(
        &mut self,
        main_ctx: &egui::Context,
        item_ids: &[ItemId],
        region: CanvasRect,
        zoom: f32,
    ) -> OffscreenScene {
        self.ensure_pixmap_textures(main_ctx);
        self.ensure_grayscale_textures(main_ctx);

        let out_w = (region.width() * zoom).ceil().max(1.0);
        let out_h = (region.height() * zoom).ceil().max(1.0);
        let target = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(out_w, out_h));

        let ctx = egui::Context::default();
        // 与主上下文同一份字体（缺省时懒构建并缓存）、同一套明暗主题样式
        // （图表取色跟随主题）
        if self.export_fonts.is_none() {
            self.export_fonts = Some(crate::app_font_definitions());
        }
        ctx.set_fonts(self.export_fonts.clone().expect("export_fonts 已初始化"));
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            ctx.set_style_of(theme, main_ctx.style_of(theme));
        }
        ctx.set_pixels_per_point(1.0);
        // begin_pass 初始化字体（virgin Context 的 fonts_mut/tessellate 都要求
        // 已开一帧）；screen_rect 覆盖目标矩形，避免预览区域被裁剪
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(target),
            ..egui::RawInput::default()
        });

        // Z 序排列选中项（底层先画）
        let items: Vec<Item> = self
            .scene
            .items_by_z_order()
            .into_iter()
            .filter(|it| item_ids.contains(&it.id))
            .cloned()
            .collect();

        // 纹理像素表：预乘 RGBA；字体图集调用方在 tessellate 前补入
        let mut textures = TexturePixels::new();
        for item in &items {
            if let ItemKind::Pixmap {
                texture_id,
                grayscale,
                ..
            } = &item.kind
            {
                let Some(rgba) = self.rgba_pixel_cache.get(texture_id) else {
                    continue;
                };
                let Some(&(w, h)) = self.rgba_size_cache.get(texture_id) else {
                    continue;
                };
                let mut px = rgba.clone();
                if *grayscale {
                    grayscale_rgba(&mut px);
                }
                premultiply_rgba(&mut px);
                if let Some(handle) = self.texture_cache.get(texture_id) {
                    textures.insert(handle.id(), (px, w as usize, h as usize));
                }
            }
        }

        // 临时视口：选区中心 → 目标矩形中心，zoom = 输出像素/画布像素
        let mut vp = ViewportState::default();
        vp.zoom = zoom;
        vp.pan = region.center().to_vector();
        vp.set_screen_rect(preferz_core::spaces::ScreenRect::new(
            preferz_core::spaces::ScreenPoint::new(target.min.x, target.min.y),
            euclid::Size2D::new(target.width(), target.height()),
        ));
        let old_viewport = std::mem::replace(&mut self.viewport, vp);

        let layer_id = egui::LayerId::new(egui::Order::Middle, egui::Id::new("export_offscreen"));
        let builder = egui::UiBuilder {
            max_rect: Some(target),
            layer_id: Some(layer_id),
            ..egui::UiBuilder::default()
        };
        let mut ui = egui::Ui::new(ctx.clone(), egui::Id::new("export_offscreen_ui"), builder);

        // 逐 item 收集：记录每项画完后的 PaintList 长度边界，切出各自的形状区间
        let mut items_shapes: Vec<Vec<egui::epaint::ClippedShape>> =
            Vec::with_capacity(items.len());
        let mut prev_len = 0usize;
        for item in &items {
            self.draw_item_visual(&mut ui, item, None);
            let len = paint_list_len(&ctx, layer_id);
            items_shapes.push(slice_paint_list(&ctx, layer_id, prev_len, len));
            prev_len = len;
        }

        self.viewport = old_viewport;

        OffscreenScene {
            ctx,
            rect: target,
            textures,
            items,
            item_shapes: items_shapes,
        }
    }

    /// 在离屏上下文里补入字体图集像素（tessellate 前调用；文字项才会用到）。
    pub(crate) fn attach_font_atlas(scene: &mut OffscreenScene) {
        if scene.item_shapes.iter().all(|s| s.is_empty()) {
            return;
        }
        // fonts_mut 触发字体初始化（无文字项时图集也可能尚未生成）
        scene.ctx.fonts_mut(|_| {});
        let image = scene.ctx.fonts(|f| f.image());
        let (w, h) = (image.size[0], image.size[1]);
        let mut px: Vec<u8> = Vec::with_capacity(w * h * 4);
        for c in &image.pixels {
            px.extend_from_slice(&c.to_array());
        }
        scene
            .textures
            .insert(egui::TextureId::default(), (px, w, h));
    }
}

/// 读取某层 PaintList 的当前长度。
fn paint_list_len(ctx: &egui::Context, layer_id: egui::LayerId) -> usize {
    ctx.graphics(|g| g.get(layer_id).map(|p| p.all_entries().len()).unwrap_or(0))
}

/// 克隆某层 PaintList 的 `[start, end)` 区间。
fn slice_paint_list(
    ctx: &egui::Context,
    layer_id: egui::LayerId,
    start: usize,
    end: usize,
) -> Vec<egui::epaint::ClippedShape> {
    if end <= start {
        return Vec::new();
    }
    ctx.graphics(|g| {
        g.get(layer_id)
            .map(|p| {
                p.all_entries()
                    .skip(start)
                    .take(end - start)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// BT.601 灰度（与 render.rs `ensure_grayscale_textures` 同系数，原地转换）。
pub(crate) fn grayscale_rgba(rgba: &mut [u8]) {
    for chunk in rgba.as_chunks_mut::<4>().0 {
        let lum = (0.299 * chunk[0] as f32 + 0.587 * chunk[1] as f32 + 0.114 * chunk[2] as f32)
            .round()
            .clamp(0.0, 255.0) as u8;
        chunk[0] = lum;
        chunk[1] = lum;
        chunk[2] = lum;
    }
}

/// 直通 RGBA → 预乘 RGBA（原地）。
pub(crate) fn premultiply_rgba(rgba: &mut [u8]) {
    for chunk in rgba.as_chunks_mut::<4>().0 {
        let a = chunk[3] as u32;
        chunk[0] = ((chunk[0] as u32 * a + 127) / 255) as u8;
        chunk[1] = ((chunk[1] as u32 * a + 127) / 255) as u8;
        chunk[2] = ((chunk[2] as u32 * a + 127) / 255) as u8;
    }
}

/// 软件光栅化：把 tessellate 产出的三角网格逐像素合成到 RGBA 缓冲。
///
/// - 输出为**直通 alpha** 的 RGBA8（可直接编码 PNG / 写剪贴板）；
/// - `opaque_white_background` = true 时先铺白底（JPEG 式观感），false 保持透明；
/// - 混合：预乘 over（`out = src + dst·(1−a_src)`）；
/// - 共享边用近似 top-left 规则避免双重混合。
pub(crate) fn rasterize_primitives(
    prims: &[egui::epaint::ClippedPrimitive],
    textures: &TexturePixels,
    out_w: usize,
    out_h: usize,
    opaque_white_background: bool,
) -> Result<Vec<u8>, String> {
    const MAX_PIXELS: usize = 16_000_000;
    if out_w.saturating_mul(out_h) > MAX_PIXELS {
        return Err(format!(
            "导出尺寸过大: {}x{} (上限 {} 像素)",
            out_w, out_h, MAX_PIXELS
        ));
    }

    // 预乘 RGBA 缓冲（u8 已够用：alpha 合成是乘加，u8 舍入误差不可见）
    let mut buf = vec![0u8; out_w * out_h * 4];
    if opaque_white_background {
        for px in buf.as_chunks_mut::<4>().0 {
            px[0] = 255;
            px[1] = 255;
            px[2] = 255;
            px[3] = 255;
        }
    }

    for prim in prims {
        let egui::epaint::Primitive::Mesh(mesh) = &prim.primitive else {
            continue;
        };
        let Some((tex_px, tw, th)) = textures.get(&mesh.texture_id) else {
            continue;
        };
        let tw = *tw;
        let th = *th;

        for tri in mesh.indices.as_chunks::<3>().0 {
            let v0 = &mesh.vertices[tri[0] as usize];
            let v1 = &mesh.vertices[tri[1] as usize];
            let v2 = &mesh.vertices[tri[2] as usize];

            // 归一化绕向（面积>0），简化内侧判定
            let area = edge(v0.pos, v1.pos, v2.pos);
            if area == 0.0 {
                continue;
            }
            let (a, b, c) = if area > 0.0 {
                (v0, v1, v2)
            } else {
                (v0, v2, v1)
            };
            let area = area.abs();

            // 像素循环范围 = 三角形 bbox ∩ clip rect
            let min_x = a.pos.x.min(b.pos.x).min(c.pos.x).max(prim.clip_rect.min.x);
            let max_x = a.pos.x.max(b.pos.x).max(c.pos.x).min(prim.clip_rect.max.x);
            let min_y = a.pos.y.min(b.pos.y).min(c.pos.y).max(prim.clip_rect.min.y);
            let max_y = a.pos.y.max(b.pos.y).max(c.pos.y).min(prim.clip_rect.max.y);
            let x0 = (min_x.floor() as i64).clamp(0, out_w as i64);
            let x1 = (max_x.ceil() as i64).clamp(0, out_w as i64);
            let y0 = (min_y.floor() as i64).clamp(0, out_h as i64);
            let y1 = (max_y.ceil() as i64).clamp(0, out_h as i64);
            if x0 >= x1 || y0 >= y1 {
                continue;
            }

            // 三角边（与 w0/w1/w2 对应：edge(b,c)、edge(c,a)、edge(a,b)）
            // + top-left 标记（近似规则：共享边只被一侧接受，避免双重混合）
            let edges = [(b.pos, c.pos), (c.pos, a.pos), (a.pos, b.pos)];
            let tl = [
                is_top_left(edges[0]),
                is_top_left(edges[1]),
                is_top_left(edges[2]),
            ];

            for y in y0..y1 {
                for x in x0..x1 {
                    let p = egui::Pos2::new(x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = edge(b.pos, c.pos, p);
                    let w1 = edge(c.pos, a.pos, p);
                    let w2 = edge(a.pos, b.pos, p);
                    let inside = (w0 > 0.0 || (w0 == 0.0 && tl[0]))
                        && (w1 > 0.0 || (w1 == 0.0 && tl[1]))
                        && (w2 > 0.0 || (w2 == 0.0 && tl[2]));
                    if !inside {
                        continue;
                    }
                    let b0 = w0 / area;
                    let b1 = w1 / area;
                    let b2 = w2 / area;

                    let u = a.uv[0] * b0 + b.uv[0] * b1 + c.uv[0] * b2;
                    let v = a.uv[1] * b0 + b.uv[1] * b1 + c.uv[1] * b2;
                    let (r, g, bl, al) = sample_bilinear(tex_px, tw, th, u, v);

                    // 顶点色（预乘）调制纹理色（预乘）→ 结果仍预乘
                    let ca = a.color; // egui 顶点色逐顶点插值在 feather 边上关键
                    let cb = b.color;
                    let cc = c.color;
                    let cvr =
                        (ca.r() as f32 * b0 + cb.r() as f32 * b1 + cc.r() as f32 * b2) / 255.0;
                    let cvg =
                        (ca.g() as f32 * b0 + cb.g() as f32 * b1 + cc.g() as f32 * b2) / 255.0;
                    let cvb =
                        (ca.b() as f32 * b0 + cb.b() as f32 * b1 + cc.b() as f32 * b2) / 255.0;
                    let cva =
                        (ca.a() as f32 * b0 + cb.a() as f32 * b1 + cc.a() as f32 * b2) / 255.0;

                    let sr = r * cvr;
                    let sg = g * cvg;
                    let sb = bl * cvb;
                    let sa = al * cva;

                    // 预乘 over（缓冲与 src 统一在 [0,1] 预乘量纲）
                    let idx = (y as usize * out_w + x as usize) * 4;
                    let inv = 1.0 - sa;
                    buf[idx] = quantize(sr + buf[idx] as f32 * inv / 255.0);
                    buf[idx + 1] = quantize(sg + buf[idx + 1] as f32 * inv / 255.0);
                    buf[idx + 2] = quantize(sb + buf[idx + 2] as f32 * inv / 255.0);
                    buf[idx + 3] = quantize(sa + buf[idx + 3] as f32 * inv / 255.0);
                }
            }
        }
    }

    // 逆预乘 → 直通 alpha
    for px in buf.as_chunks_mut::<4>().0 {
        let a = px[3];
        if a == 0 {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
        } else {
            let a = a as u32;
            px[0] = ((px[0] as u32 * 255 + a / 2) / a).min(255) as u8;
            px[1] = ((px[1] as u32 * 255 + a / 2) / a).min(255) as u8;
            px[2] = ((px[2] as u32 * 255 + a / 2) / a).min(255) as u8;
        }
    }
    Ok(buf)
}

/// 有向边函数（2 倍叉积）。
fn edge(a: egui::Pos2, b: egui::Pos2, p: egui::Pos2) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}

/// top-left 边判定（CCW 三角形共享边只被一侧光栅化，避免双重混合）。
fn is_top_left(a: (egui::Pos2, egui::Pos2)) -> bool {
    let (a, b) = a;
    // top 边：水平且向上（y 递减）；left 边：向下（y 递增）
    (a.y == b.y && b.x < a.x) || (b.y > a.y)
}

/// 双线性采样（u、v 为归一化纹理坐标；返回预乘 RGBA ∈ [0,1]）。
fn sample_bilinear(px: &[u8], w: usize, h: usize, u: f32, v: f32) -> (f32, f32, f32, f32) {
    if w == 0 || h == 0 || px.len() < w * h * 4 {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let x = u * w as f32 - 0.5;
    let y = v * h as f32 - 0.5;
    let x0 = x.floor() as i64;
    let y0 = y.floor() as i64;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let mut acc = [0.0f32; 4];
    for (dy, wy) in [(0, 1.0 - fy), (1, fy)] {
        for (dx, wx) in [(0, 1.0 - fx), (1, fx)] {
            let xi = (x0 + dx).clamp(0, w as i64 - 1) as usize;
            let yi = (y0 + dy).clamp(0, h as i64 - 1) as usize;
            let idx = (yi * w + xi) * 4;
            let weight = wx * wy;
            for ch in 0..4 {
                acc[ch] += px[idx + ch] as f32 / 255.0 * weight;
            }
        }
    }
    (acc[0], acc[1], acc[2], acc[3])
}

fn quantize(v: f32) -> u8 {
    (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 全白不透明矩形 mesh → 光栅化后中心像素应为白色，透明区外 alpha=0。
    #[test]
    fn rasterizes_white_quad() {
        let r = egui::Rect::from_min_size(egui::Pos2::new(2.0, 3.0), egui::vec2(10.0, 8.0));
        let mesh = textured_mesh(7, r, egui::Color32::WHITE);
        let prims = vec![egui::epaint::ClippedPrimitive {
            clip_rect: r,
            primitive: egui::epaint::Primitive::Mesh(mesh),
        }];
        let mut textures = TexturePixels::new();
        textures.insert(egui::TextureId::Managed(7), (vec![255; 4], 1, 1));

        let out = rasterize_primitives(&prims, &textures, 20, 20, false).expect("rasterize");
        assert_eq!(out.len(), 20 * 20 * 4);
        let center = (10 * 20 + 6) * 4;
        assert_eq!(
            [
                out[center],
                out[center + 1],
                out[center + 2],
                out[center + 3]
            ],
            [255, 255, 255, 255]
        );
        // 矩形外为透明
        let outside = (20 + 1) * 4;
        assert_eq!(out[outside + 3], 0);
    }

    /// 透明背景下不铺白底；opaque 时背景为白。
    #[test]
    fn background_modes() {
        let prims: Vec<egui::epaint::ClippedPrimitive> = Vec::new();
        let textures = TexturePixels::new();
        let transparent = rasterize_primitives(&prims, &textures, 4, 4, false).expect("ok");
        assert!(transparent.iter().all(|&b| b == 0));
        let white = rasterize_primitives(&prims, &textures, 4, 4, true).expect("ok");
        assert!(white
            .as_chunks::<4>()
            .0
            .iter()
            .all(|px| px == &[255, 255, 255, 255]));
    }

    /// 两层叠加：上层半透明黑盖在白底上应得到灰色（预乘 over 正确性）。
    #[test]
    fn alpha_blending_over() {
        let full = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(4.0, 4.0));
        // 白底 mesh（0..4 全域）
        let bg = textured_mesh(1, full, egui::Color32::WHITE);
        // 上层：预乘 50% 黑（Color32::from_black_alpha(128)）
        let fg = textured_mesh(1, full, egui::Color32::from_black_alpha(128));
        let prims = vec![
            egui::epaint::ClippedPrimitive {
                clip_rect: egui::Rect::EVERYTHING,
                primitive: egui::epaint::Primitive::Mesh(bg),
            },
            egui::epaint::ClippedPrimitive {
                clip_rect: egui::Rect::EVERYTHING,
                primitive: egui::epaint::Primitive::Mesh(fg),
            },
        ];
        let mut textures = TexturePixels::new();
        textures.insert(egui::TextureId::Managed(1), (vec![255; 4], 1, 1));
        let out = rasterize_primitives(&prims, &textures, 4, 4, false).expect("ok");
        // 白底 + 50% 黑 = 灰 ≈ 127（逆预乘后）
        let v = out[0];
        assert!((110..=145).contains(&v), "预期灰色，实际 {v}");
        assert_eq!(out[3], 255);
    }

    /// 端到端离屏管线：virgin Context 上建 Ui → 画矩形+文字 → 取 PaintList →
    /// tessellate → 光栅化。锁定 collect_offscreen_scene 依赖的全部 egui API 行为。
    #[test]
    fn offscreen_pipeline_end_to_end() {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.0);
        let target = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(60.0, 40.0));
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(target),
            ..egui::RawInput::default()
        });
        let layer_id = egui::LayerId::new(egui::Order::Middle, egui::Id::new("t"));
        let ui = egui::Ui::new(
            ctx.clone(),
            egui::Id::new("t_ui"),
            egui::UiBuilder {
                max_rect: Some(target),
                layer_id: Some(layer_id),
                ..egui::UiBuilder::default()
            },
        );
        ui.painter().rect_filled(
            egui::Rect::from_min_size(egui::Pos2::new(5.0, 5.0), egui::vec2(20.0, 10.0)),
            0.0,
            egui::Color32::RED,
        );
        ui.painter().text(
            egui::pos2(10.0, 30.0),
            egui::Align2::LEFT_TOP,
            "Hi",
            egui::FontId::proportional(12.0),
            egui::Color32::BLACK,
        );

        let shapes = ctx.graphics(|g| {
            g.get(layer_id)
                .map(|p| p.all_entries().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
        });
        assert!(!shapes.is_empty(), "PaintList 应已收到形状");

        let prims = ctx.tessellate(shapes, 1.0);
        assert!(!prims.is_empty(), "tessellate 应产出网格");
        ctx.fonts_mut(|_| {});
        let atlas = ctx.fonts(|f| f.image());
        let (aw, ah) = (atlas.size[0], atlas.size[1]);
        let mut atlas_px: Vec<u8> = Vec::with_capacity(aw * ah * 4);
        for c in &atlas.pixels {
            atlas_px.extend_from_slice(&c.to_array());
        }
        let mut textures = TexturePixels::new();
        textures.insert(egui::TextureId::default(), (atlas_px, aw, ah));

        let out = rasterize_primitives(&prims, &textures, 60, 40, true).expect("rasterize");
        // 矩形内部应为红
        let red = (10 * 60 + 10) * 4;
        assert!(
            out[red] > 180 && out[red + 1] < 100 && out[red + 2] < 100,
            "矩形应偏红"
        );
        // 文字区（y 30..40, x 10..30）应有不透明像素
        let opaque_text_pixels = (30..40)
            .flat_map(|y| (10..30).map(move |x| (y * 60 + x) * 4 + 3))
            .filter(|&i| out[i] > 0)
            .count();
        assert!(opaque_text_pixels > 0, "文字区应有不透明像素");
    }

    /// 指定纹理 id 与颜色的单矩形 mesh（quad 顶点手工构造——add_colored_rect
    /// 要求无自定义纹理）。
    fn textured_mesh(tex: u64, r: egui::Rect, color: egui::Color32) -> egui::Mesh {
        let mut mesh = egui::Mesh {
            texture_id: egui::TextureId::Managed(tex),
            ..Default::default()
        };
        let i = mesh.vertices.len() as u32;
        mesh.vertices.extend([
            egui::epaint::Vertex {
                pos: r.min,
                uv: egui::pos2(0.0, 0.0),
                color,
            },
            egui::epaint::Vertex {
                pos: egui::pos2(r.max.x, r.min.y),
                uv: egui::pos2(1.0, 0.0),
                color,
            },
            egui::epaint::Vertex {
                pos: r.max,
                uv: egui::pos2(1.0, 1.0),
                color,
            },
            egui::epaint::Vertex {
                pos: egui::pos2(r.min.x, r.max.y),
                uv: egui::pos2(0.0, 1.0),
                color,
            },
        ]);
        mesh.indices.extend([i, i + 1, i + 2, i, i + 2, i + 3]);
        mesh
    }
}
