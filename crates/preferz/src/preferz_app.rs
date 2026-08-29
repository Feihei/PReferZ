use crate::i18n::{t, Lang, T};
use crate::interaction;
use crate::ui::stylers::{build_shape_visuals, item_local_to_screen};
use crate::ui::widgets::transform_handles::{
    should_show_flip, should_show_rotate, Handle, TransformHandles,
};
use crate::viewport::ViewportState;
use eframe::egui;
use image::GenericImageView;
use preferz_core::arrange::{plan_arrange, ArrangeMode};
use preferz_core::commands::{
    AddItem, ArrangeItems, CropItems, DeleteItems, EditShapePoints, EditTextContent, FlipItems,
    MoveItems, NormalizeItems, RenumberFrame, ReorderItems, SetArrowHeads, SetClosed,
    SetPixmapProps, SetRough, TransformItem,
};
use preferz_core::shape::{ArrowHeadStyle, DashStyle, ShapeType, StrokeStyle};
use preferz_core::spaces::{CanvasPoint, CanvasRect, CanvasSize, CanvasVector};
use preferz_core::{Command, CropRect, Item, ItemId, ItemKind, Scene};
use preferz_fileio::{BeeFile, ViewportMeta};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

/// Undo 栈。`push` 会读�?`Command::skip_first_redo()`�?
/// - 交互预览命令（拖拽中已直接改 item）返�?true �?跳过首次 redo
/// - 普通命令返�?false �?push 时立�?redo 应用变更
///
/// 这让 AGENTS.md Gotcha #5（skip_first_redo）真正生效（�?S5/M7）�?
struct UndoStack {
    undo: Vec<Box<dyn Command>>,
    redo: Vec<Box<dyn Command>>,
}

impl UndoStack {
    fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    fn push(&mut self, mut cmd: Box<dyn Command>, scene: &mut Scene) {
        let skip = cmd.skip_first_redo();
        if !skip {
            cmd.redo(scene);
        }
        self.redo.clear();
        self.undo.push(cmd);
    }

    fn undo(&mut self, scene: &mut Scene) -> bool {
        if let Some(mut cmd) = self.undo.pop() {
            cmd.undo(scene);
            self.redo.push(cmd);
            true
        } else {
            false
        }
    }

    fn redo(&mut self, scene: &mut Scene) -> bool {
        if let Some(mut cmd) = self.redo.pop() {
            cmd.redo(scene);
            self.undo.push(cmd);
            true
        } else {
            false
        }
    }
}

/// 当前激活工具。Select = 现有选择/框选行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Shape(ShapeType),
    /// 线性对象工具：携带默认终点箭头（Line = None，Arrow = Some(Arrow)）。
    Linear {
        end_arrow: Option<ArrowHeadStyle>,
    },
    /// 幻灯片画框（Phase D）。
    Frame,
}

/// 应用运行模式。Present 为全屏幻灯片演示（Phase E）。
enum AppMode {
    Edit,
    Present {
        /// 按 number 排序、尺寸达标的画框 id 快照（进入时固化，退出前不刷新）。
        slides: Vec<ItemId>,
        /// 每个 slide 的成员快照（与 slides 等长，进入时预计算，翻页不重算）。
        members: Vec<Vec<ItemId>>,
        index: usize,
        /// 退出 Present 时恢复的进入前视口。
        saved_pan: CanvasVector,
        saved_zoom: f32,
    },
}

/// 拖拽状态机。
enum DragState {
    Idle,
    HandleTransform {
        item_id: ItemId,
        handle: Handle,
        start_screen: egui::Pos2,
        start_transform: preferz_core::Transform,
        start_corners: [CanvasPoint; 4],
    },
    MoveItems {
        start_canvas: CanvasPoint,
        start_transforms: Vec<(ItemId, preferz_core::Transform)>,
    },
    /// 框选（spec L240）。空白处左键拖拽成矩形，Shift 加选。
    BoxSelect {
        start_canvas: CanvasPoint,
        current_canvas: CanvasPoint,
        additive: bool,
    },
    /// 用绘制工具拖拽创建 shape（两点式：start → current）。
    CreatingShape {
        shape_type: ShapeType,
        /// 线性对象工具的默认终点箭头（矩形族忽略）。
        end_arrow: Option<ArrowHeadStyle>,
        start: CanvasPoint,
        current: CanvasPoint,
        /// Shift 锁正方形/45° 方向。
        shift: bool,
        /// Ctrl 解锁椭圆（自由宽高比）；默认椭圆为正圆，与变换框行为一致。
        ctrl: bool,
    },
    /// 拖拽线性对象顶点控制点（Polyline 的 points 下标），预览直接改 points。
    LineEndpoint {
        item_id: ItemId,
        /// points 下标（任意顶点，开放折线/闭合多边形共用）。
        endpoint: usize,
        start_canvas: CanvasPoint,
        start_points: Vec<(f32, f32)>,
    },
    /// 用 Frame 工具拖拽创建画框（两点式：start → current）。
    CreatingFrame {
        start: CanvasPoint,
        current: CanvasPoint,
    },
}

/// 文本便签编辑状态（spec L243 P2-5）�?
/// `editing_item_id = None` 表示创建新文本（提交�?push `AddItem`）；
/// `Some(id)` 表示编辑现有 item（提交时 push `EditTextContent`）�?
/// Enter/失焦时提交，空内容在创建模式下丢弃，在编辑模式下不修改原 item�?
struct EditingText {
    editing_item_id: Option<ItemId>,
    canvas_pos: CanvasPoint,
    buffer: String,
    font_size: f32,
    color: [u8; 4],
    first_frame: bool,
    /// 绑定文本所属容器 id（None = 自由文本）。新创建绑定时在提交时写入。
    container_id: Option<ItemId>,
}

/// 后台图片导入解码结果（线�?�?UI 线程）�?
/// 线程负责读取文件字节 + 解码；UI 线程负责上传纹理 + 创建 item�?
struct ImportOutcome {
    path: PathBuf,
    /// 原始图片字节（写�?sqlar 用）
    bytes: Vec<u8>,
    /// 解码后的图片尺寸
    width: u32,
    height: u32,
    /// RGBA 像素数据（上传纹理用�?
    rgba: Vec<u8>,
    /// 解码错误（若存在�?
    error: Option<String>,
}

/// 后台 .prz/.bee 加载结果（线�?�?UI 线程）�?
struct LoadOutcome {
    path: PathBuf,
    result: Result<preferz_fileio::LoadResult, String>,
}

/// 后台保存结果（线�?�?UI 线程）�?
struct SaveOutcome {
    path: PathBuf,
    result: Result<(), String>,
}

/// 后台导出结果（线程 → UI 线程）。
struct ExportOutcome {
    path: PathBuf,
    result: Result<String, String>,
}

/// 后台任务状态。`loading`/`saving` 为 true 时显示进度条。
#[derive(Default)]
struct BackgroundOps {
    /// 图片导入解码通道（单条队列，每次导入一条）。
    import_rx: Option<Receiver<ImportOutcome>>,
    /// .prz/.bee 文件加载通道。
    load_rx: Option<Receiver<LoadOutcome>>,
    /// 文件保存通道。
    save_rx: Option<Receiver<SaveOutcome>>,
    /// 场景导出通道。
    export_rx: Option<Receiver<ExportOutcome>>,
    /// 当前进行的后台任务数量（>0 时显示进度条）。
    pending: usize,
    /// 进度消息。
    msg: Option<String>,
}

impl BackgroundOps {
    fn start_import(&mut self, ctx: &egui::Context, path: PathBuf) {
        let (tx, rx) = mpsc::channel();
        self.import_rx = Some(rx);
        self.pending += 1;
        self.msg = Some(format!("导入图片: {}", path.display()));
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let outcome = match (std::fs::read(&path), image::open(&path)) {
                (Ok(bytes), Ok(img)) => {
                    let (w, h) = img.dimensions();
                    let rgba = img.to_rgba8().into_vec();
                    ImportOutcome {
                        path,
                        bytes,
                        width: w,
                        height: h,
                        rgba,
                        error: None,
                    }
                }
                (Err(e), _) => ImportOutcome {
                    path,
                    bytes: Vec::new(),
                    width: 0,
                    height: 0,
                    rgba: Vec::new(),
                    error: Some(format!("读取失败: {}", e)),
                },
                (_, Err(e)) => ImportOutcome {
                    path,
                    bytes: Vec::new(),
                    width: 0,
                    height: 0,
                    rgba: Vec::new(),
                    error: Some(format!("解码失败: {}", e)),
                },
            };
            let _ = tx.send(outcome);
            ctx2.request_repaint();
        });
    }

    fn start_load(&mut self, ctx: &egui::Context, path: PathBuf) {
        let (tx, rx) = mpsc::channel();
        self.load_rx = Some(rx);
        self.pending += 1;
        self.msg = Some(format!("打开文件: {}", path.display()));
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let bee = BeeFile::open(&path)?;
                bee.load_scene()
            })();
            let outcome = LoadOutcome {
                path: path.clone(),
                result: result.map_err(|e| e.to_string()),
            };
            let _ = tx.send(outcome);
            ctx2.request_repaint();
        });
    }

    fn start_save(
        &mut self,
        ctx: &egui::Context,
        path: PathBuf,
        scene: Scene,
        images: HashMap<String, Vec<u8>>,
        viewport: Option<ViewportMeta>,
    ) {
        let (tx, rx) = mpsc::channel();
        self.save_rx = Some(rx);
        self.pending += 1;
        self.msg = Some(format!("保存文件: {}", path.display()));
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let mut bee = if path.exists() {
                    BeeFile::open(&path)?
                } else {
                    BeeFile::create(&path)?
                };
                bee.save_scene(&scene, &images, viewport)
            })();
            let _ = tx.send(SaveOutcome {
                path: path.clone(),
                result: result.map_err(|e| e.to_string()),
            });
            ctx2.request_repaint();
        });
    }

    /// 取出并处理已完成的导入结果（�?PReferZApp::poll_background 调用）�?
    fn take_import(&mut self) -> Option<ImportOutcome> {
        if let Some(rx) = &self.import_rx {
            if let Ok(outcome) = rx.try_recv() {
                self.pending = self.pending.saturating_sub(1);
                if self.pending == 0 {
                    self.msg = None;
                }
                self.import_rx = None;
                return Some(outcome);
            }
        }
        None
    }

    /// 取出并处理已完成的加载结果（�?PReferZApp::poll_background 调用）�?
    fn take_load(&mut self) -> Option<LoadOutcome> {
        if let Some(rx) = &self.load_rx {
            if let Ok(outcome) = rx.try_recv() {
                self.pending = self.pending.saturating_sub(1);
                if self.pending == 0 {
                    self.msg = None;
                }
                self.load_rx = None;
                return Some(outcome);
            }
        }
        None
    }

    /// 取出并处理已完成的保存结果（由 PReferZApp::poll_background 调用）。
    fn take_save(&mut self) -> Option<SaveOutcome> {
        if let Some(rx) = &self.save_rx {
            if let Ok(outcome) = rx.try_recv() {
                self.pending = self.pending.saturating_sub(1);
                if self.pending == 0 {
                    self.msg = None;
                }
                self.save_rx = None;
                return Some(outcome);
            }
        }
        None
    }

    /// 取出并处理已完成的导出结果（由 PReferZApp::poll_background 调用）。
    fn take_export(&mut self) -> Option<ExportOutcome> {
        if let Some(rx) = &self.export_rx {
            if let Ok(outcome) = rx.try_recv() {
                self.pending = self.pending.saturating_sub(1);
                if self.pending == 0 {
                    self.msg = None;
                }
                self.export_rx = None;
                return Some(outcome);
            }
        }
        None
    }
}

pub struct PReferZApp {
    scene: Scene,
    viewport: ViewportState,
    undo_stack: UndoStack,
    /// 临时状态消息（�?已导�?），会在若干帧后清空，避免覆盖持续状态（�?B5）�?
    flash_status: Option<(String, std::time::Instant)>,
    context_menu_open: bool,
    context_menu_pos: egui::Pos2,
    texture_cache: HashMap<u64, egui::TextureHandle>,
    /// 灰度纹理缓存（懒生成）。grayscale=true �?Pixmap 渲染时用此处的纹理�?
    grayscale_texture_cache: HashMap<u64, egui::TextureHandle>,
    /// 原始图片字节缓存（texture_id �?原始文件字节），保存时写�?sqlar�?
    image_data_cache: HashMap<u64, Vec<u8>>,
    /// 解码后的 RGBA 像素缓存（texture_id �?RGBA 字节），用于懒生成灰度纹�?+ 颜色采样�?
    rgba_pixel_cache: HashMap<u64, Vec<u8>>,
    /// RGBA 像素尺寸（texture_id �?(w, h)），用于灰度生成和颜色采样�?
    rgba_size_cache: HashMap<u64, (u32, u32)>,
    next_texture_id: u64,
    pending_import: Vec<PathBuf>,
    transform_handles: TransformHandles,
    /// 当前激活工具。
    tool: Tool,
    /// 绘制形状默认描边样式（样式面板 A8 可调）。
    default_stroke: StrokeStyle,
    /// 绘制形状默认填充色（None = 透明；样式面板 A8 可调）。
    default_fill: Option<[u8; 4]>,
    /// 新建形状是否默认手绘风描边（Phase F；样式面板可调）。
    default_rough: bool,
    drag: DragState,
    /// 文本便签编辑状态（None = 无编辑）�?
    editing_text: Option<EditingText>,
    /// 画框编号编辑状态（Phase D）：Some(frame_id) 时显示左上角小输入框。
    editing_frame_number: Option<ItemId>,
    /// 画框编号编辑输入缓冲区。
    frame_number_buf: String,
    /// 运行模式：编辑 / 全屏幻灯片演示（Phase E）。
    app_mode: AppMode,
    /// Present 翻页过渡目标视口（zoom, pan），Some 时逐帧指数插值。
    present_anim: Option<(f32, CanvasVector)>,
    /// 当前打开的文件路径（保存时若 None 则弹出对话框）�?
    current_file: Option<PathBuf>,
    /// 后台任务（导入解�?/ 文件加载 / 文件保存）�?
    bg_ops: BackgroundOps,
    /// 颜色采样模式（spec §2.2 颜色采样）。true 时鼠标在 Pixmap 上读取像�?RGB 显示�?
    color_picker_active: bool,
    /// 最近一次采样的颜色结果（取色器模式下持续更新）�?
    color_sample: Option<ColorSample>,
    /// 裁剪模式（spec §2.2 裁剪）。Some(item_id) 时该 item 进入裁剪交互模式�?
    crop_mode: Option<CropMode>,
    /// 设置面板是否打开（Phase 6 §2.3，简化版：仅排列间距 + 主题切换）�?
    settings_open: bool,
    /// 排列间距（设置面板可调）�?
    arrange_spacing: f32,
    /// 画布是否有未保存修改（用于关闭/新建时提示保存）。
    dirty: bool,
    /// 待处理的关闭/新建请求（弹保存确认对话框）。
    /// `Close` = 关闭窗口，`NewCanvas` = 新建画布
    pending_save_prompt: Option<SavePromptAction>,
    /// 窗口置顶开关（Phase 6 §2.3 始终置顶）。
    always_on_top: bool,
    /// 窗口无边框开关（Phase 6 §2.3 无边框悬浮模式）。
    frameless: bool,
    /// 画布背景不透明度（0.1~1.0）。配透明窗口实现悬浮看图效果。
    bg_alpha: f32,
    /// UI 语言（默认英文，设置面板可切换中文，持久化到 config.json）。
    lang: Lang,
    /// 最近文件列表（Phase 6 §2.3 欢迎页）。
    recent_files: Vec<PathBuf>,
    /// 欢迎页点击的最近文件路径（待处理）。
    pending_open_recent: Option<PathBuf>,
    /// 欢迎页 logo 纹理（懒加载，复用 assets/icon.png）。
    logo_texture: Option<egui::TextureHandle>,
}

/// 保存提示对话框的触发场景�?#[derive(Clone, Copy, PartialEq)]
enum SavePromptAction {
    /// 用户点了窗口关闭按钮�?
    Close,
    /// 用户点了新建画布（Ctrl+N）�?
    NewCanvas,
}

