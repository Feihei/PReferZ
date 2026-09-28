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

/// 构建应用字体定义：思源黑体（默认族）+ 851 手写体（独立族，缺字回落思源黑体）。
///
/// 与 main.rs 启动时注册到主 `egui::Context` 的定义一致；导出选区的离屏
/// `egui::Context`（栅格化 PNG / 生成 SVG 预览）需要同一份定义才能渲染文字。
/// 每次调用会重新解压内嵌字体（约 27MB），调用方应缓存结果。
pub fn app_font_definitions() -> egui::FontDefinitions {
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

    // 851远星夜行手写体 — 免费商用（作者 Lakejason0 / 原作者 8:51:22 pm）
    // 仅注册为 FontFamily::Name，不插入 Proportional/Monospace 族——
    // 手写体只用于显式选择 Handwriting 的文字，不污染默认排版。
    font_definitions.font_data.insert(
        HANDWRITING_FONT_FAMILY.to_string(),
        egui::FontData::from_owned(load_handwriting_font()).into(),
    );
    // 手写族字体列表：子集缺字（生僻字/扩展区/颜文字符号）时回落思源黑体，
    // 避免渲染成豆腐块。egui 按列表顺序查字形。
    font_definitions.families.insert(
        egui::FontFamily::Name(HANDWRITING_FONT_FAMILY.into()),
        vec![
            HANDWRITING_FONT_FAMILY.to_string(),
            "SourceHanSansCN".to_string(),
        ],
    );
    font_definitions
}

/// 加载思源黑体。
///
/// 编译期由 build.rs 用 deflate 压缩（9.92MB → 约 6.7MB）后嵌入，运行时
/// inflate 还原。字形覆盖面与原始 TTF 完全一致，代价只是一点启动 CPU ——
/// 比子集化安全：子集化会把生僻字渲染成豆腐块。
/// `FONT_RAW_SIZE` 由 build.rs 注入，用于预分配缓冲。
fn load_font() -> Vec<u8> {
    use std::io::Read;
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

/// 加载 851远星夜行手写体。
///
/// 编译期由 build.rs 用 deflate 压缩（28MB → 约 18-22MB）后嵌入，运行时
/// inflate 还原。`HANDWRITING_FONT_RAW_SIZE` 由 build.rs 注入。
fn load_handwriting_font() -> Vec<u8> {
    use std::io::Read;
    const COMPRESSED: &[u8] = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/851LakeusNightWriting-Regular.ttf.zlib"
    ));
    let mut buf = Vec::with_capacity(
        env!("HANDWRITING_FONT_RAW_SIZE")
            .parse()
            .expect("HANDWRITING_FONT_RAW_SIZE 应为 usize"),
    );
    flate2::read::ZlibDecoder::new(COMPRESSED)
        .read_to_end(&mut buf)
        .expect("解压手写字体失败");
    buf
}
