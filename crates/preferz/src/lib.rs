pub mod i18n;
pub mod interaction;
pub mod keymap;
pub mod preferz_app;
pub mod theme;
pub mod ui;
pub mod viewport;

pub use preferz_app::PReferZApp;
/// egui 中手写字体族的注册名。main.rs 注册字体时用此名，
/// render.rs / text_edit.rs 通过 `FontFamily::Name(HANDWRITING_FONT_FAMILY)` 引用。
pub const HANDWRITING_FONT_FAMILY: &str = "Handwriting851";
