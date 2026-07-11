use eframe::egui;
use preferz_core::{Scene, Item, ItemKind, ItemId, Command};
use preferz_core::commands::{TransformItem, MoveItems, DeleteItems, AddItem, ReorderItems, FlipItems, EditTextContent};
use preferz_core::spaces::{CanvasPoint, CanvasVector, CanvasRect, CanvasSize};
use crate::viewport::ViewportState;
use crate::ui::widgets::transform_handles::{TransformHandles, Handle};
use crate::interaction;
use image::GenericImageView;
use std::path::PathBuf;
use std::collections::HashMap;

/// Undo 栈。`push` 会读取 `Command::skip_first_redo()`：
/// - 交互预览命令（拖拽中已直接改 item）返回 true → 跳过首次 redo
/// - 普通命令返回 false → push 时立即 redo 应用变更
///
/// 这让 AGENTS.md Gotcha #5（skip_first_redo）真正生效（修 S5/M7）。
struct UndoStack {
    undo: Vec<Box<dyn Command>>,
    redo: Vec<Box<dyn Command>>,
}

impl UndoStack {
    fn new() -> Self {
        Self { undo: Vec::new(), redo: Vec::new() }
    }

    fn push(&mut self, mut cmd: Box<dyn Command>, scene: &mut Scene) {
        let skip = cmd.skip_first_redo();
        if !skip {
            cmd.redo(scene);
        }
        self.redo.clear();
        self.undo.push(cmd);
    }

    fn undo(&mut self, scene: &mut Scene) -> bool {
        if let Some(mut cmd) = self.undo.pop() {
            cmd.undo(scene);
            self.redo.push(cmd);
            true
        } else {
            false
        }
    }

    fn redo(&mut self, scene: &mut Scene) -> bool {
        if let Some(mut cmd) = self.redo.pop() {
            cmd.redo(scene);
            self.undo.push(cmd);
            true
        } else {
            false
        }
    }
}

/// 拖拽状态机。Idle / 手柄变换 / 移动多 item / 框选。
enum DragState {
    Idle,
    HandleTransform {
        item_id: ItemId,
        handle: Handle,
        start_screen: egui::Pos2,
        start_transform: preferz_core::Transform,
        start_corners: [CanvasPoint; 4],
    },
    MoveItems {
        start_canvas: CanvasPoint,
        start_transforms: Vec<(ItemId, preferz_core::Transform)>,
    },
    /// 框选（spec L240）。空白处左键拖拽成矩形，Shift 加选。
    BoxSelect {
        start_canvas: CanvasPoint,
        current_canvas: CanvasPoint,
        additive: bool,
    },
}

/// 文本便签编辑状态（spec L243 P2-5）。
/// `editing_item_id = None` 表示创建新文本（提交时 push `AddItem`）；
/// `Some(id)` 表示编辑现有 item（提交时 push `EditTextContent`）。
/// Enter/失焦时提交，空内容在创建模式下丢弃，在编辑模式下不修改原 item。
struct EditingText {
    editing_item_id: Option<ItemId>,
    canvas_pos: CanvasPoint,
    buffer: String,
    font_size: f32,
    color: [u8; 4],
    first_frame: bool,
}

pub struct PReferZApp {
    scene: Scene,
    viewport: ViewportState,
    undo_stack: UndoStack,
    /// 临时状态消息（如"已导入"），会在若干帧后清空，避免覆盖持续状态（修 B5）。
    flash_status: Option<(String, std::time::Instant)>,
    context_menu_open: bool,
    context_menu_pos: egui::Pos2,
    texture_cache: HashMap<u64, egui::TextureHandle>,
    next_texture_id: u64,
    pending_import: Vec<PathBuf>,
    transform_handles: TransformHandles,
    drag: DragState,
    /// 文本便签编辑状态（None = 无编辑）。
    editing_text: Option<EditingText>,
}

impl PReferZApp {
    pub fn new() -> Self {
        Self {
            scene: Scene::new(),
            viewport: ViewportState::default(),
            undo_stack: UndoStack::new(),
            flash_status: None,
            context_menu_open: false,
            context_menu_pos: egui::Pos2::ZERO,
            texture_cache: HashMap::new(),
            next_texture_id: 1,
            pending_import: Vec::new(),
            transform_handles: TransformHandles::new(),
            drag: DragState::Idle,
            editing_text: None,
        }
    }

    fn flash(&mut self, msg: impl Into<String>) {
        self.flash_status = Some((msg.into(), std::time::Instant::now()));
    }

    /// 选中 item 的快照（按 Z 序倒序，顶层在前）。
    fn selected_items_snapshot(&self) -> Vec<Item> {
        let mut items: Vec<Item> = self.scene.selection.iter()
            .filter_map(|id| self.scene.get_item(id).cloned())
            .collect();
        items.sort_by(|a, b| b.z.cmp(&a.z));
        items
    }
}

impl Default for PReferZApp {
    fn default() -> Self {
        Self::new()
    }
}

const FLASH_DURATION_MS: u128 = 2500;