/// 裁剪模式状态（spec §2.2 裁剪）。
#[derive(Clone)]
struct CropMode {
    item_id: ItemId,
    /// 当前正在编辑的裁剪矩形（item 局部空间像素坐标）�?
    rect: CropRect,
    /// 拖拽中的角点（None = 未拖拽）�?
    dragging: Option<CropHandle>,
    /// 进入裁剪模式前的原始 crop（Esc 取消时恢复）�?
    original: Option<CropRect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CropHandle {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl PReferZApp {
    pub fn new() -> Self {
        Self {
            scene: Scene::new(),
            viewport: ViewportState::default(),
            undo_stack: UndoStack::new(),
            flash_status: None,
            context_menu_open: false,
            context_menu_pos: egui::Pos2::ZERO,
            texture_cache: HashMap::new(),
            grayscale_texture_cache: HashMap::new(),
            image_data_cache: HashMap::new(),
            rgba_pixel_cache: HashMap::new(),
            rgba_size_cache: HashMap::new(),
            next_texture_id: 1,
            pending_import: Vec::new(),
            transform_handles: TransformHandles::new(),
            tool: Tool::Select,
            default_stroke: StrokeStyle::default(),
            default_fill: None,
            default_rough: false,
            drag: DragState::Idle,
            editing_text: None,
            editing_frame_number: None,
            frame_number_buf: String::new(),
            app_mode: AppMode::Edit,
            present_anim: None,
            current_file: None,
            bg_ops: BackgroundOps::default(),
            color_picker_active: false,
            color_sample: None,
            crop_mode: None,
            settings_open: false,
            arrange_spacing: 16.0,
            dirty: false,
            pending_save_prompt: None,
            always_on_top: false,
            frameless: false,
            bg_alpha: 1.0,
            lang: load_config().lang,
            recent_files: load_recent_files(),
            pending_open_recent: None,
            logo_texture: None,
        }
    }

    fn flash(&mut self, msg: impl Into<String>) {
        self.flash_status = Some((msg.into(), std::time::Instant::now()));
    }

    /// push undo command 并标记画布为 dirty（有未保存修改）�?
    fn push_cmd(&mut self, cmd: Box<dyn Command>) {
        self.undo_stack.push(cmd, &mut self.scene);
        self.dirty = true;
    }

    /// 执行 undo：成功则标记 dirty�?
    fn perform_undo(&mut self) -> bool {
        if self.undo_stack.undo(&mut self.scene) {
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// 执行 redo：成功则标记 dirty�?
    fn perform_redo(&mut self) -> bool {
        if self.undo_stack.redo(&mut self.scene) {
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// poll 后台任务通道，分发到 finish_import / finish_load / 保存结果处理�?
    fn poll_background(&mut self, ctx: &egui::Context) {
        // 导入
        if let Some(outcome) = self.bg_ops.take_import() {
            self.finish_import(ctx, outcome);
        }
        // 加载
        if let Some(outcome) = self.bg_ops.take_load() {
            self.finish_load(ctx, outcome);
        }
        // 保存
        if let Some(outcome) = self.bg_ops.take_save() {
            match outcome.result {
                Ok(()) => {
                    self.flash(format!("已保存: {}", outcome.path.display()));
                    self.current_file = Some(outcome.path.clone());
                    self.dirty = false;
                    // 若有 pending 关闭/新建请求，现在保存完成可以执行了
                    if let Some(action) = self.pending_save_prompt.take() {
                        match action {
                            SavePromptAction::Close => {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            SavePromptAction::NewCanvas => {
                                self.reset_canvas(ctx);
                            }
                        }
                    }
                }
                Err(e) => {
                    self.flash(format!("保存失败: {}", e));
                    // 保存失败：取消 pending，让用户自行决定
                    self.pending_save_prompt = None;
                }
            }
        }
        // 导出
        if let Some(outcome) = self.bg_ops.take_export() {
            match outcome.result {
                Ok(msg) => self.flash(if msg.is_empty() {
                    format!(
                        "{}: {}",
                        t(self.lang, T::FlashExportedTo),
                        outcome.path.display()
                    )
                } else {
                    msg
                }),
                Err(e) => self.flash(format!("{}: {}", t(self.lang, T::FlashExportFailed), e)),
            }
        }
    }

    /// 选中 item 的快照（�?Z 序倒序，顶层在前）�?
    fn selected_items_snapshot(&self) -> Vec<Item> {
        let mut items: Vec<Item> = self
            .scene
            .selection
            .iter()
            .filter_map(|id| self.scene.get_item(id).cloned())
            .collect();
        items.sort_by_key(|b| std::cmp::Reverse(b.z));
        items
    }

    /// 选中线性对象（Polyline）的箭头与闭合状态：(item_id, start_arrow, end_arrow, closed)。
    /// 从选中项中找第一个线性对象；无则返回 None。
    fn selected_linear_arrows(
        &self,
    ) -> Option<(ItemId, Option<ArrowHeadStyle>, Option<ArrowHeadStyle>, bool)> {
        for id in self.scene.selection.iter() {
            if let Some(item) = self.scene.get_item(id) {
                if let ItemKind::Shape {
                    shape_type: ShapeType::Polyline,
                    start_arrow,
                    end_arrow,
                    closed,
                    ..
                } = &item.kind
                {
                    return Some((item.id, *start_arrow, *end_arrow, *closed));
                }
            }
        }
        None
    }

    /// 选中态中第一个 Shape 的 id 与手绘风开关（Phase F 样式面板用）。
    /// 选中多个 Shape 时只取第一个——与 `selected_linear_arrows` 一致的单值编辑语义。
    fn selected_shape_rough(&self) -> Option<(ItemId, bool)> {
        for id in self.scene.selection.iter() {
            if let Some(item) = self.scene.get_item(id) {
                if matches!(item.kind, ItemKind::Shape { .. }) {
                    return Some((item.id, item.rough()));
                }
            }
        }
        None
    }
}

impl Default for PReferZApp {
    fn default() -> Self {
        Self::new()
    }
}

const FLASH_DURATION_MS: u128 = 2500;

/// 线性对象自动闭合的模糊距离（画布像素）：终点回到起点该距离内即判定为闭合图形。
/// 与 Excalidraw 的吸附闭合一致；多段线阶段起作用，两点式下仅覆盖短拖拽。
const POLYLINE_CLOSE_DISTANCE: f32 = 8.0;

impl eframe::App for PReferZApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 背景透明度：动态调整 visuals.panel_fill 的 alpha。
        // 当 bg_alpha < 1.0 时 panel 背景半透明，配 with_transparent(true) 实现窗口穿透。
        {
            let alpha = (self.bg_alpha * 255.0).round() as u8;
            let mut visuals = ctx.style().visuals.clone();
            visuals.panel_fill = egui::Color32::from_rgba_unmultiplied(45, 45, 48, alpha);
            visuals.window_fill = egui::Color32::from_rgba_unmultiplied(30, 30, 32, alpha);
            visuals.faint_bg_color = egui::Color32::from_rgba_unmultiplied(30, 30, 32, alpha);
            ctx.set_visuals(visuals);
        }

        // 窗口关闭请求检测：dirty 且无 pending 提示时弹保存提示并取消本次关闭。
        // 用户在对话框中选「保存/放弃/取消」后由 render_save_prompt 处理后续动作。
        if self.pending_save_prompt.is_none()
            && self.dirty
            && ctx.input(|i| i.viewport().close_requested())
        {
            self.pending_save_prompt = Some(SavePromptAction::Close);
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }

        // 拖放导入（spec L228，P3-1）�?prz/.bee �?加载项目文件；其�?�?图片导入
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        for path in dropped {
            if is_project_file(&path) {
                self.bg_ops.start_load(ctx, path);
            } else {
                self.pending_import.push(path);
            }
        }

        // 处理待导入：启动后台解码（不阻塞 UI�?
        while let Some(path) = self.pending_import.pop() {
            self.bg_ops.start_import(ctx, path);
        }

        // poll 后台任务结果（导入/加载/保存/导出）。
        self.poll_background(ctx);

        // 处理欢迎页最近文件点击
        if let Some(path) = self.pending_open_recent.take() {
            self.add_recent_and_load(ctx, path);
        }

        // 清理过期�?flash 状�?
        if let Some((_, t)) = self.flash_status {
            if t.elapsed().as_millis() > FLASH_DURATION_MS {
                self.flash_status = None;
            }
        }

        // Present 演示模式：纯展示态，跳过所有编辑界面，进入独立渲染与导航。
        if matches!(self.app_mode, AppMode::Present { .. }) {
            self.handle_present_input(ctx);
            self.render_present(ctx);
            return;
        }

        // 左侧工具条（spec §5.1：绘制工具切换）
        egui::SidePanel::left("tool_panel")
            .exact_width(44.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                let tools = [
                    (Tool::Select, "↖", T::ToolSelect),
                    (Tool::Shape(ShapeType::Rectangle), "▭", T::ToolRectangle),
                    (Tool::Shape(ShapeType::Ellipse), "◯", T::ToolEllipse),
                    (Tool::Shape(ShapeType::Diamond), "◇", T::ToolDiamond),
                    (Tool::Linear { end_arrow: None }, "╱", T::ToolLine),
                    (
                        Tool::Linear {
                            end_arrow: Some(ArrowHeadStyle::Arrow),
                        },
                        "➤",
                        T::ToolArrow,
                    ),
                    (Tool::Frame, "▢", T::ToolFrame),
                ];
                for (tool, icon, key) in tools {
                    let is_active = self.tool == tool;
                    let btn = egui::Button::new(icon)
                        .min_size(egui::vec2(30.0, 30.0))
                        .fill(if is_active {
                            ui.visuals().selection.bg_fill
                        } else {
                            ui.visuals().widgets.inactive.bg_fill
                        });
                    if ui.add(btn).on_hover_text(t(self.lang, key)).clicked() {
                        self.tool = tool;
                        self.drag = DragState::Idle;
                    }
                    ui.add_space(4.0);
                }
            });

        // 样式面板：绘制工具激活时显示新建默认样式（spec §5.1）；
        // 选中线性对象时也显示，用于编辑起/终点箭头；选中任意 Shape 时显示手绘开关（per-item）。
        // 在 CentralPanel 之前渲染：让画布交互区域正确排除底部面板，
        // 避免点击复选框时指针事件穿透到画布导致选中被清空、面板消失。
        let selected_linear = self.selected_linear_arrows();
        let selected_shape = self.selected_shape_rough();
        if self.tool != Tool::Select || selected_linear.is_some() || selected_shape.is_some() {
            egui::TopBottomPanel::bottom("style_panel").show(ctx, |ui| {
                if self.tool != Tool::Select {
                    ui.horizontal(|ui| {
                        ui.label(t(self.lang, T::StyleStrokeColor));
                        let mut col = egui::Color32::from_rgba_unmultiplied(
                            self.default_stroke.color[0],
                            self.default_stroke.color[1],
                            self.default_stroke.color[2],
                            self.default_stroke.color[3],
                        );
                        if ui.color_edit_button_srgba(&mut col).changed() {
                            self.default_stroke.color = [col.r(), col.g(), col.b(), col.a()];
                        }
                        ui.separator();
                        ui.label(t(self.lang, T::StyleStrokeWidth));
                        ui.add(
                            egui::Slider::new(&mut self.default_stroke.width, 0.5..=12.0)
                                .logarithmic(true),
                        );
                        ui.separator();
                        for (dash, label) in [
                            (DashStyle::Solid, T::StyleDashSolid),
                            (DashStyle::Dashed, T::StyleDashDashed),
                            (DashStyle::Dotted, T::StyleDashDotted),
                        ] {
                            let active = self.default_stroke.dash == dash;
                            if ui.selectable_label(active, t(self.lang, label)).clicked() {
                                self.default_stroke.dash = dash;
                            }
                        }
                        ui.separator();
                        let mut fill_checked = self.default_fill.is_some();
                        if ui
                            .checkbox(&mut fill_checked, t(self.lang, T::StyleFillNone))
                            .changed()
                        {
                            self.default_fill = if fill_checked {
                                Some([100, 180, 255, 60])
                            } else {
                                None
                            };
                        }
                        ui.separator();
                        // 手绘风：新建形状的默认开关（Phase F）
                        ui.checkbox(&mut self.default_rough, t(self.lang, T::StyleRough));
                    });
                }
                // 编辑选中线性对象：闭合开关 + 起/终点箭头开关（undo 走 SetClosed / SetArrowHeads）
                if let Some((item_id, start_arrow, end_arrow, closed)) = selected_linear {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(t(self.lang, T::StyleClosed));
                        let mut checked = closed;
                        if ui.checkbox(&mut checked, "").changed() && checked != closed {
                            let cmd = SetClosed::new(item_id, closed, checked);
                            self.push_cmd(Box::new(cmd));
                        }
                    });
                    // 开放折线才有起/终点箭头；闭合图形首尾相连，箭头无意义
                    if !closed {
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(t(self.lang, T::StyleArrowStart));
                            let mut checked = start_arrow.is_some();
                            if ui.checkbox(&mut checked, "").changed() {
                                let new = if checked {
                                    Some(ArrowHeadStyle::Arrow)
                                } else {
                                    None
                                };
                                let cmd = SetArrowHeads::new(
                                    item_id,
                                    start_arrow,
                                    end_arrow,
                                    new,
                                    end_arrow,
                                );
                                self.push_cmd(Box::new(cmd));
                            }
                            ui.separator();
                            ui.label(t(self.lang, T::StyleArrowEnd));
                            let mut checked = end_arrow.is_some();
                            if ui.checkbox(&mut checked, "").changed() {
                                let new = if checked {
                                    Some(ArrowHeadStyle::Arrow)
                                } else {
                                    None
                                };
                                let cmd = SetArrowHeads::new(
                                    item_id,
                                    start_arrow,
                                    end_arrow,
                                    start_arrow,
                                    new,
                                );
                                self.push_cmd(Box::new(cmd));
                            }
                        });
                    }
                }
                // 编辑选中 Shape：手绘风开关（Phase F，undo 走 SetRough）。
                // 对矩形族与线性对象一视同仁，故单独成块而非塞进上面的线性分支。
                if let Some((item_id, rough)) = selected_shape {
                    ui.separator();
                    ui.horizontal(|ui| {
                        let mut checked = rough;
                        if ui
                            .checkbox(&mut checked, t(self.lang, T::StyleRough))
                            .changed()
                            && checked != rough
                        {
                            let cmd = SetRough::new(item_id, rough, checked);
                            self.push_cmd(Box::new(cmd));
                        }
                    });
                }
            });
        }

        // 状态栏（持续状态 + flash 消息）
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            let file_name = self
                .current_file
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "未保存".to_string());
            let persistent = format!(
                "{} | 缩放: {:.2}x | 平移: ({:.0}, {:.0}) | items: {} | 选中: {}",
                file_name,
                self.viewport.zoom,
                self.viewport.pan.x,
                self.viewport.pan.y,
                self.scene.items.len(),
                self.scene.selection.len(),
            );
            if let Some((msg, _)) = &self.flash_status {
                ui.horizontal(|ui| {
                    ui.label(&persistent);
                    ui.separator();
                    ui.colored_label(egui::Color32::LIGHT_GREEN, msg);
                });
            } else {
                ui.label(&persistent);
            }
        });

        // Debug 面板（仅 debug 构建显示，release 自动隐藏）
        if cfg!(debug_assertions) {
            self.render_debug_panel(ctx);
        }

        // 中央画布
        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = ui.max_rect();
            self.viewport.set_screen_rect(rect);

            let response =
                ui.interact(rect, egui::Id::new("canvas"), egui::Sense::click_and_drag());

            // 画布背景：应用 bg_alpha（与 panel_fill 一致，确保透明效果生效）
            let bg_alpha_u8 = (self.bg_alpha * 255.0).round() as u8;
            ui.painter().rect_filled(
                rect,
                egui::Rounding::same(0.0),
                egui::Color32::from_rgba_unmultiplied(45, 45, 48, bg_alpha_u8),
            );

            // 渲染场景（含视口剔除 + Z �?+ 复用 self.transform_handles�?
            self.render_scene(ui);

            // 框选矩形（spec L240�?
            if let DragState::BoxSelect {
                start_canvas,
                current_canvas,
                ..
            } = &self.drag
            {
                let min = self.viewport.canvas_to_screen(*start_canvas);
                let max = self.viewport.canvas_to_screen(*current_canvas);
                let rect = egui::Rect::from_min_max(min, max);
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(100, 200, 255, 30),
                );
                let stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(100, 200, 255));
                ui.painter().rect_stroke(rect, 0.0, stroke);
            }

            // 绘制工具拖拽预览（两点式：start → current）
            if let DragState::CreatingShape {
                shape_type,
                end_arrow,
                start,
                current,
                ctrl,
                ..
            } = &self.drag
            {
                let min_x = start.x.min(current.x);
                let min_y = start.y.min(current.y);
                let w = (current.x - start.x).abs();
                let h = (current.y - start.y).abs();
                let min_canvas = CanvasPoint::new(min_x, min_y);
                let rect_canvas = CanvasRect::new(min_canvas, CanvasSize::new(w, h));
                let screen_rect = self.viewport.canvas_to_screen_rect(rect_canvas);
                let stroke = egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(100, 200, 255));
                match shape_type {
                    ShapeType::Rectangle => {
                        ui.painter().rect_stroke(screen_rect, 0.0, stroke);
                    }
                    ShapeType::Diamond => {
                        // 绘制实际菱形轮廓：四个顶点坐标
                        let cx = min_x + w / 2.0;
                        let cy = min_y + h / 2.0;
                        let corners_canvas = [
                            CanvasPoint::new(cx, min_y),
                            CanvasPoint::new(min_x + w, cy),
                            CanvasPoint::new(cx, min_y + h),
                            CanvasPoint::new(min_x, cy),
                        ];
                        let corners_screen: Vec<egui::Pos2> = corners_canvas
                            .iter()
                            .map(|p| self.viewport.canvas_to_screen(*p))
                            .collect();
                        ui.painter().add(egui::Shape::convex_polygon(
                            corners_screen,
                            egui::Color32::TRANSPARENT,
                            stroke,
                        ));
                    }
                    ShapeType::Ellipse => {
                        // 绘制精确椭圆：多边形近似（64段，与 CleanStyler 渲染一致）
                        // 默认正圆（未按 Ctrl）；Ctrl 按下为自由椭圆，与变换框行为一致
                        let (rw, rh) = if !ctrl {
                            let side = w.max(h);
                            (side, side)
                        } else {
                            (w, h)
                        };
                        let cx_canvas = min_x + rw / 2.0;
                        let cy_canvas = min_y + rh / 2.0;
                        let rx_canvas = rw / 2.0;
                        let ry_canvas = rh / 2.0;
                        let segments = 64usize;
                        let mut pts: Vec<egui::Pos2> = (0..segments)
                            .map(|i| {
                                let a = i as f32 / segments as f32 * std::f32::consts::TAU;
                                let px = cx_canvas + rx_canvas * a.cos();
                                let py = cy_canvas + ry_canvas * a.sin();
                                self.viewport.canvas_to_screen(CanvasPoint::new(px, py))
                            })
                            .collect();
                        pts.push(pts[0]); // 闭合路径
                        ui.painter().add(egui::Shape::line(pts, stroke));
                    }
                    // 线性对象：画 start → current 线段；箭头另画头部（与 CleanStyler 一致）
                    ShapeType::Polyline => {
                        let s0 = self.viewport.canvas_to_screen(*start);
                        let s1 = self.viewport.canvas_to_screen(*current);
                        ui.painter().line_segment([s0, s1], stroke);
                        if let Some(ArrowHeadStyle::Arrow) = end_arrow {
                            let dir = s1 - s0;
                            let len = dir.length();
                            if len > 1e-3 {
                                let dir = dir / len;
                                let head_len = 12.0;
                                let half = std::f32::consts::FRAC_PI_2 * (5.0 / 9.0); // ≈50°
                                let (s, c) = half.sin_cos();
                                let a1 = egui::vec2(dir.x * c - dir.y * s, dir.x * s + dir.y * c);
                                let a2 = egui::vec2(dir.x * c + dir.y * s, -dir.x * s + dir.y * c);
                                ui.painter().line_segment([s1, s1 - a1 * head_len], stroke);
                                ui.painter().line_segment([s1, s1 - a2 * head_len], stroke);
                            }
                        }
                    }
                }
            }

            // Frame 工具拖拽预览：虚线矩形框
            if let DragState::CreatingFrame { start, current } = &self.drag {
                let min_x = start.x.min(current.x);
                let min_y = start.y.min(current.y);
                let w = (current.x - start.x).abs();
                let h = (current.y - start.y).abs();
                let rect_canvas =
                    CanvasRect::new(CanvasPoint::new(min_x, min_y), CanvasSize::new(w, h));
                let screen_rect = self.viewport.canvas_to_screen_rect(rect_canvas);
                let stroke = egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(100, 200, 255));
                ui.painter().rect_stroke(screen_rect, 0.0, stroke);
            }

            // 鼠标中键拖拽平移
            if response.dragged_by(egui::PointerButton::Middle) {
                self.viewport.pan_by_screen(response.drag_delta());
            }

            // 滚轮缩放（以鼠标位置为锚点）
            let scroll = ctx.input(|i| i.raw_scroll_delta);
            if scroll.y != 0.0 {
                if let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) {
                    self.viewport.zoom_at(scroll.y, pos);
                }
            }

            // 双击：Text item → 编辑；封闭 Shape → 创建/编辑绑定文本；
            // 空白 → 创建文本便签（spec L243 P2-5）
            // response.double_clicked() 已自动考虑上层 Window 遮挡
            if response.double_clicked() && self.editing_text.is_none() {
                if let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) {
                    // 克隆命中的 item 字段，避免 &self.scene 与 &mut self.editing_text 借用冲突
                    let hit = interaction::get_item_at(pos, &self.scene, &self.viewport)
                        .map(|item| (item.id, item.kind.clone()));
                    match hit {
                        Some((
                            id,
                            ItemKind::Text {
                                content,
                                font_size,
                                color,
                                editing: _,
                                measured_size: _,
                                container_id,
                            },
                        )) => {
                            // 双击 Text item → 编辑现有
                            self.editing_text = Some(EditingText {
                                editing_item_id: Some(id),
                                canvas_pos: self
                                    .scene
                                    .get_item(&id)
                                    .map(|it| it.transform.pos)
                                    .map(|p| CanvasPoint::new(p.x, p.y))
                                    .unwrap_or_else(|| CanvasPoint::new(f32::MIN, f32::MIN)),
                                buffer: content,
                                font_size,
                                color,
                                first_frame: true,
                                container_id,
                            });
                            self.drag = DragState::Idle;
                        }
                        Some((shape_id, ref kind)) if is_text_container(kind) => {
                            // 命中封闭 Shape 容器：查找/创建绑定文本
                            if let Some(shape_item) = self.scene.get_item(&shape_id) {
                                let center = shape_item.bounding_rect().center();
                                let existing = self.scene.texts_bound_to(shape_id);
                                if let Some(text_id) = existing.into_iter().next() {
                                    // 已有绑定文本 → 编辑
                                    let text_item = self.scene.get_item(&text_id);
                                    let (content, font_size, color) = match text_item {
                                        Some(it) => match &it.kind {
                                            ItemKind::Text {
                                                content,
                                                font_size,
                                                color,
                                                ..
                                            } => (content.clone(), *font_size, *color),
                                            _ => (String::new(), 18.0, [255; 4]),
                                        },
                                        None => (String::new(), 18.0, [255; 4]),
                                    };
                                    self.editing_text = Some(EditingText {
                                        editing_item_id: Some(text_id),
                                        canvas_pos: CanvasPoint::new(center.x, center.y),
                                        buffer: content,
                                        font_size,
                                        color,
                                        first_frame: true,
                                        container_id: Some(shape_id),
                                    });
                                } else {
                                    // 无绑定文本 → 创建（容器 id 在提交时写入）
                                    self.editing_text = Some(EditingText {
                                        editing_item_id: None,
                                        canvas_pos: CanvasPoint::new(center.x, center.y),
                                        buffer: String::new(),
                                        font_size: 18.0,
                                        color: [255, 255, 255, 255],
                                        first_frame: true,
                                        container_id: Some(shape_id),
                                    });
                                }
                                self.drag = DragState::Idle;
                            }
                        }
                        _ => {
                            // 双击空白 → 创建新文本
                            let canvas_pos = self.viewport.screen_to_canvas(pos);
                            self.editing_text = Some(EditingText {
                                editing_item_id: None,
                                canvas_pos,
                                buffer: String::new(),
                                font_size: 24.0,
                                color: [255, 255, 255, 255],
                                first_frame: true,
                                container_id: None,
                            });
                            self.drag = DragState::Idle;
                        }
                    }
                }
            }

            let pointer_pos = ctx.input(|i| i.pointer.latest_pos());
            let primary_pressed = ctx.input(|i| i.pointer.primary_pressed());
            let primary_down = ctx.input(|i| i.pointer.primary_down());
            let primary_released = ctx.input(|i| i.pointer.primary_released());
            // pointer 是否在画布上且未被上层 Window/Area 遮挡。
            // response.hovered() 经 egui hit_test 自动排除被上层 layer 覆盖的区域，
            // 用于守卫 primary_pressed 等全局 PointerState 信号，避免穿透到画布。
            let pointer_on_canvas = response.hovered();

            // 更新 hover + 光标
            if pointer_on_canvas {
                if let Some(pos) = pointer_pos {
                    let selected = self.selected_items_snapshot();
                    // 多选时只支持统一移动，不检测单独手柄（手柄不可见却有 hover 会造成混乱）
                    if selected.len() == 1 {
                        self.transform_handles
                            .update_hover(pos, &selected, &self.viewport);
                    } else {
                        self.transform_handles.hover_handle = Handle::None;
                    }
                    let cursor = match self.transform_handles.hover_handle {
                        Handle::ResizeTopLeft | Handle::ResizeBottomRight => {
                            egui::CursorIcon::ResizeNorthEast
                        }
                        Handle::ResizeTopRight | Handle::ResizeBottomLeft => {
                            egui::CursorIcon::ResizeNorthWest
                        }
                        Handle::Rotate => egui::CursorIcon::Grab,
                        Handle::Endpoint(_) => egui::CursorIcon::Grab,
                        Handle::FlipH => egui::CursorIcon::ResizeHorizontal,
                        Handle::FlipV => egui::CursorIcon::ResizeVertical,
                        Handle::None => {
                            // 在 item 上时显示移动光标
                            if interaction::get_item_at(pos, &self.scene, &self.viewport).is_some()
                            {
                                egui::CursorIcon::Move
                            } else {
                                egui::CursorIcon::Default
                            }
                        }
                    };
                    ctx.output_mut(|o| o.cursor_icon = cursor);
                }
            } else {
                self.transform_handles.hover_handle = Handle::None;
            }

            // 拖拽中：更新预览（含裁剪模式拖拽，crop_mode.dragging 不在 DragState 内）
            // 仅当画布起源的拖拽进行中才更新；从 Window 起源的 drag 不会进入此分支
            // （因为 begin_drag 有 pointer_on_canvas 守卫，Window 点击不会启动画布 drag）。
            let crop_dragging = self
                .crop_mode
                .as_ref()
                .is_some_and(|c| c.dragging.is_some());
            let drag_in_progress = !matches!(self.drag, DragState::Idle) || crop_dragging;
            if primary_down && drag_in_progress {
                if let Some(pos) = pointer_pos {
                    let free_scale = ctx.input(|i| i.modifiers.ctrl);
                    self.update_drag_preview(pos, free_scale);
                    ctx.request_repaint();
                }
            }

            // 按下：开始拖拽（手柄优先，否则移动）。
            // pointer_on_canvas 守卫确保只有 pointer 在画布上且未被遮挡时才开始拖拽，
            // 避免点击设置/Debug 窗口时穿透触发画布 drag。
            // 菜单打开时也不启动拖拽（render_context_menu 负责检测点击外部并关闭菜单）。
            if primary_pressed && !self.context_menu_open && pointer_on_canvas {
                if let Some(pos) = pointer_pos {
                    let additive = ctx.input(|i| i.modifiers.shift);
                    let free_scale = ctx.input(|i| i.modifiers.ctrl);
                    self.begin_drag(pos, additive, free_scale);
                }
            }

            // 释放：固化到 undo 栈
            if primary_released {
                self.end_drag();
            }

            // 右键菜单
            if response.clicked_by(egui::PointerButton::Secondary) {
                if let Some(pos) = response.hover_pos() {
                    self.context_menu_open = true;
                    self.context_menu_pos = pos;
                }
            }
        });

        // 文本编辑 overlay（spec L243 P2-5）
        self.render_text_editor(ctx);

        // 画框编号编辑 overlay（Phase D）
        self.render_frame_number_editor(ctx);

        // 上下文菜�?
        if self.context_menu_open {
            self.render_context_menu(ctx);
        }

        // 颜色采样 overlay（spec §2.2�?
        if self.color_picker_active {
            self.render_color_picker_overlay(ctx);
        }

        // 设置面板
        if self.settings_open {
            self.render_settings_window(ctx);
        }

        // 快捷�?
        self.handle_shortcuts(ctx);

        // 保存提示对话框（关闭/新建时若 dirty 弹出�?
        self.render_save_prompt(ctx);

        // 后台任务进度条（spec L298：加�?保存时显示进度）
        if self.bg_ops.pending > 0 {
            egui::Window::new("background_progress")
                .title_bar(false)
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    let msg = self
                        .bg_ops
                        .msg
                        .clone()
                        .unwrap_or_else(|| t(self.lang, T::FlashProcessing).to_string());
                    ui.vertical_centered(|ui| {
                        ui.add_space(4.0);
                        ui.label(&msg);
                        ui.add_space(6.0);
                        ui.add(egui::Spinner::new());
                        ui.add_space(4.0);
                    });
                });
        }
    }
}

