//! 极简 i18n：枚举查表方案（无外部依赖）。
//!
//! 设计：`Lang` 枚举 + `t(lang, key)` 函数，所有文案集中在 `TRANSLATIONS` 表。
//! 新增语言只需扩展 `Lang` 变体 + `TRANSLATIONS` 对应分支。

use serde::{Deserialize, Serialize};

use crate::keymap::Action;

/// 支持的语言。默认 `En`，可在设置面板切换为 `Zh`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Lang {
    #[default]
    En,
    Zh,
}

impl Lang {
    pub fn display_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Zh => "中文",
        }
    }

    /// 悬浮 HUD 语言按钮用的极短标签（`EN` / `中`），比 `display_name` 省地方。
    pub fn short_name(self) -> &'static str {
        match self {
            Lang::En => "EN",
            Lang::Zh => "中",
        }
    }

    /// 切换到另一种语言。
    pub fn toggled(self) -> Lang {
        match self {
            Lang::En => Lang::Zh,
            Lang::Zh => Lang::En,
        }
    }
}

/// 翻译 key。新增文案在此追加变体，然后在 `translate` 中提供两种语言文案。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum T {
    // ── 右键菜单 ──
    NewCanvas,
    OpenProject,
    LoadImage,
    PasteImage, // 含 {0} 快捷键占位
    Save,
    SaveAs,
    ExportScene,
    ExportImagesToDir,
    DeleteSelected,
    Copy, // 复制选中（右键菜单，有选中时才显示）
    Cut,  // 剪切选中（右键菜单，有选中时才显示）
    BringToFront,
    SendToBack,
    CropMode,
    NormalizeSize,
    Arrange,
    NormalizeByWidth,
    NormalizeByHeight,
    NormalizeByArea,
    ArrangeLinear,
    ArrangeGrid,
    ArrangeOptimal,
    Settings,
    FitToCanvas,
    ResetZoom,
    Exit,
    ToggleGrayscale,
    CancelGrayscale,
    ColorPickerMode,
    ExitColorPicker,
    // ── 导出子菜单 ──
    ExportPngAll,
    ExportJpgAll,
    ExportPngSelection,
    ExportJpgSelection,
    ExportAllImages,
    ExportSelectionImages,
    // ── flash 消息 ──
    FlashTextCreated,
    FlashTextUpdated,
    FlashUndo,
    FlashRedo,
    FlashNewCanvas,
    FlashNoClipboardImage,
    FlashCanvasEmptyNoExport,
    FlashNoSelectionToExport,
    FlashPixelDataNotReady,
    FlashResetZoom,
    FlashBroughtToFront,
    FlashSentToBack,
    FlashFitToCanvas,
    FlashZoomToSelection,
    FlashZoom100,
    FlashNoSelection,
    FlashFitRestore,
    FlashToggleGrayscale,
    FlashCropNeedSingleImage,
    FlashCropImageOnly,
    FlashCropHint,
    FlashCropNoChange,
    FlashCropInvalidSize,
    FlashCropApplied,
    FlashCropCancelled,
    FlashColorPickerMissed,
    FlashColorPickerImageOnly,
    FlashColorPickerHint, // 取色器模式：单击图片采样 · Esc 退出
    FlashNormalizeNeedMultiple,
    FlashExportedTo,           // 已导出到 {path}
    FlashExportImagesTo,       // 导出图片到 {path}
    FlashPasteImage,           // 粘贴图片（进度条）
    FlashExportProgress,       // 导出: {path}（进度条）
    FlashExportImagesProgress, // 导出图片到: {path}（进度条）
    FlashExportedNImages,      // 已导出 {n} 个图片到 {path}
    FlashExportFailed,         // 导出失败: {err}
    FlashProcessing,           // 处理中...（进度条默认）
    FlashDuplicated,           // 已复制 {0} 个元素（Ctrl+拖动 / Ctrl+D）
    FlashCopied,               // 已复制 {0} 项（Ctrl+C）
    FlashCut,                  // 已剪切 {0} 项（Ctrl+X）
    // ── flash / 进度条文案（原散落在 preferz_app.rs 里写死中文，收进表内）──
    FlashSaved,           // 已保存: {0}
    FlashSaveFailed,      // 保存失败: {0}
    FlashOpened,          // 已打开: {0}
    FlashOpenFailed,      // 打开失败: {0}
    FlashImported,        // 已导入: {0}
    FlashImportFailed,    // 导入失败 {0}: {1}
    FlashClipboardFailed, // 剪贴板访问失败: {0}
    ProgressImportImage,  // 导入图片: {0}（进度条）
    ProgressOpenFile,     // 打开文件: {0}（进度条）
    ProgressSaveFile,     // 保存文件: {0}（进度条）
    FlashFlipH,           // 水平翻转
    FlashFlipV,           // 垂直翻转
    FlashTransform,       // 变换: 缩放=({0}, {1}) 旋转={2}°
    FlashMoved,           // 移动: ({0}, {1})
    FlashDeleted,         // 已删除 {0} 项
    FlashShapeCreated,    // 已创建图形
    FlashLineCreated,     // 已创建直线
    FlashArrowCreated,    // 已创建箭头
    FlashFrameCreated,    // 已创建画框 #{0}
    FlashFrameRenumbered, // 画框编号 → #{0}
    // ── 图表粘贴（plan #8）──
    ChartChooserTitle,       // 检测到两列数据，粘贴为图表
    ChartChooserBar,         // 柱状图
    ChartChooserLine,        // 折线图
    ChartChooserCancel,      // 取消
    ChartChooserMore,        // …还有 {0} 行
    FlashChartCreated,       // 已创建图表
    FlashNoSelectedImages,   // 无选中的图片项
    FlashNoExportableImages, // 无可导出的图片项
    FlashArranged,           // 排列：{0}
    FlashAligned,            // 对齐：{0}
    FlashDistributed,        // 分布：{0}
    FlashNormalized,         // 归一化：{0}
    ArrangeModeLinear,       // 线形（toast 用，不带菜单里的快捷键提示）
    ArrangeModeGrid,         // 网格
    ArrangeModeOptimal,      // 最优装箱
    // ── 欢迎页 ──
    WelcomeTitle, // PReferZ（固定不翻译）
    WelcomeSubtitle,
    WelcomeRecentFiles,
    WelcomeHint,
    // ── 设置面板 ──
    SettingsTitle,
    SettingsArrange,
    SettingsSpacing,
    SettingsWindow,
    SettingsAlwaysOnTop,
    SettingsFrameless,
    SettingsBgAlpha,
    SettingsLanguage,
    SettingsTheme,
    SettingsShortcuts,
    SettingsKeymapHint,
    SettingsKeymapCapturing,
    SettingsKeymapUnbound,
    SettingsKeymapRestore,
    SettingsKeymapConflict,
    // ── 保存提示对话框 ──
    SavePromptMessage,
    SavePromptSave,
    SavePromptDiscard,
    SavePromptCancel,
    // ── 绘制工具/样式面板 ──
    ToolSelect,
    ToolRectangle,
    ToolEllipse,
    ToolDiamond,
    ToolLine,
    ToolArrow,
    #[allow(dead_code)]
    ToolFrame,
    /// 多边形工具（Phase I）。
    ToolPolygon,
    /// 徒手绘制工具（plan #10）。
    ToolFreehand,
    /// 墨迹创建成功反馈（plan #10）。
    FlashFreedrawCreated,
    /// 多边形收尾反馈：顶点不足 3 个时无法成面（Phase I）。
    PolygonTooFewPoints,
    /// 多边形创建成功反馈（Phase I）。
    PolygonCreated,
    Present,
    PresentNoFrames,
    StyleStrokeColor,
    StyleStrokeWidth,
    StyleDashSolid,
    StyleDashDashed,
    StyleDashDotted,
    StyleFillNone,
    /// 填充节标题。
    StyleFillLabel,
    /// 填充不透明度滑块（plan #2）。
    StyleFillOpacity,
    /// 填充样式三态（无填充复用 StyleFillNone）。
    StyleFillSolid,
    StyleFillHachure,
    StyleFillCrossHatch,
    /// 绘制工具激活时的默认样式侧栏标题。
    PropsDefaultsTitle,
    StyleClosed,
    // ── 箭头样式 ──
    StyleArrowStart,
    StyleArrowEnd,
    /// 箭头三态：无 / 箭头 / 圆点（Phase I）。
    StyleArrowNone,
    StyleArrowArrow,
    StyleArrowDot,
    // ── 曲线 / 圆角（Phase I）──
    StyleCurve,
    StyleCurveStraight,
    StyleCurveCurved,
    StyleRoundness,
    // ── 属性侧栏（Phase H）──
    /// 面板标题里的选中数量：`已选 {0} 项`。
    PropsSelectedCount,
    /// 选中项类型不一致时的提示。
    PropsMixedSelection,
    /// 多选时某属性值各不相同的标注（Excalidraw 的 "Mixed"）。
    PropsMixedValue,
    /// 分节标题：形状 / 文字 / 图片 / 画框。
    PropsSectionShape,
    PropsSectionFreedraw,
    PropsSectionText,
    PropsSectionPixmap,
    PropsSectionFrame,
    /// 画框节：编号字段的标签。
    PropsFrameNumber,
    // ── 对齐 / 分布（plan #6）──
    /// 属性栏分节标题：对齐。
    PropsSectionAlign,
    /// 6 向对齐。
    AlignLeft,
    AlignHCenter,
    AlignRight,
    AlignTop,
    AlignVCenter,
    AlignBottom,
    /// 分节标题：分布。
    Distribute,
    /// 分布四选：轴向 × 基准（等距 = 边界空隙相等；等心 = 中心距相等）。
    DistributeHGap,
    DistributeHCenters,
    DistributeVGap,
    DistributeVCenters,
    /// 分布需要 ≥3 个元素（不足时按钮禁用并提示）。
    FlashDistributeNeedThree,
    // ── 端点吸附（plan #5）──
    /// 线/箭头端点吸附到图形边缘时的提示。
    FlashSnappedToShape,
    // ── 多边形顶点编辑（plan #4）──
    /// Alt+单击删除顶点成功。
    FlashVertexDeleted,
    /// 开放折线顶点数已达下限，拒绝删除。
    FlashVertexMinOpen,
    /// 闭合多边形顶点数已达下限，拒绝删除。
    FlashVertexMinClosed,
    /// 端点拖回起点附近，自动闭合成多边形。
    FlashPolygonClosed,
    /// 拖开闭合合并点，自动恢复开放。
    FlashPolygonOpened,
    /// HUD 主题切换按钮 hover 提示。
    ThemeToggleHint,
    // ── 编组/解组（plan #13）──
    /// 编组成功提示。
    FlashGrouped,
    /// 解组成功提示。
    FlashUngrouped,
    /// 右键菜单：编组。
    MenuGroup,
    /// 右键菜单：解组。
    MenuUngroup,
    // ── 手绘风（Phase F）──
    StyleRough,
    /// 手绘风档位四选一（plan #3，对齐 Excalidraw sloppiness）。
    SloppinessOff,
    SloppinessArchitect,
    SloppinessArtist,
    SloppinessCartoonist,
    // ── 属性侧栏文字/图片节（Phase H）──
    StyleTextBackground,
    StyleGrayscale,
    // ── 文字样式（plan #1：对齐 + 伪手写字体）──
    StyleFontLabel,
    FontNormal,
    FontHandwriting,
    StyleAlignH,
    StyleAlignV,
    TextAlignLeft,
    TextAlignCenter,
    TextAlignRight,
    TextAlignTop,
    TextAlignMiddle,
    TextAlignBottom,
    // ── 画框比例预设（plan #3）──
    /// 属性栏：画框比例/纸张预设下拉的分节标签。
    FramePresetLabel,
    /// 下拉框未选中任何预设时的占位项。
    FramePresetPick,
    /// 预设：演示比例（纯比例，保持当前长边长度）。
    Preset16x9,
    Preset16x10,
    Preset4x3,
    Preset3x2,
    Preset1x1,
    /// 预设：A4 纸张（96 DPI 绝对尺寸，竖 / 横）。
    PresetA4Portrait,
    PresetA4Landscape,
    /// 套用预设后的提示。
    FlashFramePresetApplied,
}