impl eframe::App for PReferZApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 拖放导入（spec L228，P3-1 顺手做）
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw.dropped_files.iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() {
            self.pending_import.extend(dropped);
        }

        // 处理待导入（在 update 中访问 ctx）
        while let Some(path) = self.pending_import.pop() {
            self.process_import(ctx, path);
        }

        // 清理过期的 flash 状态
        if let Some((_, t)) = self.flash_status {
            if t.elapsed().as_millis() > FLASH_DURATION_MS {
                self.flash_status = None;
            }
        }

        // 中央画布
        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = ui.max_rect();
            self.viewport.set_screen_rect(rect);

            let response = ui.interact(rect, egui::Id::new("canvas"), egui::Sense::click_and_drag());

            // 画布背景
            ui.painter().rect_filled(
                rect,
                egui::Rounding::same(0.0),
                egui::Color32::from_rgb(45, 45, 48),
            );

            // 渲染场景（含视口剔除 + Z 序 + 复用 self.transform_handles）
            self.render_scene(ui);

            // 框选矩形（spec L240）
            if let DragState::BoxSelect { start_canvas, current_canvas, .. } = &self.drag {
                let min = self.viewport.canvas_to_screen(*start_canvas);
                let max = self.viewport.canvas_to_screen(*current_canvas);
                let rect = egui::Rect::from_min_max(min, max);
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(100, 200, 255, 30),
                );
                let stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 200, 255));
                ui.painter().rect_stroke(rect, 0.0, stroke);
            }

            // 鼠标中键拖拽平移
            if response.dragged_by(egui::PointerButton::Middle) {
                self.viewport.pan_by_screen(response.drag_delta());
            }

            // 滚轮缩放（以鼠标位置为锚点）
            let scroll = ctx.input(|i| i.raw_scroll_delta);
            if scroll.y != 0.0 {
                if let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) {
                    self.viewport.zoom_at(scroll.y, pos);
                }
            }

            // 双击：Text item → 编辑；空白 → 创建文本便签（spec L243 P2-5）
            if response.double_clicked() && self.editing_text.is_none() {
                if let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) {
                    if rect.contains(pos) {
                        // 先克隆命中 Text item 的字段，避免 &self.scene 与 &mut self.editing_text 借用冲突
                        let hit_text = interaction::get_item_at(pos, &self.scene, &self.viewport)
                            .and_then(|item| {
                                if let ItemKind::Text { content, font_size, color, .. } = &item.kind {
                                    Some((item.id, item.transform.pos, content.clone(), *font_size, *color))
                                } else {
                                    None
                                }
                            });
                        if let Some((id, pos_canvas, content, font_size, color)) = hit_text {
                            // 双击 Text item → 编辑现有
                            self.editing_text = Some(EditingText {
                                editing_item_id: Some(id),
                                canvas_pos: CanvasPoint::new(pos_canvas.x, pos_canvas.y),
                                buffer: content,
                                font_size,
                                color,
                                first_frame: true,
                            });
                            self.drag = DragState::Idle;
                        } else {
                            // 双击空白 → 创建新文本
                            let canvas_pos = self.viewport.screen_to_canvas(pos);
                            self.editing_text = Some(EditingText {
                                editing_item_id: None,
                                canvas_pos,
                                buffer: String::new(),
                                font_size: 24.0,
                                color: [255, 255, 255, 255],
                                first_frame: true,
                            });
                            self.drag = DragState::Idle;
                        }
                    }
                }
            }

            let pointer_pos = ctx.input(|i| i.pointer.latest_pos());
            let primary_pressed = ctx.input(|i| i.pointer.primary_pressed());
            let primary_down = ctx.input(|i| i.pointer.primary_down());
            let primary_released = ctx.input(|i| i.pointer.primary_released());

            // 更新 hover + 光标
            if let Some(pos) = pointer_pos {
                if rect.contains(pos) {
                    let selected = self.selected_items_snapshot();
                    // 多选时只支持统一移动，不检测单独手柄（手柄不可见却可 hover 会造成混乱）
                    if selected.len() == 1 {
                        self.transform_handles.update_hover(pos, &selected, &self.viewport);
                    } else {
                        self.transform_handles.hover_handle = Handle::None;
                    }
                    let cursor = match self.transform_handles.hover_handle {
                        Handle::ResizeTopLeft | Handle::ResizeBottomRight => egui::CursorIcon::ResizeNorthEast,
                        Handle::ResizeTopRight | Handle::ResizeBottomLeft => egui::CursorIcon::ResizeNorthWest,
                        Handle::Rotate => egui::CursorIcon::Grab,
                        Handle::FlipH => egui::CursorIcon::ResizeHorizontal,
                        Handle::FlipV => egui::CursorIcon::ResizeVertical,
                        Handle::None => {
                            // 在 item 上时显示移动光标
                            if interaction::get_item_at(pos, &self.scene, &self.viewport).is_some() {
                                egui::CursorIcon::Move
                            } else {
                                egui::CursorIcon::Default
                            }
                        }
                    };
                    ctx.output_mut(|o| o.cursor_icon = cursor);
                } else {
                    self.transform_handles.hover_handle = Handle::None;
                }
            } else {
                self.transform_handles.hover_handle = Handle::None;
            }

            // 拖拽中：更新预览
            if primary_down && !matches!(self.drag, DragState::Idle) {
                if let Some(pos) = pointer_pos {
                    let free_scale = ctx.input(|i| i.modifiers.ctrl);
                    self.update_drag_preview(pos, free_scale);
                    ctx.request_repaint();
                }
            }

            // 按下：开始拖拽（手柄优先，否则移动）。菜单打开时不启动拖拽（修 B4：
            // 让 render_context_menu 负责检测点击外部并关闭菜单）
            if primary_pressed && !self.context_menu_open {
                if let Some(pos) = pointer_pos {
                    if rect.contains(pos) {
                        let additive = ctx.input(|i| i.modifiers.shift);
                        self.begin_drag(pos, additive);
                    }
                }
            }

            // 释放：固化到 undo 栈
            if primary_released {
                self.end_drag();
            }

            // 右键菜单
            if response.clicked_by(egui::PointerButton::Secondary) {
                if let Some(pos) = response.hover_pos() {
                    self.context_menu_open = true;
                    self.context_menu_pos = pos;
                }
            }
        });

        // 文本编辑 overlay（spec L243 P2-5）
        self.render_text_editor(ctx);

        // 上下文菜单
        if self.context_menu_open {
            self.render_context_menu(ctx);
        }

        // 快捷键
        self.handle_shortcuts(ctx);

        // 状态栏（持续状态 + flash 消息）
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            let persistent = format!(
                "缩放: {:.2}x | 平移: ({:.0}, {:.0}) | items: {} | 选中: {}",
                self.viewport.zoom,
                self.viewport.pan.x,
                self.viewport.pan.y,
                self.scene.items.len(),
                self.scene.selection.len(),
            );
            if let Some((msg, _)) = &self.flash_status {
                ui.horizontal(|ui| {
                    ui.label(&persistent);
                    ui.separator();
                    ui.colored_label(egui::Color32::LIGHT_GREEN, msg);
                });
            } else {
                ui.label(&persistent);
            }
        });

        // Debug 面板
        self.render_debug_panel(ctx);
    }
}

