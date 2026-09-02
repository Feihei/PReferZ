use crate::item::{CropRect, ItemId, ItemKind};
use crate::scene::{RenumberPlan, Scene};
use crate::shape::{ArrowHeadStyle, CurveType, FillStyle, PixmapStyle, StrokeStyle, TextStyle};
use crate::spaces::CanvasVector;
use crate::transform::Transform;

/// 命令 trait。
///
/// `skip_first_redo` 用于交互预览模式：当 UI 在按下时已经把变更直接应用到 item
/// （比如拖拽中实时修改 `transform.pos`），释放时调用 [`UndoStack::push`] 应当
/// 跳过首次 `redo()`，否则会把同一变更应用两次。AGENTS.md Gotcha #5。
pub trait Command {
    fn redo(&mut self, scene: &mut Scene);
    fn undo(&mut self, scene: &mut Scene);
    fn skip_first_redo(&self) -> bool {
        false
    }
}

// ─────────────────────────── Transform ───────────────────────────

/// 单个 item 的整体 transform 变更（scale + rotate + move + flip 的任意组合）。
/// 用于变换手柄拖拽释放时把"预览态"固化到 undo 栈。
pub struct TransformItem {
    item_id: ItemId,
    old_transform: Transform,
    new_transform: Transform,
    /// 拖拽预览已经直接改到 item 上，push 时跳过首次 redo。
    preview_already_applied: bool,
}

impl TransformItem {
    pub fn new(item_id: ItemId, old_transform: Transform, new_transform: Transform) -> Self {
        Self {
            item_id,
            old_transform,
            new_transform,
            preview_already_applied: true,
        }
    }

    /// 显式声明是否为预览模式（默认 true，因为该命令几乎只在交互释放时使用）。
    pub fn with_preview_applied(mut self, applied: bool) -> Self {
        self.preview_already_applied = applied;
        self
    }
}

impl Command for TransformItem {
    fn redo(&mut self, scene: &mut Scene) {
        if let Some(item) = scene.get_item_mut(&self.item_id) {
            item.transform = self.new_transform;
        }
    }

    fn undo(&mut self, scene: &mut Scene) {
        if let Some(item) = scene.get_item_mut(&self.item_id) {
            item.transform = self.old_transform;
        }
    }

    fn skip_first_redo(&self) -> bool {
        self.preview_already_applied
    }
}

// ─────────────────────────── Shape points ───────────────────────────

/// 线类（Line/Arrow）端点编辑命令：修改 `points` 并同步 `base_size`（AABB）。
/// 拖拽端点时 UI 已直接改到 item 上，push 时跳过首次 redo。
pub struct EditShapePoints {
    item_id: ItemId,
    old_points: Vec<(f32, f32)>,
    new_points: Vec<(f32, f32)>,
}

impl EditShapePoints {
    pub fn new(item_id: ItemId, old_points: Vec<(f32, f32)>, new_points: Vec<(f32, f32)>) -> Self {
        Self {
            item_id,
            old_points,
            new_points,
        }
    }
}

impl Command for EditShapePoints {
    fn redo(&mut self, scene: &mut Scene) {
        if let Some(item) = scene.get_item_mut(&self.item_id) {
            item.kind.set_line_points(self.new_points.clone());
        }
    }

    fn undo(&mut self, scene: &mut Scene) {
        if let Some(item) = scene.get_item_mut(&self.item_id) {
            item.kind.set_line_points(self.old_points.clone());
        }
    }

    fn skip_first_redo(&self) -> bool {
        true
    }
}

// ─────────────────────────── Set arrow heads ───────────────────────────

/// 一对起/终点箭头样式的快照。批量命令里用它替代散落的四个 Option，
/// 免得 `Vec<(ItemId, Option<..>, Option<..>, Option<..>, Option<..>)>` 这种五元组。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ArrowHeads {
    pub start: Option<ArrowHeadStyle>,
    pub end: Option<ArrowHeadStyle>,
}

/// 批量设置线性对象（Polyline）起/终点箭头样式命令。
///
/// 每项自带 old/new，**整批只占一条 undo 记录**（D3：多选显示交集值、修改批量应用）。
/// 单项场景用 [`Self::new`]，多选场景用 [`Self::new_batch`]。
/// 样式面板开关触发：UI 未直接改 item，push 时正常 redo 应用（skip_first_redo = false）。
pub struct SetArrowHeads {
    items: Vec<(ItemId, ArrowHeads, ArrowHeads)>,
    preview_already_applied: bool,
}

impl SetArrowHeads {
    /// 单项便捷构造（保持旧的 5 参签名，调用点不必改）。
    pub fn new(
        item_id: ItemId,
        old_start: Option<ArrowHeadStyle>,
        old_end: Option<ArrowHeadStyle>,
        new_start: Option<ArrowHeadStyle>,
        new_end: Option<ArrowHeadStyle>,
    ) -> Self {
        Self {
            items: vec![(
                item_id,
                ArrowHeads {
                    start: old_start,
                    end: old_end,
                },
                ArrowHeads {
                    start: new_start,
                    end: new_end,
                },
            )],
            preview_already_applied: false,
        }
    }

    /// 批量构造：`(item_id, old, new)` 三元组列表。
    pub fn new_batch(items: Vec<(ItemId, ArrowHeads, ArrowHeads)>) -> Self {
        Self {
            items,
            preview_already_applied: false,
        }
    }

    /// 声明是否为预览模式（滑块/连续控件在拖动中已直接改 item，释放时跳过首次 redo）。
    pub fn with_preview_applied(mut self, applied: bool) -> Self {
        self.preview_already_applied = applied;
        self
    }

    fn apply(scene: &mut Scene, items: &[(ItemId, ArrowHeads, ArrowHeads)], new: bool) {
        for (id, old, new_heads) in items {
            let target = if new { new_heads } else { old };
            if let Some(item) = scene.get_item_mut(id) {
                if let ItemKind::Shape {
                    start_arrow,
                    end_arrow,
                    ..
                } = &mut item.kind
                {
                    *start_arrow = target.start;
                    *end_arrow = target.end;
                }
            }
        }
    }
}

impl Command for SetArrowHeads {
    fn redo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, true);
        self.items = items;
    }

    fn undo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, false);
        self.items = items;
    }

    fn skip_first_redo(&self) -> bool {
        self.preview_already_applied
    }
}

// ─────────────────────────── Set closed ───────────────────────────

/// 批量切换线性对象（Polyline）闭合状态命令（闭合时首尾相连、可填充）。
/// 整批只占一条 undo 记录（D3）；单项用 [`Self::new`]，多选批量用 [`Self::new_batch`]。
pub struct SetClosed {
    items: Vec<(ItemId, bool, bool)>,
    preview_already_applied: bool,
}

