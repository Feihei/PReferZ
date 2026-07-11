use std::path::Path;

pub struct Exporter;

impl Exporter {
    pub fn export_scene(
        _scene: &preferz_core::Scene,
        _output_path: &Path,
        _format: ExportFormat,
        _width: u32,
        _height: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 简化实现
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    Png,
    Jpeg,
    Svg,
}

impl ExportFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Png => "png",
            ExportFormat::Jpeg => "jpg",
            ExportFormat::Svg => "svg",
        }
    }
}
