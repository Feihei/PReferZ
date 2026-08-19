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
}