impl SetClosed {
    pub fn new(item_id: ItemId, old_closed: bool, new_closed: bool) -> Self {
        Self {
            items: vec![(item_id, old_closed, new_closed)],
            preview_already_applied: false,
        }
    }

    /// 批量构造：`(item_id, old, new)` 三元组列表。
    pub fn new_batch(items: Vec<(ItemId, bool, bool)>) -> Self {
        Self {
            items,
            preview_already_applied: false,
        }
    }

    /// 声明是否为预览模式（UI 已直接改 item 时传 true，push 跳过首次 redo）。
    pub fn with_preview_applied(mut self, applied: bool) -> Self {
        self.preview_already_applied = applied;
        self
    }

    fn apply(scene: &mut Scene, items: &[(ItemId, bool, bool)], new: bool) {
        for (id, old, new_closed) in items {
            let value = if new { *new_closed } else { *old };
            if let Some(item) = scene.get_item_mut(id) {
                if let ItemKind::Shape { closed, .. } = &mut item.kind {
                    *closed = value;
                }
            }
        }
    }
}

impl Command for SetClosed {
    fn redo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, true);
        self.items = items;
    }

    fn undo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, false);
        self.items = items;
    }

    fn skip_first_redo(&self) -> bool {
        self.preview_already_applied
    }
}

// ─────────────────────────── Set curve type ───────────────────────────

/// 批量切换线性对象（Polyline）曲线模式（Straight / Curved，Phase I）。
/// 整批只占一条 undo 记录（D3）；单项用 [`Self::new`]，多选批量用 [`Self::new_batch`]。
pub struct SetCurveType {
    items: Vec<(ItemId, CurveType, CurveType)>,
    preview_already_applied: bool,
}

impl SetCurveType {
    pub fn new(item_id: ItemId, old_curve: CurveType, new_curve: CurveType) -> Self {
        Self {
            items: vec![(item_id, old_curve, new_curve)],
            preview_already_applied: false,
        }
    }

    /// 批量构造：`(item_id, old, new)` 三元组列表。
    pub fn new_batch(items: Vec<(ItemId, CurveType, CurveType)>) -> Self {
        Self {
            items,
            preview_already_applied: false,
        }
    }

    /// 声明是否为预览模式（UI 已直接改 item 时传 true，push 跳过首次 redo）。
    pub fn with_preview_applied(mut self, applied: bool) -> Self {
        self.preview_already_applied = applied;
        self
    }

    fn apply(scene: &mut Scene, items: &[(ItemId, CurveType, CurveType)], new: bool) {
        for (id, old, new_curve) in items {
            let value = if new { *new_curve } else { *old };
            if let Some(item) = scene.get_item_mut(id) {
                if let ItemKind::Shape { curve_type, .. } = &mut item.kind {
                    *curve_type = value;
                }
            }
        }
    }
}

impl Command for SetCurveType {
    fn redo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, true);
        self.items = items;
    }

    fn undo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, false);
        self.items = items;
    }

    fn skip_first_redo(&self) -> bool {
        self.preview_already_applied
    }
}

// ─────────────────────────── Set roundness ───────────────────────────

/// 批量设置矩形族圆角比例（0..1，Phase I）。仅矩形族生效。
/// 整批只占一条 undo 记录（D3）；单项用 [`Self::new`]，多选批量用 [`Self::new_batch`]。
pub struct SetRoundness {
    items: Vec<(ItemId, f32, f32)>,
    preview_already_applied: bool,
}

impl SetRoundness {
    pub fn new(item_id: ItemId, old_roundness: f32, new_roundness: f32) -> Self {
        Self {
            items: vec![(item_id, old_roundness, new_roundness)],
            preview_already_applied: false,
        }
    }

    /// 批量构造：`(item_id, old, new)` 三元组列表（滑块拖动结束时一次性入栈）。
    pub fn new_batch(items: Vec<(ItemId, f32, f32)>) -> Self {
        Self {
            items,
            preview_already_applied: false,
        }
    }

    /// 声明是否为预览模式（滑块拖动中 UI 已直接改 item，释放时传 true）。
    pub fn with_preview_applied(mut self, applied: bool) -> Self {
        self.preview_already_applied = applied;
        self
    }

    fn apply(scene: &mut Scene, items: &[(ItemId, f32, f32)], new: bool) {
        for (id, old, new_roundness) in items {
            let value = if new { *new_roundness } else { *old };
            if let Some(item) = scene.get_item_mut(id) {
                if let ItemKind::Shape { roundness, .. } = &mut item.kind {
                    *roundness = value;
                }
            }
        }
    }
}

impl Command for SetRoundness {
    fn redo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, true);
        self.items = items;
    }

    fn undo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, false);
        self.items = items;
    }

    fn skip_first_redo(&self) -> bool {
        self.preview_already_applied
    }
}

// ─────────────────────────── Set rough ───────────────────────────

/// 批量切换 Shape 手绘风描边命令（Phase F）。开启时由 RoughStyler 渲染抖动描边。
/// 经 `Item::set_rough` 写入，顺带在 seed 为 0 时生成随机种子。
/// 整批只占一条 undo 记录（D3）；单项用 [`Self::new`]，多选批量用 [`Self::new_batch`]。
pub struct SetRough {
    items: Vec<(ItemId, bool, bool)>,
    preview_already_applied: bool,
}

impl SetRough {
    pub fn new(item_id: ItemId, old_rough: bool, new_rough: bool) -> Self {
        Self {
            items: vec![(item_id, old_rough, new_rough)],
            preview_already_applied: false,
        }
    }

    /// 批量构造：`(item_id, old, new)` 三元组列表。
    pub fn new_batch(items: Vec<(ItemId, bool, bool)>) -> Self {
        Self {
            items,
            preview_already_applied: false,
        }
    }

    /// 声明是否为预览模式（UI 已直接改 item 时传 true，push 跳过首次 redo）。
    pub fn with_preview_applied(mut self, applied: bool) -> Self {
        self.preview_already_applied = applied;
        self
    }

    fn apply(scene: &mut Scene, items: &[(ItemId, bool, bool)], new: bool) {
        for (id, old, new_rough) in items {
            let value = if new { *new_rough } else { *old };
            if let Some(item) = scene.get_item_mut(id) {
                item.set_rough(value);
            }
        }
    }
}

impl Command for SetRough {
    fn redo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, true);
        self.items = items;
    }

    fn undo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, false);
        self.items = items;
    }

    fn skip_first_redo(&self) -> bool {
        self.preview_already_applied
    }
}

// ─────────────────────────── Set stroke style ───────────────────────────

/// 批量设置 Shape 描边样式（颜色 / 线宽 / 线型，Phase H）。
/// 每项自带 old/new，整批只占一条 undo 记录（D3）。
pub struct SetStrokeStyle {
    items: Vec<(ItemId, StrokeStyle, StrokeStyle)>,
    preview_already_applied: bool,
}

