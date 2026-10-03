use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

use crate::item::{
    elbow_axis_effective, elbow_axis_from_anchor, elbow_axis_hysteresis, Item, ItemId, ItemKind,
};
use crate::shape::{ElbowAxis, ShapeType};
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
        // 端点被重算的 elbow 连接器（第三遍刷新取向用）。
        let mut elbow_touched: HashSet<ItemId> = HashSet::new();
        for line in &self.items {
            let ItemKind::Shape {
                shape_type: ShapeType::Polyline | ShapeType::Elbow,
                points,
                start_binding,
                end_binding,
                ..
            } = &line.kind
            else {
                continue;
            };
            let is_elbow = matches!(
                &line.kind,
                ItemKind::Shape {
                    shape_type: ShapeType::Elbow,
                    ..
                }
            );
            let last = points.len().saturating_sub(1);
            if last == 0 {
                continue; // 退化（单点），无端点可言
            }
            // 起点（points[0]）
            if let Some(b) = start_binding {
                match self.get_item(&b.target) {
                    Some(shape) if moved.contains(&b.target) => {
                        let other = points.get(last).copied().unwrap_or((0.0, 0.0));
                        let query = line.local_to_canvas().transform_point(euclid::Point2D::<
                            f32,
                            crate::item::ItemLocalSpace,
                        >::new(
                            other.0, other.1
                        ));
                        // 有锚点：端点钉在目标局部系同一点（不随另一端滑动，
                        // 修复矩形移动时端点沿边滑动/自动变水平垂直）。
                        // 无锚点（旧存档迁移）：回退“另一端点最近轮廓点”。
                        let np = match b.anchor {
                            Some(anchor) => {
                                shape.local_to_canvas().transform_point(euclid::Point2D::<
                                    f32,
                                    crate::item::ItemLocalSpace,
                                >::new(
                                    anchor.0, anchor.1
                                ))
                            }
                            None => snap::nearest_outline_point(shape, query),
                        };
                        if let Some(local) = line.canvas_to_local_point(np) {
                            updates.push((line.id, 0, local));
                            if is_elbow {
                                elbow_touched.insert(line.id);
                            }
                        }
                    }
                    // 绑定目标已不存在（被删除）→ 清除悬空绑定
                    None => dead.push((line.id, 0)),
                    _ => {}
                }
            }
            // 终点（points[last]）
            if let Some(b) = end_binding {
                match self.get_item(&b.target) {
                    Some(shape) if moved.contains(&b.target) => {
                        let other = points.first().copied().unwrap_or((0.0, 0.0));
                        let query = line.local_to_canvas().transform_point(euclid::Point2D::<
                            f32,
                            crate::item::ItemLocalSpace,
                        >::new(
                            other.0, other.1
                        ));
                        let np = match b.anchor {
                            Some(anchor) => {
                                shape.local_to_canvas().transform_point(euclid::Point2D::<
                                    f32,
                                    crate::item::ItemLocalSpace,
                                >::new(
                                    anchor.0, anchor.1
                                ))
                            }
                            None => snap::nearest_outline_point(shape, query),
                        };
                        if let Some(local) = line.canvas_to_local_point(np) {
                            updates.push((line.id, last, local));
                            if is_elbow {
                                elbow_touched.insert(line.id);
                            }
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
        // 第三遍：elbow 取向刷新（plan #21 DP-B 落地）。端点重算后解析新取向并
        // 写回 `elbow_axis`——绑定锚点优先（锚点所在边法线 = 首/末段走向，与两端
        // 点相对位置解耦，拖动被绑图形不再因 Δ 关系翻转而 90° 跳变），无绑定回退
        // 滞回。重路由是派生结果不进 undo 栈；undo 还原端点后再走
        // resolve_bindings 会解析出同一取向，可自愈。
        for id in elbow_touched {
            self.refresh_elbow_axis(id);
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

    /// elbow 取向解析（只读，plan #21 DP-B 落地）：**绑定锚点优先**——起点绑定
    /// （次选终点绑定）的锚点钉在目标形状哪条边上，首/末段就沿该边法线走
    /// （[`elbow_axis_from_anchor`]），结构性走向与两端点相对位置解耦；两端均无
    /// 锚点绑定（自由 elbow / 旧档 anchor=None）时回退**滞回**
    /// （`elbow_axis_hysteresis`，对角线附近 ±30% 稳定带不振荡）。
    ///
    /// `start`/`end` 是**生效绑定**：通常传 item 的存储绑定；端点拖拽预览中
    /// 被拖端的绑定尚未落盘（吸附 pending / 拖离即解绑），调用方应传预览值。
    pub fn resolve_elbow_axis(
        &self,
        pts: &[(f32, f32)],
        stored: Option<ElbowAxis>,
        start: Option<&crate::item::EndpointBinding>,
        end: Option<&crate::item::EndpointBinding>,
    ) -> ElbowAxis {
        // 起点绑定优先（Z 拓扑下首/末段走向一致，冲突时起点语义占先）；
        // 锚点为 None（旧存档迁移）视同无绑定，落到滞回。
        let anchor = start
            .and_then(|b| b.anchor.map(|a| (b.target, a)))
            .or_else(|| end.and_then(|b| b.anchor.map(|a| (b.target, a))));
        match anchor {
            Some((target_id, a)) => self
                .get_item(&target_id)
                .map(|t| {
                    let size = t.base_size();
                    elbow_axis_from_anchor((size.x, size.y), a)
                })
                .unwrap_or_else(|| elbow_axis_hysteresis(pts, stored)),
            None => elbow_axis_hysteresis(pts, stored),
        }
    }

    /// elbow 取向解析并写回 `elbow_axis = Some(axis)`（plan #21 DP-B 落地）。
    /// 在端点变化的变更点调用（[`Self::resolve_bindings`] 等）；渲染 / 命中 /
    /// 手柄 / 导出只读该字段派生路径（同源不变量）。非 elbow 或 item 不存在时
    /// 无副作用。
    pub fn refresh_elbow_axis(&mut self, id: ItemId) {
        let axis = {
            let Some(item) = self.get_item(&id) else {
                return;
            };
            let ItemKind::Shape {
                shape_type: ShapeType::Elbow,
                points,
                start_binding,
                end_binding,
                elbow_axis,
                ..
            } = &item.kind
            else {
                return;
            };
            self.resolve_elbow_axis(
                points,
                *elbow_axis,
                start_binding.as_ref(),
                end_binding.as_ref(),
            )
        };
        if let Some(item) = self.get_item_mut(&id) {
            if let ItemKind::Shape {
                shape_type: ShapeType::Elbow,
                elbow_axis,
                ..
            } = &mut item.kind
            {
                *elbow_axis = Some(axis);
            }
        }
    }

    /// elbow 的完整派生路由（**局部坐标**；plan #24 阶段 C，DP-6 分层模型）：
    /// 任一端绑定 → A\* 避障（障碍 = 绑定目标旋转 AABB 四边膨胀
    /// [`crate::routing::ELBOW_PADDING`]，插座腿沿锚点边法线）；两端自由 →
    /// 确定性 L/Z/S（`elbow_mid_offset` bar 语义，旧档视觉不变）；固定段
    /// （阶段 D）按坐标锚定缝合；A\* 失败回退确定性规则。
    ///
    /// **同源不变量**：命中（`interaction.rs` 注入 `contains_canvas_point_with_route`）
    /// / 渲染（`ShapeData::elbow_route`）/ 手柄（段拖拽）/ 导出全部经本函数取
    /// 路径。纯函数、每帧重算、不缓存（非均匀网格 O(k²)，DP-6 拍板）。
    pub fn elbow_route_local(&self, item: &Item) -> Vec<(f32, f32)> {
        let ItemKind::Shape {
            shape_type: ShapeType::Elbow,
            points,
            elbow_mid_offset,
            elbow_axis,
            fixed_segments,
            start_binding,
            end_binding,
            ..
        } = &item.kind
        else {
            return Vec::new();
        };
        if points.len() != 2 {
            return points.clone();
        }
        let a_c = item.local_point_to_canvas(points[0]);
        let b_c = item.local_point_to_canvas(points[1]);
        let axis = elbow_axis_effective(points, *elbow_axis);
        // 自由端 heading（画布空间）：沿存储取向朝另一端。
        let unbound_heading = |self_pt: (f32, f32), other_pt: (f32, f32)| match axis {
            crate::shape::ElbowAxis::HorizontalFirst => ((other_pt.0 - self_pt.0).signum(), 0.0),
            crate::shape::ElbowAxis::VerticalFirst => (0.0, (other_pt.1 - self_pt.1).signum()),
        };
        // 绑定端解析：锚点边**外法线**（4 向，与旋转无关）→ 障碍收集。
        // multiplier：起点 +1（heading = 外法线 = 离开方向）；终点 -1（heading
        // = 内法线 = 进入方向）。
        let resolve_bound = |binding: Option<&crate::item::EndpointBinding>,
                             fallback: (f32, f32),
                             multiplier: f32,
                             obstacles: &mut Vec<(f32, f32, f32, f32)>|
         -> ((f32, f32), bool) {
            let Some(b) = binding else {
                return (fallback, false);
            };
            let Some(anchor) = b.anchor else {
                return (fallback, false);
            };
            let Some(target) = self.get_item(&b.target) else {
                return (fallback, false);
            };
            // 障碍 = 目标旋转 AABB 膨胀（仅矩形族可碰撞节点；线类/文字不作障碍）。
            if matches!(
                &target.kind,
                ItemKind::Shape {
                    shape_type: ShapeType::Rectangle | ShapeType::Ellipse | ShapeType::Diamond,
                    ..
                }
            ) {
                let r = target.bounding_rect();
                let rect = (
                    r.min().x - crate::routing::ELBOW_PADDING,
                    r.min().y - crate::routing::ELBOW_PADDING,
                    r.max().x + crate::routing::ELBOW_PADDING,
                    r.max().y + crate::routing::ELBOW_PADDING,
                );
                if !obstacles.contains(&rect) {
                    obstacles.push(rect);
                }
            }
            let size = target.base_size();
            let outward = crate::routing::elbow_heading_from_anchor((size.x, size.y), anchor);
            ((outward.0 * multiplier, outward.1 * multiplier), true)
        };
        let mut obstacles: Vec<(f32, f32, f32, f32)> = Vec::with_capacity(2);
        // 自由端 heading 两端同向（朝 b）：起点 = 离开方向，终点 = 进入方向
        // （Z 拓扑下首末段平行且同向，进入方向 = 从 a 侧指向 b）。
        let (start_heading, start_bound) = resolve_bound(
            start_binding.as_ref(),
            unbound_heading(a_c.into(), b_c.into()),
            1.0,
            &mut obstacles,
        );
        // 终点：multiplier = -1 把外法线翻成**内法线（进入方向）**——返回值即
        // end_heading，调用方不得再取反（曾在此双重取反，ev 落进形状内部令
        // A\* 必败回退穿越路由，2026-10-03 验收修复）。
        let (end_inward, end_bound) = resolve_bound(
            end_binding.as_ref(),
            unbound_heading(a_c.into(), b_c.into()),
            -1.0,
            &mut obstacles,
        );
        let route_canvas = crate::routing::elbow_route(&crate::routing::ElbowRouteInput {
            start: a_c.into(),
            end: b_c.into(),
            start_heading,
            end_heading: end_inward,
            start_bound,
            end_bound,
            offset: *elbow_mid_offset,
            fixed_segments,
            obstacles: &obstacles,
        });
        // 画布 → 局部（逆变换奇异时兜底回退 fallback 路由）。
        let local: Option<Vec<(f32, f32)>> = route_canvas
            .iter()
            .map(|&p| item.canvas_to_local_point(CanvasPoint::new(p.0, p.1)))
            .collect();
        local.unwrap_or_else(|| {
            crate::item::elbow_route_fallback(points, *elbow_mid_offset, axis, fixed_segments)
        })
    }

    // ─────────────────────────── 编组 / 解组（plan #13） ───────────────────────────

    /// 编组（G1 单组模型）：给 `ids` 中所有**实际存在**的项赋同一新组 id。
    /// 已属其它组的项自动离开旧组（赋新 id 即换组，无需先手动解组）。
    /// 少于 2 项时不产生组（单元素编组无意义），返回 `None`。
    ///
    /// 注意：本方法只改数据，不入 undo 栈——UI 层应配合 [`crate::commands::SetGroup`]。
    pub fn group(&mut self, ids: &[ItemId]) -> Option<Uuid> {
        if ids.len() < 2 {
            return None;
        }
        let gid = Uuid::new_v4();
        let set: HashSet<ItemId> = ids.iter().copied().collect();
        for item in &mut self.items {
            if set.contains(&item.id) {
                item.group_id = Some(gid);
            }
        }
        Some(gid)
    }

    /// 解组：清空 `ids` 中各项的 `group_id`。其余同组成员不受影响（G3：
    /// 部分成员被移出时同组继续存在；彻底解组需整组传入或逐项 `Ctrl+Shift+G`）。
    pub fn ungroup(&mut self, ids: &[ItemId]) {
        let set: HashSet<ItemId> = ids.iter().copied().collect();
        for item in &mut self.items {
            if set.contains(&item.id) {
                item.group_id = None;
            }
        }
    }

    /// `item_id` 所属组的全部成员（**含自身**）；未编组时返回 `[item_id]`。
    /// 用于点击命中扩展整组（G2）。
    pub fn group_members_of(&self, item_id: &ItemId) -> Vec<ItemId> {
        let Some(gid) = self.get_item(item_id).and_then(|it| it.group_id) else {
            return vec![*item_id];
        };
        self.items
            .iter()
            .filter(|it| it.group_id == Some(gid))
            .map(|it| it.id)
            .collect()
    }

    /// 命中 id 集合 → 按组扩展后的完整选中集（未编组的保持原样）。
    /// 框选等"按实际命中成员"的场景不需要本方法（G2：框选收实际命中）。
    pub fn expand_to_groups(&self, ids: &[ItemId]) -> Vec<ItemId> {
        let mut out: Vec<ItemId> = Vec::new();
        for id in ids {
            for member in self.group_members_of(id) {
                if !out.contains(&member) {
                    out.push(member);
                }
            }
        }
        out
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
    ///   （不能让副本文字仍绑在没被复制的原件容器上）；
    /// - `group_id`（plan #13）：整组被复制时副本共享一个**新**组 id（组结构保留）；
    ///   组只有部分成员被复制时副本退化为未编组（避免点击副本选中原组）。
    ///
    /// 传入顺序即返回顺序，便于调用方建立一一对应的 id 映射。
    pub fn duplicate_items(&self, ids: &[ItemId], offset: CanvasVector) -> Vec<Item> {
        use std::collections::HashMap;

        let id_map: HashMap<ItemId, ItemId> =
            ids.iter().map(|id| (*id, uuid::Uuid::new_v4())).collect();

        // plan #13：副本的组映射。对复制集里出现的每个旧组：
        // 该组**全部**成员都在复制集内 → 副本共享一个新组 id（组结构保留）；
        // 否则副本退化为未编组（保留旧组 id 会导致点击副本选中原组）。
        let in_set: std::collections::HashSet<ItemId> = ids.iter().copied().collect();
        let mut group_map: HashMap<Uuid, Option<Uuid>> = HashMap::new();
        for id in ids {
            let Some(src) = self.get_item(id) else {
                continue;
            };
            let Some(gid) = src.group_id else {
                continue;
            };
            if group_map.contains_key(&gid) {
                continue;
            }
            let complete = self
                .items
                .iter()
                .filter(|it| it.group_id == Some(gid))
                .all(|it| in_set.contains(&it.id));
            group_map.insert(gid, complete.then(Uuid::new_v4));
        }

        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let Some(src) = self.get_item(id) else {
                continue;
            };
            let mut dup = src.clone();
            dup.id = id_map[id];
            dup.transform.pos += offset;
            dup.group_id = src
                .group_id
                .and_then(|g| group_map.get(&g).copied().flatten());
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

    /// 全选（Ctrl+A，Excalidraw 同款）。绑定文本不参与：它随容器联动
    /// （同框选语义，见 binary 层框选过滤），单独选中只会得到一个
    /// 移动不了的可视快照选中框。
    pub fn select_all(&mut self) {
        let ids: Vec<ItemId> = self
            .items
            .iter()
            .filter(|item| !self.is_bound_text(item))
            .map(|item| item.id)
            .collect();
        for id in ids {
            self.select(id);
        }
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

    /// 全部 item 的 AABB 并集（画布空间）；空场景返回 `None`。
    /// 「适配全部 / Zoom to fit」（Shift+1）的纯决策——过去在 app 层内联重算，现归 core。
    /// 与 [`selection_bounding_rect`] 共用同一 union 语义，只是遍历全集而非选区。
    pub fn content_bounding_rect(&self) -> Option<CanvasRect> {
        let mut rect: Option<CanvasRect> = None;
        for item in &self.items {
            let r = item.bounding_rect();
            rect = Some(match rect {
                Some(b) => b.union(&r),
                None => r,
            });
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
    fn content_bounding_rect_is_none_for_empty_scene() {
        let scene = Scene::new();
        assert!(scene.content_bounding_rect().is_none());
    }

    #[test]
    fn content_bounding_rect_unions_all_items() {
        let mut a = shape(); // 100×60 于原点
        a.transform.pos = CanvasVector::new(0.0, 0.0);
        let mut b = shape();
        b.transform.pos = CanvasVector::new(150.0, 0.0); // 右移，留 50px 间隙
        let mut scene = Scene::new();
        scene.add_item(a);
        scene.add_item(b);

        let r = scene.content_bounding_rect().expect("非空应有并集");
        assert_eq!(
            (r.min_x(), r.min_y(), r.max_x(), r.max_y()),
            (0.0, 0.0, 250.0, 60.0)
        );
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

    /// 全选（Ctrl+A，Excalidraw 同款）：绑定文字不参与（随容器联动，同框选
    /// 语义），形状与自由文字都参与。
    #[test]
    fn select_all_skips_bound_text() {
        let s = shape();
        let t = text(s.id);
        let free = Item::new_text("free".into(), 0.0, 0.0, 20.0, [255; 4]);
        let (sid, t_id, free_id) = (s.id, t.id, free.id);
        let mut scene = Scene::new();
        scene.add_item(s);
        scene.add_item(t);
        scene.add_item(free);

        scene.select_all();
        assert!(scene.selection_contains(&sid));
        assert!(scene.selection_contains(&free_id));
        assert!(!scene.selection_contains(&t_id), "绑定文字不参与全选");
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
        line.set_start_binding(Some(crate::item::EndpointBinding {
            target: rect_id,
            // 旧语义：无锚点（最近轮廓点回退），行为与原测试一致
            anchor: None,
        }));
        let line_id = line.id;

        let mut scene = Scene::new();
        scene.add_item(rect);
        scene.add_item(line);

        // 右移 rect 100 → 占 [100,200]×[0,60]
        scene.get_item_mut(&rect_id).unwrap().transform.pos += CanvasVector::new(100.0, 0.0);
        scene.resolve_bindings(&[rect_id]);

        let line = scene.get_item(&line_id).unwrap();
        assert_eq!(
            line.start_binding(),
            Some(crate::item::EndpointBinding {
                target: rect_id,
                anchor: None,
            })
        );
        let pts = match &line.kind {
            crate::item::ItemKind::Shape { points, .. } => points.clone(),
            _ => panic!("expected Polyline"),
        };
        // 另一端点 (300,30) 不变；起点应吸附到 rect 新右边界 (200,30)
        assert!((pts[1].0 - 300.0).abs() < 1e-3 && (pts[1].1 - 30.0).abs() < 1e-3);
        assert!((pts[0].0 - 200.0).abs() < 1.0, "start x = {}", pts[0].0);
        assert!((pts[0].1 - 30.0).abs() < 1.0, "start y = {}", pts[0].1);
    }

    /// plan #14 锚点绑定：移动矩形时端点钉在锚点（目标局部系同一点），
    /// 不再重贴"另一端点的最近轮廓点"（否则端点沿边滑动、直线被拉成水平/垂直）。
    #[test]
    fn resolve_bindings_anchored_endpoint_stays_pinned() {
        // 矩形占 [0,100]×[0,60]；线起点钉在锚点 (100, 40)（右边缘下部），
        // 另一端在 (400, 200)（斜向下）。右移矩形 50 后锚点画布位置为 (150, 40)。
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
            vec![(100.0, 40.0), (400.0, 200.0)],
            (0.0, 0.0),
            None,
            None,
            false,
            0.0,
            0.0,
            StrokeStyle::default(),
        );
        line.set_start_binding(Some(crate::item::EndpointBinding {
            target: rect_id,
            anchor: Some((100.0, 40.0)),
        }));
        let line_id = line.id;

        let mut scene = Scene::new();
        scene.add_item(rect);
        scene.add_item(line);

        scene.get_item_mut(&rect_id).unwrap().transform.pos += CanvasVector::new(50.0, 0.0);
        scene.resolve_bindings(&[rect_id]);

        let pts = match &scene.get_item(&line_id).unwrap().kind {
            crate::item::ItemKind::Shape { points, .. } => points.clone(),
            _ => panic!("expected Polyline"),
        };
        // 锚点 (100,40) 随矩形平移 → 画布 (150,40)；线局部无变换 → points[0]=(150,40)
        assert!((pts[0].0 - 150.0).abs() < 1e-3, "x = {}", pts[0].0);
        assert!((pts[0].1 - 40.0).abs() < 1e-3, "y = {}", pts[0].1);
        // 另一端不受影响
        assert!((pts[1].0 - 400.0).abs() < 1e-3 && (pts[1].1 - 200.0).abs() < 1e-3);
    }

    /// plan #21 DP-B 集成：绑定 elbow 的取向由锚点边驱动——被绑图形拖到 Δ 关系
    /// 翻转的位置（dy 从 < dx 变为 > dx）取向不变，路由不再 90° 跳变。
    #[test]
    fn resolve_bindings_elbow_axis_follows_anchor_not_delta() {
        use crate::item::elbow_axis_effective;
        use crate::shape::ElbowAxis;

        // 矩形 [0,100]×[0,60]；elbow 起点 = 右边缘锚点 (100,30)、终点 (400,80)。
        // 初始 dy=50 < dx=300；把矩形拖到让终点几何 dy ≫ dx 时，起点锚点仍在
        // 右边缘（法线水平）→ 取向应保持 HorizontalFirst 而非翻转为 VerticalFirst。
        let rect = Item::new_shape(
            ShapeType::Rectangle,
            (100.0, 60.0),
            0.0,
            0.0,
            StrokeStyle::default(),
            None,
        );
        let rect_id = rect.id;
        let mut elbow = Item::new_elbow(
            vec![(100.0, 30.0), (400.0, 80.0)],
            (300.0, 50.0),
            None,
            None,
            0.0,
            0.0,
            StrokeStyle::default(),
        );
        if let ItemKind::Shape {
            shape_type,
            start_binding,
            end_binding,
            ..
        } = &mut elbow.kind
        {
            *shape_type = ShapeType::Elbow;
            *start_binding = Some(crate::item::EndpointBinding {
                target: rect_id,
                anchor: Some((100.0, 30.0)),
            });
            *end_binding = Some(crate::item::EndpointBinding {
                target: ItemId::new_v4(),
                anchor: Some((0.0, 0.0)),
            });
        }
        let elbow_id = elbow.id;

        let mut scene = Scene::new();
        scene.add_item(rect);
        scene.add_item(elbow);

        // 拖动矩形到终点上方远处：锚点画布位置 (300, -400)，端点重算后
        // dx = 100、dy = -430 → 几何关系已翻转（dy ≫ dx）。
        scene.get_item_mut(&rect_id).unwrap().transform.pos += CanvasVector::new(200.0, -430.0);
        scene.resolve_bindings(&[rect_id]);

        let (pts, axis) = match &scene.get_item(&elbow_id).unwrap().kind {
            ItemKind::Shape {
                points, elbow_axis, ..
            } => (points.clone(), *elbow_axis),
            _ => panic!("expected Elbow"),
        };
        assert!(
            (pts[0].0 - 300.0).abs() < 1e-3 && (pts[0].1 + 400.0).abs() < 1e-3,
            "起点应钉在锚点新画布位置 (300,-400)：{:?}",
            pts[0]
        );
        assert_eq!(
            elbow_axis_effective(&pts, axis),
            ElbowAxis::HorizontalFirst,
            "锚点在右边缘 → 取向恒水平先走，Δ 翻转不跳变"
        );
    }

    /// 验收反馈（2026-10-03）回归：两端都锚在**上边缘**、形状左右并列——路由
    /// 应从上边缘向上离开、从上方进入，绝不穿过形状内部（曾因 end_heading
    /// 双重取反令 A\* 必败、回退 Z 直接穿越）。
    #[test]
    fn elbow_route_top_anchors_side_by_side_avoids_shapes() {
        use crate::shape::ElbowAxis;
        let rect_a = Item::new_shape(
            ShapeType::Rectangle,
            (100.0, 60.0),
            0.0,
            0.0,
            StrokeStyle::default(),
            None,
        );
        let rect_b = Item::new_shape(
            ShapeType::Rectangle,
            (100.0, 60.0),
            300.0,
            0.0,
            StrokeStyle::default(),
            None,
        );
        let (a_id, b_id) = (rect_a.id, rect_b.id);
        // elbow：起点锚 A 上边缘 (50,0)，终点锚 B 上边缘 (50,0)（画布 350,0）。
        let mut elbow = Item::new_elbow(
            vec![(50.0, 0.0), (350.0, 0.0)],
            (300.0, 0.0),
            None,
            None,
            0.0,
            0.0,
            StrokeStyle::default(),
        );
        if let ItemKind::Shape {
            shape_type,
            start_binding,
            end_binding,
            elbow_axis,
            ..
        } = &mut elbow.kind
        {
            *shape_type = ShapeType::Elbow;
            *start_binding = Some(crate::item::EndpointBinding {
                target: a_id,
                anchor: Some((50.0, 0.0)),
            });
            *end_binding = Some(crate::item::EndpointBinding {
                target: b_id,
                anchor: Some((50.0, 0.0)),
            });
            *elbow_axis = Some(ElbowAxis::VerticalFirst);
        }
        let mut scene = Scene::new();
        scene.add_item(rect_a);
        scene.add_item(rect_b);
        scene.add_item(elbow);
        let item = scene.get_item(&elbow_id_of(&scene)).unwrap();
        let route = scene.elbow_route_local(item);
        // 画布空间断言（item 无变换：局部 == 画布）。
        assert_eq!(route.first(), Some(&(50.0, 0.0)));
        assert_eq!(route.last(), Some(&(350.0, 0.0)));
        // 首段向上离开上边缘（插座腿方向）。
        assert!(route[1].1 < 0.0, "首段应向上离开：{:?}", route[1]);
        // 任何分段中点都不得落入两个形状的**未膨胀** AABB（真避障）。
        for w in route.windows(2) {
            let m = ((w[0].0 + w[1].0) * 0.5, (w[0].1 + w[1].1) * 0.5);
            for (x0, y0, x1, y1) in [(0.0, 0.0, 100.0, 60.0), (300.0, 0.0, 400.0, 60.0)] {
                assert!(
                    !(m.0 > x0 && m.0 < x1 && m.1 > y0 && m.1 < y1),
                    "段 {:?}→{:?} 中点 {:?} 穿过形状",
                    w[0],
                    w[1],
                    m
                );
            }
        }
    }

    /// 验收反馈（2026-10-03）回归：起于右边缘、止于上边缘（目标更低）——
    /// 最少转弯路由应为 **L 形**（1 个拐点），而非 Z 形穿障碍。
    #[test]
    fn elbow_route_right_edge_to_top_edge_is_l_shaped() {
        use crate::shape::ElbowAxis;
        let rect_a = Item::new_shape(
            ShapeType::Rectangle,
            (100.0, 60.0),
            0.0,
            0.0,
            StrokeStyle::default(),
            None,
        );
        // B 在 A 右下方：右缘锚点 (100,30)，B 上边缘锚点画布 (350,120)。
        let rect_b = Item::new_shape(
            ShapeType::Rectangle,
            (100.0, 60.0),
            300.0,
            120.0,
            StrokeStyle::default(),
            None,
        );
        let (a_id, b_id) = (rect_a.id, rect_b.id);
        let mut elbow = Item::new_elbow(
            vec![(100.0, 30.0), (350.0, 120.0)],
            (250.0, 90.0),
            None,
            None,
            0.0,
            0.0,
            StrokeStyle::default(),
        );
        if let ItemKind::Shape {
            shape_type,
            start_binding,
            end_binding,
            elbow_axis,
            ..
        } = &mut elbow.kind
        {
            *shape_type = ShapeType::Elbow;
            *start_binding = Some(crate::item::EndpointBinding {
                target: a_id,
                anchor: Some((100.0, 30.0)),
            });
            *end_binding = Some(crate::item::EndpointBinding {
                target: b_id,
                anchor: Some((50.0, 0.0)),
            });
            *elbow_axis = Some(ElbowAxis::HorizontalFirst);
        }
        let mut scene = Scene::new();
        scene.add_item(rect_a);
        scene.add_item(rect_b);
        scene.add_item(elbow);
        let item = scene.get_item(&elbow_id_of(&scene)).unwrap();
        let route = scene.elbow_route_local(item);
        // L 形：[起点, 拐点, 终点]，拐点 = (350, 30)（先右行至目标 x，再下行进入上边缘）。
        assert_eq!(route.len(), 3, "应为 L 形（1 拐点）：{:?}", route);
        assert_eq!(route[1], (350.0, 30.0), "拐点应在目标 x 与源 y 的交点");
        // 分段不穿形状。
        for w in route.windows(2) {
            let m = ((w[0].0 + w[1].0) * 0.5, (w[0].1 + w[1].1) * 0.5);
            for (x0, y0, x1, y1) in [(0.0, 0.0, 100.0, 60.0), (300.0, 120.0, 400.0, 180.0)] {
                assert!(
                    !(m.0 > x0 && m.0 < x1 && m.1 > y0 && m.1 < y1),
                    "段 {:?}→{:?} 中点 {:?} 穿过形状",
                    w[0],
                    w[1],
                    m
                );
            }
        }
    }

    /// 辅助：取场景中唯一 elbow 的 id（测试用）。
    fn elbow_id_of(scene: &Scene) -> ItemId {
        scene
            .items
            .iter()
            .find(|it| {
                matches!(
                    it.kind,
                    ItemKind::Shape {
                        shape_type: ShapeType::Elbow,
                        ..
                    }
                )
            })
            .unwrap()
            .id
    }

    /// 旧存档兼容：`start_binding` 为纯 uuid 字符串时迁移为无锚点绑定。
    #[test]
    fn serde_legacy_binding_id_migrates_without_anchor() {
        let id = uuid::Uuid::new_v4();
        let json = format!(
            r#"{{"Shape":{{"shape_type":"Polyline","base_size":[100.0,0.0],
            "points":[[0.0,0.0],[100.0,0.0]],"start_arrow":null,"end_arrow":null,
            "closed":false,"curve_type":"straight","roundness":0.0,"seed":0,
            "sloppiness":"off","stroke":{{"color":[255,255,255,255],"width":2.0,"dash":"Solid"}},
            "fill":null,"fill_style":"solid","start_binding":"{id}","end_binding":null}}}}"#
        );
        let kind: crate::item::ItemKind = serde_json::from_str(&json).unwrap();
        match kind {
            crate::item::ItemKind::Shape { start_binding, .. } => {
                assert_eq!(start_binding.map(|b| b.target), Some(id));
                assert_eq!(start_binding.and_then(|b| b.anchor), None);
            }
            _ => panic!("expected Shape"),
        }
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
        line.set_start_binding(Some(crate::item::EndpointBinding {
            target: rect_id,
            anchor: None,
        }));
        let line_id = line.id;

        let mut scene = Scene::new();
        scene.add_item(rect);
        scene.add_item(line);

        let mut cmd = DeleteItems::new(vec![rect_id]);
        cmd.redo(&mut scene);

        let line = scene.get_item(&line_id).unwrap();
        assert_eq!(line.start_binding(), None);
    }

    // ─────────────────────── 编组 / 解组（plan #13） ───────────────────────

    #[test]
    fn group_assigns_same_id_and_expand_returns_all_members() {
        let a = shape();
        let b = shape();
        let (a_id, b_id) = (a.id, b.id);
        let mut scene = Scene::new();
        scene.add_item(a);
        scene.add_item(b);
        let c = shape();
        let c_id = c.id;
        scene.add_item(c);

        let gid = scene.group(&[a_id, b_id]).expect(">=2 items should group");
        assert_eq!(scene.get_item(&a_id).unwrap().group_id, Some(gid));
        assert_eq!(scene.get_item(&b_id).unwrap().group_id, Some(gid));
        assert_eq!(scene.get_item(&c_id).unwrap().group_id, None);

        // G2：点击命中任一成员 → 扩展为整组
        let expanded = scene.expand_to_groups(&[a_id]);
        assert_eq!(expanded.len(), 2);
        assert!(expanded.contains(&b_id));
        // 未编组项保持原样
        assert_eq!(scene.expand_to_groups(&[c_id]), vec![c_id]);
        // 单项 group 无效
        assert!(scene.group(&[c_id]).is_none());
    }

    #[test]
    fn group_moves_item_out_of_old_group() {
        // G1：已属组 A 的项与其它项一起编组 → 自动离开组 A 进入新组
        let a = shape();
        let b = shape();
        let c = shape();
        let (a_id, b_id, c_id) = (a.id, b.id, c.id);
        let mut scene = Scene::new();
        scene.add_item(a);
        scene.add_item(b);
        scene.add_item(c);

        let gid_a = scene.group(&[a_id, b_id]).unwrap();
        let gid_new = scene.group(&[a_id, c_id]).unwrap();
        assert_ne!(gid_a, gid_new);
        // a 换组，b 仍留在旧组（组 A 继续存在，G3）
        assert_eq!(scene.get_item(&a_id).unwrap().group_id, Some(gid_new));
        assert_eq!(scene.get_item(&b_id).unwrap().group_id, Some(gid_a));
        assert_eq!(scene.get_item(&c_id).unwrap().group_id, Some(gid_new));
    }

    #[test]
    fn ungroup_only_clears_given_members() {
        let a = shape();
        let b = shape();
        let c = shape();
        let (a_id, b_id, c_id) = (a.id, b.id, c.id);
        let mut scene = Scene::new();
        scene.add_item(a);
        scene.add_item(b);
        scene.add_item(c);
        let gid = scene.group(&[a_id, b_id, c_id]).unwrap();

        // G3：只解组 a → 其余成员同组继续存在
        scene.ungroup(&[a_id]);
        assert_eq!(scene.get_item(&a_id).unwrap().group_id, None);
        assert_eq!(scene.get_item(&b_id).unwrap().group_id, Some(gid));
        assert_eq!(scene.get_item(&c_id).unwrap().group_id, Some(gid));
        // 点击 b 仍扩展出 {b, c}
        let expanded = scene.expand_to_groups(&[b_id]);
        assert_eq!(expanded.len(), 2);
    }

    #[test]
    fn set_group_command_roundtrip() {
        use crate::commands::{Command, SetGroup};

        let a = shape();
        let b = shape();
        let (a_id, b_id) = (a.id, b.id);
        let mut scene = Scene::new();
        scene.add_item(a);
        scene.add_item(b);

        let mut cmd = SetGroup::group(&scene, &[a_id, b_id]).unwrap();
        cmd.redo(&mut scene);
        let gid = scene.get_item(&a_id).unwrap().group_id.unwrap();
        assert_eq!(scene.get_item(&b_id).unwrap().group_id, Some(gid));

        cmd.undo(&mut scene);
        assert_eq!(scene.get_item(&a_id).unwrap().group_id, None);
        assert_eq!(scene.get_item(&b_id).unwrap().group_id, None);

        cmd.redo(&mut scene);
        assert_eq!(scene.get_item(&a_id).unwrap().group_id, Some(gid));

        // 解组命令：undo 恢复编组
        let mut un = SetGroup::ungroup(&scene, &[a_id, b_id]).unwrap();
        un.redo(&mut scene);
        assert_eq!(scene.get_item(&a_id).unwrap().group_id, None);
        un.undo(&mut scene);
        assert_eq!(scene.get_item(&a_id).unwrap().group_id, Some(gid));
    }

    #[test]
    fn duplicate_full_group_keeps_grouping() {
        let a = shape();
        let b = shape();
        let (a_id, b_id) = (a.id, b.id);
        let mut scene = Scene::new();
        scene.add_item(a);
        scene.add_item(b);
        let gid = scene.group(&[a_id, b_id]).unwrap();

        let dups = scene.duplicate_items(&[a_id, b_id], CanvasVector::new(10.0, 0.0));
        assert_eq!(dups.len(), 2);
        let dg = dups[0].group_id;
        assert!(dg.is_some());
        assert_ne!(dg, Some(gid), "副本应是新组，不能共享原组 id");
        assert_eq!(dups[1].group_id, dg, "整组复制的副本应同属一个新组");
    }

    #[test]
    fn duplicate_partial_group_drops_grouping() {
        let a = shape();
        let b = shape();
        let c = shape();
        let (a_id, b_id, c_id) = (a.id, b.id, c.id);
        let mut scene = Scene::new();
        scene.add_item(a);
        scene.add_item(b);
        scene.add_item(c);
        scene.group(&[a_id, b_id, c_id]);

        // 只复制一个成员 → 副本退化为未编组
        let dups = scene.duplicate_items(&[a_id], CanvasVector::new(10.0, 0.0));
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].group_id, None);
    }
}
