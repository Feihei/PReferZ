//! svg_export — 导出选区为 SVG 矢量文件。
//!
//! 形状来自离屏收集（`offscreen::collect_offscreen_scene`，zoom=1 → 画布像素
//! 1:1 映射 SVG 坐标），因此手绘风抖动、箭头、圆角、贝塞尔曲线与屏上观感一致。
//! Pixmap 不走形状转换（egui mesh 无法还原图像源），按 item 数据直接产出
//! `<image>`（原始字节 base64 内嵌，支持裁剪/灰度/透明度）。
//!
//! 已知取舍：文字逐行用 `<text>` 近似（字号按行高反推，手写体标记为 cursive
//! 回落），宿主环境无该字体时观感由查看器决定；字体不嵌入。

use super::*;

/// 把离屏收集的逐 item 形状组装成 SVG 文档。
///
/// `region`：选区画布包围盒（SVG viewBox 与之 1:1 对应）；`transparent` = false
/// 时铺白底；`image_bytes`：texture_id → 原始图片字节（image_data_cache）。
pub(crate) fn build_selection_svg(
    scene: &OffscreenScene,
    items: &[Item],
    region: CanvasRect,
    transparent: bool,
    image_bytes: &HashMap<u64, Vec<u8>>,
) -> String {
    let vw = region.width().ceil();
    let vh = region.height().ceil();

    let mut out = String::with_capacity(4096);
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n",
        vw, vh, vw, vh
    ));

    // 灰度/裁剪 defs 与唯一 id 池
    let mut defs = String::new();
    let mut gray_ids: HashMap<u64, String> = HashMap::new();
    let mut clip_ids: HashMap<u64, String> = HashMap::new();

    if !transparent {
        out.push_str(&format!(
            "<rect width=\"{vw}\" height=\"{vh}\" fill=\"#ffffff\"/>\n"
        ));
    }

    for (item, shapes) in items.iter().zip(&scene.item_shapes) {
        match &item.kind {
            ItemKind::Pixmap {
                ref texture_id,
                ref opacity,
                ref grayscale,
                ref crop,
                ..
            } => {
                let Some(bytes) = image_bytes.get(texture_id) else {
                    continue;
                };
                let Some(mime) = sniff_image_mime(bytes) else {
                    continue;
                };
                if *grayscale && !gray_ids.contains_key(texture_id) {
                    let id = format!("gray_{}", texture_id);
                    defs.push_str(&format!(
                        "<filter id=\"{id}\"><feColorMatrix type=\"saturate\" values=\"0\"/></filter>\n"
                    ));
                    gray_ids.insert(*texture_id, id);
                }
                if let Some(c) = crop {
                    if !clip_ids.contains_key(texture_id) {
                        let id = format!("clip_{}", texture_id);
                        defs.push_str(&format!(
                            "<clipPath id=\"{id}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath>\n",
                            c.x, c.y, c.width, c.height
                        ));
                        clip_ids.insert(*texture_id, id);
                    }
                }
                let base = item.base_size();
                let mut attrs = String::new();
                if *grayscale {
                    attrs.push_str(&format!(" filter=\"url(#{})\"", gray_ids[texture_id]));
                }
                if let Some(id) = clip_ids.get(texture_id) {
                    attrs.push_str(&format!(" clip-path=\"url(#{id})\""));
                }
                if *opacity < 1.0 {
                    attrs.push_str(&format!(" opacity=\"{:.3}\"", opacity.clamp(0.0, 1.0)));
                }
                let t = item.local_to_canvas();
                out.push_str(&format!(
                    "<g transform=\"matrix({:.4} {:.4} {:.4} {:.4} {:.4} {:.4})\">\
<image x=\"0\" y=\"0\" width=\"{:.2}\" height=\"{:.2}\" preserveAspectRatio=\"none\" href=\"data:{};base64,{}\"{attrs}/></g>\n",
                    t.m11, t.m12, t.m21, t.m22, t.m31, t.m32, base.x, base.y, mime,
                    base64_encode(bytes),
                ));
            }
            ItemKind::Frame { .. } => {} // 画框不导出（与场景导出采样器一致）
            _ => {
                for cs in shapes {
                    shape_to_svg(&cs.shape, &mut out);
                }
            }
        }
    }

    if !defs.is_empty() {
        out.insert_str(0, &format!("<defs>{defs}</defs>\n"));
    }
    out.push_str("</svg>\n");
    out
}

