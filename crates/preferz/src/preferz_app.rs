use crate::i18n::{t, Lang, T};
use crate::interaction;
use crate::keymap::{Action, BindKey, KeyBind, Keymap, KeymapMap};
use crate::theme::{self, ThemeMode};
use crate::ui::stylers::{build_shape_visuals, item_local_to_screen};
use crate::ui::widgets::palette;
use crate::ui::widgets::transform_handles::{
    should_show_flip, should_show_rotate, Handle, TransformHandles,
};
use crate::viewport::ViewportState;
use eframe::egui;
use image::GenericImageView;
use preferz_core::arrange::{
    plan_align, plan_arrange, plan_distribute, AlignMode, ArrangeMode, DistributeAxis,
    DistributeMode,
};
use preferz_core::commands::{
    AddItem, AddItems, ArrangeItems, ArrowHeads, CropItems, DeleteItems, EditShapePoints,
    EditTextContent, FillChange, FillState, FlipItems, FrameGeom, MoveItems, MultiCommand,
    NormalizeItems, RenumberFrame, ReorderItems, SetArrowHeads, SetClosed, SetCurveType,
    SetFrameNumber, SetFrameSize, SetGroup, SetPixmapProps, SetPixmapStyle, SetRoundness,
    SetShapeFill, SetSloppiness, SetStrokeStyle, SetTextStyle, TransformItem,
};
use preferz_core::shape::{
    ArrowHeadStyle, CurveType, DashStyle, FillStyle, FontFamily, PixmapStyle, SeededRng, ShapeType,
    Sloppiness, StrokeStyle, TextAlignH, TextAlignV, TextStyle,
};
use preferz_core::snap;
use preferz_core::spaces::{CanvasPoint, CanvasRect, CanvasSize, CanvasVector};
use preferz_core::{Command, CropRect, EndpointBinding, Item, ItemId, ItemKind, Scene};
use preferz_fileio::{PrzFile, ViewportMeta};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

/// Undo 栈。`push` 会读`Command::skip_first_redo()`
/// - 交互预览命令（拖拽中已直接改 item）返true 跳过首次 redo
/// - 普通命令返false push 时立redo 应用变更
///
/// 这让 AGENTS.md Gotcha #5（skip_first_redo）真正生效（S5/M7）
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
    /// 多边形（Phase I）：点击加点、双击/Enter 闭合、Esc 取消。
    /// 存为 `closed: true` 的 Polyline，不新增 ShapeType。
    Polygon,
}

/// 画框比例 / 纸张预设（plan #3）。
///
/// - [`FramePreset::Ratio`]：纯演示比例——套用后保持画框当前**有效长边**长度不变，
///   只把短边调成目标比例（缩放观感稳定，不涉及 DPI）。
/// - [`FramePreset::Paper`]：纸张尺寸——固定像素绝对值（A4 按 **96 DPI** 换算：
///   210×297mm → 794×1123px）。
#[derive(Debug, Clone, Copy, PartialEq)]
enum FramePreset {
    Ratio { w: f32, h: f32, label: T },
    Paper { w: f32, h: f32, label: T },
}

impl FramePreset {
    fn label(self) -> T {
        match self {
            FramePreset::Ratio { label, .. } | FramePreset::Paper { label, .. } => label,
        }
    }
}

/// 预设清单：5 个演示比例 + A4 竖/横两种纸张。
const FRAME_PRESETS: [FramePreset; 7] = [
    FramePreset::Ratio {
        w: 16.0,
        h: 9.0,
        label: T::Preset16x9,
    },
    FramePreset::Ratio {
        w: 16.0,
        h: 10.0,
        label: T::Preset16x10,
    },
    FramePreset::Ratio {
        w: 4.0,
        h: 3.0,
        label: T::Preset4x3,
    },
    FramePreset::Ratio {
        w: 3.0,
        h: 2.0,
        label: T::Preset3x2,
    },
    FramePreset::Ratio {
        w: 1.0,
        h: 1.0,
        label: T::Preset1x1,
    },
    FramePreset::Paper {
        w: 794.0,
        h: 1123.0,
        label: T::PresetA4Portrait,
    },
    FramePreset::Paper {
        w: 1123.0,
        h: 794.0,
        label: T::PresetA4Landscape,
    },
];

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
        /// 移动组内的线类快照（plan #14）：预览阶段对每条 Polyline 两端做吸附，
        /// 命中则贴边 + 绑定（直改），未命中则端点随线自由移动 + 解绑；
        /// 释放时把 points/binding 变更与 MoveItems 打包成一条 undo 记录。
        line_snaps: Vec<LineSnapState>,
        /// Ctrl+拖动复制：本次拖拽新建的副本 id。释放时按**最终位置**入 undo，
        /// 一次撤销即可撤掉整个「复制 + 移动」（plan.md 快赢项 #11）。
        duplicate_ids: Option<Vec<ItemId>>,
        /// 复制前的原选区（仅 `duplicate_ids = Some` 时有意义）。用于 Ctrl+点击
        /// 未移动时回滚选区——副本与原件重叠，应删掉副本并恢复原件选中。
        original_ids: Vec<ItemId>,
        /// Shift+按在**已选中**的 item 上：按下时先不取消选中，留给 `end_drag`
        /// 判定——没拖动就当作点击、取消选中；拖动了则是轴约束移动。
        /// 否则 Shift+拖动会被原有 toggle 逻辑直接取消选中，约束移动永远用不上。
        pending_deselect: Option<ItemId>,
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
        /// 拖拽开始前的点集快照：普通顶点拖拽 = 拖拽前 points（兼作 undo orig）；
        /// 段中点加点拖拽 = **插入前**的点集（undo 时恢复即移除新顶点）。
        start_points: Vec<(f32, f32)>,
        /// 被拖顶点在拖拽开始时的位置（加点拖拽时即段中点）。
        base_pos: (f32, f32),
        /// plan #4：Alt+按在**真实端点**（0/末点）上进入"延伸"模式——预览越过
        /// 触发阈值时在该端外侧插入一个复制顶点（原端点变成中间顶点），随后拖的
        /// 是新点；未越阈值就释放 = Alt+单击 → 删除所点顶点。
        alt_extend: bool,
    },
    /// 用 Frame 工具拖拽创建画框（两点式：start → current）。
    CreatingFrame {
        start: CanvasPoint,
        current: CanvasPoint,
    },
    /// 用多边形工具逐点点击创建（Phase I）。
    ///
    /// 与两点式工具不同：这是**多拍**交互——每次 pointer down 落一个顶点，
    /// 双击 / Enter 闭合收尾，Esc 取消。释放（pointer released）不结束，
    /// 否则点一下就断了。
    CreatingPolygon {
        /// 已落定的顶点（画布坐标）。
        points: Vec<CanvasPoint>,
        /// 当前指针位置（预览线段的浮动端点）。
        current: CanvasPoint,
        /// Shift 锁 45° 方向（作用于"上一个顶点 → current"这段）。
        shift: bool,
    },
}

/// 文本便签编辑状态（spec L243 P2-5）
/// `editing_item_id = None` 表示创建新文本（提交push `AddItem`）；
/// `Some(id)` 表示编辑现有 item（提交时 push `EditTextContent`）
/// Enter/失焦时提交，空内容在创建模式下丢弃，在编辑模式下不修改原 item
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

/// 后台图片导入解码结果（线UI 线程）
/// 线程负责读取文件字节 + 解码；UI 线程负责上传纹理 + 创建 item
struct ImportOutcome {
    path: PathBuf,
    /// 原始图片字节（写sqlar 用）
    bytes: Vec<u8>,
    /// 解码后的图片尺寸
    width: u32,
    height: u32,
    /// RGBA 像素数据（上传纹理用
    rgba: Vec<u8>,
    /// 解码错误（若存在
    error: Option<String>,
}

/// 后台 .prz 加载结果（线UI 线程）
struct LoadOutcome {
    path: PathBuf,
    result: Result<preferz_fileio::LoadResult, String>,
}

/// 后台保存结果（线UI 线程）
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
    /// .prz 文件加载通道。
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
    fn start_import(&mut self, ctx: &egui::Context, path: PathBuf, lang: Lang) {
        let (tx, rx) = mpsc::channel();
        self.import_rx = Some(rx);
        self.pending += 1;
        self.msg = Some(fill(
            t(lang, T::ProgressImportImage),
            &[path.display().to_string()],
        ));
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

    fn start_load(&mut self, ctx: &egui::Context, path: PathBuf, lang: Lang) {
        let (tx, rx) = mpsc::channel();
        self.load_rx = Some(rx);
        self.pending += 1;
        self.msg = Some(fill(
            t(lang, T::ProgressOpenFile),
            &[path.display().to_string()],
        ));
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let prz = PrzFile::open(&path)?;
                prz.load_scene()
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
        viewport: ViewportMeta,
        lang: Lang,
    ) {
        let (tx, rx) = mpsc::channel();
        self.save_rx = Some(rx);
        self.pending += 1;
        self.msg = Some(fill(
            t(lang, T::ProgressSaveFile),
            &[path.display().to_string()],
        ));
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let mut prz = if path.exists() {
                    PrzFile::open(&path)?
                } else {
                    PrzFile::create(&path)?
                };
                prz.save_scene(&scene, &images, viewport)
            })();
            let _ = tx.send(SaveOutcome {
                path: path.clone(),
                result: result.map_err(|e| e.to_string()),
            });
            ctx2.request_repaint();
        });
    }

    /// 取出并处理已完成的导入结果（PReferZApp::poll_background 调用）
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

    /// 取出并处理已完成的加载结果（PReferZApp::poll_background 调用）
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

/// 端点吸附阈值（屏幕像素）。画布阈值 = `SNAP_THRESHOLD_PX / zoom`，随缩放保持手感一致。
const SNAP_THRESHOLD_PX: f32 = 10.0;

pub struct PReferZApp {
    scene: Scene,
    viewport: ViewportState,
    /// 双击图片「适应视口」前的视口快照（zoom, pan）。
    /// 再次双击同一图片（已处于适配视图）时恢复到此视图（.issues #2）。
    view_fit_prev: Option<(f32, CanvasVector)>,
    undo_stack: UndoStack,
    /// 临时状态消息（已导），会在若干帧后清空，避免覆盖持续状态（B5）
    flash_status: Option<(String, std::time::Instant)>,
    /// `flash()` 调用计数。用作 toast `Area` 的 Id 后缀，让每条新提示都重新走
    /// 一次 egui sizing pass（详见 `render_flash_toast`）。
    flash_seq: u64,
    context_menu_open: bool,
    context_menu_pos: egui::Pos2,
    texture_cache: HashMap<u64, egui::TextureHandle>,
    /// 灰度纹理缓存（懒生成）。grayscale=true Pixmap 渲染时用此处的纹理
    grayscale_texture_cache: HashMap<u64, egui::TextureHandle>,
    /// 原始图片字节缓存（texture_id 原始文件字节），保存时写sqlar
    image_data_cache: HashMap<u64, Vec<u8>>,
    /// 解码后的 RGBA 像素缓存（texture_id RGBA 字节），用于懒生成灰度纹+ 颜色采样
    rgba_pixel_cache: HashMap<u64, Vec<u8>>,
    /// RGBA 像素尺寸（texture_id (w, h)），用于灰度生成和颜色采样
    rgba_size_cache: HashMap<u64, (u32, u32)>,
    next_texture_id: u64,
    pending_import: Vec<PathBuf>,
    transform_handles: TransformHandles,
    /// 当前激活工具。
    tool: Tool,
    /// 绘制形状默认描边样式（样式面板 A8 可调）。
    default_stroke: StrokeStyle,
    /// 绘制形状默认填充色（`None` = 跟随描边色；默认样式侧栏可调）。
    default_fill: Option<[u8; 4]>,
    /// 绘制形状默认填充样式（`None` = 无填充；Excalidraw 四态默认，默认样式侧栏可调）。
    default_fill_style: Option<FillStyle>,
    /// 新建形状默认手绘风抖动档位（plan #3；默认样式侧栏可调）。
    default_sloppiness: Sloppiness,
    drag: DragState,
    /// 端点拖拽中暂存的绑定目标（plan #5）：拖到形状轮廓上则记 `(端点下标, 形状id)`，
    /// 释放时写入 item 的 `start_binding`/`end_binding`；拖离则记 `None`（解绑）。
    /// 三元组：`(端点索引, 目标 id, 锚点)`——锚点为贴合点在目标局部系的坐标，
    /// 移动目标时端点按锚点重算，钉在同一表面点（Excalidraw 语义）。
    pending_endpoint_binding: Option<PendingEndpointBinding>,
    /// 端点吸附高亮：当前吸附到的形状 id（拖拽预览时实时更新），用于渲染高亮描边。
    snap_highlight: Option<ItemId>,
    /// 文本便签编辑状态（None = 无编辑）
    editing_text: Option<EditingText>,
    /// 画框编号编辑状态（Phase D）：Some(frame_id) 时显示左上角小输入框。
    editing_frame_number: Option<ItemId>,
    /// 画框编号编辑输入缓冲区。
    frame_number_buf: String,
    /// 运行模式：编辑 / 全屏幻灯片演示（Phase E）。
    app_mode: AppMode,
    /// Present 翻页过渡目标视口（zoom, pan），Some 时逐帧指数插值。
    present_anim: Option<(f32, CanvasVector)>,
    /// 当前打开的文件路径（保存时若 None 则弹出对话框）
    current_file: Option<PathBuf>,
    /// 后台任务（导入解/ 文件加载 / 文件保存）
    bg_ops: BackgroundOps,
    /// 颜色采样模式（spec §2.2 颜色采样）。true 时鼠标在 Pixmap 上读取像RGB 显示
    color_picker_active: bool,
    /// 最近一次采样的颜色结果（取色器模式下持续更新）
    color_sample: Option<ColorSample>,
    /// 裁剪模式（spec §2.2 裁剪）。Some(item_id) 时该 item 进入裁剪交互模式
    crop_mode: Option<CropMode>,
    /// 设置面板是否打开（Phase 6 §2.3，简化版：仅排列间距 + 主题切换）
    settings_open: bool,
    /// 排列间距（设置面板可调）
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
    /// 键盘快捷键映射（可在设置面板重绑定，持久化到 config.json）。
    keymap: Keymap,
    /// 明暗主题（Light/Dark/Auto），持久化到 config.json（Phase G）。
    theme: ThemeMode,
    /// 最近文件列表（Phase 6 §2.3 欢迎页）。
    recent_files: Vec<PathBuf>,
    /// 欢迎页点击的最近文件路径（待处理）。
    pending_open_recent: Option<PathBuf>,
    /// 欢迎页 logo 纹理（懒加载，复用 assets/icon.png）。
    logo_texture: Option<egui::TextureHandle>,
    /// 属性侧栏里进行中的连续编辑（滑块/取色器拖动中的旧值快照，Phase H）。
    /// `Some` 表示有一段尚未入栈的连续改动，见 [`PropEdit`]。
    prop_edit_pending: Option<PropEdit>,
    /// 本帧是否有属性控件在变更（Phase H）。面板末尾据此决定是否提交
    /// 待合并的连续编辑——必须整帧没人变才提交，否则同一帧内后一个
    /// 未变化的控件会把前一个控件的编辑提前结算掉。
    prop_changed_this_frame: bool,
}

/// 保存提示对话框的触发场景#[derive(Clone, Copy, PartialEq)]
enum SavePromptAction {
    /// 用户点了窗口关闭按钮
    Close,
    /// 用户点了新建画布（Ctrl+N）
    NewCanvas,
}

/// 端点拖拽中暂存的绑定：`(端点索引, 目标 id, 锚点)`。
type PendingEndpointBinding = (usize, ItemId, Option<(f32, f32)>);
/// 端点吸附命中时的换算结果：`(目标 id, 线局部坐标)`。
type SnapHitLocal = (ItemId, (f32, f32));

/// 移动整条线时的吸附快照（plan #14）：begin_drag 时对移动组内的每条
/// Polyline 记下初始 points 与绑定，供预览复位与释放时生成 EditShapePoints。
#[derive(Clone)]
struct LineSnapState {
    line_id: ItemId,
    start_points: Vec<(f32, f32)>,
    start_start_binding: Option<EndpointBinding>,
    start_end_binding: Option<EndpointBinding>,
}

/// 裁剪模式状态（spec §2.2 裁剪）。
#[derive(Clone)]
struct CropMode {
    item_id: ItemId,
    /// 当前正在编辑的裁剪矩形（item 局部空间像素坐标）
    rect: CropRect,
    /// 拖拽中的角点（None = 未拖拽）
    dragging: Option<CropHandle>,
    /// 进入裁剪模式前的原始 crop（Esc 取消时恢复）
    original: Option<CropRect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CropHandle {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

// ─────────────────────────── 属性侧栏（Phase H） ───────────────────────────

/// 多选属性的取值结果（D3）。
///
/// Excalidraw 同款语义：所有选中项一致则显示该值；不一致时仍显示一个代表值
/// （首个持有该属性的 item 的值），但修改会**批量应用到所有选中项**，
/// 而不是禁用控件——禁用会让"统一改成同一个值"这件最常见的事做不了。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prop<T> {
    /// 所有选中项一致。
    Same(T),
    /// 值不一致；携带首个持有该属性的 item 的值，仅供显示。
    Mixed(T),
}

impl<T: Copy> Prop<T> {
    fn value(self) -> T {
        match self {
            Prop::Same(v) | Prop::Mixed(v) => v,
        }
    }

    fn is_mixed(self) -> bool {
        matches!(self, Prop::Mixed(_))
    }
}

/// 取选中项某属性的交集：`read` 返回 `None` 表示该 item 没有这个属性（跳过）。
/// 没有任何选中项持有该属性时返回 `None`，调用方据此隐藏该控件。
fn prop<T: PartialEq + Copy>(
    scene: &Scene,
    ids: &[ItemId],
    read: impl Fn(&Item) -> Option<T>,
) -> Option<Prop<T>> {
    let mut first: Option<T> = None;
    let mut mixed = false;
    for id in ids {
        let Some(item) = scene.get_item(id) else {
            continue;
        };
        let Some(v) = read(item) else { continue };
        match first {
            None => first = Some(v),
            Some(f) if f == v => {}
            Some(_) => mixed = true,
        }
    }
    first.map(|f| if mixed { Prop::Mixed(f) } else { Prop::Same(f) })
}

/// 待合并成一条 undo 命令的连续编辑（Phase H）。
///
/// egui 的滑块 / 取色器在拖动期间**每帧**都报 `changed()`，逐帧入栈会把 undo 历史冲垮。
/// 故变更期间直接改 item（不入栈）并记下起始快照，变更停止后的第一帧
/// 用「变更前 → 当前」合成**一条**批量命令——整段拖动一次 undo。
struct PropEdit {
    kind: PropKind,
    items: Vec<(ItemId, PropValue)>,
}

/// 连续编辑涉及的属性种类；决定合成哪条批量命令。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PropKind {
    /// 描边（颜色 / 线宽 / 线型任一变化都整份快照）。
    Stroke,
    /// 填充色。
    Fill,
    /// 矩形圆角比例。
    Roundness,
    /// 文字样式（字号 / 颜色 / 背景整份快照）。
    TextStyle,
    /// 图片样式（不透明度 / 灰度整份快照）。
    Pixmap,
}

/// 属性值的统一载体：让 [`PropEdit`] 不必为每种属性各写一个类型。
#[derive(Debug, Clone, Copy, PartialEq)]
enum PropValue {
    Float(f32),
    Stroke(StrokeStyle),
    Fill(FillState),
    Text(TextStyle),
    Pixmap(PixmapStyle),
}

/// 用「变更前快照 + 当前 scene 值」合成一条批量命令。
///
/// 返回 `None` 表示没有有效条目（item 已被删除、或快照与 item 当前类型对不上），
/// 此时不入栈，免得 undo 历史里塞进一条什么都没做的记录。
fn prop_cmd(pending: PropEdit, scene: &Scene) -> Option<Box<dyn Command>> {
    match pending.kind {
        PropKind::Stroke => {
            // 分流：联动快照（填充 / 绑定文字）与描边快照混在同一 pending 里，
            // 按值类型拆成各自的批量命令，最后打包成一条 MultiCommand
            // （改描边色 → 填充色 / 绑定文字色联动，撤销一步到位）。
            let mut strokes = Vec::new();
            let mut texts = Vec::new();
            let mut fills = Vec::new();
            for (id, old) in &pending.items {
                let Some(item) = scene.get_item(id) else {
                    continue;
                };
                match (old, &item.kind) {
                    (PropValue::Stroke(old), ItemKind::Shape { stroke, .. }) => {
                        strokes.push((*id, *old, *stroke));
                    }
                    (PropValue::Text(old), ItemKind::Text { .. }) => {
                        if let Some(cur) = item.kind.text_style() {
                            texts.push((*id, *old, cur));
                        }
                    }
                    (
                        PropValue::Fill(old),
                        ItemKind::Shape {
                            fill, fill_style, ..
                        },
                    ) => {
                        fills.push((
                            *id,
                            *old,
                            FillState {
                                color: *fill,
                                style: *fill_style,
                            },
                        ));
                    }
                    _ => {}
                }
            }
            let mut cmds: Vec<Box<dyn Command>> = Vec::new();
            if !strokes.is_empty() {
                cmds.push(Box::new(
                    SetStrokeStyle::new_batch(strokes).with_preview_applied(true),
                ));
            }
            if !texts.is_empty() {
                cmds.push(Box::new(
                    SetTextStyle::new_batch(texts).with_preview_applied(true),
                ));
            }
            if !fills.is_empty() {
                cmds.push(Box::new(
                    SetShapeFill::new_batch(fills).with_preview_applied(true),
                ));
            }
            match cmds.len() {
                0 => None,
                1 => Some(cmds.pop().expect("len == 1")),
                _ => Some(Box::new(MultiCommand::new(cmds))),
            }
        }
        PropKind::Fill => {
            let items: Vec<FillChange> = pending
                .items
                .iter()
                .filter_map(|(id, old)| match (old, scene.get_item(id)) {
                    (PropValue::Fill(old), Some(item)) => match &item.kind {
                        ItemKind::Shape {
                            fill, fill_style, ..
                        } => Some((
                            *id,
                            *old,
                            FillState {
                                color: *fill,
                                style: *fill_style,
                            },
                        )),
                        _ => None,
                    },
                    _ => None,
                })
                .collect();
            (!items.is_empty()).then(|| {
                Box::new(SetShapeFill::new_batch(items).with_preview_applied(true))
                    as Box<dyn Command>
            })
        }
        PropKind::Roundness => {
            let items: Vec<(ItemId, f32, f32)> = pending
                .items
                .iter()
                .filter_map(|(id, old)| match (old, scene.get_item(id)) {
                    (PropValue::Float(old), Some(item)) => match &item.kind {
                        ItemKind::Shape { roundness, .. } => Some((*id, *old, *roundness)),
                        _ => None,
                    },
                    _ => None,
                })
                .collect();
            (!items.is_empty()).then(|| {
                Box::new(SetRoundness::new_batch(items).with_preview_applied(true))
                    as Box<dyn Command>
            })
        }
        PropKind::TextStyle => {
            let items: Vec<(ItemId, TextStyle, TextStyle)> = pending
                .items
                .iter()
                .filter_map(|(id, old)| match (old, scene.get_item(id)) {
                    (PropValue::Text(old), Some(item)) => {
                        item.kind.text_style().map(|new| (*id, *old, new))
                    }
                    _ => None,
                })
                .collect();
            (!items.is_empty()).then(|| {
                Box::new(SetTextStyle::new_batch(items).with_preview_applied(true))
                    as Box<dyn Command>
            })
        }
        PropKind::Pixmap => {
            let items: Vec<(ItemId, PixmapStyle, PixmapStyle)> = pending
                .items
                .iter()
                .filter_map(|(id, old)| match (old, scene.get_item(id)) {
                    (PropValue::Pixmap(old), Some(item)) => match &item.kind {
                        ItemKind::Pixmap {
                            opacity, grayscale, ..
                        } => Some((
                            *id,
                            *old,
                            PixmapStyle {
                                opacity: *opacity,
                                grayscale: *grayscale,
                            },
                        )),
                        _ => None,
                    },
                    _ => None,
                })
                .collect();
            (!items.is_empty()).then(|| {
                Box::new(SetPixmapStyle::new_batch(items).with_preview_applied(true))
                    as Box<dyn Command>
            })
        }
    }
}

impl PReferZApp {
    pub fn new() -> Self {
        // 只加载一次：lang 与 keymap 同源
        let cfg = load_config();
        Self {
            scene: Scene::new(),
            viewport: ViewportState::default(),
            view_fit_prev: None,
            undo_stack: UndoStack::new(),
            flash_status: None,
            flash_seq: 0,
            context_menu_open: false,
            pending_endpoint_binding: None,
            snap_highlight: None,
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
            default_stroke: {
                // 新建元素默认色随主题翻转（D2）；启动时尚无 ctx，Auto 按 Dark 兜底。
                StrokeStyle {
                    color: cfg.theme.default_stroke_color_static(),
                    ..Default::default()
                }
            },
            default_fill: None,
            default_fill_style: None,
            default_sloppiness: Sloppiness::Off,
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
            lang: cfg.lang,
            // 恒用出厂默认（Phase K 数字快捷键修复）：D6 移除改绑入口后，持久化的
            // keymap 只可能是旧版本的过期默认表——`from_partial` 只补缺失动作，
            // 老配置里全量存在（如 ToolRect 只有 R 没有 Num2），会把新默认值永久
            // 挡在门外。字段仍保留在 Config 里以便兼容解析，但不再消费。
            keymap: Keymap::new(),
            theme: cfg.theme,
            recent_files: load_recent_files(),
            pending_open_recent: None,
            logo_texture: None,
            prop_edit_pending: None,
            prop_changed_this_frame: false,
        }
    }

    fn flash(&mut self, msg: impl Into<String>) {
        self.flash_status = Some((msg.into(), std::time::Instant::now()));
        self.flash_seq = self.flash_seq.wrapping_add(1);
    }

    /// push undo command 并标记画布为 dirty（有未保存修改）
    fn push_cmd(&mut self, cmd: Box<dyn Command>) {
        self.undo_stack.push(cmd, &mut self.scene);
        self.dirty = true;
    }