impl SetStrokeStyle {
    pub fn new(item_id: ItemId, old_stroke: StrokeStyle, new_stroke: StrokeStyle) -> Self {
        Self {
            items: vec![(item_id, old_stroke, new_stroke)],
            preview_already_applied: false,
        }
    }

    /// 批量构造：`(item_id, old, new)` 三元组列表。
    pub fn new_batch(items: Vec<(ItemId, StrokeStyle, StrokeStyle)>) -> Self {
        Self {
            items,
            preview_already_applied: false,
        }
    }

    /// 声明是否为预览模式（滑块拖动中 UI 已直接改 item，释放时传 true）。
    pub fn with_preview_applied(mut self, applied: bool) -> Self {
        self.preview_already_applied = applied;
        self
    }

    fn apply(scene: &mut Scene, items: &[(ItemId, StrokeStyle, StrokeStyle)], new: bool) {
        for (id, old, new_stroke) in items {
            let value = if new { *new_stroke } else { *old };
            if let Some(item) = scene.get_item_mut(id) {
                if let ItemKind::Shape { stroke, .. } = &mut item.kind {
                    *stroke = value;
                }
            }
        }
    }
}

impl Command for SetStrokeStyle {
    fn redo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, true);
        self.items = items;
    }

    fn undo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, false);
        self.items = items;
    }

    fn skip_first_redo(&self) -> bool {
        self.preview_already_applied
    }
}

// ─────────────────────────── Set shape fill ───────────────────────────

/// 填充状态快照：颜色 + 样式。`color: None` = 无填充（此时 style 无意义，
/// 与 Excalidraw "transparent" 同语义）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FillState {
    pub color: Option<[u8; 4]>,
    pub style: FillStyle,
}

/// 一次填充变更的批量条目：`(item_id, old, new)`。
/// 起别名而非裸写，避免触发 `clippy::type_complexity`。
pub type FillChange = (ItemId, FillState, FillState);

/// 批量设置 Shape 填充（Phase H 为颜色，本批次扩展为颜色+样式）。
/// 每项自带 old/new，整批只占一条 undo 记录（D3）。
pub struct SetShapeFill {
    items: Vec<FillChange>,
    preview_already_applied: bool,
}

impl SetShapeFill {
    pub fn new(item_id: ItemId, old_fill: FillState, new_fill: FillState) -> Self {
        Self {
            items: vec![(item_id, old_fill, new_fill)],
            preview_already_applied: false,
        }
    }

    /// 批量构造：`(item_id, old, new)` 三元组列表。
    pub fn new_batch(items: Vec<FillChange>) -> Self {
        Self {
            items,
            preview_already_applied: false,
        }
    }

    /// 声明是否为预览模式（UI 已直接改 item 时传 true，push 跳过首次 redo）。
    pub fn with_preview_applied(mut self, applied: bool) -> Self {
        self.preview_already_applied = applied;
        self
    }

    fn apply(scene: &mut Scene, items: &[FillChange], new: bool) {
        for (id, old, new_fill) in items {
            let value = if new { *new_fill } else { *old };
            if let Some(item) = scene.get_item_mut(id) {
                if let ItemKind::Shape {
                    fill, fill_style, ..
                } = &mut item.kind
                {
                    *fill = value.color;
                    *fill_style = value.style;
                }
            }
        }
    }
}

impl Command for SetShapeFill {
    fn redo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, true);
        self.items = items;
    }

    fn undo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, false);
        self.items = items;
    }

    fn skip_first_redo(&self) -> bool {
        self.preview_already_applied
    }
}

// ─────────────────────────── Set text style ───────────────────────────

/// 批量设置 Text item 样式（字号 / 颜色 / 背景，Phase H）。
///
/// 命令层按**整份样式快照**存 old/new，UI 上三个控件任一变化都走同一条路径，
/// 因此一次改动只产生一条 undo 记录。背景消费 `ItemKind::Text::background`
/// 这个预留字段（默认 `None` = 不画底色）。
pub struct SetTextStyle {
    items: Vec<(ItemId, TextStyle, TextStyle)>,
    preview_already_applied: bool,
}

impl SetTextStyle {
    pub fn new(item_id: ItemId, old_style: TextStyle, new_style: TextStyle) -> Self {
        Self {
            items: vec![(item_id, old_style, new_style)],
            preview_already_applied: false,
        }
    }

    /// 批量构造：`(item_id, old, new)` 三元组列表。
    pub fn new_batch(items: Vec<(ItemId, TextStyle, TextStyle)>) -> Self {
        Self {
            items,
            preview_already_applied: false,
        }
    }

    /// 声明是否为预览模式（滑块拖动中 UI 已直接改 item，释放时传 true）。
    pub fn with_preview_applied(mut self, applied: bool) -> Self {
        self.preview_already_applied = applied;
        self
    }

    fn apply(scene: &mut Scene, items: &[(ItemId, TextStyle, TextStyle)], new: bool) {
        for (id, old, new_style) in items {
            let value = if new { *new_style } else { *old };
            if let Some(item) = scene.get_item_mut(id) {
                if let ItemKind::Text {
                    font_size,
                    color,
                    background,
                    measured_size,
                    ..
                } = &mut item.kind
                {
                    *font_size = value.font_size;
                    *color = value.color;
                    *background = value.background;
                    // 字号变了，缓存的测量尺寸作废，下一帧重测（否则变换框尺寸滞后）
                    *measured_size = None;
                }
            }
        }
    }
}

impl Command for SetTextStyle {
    fn redo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, true);
        self.items = items;
    }

    fn undo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, false);
        self.items = items;
    }

    fn skip_first_redo(&self) -> bool {
        self.preview_already_applied
    }
}

// ─────────────────────────── Set pixmap style ───────────────────────────

/// 批量设置 Pixmap item 样式（不透明度 / 灰度，Phase H）。
///
/// 与 [`SetTextStyle`] 同理：两个控件共享一条 undo 记录。
/// 只覆盖这两个字段——裁剪矩形（`crop`）有自己的交互（裁剪模式）与命令
/// [`SetPixmapProps`]，不并入此处的"样式"概念。
pub struct SetPixmapStyle {
    items: Vec<(ItemId, PixmapStyle, PixmapStyle)>,
    preview_already_applied: bool,
}

impl SetPixmapStyle {
    pub fn new(item_id: ItemId, old_style: PixmapStyle, new_style: PixmapStyle) -> Self {
        Self {
            items: vec![(item_id, old_style, new_style)],
            preview_already_applied: false,
        }
    }

    /// 批量构造：`(item_id, old, new)` 三元组列表。
    pub fn new_batch(items: Vec<(ItemId, PixmapStyle, PixmapStyle)>) -> Self {
        Self {
            items,
            preview_already_applied: false,
        }
    }

    /// 声明是否为预览模式（滑块拖动中 UI 已直接改 item，释放时传 true）。
    pub fn with_preview_applied(mut self, applied: bool) -> Self {
        self.preview_already_applied = applied;
        self
    }