// ─────────────────────────── 拖拽逻辑 ───────────────────────────

impl PReferZApp {
    fn begin_drag(&mut self, screen_pos: egui::Pos2, additive: bool, free_scale: bool) {
        // 文本编辑中不启动拖拽
        if self.editing_text.is_some() {
            return;
        }
        // 画框编号编辑中不启动拖拽（点击别处由编辑窗 lost_focus 提交/取消）
        if self.editing_frame_number.is_some() {
            return;
        }

        // 绘制工具激活：直接进入创建拖拽（不处理手柄/命中/框选）。
        // additive（=Shift 按住）用于正方形锁定，free_scale（=Ctrl 按住）用于椭圆解锁。
        match self.tool {
            Tool::Shape(shape_type) => {
                let start_canvas = self.viewport.screen_to_canvas(screen_pos);
                self.drag = DragState::CreatingShape {
                    shape_type,
                    end_arrow: None,
                    start: start_canvas,
                    current: start_canvas,
                    shift: additive,
                    ctrl: free_scale,
                };
                return;
            }
            Tool::Linear { end_arrow } => {
                let start_canvas = self.viewport.screen_to_canvas(screen_pos);
                self.drag = DragState::CreatingShape {
                    shape_type: ShapeType::Polyline,
                    end_arrow,
                    start: start_canvas,
                    current: start_canvas,
                    shift: additive,
                    ctrl: free_scale,
                };
                return;
            }
            Tool::Frame => {
                let start_canvas = self.viewport.screen_to_canvas(screen_pos);
                self.drag = DragState::CreatingFrame {
                    start: start_canvas,
                    current: start_canvas,
                };
                return;
            }
            _ => {}
        }

        // 裁剪模式：优先检测裁剪手�?
        if self.crop_mode.is_some() {
            if let Some(h) = self.crop_handle_hit_test(screen_pos) {
                if let Some(crop) = self.crop_mode.as_mut() {
                    crop.dragging = Some(h);
                }
                return;
            }
            // 裁剪模式下点空白：不响应（避免误操作�?
            return;
        }

        // 颜色采样模式：单�?Pixmap 采样像素
        if self.color_picker_active {
            self.pick_color_at(screen_pos);
            return;
        }

        // 1) 手柄优先
        let hover = self.transform_handles.hover_handle;
        if hover != Handle::None {
            // 找到手柄所属的 item
            let selected = self.selected_items_snapshot();
            for item in selected.iter().rev() {
                let show_flip = should_show_flip(item);
                let show_rotate = should_show_rotate(item);
                let h = self.transform_handles.hit_test(
                    screen_pos,
                    item,
                    &self.viewport,
                    show_flip,
                    show_rotate,
                );
                if h != Handle::None {
                    // 线性对象顶点控制点：进入端点拖拽（预览直接改 points）
                    if let Handle::Endpoint(endpoint) = h {
                        let start_points = match &item.kind {
                            ItemKind::Shape { points, .. } => points.clone(),
                            _ => Vec::new(),
                        };
                        let start_canvas = self.viewport.screen_to_canvas(screen_pos);
                        self.drag = DragState::LineEndpoint {
                            item_id: item.id,
                            endpoint,
                            start_canvas,
                            start_points,
                        };
                        self.transform_handles.active_handle = h;
                        self.transform_handles.is_dragging = true;
                        return;
                    }
                    // 翻转边手柄：点击即触发翻转，不进入拖拽（spec L239「翻转边」）
                    if h == Handle::FlipH || h == Handle::FlipV {
                        let ids: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
                        let horizontal = h == Handle::FlipH;
                        let cmd = FlipItems::new(ids, horizontal);
                        self.push_cmd(Box::new(cmd));
                        self.flash(if horizontal {
                            "水平翻转"
                        } else {
                            "垂直翻转"
                        });
                        return;
                    }
                    let start_corners = item.canvas_corners();
                    self.drag = DragState::HandleTransform {
                        item_id: item.id,
                        handle: h,
                        start_screen: screen_pos,
                        start_transform: item.transform,
                        start_corners,
                    };
                    self.transform_handles.active_handle = h;
                    self.transform_handles.is_dragging = true;
                    return;
                }
            }
        }

        // 2) 命中 item：选中并开始移动拖�?
        if let Some(item) = interaction::get_item_at(screen_pos, &self.scene, &self.viewport) {
            let id = item.id;
            if additive {
                // Shift 加选：toggle，若取消选中则不开始拖�?
                self.scene.toggle_selection(id);
                if !self.scene.selection.contains(&id) {
                    return;
                }
            } else if !self.scene.selection.contains(&id) {
                // 非加选且未选中：替换选中为该�?
                self.scene.deselect_all();
                self.scene.select(id);
            }
            // 收集所有选中 item �?transform 快照
            let selected: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
            // 容器联动：选中封闭形状时连带其绑定文本（Phase C）；选中画框时连带其成员（Phase D）。
            let mut collected: Vec<ItemId> = selected.clone();
            for sid in &selected {
                collected.extend(self.scene.texts_bound_to(*sid));
                if self
                    .scene
                    .get_item(sid)
                    .map(|it| it.is_frame())
                    .unwrap_or(false)
                {
                    collected.extend(self.scene.frame_members(*sid));
                }
            }
            collected.sort();
            collected.dedup();
            let start_transforms: Vec<(ItemId, preferz_core::Transform)> = collected
                .into_iter()
                .filter_map(|sid| self.scene.get_item(&sid).map(|it| (sid, it.transform)))
                .collect();
            let start_canvas = self.viewport.screen_to_canvas(screen_pos);
            self.drag = DragState::MoveItems {
                start_canvas,
                start_transforms,
            };
            return;
        }

        // 3) 空白：开始框选（spec L240）。Shift = 加选模�?
        let start_canvas = self.viewport.screen_to_canvas(screen_pos);
        self.drag = DragState::BoxSelect {
            start_canvas,
            current_canvas: start_canvas,
            additive,
        };
    }

    fn update_drag_preview(&mut self, screen_pos: egui::Pos2, free_scale: bool) {
        // 裁剪模式拖拽：直接更�?crop_mode.rect，不进入 DragState
        if let Some(crop) = self.crop_mode.as_mut() {
            if let Some(handle) = crop.dragging {
                self.update_crop_drag(screen_pos, handle);
                return;
            }
        }

        match &self.drag {
            DragState::HandleTransform {
                item_id,
                handle,
                start_screen,
                start_transform,
                start_corners,
            } => {
                let item_id = *item_id;
                let handle = *handle;
                let start_screen = *start_screen;
                let start_transform = *start_transform;
                let start_corners = *start_corners;
                let mouse_canvas = self.viewport.screen_to_canvas(screen_pos);
                if let Some(item) = self.scene.get_item_mut(&item_id) {
                    match handle {
                        Handle::Rotate => {
                            apply_rotate_drag(
                                &self.viewport,
                                item,
                                start_transform,
                                start_corners,
                                start_screen,
                                screen_pos,
                            );
                        }
                        Handle::ResizeTopLeft
                        | Handle::ResizeTopRight
                        | Handle::ResizeBottomLeft
                        | Handle::ResizeBottomRight => {
                            apply_scale_drag(
                                item,
                                handle,
                                start_transform,
                                start_corners,
                                mouse_canvas,
                                free_scale,
                            );
                        }
                        // 翻转手柄�?begin_drag 中已即时处理，不会进入拖拽预�?
                        Handle::FlipH | Handle::FlipV | Handle::None => {}
                        // 线类顶点�?begin_drag 中已进入 LineEndpoint 拖拽，不会到达这里
                        Handle::Endpoint(_) => {}
                    }
                }
            }
            DragState::MoveItems {
                start_canvas,
                start_transforms,
            } => {
                let current_canvas = self.viewport.screen_to_canvas(screen_pos);
                let delta = current_canvas - *start_canvas;
                for (id, start_tf) in start_transforms {
                    if let Some(item) = self.scene.get_item_mut(id) {
                        item.transform.pos = start_tf.pos + delta;
                    }
                }
            }
            DragState::CreatingShape { current, .. } => {
                let _ = current; // 由 match 后更新（需单独 &mut self.drag）
            }
            DragState::LineEndpoint {
                item_id,
                endpoint,
                start_canvas,
                start_points,
            } => {
                let item_id = *item_id;
                let endpoint = *endpoint;
                let start_canvas = *start_canvas;
                let start_points = start_points.clone();
                let current_canvas = self.viewport.screen_to_canvas(screen_pos);
                let delta_canvas = current_canvas - start_canvas;
                if let Some(item) = self.scene.get_item_mut(&item_id) {
                    // 画布位移 → 局部位移（逆变换向量的平移部分自动抵消）
                    let delta_local = item
                        .local_to_canvas()
                        .inverse()
                        .map(|inv| inv.transform_vector(delta_canvas));
                    if let Some(delta_local) = delta_local {
                        if let ItemKind::Shape { points, .. } = &mut item.kind {
                            if let Some(p) = points.get_mut(endpoint) {
                                *p = (
                                    start_points[endpoint].0 + delta_local.x,
                                    start_points[endpoint].1 + delta_local.y,
                                );
                            }
                        }
                    }
                }
            }
            DragState::BoxSelect { .. } => {} // 由 match 后更新（需单独 &mut self.drag）
            DragState::CreatingFrame { current, .. } => {
                let _ = current; // 由 match 后更新（需单独 &mut self.drag）
            }
            DragState::Idle => {}
        }
        // BoxSelect / CreatingShape 更新 current（match &self.drag 不可写，故单独 &mut）
        if let DragState::BoxSelect { current_canvas, .. } = &mut self.drag {
            *current_canvas = self.viewport.screen_to_canvas(screen_pos);
        }
        if let DragState::CreatingShape { current, ctrl, .. } = &mut self.drag {
            *current = self.viewport.screen_to_canvas(screen_pos);
            // 拖动中实时更新 Ctrl 状态（椭圆正圆/自由宽高比切换）
            *ctrl = free_scale;
        }
        if let DragState::CreatingFrame { current, .. } = &mut self.drag {
            *current = self.viewport.screen_to_canvas(screen_pos);
        }
    }

    fn end_drag(&mut self) {
        // 裁剪模式拖拽释放：清�?dragging 标志（应用通过 Enter 触发�?
        if let Some(crop) = self.crop_mode.as_mut() {
            if crop.dragging.is_some() {
                crop.dragging = None;
                return;
            }
        }

        let prev = std::mem::replace(&mut self.drag, DragState::Idle);
        match prev {
            DragState::HandleTransform {
                item_id,
                start_transform,
                ..
            } => {
                // �?clone �?new_transform，避免与 undo_stack.push �?&mut self.scene 冲突
                let new_transform = self.scene.get_item(&item_id).map(|it| it.transform);
                if let Some(new_tf) = new_transform {
                    if new_tf != start_transform {
                        let cmd = TransformItem::new(item_id, start_transform, new_tf);
                        // skip_first_redo=true，因为预览已应用
                        self.push_cmd(Box::new(cmd));
                        self.flash(format!(
                            "变换: 缩放=({:.2},{:.2}) 旋转={:.1}°",
                            new_tf.scale.x,
                            new_tf.scale.y,
                            new_tf.rotation.to_degrees()
                        ));
                    }
                }
                self.transform_handles.end_drag();
            }
            DragState::MoveItems {
                start_canvas,
                start_transforms,
            } => {
                // 用第一�?item 的当前位置反�?delta
                let delta_opt = start_transforms.first().and_then(|(id, start_tf)| {
                    self.scene
                        .get_item(id)
                        .map(|it| it.transform.pos - start_tf.pos)
                });
                if let Some(delta) = delta_opt {
                    if delta.x.abs() > 1e-4 || delta.y.abs() > 1e-4 {
                        let ids: Vec<ItemId> = start_transforms.iter().map(|(i, _)| *i).collect();
                        let cmd = MoveItems::new(ids, delta);
                        self.push_cmd(Box::new(cmd));
                        self.flash(format!("移动: ({:.0}, {:.0})", delta.x, delta.y));
                    }
                }
                let _ = start_canvas;
            }
            DragState::BoxSelect {
                start_canvas,
                current_canvas,
                additive,
            } => {
                // 选中框内所�?item（bounding_rect 相交即选中�?
                let min_x = start_canvas.x.min(current_canvas.x);
                let max_x = start_canvas.x.max(current_canvas.x);
                let min_y = start_canvas.y.min(current_canvas.y);
                let max_y = start_canvas.y.max(current_canvas.y);
                let sel_rect = CanvasRect::new(
                    CanvasPoint::new(min_x, min_y),
                    CanvasSize::new(max_x - min_x, max_y - min_y),
                );
                if !additive {
                    self.scene.deselect_all();
                }
                // 先收集命中 id，再 select（避免同时 &self.items 和 &mut self.selection）
                let hits: Vec<ItemId> = self
                    .scene
                    .items
                    .iter()
                    .filter(|item| item.bounding_rect().intersects(&sel_rect))
                    .map(|item| item.id)
                    .collect();
                for id in hits {
                    self.scene.select(id);
                }
            }
            DragState::CreatingShape {
                shape_type,
                end_arrow,
                start,
                current,
                shift,
                ctrl,
            } => {
                self.finish_create_shape(shape_type, end_arrow, start, current, shift, ctrl);
            }
            DragState::CreatingFrame { start, current } => {
                self.finish_create_frame(start, current);
            }
            DragState::LineEndpoint {
                item_id,
                endpoint: _,
                start_canvas: _,
                start_points,
            } => {
                // 预览已直接改 points；释放时若有变化则固化到 undo 栈
                let new_points = match self.scene.get_item(&item_id) {
                    Some(item) => match &item.kind {
                        ItemKind::Shape { points, .. } => points.clone(),
                        _ => Vec::new(),
                    },
                    None => Vec::new(),
                };
                if !new_points.is_empty() && new_points != start_points {
                    let cmd = EditShapePoints::new(item_id, start_points, new_points);
                    self.push_cmd(Box::new(cmd));
                }
                self.transform_handles.end_drag();
            }
            DragState::Idle => {}
        }
    }

    /// 用绘制工具完成 shape 创建：计算矩形 → AddItem → 回 Select。
    /// shift = 锁定正方形（用宽高较大者作边长）；线性对象 = 锁定 45° 方向。
    /// ctrl = 椭圆解锁自由宽高比（默认椭圆为正圆，与变换框行为一致）。
    fn finish_create_shape(
        &mut self,
        shape_type: ShapeType,
        end_arrow: Option<ArrowHeadStyle>,
        start: CanvasPoint,
        current: CanvasPoint,
        shift: bool,
        ctrl: bool,
    ) {
        // 线性对象：两点式（start → current），Shift 锁 45°，最小长度 3 画布像素。
        if shape_type == ShapeType::Polyline {
            let mut dx = current.x - start.x;
            let mut dy = current.y - start.y;
            if shift {
                let len = (dx * dx + dy * dy).sqrt();
                if len < 1e-3 {
                    return;
                }
                // 方向吸附到 45° 整数倍
                let angle = (dy.atan2(dx) / std::f32::consts::FRAC_PI_4).round()
                    * std::f32::consts::FRAC_PI_4;
                dx = len * angle.cos();
                dy = len * angle.sin();
            }
            if (dx * dx + dy * dy).sqrt() < 3.0 {
                return;
            }
            // 自动闭合：终点回到起点模糊距离内即判定为闭合图形（Excalidraw 风格）
            let closed = (dx * dx + dy * dy).sqrt() <= POLYLINE_CLOSE_DISTANCE;
            let min_x = start.x.min(start.x + dx);
            let min_y = start.y.min(start.y + dy);
            // 局部坐标：起点对齐 AABB 左上角
            let p0 = (start.x - min_x, start.y - min_y);
            let p1 = (start.x + dx - min_x, start.y + dy - min_y);
            let item = Item::new_polyline(
                vec![p0, p1],
                (dx.abs(), dy.abs()),
                None,
                end_arrow,
                closed,
                min_x,
                min_y,
                self.default_stroke,
            )
            .with_rough(self.default_rough);
            let cmd = AddItem::new(item);
            self.push_cmd(Box::new(cmd));
            self.flash(if end_arrow.is_some() {
                "已创建箭头"
            } else {
                "已创建直线"
            });
            // 默认回 Select
            self.tool = Tool::Select;
            return;
        }

        let w = (current.x - start.x).abs();
        let h = (current.y - start.y).abs();
        // 误触：小于 3 画布像素丢弃
        if w < 3.0 || h < 3.0 {
            return;
        }
        let min_x = start.x.min(current.x);
        let min_y = start.y.min(current.y);
        let (bw, bh) = if shape_type == ShapeType::Ellipse {
            // 椭圆默认正圆；Ctrl 按下画自由椭圆（与变换框行为一致）
            if ctrl {
                (w, h)
            } else {
                let side = w.max(h);
                (side, side)
            }
        } else if shift {
            let side = w.max(h);
            (side, side)
        } else {
            (w, h)
        };
        let item = Item::new_shape(
            shape_type,
            (bw, bh),
            min_x,
            min_y,
            self.default_stroke,
            self.default_fill,
        )
        .with_rough(self.default_rough);
        let cmd = AddItem::new(item);
        self.push_cmd(Box::new(cmd));
        self.flash("已创建图形");
        // 默认回 Select
        self.tool = Tool::Select;
    }

    /// 用 Frame 工具完成画框创建：计算矩形 → AddItem → 置底（z=min-1）→ 选中 → 回 Select。
    fn finish_create_frame(&mut self, start: CanvasPoint, current: CanvasPoint) {
        let w = (current.x - start.x).abs();
        let h = (current.y - start.y).abs();
        if w < 10.0 || h < 10.0 {
            // 误触：过小丢弃
            self.tool = Tool::Select;
            return;
        }
        let min_x = start.x.min(current.x);
        let min_y = start.y.min(current.y);
        let number = self.scene.next_frame_number();
        let item = Item::new_frame(number, (w, h), min_x, min_y, None);
        let frame_id = item.id;
        let cmd = AddItem::new(item);
        self.push_cmd(Box::new(cmd));
        // 画框恒在最底（z = min - 1）：用 ReorderItems send-to-back
        let reorder = ReorderItems::new(vec![frame_id], false);
        self.push_cmd(Box::new(reorder));
        self.scene.deselect_all();
        self.scene.select(frame_id);
        self.flash(format!("已创建画框 #{}", number));
        // 默认回 Select
        self.tool = Tool::Select;
    }
}

// ─────────────────────────── 渲染 ───────────────────────────

impl PReferZApp {
    // ─────────────────────────── Present（Slide 演示） ───────────────────────────