/// 查表翻译。未命中的 key 返回 debug 字符串（开发期易发现遗漏）。
pub fn t(lang: Lang, key: T) -> &'static str {
    match lang {
        Lang::En => translate_en(key),
        Lang::Zh => translate_zh(key),
    }
}

fn translate_en(key: T) -> &'static str {
    match key {
        // 右键菜单
        T::NewCanvas => "New Canvas",
        T::OpenProject => "Open Project...",
        T::LoadImage => "Load Image...",
        T::PasteImage => "Paste Image ({0})",
        T::Save => "Save",
        T::SaveAs => "Save As...",
        T::ExportScene => "Export Scene",
        T::ExportImagesToDir => "Export Images to Folder",
        T::Copy => "Copy",
        T::Cut => "Cut",
        T::DeleteSelected => "Delete Selected",
        T::BringToFront => "Bring to Front",
        T::SendToBack => "Send to Back",
        T::CropMode => "Crop Mode...",
        T::NormalizeSize => "Normalize Size",
        T::Arrange => "Arrange",
        T::NormalizeByWidth => "By Width",
        T::NormalizeByHeight => "By Height",
        T::NormalizeByArea => "By Area",
        T::ArrangeLinear => "Linear (R)",
        T::ArrangeGrid => "Grid (G)",
        T::ArrangeOptimal => "Optimal Packing (O)",
        T::Settings => "Settings...",
        T::FitToCanvas => "Fit to Canvas",
        T::ResetZoom => "Reset Zoom",
        T::Exit => "Exit",
        T::ToggleGrayscale => "Toggle Grayscale",
        T::CancelGrayscale => "Cancel Grayscale",
        T::ColorPickerMode => "Color Picker Mode",
        T::ExitColorPicker => "Exit Color Picker",
        // 导出子菜单
        T::ExportPngAll => "PNG (All)...",
        T::ExportJpgAll => "JPG (All)...",
        T::ExportPngSelection => "PNG (Selection)...",
        T::ExportJpgSelection => "JPG (Selection)...",
        T::ExportAllImages => "All Images...",
        T::ExportSelectionImages => "Selection Images...",
        // flash 消息
        T::FlashTextCreated => "Text note created",
        T::FlashTextUpdated => "Text note updated",
        T::FlashUndo => "Undo",
        T::FlashRedo => "Redo",
        T::FlashNewCanvas => "New canvas",
        T::FlashNoClipboardImage => "No image in clipboard",
        T::FlashCanvasEmptyNoExport => "Canvas is empty, nothing to export",
        T::FlashNoSelectionToExport => "No selection to export",
        T::FlashPixelDataNotReady => "Image pixel data not ready, please retry",
        T::FlashResetZoom => "Reset zoom",
        T::FlashBroughtToFront => "Brought to front",
        T::FlashSentToBack => "Sent to back",
        T::FlashFitToCanvas => "Fit to canvas",
        T::FlashZoomToSelection => "Zoom to selection",
        T::FlashZoom100 => "Zoom 100%",
        T::FlashNoSelection => "Nothing selected",
        T::FlashFitRestore => "Restored previous view",
        T::FlashToggleGrayscale => "Grayscale toggled",
        T::FlashCropNeedSingleImage => "Crop requires a single selected image",
        T::FlashCropImageOnly => "Only images support cropping",
        T::FlashCropHint => "Crop mode: drag corners · {0} to apply · {1} to cancel",
        T::FlashCropNoChange => "Crop unchanged",
        T::FlashCropInvalidSize => "Invalid crop size",
        T::FlashCropApplied => "Crop applied",
        T::FlashCropCancelled => "Crop cancelled",
        T::FlashColorPickerMissed => "No image hit",
        T::FlashColorPickerImageOnly => "Only images support sampling",
        T::FlashColorPickerHint => "Color picker: click image to sample · {0} to exit",
        T::FlashNormalizeNeedMultiple => "Normalize requires multiple selected images",
        T::FlashExportedTo => "Exported to",          // 后接路径
        T::FlashExportImagesTo => "Export images to", // 后接路径
        T::FlashPasteImage => "Pasting image",
        T::FlashExportProgress => "Export", // 后接路径
        T::FlashExportImagesProgress => "Export images to", // 后接路径
        T::FlashExportedNImages => "Exported", // 后接数量+路径
        T::FlashExportFailed => "Export failed", // 后接错误
        T::FlashProcessing => "Processing...",
        T::FlashDuplicated => "Duplicated {0} elements",
        T::FlashCopied => "Copied {0} item(s)",
        T::FlashCut => "Cut {0} item(s)",
        T::FlashSaved => "Saved {0}",
        T::FlashSaveFailed => "Save failed: {0}",
        T::FlashOpened => "Opened {0}",
        T::FlashOpenFailed => "Open failed: {0}",
        T::FlashImported => "Imported {0}",
        T::FlashImportFailed => "Import failed {0}: {1}",
        T::FlashClipboardFailed => "Clipboard access failed: {0}",
        T::ProgressImportImage => "Importing image {0}",
        T::ProgressOpenFile => "Opening file {0}",
        T::ProgressSaveFile => "Saving file {0}",
        T::FlashFlipH => "Flipped horizontally",
        T::FlashFlipV => "Flipped vertically",
        T::FlashTransform => "Transform: scale=({0}, {1}) rotation={2}°",
        T::FlashMoved => "Moved ({0}, {1})",
        T::FlashDeleted => "Deleted {0} item(s)",
        T::FlashShapeCreated => "Shape created",
        T::FlashLineCreated => "Line created",
        T::FlashArrowCreated => "Arrow created",
        T::FlashFrameCreated => "Frame created #{0}",
        T::FlashFrameRenumbered => "Frame renumbered → #{0}",
        T::ChartChooserTitle => "Two-column data detected — paste as chart",
        T::ChartChooserBar => "Bar chart",
        T::ChartChooserLine => "Line chart",
        T::ChartChooserCancel => "Cancel",
        T::ChartChooserMore => "…and {0} more rows",
        T::FlashChartCreated => "Chart created",
        T::FlashNoSelectedImages => "No selected image items",
        T::FlashNoExportableImages => "No exportable image items",
        T::FlashArranged => "Arranged: {0}",
        T::FlashAligned => "Align: {0}",
        T::FlashDistributed => "Distributed: {0}",
        T::FlashNormalized => "Normalized: {0}",
        T::ArrangeModeLinear => "Linear",
        T::ArrangeModeGrid => "Grid",
        T::ArrangeModeOptimal => "Optimal packing",
        // 欢迎页
        T::WelcomeTitle => "PReferZ",
        T::WelcomeSubtitle => "Reference image board · right-click to start",
        T::WelcomeRecentFiles => "Recent Files",
        T::WelcomeHint => "Right-click → Open Project / Load Image / Paste Image",
        // 设置面板
        T::SettingsTitle => "Settings",
        T::SettingsArrange => "Arrange",
        T::SettingsSpacing => "Spacing",
        T::SettingsWindow => "Window",
        T::SettingsAlwaysOnTop => "Always on Top",
        T::SettingsFrameless => "Frameless Float Mode",
        T::SettingsBgAlpha => "Background Opacity",
        T::SettingsLanguage => "Language",
        T::SettingsTheme => "Theme",
        T::SettingsShortcuts => "Shortcuts",
        T::SettingsKeymapHint => "Click a shortcut, then press the new combination.",
        T::SettingsKeymapCapturing => "Press the new combination...",
        T::SettingsKeymapUnbound => "Unbound",
        T::SettingsKeymapRestore => "Restore defaults",
        T::SettingsKeymapConflict => "Conflicts with {0} — overridden",
        // 保存提示
        T::SavePromptMessage => "Canvas has unsaved changes. Save?",
        T::SavePromptSave => "Save",
        T::SavePromptDiscard => "Discard",
        T::SavePromptCancel => "Cancel",
        // 绘制工具/样式面板
        T::ToolSelect => "Select",
        T::ToolRectangle => "Rectangle",
        T::ToolEllipse => "Ellipse",
        T::ToolDiamond => "Diamond",
        T::ToolLine => "Line",
        T::ToolArrow => "Arrow",
        T::ToolFrame => "Frame",
        T::ToolPolygon => "Polygon",
        T::ToolFreehand => "Freedraw",
        T::FlashFreedrawCreated => "Ink created",
        T::PolygonTooFewPoints => "A polygon needs at least 3 points — discarded.",
        T::PolygonCreated => "Polygon created",
        T::Present => "Present slides ({0})",
        T::PresentNoFrames => "No frames to present. Create a frame first (Tool ▢).",
        T::StyleStrokeColor => "Stroke color",
        T::StyleStrokeWidth => "Stroke width",
        T::StyleDashSolid => "Solid",
        T::StyleDashDashed => "Dashed",
        T::StyleDashDotted => "Dotted",
        T::StyleFillNone => "No fill",
        T::StyleFillLabel => "Fill",
        T::StyleFillOpacity => "Opacity",
        T::StyleFillSolid => "Solid",
        T::StyleFillHachure => "Hachure",
        T::StyleFillCrossHatch => "Cross-hatch",
        T::PropsDefaultsTitle => "Default style for new elements",
        T::StyleClosed => "Closed",
        T::StyleArrowStart => "Start arrow",
        T::StyleArrowEnd => "End arrow",
        T::StyleArrowNone => "None",
        T::StyleArrowArrow => "Arrow",
        T::StyleArrowDot => "Dot",
        T::StyleCurve => "Edges",
        T::StyleCurveStraight => "Sharp",
        T::StyleCurveCurved => "Round",
        T::StyleRoundness => "Roundness",
        T::PropsSelectedCount => "{0} selected",
        T::PropsMixedSelection => "Mixed types — no shared properties to edit.",
        T::PropsMixedValue => "Mixed",
        T::PropsSectionShape => "Shape",
        T::PropsSectionFreedraw => "Ink",
        T::PropsSectionText => "Text",
        T::PropsSectionPixmap => "Image",
        T::PropsSectionFrame => "Frame",
        T::PropsFrameNumber => "Number",
        T::PropsSectionAlign => "Align",
        T::AlignLeft => "Align left",
        T::AlignHCenter => "Center horizontally",
        T::AlignRight => "Align right",
        T::AlignTop => "Align top",
        T::AlignVCenter => "Center vertically",
        T::AlignBottom => "Align bottom",
        T::Distribute => "Distribute",
        T::DistributeHGap => "H gap",
        T::DistributeHCenters => "H centers",
        T::DistributeVGap => "V gap",
        T::DistributeVCenters => "V centers",
        T::FlashDistributeNeedThree => "Distribute needs at least 3 elements",
        T::FlashSnappedToShape => "Snapped to shape",
        T::FlashVertexDeleted => "Vertex deleted",
        T::FlashVertexMinOpen => "An open line needs at least 2 points",
        T::FlashVertexMinClosed => "A closed shape needs at least 3 points",
        T::FlashPolygonClosed => "Closed into a polygon",
        T::FlashPolygonOpened => "Reopened",
        T::ThemeToggleHint => "Toggle light / dark theme",
        T::FlashGrouped => "Grouped",
        T::FlashUngrouped => "Ungrouped",
        T::MenuGroup => "Group",
        T::MenuUngroup => "Ungroup",
        T::StyleRough => "Hand-drawn",
        T::SloppinessOff => "Off",
        T::SloppinessArchitect => "Architect",
        T::SloppinessArtist => "Artist",
        T::SloppinessCartoonist => "Cartoonist",
        T::StyleTextBackground => "Background",
        T::StyleGrayscale => "Grayscale",
        T::StyleFontLabel => "Font",
        T::FontNormal => "Normal",
        T::FontHandwriting => "Handwritten",
        T::StyleAlignH => "H-Align",
        T::StyleAlignV => "V-Align",
        T::TextAlignLeft => "Left",
        T::TextAlignCenter => "Center",
        T::TextAlignRight => "Right",
        T::TextAlignTop => "Top",
        T::TextAlignMiddle => "Middle",
        T::TextAlignBottom => "Bottom",
        T::FramePresetLabel => "Aspect / paper preset",
        T::FramePresetPick => "Choose preset...",
        T::Preset16x9 => "16:9",
        T::Preset16x10 => "16:10",
        T::Preset4x3 => "4:3",
        T::Preset3x2 => "3:2",
        T::Preset1x1 => "1:1",
        T::PresetA4Portrait => "A4 portrait",
        T::PresetA4Landscape => "A4 landscape",
        T::FlashFramePresetApplied => "Applied preset: {0}",
    }
}