/// 单个 egui 形状 → SVG 元素。
fn shape_to_svg(shape: &egui::Shape, out: &mut String) {
    match shape {
        egui::Shape::Noop => {}
        egui::Shape::Vec(shapes) => {
            for s in shapes {
                shape_to_svg(s, out);
            }
        }
        egui::Shape::Circle(c) => {
            let (fill, fop) = fill_attr(c.fill);
            out.push_str(&format!(
                "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\"{fill}{fop} {}/>\n",
                c.center.x,
                c.center.y,
                c.radius,
                stroke_attr(&c.stroke),
            ));
        }
        egui::Shape::Ellipse(e) => {
            let (fill, fop) = fill_attr(e.fill);
            out.push_str(&format!(
                "<ellipse cx=\"{:.2}\" cy=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\"{fill}{fop} {}/>\n",
                e.center.x,
                e.center.y,
                e.radius.x,
                e.radius.y,
                stroke_attr(&e.stroke),
            ));
        }
        egui::Shape::LineSegment { points, stroke } => {
            out.push_str(&format!(
                "<path d=\"M {:.2} {:.2} L {:.2} {:.2}\" fill=\"none\" {}/>\n",
                points[0].x,
                points[0].y,
                points[1].x,
                points[1].y,
                stroke_attr(stroke),
            ));
        }
        egui::Shape::Path(p) => {
            let (fill, fop) = fill_attr(p.fill);
            let mut d = String::new();
            for (i, pt) in p.points.iter().enumerate() {
                d.push_str(&if i == 0 {
                    format!("M {:.2} {:.2} ", pt.x, pt.y)
                } else {
                    format!("L {:.2} {:.2} ", pt.x, pt.y)
                });
            }
            if p.closed && !p.points.is_empty() {
                d.push('Z');
            }
            // SVG path 默认填充黑色：无填充（透明）时必须显式 fill="none"
            let none = if fill.is_empty() {
                " fill=\"none\""
            } else {
                ""
            };
            out.push_str(&format!(
                "<path d=\"{d}\"{none}{fill}{fop} {}/>\n",
                path_stroke_attr(&p.stroke),
            ));
        }
        egui::Shape::Rect(r) => {
            let (fill, fop) = fill_attr(r.fill);
            out.push_str(&format!(
                "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"{:.2}\"{fill}{fop} {}/>\n",
                r.rect.min.x,
                r.rect.min.y,
                r.rect.width(),
                r.rect.height(),
                r.corner_radius.nw as f32,
                stroke_attr(&r.stroke),
            ));
        }
        egui::Shape::QuadraticBezier(q) => {
            out.push_str(&format!(
                "<path d=\"M {:.2} {:.2} Q {:.2} {:.2} {:.2} {:.2}\" fill=\"none\" {}/>\n",
                q.points[0].x,
                q.points[0].y,
                q.points[1].x,
                q.points[1].y,
                q.points[2].x,
                q.points[2].y,
                path_stroke_attr(&q.stroke),
            ));
        }
        egui::Shape::CubicBezier(c) => {
            out.push_str(&format!(
                "<path d=\"M {:.2} {:.2} C {:.2} {:.2} {:.2} {:.2} {:.2} {:.2}\" fill=\"none\" {}/>\n",
                c.points[0].x, c.points[0].y, c.points[1].x, c.points[1].y, c.points[2].x,
                c.points[2].y, c.points[3].x, c.points[3].y,
                path_stroke_attr(&c.stroke),
            ));
        }
        egui::Shape::Text(t) => {
            galley_to_svg(t, out);
        }
        // Mesh（裸网格，理论上 Pixmap 已在 build_selection_svg 单独处理）与
        // Callback/Image 无法语义化还原，跳过。
        _ => {}
    }
}

