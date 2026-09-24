//! drag — 从 preferz_app 拆出的方法段（架构整固 Step 3c-ii）。逐字搬迁，`use super::*;` 够到 mod.rs 词汇/私有项。
use super::*;

// ─────────────────────────── 拖拽逻辑 ───────────────────────────

impl PReferZApp {
    /// plan #4：Alt+单击删除顶点。守卫：开放折线保 ≥2、闭合多边形保 ≥3 顶点
    /// （再删退化到无法成形），不满足时 flash 拒绝。删首/尾顶点连带解除该端
    /// 绑定（中间顶点不涉绑定）。预览直改 + `EditShapePoints`（恒 skip_first_redo）
    /// 入 undo 栈，一步可撤。
    pub(crate) fn try_delete_vertex(&mut self, item_id: ItemId, idx: usize) {
        let (old_points, closed, so, eo) = match self.scene.get_item(&item_id) {
            Some(item) => match &item.kind {
                ItemKind::Shape {
                    points,
                    closed,
                    start_binding,
                    end_binding,
                    ..
                } => (points.clone(), *closed, *start_binding, *end_binding),
                _ => return,
            },
            None => return,
        };
        let n = old_points.len();
        let min = if closed { 3 } else { 2 };
        if n <= min {
            self.flash(if closed {
                t(self.lang, T::FlashVertexMinClosed)
            } else {
                t(self.lang, T::FlashVertexMinOpen)
            });
            return;
        }
        let last = n - 1;
        let binding_touched = idx == 0 || idx == last;
        let mut new_points = old_points.clone();
        new_points.remove(idx);
        let sn = if idx == 0 { None } else { so };
        let en = if idx == last { None } else { eo };
        // 预览直改（命令 skip_first_redo 恒 true，push 不再重放 redo）
        if let Some(item) = self.scene.get_item_mut(&item_id) {
            if let ItemKind::Shape {
                start_binding,
                end_binding,
                ..
            } = &mut item.kind
            {
                if idx == 0 {
                    *start_binding = None;
                }
                if idx == last {
                    *end_binding = None;
                }
            }
            item.kind.set_line_points(new_points.clone());
        }
        let mut cmd = EditShapePoints::new(item_id, old_points, new_points);
        if binding_touched {
            cmd = cmd.with_binding_change(so, sn, eo, en);
        }
        self.push_cmd(Box::new(cmd));
        self.flash(t(self.lang, T::FlashVertexDeleted));
    }