fn translate_zh(key: T) -> &'static str {
    match key {
        // 右键菜单
        T::NewCanvas => "新建画布",
        T::OpenProject => "打开项目...",
        T::LoadImage => "载入图片...",
        T::PasteImage => "粘贴图片 ({0})",
        T::Save => "保存",
        T::SaveAs => "另存为...",
        T::ExportScene => "导出场景",
        T::ExportImagesToDir => "导出图片到目录",
        T::Copy => "复制",
        T::Cut => "剪切",
        T::DeleteSelected => "删除选中",
        T::BringToFront => "置于顶层",
        T::SendToBack => "置于底层",
        T::CropMode => "裁剪模式...",
        T::NormalizeSize => "归一化尺寸",
        T::Arrange => "排列",
        T::NormalizeByWidth => "按宽度",
        T::NormalizeByHeight => "按高度",
        T::NormalizeByArea => "按面积",
        T::ArrangeLinear => "线形 (R)",
        T::ArrangeGrid => "网格 (G)",
        T::ArrangeOptimal => "最优装箱 (O)",
        T::Settings => "设置...",
        T::FitToCanvas => "适应画布",
        T::ResetZoom => "重置缩放",
        T::Exit => "退出",
        T::ToggleGrayscale => "切换灰度",
        T::CancelGrayscale => "取消灰度",
        T::ColorPickerMode => "取色器模式",
        T::ExitColorPicker => "退出取色器",
        // 导出子菜单
        T::ExportPngAll => "PNG (全部)...",
        T::ExportJpgAll => "JPG (全部)...",
        T::ExportPngSelection => "PNG (仅选中)...",
        T::ExportJpgSelection => "JPG (仅选中)...",
        T::ExportAllImages => "全部图片...",
        T::ExportSelectionImages => "仅选中图片...",
        // flash 消息
        T::FlashTextCreated => "已创建文本便签",
        T::FlashTextUpdated => "已更新文本便签",
        T::FlashUndo => "撤销",
        T::FlashRedo => "重做",
        T::FlashNewCanvas => "新建画布",
        T::FlashNoClipboardImage => "剪贴板中无图片",
        T::FlashCanvasEmptyNoExport => "画布为空，无需导出",
        T::FlashNoSelectionToExport => "无选中项可导出",
        T::FlashPixelDataNotReady => "图片像素数据未就绪，请稍后再试",
        T::FlashResetZoom => "重置缩放",
        T::FlashBroughtToFront => "置于顶层",
        T::FlashSentToBack => "置于底层",
        T::FlashFitToCanvas => "适应画布",
        T::FlashZoomToSelection => "缩放到选中元素",
        T::FlashZoom100 => "缩放 100%",
        T::FlashNoSelection => "未选中元素",
        T::FlashFitRestore => "已恢复上一视图",
        T::FlashToggleGrayscale => "切换灰度",
        T::FlashCropNeedSingleImage => "裁剪需要选中单个图片",
        T::FlashCropImageOnly => "仅图片支持裁剪",
        T::FlashCropHint => "裁剪模式：拖拽角点 · {0} 应用 · {1} 取消",
        T::FlashCropNoChange => "裁剪未变化",
        T::FlashCropInvalidSize => "裁剪尺寸无效",
        T::FlashCropApplied => "已应用裁剪",
        T::FlashCropCancelled => "取消裁剪",
        T::FlashColorPickerMissed => "未命中图片",
        T::FlashColorPickerImageOnly => "仅图片支持采样",
        T::FlashColorPickerHint => "取色器模式：单击图片采样 · {0} 退出",
        T::FlashNormalizeNeedMultiple => "归一化需要选中多个图片",
        T::FlashExportedTo => "已导出到",
        T::FlashExportImagesTo => "导出图片到",
        T::FlashPasteImage => "粘贴图片",
        T::FlashExportProgress => "导出",
        T::FlashExportImagesProgress => "导出图片到",
        T::FlashExportedNImages => "已导出",
        T::FlashExportFailed => "导出失败",
        T::FlashProcessing => "处理中...",
        T::FlashDuplicated => "已复制 {0} 个元素",
        T::FlashCopied => "已复制 {0} 项",
        T::FlashCut => "已剪切 {0} 项",
        T::FlashSaved => "已保存: {0}",
        T::FlashSaveFailed => "保存失败: {0}",
        T::FlashOpened => "已打开: {0}",
        T::FlashOpenFailed => "打开失败: {0}",
        T::FlashImported => "已导入: {0}",
        T::FlashImportFailed => "导入失败 {0}: {1}",
        T::FlashClipboardFailed => "剪贴板访问失败: {0}",
        T::ProgressImportImage => "导入图片: {0}",
        T::ProgressOpenFile => "打开文件: {0}",
        T::ProgressSaveFile => "保存文件: {0}",
        T::FlashFlipH => "水平翻转",
        T::FlashFlipV => "垂直翻转",
        T::FlashTransform => "变换: 缩放=({0}, {1}) 旋转={2}°",
        T::FlashMoved => "移动: ({0}, {1})",
        T::FlashDeleted => "已删除 {0} 项",
        T::FlashShapeCreated => "已创建图形",
        T::FlashLineCreated => "已创建直线",
        T::FlashArrowCreated => "已创建箭头",
        T::FlashFrameCreated => "已创建画框 #{0}",
        T::FlashFrameRenumbered => "画框编号 → #{0}",
        T::ChartChooserTitle => "检测到两列数据，粘贴为图表",
        T::ChartChooserBar => "柱状图",
        T::ChartChooserLine => "折线图",
        T::ChartChooserCancel => "取消",
        T::ChartChooserMore => "…还有 {0} 行",
        T::FlashChartCreated => "已创建图表",
        T::FlashNoSelectedImages => "无选中的图片项",
        T::FlashNoExportableImages => "无可导出的图片项",
        T::FlashArranged => "排列：{0}",
        T::FlashAligned => "对齐：{0}",
        T::FlashDistributed => "分布：{0}",
        T::FlashNormalized => "归一化尺寸: {0}",
        T::ArrangeModeLinear => "线形",
        T::ArrangeModeGrid => "网格",
        T::ArrangeModeOptimal => "最优装箱",
        // 欢迎页
        T::WelcomeTitle => "PReferZ",
        T::WelcomeSubtitle => "参考图板 · 右键打开菜单开始",
        T::WelcomeRecentFiles => "最近文件",
        T::WelcomeHint => "右键 → 打开项目 / 载入图片 / 粘贴图片",
        // 设置面板
        T::SettingsTitle => "设置",
        T::SettingsArrange => "排列",
        T::SettingsSpacing => "间距",
        T::SettingsWindow => "窗口",
        T::SettingsAlwaysOnTop => "始终置顶",
        T::SettingsFrameless => "无边框悬浮模式",
        T::SettingsBgAlpha => "背景透明度",
        T::SettingsLanguage => "语言",
        T::SettingsTheme => "主题",
        T::SettingsShortcuts => "快捷键",
        T::SettingsKeymapHint => "点击某一行右侧的快捷键，然后按下新的组合键。",
        T::SettingsKeymapCapturing => "请按下新的组合键…",
        T::SettingsKeymapUnbound => "未绑定",
        T::SettingsKeymapRestore => "恢复默认",
        T::SettingsKeymapConflict => "与「{0}」冲突，已覆盖",
        // 保存提示
        T::SavePromptMessage => "画布有未保存的修改，是否保存？",
        T::SavePromptSave => "保存",
        T::SavePromptDiscard => "放弃",
        T::SavePromptCancel => "取消",
        // 绘制工具/样式面板
        T::ToolSelect => "选择",
        T::ToolRectangle => "矩形",
        T::ToolEllipse => "椭圆",
        T::ToolDiamond => "菱形",
        T::ToolLine => "直线",
        T::ToolArrow => "箭头",
        T::ToolFrame => "画框",
        T::ToolPolygon => "多边形",
        T::ToolFreehand => "徒手",
        T::FlashFreedrawCreated => "已绘制墨迹",
        T::PolygonTooFewPoints => "多边形至少需要 3 个顶点，已丢弃",
        T::PolygonCreated => "已创建多边形",
        T::Present => "幻灯片放映 ({0})",
        T::PresentNoFrames => "没有可演示的画框，请先创建画框（工具 ▢）。",
        T::StyleStrokeColor => "描边颜色",
        T::StyleStrokeWidth => "描边宽度",
        T::StyleDashSolid => "实线",
        T::StyleDashDashed => "虚线",
        T::StyleDashDotted => "点线",
        T::StyleFillNone => "无填充",
        T::StyleFillLabel => "填充",
        T::StyleFillOpacity => "不透明度",
        T::StyleFillSolid => "纯色",
        T::StyleFillHachure => "斜线",
        T::StyleFillCrossHatch => "交叉线",
        T::PropsDefaultsTitle => "新建元素默认样式",
        T::StyleClosed => "闭合",
        T::StyleArrowStart => "起点箭头",
        T::StyleArrowEnd => "终点箭头",
        T::StyleArrowNone => "无",
        T::StyleArrowArrow => "箭头",
        T::StyleArrowDot => "圆点",
        T::StyleCurve => "边角",
        T::StyleCurveStraight => "尖角",
        T::StyleCurveCurved => "圆滑",
        T::StyleRoundness => "圆角",
        T::PropsSelectedCount => "已选 {0} 项",
        T::PropsMixedSelection => "选中了不同类型的元素，没有可批量编辑的共有属性",
        T::PropsMixedValue => "不一致",
        T::PropsSectionShape => "形状",
        T::PropsSectionFreedraw => "墨迹",
        T::PropsSectionText => "文字",
        T::PropsSectionPixmap => "图片",
        T::PropsSectionFrame => "画框",
        T::PropsFrameNumber => "编号",
        T::PropsSectionAlign => "对齐",
        T::AlignLeft => "左对齐",
        T::AlignHCenter => "水平居中",
        T::AlignRight => "右对齐",
        T::AlignTop => "顶对齐",
        T::AlignVCenter => "垂直居中",
        T::AlignBottom => "底对齐",
        T::Distribute => "分布",
        T::DistributeHGap => "横向等距",
        T::DistributeHCenters => "横向等心",
        T::DistributeVGap => "纵向等距",
        T::DistributeVCenters => "纵向等心",
        T::FlashDistributeNeedThree => "分布需要至少 3 个元素",
        T::FlashSnappedToShape => "已吸附到图形边缘",
        T::FlashVertexDeleted => "已删除顶点",
        T::FlashVertexMinOpen => "开放折线至少保留 2 个顶点",
        T::FlashVertexMinClosed => "闭合多边形至少保留 3 个顶点",
        T::FlashPolygonClosed => "已闭合成多边形",
        T::FlashPolygonOpened => "已恢复开放",
        T::ThemeToggleHint => "切换明暗主题",
        T::FlashGrouped => "已编组",
        T::FlashUngrouped => "已解组",
        T::MenuGroup => "编组",
        T::MenuUngroup => "解组",
        T::StyleRough => "手绘风",
        T::SloppinessOff => "关闭",
        T::SloppinessArchitect => "建筑师",
        T::SloppinessArtist => "画师",
        T::SloppinessCartoonist => "卡通",
        T::StyleTextBackground => "背景",
        T::StyleGrayscale => "灰度",
        T::StyleFontLabel => "字体",
        T::FontNormal => "黑体",
        T::FontHandwriting => "手写",
        T::StyleAlignH => "水平对齐",
        T::StyleAlignV => "垂直对齐",
        T::TextAlignLeft => "左",
        T::TextAlignCenter => "中",
        T::TextAlignRight => "右",
        T::TextAlignTop => "上",
        T::TextAlignMiddle => "中",
        T::TextAlignBottom => "下",
        T::FramePresetLabel => "比例 / 纸张预设",
        T::FramePresetPick => "选择预设…",
        T::Preset16x9 => "16:9",
        T::Preset16x10 => "16:10",
        T::Preset4x3 => "4:3",
        T::Preset3x2 => "3:2",
        T::Preset1x1 => "1:1",
        T::PresetA4Portrait => "A4 竖版",
        T::PresetA4Landscape => "A4 横版",
        T::FlashFramePresetApplied => "已套用预设：{0}",
    }
}

