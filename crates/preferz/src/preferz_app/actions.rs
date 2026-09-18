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
    /// 选区带容器联动（封闭形状的绑定文本、画框成员），副本偏移 10px 画布避免完全重叠；
    /// 副本已经入场景，push `AddItems(preview_applied=true)`，一次 undo 撤掉整个复制。
    pub(crate) fn duplicate_in_place(&mut self) {
        let selected: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
        if selected.is_empty() {
            return;
        }
        // 容器联动：与拖拽一致
        let mut collected = selected.clone();
        for sid in &selected {
            collected.extend(self.scene.texts_bound_to(*sid));
            if self
                .scene
                .get_item(sid)
                .map(|it| it.is_frame())
                .unwrap_or(false)
            {
                collected.extend(self.scene.frame_members(*sid));
            }
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

    /// plan #7：流程图节点创建（`Ctrl+方向`，单选矩形/椭圆/菱形时）。沿该向
    /// 克隆一个**同源同风格**节点（主轴 = 源边界 + [`FLOWCHART_GAP`]，交叉轴
    /// 中心对齐——同尺寸克隆时偏移天然实现；不复制绑定文字），并连一条两端
    /// 绑定的**直箭头**（本仓库无 elbow）：anchor=各自朝向对方的边中点、初始
    /// 端点即该锚点画布位置，与 `resolve_bindings` 重算结果一致，后续移动形状
    /// 箭头自动跟随。按下即提交：新节点+箭头 `AddItems` 一条 undo，选区跳新
    /// 节点（同方向连按自然接链）。前提不满足静默（对齐 Excalidraw）。
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
                } if !matches!(shape_type, ShapeType::Polyline) => {
                    (base_size.0, base_size.1, *stroke)
                }
                _ => return,
            },
            None => return,
        };
        // 验收反馈 #7-2：该方向已有直连同级邻居时，新节点放到**邻居旁边**
        // （主轴=邻居远边 + GAP、交叉轴对齐邻居中心），而不是与原选中的源节点
        // 完全重合；无邻居则以源为基准（原行为）。箭头仍 源→新节点。
        let src_rect = match self.scene.get_item(&src_id) {
            Some(item) => item.bounding_rect(),
            None => return,
        };
        let (sw, sh) = (src_rect.width(), src_rect.height());
        let base = self
            .find_connected_neighbor(src_id, dir)
            .map(|(_, rect)| rect)
            .unwrap_or(src_rect);
        let target = match dir {
            FlowDir::Right => {
                CanvasPoint::new(base.max().x + FLOWCHART_GAP, base.center().y - sh / 2.0)
            }
            FlowDir::Left => CanvasPoint::new(
                base.min().x - FLOWCHART_GAP - sw,
                base.center().y - sh / 2.0,
            ),
            FlowDir::Down => {
                CanvasPoint::new(base.center().x - sw / 2.0, base.max().y + FLOWCHART_GAP)
            }
            FlowDir::Up => CanvasPoint::new(
                base.center().x - sw / 2.0,
                base.min().y - FLOWCHART_GAP - sh,
            ),
        };
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
        let min = CanvasPoint::new(s.x.min(d.x), s.y.min(d.y));
        let mut arrow = Item::new_polyline(
            vec![(s.x - min.x, s.y - min.y), (d.x - min.x, d.y - min.y)],
            ((d.x - s.x).abs(), (d.y - s.y).abs()),
            None,
            Some(ArrowHeadStyle::Arrow),
            false,
            min.x,
            min.y,
            stroke,
        );
        if let ItemKind::Shape {
            start_binding,
            end_binding,
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
                shape_type: ShapeType::Polyline,
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
        let ids: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
        if ids.is_empty() {
            return;
        }
        // ReorderItems 命令（修 S3/W8），不再直接z
        let cmd = ReorderItems::new(ids, true);
        self.push_cmd(Box::new(cmd));
        self.flash(t(self.lang, T::FlashBroughtToFront).to_string());
    }

    pub(crate) fn send_to_back(&mut self) {
        let ids: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
        if ids.is_empty() {
            return;
        }
        let cmd = ReorderItems::new(ids, false);
        self.push_cmd(Box::new(cmd));
        self.flash(t(self.lang, T::FlashSentToBack).to_string());
    }

    /// 计算把 `content_rect` 适配到当前视口（90% 填充）的目标 (zoom, pan)，
    /// 与 [`ViewportState::fit_to_content`] 同公式。用于判断"当前是否已是该内容
    /// 的适配视图"，从而支持双击图片在"适配↔上一视图"间切换（.issues #2）。
    pub(crate) fn compute_fit(&self, content_rect: CanvasRect) -> (f32, CanvasVector) {
        let content_w = content_rect.width().max(1.0);
        let content_h = content_rect.height().max(1.0);
        let screen_w = self.viewport.screen_rect.width().max(1.0);
        let screen_h = self.viewport.screen_rect.height().max(1.0);
        let scale = (screen_w / content_w).min(screen_h / content_h) * 0.9;
        let scale = scale.clamp(self.viewport.min_zoom, self.viewport.max_zoom);
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