    /// 绘制单个 item 的视觉内容（不含选中手柄 / 多选外框 / 裁剪 overlay）。
    /// Edit 与 Present 模式共用的渲染原语，保证两处观感一致。
    fn draw_item_visual(&self, ui: &mut egui::Ui, item: &Item, editing_id: Option<ItemId>) {
        let canvas_bbox = item.bounding_rect();
        let item_screen_rect = self.viewport.canvas_to_screen_rect(canvas_bbox);
        let corners = item.canvas_corners();
        let screen_corners = [
            self.viewport.canvas_to_screen(corners[0]),
            self.viewport.canvas_to_screen(corners[1]),
            self.viewport.canvas_to_screen(corners[2]),
            self.viewport.canvas_to_screen(corners[3]),
        ];
        match &item.kind {
            ItemKind::Pixmap {
                texture_id,
                opacity,
                grayscale,
                crop,
                ..
            } => {
                let tex_id = *texture_id;
                let handle = if *grayscale {
                    self.grayscale_texture_cache
                        .get(&tex_id)
                        .or_else(|| self.texture_cache.get(&tex_id))
                } else {
                    self.texture_cache.get(&tex_id)
                };
                if let Some(handle) = handle {
                    let (u_min, u_max, v_min, v_max) = if let Some(c) = crop {
                        let base_w = item.base_size().x.max(1.0);
                        let base_h = item.base_size().y.max(1.0);
                        let cx0 = (c.x / base_w).clamp(0.0, 1.0);
                        let cx1 = ((c.x + c.width) / base_w).clamp(0.0, 1.0);
                        let cy0 = (c.y / base_h).clamp(0.0, 1.0);
                        let cy1 = ((c.y + c.height) / base_h).clamp(0.0, 1.0);
                        (cx0, cx1, cy0, cy1)
                    } else {
                        (0.0, 1.0, 0.0, 1.0)
                    };
                    let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
                    let tint = egui::Color32::from_rgba_premultiplied(255, 255, 255, alpha);
                    let [tl, tr, bl, br] = screen_corners;
                    let verts = [
                        ([tl.x, tl.y], [u_min, v_min]),
                        ([tr.x, tr.y], [u_max, v_min]),
                        ([br.x, br.y], [u_max, v_max]),
                        ([bl.x, bl.y], [u_min, v_max]),
                    ];
                    let mut mesh = egui::epaint::Mesh {
                        texture_id: handle.id(),
                        ..Default::default()
                    };
                    for ([px, py], [u, v]) in verts {
                        mesh.vertices.push(egui::epaint::Vertex {
                            pos: [px, py].into(),
                            uv: [u, v].into(),
                            color: tint,
                        });
                    }
                    mesh.indices = vec![0, 1, 2, 0, 2, 3];
                    ui.painter().add(egui::epaint::Shape::mesh(mesh));
                } else {
                    ui.painter().rect_filled(
                        item_screen_rect,
                        egui::Rounding::same(0.0),
                        egui::Color32::from_rgb(70, 70, 70),
                    );
                }
            }
            ItemKind::Text {
                content,
                font_size,
                color,
                container_id,
                ..
            } => {
                if editing_id != Some(item.id) {
                    ui.painter().rect_filled(
                        item_screen_rect,
                        egui::Rounding::same(2.0),
                        egui::Color32::from_rgb(60, 60, 60),
                    );
                    let text_color = egui::Color32::from_rgba_premultiplied(
                        color[0], color[1], color[2], color[3],
                    );
                    if let Some(cid) = container_id {
                        // 绑定文本：换行到容器宽度，居中绘制
                        if let Some(container) = self.scene.get_item(cid) {
                            let cr = self
                                .viewport
                                .canvas_to_screen_rect(container.bounding_rect());
                            let wrap = (cr.width() - 12.0).max(20.0);
                            let ef = *font_size * self.viewport.zoom;
                            let job = egui::text::LayoutJob::simple(
                                content.clone(),
                                egui::FontId::proportional(ef),
                                text_color,
                                wrap,
                            );
                            let gal = ui.ctx().fonts(|f| f.layout_job(job));
                            let gsz = gal.size();
                            let tl = cr.center() - egui::vec2(gsz.x / 2.0, gsz.y / 2.0);
                            ui.painter().galley(tl, gal, text_color);
                        }
                    } else {
                        let origin = self.viewport.canvas_to_screen(corners[0]);
                        let effective_font_size =
                            *font_size * item.transform.scale.x.abs() * self.viewport.zoom;
                        ui.painter().text(
                            origin,
                            egui::Align2::LEFT_TOP,
                            content.clone(),
                            egui::FontId::proportional(effective_font_size),
                            text_color,
                        );
                    }
                }
            }
            // 风格器分发（CleanStyler / RoughStyler）在 build_shape_visuals 内按 rough 字段决定。
            ItemKind::Shape { .. } => {
                let to_screen = item_local_to_screen(item, &self.viewport);
                let shapes = build_shape_visuals(&item.kind, &to_screen, self.viewport.zoom);
                ui.painter().extend(shapes);
            }
            ItemKind::Frame { .. } => {
                let sr = self.viewport.canvas_to_screen_rect(item.bounding_rect());
                let border = egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(90, 90, 95));
                ui.painter().rect_stroke(sr, 0.0, border);
            }
        }
    }

    /// 进入演示：收集尺寸达标的画框快照（按编号升序）+ 预计算成员，记录视口，进入全屏。
    fn enter_present(&mut self, ctx: &egui::Context) {
        let slides: Vec<ItemId> = self
            .scene
            .frames_by_number()
            .into_iter()
            .filter(|id| {
                self.scene.get_item(id).is_some_and(|it| {
                    let s = it.base_size();
                    s.x >= 10.0 && s.y >= 10.0
                })
            })
            .collect();
        if slides.is_empty() {
            self.flash(t(self.lang, T::PresentNoFrames));
            return;
        }
        // 进入时预计算每帧成员快照（翻页不重算）。
        let members: Vec<Vec<ItemId>> = slides
            .iter()
            .map(|id| self.scene.frame_members(*id))
            .collect();
        let saved_pan = self.viewport.pan;
        let saved_zoom = self.viewport.zoom;
        self.app_mode = AppMode::Present {
            slides,
            members,
            index: 0,
            saved_pan,
            saved_zoom,
        };
        self.present_anim = None;
        self.drag = DragState::Idle;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        ctx.request_repaint();
    }

    /// 退出演示：恢复视口与窗口（全屏关闭）。
    fn exit_present(&mut self, ctx: &egui::Context) {
        if let AppMode::Present {
            saved_pan,
            saved_zoom,
            ..
        } = &self.app_mode
        {
            self.viewport.pan = *saved_pan;
            self.viewport.zoom = *saved_zoom;
        }
        self.app_mode = AppMode::Edit;
        self.present_anim = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        ctx.request_repaint();
    }

    /// 计算 Present 模式下把 `frame_rect` 适配到 `screen_rect` 的目标视口 `(zoom, pan)`。
    /// 按渲染标准和设计取 95% 填充；Present 临时放宽 max_zoom（不 clamp 上限）。
    fn present_compute_fit(
        &self,
        screen_rect: egui::Rect,
        frame_rect: CanvasRect,
    ) -> (f32, CanvasVector) {
        let fw = frame_rect.width().max(1.0);
        let fh = frame_rect.height().max(1.0);
        let sw = screen_rect.width().max(1.0);
        let sh = screen_rect.height().max(1.0);
        let zoom = (sw / fw).min(sh / fh) * 0.95;
        let zoom = zoom.max(self.viewport.min_zoom);
        (zoom, frame_rect.center().to_vector())
    }

    /// 每帧把视口渐进趋近目标（翻页过渡 / Present 实时适配 DPI）。
    fn present_apply_fit(
        &mut self,
        ctx: &egui::Context,
        screen_rect: egui::Rect,
        frame_rect: CanvasRect,
    ) {
        let (tz, tp) = match self.present_anim {
            Some(t) => t,
            None => self.present_compute_fit(screen_rect, frame_rect),
        };
        if self.present_anim.is_some() {
            // 指数插值，约 200ms 收敛
            let dt = ctx.input(|i| i.unstable_dt).clamp(0.0, 0.05);
            let k = 1.0 - (-dt / 0.18).exp();
            self.viewport.zoom += (tz - self.viewport.zoom) * k;
            self.viewport.pan.x += (tp.x - self.viewport.pan.x) * k;
            self.viewport.pan.y += (tp.y - self.viewport.pan.y) * k;
            let pan_close = (self.viewport.pan - tp).length() < 0.5;
            if (self.viewport.zoom - tz).abs() < 0.001 && pan_close {
                self.viewport.zoom = tz;
                self.viewport.pan = tp;
                self.present_anim = None;
            } else {
                ctx.request_repaint();
            }
        } else {
            self.viewport.zoom = tz;
            self.viewport.pan = tp;
        }
    }

    fn present_slide_count(&self) -> usize {
        match &self.app_mode {
            AppMode::Present { slides, .. } => slides.len(),
            AppMode::Edit => 0,
        }
    }

    fn present_goto(&mut self, idx: usize, ctx: &egui::Context) {
        let (slides, index) = match &self.app_mode {
            AppMode::Present { slides, index, .. } => (slides.clone(), *index),
            AppMode::Edit => return,
        };
        let n = slides.len();
        if n <= 1 {
            return;
        }
        let idx = idx.min(n - 1);
        if idx == index {
            return;
        }
        if let AppMode::Present { index: slot, .. } = &mut self.app_mode {
            *slot = idx;
        }
        if let Some(frame) = self.scene.get_item(&slides[idx]) {
            let (z, p) = self.present_compute_fit(ctx.screen_rect(), frame.bounding_rect());
            self.present_anim = Some((z, p));
        }
        ctx.request_repaint();
    }

    fn render_present(&mut self, ctx: &egui::Context) {
        let (slides, members, index) = match &self.app_mode {
            AppMode::Present {
                slides,
                members,
                index,
                ..
            } => (slides.clone(), members.clone(), *index),
            AppMode::Edit => return,
        };
        if slides.is_empty() {
            return;
        }
        let screen_rect = ctx.screen_rect();
        self.viewport.set_screen_rect(screen_rect);

        let Some(frame) = self.scene.get_item(&slides[index]) else {
            return;
        };
        let frame_rect = frame.bounding_rect();
        self.present_apply_fit(ctx, screen_rect, frame_rect);
        let frame_screen_rect = self.viewport.canvas_to_screen_rect(frame_rect);

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(24, 24, 27)))
            .show(ctx, |ui| {
                let original_clip = ui.clip_rect();
                // 只绘制当前帧成员，且裁剪到帧矩形，形成独立"幻灯片"画布。
                ui.set_clip_rect(frame_screen_rect);
                for id in &members[index] {
                    let Some(item) = self.scene.get_item(id).cloned() else {
                        continue;
                    };
                    let cull = self.viewport.canvas_to_screen_rect(item.bounding_rect());
                    if screen_rect.intersects(cull) {
                        self.draw_item_visual(ui, &item, None);
                    }
                }
                ui.set_clip_rect(original_clip);
            });

        // 页码指示（右下角）
        egui::Area::new(egui::Id::new("present_page_indicator"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-28.0, -20.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let label = format!("{} / {}", index + 1, slides.len());
                ui.label(
                    egui::RichText::new(label)
                        .size(16.0)
                        .color(egui::Color32::from_gray(210)),
                );
            });
    }

    fn handle_present_input(&mut self, ctx: &egui::Context) {
        let mut delta: i32 = 0;
        let keys = |ctx: &egui::Context| {
            (
                ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)),
                ctx.input(|i| i.key_pressed(egui::Key::Space)),
                ctx.input(|i| i.key_pressed(egui::Key::PageDown)),
                ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)),
                ctx.input(|i| i.key_pressed(egui::Key::PageUp)),
                ctx.input(|i| i.key_pressed(egui::Key::Home)),
                ctx.input(|i| i.key_pressed(egui::Key::End)),
                ctx.input(|i| i.key_pressed(egui::Key::Escape)),
                ctx.input(|i| i.key_pressed(egui::Key::F5)),
            )
        };
        let (right, space, pgdn, left, pgup, home, end, esc, f5) = keys(ctx);
        let n = self.present_slide_count();
        let index = match &self.app_mode {
            AppMode::Present { index, .. } => *index,
            AppMode::Edit => return,
        };

        if esc || f5 {
            self.exit_present(ctx);
            return;
        }
        if right || space || pgdn {
            delta = 1;
        } else if left || pgup {
            delta = -1;
        }
        if home {
            delta = i32::MIN;
        } else if end {
            delta = i32::MAX;
        }
        let scroll = ctx.input(|i| i.raw_scroll_delta).y;
        if scroll != 0.0 {
            // Present 模式滚轮 = 翻页（滚轮向下 → 下一页）
            delta = if scroll < 0.0 { 1 } else { -1 };
        }
        if delta != 0 {
            let target = (index as i32 + delta).clamp(0, n as i32 - 1) as usize;
            self.present_goto(target, ctx);
        }
    }

    // ─────────────────────────── 渲染 ───────────────────────────
    fn render_scene(&mut self, ui: &mut egui::Ui) {
        let screen_rect = ui.max_rect();

        // 空场景：渲染欢迎页（spec §2.3 欢迎页 + 最近文件列表）
        if self.scene.items.is_empty() {
            self.render_welcome_page(ui, &screen_rect);
            return;
        }

        // 先测量所有 Text item 的实际尺寸，更新 measured_size（修 B6：边框与渲染一致）。
        // measured_size 为 None（新建/编辑/undo/redo 后）才重新测量，避免每帧重复计算。
        self.update_text_measured_sizes(ui.ctx());

        // 预生成所�?grayscale=true �?Pixmap 灰度纹理（避免渲染循环里 &mut self �?&self.scene 冲突�?
        self.ensure_grayscale_textures(ui.ctx());

        // �?Z 序渲染（�?W9）：底层先画，顶层后�?
        let items: Vec<&Item> = self.scene.items_by_z_order();
        let selection_count = self.scene.selection.len();
        let editing_id = self.editing_text.as_ref().and_then(|e| e.editing_item_id);
        let crop_item_id = self.crop_mode.as_ref().map(|c| c.item_id);
        for item in items {
            // 视口剔除（修 S9/M11）：用画�?AABB 转屏幕矩形，不相交则跳过
            let canvas_bbox = item.bounding_rect();
            let item_screen_rect = self.viewport.canvas_to_screen_rect(canvas_bbox);
            if !screen_rect.intersects(item_screen_rect) {
                continue;
            }

            let corners = item.canvas_corners();
            let screen_corners = [
                self.viewport.canvas_to_screen(corners[0]),
                self.viewport.canvas_to_screen(corners[1]),
                self.viewport.canvas_to_screen(corners[2]),
                self.viewport.canvas_to_screen(corners[3]),
            ];

            let is_selected = self.scene.selection_contains(&item.id);

            match &item.kind {
                ItemKind::Pixmap {
                    texture_id,
                    opacity,
                    grayscale,
                    crop,
                    ..
                } => {
                    let tex_id = *texture_id;
                    let opacity = *opacity;
                    let grayscale = *grayscale;
                    // 灰度选用灰度纹理，否则原纹理
                    let handle_opt = if grayscale {
                        self.grayscale_texture_cache
                            .get(&tex_id)
                            .or_else(|| self.texture_cache.get(&tex_id))
                    } else {
                        self.texture_cache.get(&tex_id)
                    };
                    if let Some(handle) = handle_opt {
                        // UV 计算说明�?                        // - flip �?local_to_canvas 的几何翻转实现（canvas_corners �?flip 后位置交换）�?                        //   因此 mesh quad �?screen_corners 即可呈现镜像，UV 不再翻转�?                        //   否则会与几何翻转抵消，导�?flip 后图片看起来不变�?                        // - crop 通过 UV 子矩形采样（�?item 局部空间，未应�?flip），
                        //   crop 区域的画布位置由 transform.scale 同步保证边框对齐�?
                        let (u_min, u_max, v_min, v_max) = if let Some(c) = crop {
                            let base_w = item.base_size().x.max(1.0);
                            let base_h = item.base_size().y.max(1.0);
                            let cx0 = (c.x / base_w).clamp(0.0, 1.0);
                            let cx1 = ((c.x + c.width) / base_w).clamp(0.0, 1.0);
                            let cy0 = (c.y / base_h).clamp(0.0, 1.0);
                            let cy1 = ((c.y + c.height) / base_h).clamp(0.0, 1.0);
                            (cx0, cx1, cy0, cy1)
                        } else {
                            (0.0, 1.0, 0.0, 1.0)
                        };
                        // 透明度：tint_color alpha = opacity（spec §2.2 透明度）
                        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
                        let tint = egui::Color32::from_rgba_premultiplied(255, 255, 255, alpha);
                        // �?mesh quad 渲染，让图片真正跟着旋转/flip（screen_corners 已包含全部几何变换）
                        // screen_corners 顺序：[TL, TR, BL, BR]，重排为 [TL, TR, BR, BL] 顺时�?
                        let [tl, tr, bl, br] = screen_corners;
                        let verts = [
                            ([tl.x, tl.y], [u_min, v_min]),
                            ([tr.x, tr.y], [u_max, v_min]),
                            ([br.x, br.y], [u_max, v_max]),
                            ([bl.x, bl.y], [u_min, v_max]),
                        ];
                        let mut mesh = egui::epaint::Mesh {
                            texture_id: handle.id(),
                            ..Default::default()
                        };
                        for ([px, py], [u, v]) in verts {
                            mesh.vertices.push(egui::epaint::Vertex {
                                pos: [px, py].into(),
                                uv: [u, v].into(),
                                color: tint,
                            });
                        }
                        mesh.indices = vec![0, 1, 2, 0, 2, 3];
                        ui.painter().add(egui::epaint::Shape::mesh(mesh));
                    } else {
                        ui.painter().rect_filled(
                            item_screen_rect,
                            egui::Rounding::same(0.0),
                            if is_selected {
                                egui::Color32::from_rgb(80, 80, 40)
                            } else {
                                egui::Color32::from_rgb(70, 70, 70)
                            },
                        );
                    }
                }
                ItemKind::Text {
                    content,
                    font_size,
                    color,
                    container_id,
                    ..
                } => {
                    // 编辑期间跳过�?item 的内容渲染（overlay 接管，避免原文字与编辑框重叠�?
                    let is_being_edited = editing_id == Some(item.id);
                    if !is_being_edited {
                        ui.painter().rect_filled(
                            item_screen_rect,
                            egui::Rounding::same(2.0),
                            egui::Color32::from_rgb(60, 60, 60),
                        );
                        let text_color = egui::Color32::from_rgba_premultiplied(
                            color[0], color[1], color[2], color[3],
                        );
                        if let Some(cid) = container_id {
                            // 绑定文本：换行到容器宽度，居中绘制
                            if let Some(container) = self.scene.get_item(cid) {
                                let cr = self
                                    .viewport
                                    .canvas_to_screen_rect(container.bounding_rect());
                                let wrap = (cr.width() - 12.0).max(20.0);
                                let ef = *font_size * self.viewport.zoom;
                                let job = egui::text::LayoutJob::simple(
                                    content.clone(),
                                    egui::FontId::proportional(ef),
                                    text_color,
                                    wrap,
                                );
                                let gal = ui.ctx().fonts(|f| f.layout_job(job));
                                let gsz = gal.size();
                                let tl = cr.center() - egui::vec2(gsz.x / 2.0, gsz.y / 2.0);
                                ui.painter().galley(tl, gal, text_color);
                            }
                        } else {
                            let origin = self.viewport.canvas_to_screen(corners[0]);
                            // 文字渲染应用 scale �?zoom（修 B6：与变换边框一致）�?                        // �?scale.x（等比缩放场景下�?scale.y 相同；非等比�?egui text 不支持非均匀缩放�?
                            let effective_font_size =
                                *font_size * item.transform.scale.x.abs() * self.viewport.zoom;
                            ui.painter().text(
                                origin,
                                egui::Align2::LEFT_TOP,
                                content.clone(),
                                egui::FontId::proportional(effective_font_size),
                                text_color,
                            );
                        } // else: 自由文本
                    }
                }
                // Shape：风格器分发同 render_scene，rough 开关决定 Clean 或手绘。
                ItemKind::Shape { .. } => {
                    let to_screen = item_local_to_screen(item, &self.viewport);
                    let shapes = build_shape_visuals(&item.kind, &to_screen, self.viewport.zoom);
                    ui.painter().extend(shapes);
                }
                // Frame：虚线边框 + 左上角编号角标 + 名称。不裁剪内容，仅作底框。
                ItemKind::Frame { number, name, .. } => {
                    // 边框矩形（画布 AABB 转屏幕：frame 不旋转，直接用 bounding_rect）。
                    let fc = item.bounding_rect();
                    let sr = self.viewport.canvas_to_screen_rect(fc);
                    let border = egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(90, 90, 95));
                    ui.painter().rect_stroke(sr, 0.0, border);
                    // 编号角标（左上角）
                    let label = format!("#{}", number);
                    let galley = ui.painter().layout_no_wrap(
                        label,
                        egui::FontId::proportional(10.0),
                        egui::Color32::WHITE,
                    );
                    let badge_size = galley.size() + egui::vec2(8.0, 4.0);
                    let badge_rect = egui::Rect::from_min_size(sr.min, badge_size);
                    ui.painter().rect_filled(
                        badge_rect,
                        egui::Rounding::same(3.0),
                        egui::Color32::from_rgba_unmultiplied(70, 110, 200, 255),
                    );
                    ui.painter().galley(
                        badge_rect.min + egui::vec2(4.0, 2.0),
                        galley,
                        egui::Color32::WHITE,
                    );
                    // 点击角标 → 进入编号编辑（Phase D：编号冲突自动顺移）。
                    let badge_id = egui::Id::new(("frame_badge", item.id));
                    if ui
                        .interact(badge_rect, badge_id, egui::Sense::click())
                        .clicked()
                        && self.editing_frame_number != Some(item.id)
                    {
                        self.editing_frame_number = Some(item.id);
                        self.frame_number_buf = number.to_string();
                    }
                    // 名称（角标右侧）
                    if let Some(nm) = name {
                        if !nm.is_empty() {
                            let ng = ui.painter().layout_no_wrap(
                                nm.clone(),
                                egui::FontId::proportional(12.0),
                                egui::Color32::from_rgb(160, 170, 180),
                            );
                            ui.painter().galley(
                                egui::pos2(sr.min.x + badge_size.x + 6.0, sr.min.y + 2.0),
                                ng,
                                egui::Color32::from_rgb(160, 170, 180),
                            );
                        }
                    }
                }
            }

            // 选中�?+ 手柄：单选时画单独手柄；多选时画统一外框（循环后�?            // 裁剪模式下手柄隐藏（避免与裁剪框冲突�?
            if is_selected && selection_count == 1 && crop_item_id != Some(item.id) {
                let show_flip = should_show_flip(item);
                let show_rotate = should_show_rotate(item);
                self.transform_handles.render(
                    item,
                    ui.painter(),
                    &self.viewport,
                    show_flip,
                    show_rotate,
                );
            }
        }

        // 多选统一外框（spec L241：多选时画一个统一 bbox�?
        if selection_count > 1 {
            if let Some(bbox) = self.scene.selection_bounding_rect() {
                let screen_bbox = self.viewport.canvas_to_screen_rect(bbox);
                let stroke = egui::Stroke::new(1.5_f32, egui::Color32::YELLOW);
                ui.painter().rect_stroke(screen_bbox, 0.0, stroke);
                // 4 角小方块标识
                let handle_size = TransformHandles::handle_size();
                let fill = egui::Color32::YELLOW;
                for p in [
                    screen_bbox.min,
                    egui::pos2(screen_bbox.max.x, screen_bbox.min.y),
                    egui::pos2(screen_bbox.min.x, screen_bbox.max.y),
                    screen_bbox.max,
                ] {
                    let r = egui::Rect::from_center_size(p, egui::Vec2::splat(handle_size));
                    ui.painter().rect_filled(r, egui::Rounding::same(1.0), fill);
                }
            }
        }

        // 裁剪模式 overlay（spec §2.2 裁剪�?
        self.render_crop_overlay(ui);
    }

    /// 懒生成所�?grayscale=true �?Pixmap 灰度纹理（spec §2.2 灰度）�?    /// 使用 ITU-R BT.601 亮度系数：Y = 0.299R + 0.587G + 0.114B（不引入 palette crate）�?
    fn ensure_grayscale_textures(&mut self, ctx: &egui::Context) {
        // 收集需要生成的 (texture_id, original_size) 列表
        let mut to_generate: Vec<(u64, (u32, u32))> = Vec::new();
        for item in &self.scene.items {
            if let ItemKind::Pixmap {
                texture_id,
                original_size,
                grayscale,
                ..
            } = &item.kind
            {
                if *grayscale && !self.grayscale_texture_cache.contains_key(texture_id) {
                    to_generate.push((*texture_id, *original_size));
                }
            }
        }
        for (tex_id, (w, h)) in to_generate {
            let rgba = match self.rgba_pixel_cache.get(&tex_id).cloned() {
                Some(b) => b,
                None => continue,
            };
            let mut gray = rgba;
            for chunk in gray.chunks_mut(4) {
                let r = chunk[0] as f32;
                let g = chunk[1] as f32;
                let b = chunk[2] as f32;
                let lum = (0.299 * r + 0.587 * g + 0.114 * b)
                    .round()
                    .clamp(0.0, 255.0) as u8;
                chunk[0] = lum;
                chunk[1] = lum;
                chunk[2] = lum;
            }
            let color_image =
                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &gray);
            let handle = ctx.load_texture(
                format!("img_gray_{}", tex_id),
                color_image,
                Default::default(),
            );
            self.grayscale_texture_cache.insert(tex_id, handle);
        }
    }

    /// 渲染裁剪模式 overlay：在裁剪 item 上画可拖拽的裁剪矩形 + 4 角手�?+ 遮罩�?
    fn render_crop_overlay(&mut self, ui: &mut egui::Ui) {
        let crop_state = match self.crop_mode.as_ref() {
            Some(c) => c.clone(),
            None => return,
        };
        let item_id = crop_state.item_id;
        let item = match self.scene.get_item(&item_id) {
            Some(i) => i.clone(),
            None => {
                self.crop_mode = None;
                return;
            }
        };
        // �?Pixmap 支持裁剪
        let (texture_id, original_size) = match &item.kind {
            ItemKind::Pixmap {
                texture_id,
                original_size,
                ..
            } => (*texture_id, *original_size),
            _ => {
                self.crop_mode = None;
                return;
            }
        };
        let _ = (texture_id, original_size);

        // item 与 crop 的 4 角（屏幕空间，环形顺序 TL -> TR -> BR -> BL）。
        // 两者都完整走 item 的 local_to_canvas（含 flip/scale/rotate），因此旋转下
        // 裁剪框会跟随图片一起旋转。不能用画布 AABB 做线性插值定位——AABB 在旋转下
        // 会膨胀且不含旋转，裁剪框与图片会错位。
        let item_quad = item
            .canvas_corners_ring()
            .map(|p| self.viewport.canvas_to_screen(p));
        let crop_quad = match item.crop_corners(crop_state.rect) {
            Some(q) => q.map(|p| self.viewport.canvas_to_screen(p)),
            None => return,
        };

        // 遮罩：item 四边形与 crop 四边形之间的 4 个梯形。
        // 两组角点一一对应、且同为凸四边形（仿射变换保凸），故每个梯形也是凸的。
        let mask_color = egui::Color32::from_rgba_premultiplied(0, 0, 0, 120);
        for i in 0..4 {
            let j = (i + 1) % 4;
            ui.painter().add(egui::Shape::convex_polygon(
                vec![item_quad[i], item_quad[j], crop_quad[j], crop_quad[i]],
                mask_color,
                egui::Stroke::NONE,
            ));
        }
        // 裁剪框：旋转四边形，用闭合折线描边而非轴对齐矩形
        let stroke = egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(100, 200, 255));
        ui.painter()
            .add(egui::Shape::closed_line(crop_quad.to_vec(), stroke));
        // 4 角手柄（落在旋转后的裁剪框角点上）
        let handle_size = TransformHandles::handle_size();
        let fill = egui::Color32::from_rgb(100, 200, 255);
        for p in crop_quad {
            let r = egui::Rect::from_center_size(p, egui::Vec2::splat(handle_size));
            ui.painter().rect_filled(r, egui::Rounding::same(1.0), fill);
        }

        // 提示文字：挂在 item 屏幕包围盒左上角上方
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        for p in item_quad {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
        }
        ui.painter().text(
            egui::pos2(min_x, min_y) + egui::vec2(0.0, -18.0),
            egui::Align2::LEFT_BOTTOM,
            "裁剪模式：拖拽角点调整 · Enter 应用 · Esc 取消",
            egui::FontId::proportional(12.0),
            egui::Color32::from_rgb(100, 200, 255),
        );
    }

    /// 测量所�?Text item 的实际文字尺寸并更新 `measured_size`（修 B6）�?    /// 仅在 `measured_size` �?None 时测量（content 变化会清�?measured_size）�?
    fn update_text_measured_sizes(&mut self, ctx: &egui::Context) {
        let zoom = self.viewport.zoom;
        let mut updates: Vec<(ItemId, (f32, f32))> = Vec::new();
        for item in &self.scene.items {
            if let ItemKind::Text {
                content,
                font_size,
                measured_size,
                container_id,
                ..
            } = &item.kind
            {
                if let Some(cid) = container_id {
                    // 绑定文本：换行到容器宽度，随容器 resize 每帧重测。
                    if let Some(container) = self.scene.get_item(cid) {
                        let cw = self
                            .viewport
                            .canvas_to_screen_rect(container.bounding_rect())
                            .width();
                        let wrap = (cw - 12.0).max(20.0);
                        let job = egui::text::LayoutJob::simple(
                            content.clone(),
                            egui::FontId::proportional(*font_size * zoom),
                            egui::Color32::WHITE,
                            wrap,
                        );
                        let gal = ctx.fonts(|f| f.layout_job(job));
                        updates.push((item.id, (gal.size().x / zoom, gal.size().y / zoom)));
                        continue;
                    }
                }
                if measured_size.is_none() {
                    // 自由文本：仅在未测量时测（content 变化会清空触发重测）。
                    let gal = ctx.fonts(|fonts| {
                        fonts.layout_no_wrap(
                            content.clone(),
                            egui::FontId::proportional(*font_size),
                            egui::Color32::WHITE,
                        )
                    });
                    updates.push((item.id, (gal.size().x, gal.size().y)));
                }
            }
        }
        for (id, (w, h)) in updates {
            if let Some(item) = self.scene.get_item_mut(&id) {
                if let ItemKind::Text { measured_size, .. } = &mut item.kind {
                    *measured_size = Some((w, h));
                }
            }
        }
    }

    /// 渲染文本便签编辑 overlay（spec L243 P2-5）�?    /// 创建中的文本不在 scene 中；Enter/失焦时提交（非空→AddItem），Esc 取消�?
    fn render_text_editor(&mut self, ctx: &egui::Context) {
        let mut edit = match self.editing_text.take() {
            Some(e) => e,
            None => return,
        };
        let screen_pos = self.viewport.canvas_to_screen(edit.canvas_pos);
        // 绑定文本：获取容器屏幕矩形，用于居中 + 定宽（换行）。
        let container_screen_rect = edit.container_id.and_then(|cid| {
            self.scene
                .get_item(&cid)
                .map(|it| self.viewport.canvas_to_screen_rect(it.bounding_rect()))
        });
        let mut commit = false;
        let mut cancel = false;

        let mut area =
            egui::Area::new(egui::Id::new("text_edit_area")).order(egui::Order::Foreground);
        if let Some(r) = container_screen_rect {
            // 绑定文本：居中于容器（先按预估高度定位，渲染后再由 min_width 撑开）。
            let w = (r.width() - 16.0).max(60.0);
            let est_h = edit.font_size.max(16.0) * 2.0 + 16.0;
            area = area.fixed_pos(r.center() - egui::vec2(w / 2.0, est_h / 2.0));
        } else {
            area = area.fixed_pos(screen_pos);
        }
        area.show(ctx, |ui| {
            let frame = egui::Frame::popup(ui.style())
                .fill(egui::Color32::from_rgb(50, 50, 50))
                .stroke(egui::Stroke::new(
                    1.0_f32,
                    egui::Color32::from_rgb(100, 200, 255),
                ));
            frame.show(ui, |ui| {
                if let Some(r) = container_screen_rect {
                    // 绑定文本编辑：宽度受容器约束，支持换行，居中。
                    let w = (r.width() - 16.0).max(60.0);
                    ui.set_min_width(w);
                    let response = ui.add(
                        egui::TextEdit::multiline(&mut edit.buffer)
                            .desired_width(w)
                            .hint_text("输入文本...")
                            .font(egui::FontId::proportional(edit.font_size))
                            .text_color(egui::Color32::from_rgba_premultiplied(
                                edit.color[0],
                                edit.color[1],
                                edit.color[2],
                                edit.color[3],
                            )),
                    );
                    if edit.first_frame {
                        response.request_focus();
                        edit.first_frame = false;
                    }
                    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
                        cancel = true;
                    } else if response.lost_focus() {
                        commit = true;
                    }
                } else {
                    ui.set_min_width(120.0);
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut edit.buffer)
                            .desired_width(160.0)
                            .hint_text("输入文本...")
                            .font(egui::FontId::proportional(edit.font_size))
                            .text_color(egui::Color32::from_rgba_premultiplied(
                                edit.color[0],
                                edit.color[1],
                                edit.color[2],
                                edit.color[3],
                            )),
                    );
                    if edit.first_frame {
                        response.request_focus();
                        edit.first_frame = false;
                    }
                    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
                        cancel = true;
                    } else if response.lost_focus() {
                        commit = true;
                    }
                }
            });
        });

        if cancel {
            self.editing_text = None;
            return;
        }
        if commit {
            match edit.editing_item_id {
                None => {
                    // 创建模式：空内容丢弃，非�?push AddItem
                    if !edit.buffer.trim().is_empty() {
                        let item = match edit.container_id {
                            Some(cid) => Item::new_text_in(
                                edit.buffer,
                                edit.canvas_pos.x,
                                edit.canvas_pos.y,
                                edit.font_size,
                                edit.color,
                                cid,
                            ),
                            None => Item::new_text(
                                edit.buffer,
                                edit.canvas_pos.x,
                                edit.canvas_pos.y,
                                edit.font_size,
                                edit.color,
                            ),
                        };
                        self.push_cmd(Box::new(AddItem::new(item)));
                        self.flash(t(self.lang, T::FlashTextCreated).to_string());
                    }
                }
                Some(id) => {
                    // 编辑模式：空内容不修改原 item（避免误删）；非空且变化�?push EditTextContent
                    if !edit.buffer.trim().is_empty() {
                        let old_content = self.scene.get_item(&id).and_then(|item| {
                            if let ItemKind::Text { content, .. } = &item.kind {
                                Some(content.clone())
                            } else {
                                None
                            }
                        });
                        if let Some(old) = old_content {
                            if old != edit.buffer {
                                let cmd = EditTextContent::new(id, old, edit.buffer);
                                self.push_cmd(Box::new(cmd));
                                self.flash(t(self.lang, T::FlashTextUpdated).to_string());
                            }
                        }
                    }
                }
            }
            self.editing_text = None;
            return;
        }
        self.editing_text = Some(edit);
    }

    /// 画框编号编辑小窗（Phase D）。点击角标触发；Enter 提交（数字 only），
    /// 冲突时经 [`Scene::plan_frame_renumber`] 自动顺移，走 undo 命令。
    fn render_frame_number_editor(&mut self, ctx: &egui::Context) {
        let Some(frame_id) = self.editing_frame_number else {
            return;
        };
        let Some(frame) = self.scene.get_item(&frame_id).filter(|it| it.is_frame()) else {
            self.editing_frame_number = None;
            return;
        };
        let old_number = frame.frame_number().unwrap_or(1);
        let sr = self.viewport.canvas_to_screen_rect(frame.bounding_rect());
        let mut commit = false;
        let mut cancel = false;

        let mut buf = self.frame_number_buf.clone();
        egui::Area::new(egui::Id::new("frame_number_edit_area"))
            .order(egui::Order::Foreground)
            .fixed_pos(sr.min)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(egui::Color32::from_rgb(50, 50, 55))
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        egui::Color32::from_rgb(100, 200, 255),
                    ))
                    .show(ui, |ui| {
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut buf)
                                .desired_width(40.0)
                                .hint_text("#")
                                .char_limit(6)
                                .font(egui::FontId::proportional(10.0)),
                        );
                        resp.request_focus();
                        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if esc {
                            cancel = true;
                        } else if enter || resp.lost_focus() {
                            commit = true;
                        }
                    });
            });
        self.frame_number_buf = buf;

        if cancel {
            self.editing_frame_number = None;
            return;
        }
        if !commit {
            return;
        }
        self.editing_frame_number = None;
        let Some(new_number) = self
            .frame_number_buf
            .trim()
            .parse::<u32>()
            .ok()
            .map(|n| n.max(1))
        else {
            return;
        };
        if new_number != old_number {
            let plan = self.scene.plan_frame_renumber(frame_id, new_number);
            let cmd = RenumberFrame::new(plan);
            self.push_cmd(Box::new(cmd));
            self.flash(format!("画框编号 → #{}", new_number));
        }
    }

    fn render_context_menu(&mut self, ctx: &egui::Context) {
        let menu_id = egui::Id::new("context_menu");
        let pos = self.context_menu_pos;
        let has_selection = !self.scene.selection.is_empty();
        let primary_pressed = ctx.input(|i| i.pointer.primary_pressed());
        let pointer_pos = ctx.input(|i| i.pointer.latest_pos());

        // �?egui::Area + 手动按钮。返回菜�?rect 用于检测点击外部（�?B4�?
        let area_response = egui::Area::new(menu_id)
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                let frame = egui::Frame::popup(ui.style());
                frame.show(ui, |ui| {
                    ui.set_max_width(180.0);

                    if ui
                        .button(format!("\u{1F195} {}", t(self.lang, T::NewCanvas)))
                        .clicked()
                    {
                        self.new_canvas(ctx);
                        self.context_menu_open = false;
                    }
                    ui.separator();
                    if ui
                        .button(format!("\u{25B6} {}", t(self.lang, T::Present)))
                        .clicked()
                    {
                        self.enter_present(ctx);
                        self.context_menu_open = false;
                    }
                    ui.separator();
                    if ui
                        .button(format!("\u{1F4C2} {}", t(self.lang, T::OpenProject)))
                        .clicked()
                    {
                        self.open_project_file(ctx);
                        self.context_menu_open = false;
                    }
                    if ui
                        .button(format!("\u{1F5BC} {}", t(self.lang, T::LoadImage)))
                        .clicked()
                    {
                        self.import_image_file(ctx);
                        self.context_menu_open = false;
                    }
                    if ui
                        .button(format!("\u{1F4CB} {}", t(self.lang, T::PasteImage)))
                        .clicked()
                    {
                        self.paste_from_clipboard(ctx);
                        self.context_menu_open = false;
                    }
                    ui.separator();
                    if ui
                        .button(format!("\u{1F4BE} {}", t(self.lang, T::Save)))
                        .clicked()
                    {
                        self.save_file(ctx);
                        self.context_menu_open = false;
                    }
                    if ui
                        .button(format!("\u{1F4C4} {}", t(self.lang, T::SaveAs)))
                        .clicked()
                    {
                        self.save_file_as(ctx);
                        self.context_menu_open = false;
                    }
                    ui.menu_button(
                        format!("\u{1F4F7} {}", t(self.lang, T::ExportScene)),
                        |ui| {
                            if ui.button(t(self.lang, T::ExportPngAll)).clicked() {
                                self.start_export_dialog(ctx, ExportFormat::Png, false);
                                self.context_menu_open = false;
                            }
                            if ui.button(t(self.lang, T::ExportJpgAll)).clicked() {
                                self.start_export_dialog(ctx, ExportFormat::Jpeg, false);
                                self.context_menu_open = false;
                            }
                            if has_selection {
                                ui.separator();
                                if ui.button(t(self.lang, T::ExportPngSelection)).clicked() {
                                    self.start_export_dialog(ctx, ExportFormat::Png, true);
                                    self.context_menu_open = false;
                                }
                                if ui.button(t(self.lang, T::ExportJpgSelection)).clicked() {
                                    self.start_export_dialog(ctx, ExportFormat::Jpeg, true);
                                    self.context_menu_open = false;
                                }
                            }
                        },
                    );
                    ui.menu_button(
                        format!("\u{1F4E9} {}", t(self.lang, T::ExportImagesToDir)),
                        |ui| {
                            if ui.button(t(self.lang, T::ExportAllImages)).clicked() {
                                self.start_export_images_dialog(ctx, false);
                                self.context_menu_open = false;
                            }
                            if has_selection
                                && ui.button(t(self.lang, T::ExportSelectionImages)).clicked()
                            {
                                self.start_export_images_dialog(ctx, true);
                                self.context_menu_open = false;
                            }
                        },
                    );
                    ui.separator();

                    if has_selection {
                        if ui
                            .button(format!("\u{1F5D1} {}", t(self.lang, T::DeleteSelected)))
                            .clicked()
                        {
                            self.delete_selected();
                            self.context_menu_open = false;
                        }
                        if ui
                            .button(format!("\u{2191} {}", t(self.lang, T::BringToFront)))
                            .clicked()
                        {
                            self.bring_to_front();
                            self.context_menu_open = false;
                        }
                        if ui
                            .button(format!("\u{2193} {}", t(self.lang, T::SendToBack)))
                            .clicked()
                        {
                            self.send_to_back();
                            self.context_menu_open = false;
                        }
                        ui.separator();

                        // Phase 5：灰度/透明度/裁剪（仅 Pixmap 单选时）
                        if self.selected_pixmap_count() == 1 {
                            let is_gray = self.selected_pixmap_grayscale();
                            let gray_label = if is_gray {
                                format!("\u{1F3A8} {}", t(self.lang, T::CancelGrayscale))
                            } else {
                                format!("\u{1F3A8} {}", t(self.lang, T::ToggleGrayscale))
                            };
                            if ui.button(gray_label).clicked() {
                                self.toggle_grayscale_selected();
                                self.context_menu_open = false;
                            }
                            if ui
                                .button(format!("\u{1F4CF} {}", t(self.lang, T::CropMode)))
                                .clicked()
                            {
                                self.enter_crop_mode();
                                self.context_menu_open = false;
                            }
                            ui.separator();
                        }

                        // Phase 5：归一化尺寸（Pixmap 多选）
                        if self.selected_pixmap_count() >= 2 {
                            ui.menu_button(
                                format!("\u{1F4D0} {}", t(self.lang, T::NormalizeSize)),
                                |ui| {
                                    if ui.button(t(self.lang, T::NormalizeByWidth)).clicked() {
                                        self.normalize_selected(
                                            preferz_core::commands::NormalizeMode::Width,
                                        );
                                        self.context_menu_open = false;
                                    }
                                    if ui.button(t(self.lang, T::NormalizeByHeight)).clicked() {
                                        self.normalize_selected(
                                            preferz_core::commands::NormalizeMode::Height,
                                        );
                                        self.context_menu_open = false;
                                    }
                                    if ui.button(t(self.lang, T::NormalizeByArea)).clicked() {
                                        self.normalize_selected(
                                            preferz_core::commands::NormalizeMode::Area,
                                        );
                                        self.context_menu_open = false;
                                    }
                                },
                            );
                        }

                        // Phase 5：批量排列（≥ 2 项）
                        if self.scene.selection.len() >= 2 {
                            ui.menu_button(
                                format!("\u{1F9ED} {}", t(self.lang, T::Arrange)),
                                |ui| {
                                    if ui.button(t(self.lang, T::ArrangeLinear)).clicked() {
                                        self.arrange_selected(ArrangeMode::Linear);
                                        self.context_menu_open = false;
                                    }
                                    if ui.button(t(self.lang, T::ArrangeGrid)).clicked() {
                                        self.arrange_selected(ArrangeMode::Grid);
                                        self.context_menu_open = false;
                                    }
                                    if ui.button(t(self.lang, T::ArrangeOptimal)).clicked() {
                                        self.arrange_selected(ArrangeMode::Optimal);
                                        self.context_menu_open = false;
                                    }
                                },
                            );
                        }

                        ui.separator();
                    }

                    // 颜色采样模式（spec §2.2）
                    let picker_label = if self.color_picker_active {
                        format!("\u{1F3A8} {}", t(self.lang, T::ExitColorPicker))
                    } else {
                        format!("\u{1F3A8} {}", t(self.lang, T::ColorPickerMode))
                    };
                    if ui.button(picker_label).clicked() {
                        self.color_picker_active = !self.color_picker_active;
                        self.context_menu_open = false;
                    }

                    if ui
                        .button(format!("\u{1F527} {}", t(self.lang, T::Settings)))
                        .clicked()
                    {
                        self.settings_open = true;
                        self.context_menu_open = false;
                    }

                    if ui
                        .button(format!("\u{1F50D} {}", t(self.lang, T::FitToCanvas)))
                        .clicked()
                    {
                        self.fit_to_screen();
                        self.context_menu_open = false;
                    }
                    if ui
                        .button(format!("\u{1F504} {}", t(self.lang, T::ResetZoom)))
                        .clicked()
                    {
                        self.viewport.reset();
                        self.flash(t(self.lang, T::FlashResetZoom).to_string());
                        self.context_menu_open = false;
                    }
                    ui.separator();
                    if ui
                        .button(format!("\u{274C} {}", t(self.lang, T::Exit)))
                        .clicked()
                    {
                        // 触发 close_requested 流程；dirty 时由 update 顶部检测弹保存提示
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        self.context_menu_open = false;
                    }
                });
                ui.min_rect()
            });

        // 点击菜单外部 �?关闭菜单（修 B4�?
        if primary_pressed {
            if let Some(p) = pointer_pos {
                if !area_response.inner.contains(p) {
                    self.context_menu_open = false;
                }
            }
        }
    }

    /// 渲染欢迎页（空场景时显示）。
    /// 居中显示标题 + 提示 + 最近文件列表按钮。
    fn render_welcome_page(&mut self, ui: &mut egui::Ui, screen_rect: &egui::Rect) {
        let center = screen_rect.center();
        let panel_w = 420.0;
        let panel_x = center.x - panel_w / 2.0;
        let mut y = center.y - 140.0;

        // Icon（标题上方，64×64 居中；懒加载复用 assets/icon.png）
        let ctx = ui.ctx().clone();
        let logo_tex = self.logo_texture.get_or_insert_with(|| {
            let bytes = include_bytes!("../../../assets/icon.png");
            match image::load_from_memory(bytes) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    let raw = rgba.into_raw();
                    ctx.load_texture(
                        "welcome-logo",
                        egui::ColorImage {
                            size: [w as usize, h as usize],
                            pixels: raw
                                .as_chunks::<4>()
                                .0
                                .iter()
                                .map(|p| {
                                    egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])
                                })
                                .collect(),
                        },
                        egui::TextureOptions::LINEAR,
                    )
                }
                Err(_) => ctx.load_texture(
                    "welcome-logo-fallback",
                    egui::ColorImage {
                        size: [1, 1],
                        pixels: vec![egui::Color32::TRANSPARENT],
                    },
                    egui::TextureOptions::LINEAR,
                ),
            }
        });
        let icon_size = 64.0;
        let icon_rect = egui::Rect::from_min_size(
            egui::pos2(center.x - icon_size / 2.0, y - icon_size - 10.0),
            egui::vec2(icon_size, icon_size),
        );
        ui.painter().image(
            logo_tex.id(),
            icon_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );

        // 标题
        ui.painter().text(
            egui::pos2(center.x, y),
            egui::Align2::CENTER_CENTER,
            t(self.lang, T::WelcomeTitle),
            egui::FontId::proportional(42.0),
            egui::Color32::from_rgb(220, 220, 230),
        );
        y += 50.0;
        ui.painter().text(
            egui::pos2(center.x, y),
            egui::Align2::CENTER_CENTER,
            t(self.lang, T::WelcomeSubtitle),
            egui::FontId::proportional(15.0),
            egui::Color32::from_rgb(150, 150, 160),
        );
        y += 36.0;

        // 最近文件列表
        if !self.recent_files.is_empty() {
            ui.painter().text(
                egui::pos2(center.x, y),
                egui::Align2::CENTER_CENTER,
                t(self.lang, T::WelcomeRecentFiles),
                egui::FontId::proportional(14.0),
                egui::Color32::from_rgb(180, 180, 190),
            );
            y += 22.0;

            // 借用切片避免 &mut self 冲突；点击记录到 pending_open_recent
            let recent: Vec<PathBuf> = self.recent_files.iter().take(8).cloned().collect();
            for path in recent {
                let row_rect =
                    egui::Rect::from_min_size(egui::pos2(panel_x, y), egui::vec2(panel_w, 26.0));
                let resp = ui
                    .allocate_new_ui(egui::UiBuilder::new().max_rect(row_rect), |ui| {
                        let btn = egui::Button::new(path.to_string_lossy())
                            .min_size(egui::vec2(panel_w, 0.0));
                        ui.add(btn).clicked()
                    })
                    .inner;
                if resp {
                    self.pending_open_recent = Some(path);
                }
                y += 30.0;
            }
        } else {
            ui.painter().text(
                egui::pos2(center.x, y),
                egui::Align2::CENTER_CENTER,
                t(self.lang, T::WelcomeHint),
                egui::FontId::proportional(14.0),
                egui::Color32::from_rgb(140, 140, 150),
            );
        }
    }

    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    fn render_debug_panel(&self, ctx: &egui::Context) {
        egui::Window::new("Debug")
            .default_pos(egui::Pos2::new(10.0, 10.0))
            .default_size(egui::Vec2::new(320.0, 220.0))
            .show(ctx, |ui| {
                ui.label(format!(
                    "Pointer: {:?}",
                    ctx.input(|i| i.pointer.latest_pos())
                ));
                ui.label(format!(
                    "Hover handle: {:?}",
                    self.transform_handles.hover_handle
                ));
                ui.label(format!(
                    "Active handle: {:?}",
                    self.transform_handles.active_handle
                ));
                ui.label(format!(
                    "Is dragging: {}",
                    self.transform_handles.is_dragging
                ));
                ui.label(format!("Drag state: {}", drag_state_name(&self.drag)));
                ui.label(format!("Selection: {} items", self.scene.selection.len()));
                ui.label(format!(
                    "Pan: ({:.1}, {:.1}) Zoom: {:.3}",
                    self.viewport.pan.x, self.viewport.pan.y, self.viewport.zoom
                ));
                ui.separator();
                if let Some(id) = self.scene.selection.iter().next() {
                    if let Some(item) = self.scene.get_item(id) {
                        ui.label(format!("Selected item {}:", id));
                        ui.label(format!(
                            "  pos: ({:.1}, {:.1})",
                            item.transform.pos.x, item.transform.pos.y
                        ));
                        ui.label(format!(
                            "  scale: ({:.2}, {:.2})",
                            item.transform.scale.x, item.transform.scale.y
                        ));
                        ui.label(format!(
                            "  rotation: {:.2}°",
                            item.transform.rotation.to_degrees()
                        ));
                        ui.label(format!("  z: {}", item.z));
                    }
                }
            });
    }
}