    pub(crate) fn begin_drag(
        &mut self,
        screen_pos: egui::Pos2,
        additive: bool,
        free_scale: bool,
        alt: bool,
        double_click: bool,
    ) {
        // 文本编辑中不启动拖拽
        if self.editing_text.is_some() {
            return;
        }
        // 画框编号编辑中不启动拖拽（点击别处由编辑窗 lost_focus 提交/取消）
        if self.editing_frame_number.is_some() {
            return;
        }

        // 绘制工具激活：直接进入创建拖拽（不处理手柄/命中/框选）。
        // additive（=Shift 按住）用于正方形锁定，free_scale（=Ctrl 按住）用于椭圆解锁。
        match self.tool {
            Tool::Shape(shape_type) => {
                let start_canvas = self.viewport.pos2_to_canvas(screen_pos);
                self.drag = DragState::CreatingShape {
                    shape_type,
                    end_arrow: None,
                    start: start_canvas,
                    current: start_canvas,
                    shift: additive,
                    ctrl: free_scale,
                };
                return;
            }
            Tool::Linear { end_arrow } => {
                let start_canvas = self.viewport.pos2_to_canvas(screen_pos);
                self.drag = DragState::CreatingShape {
                    shape_type: ShapeType::Polyline,
                    end_arrow,
                    start: start_canvas,
                    current: start_canvas,
                    shift: additive,
                    ctrl: free_scale,
                };
                return;
            }
            Tool::Frame => {
                let start_canvas = self.viewport.pos2_to_canvas(screen_pos);
                self.drag = DragState::CreatingFrame {
                    start: start_canvas,
                    current: start_canvas,
                    shift: additive,
                };
                return;
            }
            // 多拍工具：第一拍落首个顶点，后续每拍追加一个顶点（见 finish_create_polygon）
            Tool::Polygon => {
                let p = self.viewport.pos2_to_canvas(screen_pos);
                match &mut self.drag {
                    DragState::CreatingPolygon {
                        points,
                        current,
                        shift,
                    } => {
                        let last = *points.last().expect("CreatingPolygon 至少有一个顶点");
                        let next = snap::snap_polygon_point(last, *current, *shift);
                        // 双击的第二下会先落到 begin_drag：落点与上一顶点重合时
                        // 视为"收尾"而非新增顶点（双击闭合的第二拍不该多加一个点）
                        if (next - last).length() >= POLYLINE_CLOSE_DISTANCE {
                            points.push(next);
                        }
                        *current = next;
                        // 每拍重取 Shift：下一次按下时才改方向锁定，中途松开 Shift 不影响已落顶点
                        *shift = additive;
                    }
                    _ => {
                        self.drag = DragState::CreatingPolygon {
                            points: vec![p],
                            current: p,
                            shift: additive,
                        };
                    }
                }
                return;
            }
            // 徒手绘制（plan #10）：按下起笔，采集首个画布点，随后由
            // update_drag_preview 连续追点、end_drag 定型成墨迹。
            Tool::Freehand => {
                let p = self.viewport.pos2_to_canvas(screen_pos);
                self.drag = DragState::Drawing { raw: vec![p] };
                return;
            }
            _ => {}
        }

        // 裁剪模式：优先检测裁剪手
        if self.crop_mode.is_some() {
            if let Some(h) = self.crop_handle_hit_test(screen_pos) {
                if let Some(crop) = self.crop_mode.as_mut() {
                    crop.dragging = Some(h);
                }
                return;
            }
            // 裁剪模式下点空白：不响应（避免误操作
            return;
        }

        // 颜色采样模式：单Pixmap 采样像素
        if self.color_picker_active {
            self.pick_color_at(screen_pos);
            return;
        }

        // 1) 手柄优先
        let hover = self.transform_handles.hover_handle;
        if hover != Handle::None {
            // 找到手柄所属的 item
            let selected = self.selected_items_snapshot();
            for item in selected.iter().rev() {
                let show_flip = should_show_flip(item);
                let show_rotate = should_show_rotate(item);
                let h = self.transform_handles.hit_test(
                    screen_pos,
                    item,
                    &self.viewport,
                    show_flip,
                    show_rotate,
                );
                if h != Handle::None {
                    // 线性对象顶点控制点：进入端点拖拽（预览直接改 points）。
                    // plan #4：Alt+单击内部顶点 → 按下即删；Alt+按在真实端点
                    // （0/末点）→ 延伸模式（越阈追加 / 未越阈释放=单击删除）。
                    if let Handle::Endpoint(endpoint) = h {
                        let start_points = match &item.kind {
                            ItemKind::Shape { points, .. } => points.clone(),
                            _ => Vec::new(),
                        };
                        let n = start_points.len();
                        let is_real = n > 0 && (endpoint == 0 || endpoint == n - 1);
                        if alt && !is_real {
                            self.try_delete_vertex(item.id, endpoint);
                            return;
                        }
                        let base_pos = start_points.get(endpoint).copied().unwrap_or((0.0, 0.0));
                        let start_canvas = self.viewport.pos2_to_canvas(screen_pos);
                        self.drag = DragState::LineEndpoint {
                            item_id: item.id,
                            endpoint,
                            start_canvas,
                            start_points,
                            base_pos,
                            alt_extend: alt && is_real,
                            keep_inserted: false,
                        };
                        self.transform_handles.active_handle = h;
                        self.transform_handles.is_dragging = true;
                        return;
                    }
                    // 线性对象段中点：在段中间插入顶点（预览），随即进入端点拖拽。
                    // start_points 保存插入前的点集，undo 一步即可移除新顶点。
                    // 多顶点 elbow（plan #21 DP-A）：仅**双击**在候选点插入；
                    // 单击直接吞掉（手柄只是双击目标提示，不进入任何拖拽）。
                    if let Handle::SegmentMid(seg) = h {
                        if is_multi_vertex_elbow_line(item) {
                            if double_click {
                                let prep = match &item.kind {
                                    ItemKind::Shape { points, closed, .. } => {
                                        elbow_insert_candidates(points, *closed)
                                            .into_iter()
                                            .find(|(s, _)| *s == seg)
                                            .map(|(s, mid)| {
                                                let insert_idx = if s + 1 < points.len() {
                                                    s + 1
                                                } else {
                                                    points.len()
                                                };
                                                (points.clone(), insert_idx, mid)
                                            })
                                    }
                                    _ => None,
                                };
                                if let Some((start_points, insert_idx, mid)) = prep {
                                    if let Some(it) = self.scene.get_item_mut(&item.id) {
                                        if let ItemKind::Shape { points, .. } = &mut it.kind {
                                            points.insert(insert_idx, mid);
                                        }
                                    }
                                    let start_canvas = self.viewport.pos2_to_canvas(screen_pos);
                                    self.drag = DragState::LineEndpoint {
                                        item_id: item.id,
                                        endpoint: insert_idx,
                                        start_canvas,
                                        start_points,
                                        base_pos: mid,
                                        alt_extend: false,
                                        keep_inserted: true,
                                    };
                                    self.transform_handles.active_handle = h;
                                    self.transform_handles.is_dragging = true;
                                }
                            }
                            return;
                        }
                        let prep = match &item.kind {
                            ItemKind::Shape { points, .. } => {
                                let n = points.len();
                                if n < 2 {
                                    None
                                } else {
                                    let a = points[seg];
                                    let b = points[(seg + 1) % n];
                                    let mid = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
                                    let insert_idx = if seg + 1 < n { seg + 1 } else { n };
                                    Some((points.clone(), insert_idx, mid))
                                }
                            }
                            _ => None,
                        };
                        if let Some((start_points, insert_idx, mid)) = prep {
                            if let Some(it) = self.scene.get_item_mut(&item.id) {
                                if let ItemKind::Shape { points, .. } = &mut it.kind {
                                    points.insert(insert_idx, mid);
                                }
                            }
                            let start_canvas = self.viewport.pos2_to_canvas(screen_pos);
                            self.drag = DragState::LineEndpoint {
                                item_id: item.id,
                                endpoint: insert_idx,
                                start_canvas,
                                start_points,
                                base_pos: mid,
                                alt_extend: false,
                                keep_inserted: false,
                            };
                            self.transform_handles.active_handle = h;
                            self.transform_handles.is_dragging = true;
                        }
                        return;
                    }
                    // elbow bar：进入 bar 拖拽（预览直接改 elbow_mid_offset）。
                    if let Handle::ElbowBar = h {
                        let start_offset = match &item.kind {
                            ItemKind::Shape {
                                elbow_mid_offset, ..
                            } => *elbow_mid_offset,
                            _ => 0.0,
                        };
                        let start_canvas = self.viewport.pos2_to_canvas(screen_pos);
                        self.drag = DragState::ElbowBar {
                            item_id: item.id,
                            start_canvas,
                            start_offset,
                        };
                        self.transform_handles.active_handle = h;
                        self.transform_handles.is_dragging = true;
                        return;
                    }
                    // 翻转边手柄：点击即触发翻转，不进入拖拽（spec L239「翻转边」）
                    if h == Handle::FlipH || h == Handle::FlipV {
                        let ids: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
                        let horizontal = h == Handle::FlipH;
                        let cmd = FlipItems::new(ids, horizontal);
                        self.push_cmd(Box::new(cmd));
                        self.flash(t(
                            self.lang,
                            if horizontal {
                                T::FlashFlipH
                            } else {
                                T::FlashFlipV
                            },
                        ));
                        return;
                    }
                    let start_corners = item.canvas_corners();
                    self.drag = DragState::HandleTransform {
                        item_id: item.id,
                        handle: h,
                        start_screen: screen_pos,
                        start_transform: item.transform,
                        start_corners,
                    };
                    self.transform_handles.active_handle = h;
                    self.transform_handles.is_dragging = true;
                    return;
                }
            }
        }

        // 2) 命中 item：选中并开始移动拖
        if let Some(item) = interaction::get_item_at(screen_pos, &self.scene, &self.viewport) {
            let id = item.id;
            // G2（plan #13）：命中组员 → 命中集扩展为整组（未编组项 = 自身，行为不变）
            let hit_ids: Vec<ItemId> = self.scene.expand_to_groups(&[id]);
            let hit_fully_selected = hit_ids.iter().all(|hid| self.scene.selection.contains(hid));

            let mut pending_deselect = None;
            if additive {
                if hit_fully_selected {
                    // Shift 按在**已选中**的项（组则整组在选区）上：既可能是"Shift+点击
                    // 取消选中"，也可能是"Shift+拖动做轴约束移动"——两者要到释放时才能
                    // 区分，这里先记下代表成员交给 end_drag 判定（取消时扩展回整组）。
                    // 若在此直接 toggle，Shift+拖动会先取消选中，轴约束移动就永远触发不到。
                    pending_deselect = hit_ids.first().copied();
                } else {
                    for hid in &hit_ids {
                        self.scene.toggle_selection(*hid);
                    }
                }
            } else if !hit_fully_selected {
                // 非加选且命中集未完全选中：替换选中为整组
                self.scene.deselect_all();
                for hid in &hit_ids {
                    self.scene.select(*hid);
                }
            }
            // 收集所有选中 item transform 快照
            let selected: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
            // 容器联动：选中封闭形状时连带其绑定文本（Phase C）；选中画框时连带其成员（Phase D）。
            let mut collected: Vec<ItemId> = selected.clone();
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

            // Ctrl+拖动 = 复制移动（plan #11）：先建副本、选区切到副本，原件留原地。
            // 取"按下即建副本"（plan 倾向），避免释放时才复制造成落点跳变。
            let duplicate_ids = if free_scale && !collected.is_empty() {
                let dups = self.scene.duplicate_items(&collected, CanvasVector::zero());
                let ids: Vec<ItemId> = dups.iter().map(|it| it.id).collect();
                for dup in dups {
                    self.scene.add_item(dup);
                }
                // 复制的画框须压到成员之下（与 finish_create_frame 一致）：否则它 z
                // 最高会盖住框内成员命中（frame_border_hit 已修，但近边框仍会误选
                // frame）。直接 mutate 不入 undo——最终状态由下方 AddItems 记录。
                let frame_dup_ids: Vec<ItemId> = ids
                    .iter()
                    .filter(|id| {
                        self.scene
                            .get_item(id)
                            .map(|it| it.is_frame())
                            .unwrap_or(false)
                    })
                    .copied()
                    .collect();
                if !frame_dup_ids.is_empty() {
                    ReorderItems::new(frame_dup_ids, false).redo(&mut self.scene);
                }
                self.scene.deselect_all();
                for nid in &ids {
                    self.scene.select(*nid);
                }
                Some(ids)
            } else {
                None
            };
            // 复制前的原选区（Ctrl 未移动时需要回滚）
            let original_ids = collected.clone();

            // 被拖动的是副本（复制拖动）还是原选中集；两者都从 scene 现取 transform，
            // 保证与上面可能发生的改选保持一致。
            let drag_ids = duplicate_ids.clone().unwrap_or(collected);
            let start_transforms: Vec<(ItemId, preferz_core::Transform)> = drag_ids
                .into_iter()
                .filter_map(|sid| self.scene.get_item(&sid).map(|it| (sid, it.transform)))
                .collect();
            let start_canvas = self.viewport.pos2_to_canvas(screen_pos);
            // plan #14：移动组内的 Polyline 快照（预览吸附 + 释放打包入栈）
            let line_snaps: Vec<LineSnapState> = start_transforms
                .iter()
                .filter_map(|(id, _)| {
                    let it = self.scene.get_item(id)?;
                    match &it.kind {
                        ItemKind::Shape {
                            shape_type: ShapeType::Polyline,
                            points,
                            start_binding,
                            end_binding,
                            ..
                        } if points.len() > 1 => Some(LineSnapState {
                            line_id: *id,
                            start_points: points.clone(),
                            start_start_binding: *start_binding,
                            start_end_binding: *end_binding,
                        }),
                        _ => None,
                    }
                })
                .collect();
            self.drag = DragState::MoveItems {
                start_canvas,
                start_transforms,
                line_snaps,
                duplicate_ids,
                original_ids,
                pending_deselect,
            };
            return;
        }

        // 3) 空白：开始框选（spec L240）。Shift = 加选模
        let start_canvas = self.viewport.pos2_to_canvas(screen_pos);
        self.drag = DragState::BoxSelect {
            start_canvas,
            current_canvas: start_canvas,
            additive,
        };
    }

