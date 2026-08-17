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
}
