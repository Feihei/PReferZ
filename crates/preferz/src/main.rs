mod viewport;
mod interaction;
mod preferz_app;
mod ui;

fn main() -> eframe::Result<()> {
    let mut font_definitions = egui::FontDefinitions::default();
    font_definitions.font_data.insert(
        "SimHei".to_string(),
        egui::FontData::from_static(include_bytes!("../../../assets/simhei.ttf")).into()
    );
    if let Some(families) = font_definitions.families.get_mut(&egui::FontFamily::Proportional) {
        families.insert(0, "SimHei".to_string());
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "PReferZ",
        native_options,
        Box::new(move |cc| {
            cc.egui_ctx.set_fonts(font_definitions.clone());
            Ok(Box::new(preferz_app::PReferZApp::new()))
        }),
    )
}