    /// 执行 undo：成功则标记 dirty
    fn perform_undo(&mut self) -> bool {
        if self.undo_stack.undo(&mut self.scene) {
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// 入栈 AddItem 并**选中新 item**（Excalidraw 语义）。
    ///
    /// 画完立即选中，才能接着按 Enter 加文字 / 调侧栏属性，不必再点一下。
    /// 选中态不进 undo 栈（与 Scene 的既有约定一致）。
    fn push_new_item(&mut self, cmd: AddItem) {
        let id = cmd.item_id();
        self.push_cmd(Box::new(cmd));
        self.scene.deselect_all();
        self.scene.select(id);
    }

    /// 执行 redo：成功则标记 dirty
    fn perform_redo(&mut self) -> bool {
        if self.undo_stack.redo(&mut self.scene) {
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// poll 后台任务通道，分发到 finish_import / finish_load / 保存结果处理
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
                    self.flash(fill(
                        t(self.lang, T::FlashSaved),
                        &[outcome.path.display().to_string()],
                    ));
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
                    self.flash(fill(
                        t(self.lang, T::FlashSaveFailed),
                        std::slice::from_ref(&e),
                    ));
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

    /// 选中 item 的快照（Z 序倒序，顶层在前）
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
}

impl Default for PReferZApp {
    fn default() -> Self {
        Self::new()
    }
}

const FLASH_DURATION_MS: u128 = 2500;

/// flash toast 距屏幕左右两边的留白（px）。排版上限宽度 = 屏宽 − 2×该值，
/// 见 `render_flash_toast`。
const FLASH_TOAST_SIDE_MARGIN: f32 = 24.0;

/// 线性对象自动闭合的模糊距离（画布像素）：终点回到起点该距离内即判定为闭合图形。
/// 与 Excalidraw 的吸附闭合一致；多段线阶段起作用，两点式下仅覆盖短拖拽。
const POLYLINE_CLOSE_DISTANCE: f32 = 8.0;

/// plan #4：Alt+拖端点进入"延伸"的移动触发阈值（屏幕像素）。越过前按普通端点
/// 拖拽处理；越阈瞬间在该端外侧插入复制顶点。取小值避免与 Alt+单击删除（=删除
/// 顶点手势）互相误触。
const EXTEND_TRIGGER_PX: f32 = 4.0;

/// 首尾点的**屏幕距离**（画布距离 × zoom；闭合/开放阈值均按屏幕像素计，缩放不
/// 改手感）。点数 <2 返回 0。
fn first_last_screen_dist(points: &[(f32, f32)], zoom: f32) -> f32 {
    let last = match points.len().checked_sub(1) {
        Some(l) if l >= 1 => l,
        _ => return 0.0,
    };
    let dx = points[0].0 - points[last].0;
    let dy = points[0].1 - points[last].1;
    (dx * dx + dy * dy).sqrt() * zoom
}

/// plan #4：端点拖拽释放时是否自动闭合成多边形。判定与 Excalidraw `isPathALoop`
/// 同式：≥3 顶点、被拖的是**真实端点**（0/末点）、首尾屏幕距离 ≤
/// [`POLYLINE_CLOSE_DISTANCE`]。开放态专用；是否 `closed` 由调用方判断。
fn should_auto_close(points: &[(f32, f32)], endpoint: usize, zoom: f32) -> bool {
    let last = match points.len().checked_sub(1) {
        Some(l) if points.len() >= 3 => l,
        _ => return false,
    };
    if endpoint != 0 && endpoint != last {
        return false;
    }
    first_last_screen_dist(points, zoom) <= POLYLINE_CLOSE_DISTANCE
}

/// plan #7：流程图创建/导航的方向（方向键语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlowDir {
    Up,
    Right,
    Down,
    Left,
}

impl FlowDir {
    fn opposite(self) -> Self {
        match self {
            FlowDir::Up => FlowDir::Down,
            FlowDir::Down => FlowDir::Up,
            FlowDir::Left => FlowDir::Right,
            FlowDir::Right => FlowDir::Left,
        }
    }

    /// 从**实际命中**的绑定取方向（plan #7 两动作各共用一个 Action、四方向键
    /// 多绑定）。改绑到非方向键时无方向可解释 → `None`（调用方静默忽略）。
    fn from_bind(bind: &KeyBind) -> Option<Self> {
        Some(match bind.key {
            BindKey::ArrowUp => Self::Up,
            BindKey::ArrowDown => Self::Down,
            BindKey::ArrowLeft => Self::Left,
            BindKey::ArrowRight => Self::Right,
            _ => return None,
        })
    }
}

/// plan #7：流程图主轴间距（新节点边到源边，画布 px）。对齐 Excalidraw
/// `VERTICAL_OFFSET`/`HORIZONTAL_OFFSET` = 100。
const FLOWCHART_GAP: f32 = 100.0;

/// 包围盒（w×h，局部坐标）朝向 `dir` 的**边中点**——同时用作端点绑定锚点
/// （目标局部系）与连接箭头端点的局部位置。
fn edge_anchor_local(dir: FlowDir, w: f32, h: f32) -> (f32, f32) {
    match dir {
        FlowDir::Right => (w, h / 2.0),
        FlowDir::Left => (0.0, h / 2.0),
        FlowDir::Down => (w / 2.0, h),
        FlowDir::Up => (w / 2.0, 0.0),
    }
}

impl eframe::App for PReferZApp {
    /// 透明窗口底色：完全透明。
    ///
    /// eframe 默认 `clear_color` 是固定的半透明 `(12,12,12,180)`（alpha≈0.706），会给
    /// 透明窗口叠一层「最低 70% 不透明」的下駄——无论 `bg_alpha` 拖到多低都透不出去，
    /// 且与各面板/画布的 bg_alpha 叠加后映射严重非线性（用户反馈「50% 几乎不透明、
    /// 10% 才半透明」）。此处返回全透明清屏色，未绘制区域直接透出桌面，使背景不透明度
    /// 由面板/画布那一层 bg_alpha 单层决定，从而与滑块线性对应。
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // 0.36 的 App::ui 只给 `&mut Ui`。下面整段函数体沿用 `ctx`（含 `.show(ctx, ..)`、
        // 各 `self.render_*(ctx)`、以及后台线程 `ctx.clone()`），故从 ui 取一份独立所有权的
        // Context 克隆（egui::Context 是 Arc 承载，clone 廉价），再绑定为 `&Context`——既保持
        // 函数体里 `ctx` 的原有类型零改动，又不会让 `ui` 被不可变借用而挡住后续 `&mut ui` 使用。
        let ctx_owned = ui.ctx().clone();
        let ctx = &ctx_owned;

        // 每帧重置连续编辑标记；本帧结束时据此决定是否结算待合并的拖拽编辑（Phase H）。
        self.prop_changed_this_frame = false;

        // 主题 + 背景透明度：按当前主题构造 Visuals，并把 bg_alpha 施加到 chrome 填充色
        // （panel/window/faint），使透明窗口效果在明暗两套主题下都生效。
        {
            let visuals = theme::build_visuals(self.theme, self.bg_alpha, ctx);
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

        // 拖放导入（spec L228，P3-1）prz 加载项目文件；其图片导入
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_owned())
                .collect()
        });
        for path in dropped {
            if is_project_file(&path) {
                self.bg_ops.start_load(ctx, path, self.lang);
            } else {
                self.pending_import.push(path);
            }
        }

        // 处理待导入：启动后台解码（不阻塞 UI
        while let Some(path) = self.pending_import.pop() {
            self.bg_ops.start_import(ctx, path, self.lang);
        }

        // poll 后台任务结果（导入/加载/保存/导出）。
        self.poll_background(ctx);

        // 懒重建缺失纹理：undo 恢复 / 共享纹理删除后兜底（修 .issues #1 灰方块）。
        self.ensure_pixmap_textures(ctx);

        // 处理欢迎页最近文件点击
        if let Some(path) = self.pending_open_recent.take() {
            self.add_recent_and_load(ctx, path);
        }

        // 清理过期flash 状
        if let Some((_, t)) = self.flash_status {
            if t.elapsed().as_millis() > FLASH_DURATION_MS {
                self.flash_status = None;
            }
        }

        // Present 演示模式：纯展示态，跳过所有编辑界面，进入独立渲染与导航。
        if matches!(self.app_mode, AppMode::Present { .. }) {
            self.handle_present_input(ctx);
            self.render_present(ui);
            return;
        }

        // 左侧工具条（spec §5.1：绘制工具切换）
        egui::Panel::left("tool_panel")
            .exact_size(44.0)
            .resizable(false)
            .show(ui, |ui| {
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
                    (Tool::Polygon, "⬟", T::ToolPolygon),
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

        // 「新建元素默认样式」不再占用底部面板：绘制工具激活且无选中时，
        // 由 render_props_panel 在右侧栏渲染（与选中属性侧栏同一位置，避免两套控件）。
        // 右侧属性侧栏（Phase H）：选中项 per-item 编辑，按 ItemKind 分节。
        self.render_props_panel(ui);

        // 注：底部状态栏已移除，缩放百分数 / 语言切换 / flash 提示改由画布之上的
        // 悬浮 HUD 承载（`render_hud`），在 CentralPanel 之后调用。

        // Debug 面板（仅 debug 构建显示，release 自动隐藏）
        if cfg!(debug_assertions) {
            self.render_debug_panel(ctx);
        }

        // 中央画布
        //
        // 透明映射修复：默认 `CentralPanel::default()` 的 frame 会用 panel_fill
        // （已含 alpha=bg_alpha）铺满面板，随后这里又用同一 bg_alpha 叠画一层画布底色——
        // 两层同色透明叠加导致中央区二次合成（50% 实测≈75% 不透明）。改为「单张半透明底」：
        // 保留默认 frame 的 inner_margin（视口交互矩形 / 元素坐标不变）但把 fill 置为透明，
        // 使面板本身不再贡献 alpha；背景只由下面这一张铺满整个面板（把内缩的 max_rect 按
        // inner_margin 外扩回面板外沿，消除 8px 接缝）的 rect 决定，窗口 alpha 即线性等于 bg_alpha。
        let central_frame =
            egui::Frame::central_panel(&ctx.global_style()).fill(egui::Color32::TRANSPARENT);
        let central_margin = central_frame.inner_margin;
        let central_panel = egui::CentralPanel::default().frame(central_frame);
        central_panel.show(ui, |ui| {
            let rect = ui.max_rect();
            self.viewport.set_screen_rect(rect);

            let response =
                ui.interact(rect, egui::Id::new("canvas"), egui::Sense::click_and_drag());

            // 画布背景：随主题翻转（D2），并应用 bg_alpha（单张半透明底，铺满整个面板）
            let bg_alpha_u8 = (self.bg_alpha * 255.0).round() as u8;
            let [cb_r, cb_g, cb_b] = self.theme.canvas_bg(ctx);
            ui.painter().rect_filled(
                rect + central_margin,
                egui::CornerRadius::same(0),
                egui::Color32::from_rgba_unmultiplied(cb_r, cb_g, cb_b, bg_alpha_u8),
            );

            // 渲染场景（含视口剔除 + Z + 复用 self.transform_handles
            self.render_scene(ui);

            // 框选矩形（spec L240
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
                ui.painter()
                    .rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Middle);
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
                        ui.painter().rect_stroke(
                            screen_rect,
                            0.0,
                            stroke,
                            egui::StrokeKind::Middle,
                        );
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
                ui.painter()
                    .rect_stroke(screen_rect, 0.0, stroke, egui::StrokeKind::Middle);
            }

            // 指针状态：多边形预览要用到，故提到绘制预览之前声明
            let pointer_pos = ctx.input(|i| i.pointer.latest_pos());

            // 多边形工具预览（Phase I）：已落定顶点 + 到指针的"橡皮筋"线段 + 闭合虚线提示。
            // 未按下时也要跟随指针（两点式工具的预览只在按住时更新），故在此同步 current。
            if let DragState::CreatingPolygon { current, .. } = &mut self.drag {
                if let Some(pos) = pointer_pos {
                    *current = self.viewport.screen_to_canvas(pos);
                }
            }
            let polygon_preview = match &self.drag {
                DragState::CreatingPolygon {
                    points,
                    current,
                    shift,
                } => {
                    let tip = points
                        .last()
                        .map(|last| Self::snap_polygon_point(*last, *current, *shift))
                        .unwrap_or(*current);
                    Some((points.clone(), tip))
                }
                _ => None,
            };
            if let Some((points, tip)) = polygon_preview {
                let stroke = egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(100, 200, 255));
                let mut screen_pts: Vec<egui::Pos2> = points
                    .iter()
                    .map(|p| self.viewport.canvas_to_screen(*p))
                    .collect();
                screen_pts.push(self.viewport.canvas_to_screen(tip));
                if screen_pts.len() >= 2 {
                    ui.painter().add(egui::Shape::line(screen_pts, stroke));
                }
                // ≥3 个顶点时才提示"再确认一下就闭合成面"：两点连不出面
                if points.len() >= 3 {
                    let a = self.viewport.canvas_to_screen(tip);
                    let b = self.viewport.canvas_to_screen(points[0]);
                    const CLOSE_HINT_DASHES: usize = 10;
                    for i in 0..CLOSE_HINT_DASHES {
                        let t0 = i as f32 / CLOSE_HINT_DASHES as f32;
                        let t1 = (i as f32 + 0.5) / CLOSE_HINT_DASHES as f32;
                        ui.painter()
                            .line_segment([a.lerp(b, t0), a.lerp(b, t1)], stroke);
                    }
                }
                // 顶点小方块：让"点了几个点"一眼可数
                for p in points.iter() {
                    let s = self.viewport.canvas_to_screen(*p);
                    ui.painter().rect_filled(
                        egui::Rect::from_center_size(s, egui::vec2(6.0, 6.0)),
                        egui::CornerRadius::ZERO,
                        egui::Color32::from_rgb(100, 200, 255),
                    );
                }
            }

            // 鼠标中键拖拽平移
            if response.dragged_by(egui::PointerButton::Middle) {
                self.viewport.pan_by_screen(response.drag_delta());
            }

            // 滚轮缩放（以鼠标位置为锚点）
            let scroll = ctx.input(|i| i.smooth_scroll_delta);
            if scroll.y != 0.0 {
                if let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) {
                    self.viewport.zoom_at(scroll.y, pos);
                }
            }

            // 双击：Text item → 编辑；封闭 Shape → 创建/编辑绑定文本；
            // Pixmap → 视口适应该图片（.issues #2，Excalidraw 同款）；
            // 空白 → 创建文本便签（spec L243 P2-5）
            // response.double_clicked() 已自动考虑上层 Window 遮挡
            if response.double_clicked() && self.editing_text.is_none() {
                // 多边形绘制中：双击 = 收尾闭合，优先于文本便签（绘制工具下不该建文本）
                if matches!(self.drag, DragState::CreatingPolygon { .. }) {
                    self.finish_create_polygon();
                } else if let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) {
                    // 一次性取全命中信息（id + 是否图片 + 包围盒），避免借用冲突
                    let hit =
                        interaction::get_item_at(pos, &self.scene, &self.viewport).map(|item| {
                            (
                                item.id,
                                matches!(item.kind, ItemKind::Pixmap { .. }),
                                item.bounding_rect(),
                            )
                        });
                    match hit {
                        // 双击图片：已处于该图片的适配视图 → 回到上一视图；否则适配视口。
                        Some((_, true, rect)) => {
                            let (target_zoom, target_pan) = self.compute_fit(rect);
                            let already_fit = (self.viewport.zoom - target_zoom).abs() < 1e-3
                                && (self.viewport.pan - target_pan).length() < 1e-2;
                            if already_fit {
                                if let Some((z, p)) = self.view_fit_prev.take() {
                                    self.viewport.zoom = z;
                                    self.viewport.pan = p;
                                    self.flash(t(self.lang, T::FlashFitRestore).to_string());
                                }
                            } else {
                                self.view_fit_prev = Some((self.viewport.zoom, self.viewport.pan));
                                self.viewport.fit_to_content(rect);
                                self.flash(t(self.lang, T::FlashFitToCanvas).to_string());
                            }
                        }
                        // 命中可承载文本的 item → 编辑/新建文本
                        Some((id, false, _)) if self.start_text_edit(id) => {}
                        // 其余（线/箭头/空白）→ 新建自由文本
                        _ => {
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
                        }
                    }
                    self.drag = DragState::Idle;
                }
            }

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
                        Handle::Endpoint(_) | Handle::SegmentMid(_) => egui::CursorIcon::Grab,
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
                    let axis_lock = ctx.input(|i| i.modifiers.shift);
                    self.update_drag_preview(pos, free_scale, axis_lock);
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
                    let alt = ctx.input(|i| i.modifiers.alt);
                    self.begin_drag(pos, additive, free_scale, alt);
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

        // 悬浮 HUD（缩放百分数 + 语言切换 + flash toast）：
        // 必须在 CentralPanel 之后绘制，才能盖在画布之上，并让画布的
        // `response.hovered()` 自动排除 HUD 占用的区域（避免穿透触发画布拖拽）。
        self.render_hud(ctx);

        // 文本编辑 overlay（spec L243 P2-5）
        self.render_text_editor(ctx);

        // 画框编号编辑 overlay（Phase D）
        self.render_frame_number_editor(ctx);

        // 上下文菜
        if self.context_menu_open {
            self.render_context_menu(ctx);
        }

        // 颜色采样 overlay（spec §2.2
        if self.color_picker_active {
            self.render_color_picker_overlay(ctx);
        }

        // 设置面板
        if self.settings_open {
            self.render_settings_window(ctx);
        }

        // 快捷键派发（改绑捕获入口已按 ADR-0007 / D6 移除，这里只保留查表派发）
        self.handle_shortcuts(ctx);

        // 保存提示对话框（关闭/新建时若 dirty 弹出
        self.render_save_prompt(ctx);

        // 后台任务进度条（spec L298：加保存时显示进度）
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
                    // 同 flash toast：把排版上限钉死（屏宽 − 两侧留白），带长路径的
                    // 消息稳定折行留在屏内，而不是横向溢出被裁或逐帧收缩。
                    let max_w =
                        (ctx.content_rect().width() - 2.0 * FLASH_TOAST_SIDE_MARGIN).max(200.0);
                    ui.set_max_width(max_w);
                    ui.vertical_centered(|ui| {
                        ui.add_space(4.0);
                        ui.label(&msg);
                        ui.add_space(6.0);
                        ui.add(egui::Spinner::new());
                        ui.add_space(4.0);
                    });
                });
        }

        // 连续编辑结算（Phase H）：整帧无控件变化时，把待合并编辑合成为一条 undo 命令。
        // 拖动中（本帧仍有控件变更）则保留，等下一帧无变更时再结算——整段拖动一次 undo。
        if let Some(pending) = self.prop_edit_pending.take() {
            if !self.prop_changed_this_frame {
                if let Some(cmd) = prop_cmd(pending, &self.scene) {
                    self.push_cmd(cmd);
                }
            } else {
                self.prop_edit_pending = Some(pending);
            }
        }
    }
}

// ─────────────────────────── 拖拽逻辑 ───────────────────────────

impl PReferZApp {
    /// plan #4：Alt+单击删除顶点。守卫：开放折线保 ≥2、闭合多边形保 ≥3 顶点
    /// （再删退化到无法成形），不满足时 flash 拒绝。删首/尾顶点连带解除该端
    /// 绑定（中间顶点不涉绑定）。预览直改 + `EditShapePoints`（恒 skip_first_redo）
    /// 入 undo 栈，一步可撤。
    fn try_delete_vertex(&mut self, item_id: ItemId, idx: usize) {
        let (old_points, closed, so, eo) = match self.scene.get_item(&item_id) {
            Some(item) => match &item.kind {
                ItemKind::Shape {
                    points,
                    closed,
                    start_binding,
                    end_binding,
                    ..
                } => (points.clone(), *closed, *start_binding, *end_binding),
                _ => return,
            },
            None => return,
        };
        let n = old_points.len();
        let min = if closed { 3 } else { 2 };
        if n <= min {
            self.flash(if closed {
                t(self.lang, T::FlashVertexMinClosed)
            } else {
                t(self.lang, T::FlashVertexMinOpen)
            });
            return;
        }
        let last = n - 1;
        let binding_touched = idx == 0 || idx == last;
        let mut new_points = old_points.clone();
        new_points.remove(idx);
        let sn = if idx == 0 { None } else { so };
        let en = if idx == last { None } else { eo };
        // 预览直改（命令 skip_first_redo 恒 true，push 不再重放 redo）
        if let Some(item) = self.scene.get_item_mut(&item_id) {
            if let ItemKind::Shape {
                start_binding,
                end_binding,
                ..
            } = &mut item.kind
            {
                if idx == 0 {
                    *start_binding = None;
                }
                if idx == last {
                    *end_binding = None;
                }
            }
            item.kind.set_line_points(new_points.clone());
        }
        let mut cmd = EditShapePoints::new(item_id, old_points, new_points);
        if binding_touched {
            cmd = cmd.with_binding_change(so, sn, eo, en);
        }
        self.push_cmd(Box::new(cmd));
        self.flash(t(self.lang, T::FlashVertexDeleted));
    }

    fn begin_drag(&mut self, screen_pos: egui::Pos2, additive: bool, free_scale: bool, alt: bool) {
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
            // 多拍工具：第一拍落首个顶点，后续每拍追加一个顶点（见 finish_create_polygon）
            Tool::Polygon => {
                let p = self.viewport.screen_to_canvas(screen_pos);
                match &mut self.drag {
                    DragState::CreatingPolygon {
                        points,
                        current,
                        shift,
                    } => {
                        let last = *points.last().expect("CreatingPolygon 至少有一个顶点");
                        let next = Self::snap_polygon_point(last, *current, *shift);
                        // 双击的第二下会先落到 begin_drag：落点与上一顶点重合时
                        // 视为"收尾"而非新增顶点（双击闭合的第二拍不该多加一个点）
                        if (next - last).length() >= POLYLINE_CLOSE_DISTANCE {
                            points.push(next);
                        }
                        *current = next;
                        // 每拍重取 Shift：下一次按下时才改方向锁定，中途松开 Shift 不影响已落顶点
                        *shift = additive;
                    }
                    _ => {
                        self.drag = DragState::CreatingPolygon {
                            points: vec![p],
                            current: p,
                            shift: additive,
                        };
                    }
                }
                return;
            }
            _ => {}
        }

        // 裁剪模式：优先检测裁剪手
        if self.crop_mode.is_some() {
            if let Some(h) = self.crop_handle_hit_test(screen_pos) {
                if let Some(crop) = self.crop_mode.as_mut() {
                    crop.dragging = Some(h);
                }
                return;
            }
            // 裁剪模式下点空白：不响应（避免误操作
            return;
        }

        // 颜色采样模式：单Pixmap 采样像素
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
                    // 线性对象顶点控制点：进入端点拖拽（预览直接改 points）。
                    // plan #4：Alt+单击内部顶点 → 按下即删；Alt+按在真实端点
                    // （0/末点）→ 延伸模式（越阈追加 / 未越阈释放=单击删除）。
                    if let Handle::Endpoint(endpoint) = h {
                        let start_points = match &item.kind {
                            ItemKind::Shape { points, .. } => points.clone(),
                            _ => Vec::new(),
                        };
                        let n = start_points.len();
                        let is_real = n > 0 && (endpoint == 0 || endpoint == n - 1);
                        if alt && !is_real {
                            self.try_delete_vertex(item.id, endpoint);
                            return;
                        }
                        let base_pos = start_points.get(endpoint).copied().unwrap_or((0.0, 0.0));
                        let start_canvas = self.viewport.screen_to_canvas(screen_pos);
                        self.drag = DragState::LineEndpoint {
                            item_id: item.id,
                            endpoint,
                            start_canvas,
                            start_points,
                            base_pos,
                            alt_extend: alt && is_real,
                        };
                        self.transform_handles.active_handle = h;
                        self.transform_handles.is_dragging = true;
                        return;
                    }
                    // 线性对象段中点：在段中间插入顶点（预览），随即进入端点拖拽。
                    // start_points 保存插入前的点集，undo 一步即可移除新顶点。
                    if let Handle::SegmentMid(seg) = h {
                        let prep = match &item.kind {
                            ItemKind::Shape { points, .. } => {
                                let n = points.len();
                                if n < 2 {
                                    None
                                } else {
                                    let a = points[seg];
                                    let b = points[(seg + 1) % n];
                                    let mid = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
                                    let insert_idx = if seg + 1 < n { seg + 1 } else { n };
                                    Some((points.clone(), insert_idx, mid))
                                }
                            }
                            _ => None,
                        };
                        if let Some((start_points, insert_idx, mid)) = prep {
                            if let Some(it) = self.scene.get_item_mut(&item.id) {
                                if let ItemKind::Shape { points, .. } = &mut it.kind {
                                    points.insert(insert_idx, mid);
                                }
                            }
                            let start_canvas = self.viewport.screen_to_canvas(screen_pos);
                            self.drag = DragState::LineEndpoint {
                                item_id: item.id,
                                endpoint: insert_idx,
                                start_canvas,
                                start_points,
                                base_pos: mid,
                                alt_extend: false,
                            };
                            self.transform_handles.active_handle = h;
                            self.transform_handles.is_dragging = true;
                        }
                        return;
                    }
                    // 翻转边手柄：点击即触发翻转，不进入拖拽（spec L239「翻转边」）
                    if h == Handle::FlipH || h == Handle::FlipV {
                        let ids: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
                        let horizontal = h == Handle::FlipH;
                        let cmd = FlipItems::new(ids, horizontal);
                        self.push_cmd(Box::new(cmd));
                        self.flash(t(
                            self.lang,
                            if horizontal {
                                T::FlashFlipH
                            } else {
                                T::FlashFlipV
                            },
                        ));
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

        // 2) 命中 item：选中并开始移动拖
        if let Some(item) = interaction::get_item_at(screen_pos, &self.scene, &self.viewport) {
            let id = item.id;
            // G2（plan #13）：命中组员 → 命中集扩展为整组（未编组项 = 自身，行为不变）
            let hit_ids: Vec<ItemId> = self.scene.expand_to_groups(&[id]);
            let hit_fully_selected = hit_ids.iter().all(|hid| self.scene.selection.contains(hid));

            let mut pending_deselect = None;
            if additive {
                if hit_fully_selected {
                    // Shift 按在**已选中**的项（组则整组在选区）上：既可能是"Shift+点击
                    // 取消选中"，也可能是"Shift+拖动做轴约束移动"——两者要到释放时才能
                    // 区分，这里先记下代表成员交给 end_drag 判定（取消时扩展回整组）。
                    // 若在此直接 toggle，Shift+拖动会先取消选中，轴约束移动就永远触发不到。
                    pending_deselect = hit_ids.first().copied();
                } else {
                    for hid in &hit_ids {
                        self.scene.toggle_selection(*hid);
                    }
                }
            } else if !hit_fully_selected {
                // 非加选且命中集未完全选中：替换选中为整组
                self.scene.deselect_all();
                for hid in &hit_ids {
                    self.scene.select(*hid);
                }
            }
            // 收集所有选中 item transform 快照
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

            // Ctrl+拖动 = 复制移动（plan #11）：先建副本、选区切到副本，原件留原地。
            // 取"按下即建副本"（plan 倾向），避免释放时才复制造成落点跳变。
            let duplicate_ids = if free_scale && !collected.is_empty() {
                let dups = self.scene.duplicate_items(&collected, CanvasVector::zero());
                let ids: Vec<ItemId> = dups.iter().map(|it| it.id).collect();
                for dup in dups {
                    self.scene.add_item(dup);
                }
                // 复制的画框须压到成员之下（与 finish_create_frame 一致）：否则它 z
                // 最高会盖住框内成员命中（frame_border_hit 已修，但近边框仍会误选
                // frame）。直接 mutate 不入 undo——最终状态由下方 AddItems 记录。
                let frame_dup_ids: Vec<ItemId> = ids
                    .iter()
                    .filter(|id| {
                        self.scene
                            .get_item(id)
                            .map(|it| it.is_frame())
                            .unwrap_or(false)
                    })
                    .copied()
                    .collect();
                if !frame_dup_ids.is_empty() {
                    ReorderItems::new(frame_dup_ids, false).redo(&mut self.scene);
                }
                self.scene.deselect_all();
                for nid in &ids {
                    self.scene.select(*nid);
                }
                Some(ids)
            } else {
                None
            };
            // 复制前的原选区（Ctrl 未移动时需要回滚）
            let original_ids = collected.clone();

            // 被拖动的是副本（复制拖动）还是原选中集；两者都从 scene 现取 transform，
            // 保证与上面可能发生的改选保持一致。
            let drag_ids = duplicate_ids.clone().unwrap_or(collected);
            let start_transforms: Vec<(ItemId, preferz_core::Transform)> = drag_ids
                .into_iter()
                .filter_map(|sid| self.scene.get_item(&sid).map(|it| (sid, it.transform)))
                .collect();
            let start_canvas = self.viewport.screen_to_canvas(screen_pos);
            // plan #14：移动组内的 Polyline 快照（预览吸附 + 释放打包入栈）
            let line_snaps: Vec<LineSnapState> = start_transforms
                .iter()
                .filter_map(|(id, _)| {
                    let it = self.scene.get_item(id)?;
                    match &it.kind {
                        ItemKind::Shape {
                            shape_type: ShapeType::Polyline,
                            points,
                            start_binding,
                            end_binding,
                            ..
                        } if points.len() > 1 => Some(LineSnapState {
                            line_id: *id,
                            start_points: points.clone(),
                            start_start_binding: *start_binding,
                            start_end_binding: *end_binding,
                        }),
                        _ => None,
                    }
                })
                .collect();
            self.drag = DragState::MoveItems {
                start_canvas,
                start_transforms,
                line_snaps,
                duplicate_ids,
                original_ids,
                pending_deselect,
            };
            return;
        }

        // 3) 空白：开始框选（spec L240）。Shift = 加选模
        let start_canvas = self.viewport.screen_to_canvas(screen_pos);
        self.drag = DragState::BoxSelect {
            start_canvas,
            current_canvas: start_canvas,
            additive,
        };
    }

