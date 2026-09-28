// Release 构建用 windows 子系统（不弹黑色 cmd 窗口）；
// debug 构建保留 console 子系统（方便看 panic 输出和 env_logger 日志）。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// 二进制入口：业务模块统一由 lib.rs 导出，这里直接复用 lib，避免与 lib 重复编译同一份代码。
use preferz::PReferZApp;

fn main() -> eframe::Result<()> {
    // 字体定义构建已收口到 lib.rs（导出选区的离屏 egui Context 需要同一份定义）
    let font_definitions = preferz::app_font_definitions();

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
    /// 解压产物必须与原始 TTF 完全一致。
    ///
    /// 同时防两类问题：压缩/解压链路本身出错；以及 assets 里的字体被替换后
    /// build.rs 没有重跑（rerun-if-changed 失效或产物陈旧）导致嵌进去了旧字体。
    /// 检查经 `app_font_definitions()` 收口后的两个字体条目。
    #[test]
    fn decompressed_font_matches_original_ttf() {
        let defs = preferz::app_font_definitions();
        let font = defs.font_data["SourceHanSansCN"].font.clone().into_owned();
        // sfnt 魔数 0x00010000 = TrueType outlines
        assert_eq!(&font[..4], &[0x00, 0x01, 0x00, 0x00], "sfnt 魔数不符");
    }

    /// 手写字体解压一致性检查（同上）。
    #[test]
    fn decompressed_handwriting_font_matches_original_ttf() {
        let defs = preferz::app_font_definitions();
        let font = defs.font_data[preferz::HANDWRITING_FONT_FAMILY]
            .font
            .clone()
            .into_owned();
        assert_eq!(&font[..4], &[0x00, 0x01, 0x00, 0x00], "sfnt 魔数不符");
    }
}