// ─────────────────────────── 拖拽逻辑 ───────────────────────────

impl PReferZApp {
    fn begin_drag(&mut self, screen_pos: egui::Pos2, additive: bool) {
        // 文本编辑中不启动拖拽
        if self.editing_text.is_some() {
            return;
        }
        // 1) 手柄优先
        let hover = self.transform_handles.hover_handle;
        if hover != Handle::None {
            // 找到手柄所属的 item
            let selected = self.selected_items_snapshot();
            for item in selected.iter().rev() {
                let show_flip = matches!(item.kind, ItemKind::Pixmap { .. });
                let h = self.transform_handles.hit_test(screen_pos, item, &self.viewport, show_flip);
                if h != Handle::None {
                    // 翻转边手柄：点击即触发翻转，不进入拖拽（spec L239「翻转边」）
                    if h == Handle::FlipH || h == Handle::FlipV {
                        let ids: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
                        let horizontal = h == Handle::FlipH;
                        let cmd = FlipItems::new(ids, horizontal);
                        self.undo_stack.push(Box::new(cmd), &mut self.scene);
                        self.flash(if horizontal { "水平翻转" } else { "垂直翻转" });
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

        // 2) 命中 item：选中并开始移动拖拽
        if let Some(item) = interaction::get_item_at(screen_pos, &self.scene, &self.viewport) {
            let id = item.id;
            if additive {
                // Shift 加选：toggle，若取消选中则不开始拖拽
                self.scene.toggle_selection(id);
                if !self.scene.selection.contains(&id) {
                    return;
                }
            } else if !self.scene.selection.contains(&id) {
                // 非加选且未选中：替换选中为该项
                self.scene.deselect_all();
                self.scene.select(id);
            }
            // 收集所有选中 item 的 transform 快照
            let start_transforms: Vec<(ItemId, preferz_core::Transform)> = self.scene.selection.iter()
                .filter_map(|sid| self.scene.get_item(sid).map(|it| (*sid, it.transform)))
                .collect();
            let start_canvas = self.viewport.screen_to_canvas(screen_pos);
            self.drag = DragState::MoveItems { start_canvas, start_transforms };
            return;
        }

        // 3) 空白：开始框选（spec L240）。Shift = 加选模式
        let start_canvas = self.viewport.screen_to_canvas(screen_pos);
        self.drag = DragState::BoxSelect {
            start_canvas,
            current_canvas: start_canvas,
            additive,
        };
    }

    fn update_drag_preview(&mut self, screen_pos: egui::Pos2, free_scale: bool) {
        match &self.drag {
            DragState::HandleTransform { item_id, handle, start_screen, start_transform, start_corners } => {
                let item_id = *item_id;
                let handle = *handle;
                let start_screen = *start_screen;
                let start_transform = *start_transform;
                let start_corners = *start_corners;
                let mouse_canvas = self.viewport.screen_to_canvas(screen_pos);
                if let Some(item) = self.scene.get_item_mut(&item_id) {
                    match handle {
                        Handle::Rotate => {
                            apply_rotate_drag(&self.viewport, item, start_transform, start_corners, start_screen, screen_pos);
                        }
                        Handle::ResizeTopLeft | Handle::ResizeTopRight
                        | Handle::ResizeBottomLeft | Handle::ResizeBottomRight => {
                            apply_scale_drag(item, handle, start_transform, start_corners, mouse_canvas, free_scale);
                        }
                        // 翻转手柄在 begin_drag 中已即时处理，不会进入拖拽预览
                        Handle::FlipH | Handle::FlipV | Handle::None => {}
                    }
                }
            }
            DragState::MoveItems { start_canvas, start_transforms } => {
                let current_canvas = self.viewport.screen_to_canvas(screen_pos);
                let delta = current_canvas - *start_canvas;
                for (id, start_tf) in start_transforms {
                    if let Some(item) = self.scene.get_item_mut(id) {
                        item.transform.pos = start_tf.pos + delta;
                    }
                }
            }
            DragState::BoxSelect { .. } => {} // 在 match 后更新（需要 &mut self.drag）
            DragState::Idle => {}
        }
        // BoxSelect 更新 current_canvas（match &self.drag 不可写，故单独 &mut）
        if let DragState::BoxSelect { current_canvas, .. } = &mut self.drag {
            *current_canvas = self.viewport.screen_to_canvas(screen_pos);
        }
    }

    fn end_drag(&mut self) {
        let prev = std::mem::replace(&mut self.drag, DragState::Idle);
        match prev {
            DragState::HandleTransform { item_id, start_transform, .. } => {
                // 先 clone 出 new_transform，避免与 undo_stack.push 的 &mut self.scene 冲突
                let new_transform = self.scene.get_item(&item_id).map(|it| it.transform);
                if let Some(new_tf) = new_transform {
                    if new_tf != start_transform {
                        let cmd = TransformItem::new(item_id, start_transform, new_tf);
                        // skip_first_redo=true，因为预览已应用
                        self.undo_stack.push(Box::new(cmd), &mut self.scene);
                        self.flash(format!(
                            "变换: 缩放=({:.2},{:.2}) 旋转={:.1}°",
                            new_tf.scale.x, new_tf.scale.y,
                            new_tf.rotation.to_degrees()
                        ));
                    }
                }
                self.transform_handles.end_drag();
            }
            DragState::MoveItems { start_canvas, start_transforms } => {
                // 用第一个 item 的当前位置反推 delta
                let delta_opt = start_transforms.first().and_then(|(id, start_tf)| {
                    self.scene.get_item(id).map(|it| it.transform.pos - start_tf.pos)
                });
                if let Some(delta) = delta_opt {
                    if delta.x.abs() > 1e-4 || delta.y.abs() > 1e-4 {
                        let ids: Vec<ItemId> = start_transforms.iter().map(|(i, _)| *i).collect();
                        let cmd = MoveItems::new(ids, delta);
                        self.undo_stack.push(Box::new(cmd), &mut self.scene);
                        self.flash(format!("移动: ({:.0}, {:.0})", delta.x, delta.y));
                    }
                }
                let _ = start_canvas;
            }
            DragState::BoxSelect { start_canvas, current_canvas, additive } => {
                // 选中框内所有 item（bounding_rect 相交即选中）
                let min_x = start_canvas.x.min(current_canvas.x);
                let max_x = start_canvas.x.max(current_canvas.x);
                let min_y = start_canvas.y.min(current_canvas.y);
                let max_y = start_canvas.y.max(current_canvas.y);
                let sel_rect = CanvasRect::new(
                    CanvasPoint::new(min_x, min_y),
                    CanvasSize::new(max_x - min_x, max_y - min_y),
                );
                if !additive {
                    self.scene.deselect_all();
                }
                // 先收集命中 id，再 select（避免同时 &self.items 和 &mut self.selection）
                let hits: Vec<ItemId> = self.scene.items.iter()
                    .filter(|item| item.bounding_rect().intersects(&sel_rect))
                    .map(|item| item.id)
                    .collect();
                for id in hits {
                    self.scene.select(id);
                }
            }
            DragState::Idle => {}
        }
    }
}

// ─────────────────────────── 渲染 ───────────────────────────

impl PReferZApp {
    fn render_scene(&mut self, ui: &mut egui::Ui) {
        let screen_rect = ui.max_rect();

        // 先测量所有 Text item 的实际尺寸，更新 measured_size（修 B6：边框与渲染一致）。
        // measured_size 为 None（新建/编辑/undo/redo 后）才重新测量，避免每帧重复计算。
        self.update_text_measured_sizes(ui.ctx());

        // 按 Z 序渲染（修 W9）：底层先画，顶层后画
        let items: Vec<&Item> = self.scene.items_by_z_order();
        let selection_count = self.scene.selection.len();
        let editing_id = self.editing_text.as_ref().and_then(|e| e.editing_item_id);
        for item in items {
            // 视口剔除（修 S9/M11）：用画布 AABB 转屏幕矩形，不相交则跳过
            let canvas_bbox = item.bounding_rect();
            let item_screen_rect = self.viewport.canvas_to_screen_rect(canvas_bbox);
            if !screen_rect.intersects(item_screen_rect) {
                continue;
            }

            let corners = item.canvas_corners();
            let screen_corners = [
                self.viewport.canvas_to_screen(corners[0]),
                self.viewport.canvas_to_screen(corners[1]),
                self.viewport.canvas_to_screen(corners[2]),
                self.viewport.canvas_to_screen(corners[3]),
            ];

            let is_selected = self.scene.selection_contains(&item.id);

            match &item.kind {
                ItemKind::Pixmap { texture_id, .. } => {
                    if let Some(handle) = self.texture_cache.get(texture_id) {
                        // 翻转通过 UV 翻转实现（local_to_canvas 的 flip 用于几何/命中，UV 用于图像渲染）
                        let (u_min, u_max) = if item.transform.flip_h { (1.0, 0.0) } else { (0.0, 1.0) };
                        let (v_min, v_max) = if item.transform.flip_v { (1.0, 0.0) } else { (0.0, 1.0) };
                        let uv = egui::Rect::from_min_max(
                            egui::Pos2::new(u_min, v_min),
                            egui::Pos2::new(u_max, v_max),
                        );
                        let _ = screen_corners; // 真正的 quad 贴图需自定义 mesh，超出本次修复范围
                        ui.painter().image(
                            handle.id(),
                            item_screen_rect,
                            uv,
                            egui::Color32::WHITE,
                        );
                    } else {
                        ui.painter().rect_filled(
                            item_screen_rect,
                            egui::Rounding::same(0.0),
                            if is_selected { egui::Color32::from_rgb(80, 80, 40) } else { egui::Color32::from_rgb(70, 70, 70) },
                        );
                    }
                }
                ItemKind::Text { content, font_size, color, .. } => {
                    // 编辑期间跳过该 item 的内容渲染（overlay 接管，避免原文字与编辑框重叠）
                    let is_being_edited = editing_id == Some(item.id);
                    if !is_being_edited {
                        ui.painter().rect_filled(
                            item_screen_rect,
                            egui::Rounding::same(2.0),
                            egui::Color32::from_rgb(60, 60, 60),
                        );
                        let text_color = egui::Color32::from_rgba_premultiplied(
                            color[0], color[1], color[2], color[3],
                        );
                        let origin = self.viewport.canvas_to_screen(corners[0]);
                        // 文字渲染应用 scale 和 zoom（修 B6：与变换边框一致）。
                        // 用 scale.x（等比缩放场景下与 scale.y 相同；非等比时 egui text 不支持非均匀缩放）
                        let effective_font_size = *font_size * item.transform.scale.x.abs() * self.viewport.zoom;
                        ui.painter().text(
                            origin,
                            egui::Align2::LEFT_TOP,
                            content.clone(),
                            egui::FontId::proportional(effective_font_size),
                            text_color,
                        );
                    }
                }
            }

            // 选中框 + 手柄：单选时画单独手柄；多选时画统一外框（循环后）
            if is_selected && selection_count == 1 {
                let show_flip = matches!(item.kind, ItemKind::Pixmap { .. });
                self.transform_handles.render(item, ui.painter(), &self.viewport, show_flip);
            }
        }

        // 多选统一外框（spec L241：多选时画一个统一 bbox）
        if selection_count > 1 {
            if let Some(bbox) = self.scene.selection_bounding_rect() {
                let screen_bbox = self.viewport.canvas_to_screen_rect(bbox);
                let stroke = egui::Stroke::new(1.5, egui::Color32::YELLOW);
                ui.painter().rect_stroke(screen_bbox, 0.0, stroke);
                // 4 角小方块标识
                let handle_size = TransformHandles::handle_size();
                let fill = egui::Color32::YELLOW;
                for p in [
                    screen_bbox.min,
                    egui::pos2(screen_bbox.max.x, screen_bbox.min.y),
                    egui::pos2(screen_bbox.min.x, screen_bbox.max.y),
                    screen_bbox.max,
                ] {
                    let r = egui::Rect::from_center_size(p, egui::Vec2::splat(handle_size));
                    ui.painter().rect_filled(r, egui::Rounding::same(1.0), fill);
                }
            }
        }
    }

    /// 测量所有 Text item 的实际文字尺寸并更新 `measured_size`（修 B6）。
    /// 仅在 `measured_size` 为 None 时测量（content 变化会清空 measured_size）。
    fn update_text_measured_sizes(&mut self, ctx: &egui::Context) {
        let mut updates: Vec<(ItemId, (f32, f32))> = Vec::new();
        for item in &self.scene.items {
            if let ItemKind::Text { content, font_size, measured_size, .. } = &item.kind {
                if measured_size.is_none() {
                    let gal = ctx.fonts(|fonts| {
                        fonts.layout_no_wrap(
                            content.clone(),
                            egui::FontId::proportional(*font_size),
                            egui::Color32::WHITE,
                        )
                    });
                    updates.push((item.id, (gal.size().x, gal.size().y)));
                }
            }
        }
        for (id, (w, h)) in updates {
            if let Some(item) = self.scene.get_item_mut(&id) {
                if let ItemKind::Text { measured_size, .. } = &mut item.kind {
                    *measured_size = Some((w, h));
                }
            }
        }
    }

    /// 渲染文本便签编辑 overlay（spec L243 P2-5）。
    /// 创建中的文本不在 scene 中；Enter/失焦时提交（非空→AddItem），Esc 取消。
    fn render_text_editor(&mut self, ctx: &egui::Context) {
        let mut edit = match self.editing_text.take() {
            Some(e) => e,
            None => return,
        };
        let screen_pos = self.viewport.canvas_to_screen(edit.canvas_pos);
        let mut commit = false;
        let mut cancel = false;

        egui::Area::new(egui::Id::new("text_edit_area"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen_pos)
            .show(ctx, |ui| {
                let frame = egui::Frame::popup(ui.style())
                    .fill(egui::Color32::from_rgb(50, 50, 50))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 200, 255)));
                frame.show(ui, |ui| {
                    ui.set_min_width(120.0);
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut edit.buffer)
                            .desired_width(160.0)
                            .hint_text("输入文本...")
                            .font(egui::FontId::proportional(edit.font_size))
                            .text_color(egui::Color32::from_rgba_premultiplied(
                                edit.color[0], edit.color[1], edit.color[2], edit.color[3],
                            )),
                    );
                    if edit.first_frame {
                        response.request_focus();
                        edit.first_frame = false;
                    }
                    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
                        cancel = true;
                    } else if response.lost_focus() {
                        commit = true;
                    }
                });
            });