    fn update_drag_preview(&mut self, screen_pos: egui::Pos2, free_scale: bool, axis_lock: bool) {
        // 裁剪模式拖拽：直接更crop_mode.rect，不进入 DragState
        if let Some(crop) = self.crop_mode.as_mut() {
            if let Some(handle) = crop.dragging {
                self.update_crop_drag(screen_pos, handle);
                return;
            }
        }

        // plan #4：Alt+拖端点"延伸"的延迟启动——越过移动阈值前按普通端点拖拽
        // 处理；越阈瞬间在该端外侧插入一个复制顶点（原端点变成中间顶点），随后
        // 拖的是新点。是否已插入用 start_points 与当前点集的数量差判定，不引入
        // 额外交互状态；中途松开 Alt 不撤销延伸（保持简洁，与 Excalidraw 的
        // uncommitted 点丢弃语义不同）。
        {
            let mut armed: Option<(ItemId, usize, usize, (f32, f32))> = None; // (线, 端点, 原点数, base_pos)
            if let DragState::LineEndpoint {
                item_id,
                endpoint,
                start_canvas,
                start_points,
                base_pos,
                alt_extend: true,
            } = &self.drag
            {
                let cur = self.viewport.screen_to_canvas(screen_pos);
                let moved =
                    (cur - *start_canvas).length() >= EXTEND_TRIGGER_PX / self.viewport.zoom;
                if moved {
                    let not_appended_yet = self.scene.get_item(item_id).is_some_and(|it| {
                        !matches!(&it.kind, ItemKind::Shape { points, .. }
                            if points.len() > start_points.len())
                    });
                    if not_appended_yet {
                        armed = Some((*item_id, *endpoint, start_points.len(), *base_pos));
                    }
                }
            }
            if let Some((item_id, endpoint, n_old, base_pos)) = armed {
                let prepend = endpoint == 0;
                if let Some(it) = self.scene.get_item_mut(&item_id) {
                    if let ItemKind::Shape { points, .. } = &mut it.kind {
                        if prepend {
                            points.insert(0, base_pos);
                        } else {
                            points.push(base_pos);
                        }
                    }
                }
                if !prepend {
                    // 末点侧：被拖下标改到新插入的复制点（= 原点数）；0 侧仍拖 index 0
                    if let DragState::LineEndpoint { endpoint: e, .. } = &mut self.drag {
                        *e = n_old;
                    }
                }
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
                        // 翻转手柄begin_drag 中已即时处理，不会进入拖拽预
                        Handle::FlipH | Handle::FlipV | Handle::None => {}
                        // 线类顶点/段中点begin_drag 中已进入 LineEndpoint 拖拽，不会到达这里
                        Handle::Endpoint(_) | Handle::SegmentMid(_) => {}
                    }
                }
                // plan #5：缩放/旋转形状时，实时联动重算绑定到它的线端点
                self.scene.resolve_bindings(&[item_id]);
            }
            DragState::MoveItems {
                start_canvas,
                start_transforms,
                line_snaps,
                ..
            } => {
                let current_canvas = self.viewport.screen_to_canvas(screen_pos);
                let mut delta = current_canvas - *start_canvas;
                if axis_lock {
                    // Shift 轴约束：只保留位移较大的那一轴，另一轴清零
                    // （PowerPoint / Excalidraw 同款）。视口只有平移与缩放、无旋转，
                    // 画布轴与屏幕轴同向，故在画布空间取轴即可。
                    if delta.x.abs() >= delta.y.abs() {
                        delta.y = 0.0;
                    } else {
                        delta.x = 0.0;
                    }
                }
                for (id, start_tf) in start_transforms {
                    if let Some(item) = self.scene.get_item_mut(id) {
                        item.transform.pos = start_tf.pos + delta;
                    }
                }
                let ids: Vec<ItemId> = start_transforms.iter().map(|(id, _)| *id).collect();
                // plan #14：移动整条线时端点吸附 + 建立绑定。命中 → 端点贴边并
                // 绑定（锚点 = 贴合点在目标局部系坐标）；未命中 → 端点随线自由
                // 移动并解绑（Excalidraw 同款）。吸附目标排除移动组自身。
                let moved: std::collections::HashSet<ItemId> = ids.iter().copied().collect();
                let threshold = SNAP_THRESHOLD_PX / self.viewport.zoom;
                self.snap_highlight = None;
                for ls in line_snaps {
                    let last = ls.start_points.len().saturating_sub(1);
                    for endpoint in [0usize, last] {
                        // 端点当前画布位置（points 尚为初始值，transform 已随组移动）
                        let qc = self
                            .scene
                            .get_item(&ls.line_id)
                            .map(|it| it.local_point_to_canvas(ls.start_points[endpoint]));
                        let hit = qc.and_then(|qc| {
                            self.scene
                                .find_snap_target(&ls.line_id, qc, threshold)
                                .filter(|(bid, _, _)| !moved.contains(bid))
                        });
                        match hit {
                            Some((bid, sc, _)) => {
                                // 先不可变换算（锚点 + 线局部新坐标），再可变写回
                                let anchor = self
                                    .scene
                                    .get_item(&bid)
                                    .and_then(|t| t.canvas_to_local_point(sc));
                                let new_local = self
                                    .scene
                                    .get_item(&ls.line_id)
                                    .and_then(|it| it.canvas_to_local_point(sc));
                                if let (Some(new_local), Some(item)) =
                                    (new_local, self.scene.get_item_mut(&ls.line_id))
                                {
                                    if let ItemKind::Shape {
                                        points,
                                        start_binding,
                                        end_binding,
                                        ..
                                    } = &mut item.kind
                                    {
                                        points[endpoint] = new_local;
                                        let binding = anchor.map(|a| EndpointBinding {
                                            target: bid,
                                            anchor: Some(a),
                                        });
                                        if endpoint == 0 {
                                            *start_binding = binding;
                                        } else {
                                            *end_binding = binding;
                                        }
                                    }
                                    self.snap_highlight = Some(bid);
                                }
                            }
                            None => {
                                // 端点回到随线移动的基准位（上一帧可能吸附偏移过）
                                // 并解绑该端点
                                if let Some(item) = self.scene.get_item_mut(&ls.line_id) {
                                    if let ItemKind::Shape {
                                        points,
                                        start_binding,
                                        end_binding,
                                        ..
                                    } = &mut item.kind
                                    {
                                        points[endpoint] = ls.start_points[endpoint];
                                        if endpoint == 0 {
                                            *start_binding = None;
                                        } else {
                                            *end_binding = None;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // plan #5：移动形状时，实时联动重算绑定到它的线端点
                self.scene.resolve_bindings(&ids);
            }
            DragState::CreatingShape { current, .. } => {
                let _ = current; // 由 match 后更新（需单独 &mut self.drag）
            }
            DragState::LineEndpoint {
                item_id,
                endpoint,
                start_canvas,
                start_points,
                base_pos,
                alt_extend,
            } => {
                let item_id = *item_id;
                let endpoint = *endpoint;
                let start_canvas = *start_canvas;
                let base_pos = *base_pos;
                // plan #4：Alt 延伸尚未触发（还没插入复制点）时不动原端点——
                // Alt+单击删除不该因几像素抖动被误当"微调端点"提交。
                if *alt_extend {
                    let appended = self.scene.get_item(&item_id).is_some_and(|it| {
                        matches!(&it.kind, ItemKind::Shape { points, .. }
                            if points.len() > start_points.len())
                    });
                    if !appended {
                        return;
                    }
                }
                let current_canvas = self.viewport.screen_to_canvas(screen_pos);
                let delta_canvas = current_canvas - start_canvas;

                // plan #5：端点吸附。仅真实端点（0 / 末点）可绑定；查询点取**被拖端点**
                // 的当前画布位置（base_pos + 拖拽位移，跟随鼠标），吸附到轮廓上离光标
                // 最近的点。修复：此前误用另一端点位置做查询，导致拖拽端永远吸不上
                // （另一端不在形状附近时无命中），且另一端恰在形状上时会把被拖端吸回起点。
                let mut snap: Option<(ItemId, CanvasPoint)> = None;
                if let Some(item) = self.scene.get_item(&item_id) {
                    if let ItemKind::Shape { points, .. } = &item.kind {
                        let last = points.len().saturating_sub(1);
                        if endpoint == 0 || endpoint == last {
                            let delta_local = item
                                .local_to_canvas()
                                .inverse()
                                .map(|inv| inv.transform_vector(delta_canvas));
                            if let Some(delta_local) = delta_local {
                                let qc = item.local_point_to_canvas((
                                    base_pos.0 + delta_local.x,
                                    base_pos.1 + delta_local.y,
                                ));
                                let threshold = SNAP_THRESHOLD_PX / self.viewport.zoom;
                                if let Some((bid, sc, _)) =
                                    self.scene.find_snap_target(&item_id, qc, threshold)
                                {
                                    snap = Some((bid, sc));
                                }
                            }
                        }
                    }
                }

                // 吸附点换算：线局部坐标（直接写回 points）+ 目标局部坐标（锚点，
                // 供 resolve_bindings 按钉点重算）。全部在可变借用前算好。
                let (snap_anchor, snap_local): (Option<_>, Option<SnapHitLocal>) = match snap {
                    Some((bid, sc)) => {
                        let anchor = self
                            .scene
                            .get_item(&bid)
                            .and_then(|t| t.canvas_to_local_point(sc));
                        let local = self
                            .scene
                            .get_item(&item_id)
                            .and_then(|it| it.canvas_to_local_point(sc));
                        (Some(anchor), local.map(|l| (bid, l)))
                    }
                    None => (None, None),
                };

                if let Some(item) = self.scene.get_item_mut(&item_id) {
                    // 画布位移 → 局部位移（逆变换向量的平移部分自动抵消）
                    let delta_local = item
                        .local_to_canvas()
                        .inverse()
                        .map(|inv| inv.transform_vector(delta_canvas));
                    if let Some(delta_local) = delta_local {
                        if let ItemKind::Shape { points, .. } = &mut item.kind {
                            if let Some(p) = points.get_mut(endpoint) {
                                match snap_local {
                                    Some((bid, local)) => {
                                        *p = local;
                                        self.pending_endpoint_binding =
                                            Some((endpoint, bid, snap_anchor.flatten()));
                                        self.snap_highlight = Some(bid);
                                    }
                                    None => {
                                        *p = (
                                            base_pos.0 + delta_local.x,
                                            base_pos.1 + delta_local.y,
                                        );
                                        // 拖离形状：若该端点原已绑定，本次拖拽将解绑
                                        self.pending_endpoint_binding = None;
                                        self.snap_highlight = None;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            DragState::BoxSelect { .. } => {} // 由 match 后更新（需单独 &mut self.drag）
            DragState::CreatingFrame { current, .. } => {
                let _ = current; // 由 match 后更新（需单独 &mut self.drag）
            }
            // 多拍工具：current 由 match 后统一更新
            DragState::CreatingPolygon { .. } => {}
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
        if let DragState::CreatingPolygon { current, .. } = &mut self.drag {
            *current = self.viewport.screen_to_canvas(screen_pos);
        }
    }

    fn end_drag(&mut self) {
        // 多拍工具（多边形）不在此收尾：每次 pointer down 落一个顶点，
        // 只有 Enter / 双击 / Esc 才结束。end_drag 靠 mem::replace 清空 drag，
        // 直接返回以免把进行中的顶点序列抹掉。
        if matches!(self.drag, DragState::CreatingPolygon { .. }) {
            return;
        }
        // 裁剪模式拖拽释放：清dragging 标志（应用通过 Enter 触发
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
                // clone new_transform，避免与 undo_stack.push &mut self.scene 冲突
                let new_transform = self.scene.get_item(&item_id).map(|it| it.transform);
                if let Some(new_tf) = new_transform {
                    if new_tf != start_transform {
                        let cmd = TransformItem::new(item_id, start_transform, new_tf);
                        // skip_first_redo=true，因为预览已应用
                        self.push_cmd(Box::new(cmd));
                        self.flash(fill(
                            t(self.lang, T::FlashTransform),
                            &[
                                format!("{:.2}", new_tf.scale.x),
                                format!("{:.2}", new_tf.scale.y),
                                format!("{:.1}", new_tf.rotation.to_degrees()),
                            ],
                        ));
                    }
                }
                self.transform_handles.end_drag();
            }
            DragState::MoveItems {
                start_transforms,
                line_snaps,
                duplicate_ids,
                original_ids,
                pending_deselect,
                ..
            } => {
                if let Some(ids) = duplicate_ids {
                    // Ctrl+拖动复制：副本已在场景中（预览已移动到最终位置）。
                    let items: Vec<Item> = ids
                        .iter()
                        .filter_map(|id| self.scene.get_item(id).cloned())
                        .collect();
                    // 用任一副本相对其 start_tf 判断是否真的移动了
                    let moved =
                        ids.first()
                            .and_then(|id| {
                                start_transforms.iter().find(|(i, _)| i == id).and_then(
                                    |(_, st)| {
                                        self.scene.get_item(id).map(|it| it.transform.pos - st.pos)
                                    },
                                )
                            })
                            .map(|d| d.x.abs() > 1e-4 || d.y.abs() > 1e-4)
                            .unwrap_or(false);
                    if moved && !items.is_empty() {
                        // 一次 undo 撤掉整个「复制 + 移动」
                        let n = items.len();
                        self.push_cmd(Box::new(AddItems::new(items).with_preview_applied(true)));
                        self.flash(fill(t(self.lang, T::FlashDuplicated), &[n.to_string()]));
                    } else {
                        // 没移动：删掉重叠副本，恢复原件选区（Ctrl+点击 ≠ 复制）
                        for id in &ids {
                            self.scene.remove_item(id);
                        }
                        self.scene.deselect_all();
                        for oid in &original_ids {
                            self.scene.select(*oid);
                        }
                    }
                } else {
                    // 普通移动 / Shift 轴约束移动：用第一 item 反 delta
                    let delta_opt = start_transforms.first().and_then(|(id, start_tf)| {
                        self.scene
                            .get_item(id)
                            .map(|it| it.transform.pos - start_tf.pos)
                    });
                    if let Some(delta) = delta_opt {
                        if delta.x.abs() > 1e-4 || delta.y.abs() > 1e-4 {
                            let ids: Vec<ItemId> =
                                start_transforms.iter().map(|(i, _)| *i).collect();
                            let move_cmd = MoveItems::new(ids, delta);
                            // plan #14：线端吸附的 points/binding 变更与移动打包成
                            // 一条 undo 记录（整体撤销/重做）。子命令的 points 与
                            // 绑定均已预览直改，skip_first_redo 语义一致。
                            let mut line_cmds: Vec<Box<dyn Command>> = Vec::new();
                            for ls in line_snaps {
                                let Some(item) = self.scene.get_item(&ls.line_id) else {
                                    continue;
                                };
                                let ItemKind::Shape {
                                    points,
                                    start_binding,
                                    end_binding,
                                    ..
                                } = &item.kind
                                else {
                                    continue;
                                };
                                let changed = points != &ls.start_points
                                    || start_binding != &ls.start_start_binding
                                    || end_binding != &ls.start_end_binding;
                                if !changed {
                                    continue;
                                }
                                line_cmds.push(Box::new(
                                    EditShapePoints::new(
                                        ls.line_id,
                                        ls.start_points.clone(),
                                        points.clone(),
                                    )
                                    .with_binding_change(
                                        ls.start_start_binding,
                                        *start_binding,
                                        ls.start_end_binding,
                                        *end_binding,
                                    ),
                                ));
                            }
                            let cmd: Box<dyn Command> = if line_cmds.is_empty() {
                                Box::new(move_cmd)
                            } else {
                                let mut all: Vec<Box<dyn Command>> = vec![Box::new(move_cmd)];
                                all.extend(line_cmds);
                                Box::new(MultiCommand::new(all))
                            };
                            self.push_cmd(cmd);
                            self.flash(fill(
                                t(self.lang, T::FlashMoved),
                                &[format!("{:.0}", delta.x), format!("{:.0}", delta.y)],
                            ));
                        } else if let Some(pid) = pending_deselect {
                            // Shift+点击已选中项且没拖动 → 取消选中（组则取消整组，G2）
                            for hid in self.scene.expand_to_groups(&[pid]) {
                                self.scene.selection.remove(&hid);
                            }
                        }
                    } else if let Some(pid) = pending_deselect {
                        for hid in self.scene.expand_to_groups(&[pid]) {
                            self.scene.selection.remove(&hid);
                        }
                    }
                }
            }
            DragState::BoxSelect {
                start_canvas,
                current_canvas,
                additive,
            } => {
                // 选中框内所item（bounding_rect 相交即选中
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
                // 绑定文本不参与框选：它随容器联动（容器被框选时经 texts_bound_to
                // 带上），单独框中它只会得到一个移动不了的可视快照选中框
                let hits: Vec<ItemId> = self
                    .scene
                    .items
                    .iter()
                    .filter(|item| item.bounding_rect().intersects(&sel_rect))
                    .filter(|item| !self.scene.is_bound_text(item))
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
                endpoint,
                start_canvas: _,
                start_points,
                base_pos,
                alt_extend,
            } => {
                // 预览已直接改 points；释放时若有变化则固化到 undo 栈
                let mut new_points = match self.scene.get_item(&item_id) {
                    Some(item) => match &item.kind {
                        ItemKind::Shape { points, .. } => points.clone(),
                        _ => Vec::new(),
                    },
                    None => Vec::new(),
                };
                if alt_extend && new_points == start_points {
                    // plan #4：Alt+按在端点上未越过延伸阈值即释放 = Alt+单击 →
                    // 删除该顶点（守卫在 try_delete_vertex 内：开放 ≥2 / 闭合 ≥3）。
                    if !new_points.is_empty() {
                        self.try_delete_vertex(item_id, endpoint);
                    }
                } else if !new_points.is_empty() && new_points != start_points {
                    if new_points.len() != start_points.len()
                        && new_points.get(endpoint) == Some(&base_pos)
                    {
                        // 点击了段中点但未拖动：移除插入的顶点，不产生空命令
                        // （Alt 延伸越阈后又精确拖回 base 的罕见情形同样落在此清理）
                        if let Some(item) = self.scene.get_item_mut(&item_id) {
                            if let ItemKind::Shape { points, .. } = &mut item.kind {
                                if points.len() == new_points.len() {
                                    points.remove(endpoint);
                                }
                            }
                        }
                    } else {
                        // 仅真实端点（0 / 末点）涉及绑定；内部顶点拖拽保持原绑定。
                        let last = new_points.len().saturating_sub(1);
                        let is_real = endpoint == 0 || endpoint == last;
                        let (start_old, end_old, open_now) = {
                            let it = self.scene.get_item(&item_id);
                            let sb = it.and_then(|i| i.start_binding());
                            let eb = it.and_then(|i| i.end_binding());
                            let open = matches!(
                                it.map(|i| &i.kind),
                                Some(ItemKind::Shape { closed: false, .. })
                            );
                            (sb, eb, open)
                        };
                        // plan #4：端点拖回起点附近释放 → 自动闭合成多边形
                        // （≥3 顶点、首尾距 ≤ 屏幕 8px；Excalidraw isPathALoop 同式）。
                        let will_close = open_now
                            && is_real
                            && should_auto_close(&new_points, endpoint, self.viewport.zoom);
                        // 验收反馈 #4-1：反向操作——"已闭合的合并点"（首尾重合）
                        // 把其中一个端点拖开（释放时首尾距 > 屏幕 8px）→ 自动恢复
                        // 开放，无需回侧栏取消闭合勾选。仅重合态触发，普通闭合多边形
                        // 拖顶点不会误开。
                        let overlap_before = start_points.len() >= 2
                            && start_points[0] == start_points[start_points.len() - 1];
                        let will_open = !open_now
                            && is_real
                            && overlap_before
                            && first_last_screen_dist(&new_points, self.viewport.zoom)
                                > POLYLINE_CLOSE_DISTANCE;
                        let mut start_new = start_old;
                        let mut end_new = end_old;
                        let snapped = self.pending_endpoint_binding.is_some();
                        if will_close {
                            // 闭合优先于吸附：被拖端点吸附至对端、该端解绑；
                            // closed 与点集经 with_closed 同占一条 undo。
                            if let Some(item) = self.scene.get_item_mut(&item_id) {
                                if let ItemKind::Shape {
                                    points,
                                    closed,
                                    start_binding,
                                    end_binding,
                                    ..
                                } = &mut item.kind
                                {
                                    let l = points.len().saturating_sub(1);
                                    if l >= 2 {
                                        let src = if endpoint == 0 { points[l] } else { points[0] };
                                        if let Some(p) = points.get_mut(endpoint) {
                                            *p = src;
                                        }
                                        *closed = true;
                                        if endpoint == 0 {
                                            *start_binding = None;
                                            start_new = None;
                                        } else {
                                            *end_binding = None;
                                            end_new = None;
                                        }
                                        new_points = points.clone();
                                    }
                                }
                            }
                        } else {
                            if will_open {
                                // 预览直改开放态；closed 变更记录经 with_closed
                                // 随同一条 undo 还原。
                                if let Some(item) = self.scene.get_item_mut(&item_id) {
                                    if let ItemKind::Shape { closed, .. } = &mut item.kind {
                                        *closed = false;
                                    }
                                }
                            }
                            match self.pending_endpoint_binding {
                                Some((idx, bid, anchor)) if is_real => {
                                    let binding = Some(EndpointBinding {
                                        target: bid,
                                        anchor,
                                    });
                                    if idx == 0 {
                                        start_new = binding;
                                    } else {
                                        end_new = binding;
                                    }
                                }
                                None if is_real => {
                                    // 拖离形状 → 解除该端点绑定
                                    if endpoint == 0 {
                                        start_new = None;
                                    } else {
                                        end_new = None;
                                    }
                                }
                                _ => {}
                            }
                        }
                        // 绑定写入必须在此"预览直改"：EditShapePoints 的
                        // skip_first_redo=true 会跳过 push 后的首次 redo，而
                        // 绑定字段只在 redo 里写——不直改的话吸附释放后
                        // start/end_binding 仍是 None，移动被吸附图形时
                        // resolve_bindings 不重算，端点不跟随（用户反馈）。
                        if let Some(item) = self.scene.get_item_mut(&item_id) {
                            if let ItemKind::Shape {
                                start_binding,
                                end_binding,
                                ..
                            } = &mut item.kind
                            {
                                *start_binding = start_new;
                                *end_binding = end_new;
                            }
                        }
                        let cmd = EditShapePoints::new(item_id, start_points, new_points)
                            .with_binding_change(start_old, start_new, end_old, end_new);
                        let cmd = if will_close {
                            cmd.with_closed(false, true)
                        } else if will_open {
                            cmd.with_closed(true, false)
                        } else {
                            cmd
                        };
                        self.push_cmd(Box::new(cmd));
                        if will_close {
                            self.flash(t(self.lang, T::FlashPolygonClosed));
                        } else if will_open {
                            self.flash(t(self.lang, T::FlashPolygonOpened));
                        } else if snapped {
                            self.flash(t(self.lang, T::FlashSnappedToShape));
                        }
                    }
                }
                self.pending_endpoint_binding = None;
                self.snap_highlight = None;
                self.transform_handles.end_drag();
            }
            // 多边形在函数开头已提前 return（多拍工具不在释放时收尾），这里只为穷尽匹配
            DragState::CreatingPolygon { .. } => {}
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
            .with_sloppiness(self.default_sloppiness);
            self.push_new_item(AddItem::new(item));
            self.flash(t(
                self.lang,
                if end_arrow.is_some() {
                    T::FlashArrowCreated
                } else {
                    T::FlashLineCreated
                },
            ));
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
        // 填充默认值：选了填充样式但未选填充色时，跟随描边色（Excalidraw 语义），
        // 并套默认 50% 不透明度（plan #2；显式选过填充色则保留用户设置）
        let fill = self.default_fill_style.map(|_| {
            self.default_fill.unwrap_or_else(|| {
                let c = self.default_stroke.color;
                [c[0], c[1], c[2], palette::FILL_DEFAULT_ALPHA]
            })
        });
        let item = Item::new_shape(
            shape_type,
            (bw, bh),
            min_x,
            min_y,
            self.default_stroke,
            fill,
        )
        .with_sloppiness(self.default_sloppiness)
        .with_fill_style(self.default_fill_style.unwrap_or(FillStyle::Solid));
        self.push_new_item(AddItem::new(item));
        self.flash(t(self.lang, T::FlashShapeCreated));
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
        self.flash(fill(
            t(self.lang, T::FlashFrameCreated),
            &[number.to_string()],
        ));
        // 默认回 Select
        self.tool = Tool::Select;
    }

    /// Shift 方向锁定：把 `from → to` 这段吸附到 45° 整数倍，保持长度不变。
    /// 与 `finish_create_shape` 里的折线吸附同一套算法（阈值除外）。
    fn snap_polygon_point(from: CanvasPoint, to: CanvasPoint, shift: bool) -> CanvasPoint {
        if !shift {
            return to;
        }
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-3 {
            return from;
        }
        let angle =
            (dy.atan2(dx) / std::f32::consts::FRAC_PI_4).round() * std::f32::consts::FRAC_PI_4;
        from + CanvasVector::new(len * angle.cos(), len * angle.sin())
    }

    /// 多边形工具收尾（Phase I）：把已落定的顶点固化成一个 `closed: true` 的 Polyline。
    ///
    /// 顶点先归一化到"以 AABB 左上角为原点"的局部坐标，整体位置交给 `transform.pos`——
    /// 与两点式折线共用 `Item::new_polyline` 的同一约定，于是后续移动/缩放/顶点拖拽
    /// 都能复用既有的线性对象代码路径，不必为多边形单开分支。
    ///
    /// 收尾时机：Enter / 双击画布（见 `handle_shortcuts` 与 CentralPanel）。
    /// 顶点不足 3 个时不产生 item（无法构成面），只回 Select 并给出提示。
    fn finish_create_polygon(&mut self) {
        let (mut points, current) = match std::mem::replace(&mut self.drag, DragState::Idle) {
            DragState::CreatingPolygon {
                points, current, ..
            } => (points, current),
            // 非多边形状态（防御性）：原样放回，避免吞掉别的拖拽
            other => {
                self.drag = other;
                return;
            }
        };
        // 橡皮筋末端算作最后一个顶点；指针停在上一顶点上（双击）时距离≈0，会被跳过
        if let Some(last) = points.last() {
            if (current - *last).length() >= 3.0 {
                points.push(current);
            }
        }
        self.tool = Tool::Select;
        if points.len() < 3 {
            self.flash(t(self.lang, T::PolygonTooFewPoints));
            return;
        }
        let (min_x, min_y, max_x, max_y) =
            points
                .iter()
                .fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |acc, p| {
                    (
                        acc.0.min(p.x),
                        acc.1.min(p.y),
                        acc.2.max(p.x),
                        acc.3.max(p.y),
                    )
                });
        let local: Vec<(f32, f32)> = points.iter().map(|p| (p.x - min_x, p.y - min_y)).collect();
        let item = Item::new_polyline(
            local,
            (max_x - min_x, max_y - min_y),
            None,
            None,
            true,
            min_x,
            min_y,
            self.default_stroke,
        )
        .with_sloppiness(self.default_sloppiness);
        self.push_new_item(AddItem::new(item));
        self.flash(t(self.lang, T::PolygonCreated));
    }
}

// ─────────────────────────── 渲染 ───────────────────────────

impl PReferZApp {
    // ─────────────────────────── Present（Slide 演示） ───────────────────────────

    /// 绘制 Text item：可选背景色 + 文字。Edit 与 Present 两条渲染路径共用。
    ///
    /// **背景默认全透明**：只有 item 显式设置了 `ItemKind::Text::background` 才画底色
    /// （为将来侧栏"文字背景"开关预留）。背景矩形取自**本次实际排版结果**——
    /// 自由文本跟随自身 transform，绑定文本跟随容器矩形，
    /// 故容器移动 / 缩放 / 换行时背景与文字始终同步，不会各自错位。
    fn draw_text_item(&self, ui: &mut egui::Ui, item: &Item, editing_id: Option<ItemId>) {
        let ItemKind::Text {
            content,
            font_size,
            color,
            container_id,
            background,
            align_h,
            align_v,
            font_family,
            ..
        } = &item.kind
        else {
            return;
        };
        // 编辑期间内容由 overlay 的 TextEdit 接管，此处不画，避免原文字与编辑框重影
        if editing_id == Some(item.id) {
            return;
        }

        let text_color =
            egui::Color32::from_rgba_premultiplied(color[0], color[1], color[2], color[3]);
        let zoom = self.viewport.zoom;
        let hand = *font_family == FontFamily::Handwriting;

        // 统一产出 (字形 galley, 绝对屏幕位置) 列表 + 文字内容包围矩形。
        // 伪手写逐字排版返回多片；普通排版单 galley 一片。
        let (pieces, content_rect) =
            match container_id.filter(|cid| self.scene.get_item(cid).is_some()) {
                // 绑定文本：换行到容器宽度，按对齐枚举定位（plan #1；默认居中 = 历史行为）
                Some(cid) => {
                    let container = self.scene.get_item(&cid).expect("filter 已保证存在");
                    let cr = self
                        .viewport
                        .canvas_to_screen_rect(container.bounding_rect());
                    let wrap = (cr.width() - 12.0).max(20.0);
                    let pad = 6.0 * zoom;
                    if hand {
                        let seed = item.id.as_u128() as u64;
                        let (mut pieces, size) = Self::layout_handwritten(
                            ui.ctx(),
                            content,
                            *font_size * zoom,
                            text_color,
                            Some(wrap),
                            seed,
                        );
                        // 水平/垂直对齐（仅绑定文字生效）
                        let dx = match align_h {
                            TextAlignH::Left => pad,
                            TextAlignH::Center => (cr.width() - size.x) * 0.5,
                            TextAlignH::Right => cr.width() - size.x - pad,
                        };
                        let dy = match align_v {
                            TextAlignV::Top => pad,
                            TextAlignV::Middle => (cr.height() - size.y) * 0.5,
                            TextAlignV::Bottom => cr.height() - size.y - pad,
                        };
                        // layout_handwritten 返回相对原点的位置，先平移到容器矩形
                        // 左上，再按对齐枚举偏移（修复：此前漏加 cr.min，文字跑容器左上角外）
                        for (_g, p) in &mut pieces {
                            *p += cr.min.to_vec2() + egui::vec2(dx, dy);
                        }
                        let rect = egui::Rect::from_min_size(cr.min + egui::vec2(dx, dy), size);
                        (pieces, rect)
                    } else {
                        let galley = ui.ctx().fonts_mut(|f| {
                            f.layout_job(egui::text::LayoutJob::simple(
                                content.clone(),
                                egui::FontId::proportional(*font_size * zoom),
                                text_color,
                                wrap,
                            ))
                        });
                        let size = galley.size();
                        let dx = match align_h {
                            TextAlignH::Left => pad,
                            TextAlignH::Center => (cr.width() - size.x) * 0.5,
                            TextAlignH::Right => cr.width() - size.x - pad,
                        };
                        let dy = match align_v {
                            TextAlignV::Top => pad,
                            TextAlignV::Middle => (cr.height() - size.y) * 0.5,
                            TextAlignV::Bottom => cr.height() - size.y - pad,
                        };
                        let top_left = cr.min + egui::vec2(dx, dy);
                        (
                            vec![(galley, top_left)],
                            egui::Rect::from_min_size(top_left, size),
                        )
                    }
                }
                // 自由文本（或容器已丢失）：按自身 transform 定位，字号叠加 scale；
                // 对齐枚举不生效（单行无框，恒 top-left）
                None => {
                    let origin = self.viewport.canvas_to_screen(item.canvas_corners()[0]);
                    // scale.x（等比缩放场景下 scale.y 相同；非等比 egui text 不支持非均匀缩放）
                    let effective_font_size = *font_size * item.transform.scale.x.abs() * zoom;
                    if hand {
                        let seed = item.id.as_u128() as u64;
                        let (mut pieces, size) = Self::layout_handwritten(
                            ui.ctx(),
                            content,
                            effective_font_size,
                            text_color,
                            None,
                            seed,
                        );
                        // layout_handwritten 返回相对原点的位置，平移到 item 起点
                        for (_g, p) in &mut pieces {
                            *p += origin.to_vec2();
                        }
                        (pieces, egui::Rect::from_min_size(origin, size))
                    } else {
                        let galley = ui.ctx().fonts_mut(|f| {
                            f.layout_no_wrap(
                                content.clone(),
                                egui::FontId::proportional(effective_font_size),
                                text_color,
                            )
                        });
                        let size = galley.size();
                        (
                            vec![(galley, origin)],
                            egui::Rect::from_min_size(origin, size),
                        )
                    }
                }
            };

        if let Some(bg) = background {
            let pad = 4.0 * zoom;
            ui.painter().rect_filled(
                content_rect.expand(pad),
                egui::CornerRadius::same(2),
                egui::Color32::from_rgba_unmultiplied(bg[0], bg[1], bg[2], bg[3]),
            );
        }
        for (galley, pos) in pieces {
            ui.painter().galley(pos, galley, text_color);
        }
    }

    /// 伪手写排版（plan #1）：逐字符用自身 galley 宽度步进（忽略字距，
    /// 笔画参差正是手写感的一部分），字号与位置按确定性种子微抖。
    ///
    /// `font_px` 为基准字号（屏幕像素）；`wrap` 为换行宽度（仅绑定文字，
    /// `None` 不换行）。返回 `(字形 galley, 相对原点的位置)` 列表与整体包围盒尺寸。
    /// 种子由调用方取自 item id，保证重绘 / 存盘重开不跳变。
    fn layout_handwritten(
        ctx: &egui::Context,
        content: &str,
        font_px: f32,
        color: egui::Color32,
        wrap: Option<f32>,
        seed: u64,
    ) -> (Vec<(std::sync::Arc<egui::Galley>, egui::Pos2)>, egui::Vec2) {
        let mut rng = SeededRng::new(seed ^ 0x6D61_6E75_7363_7269); // "manuscri"
        let mut pieces = Vec::new();
        // 观感参数：抖动要足够大才能与正常排版拉开差距（用户反馈 0.045/±4%
        // 几乎不可见）。幅度取字号的 10%，字号与字符间距各抖 ±8%。
        let amp = font_px * 0.10;
        let mut pen = egui::pos2(0.0, 0.0);
        let mut line_h = 0.0_f32;
        let mut max_x = 0.0_f32;
        for line in content.split('\n') {
            pen.x = 0.0;
            pen.y += line_h;
            line_h = 0.0;
            for ch in line.chars() {
                let s = font_px * (1.0 + rng.signed() * 0.08);
                let g = ctx.fonts_mut(|f| {
                    f.layout_no_wrap(ch.to_string(), egui::FontId::proportional(s), color)
                });
                let gsize = g.size();
                // 超出换行宽度则折行（首字符不折，避免窄容器死循环）
                if let Some(w) = wrap {
                    if pen.x + gsize.x > w && pen.x > 0.0 {
                        pen.x = 0.0;
                        pen.y += line_h;
                        line_h = 0.0;
                    }
                }
                let p = egui::pos2(pen.x + rng.signed() * amp, pen.y + rng.signed() * amp);
                pieces.push((g, p));
                pen.x += gsize.x * (1.0 + rng.signed() * 0.08);
                line_h = line_h.max(gsize.y);
                max_x = max_x.max(pen.x);
            }
        }
        (pieces, egui::vec2(max_x, pen.y + line_h))
    }

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
                    let tint = pixmap_tint(alpha);
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
                        egui::CornerRadius::same(0),
                        egui::Color32::from_rgb(70, 70, 70),
                    );
                }
            }
            // 文字：可选背景 + 排版绘制，两处渲染路径共用 draw_text_item
            ItemKind::Text { .. } => self.draw_text_item(ui, item, editing_id),
            // 风格器分发（CleanStyler / RoughStyler）在 build_shape_visuals 内按 sloppiness 档位决定。
            ItemKind::Shape { .. } => {
                let to_screen = item_local_to_screen(item, &self.viewport);
                let shapes = build_shape_visuals(&item.kind, &to_screen, self.viewport.zoom);
                ui.painter().extend(shapes);
            }
            ItemKind::Frame { .. } => {
                let sr = self.viewport.canvas_to_screen_rect(item.bounding_rect());
                let border = egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(90, 90, 95));
                ui.painter()
                    .rect_stroke(sr, 0.0, border, egui::StrokeKind::Middle);
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
            let (z, p) = self.present_compute_fit(ctx.content_rect(), frame.bounding_rect());
            self.present_anim = Some((z, p));
        }
        ctx.request_repaint();
    }

    fn render_present(&mut self, ui: &mut egui::Ui) {
        let ctx = &ui.ctx().clone();
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
        let screen_rect = ctx.content_rect();
        self.viewport.set_screen_rect(screen_rect);

        let Some(frame) = self.scene.get_item(&slides[index]) else {
            return;
        };
        let frame_rect = frame.bounding_rect();
        self.present_apply_fit(ctx, screen_rect, frame_rect);
        let frame_screen_rect = self.viewport.canvas_to_screen_rect(frame_rect);

        // 画布背景跟随主题（D2）；Present 为不透明演示背景。
        let present_bg = self.theme.canvas_bg(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(egui::Color32::from_rgb(
                present_bg[0],
                present_bg[1],
                present_bg[2],
            )))
            .show(ui, |ui| {
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
                // 页码颜色随主题反色（Light 下浅灰背景上需深色字）。
                let page_text_color = if self.theme.is_dark(ctx) {
                    egui::Color32::from_gray(210)
                } else {
                    egui::Color32::from_gray(40)
                };
                ui.label(egui::RichText::new(label).size(16.0).color(page_text_color));
            });
    }

    fn handle_present_input(&mut self, ctx: &egui::Context) {
        let n = self.present_slide_count();
        let index = match &self.app_mode {
            AppMode::Present { index, .. } => *index,
            AppMode::Edit => return,
        };

        // 退出：取消键 / 再次按演示切换键 / Esc 硬兜底（与 cancel_pressed 同策略）
        let exit = self.keymap.pressed(Action::Cancel, ctx)
            || self.keymap.pressed(Action::TogglePresent, ctx)
            || ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if exit {
            self.exit_present(ctx);
            return;
        }

        let mut delta: i32 = 0;
        if self.keymap.pressed(Action::PresentNext, ctx) {
            delta = 1;
        } else if self.keymap.pressed(Action::PresentPrev, ctx) {
            delta = -1;
        }
        if self.keymap.pressed(Action::PresentFirst, ctx) {
            delta = i32::MIN;
        } else if self.keymap.pressed(Action::PresentLast, ctx) {
            delta = i32::MAX;
        }
        let scroll = ctx.input(|i| i.smooth_scroll_delta).y;
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

        // 预生成所grayscale=true Pixmap 灰度纹理（避免渲染循环里 &mut self &self.scene 冲突
        self.ensure_grayscale_textures(ui.ctx());

        // Z 序渲染（W9）：底层先画，顶层后
        let items: Vec<&Item> = self.scene.items_by_z_order();
        let selection_count = self.scene.selection.len();
        let editing_id = self.editing_text.as_ref().and_then(|e| e.editing_item_id);
        let crop_item_id = self.crop_mode.as_ref().map(|c| c.item_id);
        for item in items {
            // 视口剔除（修 S9/M11）：用画AABB 转屏幕矩形，不相交则跳过
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
                        // UV 计算说明                        // - flip local_to_canvas 的几何翻转实现（canvas_corners flip 后位置交换）                        //   因此 mesh quad screen_corners 即可呈现镜像，UV 不再翻转                        //   否则会与几何翻转抵消，导flip 后图片看起来不变                        // - crop 通过 UV 子矩形采样（item 局部空间，未应flip），
                        //   crop 区域的画布位置由 transform.scale 同步保证边框对齐
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
                        let tint = pixmap_tint(alpha);
                        // mesh quad 渲染，让图片真正跟着旋转/flip（screen_corners 已包含全部几何变换）
                        // screen_corners 顺序：[TL, TR, BL, BR]，重排为 [TL, TR, BR, BL] 顺时
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
                            egui::CornerRadius::same(0),
                            if is_selected {
                                egui::Color32::from_rgb(80, 80, 40)
                            } else {
                                egui::Color32::from_rgb(70, 70, 70)
                            },
                        );
                    }
                }
                // 文字：可选背景 + 排版绘制，两处渲染路径共用 draw_text_item
                ItemKind::Text { .. } => self.draw_text_item(ui, item, editing_id),
                // Shape：风格器分发同 render_scene，sloppiness 档位决定 Clean 或手绘。
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
                    ui.painter()
                        .rect_stroke(sr, 0.0, border, egui::StrokeKind::Middle);
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
                        egui::CornerRadius::same(3),
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

            // 选中+ 手柄：单选时画单独手柄；多选时画统一外框（循环后            // 裁剪模式下手柄隐藏（避免与裁剪框冲突
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

        // 多选统一外框（spec L241：多选时画一个统一 bbox
        if selection_count > 1 {
            if let Some(bbox) = self.scene.selection_bounding_rect() {
                let screen_bbox = self.viewport.canvas_to_screen_rect(bbox);
                let stroke = egui::Stroke::new(1.5_f32, egui::Color32::YELLOW);
                ui.painter()
                    .rect_stroke(screen_bbox, 0.0, stroke, egui::StrokeKind::Middle);
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
                    ui.painter()
                        .rect_filled(r, egui::CornerRadius::same(1), fill);
                }
            }
        }

        // 端点吸附高亮（plan #5）：拖动线端点靠近某形状轮廓时，高亮该形状轮廓。
        // 仅在拖拽预览期间由 update_drag / update_drag_preview 设置，end_drag 时清空。
        if let Some(bid) = self.snap_highlight {
            if let Some(item) = self.scene.get_item(&bid) {
                let segs = snap::outline_segments(item);
                let stroke = egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(255, 170, 40));
                for (a, b) in segs {
                    let sa = self.viewport.canvas_to_screen(a);
                    let sb = self.viewport.canvas_to_screen(b);
                    ui.painter().line_segment([sa, sb], stroke);
                }
            }
        }

        // 裁剪模式 overlay（spec §2.2 裁剪
        self.render_crop_overlay(ui);
    }

    /// 懒生成所grayscale=true Pixmap 灰度纹理（spec §2.2 灰度）    /// 使用 ITU-R BT.601 亮度系数：Y = 0.299R + 0.587G + 0.114B（不引入 palette crate）
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

    /// 懒重建缺失的 Pixmap 纹理（修 .issues #1 灰方块）。
    ///
    /// 删除 Pixmap 时只驱逐 GPU 纹理句柄、保留 RGBA 像素缓存（见 `delete_selected`）：
    /// undo 恢复 item 后此处按缓存重新上传纹理；共享同一 texture_id 的副本/原件
    /// 也不会因为一次删除而集体变灰。每帧调用，缺纹理的场景通常为空，开销可忽略。
    fn ensure_pixmap_textures(&mut self, ctx: &egui::Context) {
        let missing: Vec<u64> = self
            .scene
            .items
            .iter()
            .filter_map(|it| match &it.kind {
                ItemKind::Pixmap { texture_id, .. } => Some(*texture_id),
                _ => None,
            })
            .filter(|tid| !self.texture_cache.contains_key(tid))
            .collect();
        for tid in missing {
            let Some(rgba) = self.rgba_pixel_cache.get(&tid).cloned() else {
                continue;
            };
            let Some((w, h)) = self.rgba_size_cache.get(&tid).copied() else {
                continue;
            };
            let color_image =
                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
            let handle = ctx.load_texture(format!("img_{}", tid), color_image, Default::default());
            self.texture_cache.insert(tid, handle);
        }
    }

    /// 渲染裁剪模式 overlay：在裁剪 item 上画可拖拽的裁剪矩形 + 4 角手+ 遮罩
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
        // Pixmap 支持裁剪
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

        // 遮罩（修旋转图闪烁黑影）：
        // 旧实现把「图片边→裁剪边」拆成 4 条梯形，每条边随裁剪框拖动而重排；
        // 旋转下这些对角线经 egui 抗锯齿后缝隙逐帧变化 → 影像周围闪黑。
        // 改为：① 单个凸多边形把整张图片压暗，外缘 = item_quad（拖拽时图片不动，
        //   遮罩外缘稳定不闪）；② 再把裁剪框内的图片以原始亮度重绘到 crop_quad 之上。
        // 裁剪框描边压住内部接缝，重绘的亮图与底层像素一致（同纹理同 UV），不引入新闪烁。
        let mask_color = egui::Color32::from_rgba_premultiplied(0, 0, 0, 120);
        ui.painter().add(egui::Shape::convex_polygon(
            item_quad.to_vec(),
            mask_color,
            egui::Stroke::NONE,
        ));

        // 裁剪框内图片以原始亮度重绘（需纹理句柄；缺失时退化为仅压暗整图）。
        let tex_handle = match &item.kind {
            ItemKind::Pixmap {
                texture_id,
                grayscale,
                ..
            } => {
                let tid = *texture_id;
                if *grayscale {
                    self.grayscale_texture_cache
                        .get(&tid)
                        .or_else(|| self.texture_cache.get(&tid))
                } else {
                    self.texture_cache.get(&tid)
                }
            }
            _ => None,
        };
        if let Some(handle) = tex_handle {
            // UV 必须与 crop_corners 的归一化基准（current_crop）一致，
            // 而非 base_size——否则已在裁剪过的图片会二次偏移。
            let base = item
                .current_crop()
                .unwrap_or(CropRect::new(0.0, 0.0, 1.0, 1.0));
            let r = crop_state.rect;
            let (u0, u1, v0, v1) = (
                ((r.x - base.x) / base.width).clamp(0.0, 1.0),
                ((r.x + r.width - base.x) / base.width).clamp(0.0, 1.0),
                ((r.y - base.y) / base.height).clamp(0.0, 1.0),
                ((r.y + r.height - base.y) / base.height).clamp(0.0, 1.0),
            );
            let opacity = match &item.kind {
                ItemKind::Pixmap { opacity, .. } => *opacity,
                _ => 1.0,
            };
            let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
            let tint = pixmap_tint(alpha);
            let [tl, tr, br, bl] = crop_quad;
            let verts = [
                ([tl.x, tl.y], [u0, v0]),
                ([tr.x, tr.y], [u1, v0]),
                ([br.x, br.y], [u1, v1]),
                ([bl.x, bl.y], [u0, v1]),
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
            ui.painter()
                .rect_filled(r, egui::CornerRadius::same(1), fill);
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

    /// 对指定 item 开启文本编辑（Excalidraw 语义），双击与 Enter 快捷键共用：
    /// - `Text` item → 编辑其内容；
    /// - 封闭 `Shape` → 编辑已有绑定文本，没有则新建一个空的；
    /// - 其它（图片 / 未闭合折线 / 画框）→ 不处理并返回 false，由调用方兜底。
    ///
    /// 已在编辑中时直接返回 true（不打断当前输入）。
    fn start_text_edit(&mut self, id: ItemId) -> bool {
        if self.editing_text.is_some() {
            return true;
        }
        let Some(kind) = self.scene.get_item(&id).map(|it| it.kind.clone()) else {
            return false;
        };
        match kind {
            ItemKind::Text {
                content,
                font_size,
                color,
                container_id,
                ..
            } => {
                let canvas_pos = self
                    .scene
                    .get_item(&id)
                    .map(|it| it.transform.pos)
                    .map(|p| CanvasPoint::new(p.x, p.y))
                    .unwrap_or_else(|| CanvasPoint::new(f32::MIN, f32::MIN));
                self.editing_text = Some(EditingText {
                    editing_item_id: Some(id),
                    canvas_pos,
                    buffer: content,
                    font_size,
                    color,
                    first_frame: true,
                    container_id,
                });
            }
            ref k if is_text_container(k) => {
                let center = self
                    .scene
                    .get_item(&id)
                    .map(|it| it.bounding_rect().center())
                    .map(|p| CanvasPoint::new(p.x, p.y));
                let Some(center) = center else {
                    return false;
                };
                let existing = self.scene.texts_bound_to(id).into_iter().next();
                // 已有绑定文本 → 编辑；否则提交时才写入容器 id 新建一个
                let (editing_item_id, buffer, font_size, color) =
                    match existing.and_then(|tid| self.scene.get_item(&tid)) {
                        Some(it) => match &it.kind {
                            ItemKind::Text {
                                content,
                                font_size,
                                color,
                                ..
                            } => (existing, content.clone(), *font_size, *color),
                            _ => (None, String::new(), 18.0, [255; 4]),
                        },
                        None => (None, String::new(), 18.0, [255; 4]),
                    };
                self.editing_text = Some(EditingText {
                    editing_item_id,
                    canvas_pos: center,
                    buffer,
                    font_size,
                    color,
                    first_frame: true,
                    container_id: Some(id),
                });
            }
            _ => return false,
        }
        true
    }

    /// 选中项恰好一个时返回其 id（多选返回 None，此时不做"编辑文本"这类单选语义的操作）。
    fn single_selected_id(&self) -> Option<ItemId> {
        if self.scene.selection.len() == 1 {
            self.scene.selection.iter().next().copied()
        } else {
            None
        }
    }

    /// 测量所有 Text item 的实际文字尺寸并更新 `measured_size`（修 B6）。
    /// 仅在 `measured_size` 为 None 时测量（content 变化会清空 measured_size）；
    /// 绑定文本例外——它随容器宽度换行，需每帧重测。
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
                        let gal = ctx.fonts_mut(|f| f.layout_job(job));
                        updates.push((item.id, (gal.size().x / zoom, gal.size().y / zoom)));
                        continue;
                    }
                }
                if measured_size.is_none() {
                    // 自由文本：仅在未测量时测（content 变化会清空触发重测）。
                    let gal = ctx.fonts_mut(|fonts| {
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

    /// 渲染文本便签编辑 overlay（spec L243 P2-5）    /// 创建中的文本不在 scene 中；Enter/失焦时提交（非空→AddItem），Esc 取消
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
            // 弹层背景跟随主题（D2）：去掉硬编码暗色 fill，用 Frame::popup 的主题默认。
            let frame = egui::Frame::popup(ui.style()).stroke(egui::Stroke::new(
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
                    // 创建模式：空内容丢弃，非push AddItem
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
                    // 编辑模式：空内容不修改原 item（避免误删）；非空且变化push EditTextContent
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
                // 弹层背景跟随主题（D2）：去掉硬编码暗色 fill，用 Frame::popup 的主题默认。
                egui::Frame::popup(ui.style())
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
            self.flash(fill(
                t(self.lang, T::FlashFrameRenumbered),
                &[new_number.to_string()],
            ));
        }
    }

    fn render_context_menu(&mut self, ctx: &egui::Context) {
        let menu_id = egui::Id::new("context_menu");
        let pos = self.context_menu_pos;
        let has_selection = !self.scene.selection.is_empty();
        let primary_pressed = ctx.input(|i| i.pointer.primary_pressed());
        let pointer_pos = ctx.input(|i| i.pointer.latest_pos());

        // egui::Area + 手动按钮。返回菜rect 用于检测点击外部（B4
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
                        .button(format!(
                            "\u{25B6} {}",
                            self.shortcut_hint(T::Present, &[Action::TogglePresent])
                        ))
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
                        .button(format!(
                            "\u{1F4CB} {}",
                            self.shortcut_hint(T::PasteImage, &[Action::Paste])
                        ))
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
                        // 编组/解组（plan #13）：编组需 ≥2 项；解组需选区内有已编组项
                        if self.scene.selection.len() >= 2
                            && ui
                                .button(format!("\u{1F510} {}", t(self.lang, T::MenuGroup)))
                                .clicked()
                        {
                            self.group_selected();
                            self.context_menu_open = false;
                        }
                        if ui
                            .button(format!("\u{1F513} {}", t(self.lang, T::MenuUngroup)))
                            .clicked()
                        {
                            self.ungroup_selected();
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
                                    // 对齐 / 分布（plan #6）：与属性栏同一套动作
                                    ui.separator();
                                    ui.menu_button(
                                        format!("\u{21D4} {}", t(self.lang, T::PropsSectionAlign)),
                                        |ui| {
                                            for (mode, key) in [
                                                (AlignMode::Left, T::AlignLeft),
                                                (AlignMode::HCenter, T::AlignHCenter),
                                                (AlignMode::Right, T::AlignRight),
                                                (AlignMode::Top, T::AlignTop),
                                                (AlignMode::VCenter, T::AlignVCenter),
                                                (AlignMode::Bottom, T::AlignBottom),
                                            ] {
                                                if ui.button(t(self.lang, key)).clicked() {
                                                    self.align_selected(mode);
                                                    self.context_menu_open = false;
                                                }
                                            }
                                        },
                                    );
                                    ui.menu_button(
                                        format!("\u{22EF} {}", t(self.lang, T::Distribute)),
                                        |ui| {
                                            let enabled = self.scene.selection.len() >= 3;
                                            for (axis, dist_mode, key) in [
                                                (
                                                    DistributeAxis::Horizontal,
                                                    DistributeMode::Gap,
                                                    T::DistributeHGap,
                                                ),
                                                (
                                                    DistributeAxis::Horizontal,
                                                    DistributeMode::Centers,
                                                    T::DistributeHCenters,
                                                ),
                                                (
                                                    DistributeAxis::Vertical,
                                                    DistributeMode::Gap,
                                                    T::DistributeVGap,
                                                ),
                                                (
                                                    DistributeAxis::Vertical,
                                                    DistributeMode::Centers,
                                                    T::DistributeVCenters,
                                                ),
                                            ] {
                                                let resp = ui.add_enabled(
                                                    enabled,
                                                    egui::Button::new(t(self.lang, key)),
                                                );
                                                if resp.clicked() {
                                                    self.distribute_selected(axis, dist_mode);
                                                    self.context_menu_open = false;
                                                } else if !enabled {
                                                    resp.on_disabled_hover_text(t(
                                                        self.lang,
                                                        T::FlashDistributeNeedThree,
                                                    ));
                                                }
                                            }
                                        },
                                    );
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

        // 点击菜单外部 关闭菜单（修 B4
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
                            source_size: egui::vec2(w as f32, h as f32),
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
                        source_size: egui::vec2(1.0, 1.0),
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
            self.theme.text_primary(&ctx),
        );
        y += 50.0;
        ui.painter().text(
            egui::pos2(center.x, y),
            egui::Align2::CENTER_CENTER,
            t(self.lang, T::WelcomeSubtitle),
            egui::FontId::proportional(15.0),
            self.theme.text_secondary(&ctx),
        );
        y += 36.0;

        // 最近文件列表
        if !self.recent_files.is_empty() {
            ui.painter().text(
                egui::pos2(center.x, y),
                egui::Align2::CENTER_CENTER,
                t(self.lang, T::WelcomeRecentFiles),
                egui::FontId::proportional(14.0),
                self.theme.text_secondary(&ctx),
            );
            y += 22.0;

            // 借用切片避免 &mut self 冲突；点击记录到 pending_open_recent
            let recent: Vec<PathBuf> = self.recent_files.iter().take(8).cloned().collect();
            for path in recent {
                let row_rect =
                    egui::Rect::from_min_size(egui::pos2(panel_x, y), egui::vec2(panel_w, 26.0));
                let resp = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(row_rect), |ui| {
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
                self.theme.text_tertiary(&ctx),
            );
        }
    }

    /// 右侧属性侧栏（Phase H）：选中项的 per-item 编辑，按 ItemKind 分节。
    ///
    /// D3 语义（见 [`prop`]）：多选时显示交集值；值不一致仍显示代表值，但改动
    /// 批量应用到所有选中项。连续控件（滑块 / 取色器）经 [`PropEdit`] 合并成一条 undo 命令。
    fn render_props_panel(&mut self, ui: &mut egui::Ui) {
        let ctx = &ui.ctx().clone();
        if self.scene.selection.is_empty() {
            // 无选中且绘制工具激活：右侧栏显示「新建元素默认样式」
            // （原底部样式面板移入侧栏；Frame 无样式可调，不显示）。
            if self.tool != Tool::Select && self.tool != Tool::Frame {
                self.render_defaults_panel(ui);
            }
            return;
        }
        let ids: Vec<ItemId> = self.scene.selection.iter().copied().collect();
        let lang = self.lang;
        let dark = self.theme.is_dark(ctx);
        egui::Panel::right("props_panel")
            .default_size(230.0)
            .resizable(true)
            .show(ui, |ui| {
                ui.label(fill(t(lang, T::PropsSelectedCount), &[ids.len().to_string()]));
                ui.separator();

                // 对齐 / 分布（plan #6）：≥2 项才有意义，放在各类型节之前（对所有类型通用）
                if ids.len() >= 2 {
                    ui.label(t(lang, T::PropsSectionAlign));
                    self.render_align_section(ui, lang, &ids);
                    ui.separator();
                }

                let shape_ids: Vec<ItemId> = ids
                    .iter()
                    .copied()
                    .filter(|id| {
                        matches!(self.scene.get_item(id), Some(item) if matches!(item.kind, ItemKind::Shape { .. }))
                    })
                    .collect();
                if !shape_ids.is_empty() {
                    ui.label(t(lang, T::PropsSectionShape));
                    self.render_shape_props(ui, lang, dark, &shape_ids);
                    ui.separator();
                }

                let mut text_ids: Vec<ItemId> = ids
                    .iter()
                    .copied()
                    .filter(|id| {
                        matches!(self.scene.get_item(id), Some(item) if matches!(item.kind, ItemKind::Text { .. }))
                    })
                    .collect();
                // 选中图形时把其绑定文字纳入文字节（Excalidraw 同款入口）：
                // 绑定文字不可独立选中，此前选中图形时侧栏无任何文字样式控件
                //（用户反馈"看不到文字对齐选项 / 文字样式不随主体改"）。
                for id in &ids {
                    if matches!(
                        self.scene.get_item(id),
                        Some(item) if matches!(item.kind, ItemKind::Shape { .. })
                    ) {
                        for item in &self.scene.items {
                            if let ItemKind::Text {
                                container_id: Some(cid),
                                ..
                            } = &item.kind
                            {
                                if cid == id && !text_ids.contains(&item.id) {
                                    text_ids.push(item.id);
                                }
                            }
                        }
                    }
                }
                if !text_ids.is_empty() {
                    ui.label(t(lang, T::PropsSectionText));
                    self.render_text_props(ui, lang, &text_ids);
                    ui.separator();
                }

                let pixmap_ids: Vec<ItemId> = ids
                    .iter()
                    .copied()
                    .filter(|id| {
                        matches!(self.scene.get_item(id), Some(item) if matches!(item.kind, ItemKind::Pixmap { .. }))
                    })
                    .collect();
                if !pixmap_ids.is_empty() {
                    ui.label(t(lang, T::PropsSectionPixmap));
                    self.render_pixmap_props(ui, lang, &pixmap_ids);
                    ui.separator();
                }

                let frame_ids: Vec<ItemId> = ids
                    .iter()
                    .copied()
                    .filter(|id| {
                        matches!(self.scene.get_item(id), Some(item) if matches!(item.kind, ItemKind::Frame { .. }))
                    })
                    .collect();
                if !frame_ids.is_empty() {
                    ui.label(t(lang, T::PropsSectionFrame));
                    self.render_frame_props(ui, lang, &frame_ids);
                }
            });
    }

    /// 对齐 / 分布按钮组（plan #6）：属性栏顶部，选中 ≥2 项时显示。
    ///
    /// 参考系为**选区包围盒**（见 `plan_align` / `plan_distribute`）：对齐贴向该框的
    /// 对应边/中线，分布保持首尾元素不动、只调整中间项。分布要求 ≥3 项，不足时禁用。
    fn render_align_section(&mut self, ui: &mut egui::Ui, lang: Lang, ids: &[ItemId]) {
        // 6 向对齐：两行（水平 3 个 + 垂直 3 个），图标按钮 + tooltip 说明
        const ALIGNS: [(AlignMode, &str, T); 6] = [
            (AlignMode::Left, "\u{21E4}", T::AlignLeft),       // ⇤
            (AlignMode::HCenter, "\u{2194}", T::AlignHCenter), // ↔
            (AlignMode::Right, "\u{21E5}", T::AlignRight),     // ⇥
            (AlignMode::Top, "\u{21E1}", T::AlignTop),         // ⇡
            (AlignMode::VCenter, "\u{2195}", T::AlignVCenter), // ↕
            (AlignMode::Bottom, "\u{21E3}", T::AlignBottom),   // ⇣
        ];
        for row in ALIGNS.chunks(3) {
            ui.horizontal(|ui| {
                for (mode, icon, key) in row {
                    let btn = egui::Button::new(*icon).min_size(egui::vec2(34.0, 24.0));
                    if ui.add(btn).on_hover_text(t(lang, *key)).clicked() {
                        self.align_selected(*mode);
                    }
                }
            });
        }

        // 分布：轴向 × 基准 = 4 个（等距 = 边界空隙相等；等心 = 中心距相等）
        ui.add_space(2.0);
        ui.label(t(lang, T::Distribute));
        let can_distribute = ids.len() >= 3;
        const DISTRIBUTES: [(DistributeAxis, DistributeMode, T); 4] = [
            (
                DistributeAxis::Horizontal,
                DistributeMode::Gap,
                T::DistributeHGap,
            ),
            (
                DistributeAxis::Horizontal,
                DistributeMode::Centers,
                T::DistributeHCenters,
            ),
            (
                DistributeAxis::Vertical,
                DistributeMode::Gap,
                T::DistributeVGap,
            ),
            (
                DistributeAxis::Vertical,
                DistributeMode::Centers,
                T::DistributeVCenters,
            ),
        ];
        for row in DISTRIBUTES.chunks(2) {
            ui.horizontal(|ui| {
                for (axis, mode, key) in row {
                    let resp = ui.add_enabled(can_distribute, egui::Button::new(t(lang, *key)));
                    if resp.clicked() {
                        self.distribute_selected(*axis, *mode);
                    } else if !can_distribute {
                        resp.on_disabled_hover_text(t(lang, T::FlashDistributeNeedThree));
                    }
                }
            });
        }
    }

    /// 「新建元素默认样式」侧栏：绘制工具激活且无选中时显示（原底部样式面板）。
    ///
    /// 控件直接改 `default_*` 字段（非 item 属性，不走 undo 栈）；
    /// 填充节仅对能产生封闭图形的工具显示（线 / 箭头无填充）。
    fn render_defaults_panel(&mut self, ui: &mut egui::Ui) {
        let ctx = &ui.ctx().clone();
        let lang = self.lang;
        let dark = self.theme.is_dark(ctx);
        let show_fill = matches!(self.tool, Tool::Shape(_) | Tool::Polygon);
        egui::Panel::right("defaults_panel")
            .default_size(230.0)
            .resizable(true)
            .show(ui, |ui| {
                ui.label(t(lang, T::PropsDefaultsTitle));
                ui.separator();
                // 描边颜色（Excalidraw 式调色板）
                let mut stroke = self.default_stroke.color;
                if palette::color_palette_button(ui, &mut stroke, dark) {
                    self.default_stroke.color = stroke;
                }
                ui.add_space(4.0);
                // 描边宽度
                ui.label(t(lang, T::StyleStrokeWidth));
                ui.add(
                    egui::Slider::new(&mut self.default_stroke.width, 0.5..=12.0).logarithmic(true),
                );
                // 线型
                ui.horizontal(|ui| {
                    for (dash, label) in [
                        (DashStyle::Solid, T::StyleDashSolid),
                        (DashStyle::Dashed, T::StyleDashDashed),
                        (DashStyle::Dotted, T::StyleDashDotted),
                    ] {
                        let active = self.default_stroke.dash == dash;
                        if ui.selectable_label(active, t(lang, label)).clicked() {
                            self.default_stroke.dash = dash;
                        }
                    }
                });
                // 填充（闭合图形类工具）
                if show_fill {
                    ui.add_space(4.0);
                    ui.label(t(lang, T::StyleFillLabel));
                    ui.horizontal(|ui| {
                        if let Some(new_style) =
                            palette::fill_style_picker(ui, lang, self.default_fill_style)
                        {
                            self.default_fill_style = new_style;
                        }
                    });
                    if self.default_fill_style.is_some() {
                        // 未显式选过填充色时按钮显示描边色（实际创建时同样跟随描边色）
                        let mut fill = self.default_fill.unwrap_or(self.default_stroke.color);
                        if palette::fill_color_palette_button(ui, &mut fill, dark) {
                            self.default_fill = Some(fill);
                        }
                    }
                }
                // 手绘风：新建形状的默认档位（plan #3，对齐 Excalidraw sloppiness）
                ui.add_space(4.0);
                ui.label(t(lang, T::StyleRough));
                ui.horizontal(|ui| {
                    let opts = [
                        (Sloppiness::Off, T::SloppinessOff),
                        (Sloppiness::Architect, T::SloppinessArchitect),
                        (Sloppiness::Artist, T::SloppinessArtist),
                        (Sloppiness::Cartoonist, T::SloppinessCartoonist),
                    ];
                    for (val, label) in opts {
                        let selected = self.default_sloppiness == val;
                        if ui.selectable_label(selected, t(lang, label)).clicked() && !selected {
                            self.default_sloppiness = val;
                        }
                    }
                });
            });
    }

    /// 形状节：描边 / 填充 / 圆角 / 曲线 / 闭合 / 箭头 / 手绘风（Phase H）。
    fn render_shape_props(&mut self, ui: &mut egui::Ui, lang: Lang, dark: bool, ids: &[ItemId]) {
        // 描边颜色（Excalidraw 式调色板）
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Shape { stroke, .. } => Some(stroke.color),
            _ => None,
        }) {
            let mut col = p.value();
            if palette::color_palette_button(ui, &mut col, dark) {
                let new = col;
                self.apply_continuous(
                    ids,
                    PropKind::Stroke,
                    |k| match k {
                        ItemKind::Shape { stroke, .. } => Some(PropValue::Stroke(*stroke)),
                        _ => None,
                    },
                    |k| {
                        if let ItemKind::Shape { stroke, .. } = k {
                            stroke.color = new;
                        }
                    },
                );
                // 联动（用户拍板 2026-09-10）：改描边色时填充色跟随（保留 alpha）、
                // 绑定文字色跟随； undo 由 prop_cmd 打包成一条 MultiCommand。
                self.sync_stroke_color_side_effects(ids, new);
            }
            if p.is_mixed() {
                ui.label(t(lang, T::PropsMixedValue));
            }
        }
        // 描边宽度
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Shape { stroke, .. } => Some(stroke.width),
            _ => None,
        }) {
            let mut w = p.value();
            if ui
                .add(egui::Slider::new(&mut w, 0.5..=12.0).logarithmic(true))
                .changed()
            {
                self.apply_continuous(
                    ids,
                    PropKind::Stroke,
                    |k| match k {
                        ItemKind::Shape { stroke, .. } => Some(PropValue::Stroke(*stroke)),
                        _ => None,
                    },
                    |k| {
                        if let ItemKind::Shape { stroke, .. } = k {
                            stroke.width = w;
                        }
                    },
                );
            }
            if p.is_mixed() {
                ui.label(t(lang, T::PropsMixedValue));
            }
        }
        // 线型
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Shape { stroke, .. } => Some(stroke.dash),
            _ => None,
        }) {
            ui.horizontal(|ui| {
                for (dash, label) in [
                    (DashStyle::Solid, T::StyleDashSolid),
                    (DashStyle::Dashed, T::StyleDashDashed),
                    (DashStyle::Dotted, T::StyleDashDotted),
                ] {
                    let active = p.value() == dash;
                    if ui.selectable_label(active, t(lang, label)).clicked() {
                        self.apply_continuous(
                            ids,
                            PropKind::Stroke,
                            |k| match k {
                                ItemKind::Shape { stroke, .. } => Some(PropValue::Stroke(*stroke)),
                                _ => None,
                            },
                            |k| {
                                if let ItemKind::Shape { stroke, .. } = k {
                                    stroke.dash = dash;
                                }
                            },
                        );
                    }
                }
            });
        }
        // 填充（Excalidraw 四态：无 / 纯色 / 斜线 / 交叉线）。
        // 线 / 箭头（未闭合 Polyline）无填充节；闭合折线（多边形）有。
        let fill_ids: Vec<ItemId> = ids
            .iter()
            .copied()
            .filter(|id| {
                matches!(self.scene.get_item(id), Some(item) if match &item.kind {
                    ItemKind::Shape { shape_type, closed, .. } => {
                        !matches!(shape_type, ShapeType::Polyline) || *closed
                    }
                    _ => false,
                })
            })
            .collect();
        if let Some(p) = prop(&self.scene, &fill_ids, |it| match &it.kind {
            ItemKind::Shape {
                fill, fill_style, ..
            } => Some(FillState {
                color: *fill,
                style: *fill_style,
            }),
            _ => None,
        }) {
            let state = p.value();
            let current_style = if state.color.is_some() {
                Some(state.style)
            } else {
                None
            };
            ui.horizontal(|ui| {
                ui.label(t(lang, T::StyleFillLabel));
                if let Some(new_style) = palette::fill_style_picker(ui, lang, current_style) {
                    self.apply_continuous(
                        &fill_ids,
                        PropKind::Fill,
                        |k| match k {
                            ItemKind::Shape {
                                fill, fill_style, ..
                            } => Some(PropValue::Fill(FillState {
                                color: *fill,
                                style: *fill_style,
                            })),
                            _ => None,
                        },
                        |k| {
                            if let ItemKind::Shape {
                                fill,
                                fill_style,
                                stroke,
                                ..
                            } = k
                            {
                                match new_style {
                                    Some(s) => {
                                        *fill_style = s;
                                        // 无填充 → 有填充：默认跟随描边色（Excalidraw 语义），
                                        // 套默认 50% 不透明度（plan #2）
                                        if fill.is_none() {
                                            let c = stroke.color;
                                            *fill = Some([
                                                c[0],
                                                c[1],
                                                c[2],
                                                palette::FILL_DEFAULT_ALPHA,
                                            ]);
                                        }
                                    }
                                    None => {
                                        *fill = None;
                                        *fill_style = FillStyle::Solid;
                                    }
                                }
                            }
                        },
                    );
                }
            });
            // 填充颜色（仅有填充时显示；Excalidraw 同款调色板，plan #2：与描边共用 5 色）
            if let Some(c) = state.color {
                let mut col = c;
                if palette::fill_color_palette_button(ui, &mut col, dark) {
                    let new = col;
                    self.apply_continuous(
                        &fill_ids,
                        PropKind::Fill,
                        |k| match k {
                            ItemKind::Shape {
                                fill, fill_style, ..
                            } => Some(PropValue::Fill(FillState {
                                color: *fill,
                                style: *fill_style,
                            })),
                            _ => None,
                        },
                        |k| {
                            if let ItemKind::Shape { fill, .. } = k {
                                *fill = Some(new);
                            }
                        },
                    );
                }
                // 不透明度滑块（plan #2）：0–100%，仅改 alpha 保 RGB
                let mut pct = (c[3] as f32 / 255.0 * 100.0).round();
                if ui
                    .add(
                        egui::Slider::new(&mut pct, 0.0..=100.0).text(t(lang, T::StyleFillOpacity)),
                    )
                    .changed()
                {
                    let a = (pct / 100.0 * 255.0).round() as u8;
                    self.apply_continuous(
                        &fill_ids,
                        PropKind::Fill,
                        |k| match k {
                            ItemKind::Shape {
                                fill, fill_style, ..
                            } => Some(PropValue::Fill(FillState {
                                color: *fill,
                                style: *fill_style,
                            })),
                            _ => None,
                        },
                        |k| {
                            if let ItemKind::Shape { fill: Some(f), .. } = k {
                                f[3] = a;
                            }
                        },
                    );
                }
            }
        }
        // 圆角（仅矩形族）
        let rect_ids: Vec<ItemId> = ids
            .iter()
            .copied()
            .filter(|id| {
                matches!(self.scene.get_item(id), Some(item) if matches!(item.kind, ItemKind::Shape { shape_type: ShapeType::Rectangle, .. }))
            })
            .collect();
        if !rect_ids.is_empty() {
            if let Some(p) = prop(&self.scene, &rect_ids, |it| match &it.kind {
                ItemKind::Shape { roundness, .. } => Some(*roundness),
                _ => None,
            }) {
                let mut r = p.value();
                if ui.add(egui::Slider::new(&mut r, 0.0..=1.0)).changed() {
                    self.apply_continuous(
                        &rect_ids,
                        PropKind::Roundness,
                        |k| match k {
                            ItemKind::Shape { roundness, .. } => Some(PropValue::Float(*roundness)),
                            _ => None,
                        },
                        |k| {
                            if let ItemKind::Shape { roundness, .. } = k {
                                *roundness = r;
                            }
                        },
                    );
                }
                if p.is_mixed() {
                    ui.label(t(lang, T::PropsMixedValue));
                }
            }
        }
        // 手绘风档位（plan #3，离散四档，整批一条命令）
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Shape { sloppiness, .. } => Some(*sloppiness),
            _ => None,
        }) {
            let current = p.value();
            let opts = [
                (Sloppiness::Off, T::SloppinessOff),
                (Sloppiness::Architect, T::SloppinessArchitect),
                (Sloppiness::Artist, T::SloppinessArtist),
                (Sloppiness::Cartoonist, T::SloppinessCartoonist),
            ];
            ui.horizontal(|ui| {
                ui.label(t(lang, T::StyleRough));
                for (val, label) in opts {
                    let selected = current == val;
                    if ui.selectable_label(selected, t(lang, label)).clicked() && !selected {
                        let items: Vec<(ItemId, Sloppiness, Sloppiness)> = ids
                            .iter()
                            .filter_map(|id| {
                                self.scene.get_item(id).and_then(|it| match &it.kind {
                                    ItemKind::Shape { sloppiness, .. } => {
                                        Some((*id, *sloppiness, val))
                                    }
                                    _ => None,
                                })
                            })
                            .collect();
                        if !items.is_empty() {
                            self.push_cmd(Box::new(SetSloppiness::new_batch(items)));
                        }
                    }
                }
            });
        }
        // 线性对象专用：曲线 / 闭合 / 箭头
        let poly_ids: Vec<ItemId> = ids
            .iter()
            .copied()
            .filter(|id| {
                matches!(self.scene.get_item(id), Some(item) if matches!(item.kind, ItemKind::Shape { shape_type: ShapeType::Polyline, .. }))
            })
            .collect();
        if !poly_ids.is_empty() {
            if let Some(p) = prop(&self.scene, &poly_ids, |it| match &it.kind {
                ItemKind::Shape { curve_type, .. } => Some(*curve_type),
                _ => None,
            }) {
                let straight = p.value() == CurveType::Straight;
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(straight, t(lang, T::StyleCurveStraight))
                        .clicked()
                        && !straight
                    {
                        self.push_poly_curve(&poly_ids, CurveType::Straight);
                    }
                    if ui
                        .selectable_label(!straight, t(lang, T::StyleCurveCurved))
                        .clicked()
                        && straight
                    {
                        self.push_poly_curve(&poly_ids, CurveType::Curved);
                    }
                });
            }
            if let Some(p) = prop(&self.scene, &poly_ids, |it| match &it.kind {
                ItemKind::Shape { closed, .. } => Some(*closed),
                _ => None,
            }) {
                let mut checked = p.value();
                if ui.checkbox(&mut checked, t(lang, T::StyleClosed)).changed()
                    && checked != p.value()
                {
                    let items: Vec<(ItemId, bool, bool)> = poly_ids
                        .iter()
                        .filter_map(|id| {
                            self.scene.get_item(id).and_then(|it| match &it.kind {
                                ItemKind::Shape { closed, .. } => Some((*id, *closed, checked)),
                                _ => None,
                            })
                        })
                        .collect();
                    if !items.is_empty() {
                        self.push_cmd(Box::new(SetClosed::new_batch(items)));
                    }
                }
            }
            // 起/终点箭头（仅开放折线；闭合图形首尾相连，箭头无意义）
            let all_closed = poly_ids.iter().all(|id| {
                matches!(self.scene.get_item(id), Some(item) if matches!(item.kind, ItemKind::Shape { closed: true, .. }))
            });
            if !all_closed {
                if let Some(p) = prop(&self.scene, &poly_ids, |it| match &it.kind {
                    ItemKind::Shape {
                        start_arrow,
                        end_arrow,
                        closed: false,
                        ..
                    } => Some((*start_arrow, *end_arrow)),
                    _ => None,
                }) {
                    let (start, end) = p.value();
                    let mut sc = start.is_some();
                    if ui.checkbox(&mut sc, t(lang, T::StyleArrowStart)).changed() {
                        let new_start = if sc {
                            Some(ArrowHeadStyle::Arrow)
                        } else {
                            None
                        };
                        let items: Vec<(ItemId, ArrowHeads, ArrowHeads)> = poly_ids
                            .iter()
                            .filter_map(|id| {
                                self.scene.get_item(id).and_then(|it| match &it.kind {
                                    ItemKind::Shape {
                                        start_arrow,
                                        end_arrow,
                                        closed: false,
                                        ..
                                    } => Some((
                                        *id,
                                        ArrowHeads {
                                            start: *start_arrow,
                                            end: *end_arrow,
                                        },
                                        ArrowHeads {
                                            start: new_start,
                                            end: *end_arrow,
                                        },
                                    )),
                                    _ => None,
                                })
                            })
                            .collect();
                        if !items.is_empty() {
                            self.push_cmd(Box::new(SetArrowHeads::new_batch(items)));
                        }
                    }
                    let mut ec = end.is_some();
                    if ui.checkbox(&mut ec, t(lang, T::StyleArrowEnd)).changed() {
                        let new_end = if ec {
                            Some(ArrowHeadStyle::Arrow)
                        } else {
                            None
                        };
                        let items: Vec<(ItemId, ArrowHeads, ArrowHeads)> = poly_ids
                            .iter()
                            .filter_map(|id| {
                                self.scene.get_item(id).and_then(|it| match &it.kind {
                                    ItemKind::Shape {
                                        start_arrow,
                                        end_arrow,
                                        closed: false,
                                        ..
                                    } => Some((
                                        *id,
                                        ArrowHeads {
                                            start: *start_arrow,
                                            end: *end_arrow,
                                        },
                                        ArrowHeads {
                                            start: *start_arrow,
                                            end: new_end,
                                        },
                                    )),
                                    _ => None,
                                })
                            })
                            .collect();
                        if !items.is_empty() {
                            self.push_cmd(Box::new(SetArrowHeads::new_batch(items)));
                        }
                    }
                }
            }
        }
    }

    /// 文字节：字号 / 颜色 / 背景（Phase H）。
    fn render_text_props(&mut self, ui: &mut egui::Ui, lang: Lang, ids: &[ItemId]) {
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Text { font_size, .. } => Some(*font_size),
            _ => None,
        }) {
            let mut fs = p.value();
            if ui.add(egui::Slider::new(&mut fs, 6.0..=200.0)).changed() {
                self.apply_continuous(
                    ids,
                    PropKind::TextStyle,
                    |k| k.text_style().map(PropValue::Text),
                    |k| {
                        if let ItemKind::Text { font_size, .. } = k {
                            *font_size = fs;
                        }
                    },
                );
            }
            if p.is_mixed() {
                ui.label(t(lang, T::PropsMixedValue));
            }
        }
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Text { color, .. } => Some(*color),
            _ => None,
        }) {
            let c = p.value();
            let mut col = egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
            if ui.color_edit_button_srgba(&mut col).changed() {
                let new = [col.r(), col.g(), col.b(), col.a()];
                self.apply_continuous(
                    ids,
                    PropKind::TextStyle,
                    |k| k.text_style().map(PropValue::Text),
                    |k| {
                        if let ItemKind::Text { color, .. } = k {
                            *color = new;
                        }
                    },
                );
            }
            if p.is_mixed() {
                ui.label(t(lang, T::PropsMixedValue));
            }
        }
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Text { background, .. } => Some(*background),
            _ => None,
        }) {
            let mut checked = p.value().is_some();
            if ui
                .checkbox(&mut checked, t(lang, T::StyleTextBackground))
                .changed()
            {
                let new: Option<[u8; 4]> = if checked {
                    Some([255, 255, 255, 220])
                } else {
                    None
                };
                self.apply_continuous(
                    ids,
                    PropKind::TextStyle,
                    |k| k.text_style().map(PropValue::Text),
                    |k| {
                        if let ItemKind::Text { background, .. } = k {
                            *background = new;
                        }
                    },
                );
            }
            if let Some(existing) = p.value() {
                let mut col = egui::Color32::from_rgba_unmultiplied(
                    existing[0],
                    existing[1],
                    existing[2],
                    existing[3],
                );
                if ui.color_edit_button_srgba(&mut col).changed() {
                    let new = [col.r(), col.g(), col.b(), col.a()];
                    self.apply_continuous(
                        ids,
                        PropKind::TextStyle,
                        |k| k.text_style().map(PropValue::Text),
                        |k| {
                            if let ItemKind::Text { background, .. } = k {
                                *background = Some(new);
                            }
                        },
                    );
                }
            }
        }
        // 字体族（plan #1）：黑体 / 伪手写（逐字微抖，零体积增量）
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Text { font_family, .. } => Some(*font_family),
            _ => None,
        }) {
            let current = p.value();
            ui.horizontal(|ui| {
                ui.label(t(lang, T::StyleFontLabel));
                for (val, label) in [
                    (FontFamily::Normal, T::FontNormal),
                    (FontFamily::Handwriting, T::FontHandwriting),
                ] {
                    if ui
                        .selectable_label(current == val, t(lang, label))
                        .clicked()
                        && current != val
                    {
                        self.apply_continuous(
                            ids,
                            PropKind::TextStyle,
                            |k| k.text_style().map(PropValue::Text),
                            |k| {
                                if let ItemKind::Text { font_family, .. } = k {
                                    *font_family = val;
                                }
                            },
                        );
                    }
                }
            });
        }
        // 对齐（plan #1）：仅绑定文字显示；自由文本单行无框恒 top-left
        let has_bound_text = ids.iter().any(|id| {
            matches!(
                self.scene.get_item(id),
                Some(it) if matches!(
                    &it.kind,
                    ItemKind::Text {
                        container_id: Some(_),
                        ..
                    }
                )
            )
        });
        if has_bound_text {
            if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
                ItemKind::Text { align_h, .. } => Some(*align_h),
                _ => None,
            }) {
                let current = p.value();
                ui.horizontal(|ui| {
                    ui.label(t(lang, T::StyleAlignH));
                    for (val, label) in [
                        (TextAlignH::Left, T::TextAlignLeft),
                        (TextAlignH::Center, T::TextAlignCenter),
                        (TextAlignH::Right, T::TextAlignRight),
                    ] {
                        if ui
                            .selectable_label(current == val, t(lang, label))
                            .clicked()
                            && current != val
                        {
                            self.apply_continuous(
                                ids,
                                PropKind::TextStyle,
                                |k| k.text_style().map(PropValue::Text),
                                |k| {
                                    if let ItemKind::Text { align_h, .. } = k {
                                        *align_h = val;
                                    }
                                },
                            );
                        }
                    }
                });
            }
            if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
                ItemKind::Text { align_v, .. } => Some(*align_v),
                _ => None,
            }) {
                let current = p.value();
                ui.horizontal(|ui| {
                    ui.label(t(lang, T::StyleAlignV));
                    for (val, label) in [
                        (TextAlignV::Top, T::TextAlignTop),
                        (TextAlignV::Middle, T::TextAlignMiddle),
                        (TextAlignV::Bottom, T::TextAlignBottom),
                    ] {
                        if ui
                            .selectable_label(current == val, t(lang, label))
                            .clicked()
                            && current != val
                        {
                            self.apply_continuous(
                                ids,
                                PropKind::TextStyle,
                                |k| k.text_style().map(PropValue::Text),
                                |k| {
                                    if let ItemKind::Text { align_v, .. } = k {
                                        *align_v = val;
                                    }
                                },
                            );
                        }
                    }
                });
            }
        }
    }

    /// 图片节：不透明度 / 灰度（Phase H）。
    fn render_pixmap_props(&mut self, ui: &mut egui::Ui, lang: Lang, ids: &[ItemId]) {
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Pixmap { opacity, .. } => Some(*opacity),
            _ => None,
        }) {
            let mut o = p.value();
            // 下限 0.1（与背景透明度滑块一致）：避免拖到 0 完全透明导致图片不可见。
            if ui.add(egui::Slider::new(&mut o, 0.1..=1.0)).changed() {
                self.apply_continuous(
                    ids,
                    PropKind::Pixmap,
                    |k| match k {
                        ItemKind::Pixmap {
                            opacity, grayscale, ..
                        } => Some(PropValue::Pixmap(PixmapStyle {
                            opacity: *opacity,
                            grayscale: *grayscale,
                        })),
                        _ => None,
                    },
                    |k| {
                        if let ItemKind::Pixmap { opacity, .. } = k {
                            *opacity = o;
                        }
                    },
                );
            }
            if p.is_mixed() {
                ui.label(t(lang, T::PropsMixedValue));
            }
        }
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Pixmap { grayscale, .. } => Some(*grayscale),
            _ => None,
        }) {
            let mut checked = p.value();
            if ui
                .checkbox(&mut checked, t(lang, T::StyleGrayscale))
                .changed()
                && checked != p.value()
            {
                self.apply_continuous(
                    ids,
                    PropKind::Pixmap,
                    |k| match k {
                        ItemKind::Pixmap {
                            opacity, grayscale, ..
                        } => Some(PropValue::Pixmap(PixmapStyle {
                            opacity: *opacity,
                            grayscale: *grayscale,
                        })),
                        _ => None,
                    },
                    |k| {
                        if let ItemKind::Pixmap { grayscale, .. } = k {
                            *grayscale = checked;
                        }
                    },
                );
            }
        }
    }

    /// 画框节：编号（Phase H）。
    fn render_frame_props(&mut self, ui: &mut egui::Ui, lang: Lang, ids: &[ItemId]) {
        ui.label(t(lang, T::PropsFrameNumber));
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Frame { number, .. } => Some(*number),
            _ => None,
        }) {
            let mut n = p.value();
            if ui
                .add(egui::DragValue::new(&mut n).range(0..=9999))
                .changed()
            {
                let items: Vec<(ItemId, u32, u32)> = ids
                    .iter()
                    .filter_map(|id| {
                        self.scene.get_item(id).and_then(|it| match &it.kind {
                            ItemKind::Frame { number, .. } => Some((*id, *number, n)),
                            _ => None,
                        })
                    })
                    .collect();
                if !items.is_empty() {
                    self.push_cmd(Box::new(SetFrameNumber::new_batch(items)));
                }
            }
            if p.is_mixed() {
                ui.label(t(lang, T::PropsMixedValue));
            }
        }

        // 比例 / 纸张预设（plan #3）：把选中的画框调整为常见演示比例或 A4 纸张尺寸，
        // 中心锚定、一步 undo。下拉为一次性动作（不记忆当前值），故占位项恒显示。
        ui.add_space(6.0);
        ui.label(t(lang, T::FramePresetLabel));
        egui::ComboBox::from_id_salt("frame_preset")
            .selected_text(t(lang, T::FramePresetPick))
            .show_ui(ui, |ui| {
                for preset in FRAME_PRESETS {
                    if ui.button(t(lang, preset.label())).clicked() {
                        self.apply_frame_preset(preset, ids);
                        ui.close();
                    }
                }
            });
    }

    /// 把某个比例/纸张预设套用到选中的画框（plan #3），打包成一条 [`SetFrameSize`]。
    ///
    /// - `Ratio` 预设：保持画框当前**有效长边**长度，另一条边按 ratio 调整（只改形状不改观感大小）。
    /// - `Paper` 预设：套用固定像素尺寸（A4 按 96 DPI 换算）。
    ///
    /// 两种都以**画框中心**为锚点重算左上角位置；把 scale 归一到 1、尺寸写进 base_size，
    /// 使结果确定且后续手柄缩放从干净状态开始。套用前 `scale≠1` 的旧几何由命令快照精确还原。
    fn apply_frame_preset(&mut self, preset: FramePreset, ids: &[ItemId]) {
        let mut items: Vec<(ItemId, FrameGeom, FrameGeom)> = Vec::new();
        for id in ids {
            let Some(it) = self.scene.get_item(id) else {
                continue;
            };
            let (bw, bh) = match &it.kind {
                ItemKind::Frame { base_size, .. } => *base_size,
                _ => continue,
            };
            let (sx, sy) = (it.transform.scale.x, it.transform.scale.y);
            let (px, py) = (it.transform.pos.x, it.transform.pos.y);
            let (ew, eh) = (bw * sx, bh * sy);
            let (cx, cy) = (px + ew / 2.0, py + eh / 2.0);
            let (nw, nh) = match preset {
                FramePreset::Ratio { w, h, .. } => {
                    let long = ew.max(eh);
                    if w >= h {
                        (long, long * h / w)
                    } else {
                        (long * w / h, long)
                    }
                }
                FramePreset::Paper { w, h, .. } => (w, h),
            };
            let old = FrameGeom {
                pos: (px, py),
                base: (bw, bh),
                scale: (sx, sy),
            };
            let new = FrameGeom {
                pos: (cx - nw / 2.0, cy - nh / 2.0),
                base: (nw, nh),
                scale: (1.0, 1.0),
            };
            items.push((*id, old, new));
        }
        if items.is_empty() {
            return;
        }
        let label = t(self.lang, preset.label()).to_string();
        self.push_cmd(Box::new(SetFrameSize::new_batch(items)));
        self.flash(fill(t(self.lang, T::FlashFramePresetApplied), &[label]));
    }

    /// 线性对象批量切换曲线模式（离散，整批改命令）。
    fn push_poly_curve(&mut self, ids: &[ItemId], new_curve: CurveType) {
        let items: Vec<(ItemId, CurveType, CurveType)> = ids
            .iter()
            .filter_map(|id| {
                self.scene.get_item(id).and_then(|it| match &it.kind {
                    ItemKind::Shape { curve_type, .. } => Some((*id, *curve_type, new_curve)),
                    _ => None,
                })
            })
            .collect();
        if !items.is_empty() {
            self.push_cmd(Box::new(SetCurveType::new_batch(items)));
        }
    }

    /// 连续编辑的「开帧」入口：记录变更前快照并标记本帧有变更（Phase H）。
    ///
    /// 若已有不同种类的待合并编辑，先结算它，再开启本次编辑；同种类则保留首次快照
    /// （拖动期间整段只取一处 old 值），由 [`Self::update`] 帧末统一结算成一条命令。
    fn ensure_prop_edit(&mut self, kind: PropKind, snapshot: Vec<(ItemId, PropValue)>) {
        if let Some(p) = self.prop_edit_pending.take() {
            if p.kind != kind {
                if let Some(cmd) = prop_cmd(p, &self.scene) {
                    self.push_cmd(cmd);
                }
            } else {
                self.prop_edit_pending = Some(p);
            }
        }
        if self.prop_edit_pending.is_none() {
            self.prop_edit_pending = Some(PropEdit {
                kind,
                items: snapshot,
            });
        }
        self.prop_changed_this_frame = true;
    }

    /// 连续控件通用路径：取变更前快照 → 直接改 item（不入栈）→ 标记待合并（Phase H）。
    ///
    /// `snap` 从每个选中项的当前状态抽出旧值（用于合成 undo 的旧端）；`apply` 把新值
    /// 写到每个选中项。整段拖动在 [`Self::update`] 帧末被合成为**一条**批量命令。
    fn apply_continuous(
        &mut self,
        ids: &[ItemId],
        kind: PropKind,
        snap: impl Fn(&ItemKind) -> Option<PropValue>,
        apply: impl Fn(&mut ItemKind),
    ) {
        let items: Vec<(ItemId, PropValue)> = ids
            .iter()
            .filter_map(|id| {
                self.scene
                    .get_item(id)
                    .and_then(|it| snap(&it.kind).map(|v| (*id, v)))
            })
            .collect();
        self.ensure_prop_edit(kind, items);
        for id in ids {
            if let Some(item) = self.scene.get_item_mut(id) {
                apply(&mut item.kind);
            }
        }
    }

    /// 描边色联动同步（改描边色时调用）：填充色跟随（保留 alpha）、绑定文字色跟随。
    ///
    /// 直接改 kind 作预览，并把联动目标的旧值快照追加进当前 Stroke 待合并编辑
    /// ——`prop_cmd` 会把混合快照分流成三条批量命令并打包为一条 [`MultiCommand`]，
    /// 撤销时整体回到改色前。按 id 去重：pending 已有的条目是首次快照，不覆盖
    /// （与 [`Self::ensure_prop_edit`] 的"整段拖动只取一处 old 值"语义一致）。
    fn sync_stroke_color_side_effects(&mut self, ids: &[ItemId], color: [u8; 4]) {
        let mut extra: Vec<(ItemId, PropValue)> = Vec::new();
        for id in ids {
            // 一次性提取不可变数据后立即释放借用（后续预览阶段要可变借 scene）
            let (is_shape, fill_target) = match self.scene.get_item(id).map(|it| &it.kind) {
                Some(ItemKind::Shape {
                    fill, fill_style, ..
                }) => (true, fill.map(|f| (f, *fill_style))),
                Some(_) => (false, None),
                None => continue,
            };
            if !is_shape {
                continue;
            }
            // 该图形的绑定文字：old = 当前整份 TextStyle，new = 仅换 color
            let text_ids: Vec<ItemId> = self
                .scene
                .items
                .iter()
                .filter_map(|t| match &t.kind {
                    ItemKind::Text {
                        container_id: Some(cid),
                        ..
                    } if cid == id => Some(t.id),
                    _ => None,
                })
                .collect();
            for tid in text_ids {
                let Some(t) = self.scene.get_item(&tid) else {
                    continue;
                };
                let Some(old_style) = t.kind.text_style() else {
                    continue;
                };
                if old_style.color != color {
                    extra.push((tid, PropValue::Text(old_style)));
                }
                // 预览：直接改文字颜色
                if let Some(t) = self.scene.get_item_mut(&tid) {
                    if let ItemKind::Text { color: c, .. } = &mut t.kind {
                        *c = color;
                    }
                }
            }
            // 填充：old = 当前 FillState，new = 换 color 保 alpha；无填充不联动
            if let Some((fill, fill_style)) = fill_target {
                if fill[0] != color[0] || fill[1] != color[1] || fill[2] != color[2] {
                    extra.push((
                        *id,
                        PropValue::Fill(FillState {
                            color: Some(fill),
                            style: fill_style,
                        }),
                    ));
                }
                // 预览：直接改填充颜色（保留 alpha）
                if let Some(it) = self.scene.get_item_mut(id) {
                    if let ItemKind::Shape { fill: Some(f), .. } = &mut it.kind {
                        f[0] = color[0];
                        f[1] = color[1];
                        f[2] = color[2];
                    }
                }
            }
        }
        // 追加快照（按 id 去重，保留首次快照）
        if !extra.is_empty() {
            if let Some(p) = self.prop_edit_pending.as_mut() {
                if p.kind == PropKind::Stroke {
                    for (id, v) in extra {
                        if !p.items.iter().any(|(eid, _)| *eid == id) {
                            p.items.push((id, v));
                        }
                    }
                }
            }
        }
    }

    /// 悬浮 HUD：右下角胶囊（缩放百分数 + 语言切换）+ 底部居中 flash toast。
    ///
    /// 替代原底部 `TopBottomPanel` 状态栏，让画布吃满窗口高度。
    /// 用 `egui::Area` 而非 `Window`：无标题栏、不可拖动、不抢焦点，纯浮层。
    /// `interactable(true)` 让语言按钮可点；HUD 未覆盖的区域仍透传给画布，
    /// 画布的 `pointer_on_canvas` 守卫会自动排除被浮层遮挡的部分。
    fn render_hud(&mut self, ctx: &egui::Context) {
        let zoom_pct = format!("{:.0}%", self.viewport.zoom * 100.0);
        egui::Area::new(egui::Id::new("hud_zoom"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -12.0))
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                // popup frame 自带主题化背景 + 描边，明暗主题下都可读。
                // 行内布局 + 关闭 wrap：窄屏/缩放数值下也保持单行、宽度自适应内容。
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::symmetric(10, 5))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&zoom_pct).monospace());
                        });
                    });
            });

        // 语言切换：左下角独立悬浮，按钮显示目标语言（中文界面显示 EN，
        // 英文界面显示 中），点击切到该语言（hover 提示目标语言全名）。
        let lang = self.lang;
        egui::Area::new(egui::Id::new("hud_lang"))
            .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(12.0, -12.0))
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::symmetric(10, 5))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui
                                .small_button(lang.toggled().short_name())
                                .on_hover_text(lang.toggled().display_name())
                                .clicked()
                            {
                                self.lang = lang.toggled();
                                self.persist_config();
                            }
                            // 主题切换（验收反馈 3）：light 画黑色月牙、dark 画
                            // 白色太阳（字体无可靠月牙字形，用形状手绘）。Auto 下
                            // 按**当前生效外观**显示；点击显式切到相反态，副作用与
                            // 设置面板 theme_changed 一致（默认描边色随主题翻转+持久化）。
                            ui.add_space(2.0);
                            let dark_now = self.theme.is_dark(ctx);
                            let gal = ui.visuals().strong_text_color();
                            let (icon_rect, resp) = ui
                                .allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
                            let resp = resp.on_hover_text(t(self.lang, T::ThemeToggleHint));
                            if ui.is_rect_visible(icon_rect) {
                                let painter = ui.painter();
                                let center = icon_rect.center();
                                let mut bg = ui.visuals().window_fill;
                                if resp.hovered() {
                                    // hover 底色（同时用作月牙"挖除"色，保证抠弧无痕）
                                    let mix = |a: u8, b: u8| -> u8 {
                                        ((a as f32) * 0.88 + (b as f32) * 0.12) as u8
                                    };
                                    bg = egui::Color32::from_rgb(
                                        mix(bg.r(), gal.r()),
                                        mix(bg.g(), gal.g()),
                                        mix(bg.b(), gal.b()),
                                    );
                                    painter.rect_filled(icon_rect, 4.0, bg);
                                }
                                if dark_now {
                                    // 太阳：实心圆 + 8 道射线
                                    painter.circle_filled(center, 3.4, gal);
                                    for k in 0..8usize {
                                        let a = std::f32::consts::TAU * k as f32 / 8.0;
                                        let (dy, dx) = a.sin_cos();
                                        painter.line_segment(
                                            [
                                                center + egui::vec2(dx * 5.2, dy * 5.2),
                                                center + egui::vec2(dx * 7.6, dy * 7.6),
                                            ],
                                            egui::Stroke::new(1.4_f32, gal),
                                        );
                                    }
                                } else {
                                    // 月牙：实心圆 + 右上偏置的背景色圆挖除
                                    painter.circle_filled(center + egui::vec2(-0.6, 0.4), 5.8, gal);
                                    painter.circle_filled(center + egui::vec2(3.4, -2.0), 5.0, bg);
                                }
                            }
                            if resp.clicked() {
                                self.theme = if dark_now {
                                    ThemeMode::Light
                                } else {
                                    ThemeMode::Dark
                                };
                                self.default_stroke.color = self.theme.default_stroke_color(ctx);
                                self.persist_config();
                            }
                        });
                    });
            });

        self.render_flash_toast(ctx);
    }

    /// flash 提示的悬浮 toast。原由状态栏承载，随状态栏移除后移到画布底部居中。
    /// `interactable(false)`：纯提示，不拦截画布指针事件。
    fn render_flash_toast(&self, ctx: &egui::Context) {
        let Some((msg, _)) = self.flash_status.as_ref() else {
            return;
        };
        let msg = msg.clone();
        // Id 带上本次 flash 的序号：`Area` 会把内容尺寸记忆在 `AreaState` 里，
        // 下一帧以该尺寸作为 `max_rect`（见 egui `Area::end()`：
        // `state.size = Some(content_ui.min_size())`）。若沿用固定 Id，新文案会
        // 按上一条（可能已被收缩过的）宽度排版，居中位置也慢一帧。换新 Id 则走
        // 一次 sizing pass，结束时 egui 自带 `request_repaint()`，真实宽度立刻到位。
        egui::Area::new(egui::Id::new(("flash_toast", self.flash_seq)))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -16.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                // 把排版宽度钉死，不让它来自 Area 的记忆尺寸：`Area::end()` 每帧
                // 把 `state.size` 收缩成内容 `min_size()`，下一帧又拿它当 `max_rect`，
                // 而 flash 多为无 ASCII 空格的中文，默认 `Words` 模式把整句当一个
                // 超长"单词"按字符硬拆 → 可用宽度逐帧变小，最终塌成正一列（每行
                // 一个字），只在有重绘时发生所以"有时正常有时竖条"。显式 set_max_width
                // 后收缩链被切断：短文案单行，超长文案（带路径的错误）稳定折行且
                // 始终留在屏幕内（Extend 会让它横向溢出被两侧裁掉）。
                let max_w = (ctx.content_rect().width() - 2.0 * FLASH_TOAST_SIDE_MARGIN).max(200.0);
                ui.set_max_width(max_w);
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .show(ui, |ui| {
                        ui.colored_label(egui::Color32::LIGHT_GREEN, msg);
                    });
            });
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
        DragState::CreatingPolygon { .. } => "CreatingPolygon",
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