/// Galley → 逐行 `<text>`（字号取首个排版段的字体定义；行文字由 glyph 还原）。
fn galley_to_svg(t: &egui::epaint::TextShape, out: &mut String) {
    let galley = &t.galley;
    let font_size = galley
        .job
        .sections
        .first()
        .map(|s| s.format.font_id.size)
        .unwrap_or(14.0);
    let family = galley
        .job
        .sections
        .first()
        .map(|s| match &s.format.font_id.family {
            egui::FontFamily::Name(n) if n.as_ref() == crate::HANDWRITING_FONT_FAMILY => {
                "cursive".to_string()
            }
            _ => "sans-serif".to_string(),
        })
        .unwrap_or_else(|| "sans-serif".to_string());
    // 行色：取行 mesh 首个顶点色（fallback 色作兜底）
    for row in &galley.rows {
        let text = row.row.text();
        if text.is_empty() {
            continue;
        }
        let color = row
            .row
            .visuals
            .mesh
            .vertices
            .first()
            .map(|v| v.color)
            .unwrap_or(t.fallback_color);
        if color.a() == 0 {
            continue;
        }
        let x = t.pos.x + row.pos.x;
        // glyph.pos.y 是基线（相对行顶），直接取首 glyph 基线得到精确 y
        let baseline = row
            .row
            .glyphs
            .first()
            .map(|g| g.pos.y)
            .unwrap_or(row.row.size.y * 0.8);
        let y = t.pos.y + row.pos.y + baseline;
        out.push_str(&format!(
            "<text x=\"{x:.2}\" y=\"{y:.2}\" font-family=\"{family}\" font-size=\"{font_size:.2}\" fill=\"rgb({},{},{})\" fill-opacity=\"{:.3}\" xml:space=\"preserve\">{}</text>\n",
            color.r(),
            color.g(),
            color.b(),
            color.a() as f32 / 255.0,
            xml_escape(&text),
        ));
    }
}

/// 填充色 → `fill` 属性（TRANSPARENT 返回空；预乘色按逆预乘直通色输出）。
fn fill_attr(fill: egui::Color32) -> (String, String) {
    if fill == egui::Color32::TRANSPARENT || fill.a() == 0 {
        (String::new(), String::new())
    } else {
        (
            format!(" fill=\"rgb({},{},{})\"", fill.r(), fill.g(), fill.b()),
            format!(" fill-opacity=\"{:.3}\"", fill.a() as f32 / 255.0),
        )
    }
}

fn stroke_attr(stroke: &egui::Stroke) -> String {
    if stroke.is_empty() {
        return String::new();
    }
    stroke_attrs_from(stroke.width, stroke.color)
}

/// Path 体系的描边（颜色可能是 UV 回调，此时跳过颜色只保留宽度）。
fn path_stroke_attr(stroke: &egui::epaint::PathStroke) -> String {
    match stroke.color {
        egui::epaint::ColorMode::Solid(c) => stroke_attrs_from(stroke.width, c),
        egui::epaint::ColorMode::UV(_) => String::new(),
    }
}

fn stroke_attrs_from(width: f32, c: egui::Color32) -> String {
    if width <= 0.0 || c.a() == 0 {
        return String::new();
    }
    format!(
        "stroke=\"rgb({},{},{})\" stroke-opacity=\"{:.3}\" stroke-width=\"{:.2}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"",
        c.r(),
        c.g(),
        c.b(),
        c.a() as f32 / 255.0,
        width,
    )
}

/// XML 转义（`&`/`<`/`>`/引号）。
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// 简易 base64（无新增依赖；RFC 4648 标准 alphabet + padding）。
pub(crate) fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let (full, rest) = data.as_chunks::<3>();
    for chunk in full {
        let n = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | chunk[2] as u32;
        for shift in [18, 12, 6, 0] {
            out.push(TABLE[((n >> shift) & 63) as usize] as char);
        }
    }
    match rest {
        [] => {}
        [a] => {
            let n = (*a as u32) << 16;
            out.push(TABLE[((n >> 18) & 63) as usize] as char);
            out.push(TABLE[((n >> 12) & 63) as usize] as char);
            out.push_str("==");
        }
        [a, b] => {
            let n = ((*a as u32) << 16) | ((*b as u32) << 8);
            for shift in [18, 12, 6] {
                out.push(TABLE[((n >> shift) & 63) as usize] as char);
            }
            out.push('=');
        }
        _ => unreachable!("as_chunks::<3> 余数只可能是 0/1/2 字节"),
    }
    out
}

/// 魔数嗅探图片 MIME（支持内嵌 data URI 的常见格式）。
fn sniff_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else if bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") {
        Some("image/svg+xml")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn xml_escape_escapes_specials() {
        assert_eq!(xml_escape("a<b>&\"c\""), "a&lt;b&gt;&amp;&quot;c&quot;");
    }

    #[test]
    fn sniff_detects_common_formats() {
        assert_eq!(
            sniff_image_mime(&[0x89, b'P', b'N', b'G', 0]),
            Some("image/png")
        );
        assert_eq!(sniff_image_mime(&[0xff, 0xd8, 0x00]), Some("image/jpeg"));
        assert_eq!(sniff_image_mime(b"RIFF1234WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff_image_mime(b"nope"), None);
    }
}
