//! actions — 从 preferz_app 拆出的方法段（架构整固 Step 3c）。方法体逐字搬迁。
//! `use super::*;` 取 mod.rs 的模块级词汇与私有项（子模块可访问父模块私有）。
use super::*;

impl PReferZApp {
    /// 编组选中项（plan #13，`Ctrl+G`）。G1 单组：已属别组的项自动换组；
    /// 少于 2 项不动作（单元素编组无意义）。
    pub(crate) fn group_selected(&mut self) {
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
    pub(crate) fn ungroup_selected(&mut self) {
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

    pub(crate) fn delete_selected(&mut self) {
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
    /// 选区带容器联动（封闭形状的绑定文本；画框不连带成员），副本偏移 10px 画布避免完全重叠；
    /// 副本已经入场景，push `AddItems(preview_applied=true)`，一次 undo 撤掉整个复制。
    pub(crate) fn duplicate_in_place(&mut self) {
        let selected: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
        if selected.is_empty() {
            return;
        }
        // 容器联动：封闭形状的绑定文本。画框不连带成员（构图工具语义，
        // 与拖拽一致；整体复制需先与成员编组）。
        let mut collected = selected.clone();
        for sid in &selected {
            collected.extend(self.scene.texts_bound_to(*sid));
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

    /// 复制选中项到剪贴板（`Ctrl+C`）。容器联动与 [`Self::duplicate_in_place`]
    /// 一致（封闭形状的绑定文本；画框不连带成员）。分流：
    /// - 选区恰为**单张图片**时，同时把 RGBA 像素写入系统剪贴板（可粘到外部应用）；
    /// - 其余（多张图 / 形状 / 文字 / 混合）只进应用内缓冲 `clipboard_items`，
    ///   需要图片形式走「导出选区」。
    pub(crate) fn copy_selected(&mut self) {
        let selected: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
        if selected.is_empty() {
            return;
        }
        // 容器联动与拖拽 / Ctrl+D 一致（封闭形状的绑定文本；画框不连带成员）
        let mut collected = selected.clone();
        for sid in &selected {
            collected.extend(self.scene.texts_bound_to(*sid));
        }
        collected.sort();
        collected.dedup();
        // 存原位置快照；粘贴时才分配新 id 并重定位
        let items: Vec<Item> = collected
            .iter()
            .filter_map(|id| self.scene.get_item(id).cloned())
            .collect();
        let n = items.len();
        if n == 0 {
            return;
        }
        // 单张图片 → 系统剪贴板（多选 / 非图片不走此路）
        if n == 1 {
            if let Some(item) = items.first() {
                if let ItemKind::Pixmap { texture_id, .. } = &item.kind {
                    self.copy_pixmap_to_system_clipboard(*texture_id);
                }
            }
        }
        self.clipboard_items = items;
        self.flash(fill(t(self.lang, T::FlashCopied), &[n.to_string()]));
    }

    /// 剪切选中项（`Ctrl+X`）：同 [`Self::copy_selected`] 分流 + 删除选中
    /// （删除走 undo stack，一次 undo 恢复）。
    pub(crate) fn cut_selected(&mut self) {
        let had_selection = !self.scene.selection.is_empty();
        self.copy_selected();
        // copy_selected 里清不掉选区；仅在实际复制到内容后删除。
        // 复制失败（选区空）时 delete_selected 自身也会空转，无需回滚缓冲。
        if had_selection && !self.clipboard_items.is_empty() {
            self.delete_selected();
            self.flash(fill(
                t(self.lang, T::FlashCut),
                &[self.clipboard_items.len().to_string()],
            ));
        }
    }

    /// 把 `rgba_pixel_cache` 中该纹理的像素写入系统剪贴板（单图复制）。
    /// 像素缓存缺失（理论上不该发生：可见 Pixmap 均已解码）或剪贴板打开失败
    /// 时静默降级——内部缓冲已存，Ctrl+V 仍可用。
    fn copy_pixmap_to_system_clipboard(&mut self, texture_id: u64) {
        let Some(rgba) = self.rgba_pixel_cache.get(&texture_id).cloned() else {
            return;
        };
        let Some(&(w, h)) = self.rgba_size_cache.get(&texture_id) else {
            return;
        };
        let Ok(mut clipboard) = arboard::Clipboard::new() else {
            return;
        };
        let _ = clipboard.set_image(arboard::ImageData {
            width: w as usize,
            height: h as usize,
            bytes: std::borrow::Cow::Owned(rgba),
        });
    }

    /// 粘贴应用内剪贴板缓冲（Ctrl+V 的内部路径）。以缓冲内容包围盒**中心**对齐
    /// 鼠标位置整体平移，分配新 id（`duplicate_items` 复用：组结构 / 容器绑定
    /// 映射语义与 Ctrl+D 一致），选区切到副本，一条 undo 可撤。
    /// 返回是否实际粘贴。
    pub(crate) fn paste_internal(&mut self, mouse: Option<egui::Pos2>) -> bool {
        if self.clipboard_items.is_empty() {
            return false;
        }
        // 缓冲的包围盒中心（原位置快照）
        let mut ids: Vec<ItemId> = self.clipboard_items.iter().map(|it| it.id).collect();
        ids.sort();
        ids.dedup();
        let mut min = CanvasPoint::new(f32::MAX, f32::MAX);
        let mut max = CanvasPoint::new(f32::MIN, f32::MIN);
        for it in &self.clipboard_items {
            let r = it.bounding_rect();
            min.x = min.x.min(r.min().x);
            min.y = min.y.min(r.min().y);
            max.x = max.x.max(r.max().x);
            max.y = max.y.max(r.max().y);
        }
        if min.x > max.x {
            return false;
        }
        let center = CanvasPoint::new((min.x + max.x) / 2.0, (min.y + max.y) / 2.0);
        let target = match mouse {
            Some(p) => self.viewport.pos2_to_canvas(p),
            // 无指针位置（极少见）：原位偏移，同 Ctrl+D
            None => center + CanvasVector::new(10.0, 10.0),
        };
        let offset = target - center;
        // 临时 Scene 才能调 duplicate_items（它按 id 从 items 查原件）。
        // 缓冲快照不持旧场景引用，仅借其 id 查询。
        let mut probe = Scene::default();
        for it in &self.clipboard_items {
            probe.add_item(it.clone());
        }
        let dups = probe.duplicate_items(&ids, offset);
        if dups.is_empty() {
            return false;
        }
        for dup in &dups {
            self.scene.add_item(dup.clone());
        }
        self.scene.deselect_all();
        for dup in &dups {
            self.scene.select(dup.id);
        }
        let n = dups.len();
        // 副本已手动入场景（同 duplicate_in_place），push 时跳过首次 redo——
        // 否则命令 redo 再加一遍：同 id 两份重叠，点击一个两个都被选中。
        self.push_cmd(Box::new(AddItems::new(dups).with_preview_applied(true)));
        self.flash(fill(t(self.lang, T::FlashDuplicated), &[n.to_string()]));
        true
    }

    /// plan #7：流程图节点创建（`Ctrl+方向`，单选矩形/椭圆/菱形时）。沿该向
    /// 克隆一个**同源同风格**节点（不复制绑定文字），并连一条两端绑定的**直箭头**
    /// （本仓库无 elbow）：anchor=各自朝向对方的边中点、初始端点即该锚点画布位置，
    /// 与 `resolve_bindings` 重算结果一致，后续移动形状箭头自动跟随。按下即提交：
    /// 新节点+箭头 `AddItems` 一条 undo，选区跳新节点。前提不满足静默（对齐 Excalidraw）。
    ///
    /// 落位（对齐 Excalidraw `placeCluster`，见 [`flowchart::place_node`]）：**主轴**
    /// 永远固定在源边界外 [`FLOWCHART_GAP`]（不随邻居外推），**交叉轴**在该列带内把
    /// 新节点滑到离源中心最近的空位——同向已有下一节点时自动错开成**分叉**，不再与
    /// 邻居或其后续节点重叠。障碍集取与源同连通子图的节点（[`Self::connected_flowchart_rects`]）。
    pub(crate) fn add_connected_shape(&mut self, dir: FlowDir) {
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
                } if !matches!(shape_type, ShapeType::Polyline | ShapeType::Elbow) => {
                    (base_size.0, base_size.1, *stroke)
                }
                _ => return,
            },
            None => return,
        };
        let src_rect = match self.scene.get_item(&src_id) {
            Some(item) => item.bounding_rect(),
            None => return,
        };
        // 障碍 = 与源同连通子图的其它节点包围盒；主轴恒相对源、交叉轴滑到最近空位。
        let obstacles = self.connected_flowchart_rects(src_id);
        let cdir = match dir {
            FlowDir::Right => flowchart::FlowDir::Right,
            FlowDir::Left => flowchart::FlowDir::Left,
            FlowDir::Up => flowchart::FlowDir::Up,
            FlowDir::Down => flowchart::FlowDir::Down,
        };
        let target =
            flowchart::place_node(src_rect, cdir, FLOWCHART_GAP, FLOWCHART_GAP, &obstacles);
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
        // 流程图连接箭头默认走 elbow 连接器（plan #24 独立类型）：分叉时自动正交
        // 路由，比斜线更接近 Excalidraw elbow arrow 观感；两端共线时退化为直线。
        let min = CanvasPoint::new(s.x.min(d.x), s.y.min(d.y));
        let mut arrow = Item::new_elbow(
            vec![(s.x - min.x, s.y - min.y), (d.x - min.x, d.y - min.y)],
            ((d.x - s.x).abs(), (d.y - s.y).abs()),
            None,
            Some(ArrowHeadStyle::Arrow),
            min.x,
            min.y,
            stroke,
        )
        // 默认风格总开关：克隆分叉的箭头与其他新建元素保持同档手绘风。
        .with_sloppiness(self.default_sloppiness);
        if let ItemKind::Shape {
            start_binding,
            end_binding,
            elbow_axis,
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
            // 创建即锁定取向（plan #21 DP-B）：锚点在源节点 dir 侧边缘，连线沿
            // 该边法线进出；后续拖动节点无论 Δ 关系如何翻转走向不变。
            *elbow_axis = Some(elbow_axis_from_anchor((w, h), src_anchor));
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
    pub(crate) fn find_connected_neighbor(
        &self,
        id: ItemId,
        dir: FlowDir,
    ) -> Option<(ItemId, CanvasRect)> {
        let cur_center = self.scene.get_item(&id)?.bounding_rect().center();
        let mut best: Option<(ItemId, f32, CanvasRect)> = None; // (邻居, 主轴距离, rect)
        for item in &self.scene.items {
            let ItemKind::Shape {
                shape_type: ShapeType::Polyline | ShapeType::Elbow,
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

    /// plan #7：与 `id` 处于同一**连通流程图**的所有节点包围盒（不含 `id` 自身），
    /// 沿两端绑定的 Polyline 双向 BFS（对齐 Excalidraw `getConnectedFlowchartNodes`，
    /// #8518）。供 [`add_connected_shape`] 作交叉轴避让的障碍集。
    fn connected_flowchart_rects(&self, id: ItemId) -> Vec<CanvasRect> {
        use std::collections::HashSet;
        let mut visited: HashSet<ItemId> = HashSet::from([id]);
        let mut queue: Vec<ItemId> = vec![id];
        let mut out: Vec<CanvasRect> = Vec::new();
        while let Some(cur) = queue.pop() {
            for item in &self.scene.items {
                let ItemKind::Shape {
                    shape_type: ShapeType::Polyline | ShapeType::Elbow,
                    start_binding,
                    end_binding,
                    ..
                } = &item.kind
                else {
                    continue;
                };
                // 两端都有绑定的完整连接才算一条边；自环跳过。
                let nb = match (start_binding, end_binding) {
                    (Some(b0), Some(b1)) if b0.target == cur && b1.target != cur => b1.target,
                    (Some(b0), Some(b1)) if b1.target == cur && b0.target != cur => b0.target,
                    _ => continue,
                };
                if visited.insert(nb) {
                    if let Some(rect) = self.scene.get_item(&nb).map(|i| i.bounding_rect()) {
                        out.push(rect);
                    }
                    queue.push(nb);
                }
            }
        }
        out
    }

    /// plan #7：沿连接箭头导航（`Alt+方向`）。跳选区到 `dir` 方向的直连邻居
    /// （组按 #13 点击语义整组展开）。无命令——导航不进 undo（与 Excalidraw 一致）。
    pub(crate) fn navigate_connected(&mut self, dir: FlowDir) {
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

    pub(crate) fn bring_to_front(&mut self) {
        let ids = self.reorderable_selected_ids();
        if ids.is_empty() {
            return;
        }
        let cmd = ReorderItems::new(ids, true);
        self.push_cmd(Box::new(cmd));
        self.flash(t(self.lang, T::FlashBroughtToFront).to_string());
    }

    pub(crate) fn send_to_back(&mut self) {
        let ids = self.reorderable_selected_ids();
        if ids.is_empty() {
            return;
        }
        let cmd = ReorderItems::new(ids, false);
        self.push_cmd(Box::new(cmd));
        self.flash(t(self.lang, T::FlashSentToBack).to_string());
    }

    pub(crate) fn move_forward(&mut self) {
        let ids = self.reorderable_selected_ids();
        if ids.is_empty() {
            return;
        }
        let cmd = ReorderRelative::new(ids, true);
        self.push_cmd(Box::new(cmd));
        self.flash(t(self.lang, T::FlashMovedForward).to_string());
    }

    pub(crate) fn move_backward(&mut self) {
        let ids = self.reorderable_selected_ids();
        if ids.is_empty() {
            return;
        }
        let cmd = ReorderRelative::new(ids, false);
        self.push_cmd(Box::new(cmd));
        self.flash(t(self.lang, T::FlashMovedBackward).to_string());
    }

    /// 选区中可调整 z-order 的 item ids（排除 Frame，保持恒在最底），按 z 升序
    /// 排列以保持选区内相对顺序（修复 HashSet 迭代顺序不确定导致多选置顶顺序随机化）。
    fn reorderable_selected_ids(&self) -> Vec<ItemId> {
        let mut ids: Vec<ItemId> = self
            .scene
            .selection
            .iter()
            .copied()
            .filter(|id| !self.scene.get_item(id).is_some_and(|i| i.is_frame()))
            .collect();
        ids.sort_by_key(|id| self.scene.get_item(id).map_or(0, |i| i.z));
        ids
    }

    /// 计算把 `content_rect` 适配到当前视口（90% 填充）的目标 (zoom, pan)，
    /// 与 [`ViewportState::fit_to_content`] 同公式（同样不钳制 min/max，fit 是
    /// 一次性精确赋值）。用于判断"当前是否已是该内容的适配视图"，从而支持
    /// 双击图片在"适配↔上一视图"间切换（.issues #2）。
    pub(crate) fn compute_fit(&self, content_rect: CanvasRect) -> (f32, CanvasVector) {
        let content_w = content_rect.width().max(1.0);
        let content_h = content_rect.height().max(1.0);
        let screen_w = self.viewport.screen_rect.width().max(1.0);
        let screen_h = self.viewport.screen_rect.height().max(1.0);
        let scale = (screen_w / content_w).min(screen_h / content_h) * 0.9;
        (scale, content_rect.center().to_vector())
    }

    pub(crate) fn fit_to_screen(&mut self) {
        // 「全部内容 AABB 并集」决策归 core（Scene::content_bounding_rect）；
        // app 只负责视口动作与提示。None ⟺ 空场景 → 重置视口。
        match self.scene.content_bounding_rect() {
            Some(b) => self.viewport.fit_to_content(b),
            None => self.viewport.reset(),
        }
        self.flash(t(self.lang, T::FlashFitToCanvas).to_string());
    }

    /// Shift+2：缩放视口到选中元素（Excalidraw 同款 Zoom to selection）。
    /// 无选中时仅 flash 提示，不做任何视口变更。
    pub(crate) fn zoom_to_selection(&mut self) {
        if self.scene.selection.is_empty() {
            self.flash(t(self.lang, T::FlashNoSelection).to_string());
            return;
        }
        // 「选区 AABB 并集」决策归 core（Scene::selection_bounding_rect）。
        if let Some(b) = self.scene.selection_bounding_rect() {
            self.viewport.fit_to_content(b);
            self.flash(t(self.lang, T::FlashZoomToSelection).to_string());
        }
    }

    // ─────────── Phase 5 辅助方法 ───────────

    /// 当前选中 Pixmap item 数量
    pub(crate) fn selected_pixmap_count(&self) -> usize {
        self.scene
            .selection
            .iter()
            .filter_map(|id| self.scene.get_item(id))
            .filter(|it| matches!(it.kind, ItemKind::Pixmap { .. }))
            .count()
    }

    /// 选区内 elbow 连接器数量（右键菜单「转为多段线」的显隐条件，plan #24 DP-3）。
    pub(crate) fn selected_elbow_count(&self) -> usize {
        self.scene
            .selection
            .iter()
            .filter_map(|id| self.scene.get_item(id))
            .filter(|it| {
                matches!(
                    it.kind,
                    ItemKind::Shape {
                        shape_type: ShapeType::Elbow,
                        ..
                    }
                )
            })
            .count()
    }

    /// 选区内全部 elbow 连接器烘焙为多段线（plan #24 DP-3）：每条一条
    /// `ConvertElbowToPolyline`（当前路由固化为自由顶点，视觉不变），逐条入 undo 栈。
    pub(crate) fn convert_elbows_to_polyline(&mut self) {
        let ids: Vec<ItemId> = self
            .scene
            .selection
            .iter()
            .copied()
            .filter(|id| {
                self.scene.get_item(id).is_some_and(|it| {
                    matches!(
                        it.kind,
                        ItemKind::Shape {
                            shape_type: ShapeType::Elbow,
                            ..
                        }
                    )
                })
            })
            .collect();
        let mut converted = 0usize;
        for id in ids {
            if let Some(cmd) = ConvertElbowToPolyline::build(&self.scene, id) {
                self.push_cmd(Box::new(cmd));
                converted += 1;
            }
        }
        if converted > 0 {
            self.flash(t(self.lang, T::FlashConvertedToPolyline).to_string());
        }
    }

    /// 单Pixmap grayscale 状态（用于右键菜单文案）
    pub(crate) fn selected_pixmap_grayscale(&self) -> bool {
        for id in &self.scene.selection {
            if let Some(item) = self.scene.get_item(id) {
                if let ItemKind::Pixmap { grayscale, .. } = &item.kind {
                    return *grayscale;
                }
            }
        }
        false
    }

    /// mermaid 输入弹窗（plan #9）：多行文本框 + 语法提示 + 生成/取消。
    /// 解析失败不关窗（保留输入供修改），错误经 flash 指出行号。
    pub(crate) fn render_mermaid_window(&mut self, ctx: &egui::Context) {
        let mut generate = false;
        let mut close = false;
        egui::Window::new("mermaid_input")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(480.0);
                ui.vertical_centered(|ui| {
                    ui.label(t(self.lang, T::MermaidTitle));
                });
                ui.add_space(6.0);
                let edit = egui::TextEdit::multiline(&mut self.mermaid_buf)
                    .desired_rows(6)
                    .desired_width(440.0)
                    .code_editor();
                ui.add(edit);
                ui.add_space(4.0);
                ui.small(t(self.lang, T::MermaidPlaceholder));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(t(self.lang, T::MermaidGenerate)).clicked() {
                        generate = true;
                    }
                    if ui.button(t(self.lang, T::ChartChooserCancel)).clicked() {
                        close = true;
                    }
                });
            });
        if generate {
            self.generate_mermaid_flowchart(ctx);
        } else if close {
            self.mermaid_open = false;
        }
    }

    /// 解析 mermaid 文本并生成分层布局的流程图（plan #9）：节点=矩形/椭圆/
    /// 菱形 + 绑定文字；边=两端 `EndpointBinding` 绑定的直箭头（复用 plan #7
    /// `edge_anchor_local`，锚点=双方相对的边中点）；整批 `AddItems` 一条
    /// undo，生成后全选。落点=视口中心（整体包围盒居中，同图片导入惯例）。
    pub(crate) fn generate_mermaid_flowchart(&mut self, ctx: &egui::Context) {
        let src = std::mem::take(&mut self.mermaid_buf);
        let fc = match parse_mermaid_flowchart(&src) {
            Ok(fc) => fc,
            Err(e) => {
                // 保留输入供修改
                self.mermaid_buf = src;
                self.flash(fill(t(self.lang, T::FlashMermaidParseFailed), &[e]));
                return;
            }
        };
        let layout = layout_flowchart(&fc);
        // 布局包围盒 → 整体居中到视口中心
        let (mut bx0, mut by0, mut bx1, mut by1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for ln in &layout {
            bx0 = bx0.min(ln.x);
            by0 = by0.min(ln.y);
            bx1 = bx1.max(ln.x + ln.w);
            by1 = by1.max(ln.y + ln.h);
        }
        let center = self
            .viewport
            .screen_to_canvas(self.viewport.screen_rect.center());
        let (off_x, off_y) = (center.x - (bx0 + bx1) / 2.0, center.y - (by0 + by1) / 2.0);

        let stroke = self.default_stroke;
        let mut added: Vec<Item> = Vec::with_capacity(fc.nodes.len() * 2 + fc.edges.len());
        let mut node_ids: Vec<ItemId> = Vec::with_capacity(fc.nodes.len());
        let mut node_rects: Vec<CanvasRect> = Vec::with_capacity(fc.nodes.len());
        for ln in &layout {
            let node = &fc.nodes[ln.index];
            let shape_type = match node.shape {
                MermaidShape::Rectangle => ShapeType::Rectangle,
                MermaidShape::Ellipse => ShapeType::Ellipse,
                MermaidShape::Diamond => ShapeType::Diamond,
            };
            let x = ln.x + off_x;
            let y = ln.y + off_y;
            let shape = Item::new_shape(shape_type, (ln.w, ln.h), x, y, stroke, None)
                // 默认风格总开关：mermaid 导入节点与其他新建形状同档手绘风。
                .with_sloppiness(self.default_sloppiness);
            let shape_id = shape.id;
            node_ids.push(shape_id);
            node_rects.push(CanvasRect::new(
                CanvasPoint::new(x, y),
                CanvasSize::new(ln.w, ln.h),
            ));
            added.push(shape);
            // 绑定文字：锚定容器中心（随容器联动，Phase C 模型）
            if !node.label.is_empty() {
                let txt = Item::new_text_in(
                    node.label.clone(),
                    x + ln.w / 2.0,
                    y + ln.h / 2.0,
                    18.0,
                    stroke.color,
                    shape_id,
                )
                // 默认风格总开关：导入文字随预设用手写体/黑体。
                .with_font_family(self.default_font_family);
                added.push(txt);
            }
        }
        for e in &fc.edges {
            let from = node_rects[e.from];
            let to = node_rects[e.to];
            // 箭头族 → (start/end 箭头头, 线型 dash, 宽度倍数)（plan #17）。
            let (start_arrow, end_arrow, dash, width_mult) = match e.kind {
                MermaidArrow::Arrow => (None, Some(ArrowHeadStyle::Arrow), DashStyle::Solid, 1.0),
                MermaidArrow::Open => (None, None, DashStyle::Solid, 1.0),
                MermaidArrow::Dotted => (None, Some(ArrowHeadStyle::Arrow), DashStyle::Dashed, 1.0),
                MermaidArrow::DottedOpen => (None, None, DashStyle::Dashed, 1.0),
                MermaidArrow::Thick => (None, Some(ArrowHeadStyle::Arrow), DashStyle::Solid, 2.0),
                MermaidArrow::ThickOpen => (None, None, DashStyle::Solid, 2.0),
                MermaidArrow::Double => (
                    Some(ArrowHeadStyle::Arrow),
                    Some(ArrowHeadStyle::Arrow),
                    DashStyle::Solid,
                    1.0,
                ),
            };
            let mut edge_stroke = stroke;
            edge_stroke.dash = dash;
            edge_stroke.width = stroke.width * width_mult;
            // 主轴方向取中心差的主导轴（与分层布局方向一致）
            let dx = to.center().x - from.center().x;
            let dy = to.center().y - from.center().y;
            let dir = if dx.abs() >= dy.abs() {
                if dx >= 0.0 {
                    FlowDir::Right
                } else {
                    FlowDir::Left
                }
            } else if dy >= 0.0 {
                FlowDir::Down
            } else {
                FlowDir::Up
            };
            let src_anchor = edge_anchor_local(dir, from.width(), from.height());
            let dst_anchor = edge_anchor_local(dir.opposite(), to.width(), to.height());
            let s = CanvasPoint::new(from.min().x + src_anchor.0, from.min().y + src_anchor.1);
            let d = CanvasPoint::new(to.min().x + dst_anchor.0, to.min().y + dst_anchor.1);
            let min = CanvasPoint::new(s.x.min(d.x), s.y.min(d.y));
            let mut arrow = Item::new_polyline(
                vec![(s.x - min.x, s.y - min.y), (d.x - min.x, d.y - min.y)],
                ((d.x - s.x).abs(), (d.y - s.y).abs()),
                start_arrow,
                end_arrow,
                false,
                min.x,
                min.y,
                edge_stroke,
            )
            // 默认风格总开关：mermaid 导入边与其他新建元素同档手绘风。
            .with_sloppiness(self.default_sloppiness);
            if let ItemKind::Shape {
                start_binding,
                end_binding,
                label,
                ..
            } = &mut arrow.kind
            {
                *start_binding = Some(EndpointBinding {
                    target: node_ids[e.from],
                    anchor: Some(src_anchor),
                });
                *end_binding = Some(EndpointBinding {
                    target: node_ids[e.to],
                    anchor: Some(dst_anchor),
                });
                // 边标签（plan #17 DP2）：存于线性对象自身，渲染期画在曲线中点。
                *label = e.label.clone().filter(|s| !s.is_empty());
            }
            added.push(arrow);
        }

        let node_count = fc.nodes.len();
        let edge_count = fc.edges.len();
        for it in &added {
            self.scene.add_item(it.clone());
        }
        self.scene.deselect_all();
        for id in &node_ids {
            self.scene.select(*id);
        }
        self.push_cmd(Box::new(AddItems::new(added).with_preview_applied(true)));
        self.mermaid_open = false;
        self.flash(fill(
            t(self.lang, T::FlashMermaidCreated),
            &[node_count.to_string(), edge_count.to_string()],
        ));
        ctx.request_repaint();
    }

    /// 切换选中 Pixmap item 的灰度标志（spec §2.2 灰度）
    pub(crate) fn toggle_grayscale_selected(&mut self) {
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
    pub(crate) fn enter_crop_mode(&mut self) {
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
    pub(crate) fn apply_crop(&mut self) {
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
    pub(crate) fn cancel_crop(&mut self) {
        self.crop_mode = None;
        self.flash(t(self.lang, T::FlashCropCancelled).to_string());
    }

    /// 检测鼠标是否命中裁剪手柄（4 个角点）
    /// 命中裁剪手柄：手柄位于旋转后的裁剪框 4 角，跟随 item 一起旋转。
    pub(crate) fn crop_handle_hit_test(&self, screen_pos: egui::Pos2) -> Option<CropHandle> {
        let crop_state = self.crop_mode.as_ref()?;
        let item = self.scene.get_item(&crop_state.item_id)?;
        // crop_corners 走完整 local_to_canvas（含旋转），手柄因此跟随图片一起转
        let crop_quad = item
            .crop_corners(crop_state.rect)?
            .map(|p| self.viewport.canvas_to_pos2(p));
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
    pub(crate) fn update_crop_drag(&mut self, screen_pos: egui::Pos2, handle: CropHandle) {
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
        let canvas_pos = self.viewport.pos2_to_canvas(screen_pos);
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
    pub(crate) fn pick_color_at(&mut self, screen_pos: egui::Pos2) {
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
        let canvas_pos = self.viewport.pos2_to_canvas(screen_pos);
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
    pub(crate) fn arrange_selected(&mut self, mode: ArrangeMode) {
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
    pub(crate) fn align_selected(&mut self, mode: AlignMode) {
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
    pub(crate) fn distribute_selected(&mut self, axis: DistributeAxis, mode: DistributeMode) {
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
    pub(crate) fn normalize_selected(&mut self, mode: preferz_core::commands::NormalizeMode) {
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
}