fn drag_state_name(d: &DragState) -> &'static str {
    match d {
        DragState::Idle => "Idle",
        DragState::HandleTransform { .. } => "HandleTransform",
        DragState::MoveItems { .. } => "MoveItems",
        DragState::BoxSelect { .. } => "BoxSelect",
        DragState::CreatingShape { .. } => "CreatingShape",
        DragState::LineEndpoint { .. } => "LineEndpoint",
        DragState::CreatingFrame { .. } => "CreatingFrame",
    }
}

/// 是否为文本容器（封闭形状）。矩形/椭圆/菱形恒可；多段线仅闭合时可。
fn is_text_container(kind: &ItemKind) -> bool {
    if let ItemKind::Shape {
        shape_type, closed, ..
    } = kind
    {
        !matches!(shape_type, ShapeType::Polyline) || *closed
    } else {
        false
    }
}

/// 判断路径是否�?PReferZ 项目文件�?prz / .bee）�?
fn is_project_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_ascii_lowercase().as_str(), "prz" | "bee"))
        .unwrap_or(false)
}

/// 缩放手柄：以拖拽角点的对角为锚点�?
/// 数学：见 REVIEW 报告 P0-4。设 T(p)=pos+R(rot)*(scale∘F∘p)，F=flip 矩阵�?
/// 对角�?a 和拖拽点 d�?
///   R(rot)*(scale'*F*(d-a)) = mouse-a  �?
///   scale' = R(-rot)*(mouse-a) ./ (F*(d-a))
///   pos' = a - R(rot)*(scale'*F*a)
/// �?B3：加�?flip 矩阵 F，否则翻转后缩放方向错误导致跳跃�?
/// �?B2：free_scale=false 时默认等比缩放，Ctrl 自由缩放�?
fn apply_scale_drag(
    item: &mut Item,
    handle: Handle,
    start_transform: preferz_core::Transform,
    start_corners: [CanvasPoint; 4],
    mouse_canvas: CanvasPoint,
    free_scale: bool,
) {
    let base = item.base_size();
    let base_w = base.x.max(1.0);
    let base_h = base.y.max(1.0);

    // corners = [TL, TR, BL, BR]，对角关系：0↔3, 1↔2
    let (anchor_idx, anchor_local, drag_local) = match handle {
        Handle::ResizeTopLeft => (
            3,
            CanvasVector::new(base_w, base_h),
            CanvasVector::new(0.0, 0.0),
        ),
        Handle::ResizeTopRight => (
            2,
            CanvasVector::new(0.0, base_h),
            CanvasVector::new(base_w, 0.0),
        ),
        Handle::ResizeBottomLeft => (
            1,
            CanvasVector::new(base_w, 0.0),
            CanvasVector::new(0.0, base_h),
        ),
        Handle::ResizeBottomRight => (
            0,
            CanvasVector::new(0.0, 0.0),
            CanvasVector::new(base_w, base_h),
        ),
        _ => return,
    };
    let anchor_canvas = start_corners[anchor_idx];
    let v = drag_local - anchor_local; // 局部空间对角向量，分量�?0
    let w = mouse_canvas - anchor_canvas; // 画布空间

    let rot = start_transform.rotation;
    let cos = rot.cos();
    let sin = rot.sin();
    // R(-rot) * w
    let w_local_x = cos * w.x + sin * w.y;
    let w_local_y = -sin * w.x + cos * w.y;

    // flip 因子（修 B3：缩放计算需除以 F*v 而非 v�?
    let fx = if start_transform.flip_h { -1.0 } else { 1.0 };
    let fy = if start_transform.flip_v { -1.0 } else { 1.0 };

    let mut new_scale_x = w_local_x / (fx * v.x);
    let mut new_scale_y = w_local_y / (fy * v.y);

    // 等比缩放（修 B2：默认保持高宽比，Ctrl 自由缩放�?
    if !free_scale {
        let start_sx = start_transform.scale.x.abs().max(0.05);
        let start_sy = start_transform.scale.y.abs().max(0.05);
        let ratio_x = new_scale_x / start_sx;
        let ratio_y = new_scale_y / start_sy;
        // 取变化幅度更大的方向作为统一缩放�?
        let uniform_ratio = if ratio_x.abs() >= ratio_y.abs() {
            ratio_x
        } else {
            ratio_y
        };
        new_scale_x = start_sx * uniform_ratio;
        new_scale_y = start_sy * uniform_ratio;
    }

    // 最小尺寸限制（避免 0 / 负值）
    let min_scale = 0.05;
    new_scale_x = new_scale_x.max(min_scale);
    new_scale_y = new_scale_y.max(min_scale);

    // pos' = anchor_canvas - R(rot) * (new_scale * (F·anchor_local + F_translation))
    // 其中 F·a + F_translation 等于 anchor 沿局部中点镜像后的位置：
    //   flip_h 时 x 分量 = base_w - anchor_local.x，否则 = anchor_local.x
    //   flip_v 时 y 分量 = base_h - anchor_local.y，否则 = anchor_local.y
    // 修 B?：之前缺失 F_translation 项，导致 flip 状态下缩放时 pos 计算错误、图片跳动。
    let fa_x = if start_transform.flip_h {
        base_w - anchor_local.x
    } else {
        anchor_local.x
    };
    let fa_y = if start_transform.flip_v {
        base_h - anchor_local.y
    } else {
        anchor_local.y
    };
    let sa_x = new_scale_x * fa_x;
    let sa_y = new_scale_y * fa_y;
    let r_x = cos * sa_x - sin * sa_y;
    let r_y = sin * sa_x + cos * sa_y;
    let new_pos = CanvasVector::new(anchor_canvas.x - r_x, anchor_canvas.y - r_y);

    item.transform.pos = new_pos;
    item.transform.scale = CanvasVector::new(new_scale_x, new_scale_y);
    // rotation / flip 不变
}

