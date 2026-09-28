use crate::item::{CropRect, EndpointBinding, ItemId, ItemKind};
use crate::scene::{RenumberPlan, Scene};
use crate::shape::{
    ArrowHeadStyle, CurveType, FillStyle, PixmapStyle, Sloppiness, StrokeStyle, TextStyle,
};
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

/// 复合命令：把多条命令打包成**一条** undo 记录（整体重做 / 整体撤销）。
///
/// 用于一次 UI 操作跨命令类型联动的场景——例如改描边色同时联动绑定文字色
/// 与填充色，需要 `SetStrokeStyle` + `SetTextStyle` + `SetShapeFill` 三条
/// 批量命令合并，撤销时一步回到操作前。
///
/// redo 按构造顺序执行，undo 逆序执行（后改的先撤）。
pub struct MultiCommand {
    cmds: Vec<Box<dyn Command>>,
}

impl MultiCommand {
    pub fn new(cmds: Vec<Box<dyn Command>>) -> Self {
        Self { cmds }
    }
}

impl Command for MultiCommand {
    fn redo(&mut self, scene: &mut Scene) {
        for cmd in &mut self.cmds {
            cmd.redo(scene);
        }
    }

    fn undo(&mut self, scene: &mut Scene) {
        for cmd in self.cmds.iter_mut().rev() {
            cmd.undo(scene);
        }
    }

    fn skip_first_redo(&self) -> bool {
        self.cmds.iter().any(|cmd| cmd.skip_first_redo())
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
        // 联动：形状变换后重算绑定到它的端点（plan #5）
        scene.resolve_bindings(&[self.item_id]);
    }

    fn undo(&mut self, scene: &mut Scene) {
        if let Some(item) = scene.get_item_mut(&self.item_id) {
            item.transform = self.old_transform;
        }
        scene.resolve_bindings(&[self.item_id]);
    }

    fn skip_first_redo(&self) -> bool {
        self.preview_already_applied
    }
}

// ─────────────────────────── Shape points ───────────────────────────

/// 线类（Line/Arrow）端点编辑命令：修改 `points` 并同步 `base_size`（AABB）。
/// 拖拽端点时 UI 已直接改到 item 上，push 时跳过首次 redo。
///
/// 端点吸附（plan #5）会同时改变端点绑定：通过 [`EditShapePoints::with_binding_change`]
/// 传入起/终点的 old/new 绑定。字段为 `Option<Option<EndpointBinding>>`——外层 `None` 表示该
/// 端点本次无绑定变更（保持原值），外层 `Some`、内层 `None` 表示解绑，内层 `Some(id)`
/// 表示绑到 `id`。这样整次端点拖拽只占一条 undo 记录。
///
/// plan #4：顶点删除 / Alt+拖端点延伸 / 端点拖回起点自动闭合都会顺带改 `closed`，
/// 用 [`EditShapePoints::with_closed`] 记录 old/new，与点变更同占一条 undo。
pub struct EditShapePoints {
    item_id: ItemId,
    old_points: Vec<(f32, f32)>,
    new_points: Vec<(f32, f32)>,
    old_start_binding: Option<Option<EndpointBinding>>,
    old_end_binding: Option<Option<EndpointBinding>>,
    new_start_binding: Option<Option<EndpointBinding>>,
    new_end_binding: Option<Option<EndpointBinding>>,
    old_closed: Option<bool>,
    new_closed: Option<bool>,
}

impl EditShapePoints {
    pub fn new(item_id: ItemId, old_points: Vec<(f32, f32)>, new_points: Vec<(f32, f32)>) -> Self {
        Self {
            item_id,
            old_points,
            new_points,
            old_start_binding: None,
            old_end_binding: None,
            new_start_binding: None,
            new_end_binding: None,
            old_closed: None,
            new_closed: None,
        }
    }

    /// 记录端点绑定变更（plan #5 吸附）。`*_old` / `*_new` 为 `Some(binding)` 或
    /// `None`（解绑）。仅当确实改变绑定时才调用，未改变则保持 `None`（无操作）。
    pub fn with_binding_change(
        mut self,
        start_old: Option<EndpointBinding>,
        start_new: Option<EndpointBinding>,
        end_old: Option<EndpointBinding>,
        end_new: Option<EndpointBinding>,
    ) -> Self {
        self.old_start_binding = Some(start_old);
        self.old_end_binding = Some(end_old);
        self.new_start_binding = Some(start_new);
        self.new_end_binding = Some(end_new);
        self
    }

    /// 记录 `closed` 变更（plan #4 端点拖回起点自动闭合 / 删除顶点导致的退化）。
    /// UI 预览阶段已直改 item，push 后首次 redo 跳过；redo/undo 时随点集一并写回。
    pub fn with_closed(mut self, old_closed: bool, new_closed: bool) -> Self {
        self.old_closed = Some(old_closed);
        self.new_closed = Some(new_closed);
        self
    }
}

impl Command for EditShapePoints {
    fn redo(&mut self, scene: &mut Scene) {
        if let Some(item) = scene.get_item_mut(&self.item_id) {
            item.kind.set_line_points(self.new_points.clone());
            if let ItemKind::Shape {
                start_binding,
                end_binding,
                closed,
                ..
            } = &mut item.kind
            {
                if let Some(b) = self.new_start_binding {
                    *start_binding = b;
                }
                if let Some(b) = self.new_end_binding {
                    *end_binding = b;
                }
                if let Some(c) = self.new_closed {
                    *closed = c;
                }
            }
        }
    }