/// 快捷键设置面板里的动作名。与 [`crate::keymap::Action`] 一一对应——
/// 新增动作必须在这里补两语文案，否则面板会显示 `?Action`。
pub fn action_label(lang: Lang, action: Action) -> &'static str {
    use Action::*;
    match lang {
        Lang::En => match action {
            NewCanvas => "New canvas",
            OpenProject => "Open project",
            LoadImage => "Load image",
            Save => "Save",
            SaveAs => "Save as",
            Undo => "Undo",
            Redo => "Redo",
            Paste => "Paste from clipboard",
            Copy => "Copy selection",
            Cut => "Cut selection",
            DeleteSelected => "Delete selection",
            FitToScreen => "Fit to canvas",
            ZoomToSelection => "Zoom to selection",
            Zoom100 => "Zoom to 100%",
            TogglePresent => "Toggle slideshow",
            ToolSelect => "Tool: select",
            ToolRect => "Tool: rectangle",
            ToolEllipse => "Tool: ellipse",
            ToolDiamond => "Tool: diamond",
            ToolLine => "Tool: line",
            ToolArrow => "Tool: arrow",
            ToolFrame => "Tool: frame",
            ToolPolygon => "Tool: polygon",
            ToolFreehand => "Tool: freedraw",
            Crop => "Crop mode",
            ColorPicker => "Color picker",
            ContextMenu => "Show context menu",
            PresentNext => "Slideshow: next",
            PresentPrev => "Slideshow: previous",
            PresentFirst => "Slideshow: first",
            PresentLast => "Slideshow: last",
            Cancel => "Cancel (crop / tool / picker)",
            Confirm => "Confirm (apply crop)",
            EditText => "Edit text of selection",
            DuplicateInPlace => "Duplicate in place",
            Group => "Group selection",
            Ungroup => "Ungroup selection",
            AddConnectedShape => "Flowchart: add connected shape",
            NavigateConnected => "Flowchart: navigate along connections",
        },
        Lang::Zh => match action {
            NewCanvas => "新建画布",
            OpenProject => "打开项目",
            LoadImage => "载入图片",
            Save => "保存",
            SaveAs => "另存为",
            Undo => "撤销",
            Redo => "重做",
            Paste => "从剪贴板粘贴",
            Copy => "复制选中项",
            Cut => "剪切选中项",
            DeleteSelected => "删除选中",
            FitToScreen => "适应画布",
            ZoomToSelection => "缩放到选中",
            Zoom100 => "缩放 100%",
            TogglePresent => "切换幻灯片放映",
            ToolSelect => "工具：选择",
            ToolRect => "工具：矩形",
            ToolEllipse => "工具：椭圆",
            ToolDiamond => "工具：菱形",
            ToolLine => "工具：直线",
            ToolArrow => "工具：箭头",
            ToolFrame => "工具：画框",
            ToolPolygon => "工具：多边形",
            ToolFreehand => "工具：徒手绘制",
            Crop => "进入裁剪模式",
            ColorPicker => "切换取色器",
            ContextMenu => "显示右键菜单",
            PresentNext => "放映：下一页",
            PresentPrev => "放映：上一页",
            PresentFirst => "放映：首页",
            PresentLast => "放映：末页",
            Cancel => "取消（裁剪 / 工具 / 取色器）",
            Confirm => "确认（应用裁剪）",
            EditText => "编辑选中项的文字",
            DuplicateInPlace => "原位复制",
            Group => "编组选中项",
            Ungroup => "解组选中项",
            AddConnectedShape => "流程图：添加连接图形",
            NavigateConnected => "流程图：沿连接导航",
        },
    }
}