        if cancel {
            self.editing_text = None;
            return;
        }
        if commit {
            match edit.editing_item_id {
                None => {
                    // 创建模式：空内容丢弃，非空 push AddItem
                    if !edit.buffer.trim().is_empty() {
                        let item = Item::new_text(
                            edit.buffer,
                            edit.canvas_pos.x,
                            edit.canvas_pos.y,
                            edit.font_size,
                            edit.color,
                        );
                        self.undo_stack.push(Box::new(AddItem::new(item)), &mut self.scene);
                        self.flash("已创建文本");
                    }
                }
                Some(id) => {
                    // 编辑模式：空内容不修改原 item（避免误删）；非空且变化才 push EditTextContent
                    if !edit.buffer.trim().is_empty() {
                        let old_content = self.scene.get_item(&id)
                            .and_then(|item| {
                                if let ItemKind::Text { content, .. } = &item.kind {
                                    Some(content.clone())
                                } else {
                                    None
                                }
                            });
                        if let Some(old) = old_content {
                            if old != edit.buffer {
                                let cmd = EditTextContent::new(id, old, edit.buffer);
                                self.undo_stack.push(Box::new(cmd), &mut self.scene);
                                self.flash("已更新文本");
                            }
                        }
                    }
                }
            }
            self.editing_text = None;
            return;
        }
        self.editing_text = Some(edit);
    }

    fn render_context_menu(&mut self, ctx: &egui::Context) {
        let menu_id = egui::Id::new("context_menu");
        let pos = self.context_menu_pos;
        let has_selection = !self.scene.selection.is_empty();
        let primary_pressed = ctx.input(|i| i.pointer.primary_pressed());
        let pointer_pos = ctx.input(|i| i.pointer.latest_pos());

        // 用 egui::Area + 手动按钮。返回菜单 rect 用于检测点击外部（修 B4）
        let area_response = egui::Area::new(menu_id)
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                let frame = egui::Frame::popup(&ui.style());
                frame.show(ui, |ui| {
                    ui.set_max_width(180.0);

                    if ui.button("\u{1F4C2} 打开...").clicked() {
                        self.queue_open_file();
                        self.context_menu_open = false;
                    }
                    if ui.button("\u{1F4BE} 保存...").clicked() {
                        self.save_file();
                        self.context_menu_open = false;
                    }
                    ui.separator();

                    if has_selection {
                        if ui.button("\u{1F5D1} 删除选中").clicked() {
                            self.delete_selected();
                            self.context_menu_open = false;
                        }
                        if ui.button("\u{2191} 置于顶层").clicked() {
                            self.bring_to_front();
                            self.context_menu_open = false;
                        }
                        if ui.button("\u{2193} 置于底层").clicked() {
                            self.send_to_back();
                            self.context_menu_open = false;
                        }
                        ui.separator();
                    }

                    if ui.button("\u{1F50D} 适应画布").clicked() {
                        self.fit_to_screen();
                        self.context_menu_open = false;
                    }
                    if ui.button("\u{1F504} 重置缩放").clicked() {
                        self.viewport.reset();
                        self.flash("重置缩放");
                        self.context_menu_open = false;
                    }
                });
                ui.min_rect()
            });

        // 点击菜单外部 → 关闭菜单（修 B4）
        if primary_pressed {
            if let Some(p) = pointer_pos {
                if !area_response.inner.contains(p) {
                    self.context_menu_open = false;
                }
            }
        }
    }

    fn render_debug_panel(&self, ctx: &egui::Context) {
        egui::Window::new("Debug")
            .default_pos(egui::Pos2::new(10.0, 10.0))
            .default_size(egui::Vec2::new(320.0, 220.0))
            .show(ctx, |ui| {
                ui.label(format!("Pointer: {:?}", ctx.input(|i| i.pointer.latest_pos())));
                ui.label(format!("Hover handle: {:?}", self.transform_handles.hover_handle));
                ui.label(format!("Active handle: {:?}", self.transform_handles.active_handle));
                ui.label(format!("Is dragging: {}", self.transform_handles.is_dragging));
                ui.label(format!("Drag state: {}", drag_state_name(&self.drag)));
                ui.label(format!("Selection: {} items", self.scene.selection.len()));
                ui.label(format!("Pan: ({:.1}, {:.1}) Zoom: {:.3}",
                    self.viewport.pan.x, self.viewport.pan.y, self.viewport.zoom));
                ui.separator();
                if let Some(id) = self.scene.selection.iter().next() {
                    if let Some(item) = self.scene.get_item(id) {
                        ui.label(format!("Selected item {}:", id));
                        ui.label(format!("  pos: ({:.1}, {:.1})", item.transform.pos.x, item.transform.pos.y));
                        ui.label(format!("  scale: ({:.2}, {:.2})", item.transform.scale.x, item.transform.scale.y));
                        ui.label(format!("  rotation: {:.2}°", item.transform.rotation.to_degrees()));
                        ui.label(format!("  z: {}", item.z));
                    }
                }
            });
    }
}