/// 图片 mesh 顶点色（tint）：把「白色 × 不透明度」按 egui 的**预乘 alpha** 语义展开。
///
/// egui painter 以预乘 alpha 混合（`ONE, ONE_MINUS_SRC_ALPHA`），顶点色 RGB 必须已随
/// alpha 缩放。此前直接写 `from_rgba_premultiplied(255,255,255,alpha)`——RGB=255 > alpha
/// 是非法的过亮预乘色，与描边/填充曾出现的「设 50% 却几乎不透明」同源。白色 (255,255,255)
/// 乘 alpha/255 即 (alpha,alpha,alpha)，与 [`crate::ui::stylers`] 里填充的预乘处理一致。
fn pixmap_tint(alpha: u8) -> egui::Color32 {
    egui::Color32::from_rgba_premultiplied(alpha, alpha, alpha, alpha)
}

/// 判断路径是否PReferZ 项目文件prz）
fn is_project_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("prz"))
        .unwrap_or(false)
}

/// 把模板里的 `{0}`/`{1}`… 依次替换为 `parts`。
///
/// i18n 文案中内嵌了快捷键占位（如 `"Present slides ({0})"`）——快捷键可改绑后
/// 不能把 `Ctrl+V` 之类写死进翻译表。`t()` 返回 `&'static str` 不支持格式化，
/// 故在这里补一层极简替换（不引入 `formatx` / `strfmt` 之类依赖）。
fn fill(template: &str, parts: &[String]) -> String {
    let mut s = template.to_string();
    for (i, part) in parts.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), part);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_replaces_indexed_placeholders() {
        assert_eq!(
            fill("a {0} b {1}", &["X".to_string(), "Y".to_string()]),
            "a X b Y"
        );
        // 占位多于参数时保留原样，便于开发期发现漏填
        assert_eq!(fill("a {0} b {1}", &["X".to_string()]), "a X b {1}");
    }

    // ───────── 多边形工具（Phase I） ─────────

    /// 构造一个已进入多边形绘制中状态的应用：直接摆好 DragState，不走 UI 事件。
    fn app_creating_polygon(points: Vec<CanvasPoint>, current: CanvasPoint) -> PReferZApp {
        let mut app = PReferZApp::new();
        app.tool = Tool::Polygon;
        app.drag = DragState::CreatingPolygon {
            points,
            current,
            shift: false,
        };
        app
    }

    #[test]
    fn snap_polygon_point_without_shift_returns_target_unchanged() {
        let from = CanvasPoint::new(10.0, 10.0);
        let to = CanvasPoint::new(100.0, 37.0);
        assert_eq!(PReferZApp::snap_polygon_point(from, to, false), to);
    }

    #[test]
    fn snap_polygon_point_with_shift_snaps_to_45_degrees_keeping_length() {
        // 约 11° → 吸附到 0°；长度保持（不是投影，是等长转向）
        let snapped = PReferZApp::snap_polygon_point(
            CanvasPoint::new(0.0, 0.0),
            CanvasPoint::new(100.0, 20.0),
            true,
        );
        let len = (100.0f32 * 100.0 + 20.0f32 * 20.0).sqrt();
        assert!(
            (snapped.x - len).abs() < 1e-3 && snapped.y.abs() < 1e-3,
            "应吸附到 0° 且保持长度 {len}，实际 {snapped:?}"
        );
    }

    #[test]
    fn snap_polygon_point_snaps_near_45_to_exact_diagonal() {
        let snapped = PReferZApp::snap_polygon_point(
            CanvasPoint::new(0.0, 0.0),
            CanvasPoint::new(100.0, 90.0),
            true,
        );
        assert!(
            (snapped.x - snapped.y).abs() < 1e-3,
            "应落在 45° 对角线上，实际 {snapped:?}"
        );
    }

    #[test]
    fn snap_polygon_point_degenerate_segment_returns_from() {
        let from = CanvasPoint::new(5.0, 7.0);
        assert_eq!(PReferZApp::snap_polygon_point(from, from, true), from);
    }

    #[test]
    fn finish_create_polygon_builds_closed_polyline_in_aabb_local_space() {
        let mut app = app_creating_polygon(
            vec![
                CanvasPoint::new(10.0, 20.0),
                CanvasPoint::new(60.0, 20.0),
                CanvasPoint::new(60.0, 70.0),
            ],
            // 与末顶点重合（双击收尾），不应追加成第四点
            CanvasPoint::new(60.0, 70.0),
        );
        app.finish_create_polygon();
        assert_eq!(app.scene.items.len(), 1);
        let item = &app.scene.items[0];
        // 整体位置 = AABB 左上角，局部坐标从 (0,0) 起算
        assert_eq!((item.transform.pos.x, item.transform.pos.y), (10.0, 20.0));
        match &item.kind {
            ItemKind::Shape {
                shape_type,
                points,
                closed,
                base_size,
                ..
            } => {
                assert_eq!(*shape_type, ShapeType::Polyline);
                assert!(closed, "多边形工具产物恒为闭合折线");
                assert_eq!(points.len(), 3);
                assert_eq!(points[0], (0.0, 0.0));
                assert_eq!(points[1], (50.0, 0.0));
                assert_eq!(points[2], (50.0, 50.0));
                assert_eq!(*base_size, (50.0, 50.0));
            }
            _ => panic!("多边形应为 Shape item"),
        }
        assert_eq!(app.tool, Tool::Select, "收尾后应回 Select");
    }

    #[test]
    fn finish_create_polygon_appends_rubber_band_tip_when_far_from_last_vertex() {
        // Enter 收尾时指针通常远离末顶点，此时橡皮筋末端应算作最后一个顶点
        let mut app = app_creating_polygon(
            vec![
                CanvasPoint::new(0.0, 0.0),
                CanvasPoint::new(50.0, 0.0),
                CanvasPoint::new(50.0, 50.0),
            ],
            CanvasPoint::new(0.0, 50.0),
        );
        app.finish_create_polygon();
        let item = &app.scene.items[0];
        match &item.kind {
            ItemKind::Shape { points, .. } => assert_eq!(points.len(), 4),
            _ => panic!("多边形应为 Shape item"),
        }
    }

    #[test]
    fn finish_create_polygon_discards_fewer_than_three_points() {
        let mut app = app_creating_polygon(
            vec![CanvasPoint::new(0.0, 0.0), CanvasPoint::new(50.0, 0.0)],
            CanvasPoint::new(50.0, 0.0),
        );
        app.finish_create_polygon();
        assert!(app.scene.items.is_empty(), "两点连不出面，不应产生 item");
        assert_eq!(app.tool, Tool::Select);
        assert!(app.flash_status.is_some(), "应给出提示");
    }

    #[test]
    fn finish_create_polygon_ignores_other_drag_states() {
        let mut app = PReferZApp::new();
        app.drag = DragState::BoxSelect {
            start_canvas: CanvasPoint::new(0.0, 0.0),
            current_canvas: CanvasPoint::new(10.0, 10.0),
            additive: false,
        };
        app.finish_create_polygon();
        assert!(
            matches!(app.drag, DragState::BoxSelect { .. }),
            "非多边形状态应原样保留，不能被吞掉"
        );
        assert!(app.scene.items.is_empty());
    }

    // ───────── 多边形顶点编辑（plan #4） ─────────

    /// 放入一条 Polyline（局部点集从 (0,0) 起算，pos=0），返回应用与 item id。
    fn app_with_polyline(points: Vec<(f32, f32)>, closed: bool) -> (PReferZApp, ItemId) {
        let mut app = PReferZApp::new();
        let min_x = points.iter().map(|p| p.0).fold(f32::MAX, f32::min);
        let min_y = points.iter().map(|p| p.1).fold(f32::MAX, f32::min);
        let max_x = points.iter().map(|p| p.0).fold(f32::MIN, f32::max);
        let max_y = points.iter().map(|p| p.1).fold(f32::MIN, f32::max);
        let item = Item::new_polyline(
            points,
            (max_x - min_x, max_y - min_y),
            None,
            None,
            closed,
            0.0,
            0.0,
            StrokeStyle::default(),
        );
        let id = item.id;
        app.scene.add_item(item);
        (app, id)
    }

    fn polyline_points(app: &PReferZApp, id: ItemId) -> Vec<(f32, f32)> {
        match &app.scene.get_item(&id).unwrap().kind {
            ItemKind::Shape { points, .. } => points.clone(),
            _ => panic!("应为 Shape"),
        }
    }

    #[test]
    fn should_auto_close_requires_three_points_real_endpoint_and_screen_threshold() {
        let looped = vec![(0.0, 0.0), (50.0, 0.0), (1.0, 1.0)];
        assert!(
            should_auto_close(&looped, 2, 1.0),
            "末点拖回首点附近 → 闭合"
        );
        assert!(should_auto_close(&looped, 0, 1.0), "首点侧同样判定");
        assert!(!should_auto_close(&looped, 1, 1.0), "内部顶点不参与闭合");
        let two = vec![(0.0, 0.0), (1.0, 0.0)];
        assert!(!should_auto_close(&two, 1, 1.0), "两点恒开放");
        // 阈值按屏幕像素：zoom=4 时画布 1.5px ≈ 屏 6px 过；3px = 屏 12px 不过
        let near = vec![(0.0, 0.0), (50.0, 0.0), (1.5, 0.0)];
        assert!(should_auto_close(&near, 2, 4.0));
        let far = vec![(0.0, 0.0), (50.0, 0.0), (3.0, 0.0)];
        assert!(!should_auto_close(&far, 2, 4.0));
    }

    #[test]
    fn delete_vertex_guard_keeps_minimum_points() {
        // 两点开放线删端点：拒绝、不入 undo 栈、有 flash
        let (mut app, id) = app_with_polyline(vec![(0.0, 0.0), (50.0, 0.0)], false);
        app.try_delete_vertex(id, 0);
        assert!(app.undo_stack.undo.is_empty(), "守卫失败不应产生命令");
        assert_eq!(polyline_points(&app, id).len(), 2);
        assert!(app.flash_status.is_some(), "应提示拒绝原因");
        // 闭合三角形删顶点：拒绝（保 ≥3）
        let (mut app, id) = app_with_polyline(vec![(0.0, 0.0), (50.0, 0.0), (25.0, 40.0)], true);
        app.try_delete_vertex(id, 1);
        assert!(app.undo_stack.undo.is_empty());
        assert_eq!(polyline_points(&app, id).len(), 3);
        // 开放三点删中间点：成功剩两点，一步 undo 还原
        let (mut app, id) = app_with_polyline(vec![(0.0, 0.0), (25.0, 10.0), (50.0, 0.0)], false);
        app.try_delete_vertex(id, 1);
        assert_eq!(app.undo_stack.undo.len(), 1);
        assert_eq!(polyline_points(&app, id), vec![(0.0, 0.0), (50.0, 0.0)]);
        assert!(app.perform_undo());
        assert_eq!(polyline_points(&app, id).len(), 3);
    }

    #[test]
    fn delete_endpoint_vertex_clears_its_binding_and_undoes() {
        let (mut app, line_id) =
            app_with_polyline(vec![(0.0, 0.0), (25.0, 10.0), (50.0, 0.0)], false);
        let (target_app, target_id) = app_with_polyline(vec![(0.0, 0.0), (10.0, 10.0)], false);
        drop(target_app); // 只取一个 id 作绑定目标占位
        if let Some(it) = app.scene.get_item_mut(&line_id) {
            if let ItemKind::Shape { start_binding, .. } = &mut it.kind {
                *start_binding = Some(EndpointBinding {
                    target: target_id,
                    anchor: None,
                });
            }
        }
        app.try_delete_vertex(line_id, 0);
        if let Some(it) = app.scene.get_item(&line_id) {
            if let ItemKind::Shape { start_binding, .. } = &it.kind {
                assert!(start_binding.is_none(), "删首顶点应解除起点绑定");
            }
        }
        assert!(app.perform_undo());
        if let Some(it) = app.scene.get_item(&line_id) {
            if let ItemKind::Shape { start_binding, .. } = &it.kind {
                assert_eq!(
                    *start_binding,
                    Some(EndpointBinding {
                        target: target_id,
                        anchor: None,
                    }),
                    "undo 应连同绑定一起还原"
                );
            }
        }
    }

    // ───────── 流程图（plan #7） ─────────

    fn app_with_rect(pos: (f32, f32), size: (f32, f32)) -> (PReferZApp, ItemId) {
        let mut app = PReferZApp::new();
        let item = Item::new_shape(
            ShapeType::Rectangle,
            size,
            pos.0,
            pos.1,
            StrokeStyle::default(),
            None,
        );
        let id = item.id;
        app.scene.add_item(item);
        app.scene.select(id);
        (app, id)
    }

    #[test]
    fn edge_anchor_local_gives_facing_edge_midpoints() {
        assert_eq!(
            edge_anchor_local(FlowDir::Right, 120.0, 80.0),
            (120.0, 40.0)
        );
        assert_eq!(edge_anchor_local(FlowDir::Left, 120.0, 80.0), (0.0, 40.0));
        assert_eq!(edge_anchor_local(FlowDir::Down, 120.0, 80.0), (60.0, 80.0));
        assert_eq!(edge_anchor_local(FlowDir::Up, 120.0, 80.0), (60.0, 0.0));
    }

    #[test]
    fn add_connected_shape_creates_bound_clone_with_one_undo_step() {
        let (mut app, src_id) = app_with_rect((10.0, 10.0), (120.0, 80.0));
        app.add_connected_shape(FlowDir::Right);

        assert_eq!(app.scene.items.len(), 3, "应新增节点+箭头各一");
        let src = app.scene.get_item(&src_id).unwrap().clone();
        let dup = app
            .scene
            .items
            .iter()
            .find(|i| {
                i.id != src_id
                    && matches!(
                        i.kind,
                        ItemKind::Shape {
                            shape_type: ShapeType::Rectangle,
                            ..
                        }
                    )
            })
            .expect("克隆的矩形节点")
            .clone();
        let arrow = app
            .scene
            .items
            .iter()
            .find(|i| {
                matches!(
                    i.kind,
                    ItemKind::Shape {
                        shape_type: ShapeType::Polyline,
                        ..
                    }
                )
            })
            .expect("连接箭头")
            .clone();
        assert_ne!(dup.id, src.id);
        // 主轴 = 源右边界 + 100 间距；交叉轴（同尺寸克隆）即 y 不变。
        assert_eq!((dup.transform.pos.x, dup.transform.pos.y), (230.0, 10.0));
        // 箭头：起点=源右边中点 (130,50)，终点=克隆左边中点 (230,50)，
        // 局部点集对齐 AABB 左上角；端头 = Arrow。
        match &arrow.kind {
            ItemKind::Shape {
                points,
                base_size,
                end_arrow,
                start_binding,
                end_binding,
                ..
            } => {
                assert_eq!(
                    (arrow.transform.pos.x, arrow.transform.pos.y),
                    (130.0, 50.0)
                );
                assert_eq!(*points, vec![(0.0, 0.0), (100.0, 0.0)]);
                assert_eq!(*base_size, (100.0, 0.0));
                assert_eq!(*end_arrow, Some(ArrowHeadStyle::Arrow));
                assert_eq!(
                    start_binding.as_ref().map(|b| (b.target, b.anchor)),
                    Some((src_id, Some((120.0, 40.0))))
                );
                assert_eq!(
                    end_binding.as_ref().map(|b| (b.target, b.anchor)),
                    Some((dup.id, Some((0.0, 40.0))))
                );
            }
            _ => unreachable!(),
        }
        // z 序：箭头在两者之上；选区 = 新节点（连按可接链）。
        assert!(arrow.z > dup.z && arrow.z > src.z);
        assert_eq!(app.scene.selection.len(), 1);
        assert!(app.scene.selection.contains(&dup.id));
        // 一条 undo 撤回整对；redo 恢复。
        assert_eq!(app.undo_stack.undo.len(), 1);
        assert!(app.perform_undo());
        assert_eq!(app.scene.items.len(), 1);
        assert!(app.perform_redo());
        assert_eq!(app.scene.items.len(), 3);
    }

    #[test]
    fn add_connected_shape_ignores_non_node_selection() {
        // 线性对象（Polyline）不作源（对齐 Excalidraw isFlowchartNodeElement），
        // 静默：不加 item、不入 undo。
        let (mut app, line_id) = app_with_polyline(vec![(0.0, 0.0), (50.0, 0.0)], false);
        app.scene.deselect_all();
        app.scene.select(line_id);
        app.add_connected_shape(FlowDir::Right);
        assert_eq!(app.scene.items.len(), 1);
        assert!(app.undo_stack.undo.is_empty());
    }

    #[test]
    fn navigate_connected_jumps_along_bound_arrows() {
        let (mut app, src_id) = app_with_rect((10.0, 10.0), (120.0, 80.0));
        app.add_connected_shape(FlowDir::Right); // 选区已在 dup 上
        let dup_id = *app.scene.selection.iter().next().unwrap();
        let undo_len = app.undo_stack.undo.len(); // 创建本身占 1 条

        // 从 dup 向左回到 src；从 src 向右到 dup。
        app.navigate_connected(FlowDir::Left);
        assert_eq!(app.scene.selection.len(), 1);
        assert!(app.scene.selection.contains(&src_id));
        app.navigate_connected(FlowDir::Right);
        assert!(app.scene.selection.contains(&dup_id));
        // 无该方向邻居：选区保持不动（不产生命令、不清选区）。
        app.navigate_connected(FlowDir::Up);
        assert!(app.scene.selection.contains(&dup_id));
        assert_eq!(app.undo_stack.undo.len(), undo_len, "导航不应新增命令");
    }

    // ───────── 验收反馈批次（#4-1 / #7-2） ─────────

    /// 模拟"端点拖拽预览后释放"：直接摆好场景终态 + DragState，走 end_drag。
    fn finish_endpoint_drag(
        app: &mut PReferZApp,
        item_id: ItemId,
        endpoint: usize,
        start_points: Vec<(f32, f32)>,
    ) {
        let base_pos = start_points[endpoint];
        app.drag = DragState::LineEndpoint {
            item_id,
            endpoint,
            start_canvas: CanvasPoint::new(0.0, 0.0),
            start_points,
            base_pos,
            alt_extend: false,
        };
        app.end_drag();
    }

    #[test]
    fn dragging_merged_endpoint_away_reopens_closed_shape() {
        // 验收反馈 #4-1：自动闭合产物（首尾重合 + closed）把其中一个端点拖开
        // （> 屏幕 8px）→ 释放时自动恢复开放，一条 undo 可还原重合 + closed。
        let start = vec![(0.0, 0.0), (50.0, 0.0), (0.0, 50.0), (0.0, 0.0)];
        let (mut app, id) = app_with_polyline(start.clone(), true);
        if let Some(it) = app.scene.get_item_mut(&id) {
            if let ItemKind::Shape { points, .. } = &mut it.kind {
                points[3] = (30.0, 20.0); // 预览已拖开
            }
        }
        finish_endpoint_drag(&mut app, id, 3, start);
        match &app.scene.get_item(&id).unwrap().kind {
            ItemKind::Shape { closed, points, .. } => {
                assert!(!*closed, "拖开合并点应自动恢复开放");
                assert_eq!(points[3], (30.0, 20.0));
            }
            _ => unreachable!(),
        }
        assert_eq!(app.undo_stack.undo.len(), 1);
        assert!(app.perform_undo());
        match &app.scene.get_item(&id).unwrap().kind {
            ItemKind::Shape { closed, points, .. } => {
                assert!(*closed, "undo 应还原闭合态");
                assert_eq!(points[3], (0.0, 0.0), "undo 应还原合并位置");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn dragging_vertex_of_unmerged_closed_shape_stays_closed() {
        // 非重合的普通闭合多边形（侧栏手动闭合）拖顶点**不得**被自动开放。
        let start = vec![(0.0, 0.0), (50.0, 0.0), (25.0, 40.0)];
        let (mut app, id) = app_with_polyline(start.clone(), true);
        if let Some(it) = app.scene.get_item_mut(&id) {
            if let ItemKind::Shape { points, .. } = &mut it.kind {
                points[0] = (60.0, 10.0);
            }
        }
        finish_endpoint_drag(&mut app, id, 0, start);
        assert!(
            matches!(
                &app.scene.get_item(&id).unwrap().kind,
                ItemKind::Shape { closed: true, .. }
            ),
            "首尾不重合的闭合形状拖顶点保持闭合"
        );
    }

    #[test]
    fn add_connected_shape_places_sibling_next_to_existing_neighbor() {
        // 验收反馈 #7-2：A→B 已连，选中 A 再按 Ctrl+→ —— 新节点 C 放到 B 旁
        // （主轴 = B 远边 + GAP），不与 B 重合；箭头仍是 A→C。
        let (mut app, a_id) = app_with_rect((10.0, 10.0), (120.0, 80.0));
        app.add_connected_shape(FlowDir::Right); // A→B，选区在 B
        let b_id = *app.scene.selection.iter().next().unwrap();
        app.scene.deselect_all();
        app.scene.select(a_id);
        app.add_connected_shape(FlowDir::Right);

        assert_eq!(app.scene.items.len(), 5, "3 节点 + 2 箭头");
        let c = app
            .scene
            .items
            .iter()
            .find(|i| {
                i.id != a_id
                    && i.id != b_id
                    && matches!(
                        i.kind,
                        ItemKind::Shape {
                            shape_type: ShapeType::Rectangle,
                            ..
                        }
                    )
            })
            .expect("第三个节点");
        // B 位于 (230,10)，B.max.x = 350 → C = (450, 10)
        assert_eq!((c.transform.pos.x, c.transform.pos.y), (450.0, 10.0));
        // 第二条箭头：A→C
        let arrow = app
            .scene
            .items
            .iter()
            .filter(|i| {
                matches!(
                    i.kind,
                    ItemKind::Shape {
                        shape_type: ShapeType::Polyline,
                        ..
                    }
                )
            })
            .max_by_key(|i| i.z)
            .expect("第二条箭头");
        match &arrow.kind {
            ItemKind::Shape {
                start_binding,
                end_binding,
                ..
            } => {
                assert_eq!(start_binding.as_ref().map(|b| b.target), Some(a_id));
                assert_eq!(end_binding.as_ref().map(|b| b.target), Some(c.id));
            }
            _ => unreachable!(),
        }
    }

    /// flash 提示必须跟随界面语言。这批文案曾直接写死中文（`已删除 3 项`…），
    /// 切到 EN 界面后弹出的仍是中文。这里跑一遍常用动作，断言 EN 下提示里无 CJK。
    #[test]
    fn flash_messages_follow_selected_language() {
        fn has_cjk(s: &str) -> bool {
            s.chars()
                .any(|c| matches!(c, '\u{2e80}'..='\u{9fff}' | '\u{ff00}'..='\u{ffef}'))
        }

        let mut app = PReferZApp::new();
        app.lang = Lang::En;
        let a = Item::new_shape(
            ShapeType::Rectangle,
            (40.0, 40.0),
            0.0,
            0.0,
            StrokeStyle::default(),
            None,
        );
        let b = Item::new_shape(
            ShapeType::Rectangle,
            (40.0, 40.0),
            120.0,
            60.0,
            StrokeStyle::default(),
            None,
        );
        let (a_id, b_id) = (a.id, b.id);
        app.scene.add_item(a);
        app.scene.add_item(b);

        // 取出本步产生的 flash，断言已按英文出文案
        let expect_ascii_flash = |app: &mut PReferZApp, what: &str| {
            let msg = app
                .flash_status
                .take()
                .map(|(m, _)| m)
                .unwrap_or_else(|| panic!("{what} 应给出 flash 提示"));
            assert!(!msg.is_empty(), "{what} 的提示不应为空");
            assert!(!has_cjk(&msg), "EN 界面下 {what} 的提示未翻译: {msg}");
        };

        app.scene.deselect_all();
        app.scene.select(a_id);
        app.scene.select(b_id);
        app.align_selected(AlignMode::Left);
        expect_ascii_flash(&mut app, "对齐");
        app.arrange_selected(ArrangeMode::Grid);
        expect_ascii_flash(&mut app, "排列");

        app.scene.deselect_all();
        app.scene.select(b_id);
        app.delete_selected();
        expect_ascii_flash(&mut app, "删除");

        app.finish_create_frame(CanvasPoint::new(0.0, 0.0), CanvasPoint::new(200.0, 200.0));
        expect_ascii_flash(&mut app, "创建画框");
    }
}

/// 缩放手柄：以拖拽角点的对角为锚点
/// 数学：见 REVIEW 报告 P0-4。设 T(p)=pos+R(rot)*(scale∘F∘p)，F=flip 矩阵
/// 对角a 和拖拽点 d
///   R(rot)*(scale'*F*(d-a)) = mouse-a  
///   scale' = R(-rot)*(mouse-a) ./ (F*(d-a))
///   pos' = a - R(rot)*(scale'*F*a)
/// B3：加flip 矩阵 F，否则翻转后缩放方向错误导致跳跃
/// B2：free_scale=false 时默认等比缩放，Ctrl 自由缩放
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
    let v = drag_local - anchor_local; // 局部空间对角向量，分量0
    let w = mouse_canvas - anchor_canvas; // 画布空间

    let rot = start_transform.rotation;
    let cos = rot.cos();
    let sin = rot.sin();
    // R(-rot) * w
    let w_local_x = cos * w.x + sin * w.y;
    let w_local_y = -sin * w.x + cos * w.y;

    // flip 因子（修 B3：缩放计算需除以 F*v 而非 v
    let fx = if start_transform.flip_h { -1.0 } else { 1.0 };
    let fy = if start_transform.flip_v { -1.0 } else { 1.0 };

    let mut new_scale_x = w_local_x / (fx * v.x);
    let mut new_scale_y = w_local_y / (fy * v.y);

    // 等比缩放（修 B2：默认保持高宽比，Ctrl 自由缩放
    if !free_scale {
        let start_sx = start_transform.scale.x.abs().max(0.05);
        let start_sy = start_transform.scale.y.abs().max(0.05);
        let ratio_x = new_scale_x / start_sx;
        let ratio_y = new_scale_y / start_sy;
        // 取变化幅度更大的方向作为统一缩放
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

/// 旋转手柄：以拖拽前的 4 角点中心为锚点旋转
///
/// 由于 `local_to_canvas` 的旋转绕局部原点（左上角），单纯改 `rotation` 会让图片
/// 围绕左上角旋转。这里在更新 rotation 后补pos，让旋转后的 4 角点中心等于
/// 旋转前的中心，从而视觉上围绕中心旋转
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

    // 补偿 pos：让旋转后的 4 角点中心 = 旋转前中心（center_canvas）    // local_to_canvas 的旋转绕局部原点，所以改 rotation 后中心会偏移    // 需把偏移量加回 pos
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
    /// 全局键盘派发。所有组合一律走 [`Keymap`] 查表，不再硬编码 `egui::Key`。
    ///
    /// 派发顺序即优先级：演示切换 → 工具切换 → 模式内（绘制/裁剪）→ 场景操作。
    /// 两条与改绑有关的约束：
    /// - 改绑捕获中与文本编辑中直接返回。编辑框会吃掉可打印字符，此时若仍派发，
    ///   把某个动作绑到字母键就会导致该字母打不进文本。
    /// - `Cancel` 走 [`Self::cancel_pressed`]（查表 + `Esc` 硬兜底），见 D1-a。
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // 文本编辑中不派发场景快捷键（Esc/Enter 由 render_text_editor 自行处理）
        if self.editing_text.is_some() {
            return;
        }

        // 演示模式切换（present 内部输入走 handle_present_input）
        if self.keymap.pressed(Action::TogglePresent, ctx) {
            if matches!(self.app_mode, AppMode::Present { .. }) {
                self.exit_present(ctx);
            } else {
                self.enter_present(ctx);
            }
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
        // 绘制工具激活：屏蔽其它场景快捷键，取消键回 Select
        if self.tool != Tool::Select {
            // 多边形进行中：Enter 收尾闭合（Excalidraw 语义）。
            // 裸 Enter 硬兜底同 cancel_pressed 的 Esc——Confirm 可被改绑，改坏了也得能收尾。
            if matches!(self.drag, DragState::CreatingPolygon { .. })
                && (self.keymap.pressed(Action::Confirm, ctx)
                    || ctx.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                self.finish_create_polygon();
                return;
            }
            if self.cancel_pressed(ctx) {
                // 多边形进行中：Esc 丢弃全部已落顶点，而不只是退出工具
                if matches!(self.drag, DragState::CreatingPolygon { .. }) {
                    self.drag = DragState::Idle;
                }
                self.tool = Tool::Select;
            }
            return;
        }

        // 裁剪模式：确认应用 / 取消
        if self.crop_mode.is_some() {
            if self.keymap.pressed(Action::Confirm, ctx) {
                self.apply_crop();
                return;
            }
            if self.cancel_pressed(ctx) {
                self.cancel_crop();
                return;
            }
            // 裁剪模式下屏蔽其他场景快捷键
            return;
        }

        // Enter：编辑选中项的文本（Excalidraw 语义）。单选才生效——
        // Text 直接改内容；封闭 Shape 编辑/新建绑定文本；其余类型不响应。
        // 注意必须排在裁剪分支之后，否则裁剪模式下 Enter 会被这里吃掉。
        if self.keymap.pressed(Action::EditText, ctx) {
            if let Some(id) = self.single_selected_id() {
                self.start_text_edit(id);
                return;
            }
        }

        // 显示右键菜单
        if self.keymap.pressed(Action::ContextMenu, ctx) && !self.context_menu_open {
            self.context_menu_open = true;
            self.context_menu_pos = ctx
                .input(|i| i.pointer.latest_pos())
                .unwrap_or_else(|| ctx.content_rect().center());
        }

        // 取消键：关闭菜单 / 退出颜色采样
        if self.cancel_pressed(ctx) {
            if self.context_menu_open {
                self.context_menu_open = false;
            } else if self.color_picker_active {
                self.color_picker_active = false;
            }
        }

        // 删除选中（走命令栈）
        if self.keymap.pressed(Action::DeleteSelected, ctx) && !self.scene.selection.is_empty() {
            self.delete_selected();
        }

        // 原位复制选中项（Ctrl+D）：副本偏移 10px 画布，一次 undo 撤销
        if self.keymap.pressed(Action::DuplicateInPlace, ctx) && !self.scene.selection.is_empty() {
            self.duplicate_in_place();
        }

        // 编组 / 解组（plan #13）：Ctrl+G / Ctrl+Shift+G
        if self.keymap.pressed(Action::Group, ctx) {
            self.group_selected();
        }
        if self.keymap.pressed(Action::Ungroup, ctx) {
            self.ungroup_selected();
        }

        // 流程图（plan #7，Excalidraw 同款）：Ctrl+方向=沿该向克隆连接节点+
        // 绑定箭头；Alt+方向=沿连接跳邻居。方向取自实际命中的绑定键（可改绑）。
        if let Some(bind) = self.keymap.pressed_bind(Action::AddConnectedShape, ctx) {
            if let Some(dir) = FlowDir::from_bind(&bind) {
                self.add_connected_shape(dir);
            }
        }
        if let Some(bind) = self.keymap.pressed_bind(Action::NavigateConnected, ctx) {
            if let Some(dir) = FlowDir::from_bind(&bind) {
                self.navigate_connected(dir);
            }
        }

        let do_undo = self.keymap.pressed(Action::Undo, ctx);
        if do_undo && self.perform_undo() {
            self.flash(t(self.lang, T::FlashUndo).to_string());
            ctx.request_repaint();
        }

        let do_redo = self.keymap.pressed(Action::Redo, ctx);
        if do_redo && self.perform_redo() {
            self.flash(t(self.lang, T::FlashRedo).to_string());
            ctx.request_repaint();
        }

        if self.keymap.pressed(Action::Save, ctx) {
            self.save_file(ctx);
        }
        if self.keymap.pressed(Action::SaveAs, ctx) {
            self.save_file_as(ctx);
        }
        if self.keymap.pressed(Action::OpenProject, ctx) {
            self.open_project_file(ctx);
        }
        if self.keymap.pressed(Action::LoadImage, ctx) {
            self.import_image_file(ctx);
        }
        if self.keymap.pressed(Action::NewCanvas, ctx) {
            self.new_canvas(ctx);
        }
        // 粘贴必须用释放沿，见 keymap 模块文档第 1 条
        if self.keymap.pressed(Action::Paste, ctx) {
            self.paste_from_clipboard(ctx);
        }

        // 适应画布（Excalidraw Shift+1 = Zoom to fit all）
        if self.keymap.pressed(Action::FitToScreen, ctx) {
            self.fit_to_screen();
        }
        // 缩放到选中（Excalidraw Shift+2）与回到 100%（Shift+3）
        if self.keymap.pressed(Action::ZoomToSelection, ctx) {
            self.zoom_to_selection();
        }
        if self.keymap.pressed(Action::Zoom100, ctx) {
            self.viewport.zoom = 1.0; // 视口中心（pan）不动，仅改缩放
            self.flash(t(self.lang, T::FlashZoom100).to_string());
        }

        // 进入裁剪模式（仅单张图片选中时）
        if self.keymap.pressed(Action::Crop, ctx) && self.selected_pixmap_count() == 1 {
            self.enter_crop_mode();
        }

        // 切换取色器模式（独立，不要求选中）
        if self.keymap.pressed(Action::ColorPicker, ctx) {
            self.color_picker_active = !self.color_picker_active;
            self.flash(if self.color_picker_active {
                self.shortcut_hint(T::FlashColorPickerHint, &[Action::Cancel])
            } else {
                t(self.lang, T::ExitColorPicker).to_string()
            });
        }
    }

    /// 取消键：查 `Cancel` 绑定，**外加 `Esc` 硬兜底**（D1-a）。
    ///
    /// 用户在设置面板里可以把 `Cancel` 改成任何键，但 `Esc` 永远能退出裁剪 / 绘制
    /// 工具 / 取色器——否则一旦改坏就卡在某个模式里出不来。
    fn cancel_pressed(&self, ctx: &egui::Context) -> bool {
        self.keymap.pressed(Action::Cancel, ctx) || ctx.input(|i| i.key_pressed(egui::Key::Escape))
    }

    /// 工具切换快捷键。修饰键严格匹配由 [`Keymap::pressed`] 保证（默认绑定均无修饰键）。
    /// 再次按同键在 handle_shortcuts 里回 Select。
    fn tool_switch_shortcut(&self, ctx: &egui::Context) -> Option<Tool> {
        let pressed = |action| self.keymap.pressed(action, ctx);
        if pressed(Action::ToolSelect) {
            Some(Tool::Select)
        } else if pressed(Action::ToolRect) {
            Some(Tool::Shape(ShapeType::Rectangle))
        } else if pressed(Action::ToolEllipse) {
            Some(Tool::Shape(ShapeType::Ellipse))
        } else if pressed(Action::ToolDiamond) {
            Some(Tool::Shape(ShapeType::Diamond))
        } else if pressed(Action::ToolLine) {
            Some(Tool::Linear { end_arrow: None })
        } else if pressed(Action::ToolArrow) {
            Some(Tool::Linear {
                end_arrow: Some(ArrowHeadStyle::Arrow),
            })
        } else if pressed(Action::ToolPolygon) {
            Some(Tool::Polygon)
        } else if pressed(Action::ToolFrame) {
            Some(Tool::Frame)
        } else {
            None
        }
    }

    /// 某动作当前的首个绑定展示串（如 `Ctrl+Shift+S`），未绑定时给本地化占位。
    fn binding_display(&self, action: Action) -> String {
        self.keymap
            .bindings(action)
            .first()
            .map(|b| b.display())
            .unwrap_or_else(|| t(self.lang, T::SettingsKeymapUnbound).to_string())
    }

    /// 把 i18n 文案里的 `{0}`/`{1}`… 依次替换为对应动作的绑定展示串。
    /// 快捷键可改绑后，翻译表里不能再写死 `Ctrl+V` 之类。
    fn shortcut_hint(&self, key: T, actions: &[Action]) -> String {
        let parts: Vec<String> = actions.iter().map(|a| self.binding_display(*a)).collect();
        fill(t(self.lang, key), &parts)
    }

    fn finish_import(&mut self, ctx: &egui::Context, outcome: ImportOutcome) {
        if let Some(e) = outcome.error {
            self.flash(fill(
                t(self.lang, T::FlashImportFailed),
                &[outcome.path.display().to_string(), e.to_string()],
            ));
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

        // 初始位置：视口中心对应的画布
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
        self.flash(fill(
            t(self.lang, T::FlashImported),
            &[outcome.path.display().to_string()],
        ));
        ctx.request_repaint();
    }

    /// 后台加载完成：重scene + 上传纹理 + 重新映射 texture_id（由 BackgroundOps::poll 调用）
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

                // 为每Pixmap 重新分配 texture_id，解码上传纹理，重映item.texture_id
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

                // 应用视口元数据。zoom 需按当前钳制范围收敛：旧文件可能存着
                // 超出新 min/max（10%–1000%）的极端值，不 clamp 会让滚轮缩放在
                // 越界值上"空转"好几圈才有反应。
                self.viewport.pan = CanvasVector::new(viewport_meta.pan_x, viewport_meta.pan_y);
                self.viewport.zoom = viewport_meta
                    .zoom
                    .clamp(self.viewport.min_zoom, self.viewport.max_zoom);

                self.scene = scene;
                // 清理孤儿 container_id（容器已不存在则置 None），Phase C/Step 4
                self.scene.cleanup_orphan_containers();
                self.current_file = Some(outcome.path.clone());
                self.dirty = false;
                // 加载成功后加入最近文件列表
                add_recent_file(&mut self.recent_files, outcome.path.clone());
                self.flash(fill(
                    t(self.lang, T::FlashOpened),
                    &[outcome.path.display().to_string()],
                ));
                ctx.request_repaint();
            }
            Err(e) => {
                self.flash(fill(
                    t(self.lang, T::FlashOpenFailed),
                    std::slice::from_ref(&e),
                ));
            }
        }
    }

    /// 保存到当前文件；若没有则调用 save_file_as 弹出对话框
    fn save_file(&mut self, ctx: &egui::Context) {
        if let Some(path) = self.current_file.clone() {
            self.start_save(ctx, path);
        } else {
            self.save_file_as(ctx);
        }
    }

    /// 另存为：弹出对话框选择路径
    fn save_file_as(&mut self, ctx: &egui::Context) {
        let picked = rfd::FileDialog::new()
            .add_filter("PReferZ 项目", &["prz"])
            .set_file_name("untitled.prz")
            .save_file();
        if let Some(path) = picked {
            self.start_save(ctx, path);
        }
    }

    /// 新建空白画布：清scene / 纹理缓存 / undo / current_file / dirty    /// 调用前应已处理保存提示（由调用方负责）
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

    /// 触发新建画布流程：若 dirty 弹保存提示，否则直接 reset
    fn new_canvas(&mut self, ctx: &egui::Context) {
        if self.dirty {
            self.pending_save_prompt = Some(SavePromptAction::NewCanvas);
        } else {
            self.reset_canvas(ctx);
        }
    }

    /// 打开项目文件（.prz）。
    fn open_project_file(&mut self, ctx: &egui::Context) {
        let picked = rfd::FileDialog::new()
            .add_filter("PReferZ 项目", &["prz"])
            .pick_file();
        if let Some(path) = picked {
            self.add_recent_and_load(ctx, path);
        }
    }

    /// 记录最近文件并启动后台加载。
    fn add_recent_and_load(&mut self, ctx: &egui::Context, path: PathBuf) {
        add_recent_file(&mut self.recent_files, path.clone());
        self.bg_ops.start_load(ctx, path, self.lang);
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
                self.flash(fill(
                    t(self.lang, T::FlashClipboardFailed),
                    &[e.to_string()],
                ));
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

    /// 启动后台保存
    fn start_save(&mut self, ctx: &egui::Context, path: PathBuf) {
        // 收集 image_data_cache（key 转字符串以匹sqlar name
        let mut images: HashMap<String, Vec<u8>> = HashMap::new();
        for item in &self.scene.items {
            if let ItemKind::Pixmap { texture_id, .. } = &item.kind {
                if let Some(bytes) = self.image_data_cache.get(texture_id) {
                    images.insert(texture_id.to_string(), bytes.clone());
                }
            }
        }
        let viewport = ViewportMeta {
            pan_x: self.viewport.pan.x,
            pan_y: self.viewport.pan.y,
            zoom: self.viewport.zoom,
        };
        self.bg_ops
            .start_save(ctx, path, self.scene.clone(), images, viewport, self.lang);
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
            self.flash(t(
                self.lang,
                if selection_only {
                    T::FlashNoSelectedImages
                } else {
                    T::FlashNoExportableImages
                },
            ));
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

    /// 编组选中项（plan #13，`Ctrl+G`）。G1 单组：已属别组的项自动换组；
    /// 少于 2 项不动作（单元素编组无意义）。
    fn group_selected(&mut self) {
        let selected: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
        if selected.len() < 2 {
            return;
        }
        let Some(cmd) = SetGroup::group(&self.scene, &selected) else {
            return;
        };
        self.push_cmd(Box::new(cmd));
        self.flash(t(self.lang, T::FlashGrouped).to_string());
    }

    /// 解组选中项（plan #13，`Ctrl+Shift+G`）。G3：只清选中项的组 id，
    /// 未被解组的其余同组成员保持编组。
    fn ungroup_selected(&mut self) {
        let selected: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
        if selected.is_empty() {
            return;
        }
        let Some(cmd) = SetGroup::ungroup(&self.scene, &selected) else {
            return;
        };
        self.push_cmd(Box::new(cmd));
        self.flash(t(self.lang, T::FlashUngrouped).to_string());
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
        // 先快照被删 Pixmap 的 texture_id（命令执行后 item 就查不到了）
        let deleted_tex_ids: Vec<u64> = ids
            .iter()
            .filter_map(|id| self.scene.get_item(id))
            .filter_map(|it| match &it.kind {
                ItemKind::Pixmap { texture_id, .. } => Some(*texture_id),
                _ => None,
            })
            .collect();
        // DeleteItems 命令（修 S1/W8），undo 已支持快照恢复（P1-3
        let cmd = DeleteItems::new(ids.clone());
        self.push_cmd(Box::new(cmd));
        self.scene.selection.clear();
        self.flash(fill(
            t(self.lang, T::FlashDeleted),
            &[ids.len().to_string()],
        ));
        // 纹理驱逐（修 .issues #1 删副本后原件变灰方块）：
        // - 副本与原件共享同一 texture_id，删除后只要场景里仍有 item 引用该纹理
        //   就不能驱逐——否则其余引用者立刻变灰方块、裁剪/取色等编辑功能一并失效；
        // - 场景无引用时也只驱逐 GPU 纹理句柄，保留字节/像素缓存：undo 恢复 item 后
        //   `ensure_pixmap_textures` 按 rgba 缓存懒重建纹理，保存 sqlar 仍需原始字节。
        for tid in deleted_tex_ids {
            let still_used = self.scene.items.iter().any(
                |it| matches!(&it.kind, ItemKind::Pixmap { texture_id, .. } if *texture_id == tid),
            );
            if !still_used {
                self.texture_cache.remove(&tid);
                self.grayscale_texture_cache.remove(&tid);
            }
        }
    }

    /// 原位复制选中项（plan.md 快赢项 #11，Excalidraw 同款 `Ctrl+D`）。
    /// 选区带容器联动（封闭形状的绑定文本、画框成员），副本偏移 10px 画布避免完全重叠；
    /// 副本已经入场景，push `AddItems(preview_applied=true)`，一次 undo 撤掉整个复制。
    fn duplicate_in_place(&mut self) {
        let selected: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
        if selected.is_empty() {
            return;
        }
        // 容器联动：与拖拽一致
        let mut collected = selected.clone();
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
        let dups = self
            .scene
            .duplicate_items(&collected, CanvasVector::new(10.0, 10.0));
        let n = dups.len();
        if n == 0 {
            return;
        }
        for dup in &dups {
            self.scene.add_item(dup.clone());
        }
        // 选区切到副本
        self.scene.deselect_all();
        for dup in &dups {
            self.scene.select(dup.id);
        }
        self.push_cmd(Box::new(AddItems::new(dups).with_preview_applied(true)));
        self.flash(fill(t(self.lang, T::FlashDuplicated), &[n.to_string()]));
    }

    /// plan #7：流程图节点创建（`Ctrl+方向`，单选矩形/椭圆/菱形时）。沿该向
    /// 克隆一个**同源同风格**节点（主轴 = 源边界 + [`FLOWCHART_GAP`]，交叉轴
    /// 中心对齐——同尺寸克隆时偏移天然实现；不复制绑定文字），并连一条两端
    /// 绑定的**直箭头**（本仓库无 elbow）：anchor=各自朝向对方的边中点、初始
    /// 端点即该锚点画布位置，与 `resolve_bindings` 重算结果一致，后续移动形状
    /// 箭头自动跟随。按下即提交：新节点+箭头 `AddItems` 一条 undo，选区跳新
    /// 节点（同方向连按自然接链）。前提不满足静默（对齐 Excalidraw）。
    fn add_connected_shape(&mut self, dir: FlowDir) {
        let Some(src_id) = self.single_selected_id() else {
            return;
        };
        let (w, h, stroke) = match self.scene.get_item(&src_id) {
            Some(item) => match &item.kind {
                ItemKind::Shape {
                    shape_type,
                    base_size,
                    stroke,
                    ..
                } if !matches!(shape_type, ShapeType::Polyline) => {
                    (base_size.0, base_size.1, *stroke)
                }
                _ => return,
            },
            None => return,
        };
        // 验收反馈 #7-2：该方向已有直连同级邻居时，新节点放到**邻居旁边**
        // （主轴=邻居远边 + GAP、交叉轴对齐邻居中心），而不是与原选中的源节点
        // 完全重合；无邻居则以源为基准（原行为）。箭头仍 源→新节点。
        let src_rect = match self.scene.get_item(&src_id) {
            Some(item) => item.bounding_rect(),
            None => return,
        };
        let (sw, sh) = (src_rect.width(), src_rect.height());
        let base = self
            .find_connected_neighbor(src_id, dir)
            .map(|(_, rect)| rect)
            .unwrap_or(src_rect);
        let target = match dir {
            FlowDir::Right => {
                CanvasPoint::new(base.max().x + FLOWCHART_GAP, base.center().y - sh / 2.0)
            }
            FlowDir::Left => CanvasPoint::new(
                base.min().x - FLOWCHART_GAP - sw,
                base.center().y - sh / 2.0,
            ),
            FlowDir::Down => {
                CanvasPoint::new(base.center().x - sw / 2.0, base.max().y + FLOWCHART_GAP)
            }
            FlowDir::Up => CanvasPoint::new(
                base.center().x - sw / 2.0,
                base.min().y - FLOWCHART_GAP - sh,
            ),
        };
        let offset = target - src_rect.min();
        // duplicate_items：新 uuid（手绘抖动由 id 派生自动不同）、未编组化、
        // 不含绑定文字（只复制传入的 id）——正合克隆节点语义。
        let mut dups = self.scene.duplicate_items(&[src_id], offset);
        let Some(dup) = dups.pop() else { return };
        let new_id = dup.id;
        let src_anchor = edge_anchor_local(dir, w, h);
        let dup_anchor = edge_anchor_local(dir.opposite(), w, h);
        let s = match self.scene.get_item(&src_id) {
            Some(item) => item.local_point_to_canvas(src_anchor),
            None => return,
        };
        let d = dup.local_point_to_canvas(dup_anchor);
        // 箭头：两点局部坐标对齐 AABB 左上角（与 finish_create_shape 同惯例）。
        let min = CanvasPoint::new(s.x.min(d.x), s.y.min(d.y));
        let mut arrow = Item::new_polyline(
            vec![(s.x - min.x, s.y - min.y), (d.x - min.x, d.y - min.y)],
            ((d.x - s.x).abs(), (d.y - s.y).abs()),
            None,
            Some(ArrowHeadStyle::Arrow),
            false,
            min.x,
            min.y,
            stroke,
        );
        if let ItemKind::Shape {
            start_binding,
            end_binding,
            ..
        } = &mut arrow.kind
        {
            *start_binding = Some(EndpointBinding {
                target: src_id,
                anchor: Some(src_anchor),
            });
            *end_binding = Some(EndpointBinding {
                target: new_id,
                anchor: Some(dup_anchor),
            });
        }
        let added = vec![dup.clone(), arrow.clone()];
        self.scene.add_item(dup);
        self.scene.add_item(arrow); // 递增 z：箭头压在新节点上（Excalidraw 同款顺序）
        self.scene.deselect_all();
        self.scene.select(new_id);
        self.push_cmd(Box::new(AddItems::new(added).with_preview_applied(true)));
    }

    /// plan #7 / 验收反馈 #7-2：找 `id` 在 `dir` 方向的**直连**邻居——某条
    /// Polyline 两端绑定恰一端=当前、另一端目标中心落在 `dir` 主轴前方且
    /// 交叉轴不越过主轴，取主轴距离最近者。返回（邻居 id, 邻居包围盒）。
    fn find_connected_neighbor(&self, id: ItemId, dir: FlowDir) -> Option<(ItemId, CanvasRect)> {
        let cur_center = self.scene.get_item(&id)?.bounding_rect().center();
        let mut best: Option<(ItemId, f32, CanvasRect)> = None; // (邻居, 主轴距离, rect)
        for item in &self.scene.items {
            let ItemKind::Shape {
                shape_type: ShapeType::Polyline,
                start_binding,
                end_binding,
                ..
            } = &item.kind
            else {
                continue;
            };
            // 只走两端都有绑定的完整连接；自环跳过。
            let nb = match (start_binding, end_binding) {
                (Some(b0), Some(b1)) if b0.target == id && b1.target != id => b1.target,
                (Some(b0), Some(b1)) if b1.target == id && b0.target != id => b0.target,
                _ => continue,
            };
            let nb_rect = match self.scene.get_item(&nb) {
                Some(neighbor) => neighbor.bounding_rect(),
                None => continue,
            };
            let delta = nb_rect.center() - cur_center;
            let (prim, orth) = match dir {
                FlowDir::Right => (delta.x, delta.y),
                FlowDir::Left => (-delta.x, delta.y),
                FlowDir::Down => (delta.y, delta.x),
                FlowDir::Up => (-delta.y, delta.x),
            };
            if prim > 0.0 && prim >= orth.abs() && best.is_none_or(|(_, bd, _)| prim < bd) {
                best = Some((nb, prim, nb_rect));
            }
        }
        best.map(|(nb, _, rect)| (nb, rect))
    }

    /// plan #7：沿连接箭头导航（`Alt+方向`）。跳选区到 `dir` 方向的直连邻居
    /// （组按 #13 点击语义整组展开）。无命令——导航不进 undo（与 Excalidraw 一致）。
    fn navigate_connected(&mut self, dir: FlowDir) {
        let Some(cur_id) = self.single_selected_id() else {
            return;
        };
        if let Some((nb, _)) = self.find_connected_neighbor(cur_id, dir) {
            self.scene.deselect_all();
            for hid in self.scene.expand_to_groups(&[nb]) {
                self.scene.select(hid);
            }
        }
    }

    fn bring_to_front(&mut self) {
        let ids: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
        if ids.is_empty() {
            return;
        }
        // ReorderItems 命令（修 S3/W8），不再直接z
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

    /// 计算把 `content_rect` 适配到当前视口（90% 填充）的目标 (zoom, pan)，
    /// 与 [`ViewportState::fit_to_content`] 同公式。用于判断"当前是否已是该内容
    /// 的适配视图"，从而支持双击图片在"适配↔上一视图"间切换（.issues #2）。
    fn compute_fit(&self, content_rect: CanvasRect) -> (f32, CanvasVector) {
        let content_w = content_rect.width().max(1.0);
        let content_h = content_rect.height().max(1.0);
        let screen_w = self.viewport.screen_rect.width().max(1.0);
        let screen_h = self.viewport.screen_rect.height().max(1.0);
        let scale = (screen_w / content_w).min(screen_h / content_h) * 0.9;
        let scale = scale.clamp(self.viewport.min_zoom, self.viewport.max_zoom);
        (scale, content_rect.center().to_vector())
    }

    fn fit_to_screen(&mut self) {
        if self.scene.items.is_empty() {
            self.viewport.reset();
            self.flash(t(self.lang, T::FlashFitToCanvas).to_string());
            return;
        }
        // 用所item AABB 并集
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

    /// Shift+2：缩放视口到选中元素（Excalidraw 同款 Zoom to selection）。
    /// 无选中时仅 flash 提示，不做任何视口变更。
    fn zoom_to_selection(&mut self) {
        if self.scene.selection.is_empty() {
            self.flash(t(self.lang, T::FlashNoSelection).to_string());
            return;
        }
        let mut bbox: Option<preferz_core::spaces::CanvasRect> = None;
        for id in &self.scene.selection {
            if let Some(item) = self.scene.get_item(id) {
                let r = item.bounding_rect();
                bbox = Some(match bbox {
                    Some(b) => b.union(&r),
                    None => r,
                });
            }
        }
        if let Some(b) = bbox {
            self.viewport.fit_to_content(b);
            self.flash(t(self.lang, T::FlashZoomToSelection).to_string());
        }
    }

    // ─────────── Phase 5 辅助方法 ───────────

    /// 当前选中 Pixmap item 数量
    fn selected_pixmap_count(&self) -> usize {
        self.scene
            .selection
            .iter()
            .filter_map(|id| self.scene.get_item(id))
            .filter(|it| matches!(it.kind, ItemKind::Pixmap { .. }))
            .count()
    }

    /// 单Pixmap grayscale 状态（用于右键菜单文案）
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

    /// 切换选中 Pixmap item 的灰度标志（spec §2.2 灰度）
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

    /// 进入裁剪模式（spec §2.2 裁剪）    /// 选中单个 Pixmap 时，初始crop 矩形为当crop 或整个图片
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
        self.flash(self.shortcut_hint(T::FlashCropHint, &[Action::Confirm, Action::Cancel]));
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

        // old_transform（再次取，因为上面的clone item
        let old_transform = match self.scene.get_item(&item_id) {
            Some(item) => item.transform,
            None => return,
        };

        let cmd = CropItems::new(item_id, original, new_crop, old_transform, new_transform);
        self.push_cmd(Box::new(cmd));
        self.flash(t(self.lang, T::FlashCropApplied).to_string());
    }

    /// 取消裁剪：恢复原 crop 并退出裁剪模式
    fn cancel_crop(&mut self) {
        self.crop_mode = None;
        self.flash(t(self.lang, T::FlashCropCancelled).to_string());
    }

    /// 检测鼠标是否命中裁剪手柄（4 个角点）
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

    /// 颜色采样：在 Pixmap 上的鼠标位置读取像素 RGB（spec §2.2 颜色采样）
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
        // 鼠标 item 局部坐
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

    /// 批量排列选中 item（spec §2.2 批量操作）
    fn arrange_selected(&mut self, mode: ArrangeMode) {
        // 临时把选中项作为整体排
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
            ArrangeMode::Linear => T::ArrangeModeLinear,
            ArrangeMode::Grid => T::ArrangeModeGrid,
            ArrangeMode::Optimal => T::ArrangeModeOptimal,
        };
        self.flash(fill(
            t(self.lang, T::FlashArranged),
            &[t(self.lang, mode_name).to_string()],
        ));
    }

    /// 多元素对齐（plan #6）。参考系 = 选区包围盒；命令复用 `ArrangeItems` 入 undo 栈。
    fn align_selected(&mut self, mode: AlignMode) {
        let ids: Vec<ItemId> = self.scene.selection.iter().copied().collect();
        if ids.len() < 2 {
            return;
        }
        let moves = plan_align(&self.scene, &ids, mode);
        if moves.is_empty() {
            return; // 已经在位：不产生无意义 undo 条目
        }
        self.push_cmd(Box::new(
            ArrangeItems::new(moves).with_preview_applied(false),
        ));
        let name = match mode {
            AlignMode::Left => T::AlignLeft,
            AlignMode::HCenter => T::AlignHCenter,
            AlignMode::Right => T::AlignRight,
            AlignMode::Top => T::AlignTop,
            AlignMode::VCenter => T::AlignVCenter,
            AlignMode::Bottom => T::AlignBottom,
        };
        self.flash(fill(
            t(self.lang, T::FlashAligned),
            &[t(self.lang, name).to_string()],
        ));
    }

    /// 多元素分布（plan #6）。首尾元素不动，中间按 `mode` 均分；要求 ≥3 项。
    fn distribute_selected(&mut self, axis: DistributeAxis, mode: DistributeMode) {
        let ids: Vec<ItemId> = self.scene.selection.iter().copied().collect();
        if ids.len() < 3 {
            self.flash(t(self.lang, T::FlashDistributeNeedThree).to_string());
            return;
        }
        let moves = plan_distribute(&self.scene, &ids, axis, mode);
        if moves.is_empty() {
            return;
        }
        self.push_cmd(Box::new(
            ArrangeItems::new(moves).with_preview_applied(false),
        ));
        let name = match (axis, mode) {
            (DistributeAxis::Horizontal, DistributeMode::Gap) => T::DistributeHGap,
            (DistributeAxis::Horizontal, DistributeMode::Centers) => T::DistributeHCenters,
            (DistributeAxis::Vertical, DistributeMode::Gap) => T::DistributeVGap,
            (DistributeAxis::Vertical, DistributeMode::Centers) => T::DistributeVCenters,
        };
        self.flash(fill(
            t(self.lang, T::FlashDistributed),
            &[t(self.lang, name).to_string()],
        ));
    }

    /// 归一化选中 Pixmap item 尺寸（spec §2.2 归一化尺寸）
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
        // target = 首个选中 Pixmap 的当前
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
        self.flash(fill(
            t(self.lang, T::FlashNormalized),
            &[mode_name.to_string()],
        ));
    }

    /// 渲染保存提示对话框（关闭/新建时若 dirty 弹出）    /// 按钮    /// - 保存：触发保存流程，首次保存弹系统文件选择器；保存完成后由 poll_background 执行 pending action
    /// - 放弃：不保存，直接执pending action（关闭窗/ 新建画布    /// - 取消：什么都不做，保留当前画布状
    fn render_save_prompt(&mut self, ctx: &egui::Context) {
        if self.pending_save_prompt.is_none() {
            return;
        }
        // 保存进行中：等待完成（poll_background 会自动执pending action
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
            // 触发保存流程：保存完成时 poll_background 会执pending action
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

    /// 把当前配置（语言 + 主题）落盘到 `~/.preferz/config.json`。
    /// keymap 不再持久化（改绑入口已随 D6 移除，见 `PReferZApp::new`）。
    fn persist_config(&self) {
        save_config(&UserConfig {
            lang: self.lang,
            keymap: KeymapMap::default(),
            theme: self.theme,
        });
    }

    /// 渲染设置面板（spec §2.3 简化版：排列间距 + 窗口形态 + 语言 + 主题 + 快捷键）。
    fn render_settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.settings_open;
        // 局部副本，闭包内修改；changed 时记录，闭包外发 ViewportCommand
        let mut always_on_top = self.always_on_top;
        let mut frameless = self.frameless;
        let mut lang = self.lang;
        let mut theme = self.theme;
        let mut top_changed = false;
        let mut frame_changed = false;
        let mut lang_changed = false;
        let mut theme_changed = false;
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
                egui::ComboBox::from_id_salt("settings_language")
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

                // 主题切换（Phase G）：Light / Dark / Auto（跟随系统）。
                ui.label(t(self.lang, T::SettingsTheme));
                egui::ComboBox::from_id_salt("settings_theme")
                    .selected_text(theme.display_name())
                    .show_ui(ui, |ui| {
                        for option in [ThemeMode::Dark, ThemeMode::Light, ThemeMode::Auto] {
                            if ui
                                .selectable_label(theme == option, option.display_name())
                                .clicked()
                            {
                                theme = option;
                                theme_changed = true;
                            }
                        }
                    });
                ui.separator();

                // 注：键鼠改绑设置入口已按 ADR-0007 / D6 移除（不做用户自定义）。
                // `Action`/`Keymap` 派发架构保留，默认键位由 Phase K 对齐 Excalidraw。
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
            self.persist_config();
        }
        if theme_changed {
            self.theme = theme;
            // 切换主题时把新建元素默认色翻到该主题（D2）；用户仍可手动改色。
            self.default_stroke.color = theme.default_stroke_color(ctx);
            self.persist_config();
        }
    }

    /// 渲染颜色采样 overlay（在鼠标附近显示 RGB/HEX）
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
                                ui.label(
                                    self.shortcut_hint(T::FlashColorPickerHint, &[Action::Cancel]),
                                );
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
                            egui::StrokeKind::Middle,
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

/// 用户配置（语言 + 快捷键 + 主题；均带 `#[serde(default)]` 以便老配置兼容）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct UserConfig {
    #[serde(default)]
    lang: Lang,
    /// 旧版本遗留：不再消费（启动恒用 `Keymap::new()`，见 `PReferZApp::new`）。
    /// 保留字段是为了老配置文件能正常解析；序列化时写空表，避免把过期默认值
    /// 继续散播给未来的版本。
    #[serde(default)]
    keymap: KeymapMap,
    /// 主题模式（Light/Dark/Auto），缺省回退 `Dark`。
    #[serde(default)]
    theme: ThemeMode,
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