    fn apply(scene: &mut Scene, items: &[(ItemId, PixmapStyle, PixmapStyle)], new: bool) {
        for (id, old, new_style) in items {
            let value = if new { *new_style } else { *old };
            if let Some(item) = scene.get_item_mut(id) {
                if let ItemKind::Pixmap {
                    opacity, grayscale, ..
                } = &mut item.kind
                {
                    *opacity = value.opacity;
                    *grayscale = value.grayscale;
                }
            }
        }
    }
}

impl Command for SetPixmapStyle {
    fn redo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, true);
        self.items = items;
    }

    fn undo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, false);
        self.items = items;
    }

    fn skip_first_redo(&self) -> bool {
        self.preview_already_applied
    }
}

// ─────────────────────────── Set frame number ───────────────────────────

/// 批量设置画框编号（Phase H）。
///
/// 侧栏的编号控件用它；整批改号只占一条 undo 记录。
/// 注意它**不做重排**——改完可能与其他画框撞号，那是 [`RenumberFrame`] 的职责。
pub struct SetFrameNumber {
    items: Vec<(ItemId, u32, u32)>,
    preview_already_applied: bool,
}

impl SetFrameNumber {
    pub fn new(item_id: ItemId, old_number: u32, new_number: u32) -> Self {
        Self {
            items: vec![(item_id, old_number, new_number)],
            preview_already_applied: false,
        }
    }

    /// 批量构造：`(item_id, old, new)` 三元组列表。
    pub fn new_batch(items: Vec<(ItemId, u32, u32)>) -> Self {
        Self {
            items,
            preview_already_applied: false,
        }
    }

    /// 声明是否为预览模式（UI 已直接改 item 时传 true，push 跳过首次 redo）。
    pub fn with_preview_applied(mut self, applied: bool) -> Self {
        self.preview_already_applied = applied;
        self
    }

    fn apply(scene: &mut Scene, items: &[(ItemId, u32, u32)], new: bool) {
        for (id, old, new_number) in items {
            let value = if new { *new_number } else { *old };
            if let Some(item) = scene.get_item_mut(id) {
                if let ItemKind::Frame { number, .. } = &mut item.kind {
                    *number = value;
                }
            }
        }
    }
}

impl Command for SetFrameNumber {
    fn redo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, true);
        self.items = items;
    }

    fn undo(&mut self, scene: &mut Scene) {
        let items = std::mem::take(&mut self.items);
        Self::apply(scene, &items, false);
        self.items = items;
    }

    fn skip_first_redo(&self) -> bool {
        self.preview_already_applied
    }
}

// ─────────────────────────── Move ───────────────────────────

/// 平移多个 item（拖拽移动的命令）。
pub struct MoveItems {
    item_ids: Vec<ItemId>,
    delta: CanvasVector,
    preview_already_applied: bool,
}

impl MoveItems {
    pub fn new(item_ids: Vec<ItemId>, delta: CanvasVector) -> Self {
        Self {
            item_ids,
            delta,
            preview_already_applied: true,
        }
    }

    pub fn with_preview_applied(mut self, applied: bool) -> Self {
        self.preview_already_applied = applied;
        self
    }
}

impl Command for MoveItems {
    fn redo(&mut self, scene: &mut Scene) {
        for id in &self.item_ids {
            if let Some(item) = scene.get_item_mut(id) {
                item.transform.pos += self.delta;
            }
        }
    }

    fn undo(&mut self, scene: &mut Scene) {
        for id in &self.item_ids {
            if let Some(item) = scene.get_item_mut(id) {
                item.transform.pos -= self.delta;
            }
        }
    }

    fn skip_first_redo(&self) -> bool {
        self.preview_already_applied
    }
}

// ─────────────────────────── Scale ───────────────────────────

// 注意：ScaleItems/RotateItems 用统一的 factor/angle，对"以锚点缩放/旋转"场景
// 不够精确。交互层应优先使用 TransformItem（带 old/new transform）。
// 这两个命令保留给键盘快捷键等"无锚点"批量操作。

pub struct ScaleItems {
    item_ids: Vec<ItemId>,
    factor: f32,
}

impl ScaleItems {
    pub fn new(item_ids: Vec<ItemId>, factor: f32) -> Self {
        Self { item_ids, factor }
    }
}

impl Command for ScaleItems {
    fn redo(&mut self, scene: &mut Scene) {
        for id in &self.item_ids {
            if let Some(item) = scene.get_item_mut(id) {
                item.transform.scale.x *= self.factor;
                item.transform.scale.y *= self.factor;
            }
        }
    }

    fn undo(&mut self, scene: &mut Scene) {
        for id in &self.item_ids {
            if let Some(item) = scene.get_item_mut(id) {
                item.transform.scale.x /= self.factor;
                item.transform.scale.y /= self.factor;
            }
        }
    }
}

// ─────────────────────────── Rotate ───────────────────────────

pub struct RotateItems {
    item_ids: Vec<ItemId>,
    angle: f32,
}

impl RotateItems {
    pub fn new(item_ids: Vec<ItemId>, angle: f32) -> Self {
        Self { item_ids, angle }
    }
}

impl Command for RotateItems {
    fn redo(&mut self, scene: &mut Scene) {
        for id in &self.item_ids {
            if let Some(item) = scene.get_item_mut(id) {
                item.transform.rotation += self.angle;
            }
        }
    }

    fn undo(&mut self, scene: &mut Scene) {
        for id in &self.item_ids {
            if let Some(item) = scene.get_item_mut(id) {
                item.transform.rotation -= self.angle;
            }
        }
    }
}

// ─────────────────────────── Flip ───────────────────────────

/// 翻转多个 item（边手柄触发，spec L239「翻转边」）。
pub struct FlipItems {
    item_ids: Vec<ItemId>,
    horizontal: bool, // true=flip_h, false=flip_v
}

impl FlipItems {
    pub fn new(item_ids: Vec<ItemId>, horizontal: bool) -> Self {
        Self {
            item_ids,
            horizontal,
        }
    }
}

impl Command for FlipItems {
    fn redo(&mut self, scene: &mut Scene) {
        for id in &self.item_ids {
            if let Some(item) = scene.get_item_mut(id) {
                if self.horizontal {
                    item.transform.flip_h = !item.transform.flip_h;
                } else {
                    item.transform.flip_v = !item.transform.flip_v;
                }
            }
        }
    }

    fn undo(&mut self, scene: &mut Scene) {
        // 翻转自反
        self.redo(scene);
    }
}

// ─────────────────────────── Delete ───────────────────────────

/// 删除多个 item。`undo` 会按原 Z 序与位置还原（存了完整快照）。
pub struct DeleteItems {
    item_ids: Vec<ItemId>,
    /// 第一次 redo 时填入被删 item 的快照（按删除前的 items 顺序），
    /// 用于 undo 恢复。Vec<Option<...>> 是因为 redo 后 item 已不在 scene。
    snapshots: std::cell::RefCell<Vec<Option<crate::item::Item>>>,
}