    pub(crate) fn update_drag_preview(
        &mut self,
        screen_pos: egui::Pos2,
        free_scale: bool,
        axis_lock: bool,
    ) {
        // 裁剪模式拖拽：直接更crop_mode.rect，不进入 DragState
        if let Some(crop) = self.crop_mode.as_mut() {
            if let Some(handle) = crop.dragging {
                self.update_crop_drag(screen_pos, handle);
                return;
            }
        }

        // plan #4：Alt+拖端点"延伸"的延迟启动——越过移动阈值前按普通端点拖拽
        // 处理；越阈瞬间在该端外侧插入一个复制顶点（原端点变成中间顶点），随后
        // 拖的是新点。是否已插入用 start_points 与当前点集的数量差判定，不引入
        // 额外交互状态；中途松开 Alt 不撤销延伸（保持简洁，与 Excalidraw 的
        // uncommitted 点丢弃语义不同）。
        {
            let mut armed: Option<(ItemId, usize, usize, (f32, f32))> = None; // (线, 端点, 原点数, base_pos)
            if let DragState::LineEndpoint {
                item_id,
                endpoint,
                start_canvas,
                start_points,
                base_pos,
                alt_extend: true,
                ..
            } = &self.drag
            {
                let cur = self.viewport.pos2_to_canvas(screen_pos);
                let moved =
                    (cur - *start_canvas).length() >= EXTEND_TRIGGER_PX / self.viewport.zoom;
                if moved {
                    let not_appended_yet = self.scene.get_item(item_id).is_some_and(|it| {
                        !matches!(&it.kind, ItemKind::Shape { points, .. }
                            if points.len() > start_points.len())
                    });
                    if not_appended_yet {
                        armed = Some((*item_id, *endpoint, start_points.len(), *base_pos));
                    }
                }
            }
            if let Some((item_id, endpoint, n_old, base_pos)) = armed {
                let prepend = endpoint == 0;
                if let Some(it) = self.scene.get_item_mut(&item_id) {
                    if let ItemKind::Shape { points, .. } = &mut it.kind {
                        if prepend {
                            points.insert(0, base_pos);
                        } else {
                            points.push(base_pos);
                        }
                    }
                }
                if !prepend {
                    // 末点侧：被拖下标改到新插入的复制点（= 原点数）；0 侧仍拖 index 0
                    if let DragState::LineEndpoint { endpoint: e, .. } = &mut self.drag {
                        *e = n_old;
                    }
                }
            }
        }

        match &self.drag {
            DragState::HandleTransform {
                item_id,
                handle,
                start_screen,
                start_transform,
                start_corners,
            } => {
                let item_id = *item_id;
                let handle = *handle;
                let start_screen = *start_screen;
                let start_transform = *start_transform;
                let start_corners = *start_corners;
                let mouse_canvas = self.viewport.pos2_to_canvas(screen_pos);
                if let Some(item) = self.scene.get_item_mut(&item_id) {
                    match handle {
                        Handle::Rotate => {
                            apply_rotate_drag(
                                &self.viewport,
                                item,
                                start_transform,
                                start_corners,
                                start_screen,
                                screen_pos,
                            );
                        }
                        Handle::ResizeTopLeft
                        | Handle::ResizeTopRight
                        | Handle::ResizeBottomLeft
                        | Handle::ResizeBottomRight => {
                            apply_scale_drag(
                                item,
                                handle,
                                start_transform,
                                start_corners,
                                mouse_canvas,
                                free_scale,
                            );
                        }
                        // 翻转手柄begin_drag 中已即时处理，不会进入拖拽预
                        Handle::FlipH | Handle::FlipV | Handle::None => {}
                        // 线类顶点/段中点begin_drag 中已进入 LineEndpoint 拖拽，不会到达这里
                        Handle::Endpoint(_) | Handle::SegmentMid(_) => {}
                        // elbow barbegin_drag 中已进入 ElbowBar 拖拽，不会到达这里
                        Handle::ElbowBar => {}
                    }
                }
                // plan #5：缩放/旋转形状时，实时联动重算绑定到它的线端点
                self.scene.resolve_bindings(&[item_id]);
            }
            DragState::MoveItems {
                start_canvas,
                start_transforms,
                line_snaps,
                ..
            } => {
                let current_canvas = self.viewport.pos2_to_canvas(screen_pos);
                let mut delta = current_canvas - *start_canvas;
                if axis_lock {
                    // Shift 轴约束：只保留位移较大的那一轴，另一轴清零
                    // （PowerPoint / Excalidraw 同款）。视口只有平移与缩放、无旋转，
                    // 画布轴与屏幕轴同向，故在画布空间取轴即可。
                    if delta.x.abs() >= delta.y.abs() {
                        delta.y = 0.0;
                    } else {
                        delta.x = 0.0;
                    }
                }
                for (id, start_tf) in start_transforms {
                    if let Some(item) = self.scene.get_item_mut(id) {
                        item.transform.pos = start_tf.pos + delta;
                    }
                }
                let ids: Vec<ItemId> = start_transforms.iter().map(|(id, _)| *id).collect();
                // plan #14：移动整条线时端点吸附 + 建立绑定。命中 → 端点贴边并
                // 绑定（锚点 = 贴合点在目标局部系坐标）；未命中 → 端点随线自由
                // 移动并解绑（Excalidraw 同款）。吸附目标排除移动组自身。
                let moved: std::collections::HashSet<ItemId> = ids.iter().copied().collect();
                let threshold = SNAP_THRESHOLD_PX / self.viewport.zoom;
                self.snap_highlight = None;
                for ls in line_snaps {
                    let last = ls.start_points.len().saturating_sub(1);
                    for endpoint in [0usize, last] {
                        // 端点当前画布位置（points 尚为初始值，transform 已随组移动）
                        let qc = self
                            .scene
                            .get_item(&ls.line_id)
                            .map(|it| it.local_point_to_canvas(ls.start_points[endpoint]));
                        let hit = qc.and_then(|qc| {
                            self.scene
                                .find_snap_target(&ls.line_id, qc, threshold)
                                .filter(|(bid, _, _)| !moved.contains(bid))
                        });
                        match hit {
                            Some((bid, sc, _)) => {
                                // 先不可变换算（锚点 + 线局部新坐标），再可变写回
                                let anchor = self
                                    .scene
                                    .get_item(&bid)
                                    .and_then(|t| t.canvas_to_local_point(sc));
                                let new_local = self
                                    .scene
                                    .get_item(&ls.line_id)
                                    .and_then(|it| it.canvas_to_local_point(sc));
                                if let (Some(new_local), Some(item)) =
                                    (new_local, self.scene.get_item_mut(&ls.line_id))
                                {
                                    if let ItemKind::Shape {
                                        points,
                                        start_binding,
                                        end_binding,
                                        ..
                                    } = &mut item.kind
                                    {
                                        points[endpoint] = new_local;
                                        let binding = anchor.map(|a| EndpointBinding {
                                            target: bid,
                                            anchor: Some(a),
                                        });
                                        if endpoint == 0 {
                                            *start_binding = binding;
                                        } else {
                                            *end_binding = binding;
                                        }
                                    }
                                    self.snap_highlight = Some(bid);
                                }
                            }
                            None => {
                                // 端点回到随线移动的基准位（上一帧可能吸附偏移过）
                                // 并解绑该端点
                                if let Some(item) = self.scene.get_item_mut(&ls.line_id) {
                                    if let ItemKind::Shape {
                                        points,
                                        start_binding,
                                        end_binding,
                                        ..
                                    } = &mut item.kind
                                    {
                                        points[endpoint] = ls.start_points[endpoint];
                                        if endpoint == 0 {
                                            *start_binding = None;
                                        } else {
                                            *end_binding = None;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // plan #5：移动形状时，实时联动重算绑定到它的线端点
                self.scene.resolve_bindings(&ids);
            }
            DragState::CreatingShape { current, .. } => {
                let _ = current; // 由 match 后更新（需单独 &mut self.drag）
            }
            DragState::LineEndpoint {
                item_id,
                endpoint,
                start_canvas,
                start_points,
                base_pos,
                alt_extend,
                ..
            } => {
                let item_id = *item_id;
                let endpoint = *endpoint;
                let start_canvas = *start_canvas;
                let base_pos = *base_pos;
                // plan #4：Alt 延伸尚未触发（还没插入复制点）时不动原端点——
                // Alt+单击删除不该因几像素抖动被误当"微调端点"提交。
                if *alt_extend {
                    let appended = self.scene.get_item(&item_id).is_some_and(|it| {
                        matches!(&it.kind, ItemKind::Shape { points, .. }
                            if points.len() > start_points.len())
                    });
                    if !appended {
                        return;
                    }
                }
                let current_canvas = self.viewport.pos2_to_canvas(screen_pos);
                let delta_canvas = current_canvas - start_canvas;

                // plan #5：端点吸附。仅真实端点（0 / 末点）可绑定；查询点取**被拖端点**
                // 的当前画布位置（base_pos + 拖拽位移，跟随鼠标），吸附到轮廓上离光标
                // 最近的点。修复：此前误用另一端点位置做查询，导致拖拽端永远吸不上
                // （另一端不在形状附近时无命中），且另一端恰在形状上时会把被拖端吸回起点。
                let mut snap: Option<(ItemId, CanvasPoint)> = None;
                if let Some(item) = self.scene.get_item(&item_id) {
                    if let ItemKind::Shape { points, .. } = &item.kind {
                        let last = points.len().saturating_sub(1);
                        if endpoint == 0 || endpoint == last {
                            let delta_local = item
                                .local_to_canvas()
                                .inverse()
                                .map(|inv| inv.transform_vector(delta_canvas));
                            if let Some(delta_local) = delta_local {
                                let qc = item.local_point_to_canvas((
                                    base_pos.0 + delta_local.x,
                                    base_pos.1 + delta_local.y,
                                ));
                                let threshold = SNAP_THRESHOLD_PX / self.viewport.zoom;
                                if let Some((bid, sc, _)) =
                                    self.scene.find_snap_target(&item_id, qc, threshold)
                                {
                                    snap = Some((bid, sc));
                                }
                            }
                        }
                    }
                }

                // 吸附点换算：线局部坐标（直接写回 points）+ 目标局部坐标（锚点，
                // 供 resolve_bindings 按钉点重算）。全部在可变借用前算好。
                let (snap_anchor, snap_local): (Option<_>, Option<SnapHitLocal>) = match snap {
                    Some((bid, sc)) => {
                        let anchor = self
                            .scene
                            .get_item(&bid)
                            .and_then(|t| t.canvas_to_local_point(sc));
                        let local = self
                            .scene
                            .get_item(&item_id)
                            .and_then(|it| it.canvas_to_local_point(sc));
                        (Some(anchor), local.map(|l| (bid, l)))
                    }
                    None => (None, None),
                };

                if let Some(item) = self.scene.get_item_mut(&item_id) {
                    // 画布位移 → 局部位移（逆变换向量的平移部分自动抵消）
                    let delta_local = item
                        .local_to_canvas()
                        .inverse()
                        .map(|inv| inv.transform_vector(delta_canvas));
                    if let Some(delta_local) = delta_local {
                        if let ItemKind::Shape { points, .. } = &mut item.kind {
                            if let Some(p) = points.get_mut(endpoint) {
                                match snap_local {
                                    Some((bid, local)) => {
                                        *p = local;
                                        self.pending_endpoint_binding =
                                            Some((endpoint, bid, snap_anchor.flatten()));
                                        self.snap_highlight = Some(bid);
                                    }
                                    None => {
                                        *p = (
                                            base_pos.0 + delta_local.x,
                                            base_pos.1 + delta_local.y,
                                        );
                                        // 拖离形状：若该端点原已绑定，本次拖拽将解绑
                                        self.pending_endpoint_binding = None;
                                        self.snap_highlight = None;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            DragState::ElbowBar {
                item_id,
                start_canvas,
                start_offset,
            } => {
                let item_id = *item_id;
                let start_canvas = *start_canvas;
                let start_offset = *start_offset;
                let current_canvas = self.viewport.pos2_to_canvas(screen_pos);
                let delta_canvas = current_canvas - start_canvas;
                // bar 沿局部短轴偏移：elbow_polyline_offset 按 |dx|<=|dy| 选轴，
                // 拖拽位移逆变换到局部后取对应分量叠加到 start_offset。
                let new_offset = self.scene.get_item(&item_id).and_then(|item| {
                    let pts = match &item.kind {
                        ItemKind::Shape { points, .. } => points.clone(),
                        _ => return None,
                    };
                    if pts.len() != 2 {
                        return None;
                    }
                    let dx = pts[1].0 - pts[0].0;
                    let dy = pts[1].1 - pts[0].1;
                    let delta_local = item
                        .local_to_canvas()
                        .inverse()
                        .map(|inv| inv.transform_vector(delta_canvas))?;
                    let d = if dx.abs() <= dy.abs() {
                        delta_local.x
                    } else {
                        delta_local.y
                    };
                    Some(start_offset + d)
                });
                if let Some(new_offset) = new_offset {
                    if let Some(item) = self.scene.get_item_mut(&item_id) {
                        if let ItemKind::Shape {
                            elbow_mid_offset, ..
                        } = &mut item.kind
                        {
                            *elbow_mid_offset = new_offset;
                        }
                    }
                }
            }
            DragState::BoxSelect { .. } => {} // 由 match 后更新（需单独 &mut self.drag）
            DragState::CreatingFrame { current, .. } => {
                let _ = current; // 由 match 后更新（需单独 &mut self.drag）
            }
            // 多拍工具：current 由 match 后统一更新
            DragState::CreatingPolygon { .. } => {}
            // 徒手绘制：采样点由 match 后统一追加（需 &mut self.drag）
            DragState::Drawing { .. } => {}
            DragState::Idle => {}
        }
        // BoxSelect / CreatingShape 更新 current（match &self.drag 不可写，故单独 &mut）
        if let DragState::BoxSelect { current_canvas, .. } = &mut self.drag {
            *current_canvas = self.viewport.pos2_to_canvas(screen_pos);
        }
        if let DragState::CreatingShape { current, ctrl, .. } = &mut self.drag {
            *current = self.viewport.pos2_to_canvas(screen_pos);
            // 拖动中实时更新 Ctrl 状态（椭圆正圆/自由宽高比切换）
            *ctrl = free_scale;
        }
        if let DragState::CreatingFrame {
            start,
            current,
            shift,
        } = &mut self.drag
        {
            *current = self.viewport.pos2_to_canvas(screen_pos);
            // 全局比例锁定（所见即所得）：拖拽全程把 current 钳到比例上（长边跟
            // 主方向），Shift 临时解除。全局为「自由」时保持自由矩形。
            *shift = axis_lock;
            if let Some(r) = self.frame_ratio {
                if !axis_lock {
                    *current = constrain_drag_to_ratio(*start, *current, (r.0 as f32, r.1 as f32));
                }
            }
        }
        if let DragState::CreatingPolygon { current, .. } = &mut self.drag {
            *current = self.viewport.pos2_to_canvas(screen_pos);
        }
        // 徒手绘制（plan #10）：按屏幕最小间距过滤后连续追点（去抖/去重合，但保留
        // 间距→速度的信号供释放时定锥形笔宽）。
        if let DragState::Drawing { raw } = &mut self.drag {
            let p = self.viewport.pos2_to_canvas(screen_pos);
            let min_d = FREEDRAW_MIN_SPACING_PX / self.viewport.zoom;
            if raw.last().is_none_or(|last| (p - *last).length() >= min_d) {
                raw.push(p);
            }
        }
    }

    pub(crate) fn end_drag(&mut self) {
        // 多拍工具（多边形）不在此收尾：每次 pointer down 落一个顶点，
        // 只有 Enter / 双击 / Esc 才结束。end_drag 靠 mem::replace 清空 drag，
        // 直接返回以免把进行中的顶点序列抹掉。
        if matches!(self.drag, DragState::CreatingPolygon { .. }) {
            return;
        }
        // 裁剪模式拖拽释放：清dragging 标志（应用通过 Enter 触发
        if let Some(crop) = self.crop_mode.as_mut() {
            if crop.dragging.is_some() {
                crop.dragging = None;
                return;
            }
        }

        let prev = std::mem::replace(&mut self.drag, DragState::Idle);
        match prev {
            DragState::HandleTransform {
                item_id,
                start_transform,
                ..
            } => {
                // clone new_transform，避免与 undo_stack.push &mut self.scene 冲突
                let new_transform = self.scene.get_item(&item_id).map(|it| it.transform);
                if let Some(new_tf) = new_transform {
                    if new_tf != start_transform {
                        let cmd = TransformItem::new(item_id, start_transform, new_tf);
                        // skip_first_redo=true，因为预览已应用
                        self.push_cmd(Box::new(cmd));
                        self.flash(fill(
                            t(self.lang, T::FlashTransform),
                            &[
                                format!("{:.2}", new_tf.scale.x),
                                format!("{:.2}", new_tf.scale.y),
                                format!("{:.1}", new_tf.rotation.to_degrees()),
                            ],
                        ));
                    }
                }
                self.transform_handles.end_drag();
            }
            DragState::MoveItems {
                start_transforms,
                line_snaps,
                duplicate_ids,
                original_ids,
                pending_deselect,
                ..
            } => {
                if let Some(ids) = duplicate_ids {
                    // Ctrl+拖动复制：副本已在场景中（预览已移动到最终位置）。
                    let items: Vec<Item> = ids
                        .iter()
                        .filter_map(|id| self.scene.get_item(id).cloned())
                        .collect();
                    // 用任一副本相对其 start_tf 判断是否真的移动了
                    let moved =
                        ids.first()
                            .and_then(|id| {
                                start_transforms.iter().find(|(i, _)| i == id).and_then(
                                    |(_, st)| {
                                        self.scene.get_item(id).map(|it| it.transform.pos - st.pos)
                                    },
                                )
                            })
                            .map(|d| d.x.abs() > 1e-4 || d.y.abs() > 1e-4)
                            .unwrap_or(false);
                    if moved && !items.is_empty() {
                        // 一次 undo 撤掉整个「复制 + 移动」
                        let n = items.len();
                        self.push_cmd(Box::new(AddItems::new(items).with_preview_applied(true)));
                        self.flash(fill(t(self.lang, T::FlashDuplicated), &[n.to_string()]));
                    } else {
                        // 没移动：删掉重叠副本，恢复原件选区（Ctrl+点击 ≠ 复制）
                        for id in &ids {
                            self.scene.remove_item(id);
                        }
                        self.scene.deselect_all();
                        for oid in &original_ids {
                            self.scene.select(*oid);
                        }
                    }
                } else {
                    // 普通移动 / Shift 轴约束移动：用第一 item 反 delta
                    let delta_opt = start_transforms.first().and_then(|(id, start_tf)| {
                        self.scene
                            .get_item(id)
                            .map(|it| it.transform.pos - start_tf.pos)
                    });
                    if let Some(delta) = delta_opt {
                        if delta.x.abs() > 1e-4 || delta.y.abs() > 1e-4 {
                            let ids: Vec<ItemId> =
                                start_transforms.iter().map(|(i, _)| *i).collect();
                            let move_cmd = MoveItems::new(ids, delta);
                            // plan #14：线端吸附的 points/binding 变更与移动打包成
                            // 一条 undo 记录（整体撤销/重做）。子命令的 points 与
                            // 绑定均已预览直改，skip_first_redo 语义一致。
                            let mut line_cmds: Vec<Box<dyn Command>> = Vec::new();
                            for ls in line_snaps {
                                let Some(item) = self.scene.get_item(&ls.line_id) else {
                                    continue;
                                };
                                let ItemKind::Shape {
                                    points,
                                    start_binding,
                                    end_binding,
                                    ..
                                } = &item.kind
                                else {
                                    continue;
                                };
                                let changed = points != &ls.start_points
                                    || start_binding != &ls.start_start_binding
                                    || end_binding != &ls.start_end_binding;
                                if !changed {
                                    continue;
                                }
                                line_cmds.push(Box::new(
                                    EditShapePoints::new(
                                        ls.line_id,
                                        ls.start_points.clone(),
                                        points.clone(),
                                    )
                                    .with_binding_change(
                                        ls.start_start_binding,
                                        *start_binding,
                                        ls.start_end_binding,
                                        *end_binding,
                                    ),
                                ));
                            }
                            let cmd: Box<dyn Command> = if line_cmds.is_empty() {
                                Box::new(move_cmd)
                            } else {
                                let mut all: Vec<Box<dyn Command>> = vec![Box::new(move_cmd)];
                                all.extend(line_cmds);
                                Box::new(MultiCommand::new(all))
                            };
                            self.push_cmd(cmd);
                            self.flash(fill(
                                t(self.lang, T::FlashMoved),
                                &[format!("{:.0}", delta.x), format!("{:.0}", delta.y)],
                            ));
                        } else if let Some(pid) = pending_deselect {
                            // Shift+点击已选中项且没拖动 → 取消选中（组则取消整组，G2）
                            for hid in self.scene.expand_to_groups(&[pid]) {
                                self.scene.selection.remove(&hid);
                            }
                        }
                    } else if let Some(pid) = pending_deselect {
                        for hid in self.scene.expand_to_groups(&[pid]) {
                            self.scene.selection.remove(&hid);
                        }
                    }
                }
            }
            DragState::BoxSelect {
                start_canvas,
                current_canvas,
                additive,
            } => {
                // CAD 语义：左→右=窗口（完全包含才选中），右→左=交叉（相交即选中）
                let min_x = start_canvas.x.min(current_canvas.x);
                let max_x = start_canvas.x.max(current_canvas.x);
                let min_y = start_canvas.y.min(current_canvas.y);
                let max_y = start_canvas.y.max(current_canvas.y);
                let sel_rect = CanvasRect::new(
                    CanvasPoint::new(min_x, min_y),
                    CanvasSize::new(max_x - min_x, max_y - min_y),
                );
                let mode = BoxSelectMode::from_drag(start_canvas, current_canvas);
                if !additive {
                    self.scene.deselect_all();
                }
                // 先收集命中 id，再 select（避免同时 &self.items 和 &mut self.selection）
                // 绑定文本不参与框选：它随容器联动（容器被框选时经 texts_bound_to
                // 带上），单独框中它只会得到一个移动不了的可视快照选中框
                let hits: Vec<ItemId> = self
                    .scene
                    .items
                    .iter()
                    .filter(|item| mode.hits(&sel_rect, &item.bounding_rect()))
                    .filter(|item| !self.scene.is_bound_text(item))
                    .map(|item| item.id)
                    .collect();
                for id in hits {
                    self.scene.select(id);
                }
            }
            DragState::CreatingShape {
                shape_type,
                end_arrow,
                start,
                current,
                shift,
                ctrl,
            } => {
                self.finish_create_shape(shape_type, end_arrow, start, current, shift, ctrl);
            }
            DragState::Drawing { raw } => {
                self.finish_create_freedraw(raw);
            }
            DragState::CreatingFrame { start, current, .. } => {
                self.finish_create_frame(start, current);
            }
            DragState::LineEndpoint {
                item_id,
                endpoint,
                start_canvas: _,
                start_points,
                base_pos,
                alt_extend,
                keep_inserted,
            } => {
                // 预览已直接改 points；释放时若有变化则固化到 undo 栈
                let mut new_points = match self.scene.get_item(&item_id) {
                    Some(item) => match &item.kind {
                        ItemKind::Shape { points, .. } => points.clone(),
                        _ => Vec::new(),
                    },
                    None => Vec::new(),
                };
                if alt_extend && new_points == start_points {
                    // plan #4：Alt+按在端点上未越过延伸阈值即释放 = Alt+单击 →
                    // 删除该顶点（守卫在 try_delete_vertex 内：开放 ≥2 / 闭合 ≥3）。
                    if !new_points.is_empty() {
                        self.try_delete_vertex(item_id, endpoint);
                    }
                } else if !new_points.is_empty() && new_points != start_points {
                    if !keep_inserted
                        && new_points.len() != start_points.len()
                        && new_points.get(endpoint) == Some(&base_pos)
                    {
                        // 点击了段中点但未拖动：移除插入的顶点，不产生空命令
                        // （Alt 延伸越阈后又精确拖回 base 的罕见情形同样落在此清理）
                        if let Some(item) = self.scene.get_item_mut(&item_id) {
                            if let ItemKind::Shape { points, .. } = &mut item.kind {
                                if points.len() == new_points.len() {
                                    points.remove(endpoint);
                                }
                            }
                        }
                    } else {
                        // 仅真实端点（0 / 末点）涉及绑定；内部顶点拖拽保持原绑定。
                        let last = new_points.len().saturating_sub(1);
                        let is_real = endpoint == 0 || endpoint == last;
                        let (start_old, end_old, open_now) = {
                            let it = self.scene.get_item(&item_id);
                            let sb = it.and_then(|i| i.start_binding());
                            let eb = it.and_then(|i| i.end_binding());
                            let open = matches!(
                                it.map(|i| &i.kind),
                                Some(ItemKind::Shape { closed: false, .. })
                            );
                            (sb, eb, open)
                        };
                        // plan #4：端点拖回起点附近释放 → 自动闭合成多边形
                        // （≥3 顶点、首尾距 ≤ 屏幕 8px；Excalidraw isPathALoop 同式）。
                        let will_close = open_now
                            && is_real
                            && should_auto_close(&new_points, endpoint, self.viewport.zoom);
                        // 验收反馈 #4-1：反向操作——"已闭合的合并点"（首尾重合）
                        // 把其中一个端点拖开（释放时首尾距 > 屏幕 8px）→ 自动恢复
                        // 开放，无需回侧栏取消闭合勾选。仅重合态触发，普通闭合多边形
                        // 拖顶点不会误开。
                        let overlap_before = start_points.len() >= 2
                            && start_points[0] == start_points[start_points.len() - 1];
                        let will_open = !open_now
                            && is_real
                            && overlap_before
                            && first_last_screen_dist(&new_points, self.viewport.zoom)
                                > POLYLINE_CLOSE_DISTANCE;
                        let mut start_new = start_old;
                        let mut end_new = end_old;
                        let snapped = self.pending_endpoint_binding.is_some();
                        if will_close {
                            // 闭合优先于吸附：被拖端点吸附至对端、该端解绑；
                            // closed 与点集经 with_closed 同占一条 undo。
                            if let Some(item) = self.scene.get_item_mut(&item_id) {
                                if let ItemKind::Shape {
                                    points,
                                    closed,
                                    start_binding,
                                    end_binding,
                                    ..
                                } = &mut item.kind
                                {
                                    let l = points.len().saturating_sub(1);
                                    if l >= 2 {
                                        let src = if endpoint == 0 { points[l] } else { points[0] };
                                        if let Some(p) = points.get_mut(endpoint) {
                                            *p = src;
                                        }
                                        *closed = true;
                                        if endpoint == 0 {
                                            *start_binding = None;
                                            start_new = None;
                                        } else {
                                            *end_binding = None;
                                            end_new = None;
                                        }
                                        new_points = points.clone();
                                    }
                                }
                            }
                        } else {
                            if will_open {
                                // 预览直改开放态；closed 变更记录经 with_closed
                                // 随同一条 undo 还原。
                                if let Some(item) = self.scene.get_item_mut(&item_id) {
                                    if let ItemKind::Shape { closed, .. } = &mut item.kind {
                                        *closed = false;
                                    }
                                }
                            }
                            match self.pending_endpoint_binding {
                                Some((idx, bid, anchor)) if is_real => {
                                    let binding = Some(EndpointBinding {
                                        target: bid,
                                        anchor,
                                    });
                                    if idx == 0 {
                                        start_new = binding;
                                    } else {
                                        end_new = binding;
                                    }
                                }
                                None if is_real => {
                                    // 拖离形状 → 解除该端点绑定
                                    if endpoint == 0 {
                                        start_new = None;
                                    } else {
                                        end_new = None;
                                    }
                                }
                                _ => {}
                            }
                        }
                        // 绑定写入必须在此"预览直改"：EditShapePoints 的
                        // skip_first_redo=true 会跳过 push 后的首次 redo，而
                        // 绑定字段只在 redo 里写——不直改的话吸附释放后
                        // start/end_binding 仍是 None，移动被吸附图形时
                        // resolve_bindings 不重算，端点不跟随（用户反馈）。
                        if let Some(item) = self.scene.get_item_mut(&item_id) {
                            if let ItemKind::Shape {
                                start_binding,
                                end_binding,
                                ..
                            } = &mut item.kind
                            {
                                *start_binding = start_new;
                                *end_binding = end_new;
                            }
                        }
                        let cmd = EditShapePoints::new(item_id, start_points, new_points)
                            .with_binding_change(start_old, start_new, end_old, end_new);
                        let cmd = if will_close {
                            cmd.with_closed(false, true)
                        } else if will_open {
                            cmd.with_closed(true, false)
                        } else {
                            cmd
                        };
                        self.push_cmd(Box::new(cmd));
                        if will_close {
                            self.flash(t(self.lang, T::FlashPolygonClosed));
                        } else if will_open {
                            self.flash(t(self.lang, T::FlashPolygonOpened));
                        } else if snapped {
                            self.flash(t(self.lang, T::FlashSnappedToShape));
                        }
                    }
                }
                self.pending_endpoint_binding = None;
                self.snap_highlight = None;
                self.transform_handles.end_drag();
            }
            // 多边形在函数开头已提前 return（多拍工具不在释放时收尾），这里只为穷尽匹配
            DragState::CreatingPolygon { .. } => {}
            DragState::ElbowBar {
                item_id,
                start_offset,
                ..
            } => {
                // 预览已直改 elbow_mid_offset；释放时若有变化则固化到 undo 栈
                let cur_offset = self
                    .scene
                    .get_item(&item_id)
                    .and_then(|item| match &item.kind {
                        ItemKind::Shape {
                            elbow_mid_offset, ..
                        } => Some(*elbow_mid_offset),
                        _ => None,
                    })
                    .unwrap_or(start_offset);
                if (cur_offset - start_offset).abs() > 1e-4 {
                    let cmd = SetElbowOffset::new(item_id, start_offset, cur_offset)
                        .with_preview_applied(true);
                    self.push_cmd(Box::new(cmd));
                }
                self.transform_handles.end_drag();
            }
            DragState::Idle => {}
        }
    }

    /// 用绘制工具完成 shape 创建：计算矩形 → AddItem → 回 Select。
    /// shift = 锁定正方形（用宽高较大者作边长）；线性对象 = 锁定 45° 方向。
    /// ctrl = 椭圆解锁自由宽高比（默认椭圆为正圆，与变换框行为一致）。
    pub(crate) fn finish_create_shape(
        &mut self,
        shape_type: ShapeType,
        end_arrow: Option<ArrowHeadStyle>,
        start: CanvasPoint,
        current: CanvasPoint,
        shift: bool,
        ctrl: bool,
    ) {
        // 线性对象：两点式（start → current），Shift 锁 45°，最小长度 3 画布像素。
        if shape_type == ShapeType::Polyline {
            let mut dx = current.x - start.x;
            let mut dy = current.y - start.y;
            if shift {
                let len = (dx * dx + dy * dy).sqrt();
                if len < 1e-3 {
                    return;
                }
                // 方向吸附到 45° 整数倍
                let angle = (dy.atan2(dx) / std::f32::consts::FRAC_PI_4).round()
                    * std::f32::consts::FRAC_PI_4;
                dx = len * angle.cos();
                dy = len * angle.sin();
            }
            if (dx * dx + dy * dy).sqrt() < 3.0 {
                return;
            }
            // 自动闭合：终点回到起点模糊距离内即判定为闭合图形（Excalidraw 风格）
            let closed = (dx * dx + dy * dy).sqrt() <= POLYLINE_CLOSE_DISTANCE;
            let min_x = start.x.min(start.x + dx);
            let min_y = start.y.min(start.y + dy);
            // 局部坐标：起点对齐 AABB 左上角
            let p0 = (start.x - min_x, start.y - min_y);
            let p1 = (start.x + dx - min_x, start.y + dy - min_y);
            let item = Item::new_polyline(
                vec![p0, p1],
                (dx.abs(), dy.abs()),
                None,
                end_arrow,
                closed,
                min_x,
                min_y,
                self.default_stroke,
            )
            .with_sloppiness(self.default_sloppiness);
            self.push_new_item(AddItem::new(item));
            self.flash(t(
                self.lang,
                if end_arrow.is_some() {
                    T::FlashArrowCreated
                } else {
                    T::FlashLineCreated
                },
            ));
            // 默认回 Select
            self.tool = Tool::Select;
            return;
        }

        let w = (current.x - start.x).abs();
        let h = (current.y - start.y).abs();
        // 误触：小于 3 画布像素丢弃
        if w < 3.0 || h < 3.0 {
            return;
        }
        let min_x = start.x.min(current.x);
        let min_y = start.y.min(current.y);
        let (bw, bh) = if shape_type == ShapeType::Ellipse {
            // 椭圆默认正圆；Ctrl 按下画自由椭圆（与变换框行为一致）
            if ctrl {
                (w, h)
            } else {
                let side = w.max(h);
                (side, side)
            }
        } else if shift {
            let side = w.max(h);
            (side, side)
        } else {
            (w, h)
        };
        // 填充默认值：选了填充样式但未选填充色时，跟随描边色（Excalidraw 语义），
        // 并套默认 50% 不透明度（plan #2；显式选过填充色则保留用户设置）
        let fill = self.default_fill_style.map(|_| {
            self.default_fill.unwrap_or_else(|| {
                let c = self.default_stroke.color;
                [c[0], c[1], c[2], palette::FILL_DEFAULT_ALPHA]
            })
        });
        let item = Item::new_shape(
            shape_type,
            (bw, bh),
            min_x,
            min_y,
            self.default_stroke,
            fill,
        )
        .with_sloppiness(self.default_sloppiness)
        .with_fill_style(self.default_fill_style.unwrap_or(FillStyle::Solid));
        self.push_new_item(AddItem::new(item));
        self.flash(t(self.lang, T::FlashShapeCreated));
        // 默认回 Select
        self.tool = Tool::Select;
    }

    /// 徒手绘制收尾（plan #10）：把连续采样的画布点按「间距→速度」算出逐点笔宽，
    /// 归一化成 `ItemKind::Freedraw`（点存 AABB 局部坐标、位置进 transform.pos），
    /// `AddItem` 一步入 undo、随即回 Select。点数 <2（误触/单击）丢弃、不产生命令。
    pub(crate) fn finish_create_freedraw(&mut self, raw: Vec<CanvasPoint>) {
        self.tool = Tool::Select;
        if raw.len() < 2 {
            return;
        }
        let raw_pts: Vec<(f32, f32)> = raw.iter().map(|p| (p.x, p.y)).collect();
        let raw_pressures =
            preferz_core::freedraw::pressures_from_spacing(&raw_pts, self.viewport.zoom);
        // 落笔定型即做 Catmull-Rom 重采样（压力同步插值）：存储即平滑，命中与渲染用同
        // 一份点集，消除快运笔稀疏采样的折角分段感。
        let (pts, pressures) = preferz_core::freedraw::smooth_centerline(
            &raw_pts,
            &raw_pressures,
            preferz_core::freedraw::SMOOTH_SAMPLES,
        );
        let item = Item::new_freedraw(
            &pts,
            &pressures,
            self.default_stroke.width,
            self.default_stroke.color,
        );
        self.push_new_item(AddItem::new(item));
        self.flash(t(self.lang, T::FlashFreedrawCreated));
    }

    /// 用 Frame 工具完成画框创建：计算矩形 → AddItem → 置底（z=min-1）→ 选中 → 回 Select。
    pub(crate) fn finish_create_frame(&mut self, start: CanvasPoint, current: CanvasPoint) {
        let w = (current.x - start.x).abs();
        let h = (current.y - start.y).abs();
        if w < 10.0 || h < 10.0 {
            // 误触：过小丢弃
            self.tool = Tool::Select;
            return;
        }
        let min_x = start.x.min(current.x);
        let min_y = start.y.min(current.y);
        let number = self.scene.next_frame_number();
        let item = Item::new_frame(number, (w, h), min_x, min_y, None);
        let frame_id = item.id;
        let cmd = AddItem::new(item);
        self.push_cmd(Box::new(cmd));
        // 画框恒在最底（z = min - 1）：用 ReorderItems send-to-back
        let reorder = ReorderItems::new(vec![frame_id], false);
        self.push_cmd(Box::new(reorder));
        self.scene.deselect_all();
        self.scene.select(frame_id);
        self.flash(fill(
            t(self.lang, T::FlashFrameCreated),
            &[number.to_string()],
        ));
        // 默认回 Select
        self.tool = Tool::Select;
    }

    /// 多边形工具收尾（Phase I）：把已落定的顶点固化成一个 `closed: true` 的 Polyline。
    ///
    /// 顶点先归一化到"以 AABB 左上角为原点"的局部坐标，整体位置交给 `transform.pos`——
    /// 与两点式折线共用 `Item::new_polyline` 的同一约定，于是后续移动/缩放/顶点拖拽
    /// 都能复用既有的线性对象代码路径，不必为多边形单开分支。
    ///
    /// 收尾时机：Enter / 双击画布（见 `handle_shortcuts` 与 CentralPanel）。
    /// 顶点不足 3 个时不产生 item（无法构成面），只回 Select 并给出提示。
    pub(crate) fn finish_create_polygon(&mut self) {
        let (mut points, current) = match std::mem::replace(&mut self.drag, DragState::Idle) {
            DragState::CreatingPolygon {
                points, current, ..
            } => (points, current),
            // 非多边形状态（防御性）：原样放回，避免吞掉别的拖拽
            other => {
                self.drag = other;
                return;
            }
        };
        // 橡皮筋末端算作最后一个顶点；指针停在上一顶点上（双击）时距离≈0，会被跳过
        if let Some(last) = points.last() {
            if (current - *last).length() >= 3.0 {
                points.push(current);
            }
        }
        self.tool = Tool::Select;
        if points.len() < 3 {
            self.flash(t(self.lang, T::PolygonTooFewPoints));
            return;
        }
        let (min_x, min_y, max_x, max_y) =
            points
                .iter()
                .fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |acc, p| {
                    (
                        acc.0.min(p.x),
                        acc.1.min(p.y),
                        acc.2.max(p.x),
                        acc.3.max(p.y),
                    )
                });
        let local: Vec<(f32, f32)> = points.iter().map(|p| (p.x - min_x, p.y - min_y)).collect();
        let item = Item::new_polyline(
            local,
            (max_x - min_x, max_y - min_y),
            None,
            None,
            true,
            min_x,
            min_y,
            self.default_stroke,
        )
        .with_sloppiness(self.default_sloppiness);
        self.push_new_item(AddItem::new(item));
        self.flash(t(self.lang, T::PolygonCreated));
    }
}