#[cfg(test)]
mod tests {
    /// 英文表不允许出现 CJK 字符。
    ///
    /// `translate_en` 的穷尽 `match` 只能保证「每个 key 都有 entry」，保证不了
    /// 「entry 真是英文」——历史上 flash / 进度条文案直接写死在 `preferz_app.rs`
    /// 里（`已保存: ...`），切到 EN 界面照样弹中文。这里扫自身源码补上这道闸，
    /// 行内 `//` 注释（大量中文说明）不计入检查。
    #[test]
    fn english_table_contains_no_cjk() {
        let src = include_str!("i18n.rs");
        let after = src
            .split_once("fn translate_en(key: T) -> &'static str {")
            .expect("translate_en 应存在")
            .1;
        let body = after
            .split_once("\nfn ")
            .expect("translate_en 之后应有下一个 fn")
            .0;
        let bad: Vec<&str> = body
            .lines()
            .map(|line| line.split("//").next().unwrap_or(""))
            .filter(|code| {
                code.chars().any(|c| {
                    matches!(
                        c,
                        '\u{2e80}'..='\u{9fff}' | '\u{ac00}'..='\u{d7af}' | '\u{ff00}'..='\u{ffef}'
                    )
                })
            })
            .map(str::trim)
            .collect();
        assert!(
            bad.is_empty(),
            "英文表含中文文案（切 EN 界面会漏出中文）: {bad:?}"
        );
    }
}