impl DeleteItems {
    pub fn new(item_ids: Vec<ItemId>) -> Self {
        Self {
            item_ids,
            snapshots: std::cell::RefCell::new(Vec::new()),
        }
    }
}

impl Command for DeleteItems {
    fn redo(&mut self, scene: &mut Scene) {
        // 第一次 redo 时抓快照（snapshots 为空），之后 redo（重做）时清空再删
        let mut snaps = self.snapshots.borrow_mut();
        if snaps.is_empty() {
            for id in &self.item_ids {
                let snap = scene.get_item(id).cloned();
                snaps.push(snap);
            }
        }
        for id in &self.item_ids {
            scene.remove_item(id);
        }
    }

    fn undo(&mut self, scene: &mut Scene) {
        let snaps = self.snapshots.borrow();
        // 用 preserve_z 恢复，保留 item 原本的 z 值
        for item in snaps.iter().flatten() {
            scene.add_item_preserve_z(item.clone());
        }
        // 保留 snapshots 以便下次 redo 复用（不必重新抓）
    }
}

// ─────────────────────────── Add ───────────────────────────

pub struct AddItem {
    item: crate::item::Item,
}

impl AddItem {
    pub fn new(item: crate::item::Item) -> Self {
        Self { item }
    }

    pub fn item_id(&self) -> ItemId {
        self.item.id
    }
}

impl Command for AddItem {
    fn redo(&mut self, scene: &mut Scene) {
        scene.add_item(self.item.clone());
    }

    fn undo(&mut self, scene: &mut Scene) {
        scene.remove_item(&self.item.id);
    }
}

// ─────────────────────────── Reorder ───────────────────────────

/// 置顶/置底多个 item。
pub struct ReorderItems {
    item_ids: Vec<ItemId>,
    to_front: bool,
    /// 旧 z 值，按 item_ids 顺序记录。
    old_z: Vec<(ItemId, i32)>,
    /// 新 z 值（redo 时填入）。
    new_z: Vec<(ItemId, i32)>,
}

impl ReorderItems {
    pub fn new(item_ids: Vec<ItemId>, to_front: bool) -> Self {
        Self {
            item_ids,
            to_front,
            old_z: Vec::new(),
            new_z: Vec::new(),
        }
    }
}

impl Command for ReorderItems {
    fn redo(&mut self, scene: &mut Scene) {
        // 第一次 redo：记录 old_z，计算 new_z
        if self.old_z.is_empty() {
            for id in &self.item_ids {
                if let Some(item) = scene.get_item(id) {
                    self.old_z.push((*id, item.z));
                }
            }
            if self.to_front {
                let base = scene.next_z;
                for (i, id) in self.item_ids.iter().enumerate() {
                    self.new_z.push((*id, base + i as i32));
                }
            } else {
                let min_z = scene.items.iter().map(|i| i.z).min().unwrap_or(0);
                let n = self.item_ids.len() as i32;
                for (i, id) in self.item_ids.iter().enumerate() {
                    self.new_z.push((*id, min_z - (n - i as i32)));
                }
            }
        }
        for (id, z) in &self.new_z {
            if let Some(item) = scene.get_item_mut(id) {
                item.z = *z;
            }
        }
        if self.to_front {
            // next_z 推进
            if let Some((_, z)) = self.new_z.last() {
                scene.next_z = scene.next_z.max(*z + 1);
            }
        }
    }

    fn undo(&mut self, scene: &mut Scene) {
        for (id, z) in &self.old_z {
            if let Some(item) = scene.get_item_mut(id) {
                item.z = *z;
            }
        }
    }
}

// ─────────────────────────── Arrange ───────────────────────────

/// 排列多个 item（Linear / Grid 等）。存储 (id, old_pos, new_pos)。
pub struct ArrangeItems {
    moves: Vec<(ItemId, CanvasVector, CanvasVector)>, // id, old_pos, new_pos
    preview_already_applied: bool,
}

impl ArrangeItems {
    pub fn new(moves: Vec<(ItemId, CanvasVector, CanvasVector)>) -> Self {
        Self {
            moves,
            preview_already_applied: false,
        }
    }

    pub fn with_preview_applied(mut self, applied: bool) -> Self {
        self.preview_already_applied = applied;
        self
    }
}

impl Command for ArrangeItems {
    fn redo(&mut self, scene: &mut Scene) {
        for (id, _old, new) in &self.moves {
            if let Some(item) = scene.get_item_mut(id) {
                item.transform.pos = *new;
            }
        }
    }

    fn undo(&mut self, scene: &mut Scene) {
        for (id, old, _new) in &self.moves {
            if let Some(item) = scene.get_item_mut(id) {
                item.transform.pos = *old;
            }
        }
    }

    fn skip_first_redo(&self) -> bool {
        self.preview_already_applied
    }
}

// ─────────────────────────── EditTextContent ───────────────────────────

/// 修改 Text item 的 content（双击编辑，spec L243 P2-5）。
/// redo/undo 都会清空 `measured_size`，强制 UI 层重新测量，确保变换边框与新内容一致（修 B6）。
pub struct EditTextContent {
    item_id: ItemId,
    old_content: String,
    new_content: String,
}

impl EditTextContent {
    pub fn new(item_id: ItemId, old_content: String, new_content: String) -> Self {
        Self {
            item_id,
            old_content,
            new_content,
        }
    }
}

impl Command for EditTextContent {
    fn redo(&mut self, scene: &mut Scene) {
        if let Some(item) = scene.get_item_mut(&self.item_id) {
            if let ItemKind::Text {
                content,
                measured_size,
                ..
            } = &mut item.kind
            {
                *content = self.new_content.clone();
                *measured_size = None; // 强制重新测量
            }
        }
    }

    fn undo(&mut self, scene: &mut Scene) {
        if let Some(item) = scene.get_item_mut(&self.item_id) {
            if let ItemKind::Text {
                content,
                measured_size,
                ..
            } = &mut item.kind
            {
                *content = self.old_content.clone();
                *measured_size = None;
            }
        }
    }
}

// ─────────────────────────── SetPixmapProps ───────────────────────────

/// 修改 Pixmap item 的 opacity / grayscale / crop 任意组合（spec §2.2 灰度/透明度/裁剪）。
/// `old_opacity / new_opacity`、`old_grayscale / new_grayscale`、`old_crop / new_crop`
/// 任一对为 `None` 表示不修改该字段。
pub struct SetPixmapProps {
    item_id: ItemId,
    old_opacity: Option<f32>,
    new_opacity: Option<f32>,
    old_grayscale: Option<bool>,
    new_grayscale: Option<bool>,
    old_crop: Option<Option<CropRect>>,
    new_crop: Option<Option<CropRect>>,
}

