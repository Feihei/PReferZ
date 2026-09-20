use crate::i18n::{t, Lang, T};
use crate::interaction;
use crate::keymap::{Action, BindKey, KeyBind, Keymap, KeymapMap};
use crate::theme::{self, ThemeMode};
use crate::ui::stylers::{
    build_freedraw_visuals, build_shape_visuals, freedraw_stroke_shapes, item_local_to_screen,
};
use crate::ui::widgets::palette;
use crate::ui::widgets::transform_handles::{
    should_show_flip, should_show_rotate, Handle, TransformHandles,
};
use crate::viewport::{ViewportEgui, ViewportState};
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
use preferz_fileio::ViewportMeta;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self};
// —— Step 3：从 preferz_app 拆出的子模块（见 core-sink 计划）——
mod background_ops;
mod config;
mod export;
pub(crate) use background_ops::{BackgroundOps, ExportOutcome, ImportOutcome, LoadOutcome};
pub(crate) use config::{add_recent_file, load_config, load_recent_files, save_config, UserConfig};
pub(crate) use export::{export_pixmaps_to_dir, export_scene_to_file, ColorSample, ExportFormat};
mod actions;
mod context_menu;
mod drag;
mod file_io;
mod present;
mod props;
mod render;
mod settings;
mod text_edit;

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
    /// 徒手绘制（plan #10）：按住连续采样，释放定型为一条 `ItemKind::Freedraw` 墨迹。
    /// 对齐 Excalidraw freedraw（裸 P + 本项目 Num7）。
    Freehand,
}