fn drag_state_name(d: &DragState) -> &'static str {
    match d {
        DragState::Idle => "Idle",
        DragState::HandleTransform { .. } => "HandleTransform",
        DragState::MoveItems { .. } => "MoveItems",
        DragState::BoxSelect { .. } => "BoxSelect",
    }
}

/// 缩放手柄：以拖拽角点的对角为锚点。
/// 数学：见 REVIEW 报告 P0-4。设 T(p)=pos+R(rot)*(scale∘F∘p)，F=flip 矩阵，
/// 对角点 a 和拖拽点 d：
///   R(rot)*(scale'*F*(d-a)) = mouse-a  →  scale' = R(-rot)*(mouse-a) ./ (F*(d-a))
///   pos' = a - R(rot)*(scale'*F*a)
/// 修 B3：加入 flip 矩阵 F，否则翻转后缩放方向错误导致跳跃。
/// 修 B2：free_scale=false 时默认等比缩放，Ctrl 自由缩放。
fn apply_scale_drag(
    item: &mut Item,
    handle: Handle,
    start_transform: preferz_core::Transform,
    start_corners: [CanvasPoint; 4],
    mouse_canvas: CanvasPoint,
    free_scale: bool,
) {
    let base = item.base_size();
    let base_w = base.x.max(1.0);
    let base_h = base.y.max(1.0);

    // corners = [TL, TR, BL, BR]，对角关系：0↔3, 1↔2
    let (anchor_idx, anchor_local, drag_local) = match handle {
        Handle::ResizeTopLeft     => (3, CanvasVector::new(base_w, base_h), CanvasVector::new(0.0, 0.0)),
        Handle::ResizeTopRight    => (2, CanvasVector::new(0.0,   base_h), CanvasVector::new(base_w, 0.0)),
        Handle::ResizeBottomLeft  => (1, CanvasVector::new(base_w, 0.0),   CanvasVector::new(0.0, base_h)),
        Handle::ResizeBottomRight => (0, CanvasVector::new(0.0,   0.0),   CanvasVector::new(base_w, base_h)),
        _ => return,
    };
    let anchor_canvas = start_corners[anchor_idx];
    let v = drag_local - anchor_local; // 局部空间对角向量，分量非 0
    let w = mouse_canvas - anchor_canvas; // 画布空间

    let rot = start_transform.rotation;
    let cos = rot.cos();
    let sin = rot.sin();
    // R(-rot) * w
    let w_local_x = cos * w.x + sin * w.y;
    let w_local_y = -sin * w.x + cos * w.y;

    // flip 因子（修 B3：缩放计算需除以 F*v 而非 v）
    let fx = if start_transform.flip_h { -1.0 } else { 1.0 };
    let fy = if start_transform.flip_v { -1.0 } else { 1.0 };

    let mut new_scale_x = w_local_x / (fx * v.x);
    let mut new_scale_y = w_local_y / (fy * v.y);

    // 等比缩放（修 B2：默认保持高宽比，Ctrl 自由缩放）
    if !free_scale {
        let start_sx = start_transform.scale.x.abs().max(0.05);
        let start_sy = start_transform.scale.y.abs().max(0.05);
        let ratio_x = new_scale_x / start_sx;
        let ratio_y = new_scale_y / start_sy;
        // 取变化幅度更大的方向作为统一缩放比
        let uniform_ratio = if ratio_x.abs() >= ratio_y.abs() { ratio_x } else { ratio_y };
        new_scale_x = start_sx * uniform_ratio;
        new_scale_y = start_sy * uniform_ratio;
    }

    // 最小尺寸限制（避免 0 / 负值）
    let min_scale = 0.05;
    new_scale_x = new_scale_x.max(min_scale);
    new_scale_y = new_scale_y.max(min_scale);

    // pos' = anchor_canvas - R(rot) * (new_scale * F * anchor_local)
    let sa_x = new_scale_x * fx * anchor_local.x;
    let sa_y = new_scale_y * fy * anchor_local.y;
    let r_x = cos * sa_x - sin * sa_y;
    let r_y = sin * sa_x + cos * sa_y;
    let new_pos = CanvasVector::new(anchor_canvas.x - r_x, anchor_canvas.y - r_y);

    item.transform.pos = new_pos;
    item.transform.scale = CanvasVector::new(new_scale_x, new_scale_y);
    // rotation / flip 不变
}

