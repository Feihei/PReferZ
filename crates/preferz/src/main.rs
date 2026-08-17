// Release 构建用 windows 子系统（不弹黑色 cmd 窗口）；
// debug 构建保留 console 子系统（方便看 panic 输出和 env_logger 日志）。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod i18n;
mod interaction;
mod preferz_app;
mod ui;
mod viewport;

fn main() -> eframe::Result<()> {
    let mut font_definitions = egui::FontDefinitions::default();
    // 思源黑体（Source Han Sans CN）— OFL-1.1 许可，支持中英文且字形美观
    font_definitions.font_data.insert(
        "SourceHanSansCN".to_string(),
        egui::FontData::from_static(include_bytes!(
            "../../../assets/SourceHanSansCN-Regular.ttf"
        )),
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
            // 仅暗色主题（用户要求去掉亮色选项）
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(preferz_app::PReferZApp::new()))
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
