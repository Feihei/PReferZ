//! 极简 i18n：枚举查表方案（无外部依赖）。
//!
//! 设计：`Lang` 枚举 + `t(lang, key)` 函数，所有文案集中在 `TRANSLATIONS` 表。
//! 新增语言只需扩展 `Lang` 变体 + `TRANSLATIONS` 对应分支。

use serde::{Deserialize, Serialize};

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
}

/// 翻译 key。新增文案在此追加变体，然后在 `translate` 中提供两种语言文案。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum T {
    // ── 右键菜单 ──
    NewCanvas,
    OpenProject,
    LoadImage,
    PasteImage, // 含快捷键后缀
    Save,
    SaveAs,
    ExportScene,
    ExportImagesToDir,
    DeleteSelected,
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
    SettingsShortcuts,
    SettingsShortcutArrange,
    SettingsShortcutUndo,
    SettingsShortcutFile,
    SettingsShortcutPaste,
    // ── 保存提示对话框 ──
    SavePromptMessage,
    SavePromptSave,
    SavePromptDiscard,
    SavePromptCancel,
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
        T::PasteImage => "Paste Image (Ctrl+V)",
        T::Save => "Save",
        T::SaveAs => "Save As...",
        T::ExportScene => "Export Scene",
        T::ExportImagesToDir => "Export Images to Folder",
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
        T::FlashToggleGrayscale => "Grayscale toggled",
        T::FlashCropNeedSingleImage => "Crop requires a single selected image",
        T::FlashCropImageOnly => "Only images support cropping",
        T::FlashCropHint => "Crop mode: drag corners · Enter to apply · Esc to cancel",
        T::FlashCropNoChange => "Crop unchanged",
        T::FlashCropInvalidSize => "Invalid crop size",
        T::FlashCropApplied => "Crop applied",
        T::FlashCropCancelled => "Crop cancelled",
        T::FlashColorPickerMissed => "No image hit",
        T::FlashColorPickerImageOnly => "Only images support sampling",
        T::FlashColorPickerHint => "Color picker: click image to sample · Esc to exit",
        T::FlashNormalizeNeedMultiple => "Normalize requires multiple selected images",
        T::FlashExportedTo => "Exported to",          // 后接路径
        T::FlashExportImagesTo => "Export images to", // 后接路径
        T::FlashPasteImage => "Pasting image",
        T::FlashExportProgress => "Export", // 后接路径
        T::FlashExportImagesProgress => "Export images to", // 后接路径
        T::FlashExportedNImages => "Exported", // 后接数量+路径
        T::FlashExportFailed => "Export failed", // 后接错误
        T::FlashProcessing => "Processing...",
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
        T::SettingsShortcuts => "Shortcuts",
        T::SettingsShortcutArrange => "R/G/O arrange · C crop · I color picker · F fit",
        T::SettingsShortcutUndo => "Ctrl+Z undo · Ctrl+Shift+Z redo",
        T::SettingsShortcutFile => "Ctrl+N new · Ctrl+O open · Ctrl+I load image",
        T::SettingsShortcutPaste => "Ctrl+V paste · Ctrl+S save · Ctrl+Shift+S save as",
        // 保存提示
        T::SavePromptMessage => "Canvas has unsaved changes. Save?",
        T::SavePromptSave => "Save",
        T::SavePromptDiscard => "Discard",
        T::SavePromptCancel => "Cancel",
    }
}

fn translate_zh(key: T) -> &'static str {
    match key {
        // 右键菜单
        T::NewCanvas => "新建画布",
        T::OpenProject => "打开项目...",
        T::LoadImage => "载入图片...",
        T::PasteImage => "粘贴图片 (Ctrl+V)",
        T::Save => "保存",
        T::SaveAs => "另存为...",
        T::ExportScene => "导出场景",
        T::ExportImagesToDir => "导出图片到目录",
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
        T::FlashToggleGrayscale => "切换灰度",
        T::FlashCropNeedSingleImage => "裁剪需要选中单个图片",
        T::FlashCropImageOnly => "仅图片支持裁剪",
        T::FlashCropHint => "裁剪模式：拖拽角点 · Enter 应用 · Esc 取消",
        T::FlashCropNoChange => "裁剪未变化",
        T::FlashCropInvalidSize => "裁剪尺寸无效",
        T::FlashCropApplied => "已应用裁剪",
        T::FlashCropCancelled => "取消裁剪",
        T::FlashColorPickerMissed => "未命中图片",
        T::FlashColorPickerImageOnly => "仅图片支持采样",
        T::FlashColorPickerHint => "取色器模式：单击图片采样 · Esc 退出",
        T::FlashNormalizeNeedMultiple => "归一化需要选中多个图片",
        T::FlashExportedTo => "已导出到",
        T::FlashExportImagesTo => "导出图片到",
        T::FlashPasteImage => "粘贴图片",
        T::FlashExportProgress => "导出",
        T::FlashExportImagesProgress => "导出图片到",
        T::FlashExportedNImages => "已导出",
        T::FlashExportFailed => "导出失败",
        T::FlashProcessing => "处理中...",
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
        T::SettingsShortcuts => "快捷键",
        T::SettingsShortcutArrange => "R/G/O 排列 · C 裁剪 · I 取色器 · F 适应画布",
        T::SettingsShortcutUndo => "Ctrl+Z 撤销 · Ctrl+Shift+Z 重做",
        T::SettingsShortcutFile => "Ctrl+N 新建 · Ctrl+O 打开 · Ctrl+I 载入图片",
        T::SettingsShortcutPaste => "Ctrl+V 粘贴图片 · Ctrl+S 保存 · Ctrl+Shift+S 另存为",
        // 保存提示
        T::SavePromptMessage => "画布有未保存的修改，是否保存？",
        T::SavePromptSave => "保存",
        T::SavePromptDiscard => "放弃",
        T::SavePromptCancel => "取消",
    }
}