    fn undo(&mut self, scene: &mut Scene) {
        if let Some(item) = scene.get_item_mut(&self.item_id) {
            item.kind.set_line_points(self.old_points.clone());
            if let ItemKind::Shape {
                start_binding,
                end_binding,
                closed,
                ..
            } = &mut item.kind
            {
                if let Some(b) = self.old_start_binding {
                    *start_binding = b;
                }
                if let Some(b) = self.old_end_binding {
                    *end_binding = b;
                }
                if let Some(c) = self.old_closed {
                    *closed = c;
                }
            }
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

// ─────────────────────────── Set elbow offset ───────────────────────────

/// 批量设置 elbow 直角折线的中间 bar 交叉轴偏移（plan #16 E1）。仅 `curve_type=Elbow`
/// 的线性对象消费该值；其它 shape 写入无害（不读）。单项用 [`Self::new`]，
/// 多选批量用 [`Self::new_batch`]。undo / redo 写回原值，redo 不 clamp——
/// clamp 属几何层职责（[`preferz_core::item::elbow_polyline_offset`]），命令只存
/// 用户意图原值，端点移动后超界由几何兜底。
pub struct SetElbowOffset {
    items: Vec<(ItemId, f32, f32)>,
    preview_already_applied: bool,
}

impl SetElbowOffset {
    pub fn new(item_id: ItemId, old_offset: f32, new_offset: f32) -> Self {
        Self {
            items: vec![(item_id, old_offset, new_offset)],
            preview_already_applied: false,
        }
    }

    /// 批量构造：`(item_id, old, new)` 三元组列表。
    pub fn new_batch(items: Vec<(ItemId, f32, f32)>) -> Self {
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

    fn apply(scene: &mut Scene, items: &[(ItemId, f32, f32)], new: bool) {
        for (id, old, new_offset) in items {
            let value = if new { *new_offset } else { *old };
            if let Some(item) = scene.get_item_mut(id) {
                if let ItemKind::Shape {
                    elbow_mid_offset, ..
                } = &mut item.kind
                {
                    *elbow_mid_offset = value;
                }
            }
        }
    }
}

impl Command for SetElbowOffset {
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

// ─────────────────────────── Set sloppiness ───────────────────────────

/// 批量设置 Shape 手绘风抖动档位命令（plan #3，取代旧 `SetRough`）。
/// 非 `Off` 档位由 RoughStyler 渲染抖动描边。经 `Item::set_sloppiness` 写入，
/// 顺带在 seed 为 0 时生成随机种子。
/// 整批只占一条 undo 记录（D3）；单项用 [`Self::new`]，多选批量用 [`Self::new_batch`]。
pub struct SetSloppiness {
    items: Vec<(ItemId, Sloppiness, Sloppiness)>,
    preview_already_applied: bool,
}

impl SetSloppiness {
    pub fn new(item_id: ItemId, old: Sloppiness, new: Sloppiness) -> Self {
        Self {
            items: vec![(item_id, old, new)],
            preview_already_applied: false,
        }
    }

    /// 批量构造：`(item_id, old, new)` 三元组列表。
    pub fn new_batch(items: Vec<(ItemId, Sloppiness, Sloppiness)>) -> Self {
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

    fn apply(scene: &mut Scene, items: &[(ItemId, Sloppiness, Sloppiness)], new: bool) {
        for (id, old, new_s) in items {
            let value = if new { *new_s } else { *old };
            if let Some(item) = scene.get_item_mut(id) {
                item.set_sloppiness(value);
            }
        }
    }
}

impl Command for SetSloppiness {
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

// ─────────────────────────── Set freedraw style ───────────────────────────

/// 墨迹可编辑样式（plan #10）：颜色 + 基准笔宽。逐点 `pressures`（速度锥形形状）不随
/// 属性编辑变化，故快照只需这两个 Copy 字段。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FreedrawStyle {
    pub color: [u8; 4],
    pub stroke_width: f32,
}

/// 批量设置墨迹样式（plan #10，对齐 Shape 的 `SetStrokeStyle`）。每项自带 old/new，
/// 整批只占一条 undo 记录。points / pressures 不动，仅改 color 与 stroke_width。
pub struct SetFreedrawStyle {
    items: Vec<(ItemId, FreedrawStyle, FreedrawStyle)>,
    preview_already_applied: bool,
}

impl SetFreedrawStyle {
    pub fn new(item_id: ItemId, old: FreedrawStyle, new: FreedrawStyle) -> Self {
        Self {
            items: vec![(item_id, old, new)],
            preview_already_applied: false,
        }
    }

    /// 批量构造：`(item_id, old, new)` 三元组列表。
    pub fn new_batch(items: Vec<(ItemId, FreedrawStyle, FreedrawStyle)>) -> Self {
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

    fn apply(scene: &mut Scene, items: &[(ItemId, FreedrawStyle, FreedrawStyle)], new: bool) {
        for (id, old, new_style) in items {
            let value = if new { *new_style } else { *old };
            if let Some(item) = scene.get_item_mut(id) {
                if let ItemKind::Freedraw {
                    stroke_width,
                    color,
                    ..
                } = &mut item.kind
                {
                    *stroke_width = value.stroke_width;
                    *color = value.color;
                }
            }
        }
    }
}

impl Command for SetFreedrawStyle {
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

/// 填充状态快照：颜色 + 样式 + 是否跟随描边。`color: None` = 无填充（此时 style 无意义，
/// 与 Excalidraw "transparent" 同语义）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FillState {
    pub color: Option<[u8; 4]>,
    pub style: FillStyle,
    /// 填充色是否跟随形状描边色（对应 `ItemKind::Shape::fill_follow_stroke`）。
    pub follow_stroke: bool,
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
                    fill,
                    fill_style,
                    fill_follow_stroke,
                    ..
                } = &mut item.kind
                {
                    *fill = value.color;
                    *fill_style = value.style;
                    *fill_follow_stroke = value.follow_stroke;
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
                item.set_text_style(value);
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

// ─────────────────────────── Set frame size ───────────────────────────

/// 画框几何快照：位置（左上角）、局部基准尺寸、缩放。三者共同决定画框的有效尺寸
/// （= base × scale）与在画布上的落点。画框不旋转不翻转，故只需这三项。
#[derive(Clone, Copy, PartialEq)]
pub struct FrameGeom {
    pub pos: (f32, f32),
    pub base: (f32, f32),
    pub scale: (f32, f32),
}

/// 批量设置画框几何（plan #3 演示比例预设）。
///
/// 侧栏「比例预设」下拉用它：一次套用 = 一条 undo 记录。每项存变更前后的完整几何
/// （pos + base + scale），撤销可精确回到原状态（含套用前 scale≠1 的情形）。
/// 命令本身只负责写回快照，"由预设算出新几何"的逻辑在 UI 层完成。
pub struct SetFrameSize {
    items: Vec<(ItemId, FrameGeom, FrameGeom)>,
}

impl SetFrameSize {
    /// 单画框构造。
    pub fn new(item_id: ItemId, old: FrameGeom, new: FrameGeom) -> Self {
        Self {
            items: vec![(item_id, old, new)],
        }
    }

    /// 批量构造：`(item_id, old, new)` 三元组列表。
    pub fn new_batch(items: Vec<(ItemId, FrameGeom, FrameGeom)>) -> Self {
        Self { items }
    }

    fn apply(scene: &mut Scene, items: &[(ItemId, FrameGeom, FrameGeom)], new: bool) {
        for (id, old, new_geom) in items {
            let g = if new { *new_geom } else { *old };
            if let Some(item) = scene.get_item_mut(id) {
                if let ItemKind::Frame { base_size, .. } = &mut item.kind {
                    *base_size = g.base;
                }
                item.transform.pos = CanvasVector::new(g.pos.0, g.pos.1);
                item.transform.scale = CanvasVector::new(g.scale.0, g.scale.1);
            }
        }
    }
}

impl Command for SetFrameSize {
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
}

/// 批量设置画框「跟随全局比例」状态（设置面板联动 / 侧栏覆盖）。
///
/// - 侧栏套用比例预设 = 解除跟随（覆盖全局），与 [`SetFrameSize`] 打包进
///   [`MultiCommand`] 一步 undo。
/// - 侧栏勾回「跟随全局」= 恢复联动，同时按全局比例重算尺寸。
///   布尔快照成对存 (old, new)，撤销精确还原。
pub struct SetFrameFollowGlobal {
    items: Vec<(ItemId, bool, bool)>,
}

impl SetFrameFollowGlobal {
    /// 批量构造：`(item_id, old, new)` 三元组列表。
    pub fn new_batch(items: Vec<(ItemId, bool, bool)>) -> Self {
        Self { items }
    }

    fn apply(scene: &mut Scene, items: &[(ItemId, bool, bool)], new: bool) {
        for (id, old, new_follow) in items {
            let value = if new { *new_follow } else { *old };
            if let Some(item) = scene.get_item_mut(id) {
                item.set_frame_follow_global_ratio(value);
            }
        }
    }
}

impl Command for SetFrameFollowGlobal {
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
        scene.resolve_bindings(&self.item_ids);
    }

    fn undo(&mut self, scene: &mut Scene) {
        for id in &self.item_ids {
            if let Some(item) = scene.get_item_mut(id) {
                item.transform.pos -= self.delta;
            }
        }
        scene.resolve_bindings(&self.item_ids);
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
        scene.resolve_bindings(&self.item_ids);
    }

    fn undo(&mut self, scene: &mut Scene) {
        for id in &self.item_ids {
            if let Some(item) = scene.get_item_mut(id) {
                item.transform.scale.x /= self.factor;
                item.transform.scale.y /= self.factor;
            }
        }
        scene.resolve_bindings(&self.item_ids);
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
        scene.resolve_bindings(&self.item_ids);
    }

    fn undo(&mut self, scene: &mut Scene) {
        for id in &self.item_ids {
            if let Some(item) = scene.get_item_mut(id) {
                item.transform.rotation -= self.angle;
            }
        }
        scene.resolve_bindings(&self.item_ids);
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
        scene.resolve_bindings(&self.item_ids);
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
        // 清理指向被删 item 的悬空端点绑定（plan #5）
        scene.resolve_bindings(&self.item_ids);
    }

    fn undo(&mut self, scene: &mut Scene) {
        let snaps = self.snapshots.borrow();
        // 用 preserve_z 恢复，保留 item 原本的 z 值
        for item in snaps.iter().flatten() {
            scene.add_item_preserve_z(item.clone());
        }
        // 保留 snapshots 以便下次 redo 复用（不必重新抓）
        // 恢复后重新联动：被删形状回归，原绑定端点重新吸附
        scene.resolve_bindings(&self.item_ids);
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

/// 批量添加 item（Ctrl+拖动复制 / Ctrl+D 原位复制）。
///
/// 与 `AddItem` 的差别只在批量：`MoveItems` 等命令都是批量语义，复制多元素时
/// 逐条 `AddItem` 会让一次 undo 只撤掉一个副本。
///
/// `preview_already_applied` 对应 Ctrl+拖动：副本在按下时已插入场景并跟着指针
/// 移动，push 命令时不该再加一次（skip_first_redo = true），且命令里存的必须是
/// **最终位置**的快照，这样一次 undo 就能撤掉整个「复制 + 移动」。
pub struct AddItems {
    items: Vec<crate::item::Item>,
    preview_already_applied: bool,
}

impl AddItems {
    pub fn new(items: Vec<crate::item::Item>) -> Self {
        Self {
            items,
            preview_already_applied: false,
        }
    }

    /// 副本已在场景中（交互预览已应用）时用此项：push 时跳过首次 redo。
    pub fn with_preview_applied(mut self, applied: bool) -> Self {
        self.preview_already_applied = applied;
        self
    }

    pub fn item_ids(&self) -> Vec<ItemId> {
        self.items.iter().map(|it| it.id).collect()
    }
}

impl Command for AddItems {
    fn redo(&mut self, scene: &mut Scene) {
        // 用 preserve_z：Ctrl 复制的画框已在交互期被压到成员之下（z 最低），
        // 这里保留其 z，避免 redo 后画框被 add_item 重置为最高 z 而重新盖住成员。
        for item in &self.items {
            scene.add_item_preserve_z(item.clone());
        }
    }

    fn undo(&mut self, scene: &mut Scene) {
        for item in &self.items {
            scene.remove_item(&item.id);
        }
    }

    fn skip_first_redo(&self) -> bool {
        self.preview_already_applied
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
    /// redo 前的 next_z，undo 时恢复（避免 z 空洞单调增长）。
    old_next_z: i32,
}

impl ReorderItems {
    pub fn new(item_ids: Vec<ItemId>, to_front: bool) -> Self {
        Self {
            item_ids,
            to_front,
            old_z: Vec::new(),
            new_z: Vec::new(),
            old_next_z: 0,
        }
    }
}

impl Command for ReorderItems {
    fn redo(&mut self, scene: &mut Scene) {
        // 第一次 redo：记录 old_z，计算 new_z
        if self.old_z.is_empty() {
            self.old_next_z = scene.next_z;
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
        scene.next_z = self.old_next_z;
    }
}

/// 上移/下移一层：每个选中 item 与 z 序中紧邻的**非选中** item 交换 z。
///
/// 连续选中块只有块边界与外部交换（需多次调用逐层上移整体块）；
/// 选区内相对顺序在单次操作中保持不变。undo 恢复所有被改 item（含交换邻居）。
pub struct ReorderRelative {
    item_ids: Vec<ItemId>,
    forward: bool,
    /// 所有被改 item（选中 + 交换邻居）的旧 z。
    old_z: Vec<(ItemId, i32)>,
    /// 所有被改 item 的新 z。
    new_z: Vec<(ItemId, i32)>,
}

impl ReorderRelative {
    pub fn new(item_ids: Vec<ItemId>, forward: bool) -> Self {
        Self {
            item_ids,
            forward,
            old_z: Vec::new(),
            new_z: Vec::new(),
        }
    }
}

impl Command for ReorderRelative {
    fn redo(&mut self, scene: &mut Scene) {
        if self.old_z.is_empty() {
            let mut z_sorted: Vec<(ItemId, i32)> =
                scene.items.iter().map(|i| (i.id, i.z)).collect();
            z_sorted.sort_by_key(|(_, z)| *z);
            let selected: std::collections::HashSet<ItemId> =
                self.item_ids.iter().copied().collect();
            let pos: std::collections::HashMap<ItemId, usize> = z_sorted
                .iter()
                .enumerate()
                .map(|(i, (id, _))| (*id, i))
                .collect();
            for id in &self.item_ids {
                let Some(&p) = pos.get(id) else {
                    continue;
                };
                let neighbor = if self.forward {
                    (p + 1 < z_sorted.len()).then(|| p + 1)
                } else {
                    (p > 0).then(|| p - 1)
                };
                if let Some(np) = neighbor {
                    let (nid, nz) = z_sorted[np];
                    if !selected.contains(&nid) {
                        let my_z = z_sorted[p].1;
                        self.new_z.push((*id, nz));
                        self.new_z.push((nid, my_z));
                    }
                }
            }
            for (id, _) in &self.new_z {
                if let Some(item) = scene.get_item(id) {
                    self.old_z.push((*id, item.z));
                }
            }
        }
        for (id, z) in &self.new_z {
            if let Some(item) = scene.get_item_mut(id) {
                item.z = *z;
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
        let ids: Vec<ItemId> = self.moves.iter().map(|(id, _, _)| *id).collect();
        scene.resolve_bindings(&ids);
    }

    fn undo(&mut self, scene: &mut Scene) {
        for (id, old, _new) in &self.moves {
            if let Some(item) = scene.get_item_mut(id) {
                item.transform.pos = *old;
            }
        }
        let ids: Vec<ItemId> = self.moves.iter().map(|(id, _, _)| *id).collect();
        scene.resolve_bindings(&ids);
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

// ─────────────────────────── Group / Ungroup（plan #13） ───────────────────────────

/// 编组/解组命令。快照每个受影响 item 的 `(id, old_group_id, new_group_id)`，
/// redo 应用新值、undo 还原旧值——天然支持"换组"（G1：编组前已属别组的项
/// 一步离开旧组进入新组，undo 一步回原组）。
///
/// 通过 [`SetGroup::group`] / [`SetGroup::ungroup`] 构造；构造时即快照当前值，
/// 但**不应用**——首次 redo 由 undo 栈驱动（非预览命令）。
pub struct SetGroup {
    entries: Vec<(ItemId, Option<uuid::Uuid>, Option<uuid::Uuid>)>,
}

impl SetGroup {
    /// 编组 `ids`（≥2 项才有效）。返回 `None` 表示无可执行命令（不足 2 项）。
    pub fn group(scene: &Scene, ids: &[ItemId]) -> Option<Self> {
        if ids.len() < 2 {
            return None;
        }
        let gid = uuid::Uuid::new_v4();
        let set: std::collections::HashSet<ItemId> = ids.iter().copied().collect();
        let entries = scene
            .items
            .iter()
            .filter(|it| set.contains(&it.id))
            .map(|it| (it.id, it.group_id, Some(gid)))
            .collect();
        Some(Self { entries })
    }

    /// 解组 `ids`（清空其 group_id）。
    pub fn ungroup(scene: &Scene, ids: &[ItemId]) -> Option<Self> {
        let set: std::collections::HashSet<ItemId> = ids.iter().copied().collect();
        let entries: Vec<_> = scene
            .items
            .iter()
            .filter(|it| set.contains(&it.id) && it.group_id.is_some())
            .map(|it| (it.id, it.group_id, None))
            .collect();
        if entries.is_empty() {
            return None;
        }
        Some(Self { entries })
    }

    fn apply(&mut self, scene: &mut Scene, take_new: bool) {
        for (id, old, new) in &self.entries {
            if let Some(item) = scene.get_item_mut(id) {
                item.group_id = if take_new { *new } else { *old };
            }
        }
    }
}

impl Command for SetGroup {
    fn redo(&mut self, scene: &mut Scene) {
        self.apply(scene, true);
    }

    fn undo(&mut self, scene: &mut Scene) {
        self.apply(scene, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;
    use crate::shape::{
        DashStyle, FontFamily, ShapeType, Sloppiness, StrokeStyle, TextAlignH, TextAlignV,
    };

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

    fn frame_in(scene: &mut Scene, w: f32, h: f32) -> ItemId {
        let item = Item::new_frame(1, (w, h), 0.0, 0.0, None);
        let id = item.id;
        scene.add_item(item);
        id
    }

    #[test]
    fn set_frame_follow_global_redo_undo_restores_state() {
        let mut scene = Scene::new();
        let f = frame_in(&mut scene, 160.0, 90.0);
        // new_frame 构造的画框默认跟随全局
        assert!(scene.get_item(&f).unwrap().frame_follows_global_ratio());

        let mut cmd = SetFrameFollowGlobal::new_batch(vec![(f, true, false)]);
        cmd.redo(&mut scene);
        assert!(!scene.get_item(&f).unwrap().frame_follows_global_ratio());
        cmd.undo(&mut scene);
        assert!(scene.get_item(&f).unwrap().frame_follows_global_ratio());

        // 非画框 item 不受影响（无副作用、不 panic）
        let s = shape_in(&mut scene);
        let mut cmd2 = SetFrameFollowGlobal::new_batch(vec![(s, false, true)]);
        cmd2.redo(&mut scene);
        assert!(!scene.get_item(&s).unwrap().frame_follows_global_ratio());
    }

    #[test]
    fn set_frame_size_batch_roundtrip_keeps_geometry() {
        use crate::item::frame_geom_for_ratio;
        let mut scene = Scene::new();
        let f = frame_in(&mut scene, 200.0, 100.0);
        let g = FrameGeom {
            pos: (0.0, 0.0),
            base: (200.0, 100.0),
            scale: (1.0, 1.0),
        };
        let new = frame_geom_for_ratio(g, (16.0, 9.0));
        let mut cmd = SetFrameSize::new_batch(vec![(f, g, new)]);
        cmd.redo(&mut scene);
        let it = scene.get_item(&f).unwrap();
        assert!((it.bounding_rect().height() - new.base.1).abs() < 1e-3);
        cmd.undo(&mut scene);
        let it = scene.get_item(&f).unwrap();
        assert!((it.bounding_rect().height() - 100.0).abs() < 1e-3);
    }

    #[test]
    fn multi_command_redo_all_undo_reverse() {
        let mut scene = Scene::new();
        let id = shape_in(&mut scene);

        // 两条子命令打包：sloppiness + dash，redo 全生效，undo 逆序全还原
        let mut cmd = MultiCommand::new(vec![
            Box::new(SetSloppiness::new(id, Sloppiness::Off, Sloppiness::Artist)),
            Box::new(SetStrokeStyle::new(
                id,
                StrokeStyle::default(),
                StrokeStyle {
                    dash: DashStyle::Dashed,
                    ..StrokeStyle::default()
                },
            )),
        ]);
        cmd.redo(&mut scene);
        let item = scene.get_item(&id).unwrap();
        assert_eq!(item.sloppiness(), Sloppiness::Artist);
        let dash = match &scene.get_item(&id).unwrap().kind {
            ItemKind::Shape { stroke, .. } => stroke.dash,
            _ => panic!("not a shape"),
        };
        assert_eq!(dash, DashStyle::Dashed);

        cmd.undo(&mut scene);
        let item = scene.get_item(&id).unwrap();
        assert_eq!(item.sloppiness(), Sloppiness::Off);
        let dash = match &scene.get_item(&id).unwrap().kind {
            ItemKind::Shape { stroke, .. } => stroke.dash,
            _ => panic!("not a shape"),
        };
        assert_eq!(dash, DashStyle::Solid);
    }

    #[test]
    fn set_sloppiness_redo_undo_restores_previous_state() {
        let mut scene = Scene::new();
        let id = shape_in(&mut scene);
        assert_eq!(scene.get_item(&id).unwrap().sloppiness(), Sloppiness::Off);

        let mut cmd = SetSloppiness::new(id, Sloppiness::Off, Sloppiness::Artist);
        cmd.redo(&mut scene);
        assert_eq!(
            scene.get_item(&id).unwrap().sloppiness(),
            Sloppiness::Artist
        );
        cmd.undo(&mut scene);
        assert_eq!(scene.get_item(&id).unwrap().sloppiness(), Sloppiness::Off);
    }

    #[test]
    fn set_sloppiness_generates_seed_and_keeps_it_across_undo() {
        let mut scene = Scene::new();
        let id = shape_in(&mut scene);

        let mut cmd = SetSloppiness::new(id, Sloppiness::Off, Sloppiness::Artist);
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
    fn set_sloppiness_on_non_shape_is_noop() {
        let mut scene = Scene::new();
        let txt = Item::new_text("x".to_string(), 0.0, 0.0, 16.0, [255; 4]);
        let id = txt.id;
        scene.add_item(txt);
        let mut cmd = SetSloppiness::new(id, Sloppiness::Off, Sloppiness::Artist);
        cmd.redo(&mut scene);
        assert_eq!(scene.get_item(&id).unwrap().sloppiness(), Sloppiness::Off);
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
    fn set_sloppiness_batch_sets_every_item() {
        let mut scene = Scene::new();
        let (a, b) = two_shapes(&mut scene);
        let mut cmd = SetSloppiness::new_batch(vec![
            (a, Sloppiness::Off, Sloppiness::Cartoonist),
            (b, Sloppiness::Off, Sloppiness::Architect),
        ]);
        cmd.redo(&mut scene);
        assert_eq!(
            scene.get_item(&a).unwrap().sloppiness(),
            Sloppiness::Cartoonist
        );
        assert_eq!(
            scene.get_item(&b).unwrap().sloppiness(),
            Sloppiness::Architect
        );
        cmd.undo(&mut scene);
        assert_eq!(scene.get_item(&a).unwrap().sloppiness(), Sloppiness::Off);
        assert_eq!(scene.get_item(&b).unwrap().sloppiness(), Sloppiness::Off);
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

    fn elbow_offset_of(scene: &Scene, id: ItemId) -> f32 {
        match &scene.get_item(&id).unwrap().kind {
            ItemKind::Shape {
                elbow_mid_offset, ..
            } => *elbow_mid_offset,
            _ => panic!("非 Shape"),
        }
    }

    #[test]
    fn set_elbow_offset_batch_applies_undoes_and_keeps_each_old_value() {
        use crate::shape::{ArrowHeadStyle, StrokeStyle};
        let mut scene = Scene::new();
        let mk = |scene: &mut Scene, x: f32| {
            let it = Item::new_polyline(
                vec![(0.0, 0.0), (100.0, 40.0)],
                (100.0, 40.0),
                None,
                Some(ArrowHeadStyle::Arrow),
                false,
                x,
                0.0,
                StrokeStyle::default(),
            );
            let id = it.id;
            scene.add_item(it);
            id
        };
        let (a, b) = (mk(&mut scene, 0.0), mk(&mut scene, 200.0));
        // 两个 item 旧值不同（0 与 -12），undo 必须各回各的
        let mut cmd = SetElbowOffset::new_batch(vec![(a, 0.0, 25.0), (b, -12.0, 40.0)]);
        cmd.redo(&mut scene);
        assert!((elbow_offset_of(&scene, a) - 25.0).abs() < 1e-6);
        assert!((elbow_offset_of(&scene, b) - 40.0).abs() < 1e-6);
        cmd.undo(&mut scene);
        assert_eq!(elbow_offset_of(&scene, a), 0.0);
        assert!((elbow_offset_of(&scene, b) + 12.0).abs() < 1e-6);
        // skip_first_redo 语义：预览已应用 → push_cmd 首次 redo 跳过（标志由 UndoStack 读）。
        let preview_cmd = SetElbowOffset::new(a, 0.0, 7.0).with_preview_applied(true);
        assert!(preview_cmd.skip_first_redo());
        assert!(!SetElbowOffset::new(a, 0.0, 7.0).skip_first_redo());
    }

    #[test]
    fn edit_shape_points_with_closed_roundtrips_points_and_flag() {
        // plan #4：端点拖回起点自动闭合 = 点集 + closed 同占一条命令，
        // redo/undo 必须两者一起生效/还原。
        let mut scene = Scene::new();
        let item = Item::new_polyline(
            vec![(0.0, 0.0), (50.0, 0.0), (50.0, 50.0)],
            (50.0, 50.0),
            None,
            None,
            false,
            0.0,
            0.0,
            StrokeStyle::default(),
        );
        let id = item.id;
        scene.add_item(item);
        let mut cmd = EditShapePoints::new(
            id,
            vec![(0.0, 0.0), (50.0, 0.0), (50.0, 50.0)],
            vec![(0.0, 0.0), (50.0, 0.0), (2.0, 3.0)],
        )
        .with_closed(false, true);
        cmd.redo(&mut scene);
        assert!(closed_of(&scene, id), "redo 应同时写 closed");
        match &scene.get_item(&id).unwrap().kind {
            ItemKind::Shape { points, .. } => assert_eq!(points[2], (2.0, 3.0)),
            _ => panic!("expected Shape kind"),
        }
        cmd.undo(&mut scene);
        assert!(!closed_of(&scene, id), "undo 应还原 closed");
        match &scene.get_item(&id).unwrap().kind {
            ItemKind::Shape { points, .. } => assert_eq!(points[2], (50.0, 50.0)),
            _ => panic!("expected Shape kind"),
        }
    }

    #[test]
    fn set_arrow_heads_batch_sets_both_ends_per_item() {
        let mut scene = Scene::new();
        let (a, b) = two_shapes(&mut scene);
        let old = ArrowHeads::default();
        let new = ArrowHeads {
            start: Some(ArrowHeadStyle::Triangle),
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
    fn set_freedraw_style_batch_applies_and_undoes() {
        let mut scene = Scene::new();
        let mk = |scene: &mut Scene| {
            let it =
                Item::new_freedraw(&[(0.0, 0.0), (10.0, 0.0)], &[1.0, 0.5], 3.0, [0, 0, 0, 255]);
            let id = it.id;
            scene.add_item(it);
            id
        };
        let a = mk(&mut scene);
        let b = mk(&mut scene);
        let old = FreedrawStyle {
            color: [0, 0, 0, 255],
            stroke_width: 3.0,
        };
        let new = FreedrawStyle {
            color: [200, 10, 10, 255],
            stroke_width: 8.0,
        };
        let style_of = |scene: &Scene, id: &ItemId| match &scene.get_item(id).unwrap().kind {
            ItemKind::Freedraw {
                color,
                stroke_width,
                pressures,
                ..
            } => (*color, *stroke_width, pressures.clone()),
            _ => panic!("expected Freedraw"),
        };
        // 记录 pressures 以确证样式命令不触碰形状。
        let pressures_before = style_of(&scene, &a).2;

        let mut cmd = SetFreedrawStyle::new_batch(vec![(a, old, new), (b, old, new)]);
        cmd.redo(&mut scene);
        let (ca, wa, pa) = style_of(&scene, &a);
        assert_eq!((ca, wa), (new.color, new.stroke_width));
        assert_eq!(style_of(&scene, &b).1, new.stroke_width);
        cmd.undo(&mut scene);
        assert_eq!(style_of(&scene, &a).0, old.color);
        assert_eq!(style_of(&scene, &a).1, old.stroke_width);
        assert_eq!(style_of(&scene, &b).1, old.stroke_width);
        // 逐点相对笔宽（形状）始终不变。
        assert_eq!(pa, pressures_before);
        assert_eq!(style_of(&scene, &a).2, pressures_before);
    }

    #[test]
    fn set_shape_fill_batch_none_clears_fill_on_undo_restores() {
        let mut scene = Scene::new();
        let (a, b) = two_shapes(&mut scene);
        let solid = |c: Option<[u8; 4]>| FillState {
            color: c,
            style: FillStyle::Solid,
            follow_stroke: false,
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
            follow_stroke: false,
        };
        let new = FillState {
            color: Some([200, 30, 30, 255]),
            style: FillStyle::Hachure,
            follow_stroke: false,
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
            align_h: TextAlignH::Center,
            align_v: TextAlignV::Middle,
            font_family: FontFamily::Normal,
            follow_stroke: true,
        };
        let new = TextStyle {
            font_size: 32.0,
            color: [10, 20, 30, 255],
            background: Some([0, 0, 0, 128]),
            align_h: TextAlignH::Left,
            align_v: TextAlignV::Top,
            font_family: FontFamily::Handwriting,
            follow_stroke: false,
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
            align_h: TextAlignH::Center,
            align_v: TextAlignV::Middle,
            font_family: FontFamily::Normal,
            follow_stroke: true,
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

    #[test]
    fn set_frame_size_roundtrip_restores_full_geom() {
        use crate::item::ItemKind;
        let mut scene = Scene::new();
        // 一个 scale≠1 的画框（模拟已被手柄缩放）：base 200×100，scale 1.5×1.5。
        let mut frame = Item::new_frame(1, (200.0, 100.0), 50.0, 40.0, None);
        frame.transform.scale = CanvasVector::new(1.5, 1.5);
        let fid = frame.id;
        scene.add_item(frame);

        let old = FrameGeom {
            pos: (50.0, 40.0),
            base: (200.0, 100.0),
            scale: (1.5, 1.5),
        };
        // 套用预设后归一：base 直接等于新尺寸、scale 复位 1、pos 重新锚定中心。
        let new = FrameGeom {
            pos: (25.0, 65.0),
            base: (300.0, 150.0),
            scale: (1.0, 1.0),
        };
        let mut cmd = SetFrameSize::new(fid, old, new);

        cmd.redo(&mut scene);
        let it = scene.get_item(&fid).unwrap();
        match &it.kind {
            ItemKind::Frame { base_size, .. } => assert_eq!(*base_size, (300.0, 150.0)),
            _ => panic!("expected Frame"),
        }
        assert_eq!(it.transform.pos, CanvasVector::new(25.0, 65.0));
        assert_eq!(it.transform.scale, CanvasVector::new(1.0, 1.0));

        cmd.undo(&mut scene);
        let it = scene.get_item(&fid).unwrap();
        match &it.kind {
            ItemKind::Frame { base_size, .. } => assert_eq!(*base_size, (200.0, 100.0)),
            _ => panic!("expected Frame"),
        }
        assert_eq!(it.transform.pos, CanvasVector::new(50.0, 40.0));
        assert_eq!(it.transform.scale, CanvasVector::new(1.5, 1.5));
    }

    // ─── z-order（ReorderRelative / ReorderItems）───

    #[test]
    fn reorder_relative_forward_swaps_with_above_neighbor() {
        let mut scene = Scene::new();
        let a = shape_in(&mut scene);
        let b = shape_in(&mut scene);
        let c = shape_in(&mut scene);
        assert_eq!(
            (
                scene.get_item(&a).unwrap().z,
                scene.get_item(&b).unwrap().z,
                scene.get_item(&c).unwrap().z
            ),
            (0, 1, 2)
        );
        let mut cmd = ReorderRelative::new(vec![b], true);
        cmd.redo(&mut scene);
        assert_eq!(scene.get_item(&b).unwrap().z, 2);
        assert_eq!(scene.get_item(&c).unwrap().z, 1);
        assert_eq!(scene.get_item(&a).unwrap().z, 0);
        cmd.undo(&mut scene);
        assert_eq!(scene.get_item(&b).unwrap().z, 1);
        assert_eq!(scene.get_item(&c).unwrap().z, 2);
    }

    #[test]
    fn reorder_relative_backward_swaps_with_below_neighbor() {
        let mut scene = Scene::new();
        let a = shape_in(&mut scene);
        let b = shape_in(&mut scene);
        let c = shape_in(&mut scene);
        let mut cmd = ReorderRelative::new(vec![b], false);
        cmd.redo(&mut scene);
        assert_eq!(scene.get_item(&b).unwrap().z, 0);
        assert_eq!(scene.get_item(&a).unwrap().z, 1);
        assert_eq!(scene.get_item(&c).unwrap().z, 2);
        cmd.undo(&mut scene);
        assert_eq!(scene.get_item(&b).unwrap().z, 1);
        assert_eq!(scene.get_item(&a).unwrap().z, 0);
    }

    #[test]
    fn reorder_relative_skips_selected_neighbor() {
        let mut scene = Scene::new();
        let a = shape_in(&mut scene);
        let b = shape_in(&mut scene);
        let c = shape_in(&mut scene);
        let d = shape_in(&mut scene);
        let mut cmd = ReorderRelative::new(vec![b, c], true);
        cmd.redo(&mut scene);
        assert_eq!(scene.get_item(&b).unwrap().z, 1);
        assert_eq!(scene.get_item(&c).unwrap().z, 3);
        assert_eq!(scene.get_item(&d).unwrap().z, 2);
        assert_eq!(scene.get_item(&a).unwrap().z, 0);
        cmd.undo(&mut scene);
        assert_eq!(
            (
                scene.get_item(&a).unwrap().z,
                scene.get_item(&b).unwrap().z,
                scene.get_item(&c).unwrap().z,
                scene.get_item(&d).unwrap().z
            ),
            (0, 1, 2, 3)
        );
    }

    #[test]
    fn reorder_relative_at_boundary_is_no_op() {
        let mut scene = Scene::new();
        let a = shape_in(&mut scene);
        let b = shape_in(&mut scene);
        let mut cmd = ReorderRelative::new(vec![b], true);
        cmd.redo(&mut scene);
        assert_eq!(scene.get_item(&b).unwrap().z, 1);
        assert_eq!(scene.get_item(&a).unwrap().z, 0);
        let mut cmd2 = ReorderRelative::new(vec![a], false);
        cmd2.redo(&mut scene);
        assert_eq!(scene.get_item(&a).unwrap().z, 0);
    }

    #[test]
    fn reorder_relative_undo_restores_neighbor_z() {
        let mut scene = Scene::new();
        let a = shape_in(&mut scene);
        let b = shape_in(&mut scene);
        let c = shape_in(&mut scene);
        let mut cmd = ReorderRelative::new(vec![b], true);
        cmd.redo(&mut scene);
        assert_eq!(
            (scene.get_item(&b).unwrap().z, scene.get_item(&c).unwrap().z),
            (2, 1)
        );
        cmd.undo(&mut scene);
        assert_eq!(
            (
                scene.get_item(&a).unwrap().z,
                scene.get_item(&b).unwrap().z,
                scene.get_item(&c).unwrap().z
            ),
            (0, 1, 2)
        );
    }

    #[test]
    fn reorder_items_undo_restores_next_z() {
        let mut scene = Scene::new();
        let a = shape_in(&mut scene);
        let _b = shape_in(&mut scene);
        let next_z_before = scene.next_z;
        let mut cmd = ReorderItems::new(vec![a], true);
        cmd.redo(&mut scene);
        assert!(scene.next_z > next_z_before);
        cmd.undo(&mut scene);
        assert_eq!(scene.next_z, next_z_before);
    }

    #[test]
    fn reorder_items_to_back_undo_restores_next_z() {
        let mut scene = Scene::new();
        let a = shape_in(&mut scene);
        let _b = shape_in(&mut scene);
        let next_z_before = scene.next_z;
        let mut cmd = ReorderItems::new(vec![a], false);
        cmd.redo(&mut scene);
        cmd.undo(&mut scene);
        assert_eq!(scene.next_z, next_z_before);
    }
}