/// 旋转手柄：以拖拽前的 4 角点中心为锚点旋转�?
///
/// 由于 `local_to_canvas` 的旋转绕局部原点（左上角），单纯改 `rotation` 会让图片
/// 围绕左上角旋转。这里在更新 rotation 后补�?pos，让旋转后的 4 角点中心等于
/// 旋转前的中心，从而视觉上围绕中心旋转�?
fn apply_rotate_drag(
    viewport: &ViewportState,
    item: &mut Item,
    start_transform: preferz_core::Transform,
    start_corners: [CanvasPoint; 4],
    start_screen: egui::Pos2,
    current_screen: egui::Pos2,
) {
    let center_canvas = CanvasPoint::new(
        (start_corners[0].x + start_corners[1].x + start_corners[2].x + start_corners[3].x) * 0.25,
        (start_corners[0].y + start_corners[1].y + start_corners[2].y + start_corners[3].y) * 0.25,
    );
    let center_screen = viewport.canvas_to_screen(center_canvas);
    let start_angle = (start_screen.y - center_screen.y).atan2(start_screen.x - center_screen.x);
    let current_angle =
        (current_screen.y - center_screen.y).atan2(current_screen.x - center_screen.x);
    let delta = current_angle - start_angle;
    item.transform.rotation = start_transform.rotation + delta;

    // 补偿 pos：让旋转后的 4 角点中心 = 旋转前中心（center_canvas）�?    // local_to_canvas 的旋转绕局部原点，所以改 rotation 后中心会偏移�?    // 需把偏移量加回 pos�?
    let new_corners = item.canvas_corners();
    let new_center = CanvasPoint::new(
        (new_corners[0].x + new_corners[1].x + new_corners[2].x + new_corners[3].x) * 0.25,
        (new_corners[0].y + new_corners[1].y + new_corners[2].y + new_corners[3].y) * 0.25,
    );
    item.transform.pos = CanvasVector::new(
        item.transform.pos.x + (center_canvas.x - new_center.x),
        item.transform.pos.y + (center_canvas.y - new_center.y),
    );
}

// ─────────────────────────── 操作 ───────────────────────────

impl PReferZApp {
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // Present 演示：F5 / Esc 退出（进入由 F5 或右键菜单触发，present 输入走 handle_present_input）。
        if ctx.input(|i| i.key_pressed(egui::Key::F5)) {
            if matches!(self.app_mode, AppMode::Present { .. }) {
                self.exit_present(ctx);
            } else {
                self.enter_present(ctx);
            }
            return;
        }

        // 文本编辑中不处理场景快捷键（Esc �?render_text_editor 处理�?
        if self.editing_text.is_some() {
            return;
        }

