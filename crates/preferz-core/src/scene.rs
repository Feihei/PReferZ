use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::item::{Item, ItemId, ItemKind};
use crate::shape::ShapeType;
use crate::snap;
use crate::spaces::{CanvasPoint, CanvasRect, CanvasVector};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub items: Vec<Item>,
    pub selection: HashSet<ItemId>,
    pub next_z: i32,
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

/// 画框重编号计划（Phase D）。把 `frame_id` 编号设为 `new_number`，
/// 冲突规则：其它编号 >= `new_number` 的画框全部 +1（插入式顺移）。
pub struct RenumberPlan {
    pub frame_id: ItemId,
    pub old_number: u32,
    pub new_number: u32,
    /// 被顺移的画框 (id, 旧编号)。redo 时置 new，undo 时还原。
    pub affected: Vec<(ItemId, u32)>,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            selection: HashSet::new(),
            next_z: 0,
        }
    }

    /// 添加 item，自动赋 z = next_z 并自增，保证渲染按添加顺序堆叠。
    ///
    /// 注意：若传入 item 已带特定 z（例如从快照恢复），仍会被覆盖为 next_z。
    /// 快照恢复场景请用 [`add_item_preserve_z`]。
    ///
    /// [`add_item_preserve_z`]: Scene::add_item_preserve_z
    pub fn add_item(&mut self, mut item: Item) {
        item.z = self.next_z;
        self.next_z += 1;
        self.items.push(item);
    }

    /// 添加 item 但保留其原有 z（用于 DeleteItems::undo 恢复快照）。
    /// 若 item.z >= next_z，则推进 next_z。
    pub fn add_item_preserve_z(&mut self, item: Item) {
        if item.z >= self.next_z {
            self.next_z = item.z + 1;
        }
        self.items.push(item);
    }

    pub fn remove_item(&mut self, id: &ItemId) {
        self.items.retain(|item| item.id != *id);
        self.selection.remove(id);
    }

    pub fn get_item(&self, id: &ItemId) -> Option<&Item> {
        self.items.iter().find(|item| item.id == *id)
    }

    pub fn get_item_mut(&mut self, id: &ItemId) -> Option<&mut Item> {
        self.items.iter_mut().find(|item| item.id == *id)
    }

    /// 查询 `query` 在画布空间下离哪个 item 轮廓最近（用于端点吸附）。
    ///
    /// 跳过 `exclude_id`（被拖动的线自身）与绑定文本（`is_bound_text`，其位置由容器
    /// 决定，不参与吸附）。返回 `(item_id, 最近轮廓点, 距离)`，仅当距离 < `threshold`
    /// 时才返回 `Some`。
    pub fn find_snap_target(
        &self,
        exclude_id: &ItemId,
        query: CanvasPoint,
        threshold: f32,
    ) -> Option<(ItemId, CanvasPoint, f32)> {
        let mut best: Option<(ItemId, CanvasPoint, f32)> = None;
        for item in &self.items {
            if &item.id == exclude_id || self.is_bound_text(item) {
                continue;
            }
            let np = snap::nearest_outline_point(item, query);
            let d = query.distance_to(np);
            if d <= threshold && best.as_ref().is_none_or(|(_, _, bd)| d < *bd) {
                best = Some((item.id, np, d));
            }
        }
        best
    }

    /// 联动重算绑定端点：对每条 Polyline，若其起点/终点绑定到 `moved_ids` 中的
    /// 某个 shape，则把该端点重定位到该 shape 轮廓上「离另一端最近」的点。
    ///
    /// 设计要点（plan #5）：
    /// - 吸附点不存绝对坐标，每次都按另一端方向动态重算，天然兼容形状移动/缩放/旋转。
    /// - 在形状移动命令（TransformItem / MoveItems / ScaleItems / RotateItems /
    ///   FlipItems / ArrangeItems）的 redo/undo 中调用，使联动随 undo/redo 一致。
    /// - 绑定目标已不存在（被删除）时，自动清除该端点的绑定（避免悬空引用）。
    pub fn resolve_bindings(&mut self, moved_ids: &[ItemId]) {
        if moved_ids.is_empty() {
            return;
        }
        let moved: HashSet<&ItemId> = moved_ids.iter().collect();

        // 第一遍：只读地计算需要更新的端点（避免同时借用 self.items 读写）。
        let mut updates: Vec<(ItemId, usize, (f32, f32))> = Vec::new();
        let mut dead: Vec<(ItemId, usize)> = Vec::new();
        for line in &self.items {
            let ItemKind::Shape {
                shape_type: ShapeType::Polyline,
                points,
                start_binding,
                end_binding,
                ..
            } = &line.kind
            else {
                continue;
            };
            let last = points.len().saturating_sub(1);
            if last == 0 {
                continue; // 退化（单点），无端点可言
            }
            // 起点（points[0]）
            if let Some(bid) = start_binding {
                match self.get_item(bid) {
                    Some(shape) if moved.contains(bid) => {
                        let other = points.get(last).copied().unwrap_or((0.0, 0.0));
                        let query = line.local_to_canvas().transform_point(euclid::Point2D::<
                            f32,
                            crate::item::ItemLocalSpace,
                        >::new(
                            other.0, other.1
                        ));
                        let np = snap::nearest_outline_point(shape, query);
                        if let Some(local) = line.canvas_to_local_point(np) {
                            updates.push((line.id, 0, local));
                        }
                    }
                    // 绑定目标已不存在（被删除）→ 清除悬空绑定
                    None => dead.push((line.id, 0)),
                    _ => {}
                }
            }
            // 终点（points[last]）
            if let Some(bid) = end_binding {
                match self.get_item(bid) {
                    Some(shape) if moved.contains(bid) => {
                        let other = points.first().copied().unwrap_or((0.0, 0.0));
                        let query = line.local_to_canvas().transform_point(euclid::Point2D::<
                            f32,
                            crate::item::ItemLocalSpace,
                        >::new(
                            other.0, other.1
                        ));
                        let np = snap::nearest_outline_point(shape, query);
                        if let Some(local) = line.canvas_to_local_point(np) {
                            updates.push((line.id, last, local));
                        }
                    }
                    None => dead.push((line.id, last)),
                    _ => {}
                }
            }
        }

        // 第二遍：应用端点位移并同步 base_size。
        for (id, idx, pos) in updates {
            if let Some(line) = self.get_item_mut(&id) {
                if let ItemKind::Shape { points, .. } = &mut line.kind {
                    if let Some(p) = points.get_mut(idx) {
                        *p = pos;
                    }
                }
                // 同步 points 包围盒（base_size），与 EditShapePoints 行为一致
                if let ItemKind::Shape { points, .. } = &line.kind {
                    line.kind.set_line_points(points.clone());
                }
            }
        }
        // 清除悬空绑定
        for (id, idx) in dead {
            if let Some(line) = self.get_item_mut(&id) {
                if let ItemKind::Shape {
                    start_binding,
                    end_binding,
                    ..
                } = &mut line.kind
                {
                    if idx == 0 {
                        *start_binding = None;
                    } else {
                        *end_binding = None;
                    }
                }
            }
        }
    }

    /// 返回所有绑定到指定容器（封闭形状）的 Text item id（Phase C）。
    /// 用于移动/缩放/删除时随容器联动。
    pub fn texts_bound_to(&self, container_id: ItemId) -> Vec<ItemId> {
        self.items
            .iter()
            .filter(|it| {
                matches!(
                    &it.kind,
                    crate::item::ItemKind::Text {
                        container_id: Some(c),
                        ..
                    } if *c == container_id
                )
            })
            .map(|it| it.id)
            .collect()
    }

    /// 克隆一批 item 用于复制（Ctrl+拖动 / Ctrl+D），返回**尚未加入场景**的副本。
    ///
    /// - 每个副本分配新 `Uuid`，`transform.pos` 加上 `offset`；
    /// - 副本的 `z` 沿用原件（真正的 z 由 `add_item` 在入场景时重排）；
    /// - `Text.container_id` 会按 old→new 映射改写：容器也在本次复制集内则指向
    ///   新容器（复制封闭图形时其绑定文字跟着走），否则置 `None` 退化为自由文本
    ///   （不能让副本文字仍绑在没被复制的原件容器上）。
    ///
    /// 传入顺序即返回顺序，便于调用方建立一一对应的 id 映射。
    pub fn duplicate_items(&self, ids: &[ItemId], offset: CanvasVector) -> Vec<Item> {
        use std::collections::HashMap;

        let id_map: HashMap<ItemId, ItemId> =
            ids.iter().map(|id| (*id, uuid::Uuid::new_v4())).collect();

        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let Some(src) = self.get_item(id) else {
                continue;
            };
            let mut dup = src.clone();
            dup.id = id_map[id];
            dup.transform.pos += offset;
            if let crate::item::ItemKind::Text { container_id, .. } = &mut dup.kind {
                if let Some(old_cid) = *container_id {
                    *container_id = id_map.get(&old_cid).copied();
                }
            }
            out.push(dup);
        }
        out
    }

    /// 该 item 是否为「绑定在存活容器上的文本」（Phase C 语义的逆查询）。
    ///
    /// 绑定文本不可被独立选中/拖动：渲染位置由容器矩形实时决定，其自身
    /// transform 只是创建时的快照，直接操作只会造成选中框与文字错位。
    /// 交互层应把命中它的点击重定向到容器（Excalidraw 同款语义）。
    pub fn is_bound_text(&self, item: &Item) -> bool {
        matches!(
            &item.kind,
            crate::item::ItemKind::Text {
                container_id: Some(c),
                ..
            } if self.get_item(c).is_some()
        )
    }

    /// 返回画框（编号升序）。
    pub fn frames_by_number(&self) -> Vec<ItemId> {
        let mut frames: Vec<(u32, ItemId)> = self
            .items
            .iter()
            .filter_map(|it| it.frame_number().map(|n| (n, it.id)))
            .collect();
        frames.sort_by_key(|(n, _)| *n);
        frames.into_iter().map(|(_, id)| id).collect()
    }

    /// 画框的成员：完全落在画框（canvas AABB）内的其它 item。
    /// 画框自身排除；内含画框排除。绑定文本已含在其容器内，天然跟随。
    pub fn frame_members(&self, frame_id: ItemId) -> Vec<ItemId> {
        let Some(frame) = self.get_item(&frame_id) else {
            return Vec::new();
        };
        if !frame.is_frame() {
            return Vec::new();
        }
        let fr = frame.bounding_rect();
        self.items
            .iter()
            .filter(|it| {
                it.id != frame_id && !it.is_frame() && fr.contains_rect(&it.bounding_rect())
            })
            .map(|it| it.id)
            .collect()
    }

    /// 下一个可用的画框编号：max(n)+1，无画框时为 1。
    pub fn next_frame_number(&self) -> u32 {
        self.items
            .iter()
            .filter_map(|it| it.frame_number())
            .max()
            .map(|n| n + 1)
            .unwrap_or(1)
    }

    /// 规划一次画框重编号。`new_number` 会被收敛到 >=1。
    pub fn plan_frame_renumber(&self, frame_id: ItemId, new_number: u32) -> RenumberPlan {
        let new_number = new_number.max(1);
        let old_number = self
            .get_item(&frame_id)
            .and_then(|it| it.frame_number())
            .unwrap_or(1);
        let affected: Vec<(ItemId, u32)> = self
            .items
            .iter()
            .filter(|it| it.id != frame_id)
            .filter_map(|it| it.frame_number().map(|n| (it.id, n)))
            .filter(|(_, n)| *n >= new_number)
            .collect();
        RenumberPlan {
            frame_id,
            old_number,
            new_number,
            affected,
        }
    }

    /// 应用重编号计划（redo）。
    pub fn apply_renumber(&mut self, plan: &RenumberPlan) {
        for (id, old) in &plan.affected {
            if let Some(it) = self.get_item_mut(id) {
                it.set_frame_number(old + 1);
            }
        }
        if let Some(it) = self.get_item_mut(&plan.frame_id) {
            it.set_frame_number(plan.new_number);
        }
    }

    /// 还原重编号计划（undo）。
    pub fn undo_renumber(&mut self, plan: &RenumberPlan) {
        for (id, old) in &plan.affected {
            if let Some(it) = self.get_item_mut(id) {
                it.set_frame_number(*old);
            }
        }
        if let Some(it) = self.get_item_mut(&plan.frame_id) {
            it.set_frame_number(plan.old_number);
        }
    }

    pub fn selection_contains(&self, id: &ItemId) -> bool {
        self.selection.contains(id)
    }

    pub fn select(&mut self, id: ItemId) {
        self.selection.insert(id);
    }

    pub fn deselect_all(&mut self) {
        self.selection.clear();
    }

    pub fn toggle_selection(&mut self, id: ItemId) {
        if self.selection.contains(&id) {
            self.selection.remove(&id);
        } else {
            self.selection.insert(id);
        }
    }

    /// 按 Z 序返回 items（升序：底层在前，顶层在后）。
    pub fn items_by_z_order(&self) -> Vec<&Item> {
        let mut items: Vec<&Item> = self.items.iter().collect();
        items.sort_by_key(|a| a.z);
        items
    }

    /// 多选统一外框（spec L241：多选时画一个统一 bbox）。
    pub fn selection_bounding_rect(&self) -> Option<CanvasRect> {
        if self.selection.is_empty() {
            return None;
        }
        let mut rect: Option<CanvasRect> = None;
        for id in &self.selection {
            if let Some(item) = self.get_item(id) {
                let r = item.bounding_rect();
                rect = Some(match rect {
                    Some(b) => b.union(&r),
                    None => r,
                });
            }
        }
        rect
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.selection.clear();
        self.next_z = 0;
    }

    /// 清理孤儿 container_id：把指向`已不存在 item`的绑定文本 container_id 置 None（Phase C/Step 4）。
    /// 加载存档后调用，避免容器被删除/undo 后留下悬空引用。
    pub fn cleanup_orphan_containers(&mut self) {
        // 预先收集现存 item id，避免遍历 &mut self.items 时再次借贷 self
        let existing: HashSet<ItemId> = self.items.iter().map(|it| it.id).collect();
        for item in &mut self.items {
            if let crate::item::ItemKind::Text { container_id, .. } = &mut item.kind {
                if let Some(cid) = *container_id {
                    if !existing.contains(&cid) {
                        *container_id = None;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::StrokeStyle;
    use crate::Item;

    fn shape() -> Item {
        Item::new_shape(
            crate::shape::ShapeType::Rectangle,
            (100.0, 60.0),
            0.0,
            0.0,
            StrokeStyle::default(),
            None,
        )
    }

    fn text(container: ItemId) -> Item {
        Item::new_text_in("hello".into(), 50.0, 30.0, 20.0, [255; 4], container)
    }

    #[test]
    fn texts_bound_to_returns_only_matching_containers() {
        let s = shape();
        let other = shape();
        let t = text(s.id);
        let t_id = t.id;
        let free = Item::new_text("free".into(), 0.0, 0.0, 20.0, [255; 4]);
        let sid = s.id;
        let other_id = other.id;
        let mut scene = Scene::new();
        scene.add_item(s);
        scene.add_item(other);
        scene.add_item(t);
        scene.add_item(free);

        let ids = scene.texts_bound_to(other_id);
        assert!(ids.is_empty());
        let ids = scene.texts_bound_to(sid);
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], t_id);
    }

    #[test]
    fn is_bound_text_requires_live_container() {
        let s = shape();
        let t = text(s.id);
        let free = Item::new_text("free".into(), 0.0, 0.0, 20.0, [255; 4]);
        let t_id = t.id;
        let free_id = free.id;
        let sid = s.id;
        let mut scene = Scene::new();
        scene.add_item(s);
        scene.add_item(t);
        scene.add_item(free);

        // 容器在场景中 → 是绑定文本
        assert!(scene.is_bound_text(scene.get_item(&t_id).unwrap()));
        // 自由文本 → 不是
        assert!(!scene.is_bound_text(scene.get_item(&free_id).unwrap()));

        // 容器删除后（孤儿引用）→ 不再视为绑定文本
        scene.remove_item(&sid);
        assert!(!scene.is_bound_text(scene.get_item(&t_id).unwrap()));
    }

    #[test]
    fn cleanup_orphan_containers_nulls_missing_container() {
        let s = shape();
        let t = text(s.id);
        let t_id = t.id;
        let mut scene = Scene::new();
        // 只加文本不加容器 → 孤儿引用
        scene.add_item(t);
        // 但 t 绑定 s 不在场景中；清理后 container_id 应为 None
        scene.cleanup_orphan_containers();
        let t_item = scene.get_item(&t_id).unwrap();
        match &t_item.kind {
            crate::item::ItemKind::Text { container_id, .. } => assert_eq!(*container_id, None),
            _ => panic!("expected Text"),
        }
    }

    /// 复制：副本应带新 id 并整体偏移，且不改动原场景。
    #[test]
    fn duplicate_items_assigns_new_ids_and_offsets() {
        let s = shape();
        let s_id = s.id;
        let mut scene = Scene::new();
        scene.add_item(s);

        let dups = scene.duplicate_items(&[s_id], CanvasVector::new(10.0, -20.0));
        assert_eq!(dups.len(), 1);
        assert_ne!(dups[0].id, s_id, "副本必须换 id，否则与原件冲突");
        assert_eq!(dups[0].transform.pos.x, 10.0);
        assert_eq!(dups[0].transform.pos.y, -20.0);
        // 原场景未被改动（副本尚未入场景）
        assert_eq!(scene.items.len(), 1);
        assert_eq!(scene.get_item(&s_id).unwrap().transform.pos.x, 0.0);
    }

    /// 容器与其绑定文本一起复制时，副本文本应改绑到副本容器。
    #[test]
    fn duplicate_items_remaps_container_id_within_set() {
        let s = shape();
        let s_id = s.id;
        let t = text(s_id);
        let t_id = t.id;
        let mut scene = Scene::new();
        scene.add_item(s);
        scene.add_item(t);

        let dups = scene.duplicate_items(&[s_id, t_id], CanvasVector::zero());
        assert_eq!(dups.len(), 2);
        let new_container = dups[0].id;
        let dup_text = &dups[1];
        match &dup_text.kind {
            crate::item::ItemKind::Text { container_id, .. } => {
                assert_eq!(*container_id, Some(new_container));
            }
            _ => panic!("expected Text"),
        }
    }

    /// 只复制绑定文本（容器不在复制集）时，副本应退化为自由文本，
    /// 不能继续指向没被复制的原容器。
    #[test]
    fn duplicate_items_drops_container_id_outside_set() {
        let s = shape();
        let s_id = s.id;
        let t = text(s_id);
        let t_id = t.id;
        let mut scene = Scene::new();
        scene.add_item(s);
        scene.add_item(t);

        let dups = scene.duplicate_items(&[t_id], CanvasVector::zero());
        match &dups[0].kind {
            crate::item::ItemKind::Text { container_id, .. } => assert_eq!(*container_id, None),
            _ => panic!("expected Text"),
        }
    }

    fn frame(number: u32, x: f32, y: f32, w: f32, h: f32) -> Item {
        Item::new_frame(number, (w, h), x, y, None)
    }

    #[test]
    fn frame_members_includes_fully_inside_and_excludes_touching() {
        let fr = frame(1, 0.0, 0.0, 500.0, 400.0);
        let fr_id = fr.id;
        // 完全落在画框内 → 成员
        let inside = Item::new_shape(
            crate::shape::ShapeType::Rectangle,
            (100.0, 60.0),
            10.0,
            10.0,
            StrokeStyle::default(),
            None,
        );
        // 与画框右边相切（x=500 顶到边界，未被 6 阈值包含）→ 非成员
        let touching = Item::new_shape(
            crate::shape::ShapeType::Rectangle,
            (100.0, 60.0),
            500.0,
            10.0,
            StrokeStyle::default(),
            None,
        );
        // 内部画框 → 排除
        let nested = frame(2, 20.0, 20.0, 100.0, 100.0);
        let inside_id = inside.id;
        let touching_id = touching.id;
        let nested_id = nested.id;
        let mut scene = Scene::new();
        scene.add_item(fr);
        scene.add_item(inside);
        scene.add_item(touching);
        scene.add_item(nested);
        let ids = scene.frame_members(fr_id);
        assert_eq!(ids, vec![inside_id], "只应收纳完全在内的普通 item");
        assert!(!ids.contains(&touching_id), "相切不属于成员");
        assert!(!ids.contains(&nested_id), "内含画框被排除");
    }

    #[test]
    fn frame_members_excludes_non_frame_request() {
        let s = shape();
        let sid = s.id;
        let mut scene = Scene::new();
        scene.add_item(s);
        assert!(scene.frame_members(sid).is_empty());
    }

    #[test]
    fn frames_by_number_sorts_ascending() {
        let fr2 = frame(2, 0.0, 0.0, 10.0, 10.0);
        let fr1 = frame(1, 0.0, 0.0, 10.0, 10.0);
        let fr3 = frame(3, 0.0, 0.0, 10.0, 10.0);
        let id1 = fr1.id;
        let id2 = fr2.id;
        let id3 = fr3.id;
        let mut scene = Scene::new();
        scene.add_item(fr2);
        scene.add_item(fr1);
        scene.add_item(fr3);
        assert_eq!(scene.frames_by_number(), vec![id1, id2, id3]);
    }

    #[test]
    fn next_frame_number_is_max_plus_one() {
        let mut scene = Scene::new();
        assert_eq!(scene.next_frame_number(), 1);
        scene.add_item(frame(5, 0.0, 0.0, 10.0, 10.0));
        scene.add_item(frame(2, 0.0, 0.0, 10.0, 10.0));
        assert_eq!(scene.next_frame_number(), 6);
    }

    #[test]
    fn renumber_conflict_shifts_affected_and_undo_restores() {
        let a = frame(1, 0.0, 0.0, 10.0, 10.0);
        let b = frame(2, 0.0, 0.0, 10.0, 10.0);
        let c = frame(3, 0.0, 0.0, 10.0, 10.0);
        let a_id = a.id;
        let b_id = b.id;
        let c_id = c.id;
        let mut scene = Scene::new();
        scene.add_item(a);
        scene.add_item(b);
        scene.add_item(c);

        // 把 C 重新编号为 2 → 冲突项 B(>=2) 顺移 +1
        let plan = scene.plan_frame_renumber(c_id, 2);
        assert_eq!(plan.old_number, 3);
        assert_eq!(plan.new_number, 2);
        assert_eq!(plan.affected, vec![(b_id, 2)]);

        scene.apply_renumber(&plan);
        assert_eq!(scene.get_item(&a_id).unwrap().frame_number(), Some(1));
        assert_eq!(scene.get_item(&b_id).unwrap().frame_number(), Some(3));
        assert_eq!(scene.get_item(&c_id).unwrap().frame_number(), Some(2));

        scene.undo_renumber(&plan);
        assert_eq!(scene.get_item(&a_id).unwrap().frame_number(), Some(1));
        assert_eq!(scene.get_item(&b_id).unwrap().frame_number(), Some(2));
        assert_eq!(scene.get_item(&c_id).unwrap().frame_number(), Some(3));
    }

    #[test]
    fn renumber_clamps_to_min_one() {
        let a = frame(1, 0.0, 0.0, 10.0, 10.0);
        let a_id = a.id;
        let mut scene = Scene::new();
        scene.add_item(a);
        let plan = scene.plan_frame_renumber(a_id, 0);
        assert_eq!(plan.new_number, 1);
        scene.apply_renumber(&plan);
        assert_eq!(scene.get_item(&a_id).unwrap().frame_number(), Some(1));
    }

    /// plan #5：形状移动后，绑定到它的端点应联动跟随轮廓最近点。
    #[test]
    fn resolve_bindings_follows_moved_shape() {
        let rect = Item::new_shape(
            crate::shape::ShapeType::Rectangle,
            (100.0, 60.0),
            0.0,
            0.0,
            StrokeStyle::default(),
            None,
        );
        let rect_id = rect.id;
        // 线：起点 (200,30) 终点 (300,30)，起点绑定到 rect（初始 rect 占 [0,100]×[0,60]）
        let mut line = Item::new_polyline(
            vec![(200.0, 30.0), (300.0, 30.0)],
            (100.0, 0.0),
            None,
            None,
            false,
            0.0,
            0.0,
            StrokeStyle::default(),
        );
        line.set_start_binding(Some(rect_id));
        let line_id = line.id;

        let mut scene = Scene::new();
        scene.add_item(rect);
        scene.add_item(line);

        // 右移 rect 100 → 占 [100,200]×[0,60]
        scene.get_item_mut(&rect_id).unwrap().transform.pos += CanvasVector::new(100.0, 0.0);
        scene.resolve_bindings(&[rect_id]);

        let line = scene.get_item(&line_id).unwrap();
        assert_eq!(line.start_binding(), Some(rect_id));
        let pts = match &line.kind {
            crate::item::ItemKind::Shape { points, .. } => points.clone(),
            _ => panic!("expected Polyline"),
        };
        // 另一端点 (300,30) 不变；起点应吸附到 rect 新右边界 (200,30)
        assert!((pts[1].0 - 300.0).abs() < 1e-3 && (pts[1].1 - 30.0).abs() < 1e-3);
        assert!((pts[0].0 - 200.0).abs() < 1.0, "start x = {}", pts[0].0);
        assert!((pts[0].1 - 30.0).abs() < 1.0, "start y = {}", pts[0].1);
    }

    /// plan #5：绑定目标被删除后，DeleteItems 应清除指向它的悬空绑定。
    #[test]
    fn delete_items_clears_dead_binding() {
        use crate::commands::{Command, DeleteItems};

        let rect = Item::new_shape(
            crate::shape::ShapeType::Rectangle,
            (100.0, 60.0),
            0.0,
            0.0,
            StrokeStyle::default(),
            None,
        );
        let rect_id = rect.id;
        let mut line = Item::new_polyline(
            vec![(200.0, 30.0), (300.0, 30.0)],
            (100.0, 0.0),
            None,
            None,
            false,
            0.0,
            0.0,
            StrokeStyle::default(),
        );
        line.set_start_binding(Some(rect_id));
        let line_id = line.id;

        let mut scene = Scene::new();
        scene.add_item(rect);
        scene.add_item(line);

        let mut cmd = DeleteItems::new(vec![rect_id]);
        cmd.redo(&mut scene);

        let line = scene.get_item(&line_id).unwrap();
        assert_eq!(line.start_binding(), None);
    }
}