/// 旋转手柄：以拖拽前的 4 角点中心为锚点（不每帧重算，修 W6 漂移）。
fn apply_rotate_drag(
    viewport: &ViewportState,
    item: &mut Item,
    start_transform: preferz_core::Transform,
    start_corners: [CanvasPoint; 4],
    start_screen: egui::Pos2,
    current_screen: egui::Pos2,
) {
    let center_canvas = CanvasPoint::new(
        (start_corners[0].x + start_corners[1].x + start_corners[2].x + start_corners[3].x) * 0.25,
        (start_corners[0].y + start_corners[1].y + start_corners[2].y + start_corners[3].y) * 0.25,
    );
    let center_screen = viewport.canvas_to_screen(center_canvas);
    let start_angle = (start_screen.y - center_screen.y).atan2(start_screen.x - center_screen.x);
    let current_angle = (current_screen.y - center_screen.y).atan2(current_screen.x - center_screen.x);
    let delta = current_angle - start_angle;
    item.transform.rotation = start_transform.rotation + delta;
}

// ─────────────────────────── 操作 ───────────────────────────

impl PReferZApp {
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // 文本编辑中不处理场景快捷键（Esc 由 render_text_editor 处理）
        if self.editing_text.is_some() {
            return;
        }

        // Ctrl+Shift+P 显示菜单
        let show_menu = ctx.input(|i| {
            i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::P)
        });
        if show_menu && !self.context_menu_open {
            self.context_menu_open = true;
            self.context_menu_pos = ctx.input(|i| i.pointer.latest_pos()).unwrap_or_else(|| ctx.screen_rect().center());
        }

        // ESC 关闭菜单
        if self.context_menu_open && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.context_menu_open = false;
        }

        // Delete 删除选中（走命令）
        if ctx.input(|i| i.key_pressed(egui::Key::Delete)) && !self.scene.selection.is_empty() {
            self.delete_selected();
        }

        // Ctrl+Z 撤销
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Z) && !i.modifiers.shift) {
            if self.undo_stack.undo(&mut self.scene) {
                self.flash("撤销");
                ctx.request_repaint();
            }
        }

        // Ctrl+Y / Ctrl+Shift+Z 重做
        if ctx.input(|i| {
            i.modifiers.ctrl && (i.key_pressed(egui::Key::Y) || (i.key_pressed(egui::Key::Z) && i.modifiers.shift))
        }) {
            if self.undo_stack.redo(&mut self.scene) {
                self.flash("重做");
                ctx.request_repaint();
            }
        }

        // F 适应画布（替代原双击手势，双击已用于创建文本便签）
        if ctx.input(|i| i.key_pressed(egui::Key::F)) {
            self.fit_to_screen();
        }
    }

    fn queue_open_file(&mut self) {
        self.flash("打开文件...");
        // rfd 仍同步（P3-2 未做后台化，但至少包线程需更大重构）
        let picked = rfd::FileDialog::new()
            .add_filter("图片", &["png", "jpg", "jpeg", "gif", "bmp", "webp"])
            .pick_file();
        if let Some(path) = picked {
            self.pending_import.push(path);
        }
    }

    fn process_import(&mut self, ctx: &egui::Context, path: PathBuf) {
        // 注：此处仍同步解码（S10/P3-2 未做后台线程），作为 P0/P1 阶段的占位
        match image::open(&path) {
            Ok(image) => {
                let (width, height) = image.dimensions();
                let texture_id = self.next_texture_id;
                self.next_texture_id += 1;

                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [width as usize, height as usize],
                    &image.to_rgba8().into_vec(),
                );
                let texture_handle = ctx.load_texture(
                    format!("img_{}", texture_id),
                    color_image,
                    Default::default(),
                );
                self.texture_cache.insert(texture_id, texture_handle);

                // 初始位置：视口中心对应的画布点
                let center_canvas = self.viewport.screen_to_canvas(self.viewport.screen_rect.center());
                let item = Item::new_pixmap(
                    texture_id,
                    Some(path.to_string_lossy().to_string()),
                    (width, height),
                    center_canvas.x - width as f32 / 2.0,
                    center_canvas.y - height as f32 / 2.0,
                    1.0,
                    1.0,
                );

                // 走 AddItem 命令（修 S2/W8）
                let cmd = AddItem::new(item);
                self.undo_stack.push(Box::new(cmd), &mut self.scene);
                self.flash(format!("已导入: {}", path.display()));
                ctx.request_repaint();
            }
            Err(e) => {
                self.flash(format!("加载失败: {}", e));
            }
        }
    }

    fn save_file(&mut self) {
        self.flash("保存文件...");
        let _ = rfd::FileDialog::new()
            .add_filter("PReferZ", &["prz"])
            .save_file();
        // Phase 4 才实现真正的 .prz 序列化
    }

    fn delete_selected(&mut self) {
        let ids: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
        if ids.is_empty() {
            return;
        }
        // 清理纹理缓存（Pixmap）
        for id in &ids {
            if let Some(item) = self.scene.get_item(id) {
                if let ItemKind::Pixmap { texture_id, .. } = &item.kind {
                    self.texture_cache.remove(texture_id);
                }
            }
        }
        // 走 DeleteItems 命令（修 S1/W8），undo 已支持快照恢复（P1-3）
        let cmd = DeleteItems::new(ids.clone());
        self.undo_stack.push(Box::new(cmd), &mut self.scene);
        self.scene.selection.clear();
        self.flash(format!("已删除 {} 项", ids.len()));
    }

    fn bring_to_front(&mut self) {
        let ids: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
        if ids.is_empty() {
            return;
        }
        // 走 ReorderItems 命令（修 S3/W8），不再直接写 z
        let cmd = ReorderItems::new(ids, true);
        self.undo_stack.push(Box::new(cmd), &mut self.scene);
        self.flash("置于顶层");
    }

    fn send_to_back(&mut self) {
        let ids: Vec<ItemId> = self.scene.selection.iter().cloned().collect();
        if ids.is_empty() {
            return;
        }
        let cmd = ReorderItems::new(ids, false);
        self.undo_stack.push(Box::new(cmd), &mut self.scene);
        self.flash("置于底层");
    }

    fn fit_to_screen(&mut self) {
        if self.scene.items.is_empty() {
            self.viewport.reset();
            self.flash("适应画布");
            return;
        }
        // 用所有 item 的 AABB 并集
        let mut bbox: Option<preferz_core::spaces::CanvasRect> = None;
        for item in &self.scene.items {
            let r = item.bounding_rect();
            bbox = Some(match bbox {
                Some(b) => b.union(&r),
                None => r,
            });
        }
        if let Some(b) = bbox {
            self.viewport.fit_to_content(b);
            self.flash("适应画布");
        }
    }
}