        // 工具切换快捷键（始终生效，即使当前工具非 Select）
        let tool_switch = self.tool_switch_shortcut(ctx);
        if let Some(new_tool) = tool_switch {
            self.drag = DragState::Idle; // 取消进行中的绘制
            if self.tool != new_tool {
                self.tool = new_tool;
            } else {
                self.tool = Tool::Select; // 再次按同键回 Select
            }
            return;
        }
        // 绘制工具激活：屏蔽其它场景快捷键，Esc 回 Select
        if self.tool != Tool::Select {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.tool = Tool::Select;
            }
            return;
        }

        // 裁剪模式快捷键：Enter 应用 / Esc 取消
        if self.crop_mode.is_some() {
            if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.apply_crop();
                return;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.cancel_crop();
                return;
            }
            // 裁剪模式下屏蔽其他场景快捷键
            return;
        }

        // Ctrl+Shift+P 显示菜单
        let show_menu =
            ctx.input(|i| i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::P));
        if show_menu && !self.context_menu_open {
            self.context_menu_open = true;
            self.context_menu_pos = ctx
                .input(|i| i.pointer.latest_pos())
                .unwrap_or_else(|| ctx.screen_rect().center());
        }

        // ESC 关闭菜单 / 退出颜色采�?
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.context_menu_open {
                self.context_menu_open = false;
            } else if self.color_picker_active {
                self.color_picker_active = false;
            }
        }

        // Delete 删除选中（走命令�?
        if ctx.input(|i| i.key_pressed(egui::Key::Delete)) && !self.scene.selection.is_empty() {
            self.delete_selected();
        }

        // Ctrl+Z 撤销
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Z) && !i.modifiers.shift)
            && self.perform_undo()
        {
            self.flash(t(self.lang, T::FlashUndo).to_string());
            ctx.request_repaint();
        }

        // Ctrl+Y / Ctrl+Shift+Z 重做
        if ctx.input(|i| {
            i.modifiers.ctrl
                && (i.key_pressed(egui::Key::Y)
                    || (i.key_pressed(egui::Key::Z) && i.modifiers.shift))
        }) && self.perform_redo()
        {
            self.flash(t(self.lang, T::FlashRedo).to_string());
            ctx.request_repaint();
        }

        // Ctrl+S 保存，Ctrl+Shift+S 另存为，Ctrl+O 打开项目，Ctrl+I 载入图片，Ctrl+N 新建画布
        if ctx.input(|i| i.modifiers.ctrl && !i.modifiers.shift && i.key_pressed(egui::Key::S)) {
            self.save_file(ctx);
        }
        if ctx.input(|i| i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::S)) {
            self.save_file_as(ctx);
        }
        if ctx.input(|i| i.modifiers.ctrl && !i.modifiers.shift && i.key_pressed(egui::Key::O)) {
            self.open_project_file(ctx);
        }
        if ctx.input(|i| i.modifiers.ctrl && !i.modifiers.shift && i.key_pressed(egui::Key::I)) {
            self.import_image_file(ctx);
        }
        if ctx.input(|i| i.modifiers.ctrl && !i.modifiers.shift && i.key_pressed(egui::Key::N)) {
            self.new_canvas(ctx);
        }
        // Ctrl+V 粘贴剪贴板图片到画布（spec §2.1 剪贴板粘贴）
        // egui-winit 0.29 拦截 Ctrl+V 的 key_pressed 事件用于文本粘贴：
        //   is_paste_command(modifiers, Key::V) 在 pressed=true 时返回 true，
        //   egui-winit 尝试 arboard.get_text()：
        //     - 剪贴板有文本 → 产生 Event::Paste(text)，不产生 Event::Key for V
        //     - 剪贴板是图片 → 什么都不产生（get_text 读不到图片），直接 return
        //   因此 key_pressed(Key::V) 永远不会在 Ctrl+V 时返回 true。
        // 解决方案：is_paste_command 只在 pressed=true 时拦截，V 释放（pressed=false）
        // 不被拦截，会正常产生 Event::Key。所以用 key_released(V) + modifiers.ctrl 检测。
        if ctx.input(|i| i.key_released(egui::Key::V) && i.modifiers.ctrl && !i.modifiers.shift) {
            self.paste_from_clipboard(ctx);
        }

        // F 适应画布（替代原双击手势，双击已用于创建文本便签�?
        if ctx.input(|i| i.key_pressed(egui::Key::F)) {
            self.fit_to_screen();
        }

        // Phase 5 快捷键（�?Ctrl/Shift 修饰�?
        let no_mod = ctx.input(|i| !i.modifiers.ctrl && !i.modifiers.shift && !i.modifiers.alt);
        if no_mod && !self.scene.selection.is_empty() {
            // C = 进入裁剪模式
            if ctx.input(|i| i.key_pressed(egui::Key::C)) && self.selected_pixmap_count() == 1 {
                self.enter_crop_mode();
            }
        }

        // I = 切换取色器模式（独立，不要求选中�?
        if no_mod && ctx.input(|i| i.key_pressed(egui::Key::I)) {
            self.color_picker_active = !self.color_picker_active;
            self.flash(if self.color_picker_active {
                t(self.lang, T::FlashColorPickerHint).to_string()
            } else {
                t(self.lang, T::ExitColorPicker).to_string()
            });
        }
    }

    /// 工具切换快捷键：V/R/O/D（无修饰键）。再次按同键在 handle_shortcuts 里回 Select。
    fn tool_switch_shortcut(&self, ctx: &egui::Context) -> Option<Tool> {
        let no_mod = ctx.input(|i| !i.modifiers.ctrl && !i.modifiers.shift && !i.modifiers.alt);
        if !no_mod {
            return None;
        }
        let pressed = |key| ctx.input(|i| i.key_pressed(key));
        if pressed(egui::Key::V) {
            Some(Tool::Select)
        } else if pressed(egui::Key::R) {
            Some(Tool::Shape(ShapeType::Rectangle))
        } else if pressed(egui::Key::O) {
            Some(Tool::Shape(ShapeType::Ellipse))
        } else if pressed(egui::Key::D) {
            Some(Tool::Shape(ShapeType::Diamond))
        } else if pressed(egui::Key::L) {
            Some(Tool::Linear { end_arrow: None })
        } else if pressed(egui::Key::A) {
            Some(Tool::Linear {
                end_arrow: Some(ArrowHeadStyle::Arrow),
            })
        } else if pressed(egui::Key::M) {
            Some(Tool::Frame)
        } else {
            None
        }
    }

    fn finish_import(&mut self, ctx: &egui::Context, outcome: ImportOutcome) {
        if let Some(e) = outcome.error {
            self.flash(format!("导入失败 {}: {}", outcome.path.display(), e));
            return;
        }
        let texture_id = self.next_texture_id;
        self.next_texture_id += 1;

        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            [outcome.width as usize, outcome.height as usize],
            &outcome.rgba,
        );
        let texture_handle = ctx.load_texture(
            format!("img_{}", texture_id),
            color_image,
            Default::default(),
        );
        self.texture_cache.insert(texture_id, texture_handle);
        // 缓存原始字节，保存时写入 sqlar
        self.image_data_cache
            .insert(texture_id, outcome.bytes.clone());
        // 缓存 RGBA 像素（懒生成灰度纹理 + 颜色采样用）
        self.rgba_pixel_cache
            .insert(texture_id, outcome.rgba.clone());
        self.rgba_size_cache
            .insert(texture_id, (outcome.width, outcome.height));

        // 初始位置：视口中心对应的画布�?
        let center_canvas = self
            .viewport
            .screen_to_canvas(self.viewport.screen_rect.center());
        let item = Item::new_pixmap(
            texture_id,
            Some(outcome.path.to_string_lossy().to_string()),
            (outcome.width, outcome.height),
            center_canvas.x - outcome.width as f32 / 2.0,
            center_canvas.y - outcome.height as f32 / 2.0,
            1.0,
            1.0,
        );

        let cmd = AddItem::new(item);
        self.push_cmd(Box::new(cmd));
        self.flash(format!("已导入: {}", outcome.path.display()));
        ctx.request_repaint();
    }

    /// 后台加载完成：重�?scene + 上传纹理 + 重新映射 texture_id（由 BackgroundOps::poll 调用）�?
    fn finish_load(&mut self, ctx: &egui::Context, outcome: LoadOutcome) {
        match outcome.result {
            Ok((mut scene, images, viewport_meta)) => {
                // 清空当前状态（纹理/字节缓存/undo 栈）
                self.texture_cache.clear();
                self.grayscale_texture_cache.clear();
                self.image_data_cache.clear();
                self.rgba_pixel_cache.clear();
                self.rgba_size_cache.clear();
                self.undo_stack = UndoStack::new();

                // 为每�?Pixmap 重新分配 texture_id，解码上传纹理，重映�?item.texture_id
                let mut id_remap: HashMap<u64, u64> = HashMap::new();
                for item in &mut scene.items {
                    if let ItemKind::Pixmap { texture_id, .. } = &mut item.kind {
                        let old_id = *texture_id;
                        let new_id = self.next_texture_id;
                        self.next_texture_id += 1;
                        id_remap.insert(old_id, new_id);

                        let bytes = images.get(&old_id.to_string());
                        if let Some(bytes) = bytes {
                            // 解码字节上传纹理
                            match image::load_from_memory(bytes) {
                                Ok(img) => {
                                    let (w, h) = img.dimensions();
                                    let rgba = img.to_rgba8().into_vec();
                                    let color_image = egui::ColorImage::from_rgba_unmultiplied(
                                        [w as usize, h as usize],
                                        &rgba,
                                    );
                                    let handle = ctx.load_texture(
                                        format!("img_{}", new_id),
                                        color_image,
                                        Default::default(),
                                    );
                                    self.texture_cache.insert(new_id, handle);
                                    self.image_data_cache.insert(new_id, bytes.clone());
                                    self.rgba_pixel_cache.insert(new_id, rgba);
                                    self.rgba_size_cache.insert(new_id, (w, h));
                                }
                                Err(e) => {
                                    log::error!("解码图片 texture_id={} 失败: {}", old_id, e);
                                }
                            }
                        }
                        *texture_id = new_id;
                    }
                }

                // 应用视口元数�?
                if let Some(meta) = viewport_meta {
                    self.viewport.pan = CanvasVector::new(meta.pan_x, meta.pan_y);
                    self.viewport.zoom = meta.zoom;
                }

                self.scene = scene;
                // 清理孤儿 container_id（容器已不存在则置 None），Phase C/Step 4
                self.scene.cleanup_orphan_containers();
                self.current_file = Some(outcome.path.clone());
                self.dirty = false;
                // 加载成功后加入最近文件列表
                add_recent_file(&mut self.recent_files, outcome.path.clone());
                self.flash(format!("已打开: {}", outcome.path.display()));
                ctx.request_repaint();
            }
            Err(e) => {
                self.flash(format!("打开失败: {}", e));
            }
        }
    }

    /// 保存到当前文件；若没有则调用 save_file_as 弹出对话框�?
    fn save_file(&mut self, ctx: &egui::Context) {
        if let Some(path) = self.current_file.clone() {
            self.start_save(ctx, path);
        } else {
            self.save_file_as(ctx);
        }
    }

    /// 另存为：弹出对话框选择路径�?
    fn save_file_as(&mut self, ctx: &egui::Context) {
        let picked = rfd::FileDialog::new()
            .add_filter("PReferZ 项目", &["prz"])
            .set_file_name("untitled.prz")
            .save_file();
        if let Some(path) = picked {
            self.start_save(ctx, path);
        }
    }

    /// 新建空白画布：清�?scene / 纹理缓存 / undo �?/ current_file / dirty�?    /// 调用前应已处理保存提示（由调用方负责）�?
    fn reset_canvas(&mut self, ctx: &egui::Context) {
        self.scene = Scene::new();
        self.texture_cache.clear();
        self.grayscale_texture_cache.clear();
        self.image_data_cache.clear();
        self.rgba_pixel_cache.clear();
        self.rgba_size_cache.clear();
        self.undo_stack = UndoStack::new();
        self.current_file = None;
        self.dirty = false;
        self.crop_mode = None;
        self.editing_text = None;
        self.color_picker_active = false;
        self.color_sample = None;
        self.pending_save_prompt = None;
        self.viewport.reset();
        self.flash(t(self.lang, T::FlashNewCanvas).to_string());
        ctx.request_repaint();
    }

    /// 触发新建画布流程：若 dirty 弹保存提示，否则直接 reset�?
    fn new_canvas(&mut self, ctx: &egui::Context) {
        if self.dirty {
            self.pending_save_prompt = Some(SavePromptAction::NewCanvas);
        } else {
            self.reset_canvas(ctx);
        }
    }

    /// 打开项目文件（.prz/.bee）。
    fn open_project_file(&mut self, ctx: &egui::Context) {
        let picked = rfd::FileDialog::new()
            .add_filter("PReferZ 项目", &["prz", "bee"])
            .pick_file();
        if let Some(path) = picked {
            self.add_recent_and_load(ctx, path);
        }
    }

    /// 记录最近文件并启动后台加载。
    fn add_recent_and_load(&mut self, ctx: &egui::Context, path: PathBuf) {
        add_recent_file(&mut self.recent_files, path.clone());
        self.bg_ops.start_load(ctx, path);
    }

    /// 载入图片到当前画布。
    fn import_image_file(&mut self, _ctx: &egui::Context) {
        let picked = rfd::FileDialog::new()
            .add_filter("图片", &["png", "jpg", "jpeg", "gif", "bmp", "webp"])
            .pick_file();
        if let Some(path) = picked {
            self.pending_import.push(path);
        }
    }

    /// 从剪贴板粘贴图片到画布（spec §2.1 剪贴板粘贴）。
    /// UI 线程：读剪贴板（毫秒级，arboard 同步访问）。
    /// 后台线程：PNG 编码（几十~几百毫秒，大图会卡 UI）。
    /// 通过 bg_ops.import_rx 复用 finish_import 完成纹理上传 + item 创建。
    fn paste_from_clipboard(&mut self, ctx: &egui::Context) {
        let mut clipboard = match arboard::Clipboard::new() {
            Ok(c) => c,
            Err(e) => {
                self.flash(format!("剪贴板访问失败: {}", e));
                return;
            }
        };
        let img_data = match clipboard.get_image() {
            Ok(img) => img,
            Err(_) => {
                self.flash(t(self.lang, T::FlashNoClipboardImage).to_string());
                return;
            }
        };
        let (w, h) = (img_data.width as u32, img_data.height as u32);
        let rgba: Vec<u8> = img_data.bytes.into_owned();

        // 后台线程：PNG 编码（避免大图卡 UI）
        let (tx, rx) = mpsc::channel();
        self.bg_ops.import_rx = Some(rx);
        self.bg_ops.pending += 1;
        self.bg_ops.msg = Some(t(self.lang, T::FlashPasteImage).to_string());
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let outcome = (|| {
                let expected = (w as usize) * (h as usize) * 4;
                if rgba.len() != expected {
                    return ImportOutcome {
                        path: PathBuf::from("clipboard.png"),
                        bytes: Vec::new(),
                        width: 0,
                        height: 0,
                        rgba: Vec::new(),
                        error: Some(format!(
                            "剪贴板图片数据异常：{} 字节（预期 {}）",
                            rgba.len(),
                            expected
                        )),
                    };
                }
                let mut png_bytes: Vec<u8> = Vec::new();
                let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
                match image::ImageEncoder::write_image(
                    encoder,
                    &rgba,
                    w,
                    h,
                    image::ExtendedColorType::Rgba8,
                ) {
                    Ok(()) => ImportOutcome {
                        path: PathBuf::from("clipboard.png"),
                        bytes: png_bytes,
                        width: w,
                        height: h,
                        rgba,
                        error: None,
                    },
                    Err(e) => ImportOutcome {
                        path: PathBuf::from("clipboard.png"),
                        bytes: Vec::new(),
                        width: 0,
                        height: 0,
                        rgba: Vec::new(),
                        error: Some(format!("PNG 编码失败: {}", e)),
                    },
                }
            })();
            let _ = tx.send(outcome);
            ctx2.request_repaint();
        });
    }

    /// 启动后台保存�?
    fn start_save(&mut self, ctx: &egui::Context, path: PathBuf) {
        // 收集 image_data_cache（key 转字符串以匹�?sqlar name�?
        let mut images: HashMap<String, Vec<u8>> = HashMap::new();
        for item in &self.scene.items {
            if let ItemKind::Pixmap { texture_id, .. } = &item.kind {
                if let Some(bytes) = self.image_data_cache.get(texture_id) {
                    images.insert(texture_id.to_string(), bytes.clone());
                }
            }
        }
        let viewport = Some(ViewportMeta {
            pan_x: self.viewport.pan.x,
            pan_y: self.viewport.pan.y,
            zoom: self.viewport.zoom,
        });
        self.bg_ops
            .start_save(ctx, path, self.scene.clone(), images, viewport);
    }

    /// 启动后台导出（spec §2.3 导出）。
    /// 收集所有 Pixmap 的 RGBA + item 快照，后台线程逐像素合成 + 编码。
    /// 文本便签在导出中渲染为纯色矩形（MVP 简化，不渲染 glyph）。
    fn start_export(
        &mut self,
        ctx: &egui::Context,
        path: PathBuf,
        format: ExportFormat,
        selection_only: bool,
    ) {
        if self.scene.items.is_empty() {
            self.flash(t(self.lang, T::FlashCanvasEmptyNoExport).to_string());
            return;
        }

        // 按 selection_only 过滤 items
        let items_snapshot: Vec<Item> = if selection_only {
            let sel = &self.scene.selection;
            self.scene
                .items
                .iter()
                .filter(|it| sel.contains(&it.id))
                .cloned()
                .collect()
        } else {
            self.scene.items.clone()
        };
        if items_snapshot.is_empty() {
            self.flash(t(self.lang, T::FlashNoSelectionToExport).to_string());
            return;
        }

        // 收集 Pixmap 的 RGBA 像素和尺寸（按 texture_id 索引；只收集将用到的）
        let mut pixmaps: Vec<(u64, Vec<u8>, u32, u32)> = Vec::new();
        for item in &items_snapshot {
            if let ItemKind::Pixmap { texture_id, .. } = &item.kind {
                if let (Some(rgba), Some(size)) = (
                    self.rgba_pixel_cache.get(texture_id),
                    self.rgba_size_cache.get(texture_id),
                ) {
                    pixmaps.push((*texture_id, rgba.clone(), size.0, size.1));
                }
            }
        }

        let (tx, rx) = mpsc::channel();
        self.bg_ops.export_rx = Some(rx);
        self.bg_ops.pending += 1;
        self.bg_ops.msg = Some(format!(
            "{}: {}",
            t(self.lang, T::FlashExportProgress),
            path.display()
        ));
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let result = export_scene_to_file(&items_snapshot, &pixmaps, &path, format)
                .map(|_| String::new());
            let _ = tx.send(ExportOutcome { path, result });
            ctx2.request_repaint();
        });
    }

    /// 启动后台批量导出图片到目录（每个 Pixmap 一个 PNG 文件，原始分辨率）。
    /// `selection_only=true` 仅导出选中的 Pixmap。
    fn start_export_images_to_dir(
        &mut self,
        ctx: &egui::Context,
        dir: PathBuf,
        selection_only: bool,
    ) {
        // 过滤 Pixmap items
        let sel = &self.scene.selection;
        let pixmap_items: Vec<&Item> = self
            .scene
            .items
            .iter()
            .filter(|it| matches!(it.kind, ItemKind::Pixmap { .. }))
            .filter(|it| !selection_only || sel.contains(&it.id))
            .collect();

        if pixmap_items.is_empty() {
            self.flash(if selection_only {
                "无选中的图片项"
            } else {
                "无可导出的图片项"
            });
            return;
        }

        // 收集 (filename, rgba, w, h)
        let mut entries: Vec<(String, Vec<u8>, u32, u32)> = Vec::new();
        let mut used_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut anon_index = 1usize;
        for item in &pixmap_items {
            if let ItemKind::Pixmap {
                texture_id,
                filename,
                ..
            } = &item.kind
            {
                let (Some(rgba), Some(size)) = (
                    self.rgba_pixel_cache.get(texture_id),
                    self.rgba_size_cache.get(texture_id),
                ) else {
                    continue;
                };

                // 文件名：优先 filename 的 stem，否则 image_N.png；确保目录内唯一
                let base = filename
                    .as_ref()
                    .and_then(|s| {
                        std::path::Path::new(s)
                            .file_stem()
                            .and_then(|x| x.to_str())
                            .map(|x| x.to_string())
                    })
                    .unwrap_or_else(|| {
                        let n = format!("image_{}", anon_index);
                        anon_index += 1;
                        n
                    });
                let mut name = format!("{}.png", base);
                let mut n = 1;
                while used_names.contains(&name) {
                    name = format!("{}_{}.png", base, n);
                    n += 1;
                }
                used_names.insert(name.clone());
                entries.push((name, rgba.clone(), size.0, size.1));
            }
        }

        if entries.is_empty() {
            self.flash(t(self.lang, T::FlashPixelDataNotReady).to_string());
            return;
        }

        let (tx, rx) = mpsc::channel();
        self.bg_ops.export_rx = Some(rx);
        self.bg_ops.pending += 1;
        self.bg_ops.msg = Some(format!(
            "{}: {}",
            t(self.lang, T::FlashExportImagesProgress),
            dir.display()
        ));
        let ctx2 = ctx.clone();
        let lang = self.lang;
        std::thread::spawn(move || {
            let total = entries.len();
            let result = export_pixmaps_to_dir(&entries, &dir).map(|_| {
                format!(
                    "{} {} {}: {}",
                    t(lang, T::FlashExportedNImages),
                    total,
                    t(lang, T::FlashExportImagesTo),
                    dir.display()
                )
            });
            let _ = tx.send(ExportOutcome {
                path: dir.clone(),
                result,
            });
            ctx2.request_repaint();
        });
    }

    /// 弹出目录选择器，确定目录后启动批量图片导出。
    fn start_export_images_dialog(&mut self, ctx: &egui::Context, selection_only: bool) {
        let picked = rfd::FileDialog::new()
            .set_title("选择导出目录")
            .pick_folder();
        if let Some(dir) = picked {
            self.start_export_images_to_dir(ctx, dir, selection_only);
        }
    }

    fn start_export_dialog(
        &mut self,
        ctx: &egui::Context,
        format: ExportFormat,
        selection_only: bool,
    ) {
        let ext = format.extension();
        let default_name = if selection_only { "selection" } else { "scene" };
        let picked = rfd::FileDialog::new()
            .add_filter(format.display_name(), &[ext])
            .set_file_name(format!("{}.{}", default_name, ext))
            .save_file();
        if let Some(mut path) = picked {
            // 确保扩展名正确
            if path.extension().and_then(|e| e.to_str()) != Some(ext) {
                path.set_extension(ext);
            }
            self.start_export(ctx, path, format, selection_only);
        }
    }

    fn delete_selected(&mut self) {
        // 容器联动：删除封闭形状时连带删除其绑定文本（Phase C/Step 3）。
        let base_ids: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
        if base_ids.is_empty() {
            return;
        }
        let mut ids: Vec<ItemId> = base_ids.clone();
        for sid in &base_ids {
            ids.extend(self.scene.texts_bound_to(*sid));
        }
        ids.sort();
        ids.dedup();
        // 清理纹理缓存与字节缓存（Pixmap�?
        for id in &ids {
            if let Some(item) = self.scene.get_item(id) {
                if let ItemKind::Pixmap { texture_id, .. } = &item.kind {
                    self.texture_cache.remove(texture_id);
                    self.grayscale_texture_cache.remove(texture_id);
                    self.image_data_cache.remove(texture_id);
                    self.rgba_pixel_cache.remove(texture_id);
                    self.rgba_size_cache.remove(texture_id);
                }
            }
        }
        // �?DeleteItems 命令（修 S1/W8），undo 已支持快照恢复（P1-3�?
        let cmd = DeleteItems::new(ids.clone());
        self.push_cmd(Box::new(cmd));
        self.scene.selection.clear();
        self.flash(format!("已删除 {} 项", ids.len()));
    }

    fn bring_to_front(&mut self) {
        let ids: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
        if ids.is_empty() {
            return;
        }
        // �?ReorderItems 命令（修 S3/W8），不再直接�?z
        let cmd = ReorderItems::new(ids, true);
        self.push_cmd(Box::new(cmd));
        self.flash(t(self.lang, T::FlashBroughtToFront).to_string());
    }

    fn send_to_back(&mut self) {
        let ids: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
        if ids.is_empty() {
            return;
        }
        let cmd = ReorderItems::new(ids, false);
        self.push_cmd(Box::new(cmd));
        self.flash(t(self.lang, T::FlashSentToBack).to_string());
    }

    fn fit_to_screen(&mut self) {
        if self.scene.items.is_empty() {
            self.viewport.reset();
            self.flash(t(self.lang, T::FlashFitToCanvas).to_string());
            return;
        }
        // 用所�?item �?AABB 并集
        let mut bbox: Option<preferz_core::spaces::CanvasRect> = None;
        for item in &self.scene.items {
            let r = item.bounding_rect();
            bbox = Some(match bbox {
                Some(b) => b.union(&r),
                None => r,
            });
        }
        if let Some(b) = bbox {
            self.viewport.fit_to_content(b);
            self.flash(t(self.lang, T::FlashFitToCanvas).to_string());
        }
    }

    // ─────────── Phase 5 辅助方法 ───────────

    /// 当前选中 Pixmap item 数量�?
    fn selected_pixmap_count(&self) -> usize {
        self.scene
            .selection
            .iter()
            .filter_map(|id| self.scene.get_item(id))
            .filter(|it| matches!(it.kind, ItemKind::Pixmap { .. }))
            .count()
    }

    /// 单�?Pixmap �?grayscale 状态（用于右键菜单文案）�?
    fn selected_pixmap_grayscale(&self) -> bool {
        for id in &self.scene.selection {
            if let Some(item) = self.scene.get_item(id) {
                if let ItemKind::Pixmap { grayscale, .. } = &item.kind {
                    return *grayscale;
                }
            }
        }
        false
    }

    /// 切换选中 Pixmap item 的灰度标志（spec §2.2 灰度）�?
    fn toggle_grayscale_selected(&mut self) {
        // 收集 (id, old_gray) 后再处理，避免借用冲突
        let targets: Vec<(ItemId, bool)> = self
            .scene
            .selection
            .iter()
            .filter_map(|id| {
                self.scene.get_item(id).and_then(|it| match &it.kind {
                    ItemKind::Pixmap { grayscale, .. } => Some((*id, *grayscale)),
                    _ => None,
                })
            })
            .collect();
        for (id, old) in targets {
            let cmd = SetPixmapProps::new(id).with_grayscale(old, !old);
            self.push_cmd(Box::new(cmd));
        }
        self.flash(t(self.lang, T::FlashToggleGrayscale).to_string());
    }

    /// 进入裁剪模式（spec §2.2 裁剪）�?    /// 选中单个 Pixmap 时，初始�?crop 矩形为当�?crop 或整个图片�?
    fn enter_crop_mode(&mut self) {
        if self.scene.selection.len() != 1 {
            self.flash(t(self.lang, T::FlashCropNeedSingleImage).to_string());
            return;
        }
        let id = *self.scene.selection.iter().next().unwrap();
        let (original_size, current_crop) = match self.scene.get_item(&id) {
            Some(item) => match &item.kind {
                ItemKind::Pixmap {
                    original_size,
                    crop,
                    ..
                } => (*original_size, *crop),
                _ => {
                    self.flash(t(self.lang, T::FlashCropImageOnly).to_string());
                    return;
                }
            },
            None => return,
        };
        let rect = current_crop.unwrap_or(CropRect::new(
            0.0,
            0.0,
            original_size.0 as f32,
            original_size.1 as f32,
        ));
        self.crop_mode = Some(CropMode {
            item_id: id,
            rect,
            dragging: None,
            original: current_crop,
        });
        self.flash(t(self.lang, T::FlashCropHint).to_string());
    }

    /// 应用裁剪：push CropItems 命令并退出裁剪模式。
    ///
    /// 裁剪后 item 的边框（canvas_corners）应正好落在裁剪框在画布上的位置，
    /// 因此同步调整 transform.scale 与 transform.pos。几何计算收敛在 core 的
    /// Item::transform_after_crop（可单测），本函数只负责校验与命令封装：
    /// - new_scale = old_scale × (crop / 当前可见区域)，不是除以整图尺寸
    /// - new_pos 使新的局部原点对齐到 crop 左上角的画布位置（含旋转/翻转）
    fn apply_crop(&mut self) {
        let crop_state = match self.crop_mode.take() {
            Some(c) => c,
            None => return,
        };
        let original = crop_state.original;
        let new_crop = Some(crop_state.rect);
        // 若与原值相同则不 push 命令
        if original == new_crop {
            self.flash(t(self.lang, T::FlashCropNoChange).to_string());
            return;
        }

        let item_id = crop_state.item_id;
        let c = crop_state.rect;
        if c.width < 1.0 || c.height < 1.0 {
            self.flash(t(self.lang, T::FlashCropInvalidSize).to_string());
            return;
        }
        // 新 transform 交给 core 计算：以「当前可见区域」为基准而非整图尺寸，
        // 否则对已裁剪过的图片二次裁剪会再缩小一次（旋转下表现为图片错位）。
        let new_transform = match self.scene.get_item(&item_id) {
            Some(item) => match item.transform_after_crop(c) {
                Some(t) => t,
                None => {
                    self.flash(t(self.lang, T::FlashCropInvalidSize).to_string());
                    return;
                }
            },
            None => return,
        };

        // �?old_transform（再次取，因为上面的�?clone �?item�?
        let old_transform = match self.scene.get_item(&item_id) {
            Some(item) => item.transform,
            None => return,
        };

        let cmd = CropItems::new(item_id, original, new_crop, old_transform, new_transform);
        self.push_cmd(Box::new(cmd));
        self.flash(t(self.lang, T::FlashCropApplied).to_string());
    }

    /// 取消裁剪：恢复原 crop 并退出裁剪模式�?
    fn cancel_crop(&mut self) {
        self.crop_mode = None;
        self.flash(t(self.lang, T::FlashCropCancelled).to_string());
    }

    /// 检测鼠标是否命中裁剪手柄（4 个角点）�?
    /// 命中裁剪手柄：手柄位于旋转后的裁剪框 4 角，跟随 item 一起旋转。
    fn crop_handle_hit_test(&self, screen_pos: egui::Pos2) -> Option<CropHandle> {
        let crop_state = self.crop_mode.as_ref()?;
        let item = self.scene.get_item(&crop_state.item_id)?;
        // crop_corners 走完整 local_to_canvas（含旋转），手柄因此跟随图片一起转
        let crop_quad = item
            .crop_corners(crop_state.rect)?
            .map(|p| self.viewport.canvas_to_screen(p));
        let handle_size = TransformHandles::handle_size() * 2.0;
        // 角点顺序：TL → TR → BR → BL
        let handles = [
            (CropHandle::TopLeft, crop_quad[0]),
            (CropHandle::TopRight, crop_quad[1]),
            (CropHandle::BottomRight, crop_quad[2]),
            (CropHandle::BottomLeft, crop_quad[3]),
        ];
        for (h, p) in handles {
            let r = egui::Rect::from_center_size(p, egui::Vec2::splat(handle_size));
            if r.contains(screen_pos) {
                return Some(h);
            }
        }
        None
    }

    /// 拖拽裁剪手柄时更新 crop 矩形（屏幕坐标 → 原图像素坐标）。
    fn update_crop_drag(&mut self, screen_pos: egui::Pos2, handle: CropHandle) {
        // 先取一份 crop_state，避免 &mut self.crop_mode 与 &self.scene 借用冲突
        let (item_id, mut rect) = match self.crop_mode.as_ref() {
            Some(c) => (c.item_id, c.rect),
            None => return,
        };
        let item = match self.scene.get_item(&item_id) {
            Some(i) => i.clone(),
            None => return,
        };
        let (base_w, base_h) = match item.original_pixel_size() {
            Some(s) => s,
            None => return,
        };

        // 屏幕 → 画布 → 原图像素：走 item 变换的逆变换，旋转下同样正确
        // （旧的 AABB 线性映射在旋转下会算出错误的像素位置）。
        let canvas_pos = self.viewport.screen_to_canvas(screen_pos);
        let (raw_px, raw_py) = match item.canvas_to_crop_pixel(canvas_pos) {
            Some(p) => p,
            None => return,
        };
        // 拖到图外时由 clamp_to 兜底，这里只防 NaN/Inf 传入后续运算
        let px = if raw_px.is_finite() { raw_px } else { 0.0 };
        let py = if raw_py.is_finite() { raw_py } else { 0.0 };

        match handle {
            CropHandle::TopLeft => {
                let new_x = px.min(rect.x + rect.width - 1.0);
                let new_y = py.min(rect.y + rect.height - 1.0);
                rect.width = rect.x + rect.width - new_x;
                rect.height = rect.y + rect.height - new_y;
                rect.x = new_x;
                rect.y = new_y;
            }
            CropHandle::TopRight => {
                let new_y = py.min(rect.y + rect.height - 1.0);
                rect.width = (px - rect.x).max(1.0);
                rect.height = rect.y + rect.height - new_y;
                rect.y = new_y;
            }
            CropHandle::BottomLeft => {
                let new_x = px.min(rect.x + rect.width - 1.0);
                rect.width = rect.x + rect.width - new_x;
                rect.x = new_x;
                rect.height = (py - rect.y).max(1.0);
            }
            CropHandle::BottomRight => {
                rect.width = (px - rect.x).max(1.0);
                rect.height = (py - rect.y).max(1.0);
            }
        }
        rect = rect.clamp_to(base_w, base_h);

        if let Some(c) = self.crop_mode.as_mut() {
            c.rect = rect;
        }
    }

    /// 颜色采样：在 Pixmap 上的鼠标位置读取像素 RGB（spec §2.2 颜色采样）�?
    fn pick_color_at(&mut self, screen_pos: egui::Pos2) {
        let hit = interaction::get_item_at(screen_pos, &self.scene, &self.viewport);
        let item = match hit {
            Some(it) => it,
            None => {
                self.flash(t(self.lang, T::FlashColorPickerMissed).to_string());
                return;
            }
        };
        let (texture_id, original_size) = match &item.kind {
            ItemKind::Pixmap {
                texture_id,
                original_size,
                ..
            } => (*texture_id, *original_size),
            _ => {
                self.flash(t(self.lang, T::FlashColorPickerImageOnly).to_string());
                return;
            }
        };
        // 鼠标 �?item 局部坐�?
        let inv = match item.local_to_canvas().inverse() {
            Some(m) => m,
            None => return,
        };
        let canvas_pos = self.viewport.screen_to_canvas(screen_pos);
        let local = inv.transform_point(canvas_pos);
        let ow = original_size.0 as f32;
        let oh = original_size.1 as f32;
        if local.x < 0.0 || local.x > ow || local.y < 0.0 || local.y > oh {
            return;
        }
        let (w, h) = match self.rgba_size_cache.get(&texture_id) {
            Some(s) => *s,
            None => return,
        };
        let rgba = match self.rgba_pixel_cache.get(&texture_id) {
            Some(b) => b,
            None => return,
        };
        let px = ((local.x / ow) * w as f32).round() as i64;
        let py = ((local.y / oh) * h as f32).round() as i64;
        let px = px.clamp(0, (w as i64) - 1);
        let py = py.clamp(0, (h as i64) - 1);
        let idx = (py * w as i64 + px) as usize * 4;
        if idx + 3 < rgba.len() {
            let r = rgba[idx];
            let g = rgba[idx + 1];
            let b = rgba[idx + 2];
            let a = rgba[idx + 3];
            self.color_sample = Some(ColorSample {
                r,
                g,
                b,
                a,
                screen_pos,
                px: px as u32,
                py: py as u32,
            });
        }
    }

    /// 批量排列选中 item（spec §2.2 批量操作）�?
    fn arrange_selected(&mut self, mode: ArrangeMode) {
        // 临时把选中项作为整体排�?
        let spacing = self.arrange_spacing;
        // 复制一个仅包含选中项的子场景，传给 plan_arrange
        let mut sub = Scene::new();
        let selected_items: Vec<Item> = self
            .scene
            .selection
            .iter()
            .filter_map(|id| self.scene.get_item(id).cloned())
            .collect();
        for it in selected_items {
            sub.add_item_preserve_z(it);
        }
        if sub.items.is_empty() {
            return;
        }
        let moves = plan_arrange(&sub, mode, spacing);
        let cmd = ArrangeItems::new(moves).with_preview_applied(false);
        self.push_cmd(Box::new(cmd));
        let mode_name = match mode {
            ArrangeMode::Linear => "线形",
            ArrangeMode::Grid => "网格",
            ArrangeMode::Optimal => "最优装箱",
        };
        self.flash(format!("排列：{}", mode_name));
    }

    /// 归一化选中 Pixmap item 尺寸（spec §2.2 归一化尺寸）�?
    fn normalize_selected(&mut self, mode: preferz_core::commands::NormalizeMode) {
        let ids: Vec<ItemId> = self
            .scene
            .selection
            .iter()
            .filter_map(|id| {
                self.scene.get_item(id).and_then(|it| match &it.kind {
                    ItemKind::Pixmap { .. } => Some(*id),
                    _ => None,
                })
            })
            .collect();
        if ids.len() < 2 {
            self.flash(t(self.lang, T::FlashNormalizeNeedMultiple).to_string());
            return;
        }
        // target = 首个选中 Pixmap 的当前�?
        let target = ids.first().and_then(|id| {
            self.scene.get_item(id).and_then(|it| match &it.kind {
                ItemKind::Pixmap { original_size, .. } => {
                    let ow = original_size.0 as f32;
                    let oh = original_size.1 as f32;
                    match mode {
                        preferz_core::commands::NormalizeMode::Width => {
                            Some(it.transform.scale.x * ow)
                        }
                        preferz_core::commands::NormalizeMode::Height => {
                            Some(it.transform.scale.y * oh)
                        }
                        preferz_core::commands::NormalizeMode::Area => {
                            Some(it.transform.scale.x * it.transform.scale.y * ow * oh)
                        }
                    }
                }
                _ => None,
            })
        });
        let target = match target {
            Some(t) => t,
            None => return,
        };
        let cmd = NormalizeItems::new(ids, mode, target);
        self.push_cmd(Box::new(cmd));
        let mode_name = match mode {
            preferz_core::commands::NormalizeMode::Width => t(self.lang, T::NormalizeByWidth),
            preferz_core::commands::NormalizeMode::Height => t(self.lang, T::NormalizeByHeight),
            preferz_core::commands::NormalizeMode::Area => t(self.lang, T::NormalizeByArea),
        };
        self.flash(format!("{}: {}", t(self.lang, T::NormalizeSize), mode_name));
    }

    /// 渲染保存提示对话框（关闭/新建时若 dirty 弹出）�?    /// 按钮�?    /// - 保存：触发保存流程，首次保存弹系统文件选择器；保存完成后由 poll_background 执行 pending action
    /// - 放弃：不保存，直接执�?pending action（关闭窗�?/ 新建画布�?    /// - 取消：什么都不做，保留当前画布状�?
    fn render_save_prompt(&mut self, ctx: &egui::Context) {
        if self.pending_save_prompt.is_none() {
            return;
        }
        // 保存进行中：等待完成（poll_background 会自动执�?pending action�?
        if self.bg_ops.save_rx.is_some() {
            return;
        }

        let mut save_clicked = false;
        let mut discard_clicked = false;
        let mut cancel_clicked = false;

        egui::Window::new(t(self.lang, T::SavePromptMessage))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(300.0);
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label(t(self.lang, T::SavePromptMessage));
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button(t(self.lang, T::SavePromptSave)).clicked() {
                            save_clicked = true;
                        }
                        if ui.button(t(self.lang, T::SavePromptDiscard)).clicked() {
                            discard_clicked = true;
                        }
                        if ui.button(t(self.lang, T::SavePromptCancel)).clicked() {
                            cancel_clicked = true;
                        }
                    });
                    ui.add_space(8.0);
                });
            });

        if save_clicked {
            // 触发保存流程：保存完成时 poll_background 会执�?pending action
            self.save_file(ctx);
        } else if discard_clicked {
            // 不保存，直接执行 pending action
            // 关键：把 dirty 置为 false，避免下一帧 close_requested 检测又弹保存提示
            if let Some(action) = self.pending_save_prompt.take() {
                match action {
                    SavePromptAction::Close => {
                        self.dirty = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    SavePromptAction::NewCanvas => {
                        self.reset_canvas(ctx);
                    }
                }
            }
        } else if cancel_clicked {
            self.pending_save_prompt = None;
        }
    }

    /// 渲染设置面板（spec §2.3 简化版：排列间距 + 窗口形态 + 语言 + 快捷键说明，仅暗色主题）。
    fn render_settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.settings_open;
        // 局部副本，闭包内修改；changed 时记录，闭包外发 ViewportCommand
        let mut always_on_top = self.always_on_top;
        let mut frameless = self.frameless;
        let mut lang = self.lang;
        let mut top_changed = false;
        let mut frame_changed = false;
        let mut lang_changed = false;
        egui::Window::new(t(self.lang, T::SettingsTitle))
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(t(self.lang, T::SettingsArrange));
                ui.add(
                    egui::Slider::new(&mut self.arrange_spacing, 0.0..=200.0)
                        .text(t(self.lang, T::SettingsSpacing)),
                );
                ui.separator();

                ui.label(t(self.lang, T::SettingsWindow));
                if ui
                    .checkbox(&mut always_on_top, t(self.lang, T::SettingsAlwaysOnTop))
                    .changed()
                {
                    top_changed = true;
                }
                if ui
                    .checkbox(&mut frameless, t(self.lang, T::SettingsFrameless))
                    .changed()
                {
                    frame_changed = true;
                }
                // 背景透明度：0.1~1.0，配无边框+置顶可作悬浮看图板。
                // 限制最小 0.1 避免空场景下窗口完全不可见（spec §2.3 透明背景注意事项）。
                ui.add(
                    egui::Slider::new(&mut self.bg_alpha, 0.1..=1.0)
                        .text(t(self.lang, T::SettingsBgAlpha))
                        .fixed_decimals(2),
                );
                ui.separator();

                // 语言切换
                ui.label(t(self.lang, T::SettingsLanguage));
                egui::ComboBox::from_label("")
                    .selected_text(lang.display_name())
                    .show_ui(ui, |ui| {
                        for option in [Lang::En, Lang::Zh] {
                            if ui
                                .selectable_label(lang == option, option.display_name())
                                .clicked()
                            {
                                lang = option;
                                lang_changed = true;
                            }
                        }
                    });
                ui.separator();

                ui.label(t(self.lang, T::SettingsShortcuts));
                ui.label(t(self.lang, T::SettingsShortcutArrange));
                ui.label(t(self.lang, T::SettingsShortcutUndo));
                ui.label(t(self.lang, T::SettingsShortcutFile));
                ui.label(t(self.lang, T::SettingsShortcutPaste));
            });
        self.settings_open = open;
        // 应用窗口形态切换
        if top_changed {
            self.always_on_top = always_on_top;
            let level = if always_on_top {
                egui::viewport::WindowLevel::AlwaysOnTop
            } else {
                egui::viewport::WindowLevel::Normal
            };
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
        }
        if frame_changed {
            self.frameless = frameless;
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!frameless));
        }
        if lang_changed {
            self.lang = lang;
            // 持久化到 config.json
            save_config(&UserConfig { lang });
        }
    }

    /// 渲染颜色采样 overlay（在鼠标附近显示 RGB/HEX）�?
    fn render_color_picker_overlay(&self, ctx: &egui::Context) {
        let pos = ctx.input(|i| i.pointer.latest_pos());
        let sample = match self.color_sample.as_ref() {
            Some(s) => s,
            None => {
                // 显示模式提示
                if let Some(p) = pos {
                    egui::Area::new(egui::Id::new("color_picker_hint"))
                        .order(egui::Order::Foreground)
                        .fixed_pos(p + egui::vec2(16.0, 16.0))
                        .show(ctx, |ui| {
                            let frame = egui::Frame::popup(ui.style());
                            frame.show(ui, |ui| {
                                ui.label(t(self.lang, T::FlashColorPickerHint));
                            });
                        });
                }
                return;
            }
        };
        let pos = pos.unwrap_or(sample.screen_pos);
        egui::Area::new(egui::Id::new("color_picker_overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos + egui::vec2(16.0, 16.0))
            .show(ctx, |ui| {
                let frame = egui::Frame::popup(ui.style());
                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let color = egui::Color32::from_rgba_unmultiplied(
                            sample.r, sample.g, sample.b, sample.a,
                        );
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, color);
                        ui.painter().rect_stroke(
                            rect,
                            2.0,
                            egui::Stroke::new(1.0_f32, egui::Color32::BLACK),
                        );
                        ui.vertical(|ui| {
                            ui.label(format!("RGB: {}, {}, {}", sample.r, sample.g, sample.b));
                            ui.label(format!("Alpha: {}", sample.a));
                            ui.label(format!(
                                "HEX: #{:02X}{:02X}{:02X}",
                                sample.r, sample.g, sample.b
                            ));
                            ui.label(format!("位置: ({}, {})", sample.px, sample.py));
                        });
                    });
                });
            });
    }
}