/// 画框比例 / 纸张预设（plan #3）。
///
/// - [`FramePreset::Ratio`]：纯演示比例——套用后保持画框当前**有效长边**长度不变，
///   只把短边调成目标比例（缩放观感稳定，不涉及 DPI）。
/// - [`FramePreset::Paper`]：纸张尺寸——固定像素绝对值（A4 按 **96 DPI** 换算：
///   210×297mm → 794×1123px）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum FramePreset {
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
    /// 徒手绘制（plan #10）：按住左键连续采样的原始画布点（含首点）。
    /// 释放时按速度→笔宽定锥形成 `ItemKind::Freedraw` 并 `AddItem` 入 undo。
    Drawing {
        /// 采样中心线点（画布坐标，按屏幕最小间距过滤）。
        raw: Vec<CanvasPoint>,
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
    /// 应用内剪贴板缓冲（Ctrl+C / Ctrl+X 存选中项快照）。单张图片时同时写系统
    /// 剪贴板；粘贴（Ctrl+V）优先于此缓冲，缓冲为空才回退系统剪贴板图片。
    /// 存原位置快照，粘贴时分配新 id 并重定位到鼠标处。
    clipboard_items: Vec<Item>,
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
pub(crate) enum CropHandle {
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
pub(crate) enum PropKind {
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
pub(crate) enum PropValue {
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
            clipboard_items: Vec::new(),
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

/// plan #10：徒手绘制采样的屏幕最小间距（像素）——只做去抖/去重合，阈值取小，
/// 保留「快→点稀、慢→点密」的间距速度信号（`freedraw::widths_from_spacing` 依赖它）。
/// 画布阈值 = 此值 / zoom，缩放不改手感。
const FREEDRAW_MIN_SPACING_PX: f32 = 2.0;

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
pub(crate) enum FlowDir {
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
                    (Tool::Freehand, "✎", T::ToolFreehand),
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
            self.viewport.set_screen_rect_egui(rect);

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
                let min = self.viewport.canvas_to_pos2(*start_canvas);
                let max = self.viewport.canvas_to_pos2(*current_canvas);
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
                let screen_rect = self.viewport.canvas_rect_to_egui(rect_canvas);
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
                            .map(|p| self.viewport.canvas_to_pos2(*p))
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
                                self.viewport.canvas_to_pos2(CanvasPoint::new(px, py))
                            })
                            .collect();
                        pts.push(pts[0]); // 闭合路径
                        ui.painter().add(egui::Shape::line(pts, stroke));
                    }
                    // 线性对象：画 start → current 线段；箭头另画头部（与 CleanStyler 一致）
                    ShapeType::Polyline => {
                        let s0 = self.viewport.canvas_to_pos2(*start);
                        let s1 = self.viewport.canvas_to_pos2(*current);
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
                let screen_rect = self.viewport.canvas_rect_to_egui(rect_canvas);
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
                    *current = self.viewport.pos2_to_canvas(pos);
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
                        .map(|last| snap::snap_polygon_point(*last, *current, *shift))
                        .unwrap_or(*current);
                    Some((points.clone(), tip))
                }
                _ => None,
            };
            if let Some((points, tip)) = polygon_preview {
                let stroke = egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(100, 200, 255));
                let mut screen_pts: Vec<egui::Pos2> = points
                    .iter()
                    .map(|p| self.viewport.canvas_to_pos2(*p))
                    .collect();
                screen_pts.push(self.viewport.canvas_to_pos2(tip));
                if screen_pts.len() >= 2 {
                    ui.painter().add(egui::Shape::line(screen_pts, stroke));
                }
                // ≥3 个顶点时才提示"再确认一下就闭合成面"：两点连不出面
                if points.len() >= 3 {
                    let a = self.viewport.canvas_to_pos2(tip);
                    let b = self.viewport.canvas_to_pos2(points[0]);
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
                    let s = self.viewport.canvas_to_pos2(*p);
                    ui.painter().rect_filled(
                        egui::Rect::from_center_size(s, egui::vec2(6.0, 6.0)),
                        egui::CornerRadius::ZERO,
                        egui::Color32::from_rgb(100, 200, 255),
                    );
                }
            }

            // 徒手绘制实时预览（plan #10）：用与终稿同一套逐段描边几何（笔宽随 zoom），
            // 落笔即见收笔尖、运笔粗的墨迹，且不会是填充区域。
            if let DragState::Drawing { raw } = &self.drag {
                let pts: Vec<(f32, f32)> = raw.iter().map(|p| (p.x, p.y)).collect();
                if pts.len() >= 2 {
                    let widths = preferz_core::freedraw::widths_from_spacing(
                        &pts,
                        self.default_stroke.width,
                        self.viewport.zoom,
                    );
                    let screen_pts: Vec<egui::Pos2> = pts
                        .iter()
                        .map(|&(x, y)| self.viewport.canvas_to_pos2(CanvasPoint::new(x, y)))
                        .collect();
                    let screen_widths: Vec<f32> =
                        widths.iter().map(|w| w * self.viewport.zoom).collect();
                    let c = self.default_stroke.color;
                    let color = egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
                    ui.painter()
                        .extend(freedraw_stroke_shapes(&screen_pts, &screen_widths, color));
                }
            }

            // 鼠标中键拖拽平移
            if response.dragged_by(egui::PointerButton::Middle) {
                self.viewport.pan_by_screen_egui(response.drag_delta());
            }

            // 滚轮缩放（以鼠标位置为锚点）。SCROLL_ZOOM_SENSITIVITY：换 glow 后端后
            // smooth_scroll_delta 每格增量比原后端大，乘系数放慢，手感对齐旧后端。
            const SCROLL_ZOOM_SENSITIVITY: f32 = 0.5;
            let scroll = ctx.input(|i| i.smooth_scroll_delta);
            if scroll.y != 0.0 {
                if let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) {
                    self.viewport
                        .zoom_at_egui(scroll.y * SCROLL_ZOOM_SENSITIVITY, pos);
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
                            let canvas_pos = self.viewport.pos2_to_canvas(pos);
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

// ─────────────────────────── 渲染 ───────────────────────────

impl PReferZApp {
    // ─────────────────────────── Present（Slide 演示） ───────────────────────────

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
            let bytes = include_bytes!("../../../../assets/icon.png");
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
        DragState::Drawing { .. } => "Drawing",
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
                // 徒手绘制进行中：Esc 丢弃当前未定型的墨迹采样
                if matches!(self.drag, DragState::Drawing { .. }) {
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
        // 复制 / 剪切（释放沿，同 Paste，见 keymap 模块文档第 1 条）
        if self.keymap.pressed(Action::Copy, ctx) {
            self.copy_selected();
        }
        if self.keymap.pressed(Action::Cut, ctx) {
            self.cut_selected();
        }
        // 粘贴必须用释放沿，见 keymap 模块文档第 1 条。
        // 分流：应用内缓冲非空 → 粘缓冲到鼠标位置；空 → 回退系统剪贴板图片。
        if self.keymap.pressed(Action::Paste, ctx) {
            let mouse = ctx.input(|i| i.pointer.latest_pos());
            if !self.paste_internal(mouse) {
                self.paste_from_clipboard(ctx);
            }
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
        } else if pressed(Action::ToolFreehand) {
            Some(Tool::Freehand)
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

    // ───────── 徒手绘制工具（plan #10） ─────────

    #[test]
    fn finish_create_freedraw_builds_tapered_ink_and_resets_tool() {
        let mut app = PReferZApp::new();
        app.tool = Tool::Freehand;
        let base_width = app.default_stroke.width;
        let color = app.default_stroke.color;
        // 一段先慢后快的采样（画布坐标）：点数≥2 应成墨迹。速度→笔宽的曲线由
        // core `freedraw` 单测覆盖，这里只验手势落地的不变量。
        let raw = vec![
            CanvasPoint::new(10.0, 30.0),
            CanvasPoint::new(14.0, 30.0),
            CanvasPoint::new(18.0, 30.0),
            CanvasPoint::new(80.0, 30.0), // 大步距 = 快
            CanvasPoint::new(160.0, 30.0),
            CanvasPoint::new(240.0, 30.0),
        ];
        app.finish_create_freedraw(raw);
        assert_eq!(app.scene.items.len(), 1);
        assert_eq!(app.tool, Tool::Select, "收尾后应回 Select");
        let item = &app.scene.items[0];
        // 整体位置 = 点集 AABB 左上角 (10,30)。
        assert_eq!((item.transform.pos.x, item.transform.pos.y), (10.0, 30.0));
        match &item.kind {
            ItemKind::Freedraw {
                points,
                widths,
                color: c,
            } => {
                assert_eq!(points.len(), 6);
                assert_eq!(points[0], (0.0, 0.0));
                assert_eq!(widths.len(), points.len());
                assert!(widths.iter().all(|&w| w > 0.0 && w.is_finite()));
                assert_eq!(*c, color);
                // 笔宽恒不超过基准（慢速 = 基准，快速/收笔 ≤ 基准）。
                let max_w = widths.iter().fold(0.0f32, |a, &b| a.max(b));
                assert!(max_w <= base_width + 1e-3);
            }
            _ => panic!("徒手产物应为 Freedraw item"),
        }
    }

    #[test]
    fn finish_create_freedraw_discards_degenerate_stroke_but_resets_tool() {
        let mut app = PReferZApp::new();
        app.tool = Tool::Freehand;
        // 单点（误触/单击）：不产生 item，但仍回 Select。
        app.finish_create_freedraw(vec![CanvasPoint::new(5.0, 5.0)]);
        assert!(app.scene.items.is_empty(), "单点墨迹应被丢弃");
        assert_eq!(app.tool, Tool::Select);
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
