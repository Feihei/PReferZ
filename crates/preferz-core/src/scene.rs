use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::item::{Item, ItemId};
use crate::spaces::CanvasRect;

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
}