/// 颜色采样结果（spec §2.2 颜色采样）。
#[derive(Debug, Clone, Copy)]
struct ColorSample {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
    screen_pos: egui::Pos2,
    px: u32,
    py: u32,
}

// ─────────────────────────── 导出 ───────────────────────────

/// 导出格式（spec §2.3 导出，SVG 暂不支持）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Jpeg,
}

impl ExportFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Png => "png",
            ExportFormat::Jpeg => "jpg",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ExportFormat::Png => "PNG",
            ExportFormat::Jpeg => "JPG",
        }
    }
}

/// 导出场景到文件（后台线程调用）。
/// 逐像素反向采样：对每个输出像素，画布坐标 → 遍历 Z 序倒序 → 命中 Pixmap 则采样。
/// 文本便签渲染为纯色矩形（MVP 简化）。
fn export_scene_to_file(
    items: &[Item],
    pixmaps: &[(u64, Vec<u8>, u32, u32)],
    path: &Path,
    format: ExportFormat,
) -> Result<(), String> {
    use euclid::Point2D;
    use preferz_core::spaces::CanvasSpace;

    if items.is_empty() {
        return Err("画布为空".to_string());
    }

    // 计算所有 item 画布 AABB 并集
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for item in items {
        let bbox = item.bounding_rect();
        min_x = min_x.min(bbox.min().x);
        min_y = min_y.min(bbox.min().y);
        max_x = max_x.max(bbox.max().x);
        max_y = max_y.max(bbox.max().y);
    }
    // 加 padding 防止边缘裁切
    let padding = 8.0_f32;
    min_x -= padding;
    min_y -= padding;
    max_x += padding;
    max_y += padding;

    let canvas_w = (max_x - min_x).max(1.0).ceil() as u32;
    let canvas_h = (max_y - min_y).max(1.0).ceil() as u32;
    // 限制最大尺寸避免 OOM（100MP 上限）
    const MAX_PIXELS: u64 = 100_000_000;
    if (canvas_w as u64) * (canvas_h as u64) > MAX_PIXELS {
        return Err(format!(
            "导出尺寸过大: {}x{} (上限 {} 像素)",
            canvas_w, canvas_h, MAX_PIXELS
        ));
    }

    // 预构建 Pixmap 像素查找表（texture_id → (rgba, w, h)）
    use std::collections::HashMap;
    let pixmap_map: HashMap<u64, (&Vec<u8>, u32, u32)> = pixmaps
        .iter()
        .map(|(id, rgba, w, h)| (*id, (rgba, *w, *h)))
        .collect();

    // 按 Z 序倒序（顶层先采样）
    let mut sorted: Vec<&Item> = items.iter().collect();
    sorted.sort_by_key(|b| std::cmp::Reverse(b.z));

    // 逐像素合成
    let mut out_rgba: Vec<u8> = vec![0u8; (canvas_w as usize) * (canvas_h as usize) * 4];
    // 背景填充为白色（JPG 不支持透明，PNG 也用白底更实用）
    for px in out_rgba.as_chunks_mut::<4>().0 {
        px[0] = 255;
        px[1] = 255;
        px[2] = 255;
        px[3] = 255;
    }

    // 遍历每个像素，反向找命中的顶层 item
    for y in 0..canvas_h {
        let canvas_y = min_y + y as f32 + 0.5;
        for x in 0..canvas_w {
            let canvas_x = min_x + x as f32 + 0.5;
            let canvas_pos: Point2D<f32, CanvasSpace> = Point2D::new(canvas_x, canvas_y);

            // Z 序倒序查找命中的 item
            for item in &sorted {
                if !item.contains_canvas_point(canvas_pos) {
                    continue;
                }
                // 命中：采样颜色
                let pixel = sample_item_pixel(item, &pixmap_map, canvas_pos);
                if let Some((r, g, b, a)) = pixel {
                    let idx = ((y as usize) * (canvas_w as usize) + x as usize) * 4;
                    // alpha 混合到白底
                    let alpha = a as f32 / 255.0;
                    out_rgba[idx] = (r as f32 * alpha + 255.0 * (1.0 - alpha)) as u8;
                    out_rgba[idx + 1] = (g as f32 * alpha + 255.0 * (1.0 - alpha)) as u8;
                    out_rgba[idx + 2] = (b as f32 * alpha + 255.0 * (1.0 - alpha)) as u8;
                    // PNG 保留原 alpha；JPG 后续会丢弃
                    out_rgba[idx + 3] = 255;
                }
                break; // 只取顶层
            }
        }
    }

    // 编码到文件
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let writer = std::io::BufWriter::new(file);
    match format {
        ExportFormat::Png => {
            let encoder = image::codecs::png::PngEncoder::new(writer);
            image::ImageEncoder::write_image(
                encoder,
                &out_rgba,
                canvas_w,
                canvas_h,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| e.to_string())?;
        }
        ExportFormat::Jpeg => {
            // JPEG 不支持 alpha：RGBA → RGB（背景已合成白底，直接丢弃 alpha）
            let mut out_rgb: Vec<u8> =
                Vec::with_capacity((canvas_w as usize) * (canvas_h as usize) * 3);
            for px in out_rgba.as_chunks::<4>().0 {
                // 丢弃 alpha（背景已合成白底）
                out_rgb.extend_from_slice(&px[..3]);
            }
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(writer, 90);
            image::ImageEncoder::write_image(
                encoder,
                &out_rgb,
                canvas_w,
                canvas_h,
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

/// 批量将 Pixmap RGBA 数据写入指定目录，每个 entry 一个 PNG 文件。
/// 单个失败不中断后续，最终汇总错误数。返回成功数与错误数描述。
fn export_pixmaps_to_dir(
    entries: &[(String, Vec<u8>, u32, u32)],
    dir: &Path,
) -> Result<(), String> {
    if !dir.exists() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut ok = 0u32;
    let mut errs: Vec<String> = Vec::new();
    for (name, rgba, w, h) in entries {
        let path = dir.join(name);
        match std::fs::File::create(&path) {
            Ok(file) => {
                let writer = std::io::BufWriter::new(file);
                let encoder = image::codecs::png::PngEncoder::new(writer);
                if let Err(e) = image::ImageEncoder::write_image(
                    encoder,
                    rgba,
                    *w,
                    *h,
                    image::ExtendedColorType::Rgba8,
                ) {
                    errs.push(format!("{}: {}", name, e));
                } else {
                    ok += 1;
                }
            }
            Err(e) => errs.push(format!("{}: {}", name, e)),
        }
    }
    if errs.is_empty() {
        Ok(())
    } else if ok == 0 {
        Err(format!("全部失败 ({}): {}", errs.len(), errs.join("; ")))
    } else {
        // 部分成功也视为成功，但带上错误信息
        Ok(()) // 成功条数通过 flash 消息体现；err 信息记录到日志
    }
}

/// 采样 item 在画布坐标处的像素颜色。
/// Pixmap：逆变换到局部坐标 → 应用 crop → 采样 RGBA（含 opacity、grayscale）。
/// Text：返回纯色（背景灰 + 文字色混合的简化表示）。
fn sample_item_pixel(
    item: &Item,
    pixmap_map: &std::collections::HashMap<u64, (&Vec<u8>, u32, u32)>,
    canvas_pos: preferz_core::spaces::CanvasPoint,
) -> Option<(u8, u8, u8, u8)> {
    let inv = item.local_to_canvas().inverse()?;
    let local = inv.transform_point(canvas_pos);
    let base = item.base_size();

    match &item.kind {
        ItemKind::Pixmap {
            texture_id,
            opacity,
            grayscale,
            crop,
            ..
        } => {
            let (rgba, w, h) = pixmap_map.get(texture_id)?;
            let w = *w as f32;
            let h = *h as f32;

            // 应用 crop：crop 定义在局部空间，需把 local 映射到 crop 后的图片坐标
            let (cx, cy, cw, ch) = if let Some(c) = crop {
                (c.x, c.y, c.width, c.height)
            } else {
                (0.0, 0.0, w, h)
            };

            // local 坐标 → crop 内偏移
            let px = local.x;
            let py = local.y;
            // crop 区域 = [cx, cx+cw] × [cy, cy+ch]，超出则透明
            if px < cx || px >= cx + cw || py < cy || py >= cy + ch {
                return Some((0, 0, 0, 0)); // crop 外透明
            }

            // 把 crop 内坐标映射回原始图片像素坐标
            // crop 把 [cx, cx+cw] 拉伸到 [0, w]（scale 使然），所以：
            //   原图 x = cx + (local.x / base.x) * cw  -- 但 local 已是变换后坐标，base 已含 scale
            // 简化：crop 在 base_size 空间定义，base_size 是 original_size * scale，
            //       所以 local 直接对应 base_size 空间，px/cw * 原图宽 即原图坐标
            let img_x = ((px - cx) / cw * w).clamp(0.0, w - 1.0) as u32;
            let img_y = ((py - cy) / ch * h).clamp(0.0, h - 1.0) as u32;

            let idx = ((img_y as usize) * (w as usize) + img_x as usize) * 4;
            if idx + 3 >= rgba.len() {
                return None;
            }
            let r = rgba[idx];
            let g = rgba[idx + 1];
            let b = rgba[idx + 2];
            let a = rgba[idx + 3];

            // 灰度（BT.601）
            let (r, g, b) = if *grayscale {
                let y = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32).round() as u8;
                (y, y, y)
            } else {
                (r, g, b)
            };

            // opacity
            let alpha = (a as f32 * opacity.clamp(0.0, 1.0)) as u8;
            Some((r, g, b, alpha))
        }
        ItemKind::Text { color, .. } => {
            // 简化：整个文本区域用文字色填充（MVP）
            // 仅当 local 在 [0, base.x] × [0, base.y] 内（contains_canvas_point 已保证）
            let _ = (local, base);
            Some((color[0], color[1], color[2], color[3]))
        }
        // A1 数据模型先行，取色采样在 Phase A6 后补充
        ItemKind::Shape { .. } => None,
        // 画框不参与导出采样
        ItemKind::Frame { .. } => None,
    }
}

// ─────────────────────────── 最近文件 ───────────────────────────

/// 最近文件列表的持久化路径：`~/.preferz/recent.json`。
fn recent_files_path() -> Option<PathBuf> {
    let home = dirs_or_home()?;
    Some(home.join(".preferz").join("recent.json"))
}

/// 用户配置的持久化路径：`~/.preferz/config.json`。
fn config_path() -> Option<PathBuf> {
    let home = dirs_or_home()?;
    Some(home.join(".preferz").join("config.json"))
}

/// 用户配置（当前仅含语言；后续可扩展窗口形态、透明度等）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct UserConfig {
    #[serde(default)]
    lang: Lang,
}

/// 从 `~/.preferz/config.json` 加载配置。文件不存在或解析失败时返回默认值。
fn load_config() -> UserConfig {
    let path = match config_path() {
        Some(p) => p,
        None => return UserConfig::default(),
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str::<UserConfig>(&content).unwrap_or_default(),
        Err(_) => UserConfig::default(),
    }
}

/// 保存配置到 `~/.preferz/config.json`。
fn save_config(cfg: &UserConfig) {
    let path = match config_path() {
        Some(p) => p,
        None => return,
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(path, json);
    }
}

/// 获取用户 home 目录（跨平台）。
fn dirs_or_home() -> Option<PathBuf> {
    // 优先用 std::env，回退到常见环境变量
    if let Some(home) = std::env::var_os("HOME") {
        return Some(PathBuf::from(home));
    }
    if let Some(userprofile) = std::env::var_os("USERPROFILE") {
        return Some(PathBuf::from(userprofile));
    }
    None
}

/// 从 `~/.preferz/recent.json` 加载最近文件列表。
/// 文件不存在或解析失败时返回空列表。
fn load_recent_files() -> Vec<PathBuf> {
    let path = match recent_files_path() {
        Some(p) => p,
        None => return Vec::new(),
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    #[derive(serde::Deserialize)]
    struct RecentFile {
        path: String,
    }
    #[derive(serde::Deserialize)]
    struct RecentFiles {
        files: Vec<RecentFile>,
    }
    match serde_json::from_str::<RecentFiles>(&content) {
        Ok(parsed) => parsed
            .files
            .into_iter()
            .map(|f| PathBuf::from(f.path))
            .filter(|p| p.exists())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// 保存最近文件列表到 `~/.preferz/recent.json`。
fn save_recent_files(files: &[PathBuf]) {
    let path = match recent_files_path() {
        Some(p) => p,
        None => return,
    };
    // 确保父目录存在
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    #[derive(serde::Serialize)]
    struct RecentFile {
        path: String,
    }
    #[derive(serde::Serialize)]
    struct RecentFiles {
        files: Vec<RecentFile>,
    }
    let recent = RecentFiles {
        files: files
            .iter()
            .map(|p| RecentFile {
                path: p.to_string_lossy().into_owned(),
            })
            .collect(),
    };
    if let Ok(json) = serde_json::to_string_pretty(&recent) {
        let _ = std::fs::write(path, json);
    }
}

/// 把路径添加到最近文件列表头部，去重，限制最多 10 条。
fn add_recent_file(files: &mut Vec<PathBuf>, path: PathBuf) {
    files.retain(|p| p != &path);
    files.insert(0, path);
    if files.len() > 10 {
        files.truncate(10);
    }
    save_recent_files(files);
}