impl SetPixmapProps {
    pub fn new(item_id: ItemId) -> Self {
        Self {
            item_id,
            old_opacity: None,
            new_opacity: None,
            old_grayscale: None,
            new_grayscale: None,
            old_crop: None,
            new_crop: None,
        }
    }

    pub fn with_opacity(mut self, old: f32, new: f32) -> Self {
        self.old_opacity = Some(old);
        self.new_opacity = Some(new);
        self
    }

    pub fn with_grayscale(mut self, old: bool, new: bool) -> Self {
        self.old_grayscale = Some(old);
        self.new_grayscale = Some(new);
        self
    }

    pub fn with_crop(mut self, old: Option<CropRect>, new: Option<CropRect>) -> Self {
        self.old_crop = Some(old);
        self.new_crop = Some(new);
        self
    }
}

impl Command for SetPixmapProps {
    fn redo(&mut self, scene: &mut Scene) {
        if let Some(item) = scene.get_item_mut(&self.item_id) {
            if let ItemKind::Pixmap {
                opacity,
                grayscale,
                crop,
                ..
            } = &mut item.kind
            {
                if let Some(v) = self.new_opacity {
                    *opacity = v;
                }
                if let Some(v) = self.new_grayscale {
                    *grayscale = v;
                }
                if let Some(v) = &self.new_crop {
                    *crop = *v;
                }
            }
        }
    }

    fn undo(&mut self, scene: &mut Scene) {
        if let Some(item) = scene.get_item_mut(&self.item_id) {
            if let ItemKind::Pixmap {
                opacity,
                grayscale,
                crop,
                ..
            } = &mut item.kind
            {
                if let Some(v) = self.old_opacity {
                    *opacity = v;
                }
                if let Some(v) = self.old_grayscale {
                    *grayscale = v;
                }
                if let Some(v) = &self.old_crop {
                    *crop = *v;
                }
            }
        }
    }
}

// ─────────────────────────── CropItems ───────────────────────────

/// 修改 Pixmap item 的 crop（spec §2.2 裁剪）。
///
/// 同时记录 transform 变化：裁剪确认后 item 的边框（canvas_corners）应等于裁剪框
/// 在画布上的位置和尺寸，因此 apply_crop 会同时调整 transform.pos 和 transform.scale。
/// 用一条命令同时改 crop + transform，使 undo 一次回滚到裁剪前状态。
pub struct CropItems {
    inner: SetPixmapProps,
    item_id: ItemId,
    old_transform: Transform,
    new_transform: Transform,
    transform_applied: bool,
}

impl CropItems {
    pub fn new(
        item_id: ItemId,
        old_crop: Option<CropRect>,
        new_crop: Option<CropRect>,
        old_transform: Transform,
        new_transform: Transform,
    ) -> Self {
        Self {
            inner: SetPixmapProps::new(item_id).with_crop(old_crop, new_crop),
            item_id,
            old_transform,
            new_transform,
            transform_applied: false,
        }
    }
}

impl Command for CropItems {
    fn redo(&mut self, scene: &mut Scene) {
        self.inner.redo(scene);
        if let Some(item) = scene.get_item_mut(&self.item_id) {
            item.transform = self.new_transform;
        }
        self.transform_applied = true;
    }

    fn undo(&mut self, scene: &mut Scene) {
        if self.transform_applied {
            if let Some(item) = scene.get_item_mut(&self.item_id) {
                item.transform = self.old_transform;
            }
            self.transform_applied = false;
        }
        self.inner.undo(scene);
    }
}

// ─────────────────────────── NormalizeItems ───────────────────────────

/// 归一化选中 Pixmap item 的尺寸（spec §2.2 批量操作：归一化尺寸）。
///
/// - `Width`：所有 item 渲染宽度统一（scale.x = target_width / original_width）
/// - `Height`：所有 item 渲染高度统一
/// - `Area`：所有 item 渲染面积统一（scale.x * scale.y = target_area / (original_w * original_h)，
///   保持各自宽高比，取统一 factor）
///
/// 存储 (id, old_transform, new_transform) 以支持完整 undo。
pub struct NormalizeItems {
    item_ids: Vec<ItemId>,
    mode: NormalizeMode,
    /// 记录原始 transform，undo 时恢复。
    old_transforms: Vec<(ItemId, Transform)>,
    /// 计算 new_transform 所需的目标值（redo 时计算并缓存）。
    target: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NormalizeMode {
    Width,
    Height,
    Area,
}

impl NormalizeItems {
    /// 创建归一化命令。target 由调用方根据选中 item 决定（通常取首个 item 的当前值）。
    pub fn new(item_ids: Vec<ItemId>, mode: NormalizeMode, target: f32) -> Self {
        Self {
            item_ids,
            mode,
            old_transforms: Vec::new(),
            target: Some(target),
        }
    }
}

impl Command for NormalizeItems {
    fn redo(&mut self, scene: &mut Scene) {
        // 首次 redo：记录 old_transforms 并计算 new_transforms
        if self.old_transforms.is_empty() {
            for id in &self.item_ids {
                if let Some(item) = scene.get_item(id) {
                    self.old_transforms.push((*id, item.transform));
                }
            }
        }
        let target = self.target.unwrap_or(0.0);
        for (id, _old) in &self.old_transforms {
            if let Some(item) = scene.get_item_mut(id) {
                if let ItemKind::Pixmap { original_size, .. } = &item.kind {
                    let ow = original_size.0 as f32;
                    let oh = original_size.1 as f32;
                    match self.mode {
                        NormalizeMode::Width => {
                            if ow > 0.0 {
                                item.transform.scale.x = target / ow;
                            }
                        }
                        NormalizeMode::Height => {
                            if oh > 0.0 {
                                item.transform.scale.y = target / oh;
                            }
                        }
                        NormalizeMode::Area => {
                            // 保持宽高比：factor = sqrt(target / (ow * oh * old_sx * old_sy))
                            // 但用 base area = ow * oh，则 factor^2 * ow * oh = target
                            let base_area = (ow * oh).max(1e-6);
                            let factor = (target / base_area).sqrt();
                            item.transform.scale.x = factor;
                            item.transform.scale.y = factor;
                        }
                    }
                }
            }
        }
    }

    fn undo(&mut self, scene: &mut Scene) {
        for (id, old_tf) in &self.old_transforms {
            if let Some(item) = scene.get_item_mut(id) {
                item.transform = *old_tf;
            }
        }
    }
}

// ─────────────────────────── Frame renumber ───────────────────────────

/// 画框编号重排命令（Phase D）。redo 应用冲突顺移，undo 还原。
/// 使用 [`Scene::plan_frame_renumber`] 生成的计划，经 [`Scene::apply_renumber`] /
/// [`Scene::undo_renumber`] 执行。
pub struct RenumberFrame {
    plan: RenumberPlan,
    applied: bool,
}

impl RenumberFrame {
    pub fn new(plan: RenumberPlan) -> Self {
        Self {
            plan,
            applied: false,
        }
    }
}

impl Command for RenumberFrame {
    fn redo(&mut self, scene: &mut Scene) {
        if self.applied {
            return;
        }
        self.applied = true;
        scene.apply_renumber(&self.plan);
    }

