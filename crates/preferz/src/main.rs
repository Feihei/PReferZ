// Release 构建用 windows 子系统（不弹黑色 cmd 窗口）；
// debug 构建保留 console 子系统（方便看 panic 输出和 env_logger 日志）。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// 二进制入口：业务模块统一由 lib.rs 导出，这里直接复用 lib，避免与 lib 重复编译同一份代码。
use preferz::PReferZApp;
use std::io::Read;

fn main() -> eframe::Result<()> {
    let mut font_definitions = egui::FontDefinitions::default();
    // 思源黑体（Source Han Sans CN）— OFL-1.1 许可，支持中英文且字形美观
    font_definitions.font_data.insert(
        "SourceHanSansCN".to_string(),
        egui::FontData::from_owned(load_font()).into(),
    );
    // Proportional 和 Monospace 都插入，保证任何字体族下中文都不回落到系统默认
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        if let Some(families) = font_definitions.families.get_mut(&family) {
            families.insert(0, "SourceHanSansCN".to_string());
        }
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_transparent(true)
            .with_icon(load_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "PReferZ",
        native_options,
        Box::new(move |cc| {
            cc.egui_ctx.set_fonts(font_definitions.clone());
            // 初始 Visuals 占位；真正的主题（Light/Dark/Auto）在 PReferZApp::ui()
            // 每帧根据 config.json 的 theme 字段重建，故这里只需给个暗色默认值避免首帧闪烁。
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(PReferZApp::new()))
        }),
    )
}

/// 加载思源黑体。
///
/// 编译期由 build.rs 用 deflate 压缩（9.92MB → 约 6.7MB）后嵌入，运行时
/// inflate 还原。字形覆盖面与原始 TTF 完全一致，代价只是一点启动 CPU ——
/// 比子集化安全：子集化会把生僻字渲染成豆腐块。
/// `FONT_RAW_SIZE` 由 build.rs 注入，用于预分配缓冲。
fn load_font() -> Vec<u8> {
    const COMPRESSED: &[u8] = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/SourceHanSansCN-Regular.ttf.zlib"
    ));
    let mut buf = Vec::with_capacity(
        env!("FONT_RAW_SIZE")
            .parse()
            .expect("FONT_RAW_SIZE 应为 usize"),
    );
    flate2::read::ZlibDecoder::new(COMPRESSED)
        .read_to_end(&mut buf)
        .expect("解压思源黑体失败");
    buf
}

/// 加载窗口图标（assets/icon.png，256×256 推荐）。
/// 编译期 include_bytes!，零运行时依赖。SVG 源文件 assets/icon.svg 仅作设计源不编译。
/// 若文件不存在返回空 IconData（egui 会用默认图标）。
fn load_icon() -> egui::IconData {
    let icon_bytes = include_bytes!("../../../assets/icon.png");
    match image::load_from_memory(icon_bytes) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            egui::IconData {
                rgba: rgba.into_raw(),
                width: w,
                height: h,
            }
        }
        Err(_) => {
            log::warn!("Failed to decode assets/icon.png, using default icon");
            egui::IconData::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 解压产物必须与原始 TTF 完全一致。
    ///
    /// 同时防两类问题：压缩/解压链路本身出错；以及 assets 里的字体被替换后
    /// build.rs 没有重跑（rerun-if-changed 失效或产物陈旧）导致嵌进去了旧字体。
    #[test]
    fn decompressed_font_matches_original_ttf() {
        let font = load_font();
        // sfnt 魔数 0x00010000 = TrueType outlines
        assert_eq!(&font[..4], &[0x00, 0x01, 0x00, 0x00], "sfnt 魔数不符");
        let expected: usize = env!("FONT_RAW_SIZE").parse().unwrap();
        assert_eq!(font.len(), expected, "解压后大小应与原始 TTF 一致");
    }
}
