use crate::item::{CropRect, ItemId, ItemKind};
use crate::scene::Scene;
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