    fn undo(&mut self, scene: &mut Scene) {
        if !self.applied {
            return;
        }
        self.applied = false;
        scene.undo_renumber(&self.plan);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;
    use crate::shape::{DashStyle, ShapeType, StrokeStyle};

    fn shape_in(scene: &mut Scene) -> ItemId {
        let item = Item::new_shape(
            ShapeType::Rectangle,
            (10.0, 10.0),
            0.0,
            0.0,
            StrokeStyle::default(),
            None,
        );
        let id = item.id;
        scene.add_item(item);
        id
    }

    #[test]
    fn set_rough_redo_undo_restores_previous_state() {
        let mut scene = Scene::new();
        let id = shape_in(&mut scene);
        assert!(!scene.get_item(&id).unwrap().rough());

        let mut cmd = SetRough::new(id, false, true);
        cmd.redo(&mut scene);
        assert!(scene.get_item(&id).unwrap().rough());
        cmd.undo(&mut scene);
        assert!(!scene.get_item(&id).unwrap().rough());
    }

    #[test]
    fn set_rough_generates_seed_and_keeps_it_across_undo() {
        let mut scene = Scene::new();
        let id = shape_in(&mut scene);

        let mut cmd = SetRough::new(id, false, true);
        cmd.redo(&mut scene);
        let seed = match &scene.get_item(&id).unwrap().kind {
            ItemKind::Shape { seed, .. } => *seed,
            _ => panic!("expected Shape kind"),
        };
        assert_ne!(seed, 0, "redo 应生成非 0 种子");

        cmd.undo(&mut scene);
        cmd.redo(&mut scene);
        let seed2 = match &scene.get_item(&id).unwrap().kind {
            ItemKind::Shape { seed, .. } => *seed,
            _ => panic!("expected Shape kind"),
        };
        assert_eq!(seed, seed2, "undo/redo 往返应保持种子不变");
    }

    #[test]
    fn set_rough_on_non_shape_is_noop() {
        let mut scene = Scene::new();
        let txt = Item::new_text("x".to_string(), 0.0, 0.0, 16.0, [255; 4]);
        let id = txt.id;
        scene.add_item(txt);
        let mut cmd = SetRough::new(id, false, true);
        cmd.redo(&mut scene);
        assert!(!scene.get_item(&id).unwrap().rough());
    }

    // ───────── 批量命令（Phase H / D3：多选一次改动一条 undo） ─────────

    fn two_shapes(scene: &mut Scene) -> (ItemId, ItemId) {
        (shape_in(scene), shape_in(scene))
    }

    fn text_in(scene: &mut Scene) -> ItemId {
        let item = Item::new_text("hi".to_string(), 0.0, 0.0, 16.0, [255, 255, 255, 255]);
        let id = item.id;
        scene.add_item(item);
        id
    }

    fn roundness_of(scene: &Scene, id: ItemId) -> f32 {
        match &scene.get_item(&id).unwrap().kind {
            ItemKind::Shape { roundness, .. } => *roundness,
            _ => panic!("expected Shape kind"),
        }
    }

    fn curve_of(scene: &Scene, id: ItemId) -> CurveType {
        match &scene.get_item(&id).unwrap().kind {
            ItemKind::Shape { curve_type, .. } => *curve_type,
            _ => panic!("expected Shape kind"),
        }
    }

    fn closed_of(scene: &Scene, id: ItemId) -> bool {
        match &scene.get_item(&id).unwrap().kind {
            ItemKind::Shape { closed, .. } => *closed,
            _ => panic!("expected Shape kind"),
        }
    }

    fn arrows_of(scene: &Scene, id: ItemId) -> (Option<ArrowHeadStyle>, Option<ArrowHeadStyle>) {
        match &scene.get_item(&id).unwrap().kind {
            ItemKind::Shape {
                start_arrow,
                end_arrow,
                ..
            } => (*start_arrow, *end_arrow),
            _ => panic!("expected Shape kind"),
        }
    }

    fn stroke_of(scene: &Scene, id: ItemId) -> StrokeStyle {
        match &scene.get_item(&id).unwrap().kind {
            ItemKind::Shape { stroke, .. } => *stroke,
            _ => panic!("expected Shape kind"),
        }
    }

    fn fill_of(scene: &Scene, id: ItemId) -> Option<[u8; 4]> {
        match &scene.get_item(&id).unwrap().kind {
            ItemKind::Shape { fill, .. } => *fill,
            _ => panic!("expected Shape kind"),
        }
    }

    #[test]
    fn set_rough_batch_toggles_every_item() {
        let mut scene = Scene::new();
        let (a, b) = two_shapes(&mut scene);
        let mut cmd = SetRough::new_batch(vec![(a, false, true), (b, false, true)]);
        cmd.redo(&mut scene);
        assert!(scene.get_item(&a).unwrap().rough());
        assert!(scene.get_item(&b).unwrap().rough());
        cmd.undo(&mut scene);
        assert!(!scene.get_item(&a).unwrap().rough());
        assert!(!scene.get_item(&b).unwrap().rough());
    }

    #[test]
    fn set_roundness_batch_undo_restores_each_items_own_old_value() {
        let mut scene = Scene::new();
        let (a, b) = two_shapes(&mut scene);
        // 两个 item 旧值不同，undo 必须各回各的，不能统一写回某个值
        let mut cmd = SetRoundness::new_batch(vec![(a, 0.0, 0.5), (b, 0.25, 0.5)]);
        cmd.redo(&mut scene);
        assert_eq!(roundness_of(&scene, a), 0.5);
        assert_eq!(roundness_of(&scene, b), 0.5);
        cmd.undo(&mut scene);
        assert_eq!(roundness_of(&scene, a), 0.0);
        assert_eq!(roundness_of(&scene, b), 0.25);
    }

    #[test]
    fn set_curve_type_batch_applies_and_undoes() {
        let mut scene = Scene::new();
        let (a, b) = two_shapes(&mut scene);
        let mut cmd = SetCurveType::new_batch(vec![
            (a, CurveType::Straight, CurveType::Curved),
            (b, CurveType::Straight, CurveType::Curved),
        ]);
        cmd.redo(&mut scene);
        assert_eq!(curve_of(&scene, a), CurveType::Curved);
        assert_eq!(curve_of(&scene, b), CurveType::Curved);
        cmd.undo(&mut scene);
        assert_eq!(curve_of(&scene, a), CurveType::Straight);
        assert_eq!(curve_of(&scene, b), CurveType::Straight);
    }

    #[test]
    fn set_closed_batch_applies_and_undoes() {
        let mut scene = Scene::new();
        let (a, b) = two_shapes(&mut scene);
        let mut cmd = SetClosed::new_batch(vec![(a, false, true), (b, false, true)]);
        cmd.redo(&mut scene);
        assert!(closed_of(&scene, a));
        assert!(closed_of(&scene, b));
        cmd.undo(&mut scene);
        assert!(!closed_of(&scene, a));
        assert!(!closed_of(&scene, b));
    }

    #[test]
    fn set_arrow_heads_batch_sets_both_ends_per_item() {
        let mut scene = Scene::new();
        let (a, b) = two_shapes(&mut scene);
        let old = ArrowHeads::default();
        let new = ArrowHeads {
            start: Some(ArrowHeadStyle::Dot),
            end: Some(ArrowHeadStyle::Arrow),
        };
        let mut cmd = SetArrowHeads::new_batch(vec![(a, old, new), (b, old, new)]);
        cmd.redo(&mut scene);
        assert_eq!(arrows_of(&scene, a), (new.start, new.end));
        assert_eq!(arrows_of(&scene, b), (new.start, new.end));
        cmd.undo(&mut scene);
        assert_eq!(arrows_of(&scene, a), (None, None));
        assert_eq!(arrows_of(&scene, b), (None, None));
    }

    #[test]
    fn set_stroke_style_batch_applies_and_undoes() {
        let mut scene = Scene::new();
        let (a, b) = two_shapes(&mut scene);
        let old = StrokeStyle::default();
        let new = StrokeStyle {
            color: [255, 0, 0, 255],
            width: 4.0,
            dash: DashStyle::Dashed,
        };
        let mut cmd = SetStrokeStyle::new_batch(vec![(a, old, new), (b, old, new)]);
        cmd.redo(&mut scene);
        assert_eq!(stroke_of(&scene, a), new);
        assert_eq!(stroke_of(&scene, b), new);
        cmd.undo(&mut scene);
        assert_eq!(stroke_of(&scene, a), old);
        assert_eq!(stroke_of(&scene, b), old);
    }

    #[test]
    fn set_shape_fill_batch_none_clears_fill_on_undo_restores() {
        let mut scene = Scene::new();
        let (a, b) = two_shapes(&mut scene);
        let solid = |c: Option<[u8; 4]>| FillState {
            color: c,
            style: FillStyle::Solid,
        };
        let old = solid(Some([1, 2, 3, 4]));
        let mut cmd = SetShapeFill::new_batch(vec![
            (a, old, solid(None)),
            (b, solid(None), solid(Some([9, 9, 9, 9]))),
        ]);
        cmd.redo(&mut scene);
        assert_eq!(fill_of(&scene, a), None);
        assert_eq!(fill_of(&scene, b), Some([9, 9, 9, 9]));
        cmd.undo(&mut scene);
        assert_eq!(fill_of(&scene, a), old.color);
        assert_eq!(fill_of(&scene, b), None);
    }

    #[test]
    fn set_shape_fill_roundtrips_style() {
        // 填充样式（Hachure 等）随颜色一起走 old/new 快照
        let mut scene = Scene::new();
        let (a, _b) = two_shapes(&mut scene);
        let old = FillState {
            color: None,
            style: FillStyle::Solid,
        };
        let new = FillState {
            color: Some([200, 30, 30, 255]),
            style: FillStyle::Hachure,
        };
        let mut cmd = SetShapeFill::new(a, old, new);
        cmd.redo(&mut scene);
        match &scene.get_item(&a).unwrap().kind {
            ItemKind::Shape {
                fill, fill_style, ..
            } => {
                assert_eq!(*fill, new.color);
                assert_eq!(*fill_style, FillStyle::Hachure);
            }
            _ => panic!("expected Shape kind"),
        }
        cmd.undo(&mut scene);
        match &scene.get_item(&a).unwrap().kind {
            ItemKind::Shape {
                fill, fill_style, ..
            } => {
                assert_eq!(*fill, None);
                assert_eq!(*fill_style, FillStyle::Solid);
            }
            _ => panic!("expected Shape kind"),
        }
    }

    #[test]
    fn set_text_style_batch_updates_size_color_and_background() {
        let mut scene = Scene::new();
        let id = text_in(&mut scene);
        let old = TextStyle {
            font_size: 16.0,
            color: [255, 255, 255, 255],
            background: None,
        };
        let new = TextStyle {
            font_size: 32.0,
            color: [10, 20, 30, 255],
            background: Some([0, 0, 0, 128]),
        };
        let mut cmd = SetTextStyle::new_batch(vec![(id, old, new)]);
        cmd.redo(&mut scene);
        match &scene.get_item(&id).unwrap().kind {
            ItemKind::Text {
                font_size,
                color,
                background,
                ..
            } => {
                assert_eq!(*font_size, 32.0);
                assert_eq!(*color, [10, 20, 30, 255]);
                assert_eq!(*background, Some([0, 0, 0, 128]));
            }
            _ => panic!("expected Text kind"),
        }
        cmd.undo(&mut scene);
        match &scene.get_item(&id).unwrap().kind {
            ItemKind::Text {
                font_size,
                color,
                background,
                ..
            } => {
                assert_eq!(*font_size, 16.0);
                assert_eq!(*color, [255, 255, 255, 255]);
                assert_eq!(*background, None);
            }
            _ => panic!("expected Text kind"),
        }
    }

    #[test]
    fn set_text_style_invalidates_measured_size_cache() {
        let mut scene = Scene::new();
        let id = text_in(&mut scene);
        if let ItemKind::Text { measured_size, .. } = &mut scene.get_item_mut(&id).unwrap().kind {
            *measured_size = Some((40.0, 20.0));
        }
        let old = TextStyle {
            font_size: 16.0,
            color: [255; 4],
            background: None,
        };
        let mut new = old;
        new.font_size = 48.0;
        let mut cmd = SetTextStyle::new(id, old, new);
        cmd.redo(&mut scene);
        match &scene.get_item(&id).unwrap().kind {
            // 字号变了，缓存尺寸必须作废，否则变换框仍按旧字号画
            ItemKind::Text { measured_size, .. } => assert_eq!(*measured_size, None),
            _ => panic!("expected Text kind"),
        }
    }

    #[test]
    fn batch_commands_skip_first_redo_only_when_declared() {
        let mut scene = Scene::new();
        let (a, _) = two_shapes(&mut scene);
        assert!(
            !SetRoundness::new(a, 0.0, 0.5).skip_first_redo(),
            "默认应为 false（UI 未预改 item）"
        );
        assert!(
            SetRoundness::new(a, 0.0, 0.5)
                .with_preview_applied(true)
                .skip_first_redo(),
            "滑块拖动释放固化时应为 true（预览已直接改过 item）"
        );
    }
}
