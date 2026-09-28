//! props — 从 preferz_app 拆出的方法段（架构整固 Step 3c-ii）。逐字搬迁，`use super::*;` 够到 mod.rs 词汇/私有项。
use super::*;

/// 单端箭头样式选择器：五个小图标（无 / 箭头 / 三角 / 空心三角 / 圆点），
/// 悬停显示样式名。点击且与当前值不同时返回 `Some(新值)`；`None` 是合法值（无箭头）。
fn arrowhead_selector(
    ui: &mut egui::Ui,
    lang: Lang,
    current: Option<ArrowHeadStyle>,
) -> Option<Option<ArrowHeadStyle>> {
    let options: [(Option<ArrowHeadStyle>, T); 5] = [
        (None, T::StyleArrowNone),
        (Some(ArrowHeadStyle::Arrow), T::StyleArrowArrow),
        (Some(ArrowHeadStyle::Triangle), T::StyleArrowTriangle),
        (
            Some(ArrowHeadStyle::TriangleOutline),
            T::StyleArrowTriangleOutline,
        ),
        (Some(ArrowHeadStyle::Dot), T::StyleArrowDot),
    ];
    let mut picked = None;
    ui.horizontal(|ui| {
        for (value, hint) in options {
            if arrowhead_icon(ui, current == value, value, t(lang, hint)) && current != value {
                picked = Some(value);
            }
        }
    });
    picked
}

/// 画一个「线段 + 端头」小图标（与画布渲染同参数：头长 10px、半张角 30°），
/// 选中/悬停时加高亮底色，返回是否被点击。
fn arrowhead_icon(
    ui: &mut egui::Ui,
    selected: bool,
    style: Option<ArrowHeadStyle>,
    tooltip: &str,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(28.0, 18.0), egui::Sense::click());
    let bg = if selected {
        Some(ui.visuals().selection.bg_fill)
    } else if resp.hovered() {
        Some(ui.visuals().widgets.hovered.bg_fill)
    } else {
        None
    };
    if let Some(bg) = bg {
        ui.painter().rect_filled(rect.expand(2.0), 3.0, bg);
    }
    let color = ui.visuals().text_color();
    let stroke = egui::Stroke::new(1.6, color);
    let cy = rect.center().y;
    let tip = egui::pos2(rect.right() - 3.0, cy);
    // 一小段线示意「这是端头」
    ui.painter()
        .line_segment([egui::pos2(rect.left() + 3.0, cy), tip], stroke);
    let head_len = 10.0;
    let (sin, cos) = ArrowHeadStyle::HALF_ANGLE.sin_cos();
    let b1 = tip - egui::vec2(head_len * cos, head_len * sin);
    let b2 = tip - egui::vec2(head_len * cos, -head_len * sin);
    match style {
        None => {}
        Some(ArrowHeadStyle::Arrow) => {
            ui.painter().line_segment([tip, b1], stroke);
            ui.painter().line_segment([tip, b2], stroke);
        }
        Some(ArrowHeadStyle::Triangle) => {
            ui.painter().add(egui::Shape::Path(egui::epaint::PathShape {
                points: vec![tip, b1, b2],
                closed: true,
                fill: color,
                stroke: egui::epaint::PathStroke::NONE,
            }));
        }
        Some(ArrowHeadStyle::TriangleOutline) => {
            ui.painter().add(egui::Shape::Path(egui::epaint::PathShape {
                points: vec![tip, b1, b2],
                closed: true,
                fill: egui::Color32::TRANSPARENT,
                stroke: stroke.into(),
            }));
        }
        Some(ArrowHeadStyle::Dot) => {
            // 实心圆点，相切于端点（圆心向线段内缩一个半径）
            let radius = 4.0;
            ui.painter()
                .circle_filled(tip - egui::vec2(radius, 0.0), radius, color);
        }
    }
    let clicked = resp.clicked();
    resp.on_hover_text(tooltip);
    clicked
}

/// 从 Shape 的 `ItemKind` 抽填充快照（颜色 + 样式 + 是否跟随描边），供 `apply_continuous`
/// 的 snap 端复用（fn item 可多次传入，闭包则会被 move）。
fn fill_snap(k: &ItemKind) -> Option<PropValue> {
    match k {
        ItemKind::Shape {
            fill,
            fill_style,
            fill_follow_stroke,
            ..
        } => Some(PropValue::Fill(FillState {
            color: *fill,
            style: *fill_style,
            follow_stroke: *fill_follow_stroke,
        })),
        _ => None,
    }
}

impl PReferZApp {
    /// 右侧属性侧栏（Phase H）：选中项的 per-item 编辑，按 ItemKind 分节。
    ///
    /// D3 语义（见 [`prop`]）：多选时显示交集值；值不一致仍显示代表值，但改动
    /// 批量应用到所有选中项。连续控件（滑块 / 取色器）经 [`PropEdit`] 合并成一条 undo 命令。
    pub(crate) fn render_props_panel(&mut self, ctx: &egui::Context) {
        // 始终渲染（anim=0 时偏移到屏外不可见，不拦截事件）；避免 Area 首帧闪现。
        if self.scene.selection.is_empty() {
            // 无选中且绘制工具激活：右侧栏显示「新建元素默认样式」
            // （原底部样式面板移入侧栏；Frame 无样式可调，不显示）。
            if self.tool != Tool::Select && self.tool != Tool::Frame {
                self.render_defaults_panel(ctx);
            }
            return;
        }
        let ids: Vec<ItemId> = self.scene.selection.iter().copied().collect();
        let lang = self.lang;
        let dark = self.theme.is_dark(ctx);
        // 指数缓动 anim 直接映射偏移（不套 smoothstep，避免端点导数≈0 导致贴边卡顿）。
        // 滑出距离需覆盖 Area 实际宽度（content + 两侧 inner_margin + stroke + 余量），
        // 否则 anim=0 时面板边缘残留在屏内。
        let slide_dist = chrome::PROPS_BAR_WIDTH
            + 2.0 * chrome::BAR_INNER_MARGIN as f32
            + chrome::BAR_MARGIN
            + 32.0;
        let offset_x = -chrome::BAR_MARGIN + (1.0 - self.props_anim) * slide_dist;
        egui::Area::new(egui::Id::new("props_panel"))
            .anchor(
                egui::Align2::RIGHT_TOP,
                egui::vec2(offset_x, chrome::BAR_MARGIN),
            )
            .order(egui::Order::Foreground)
            .constrain(false)
            .interactable(true)
            .show(ctx, |ui| {
                chrome::floating_bar_frame(ui.style()).show(ui, |ui| {
                    ui.set_min_width(chrome::PROPS_BAR_WIDTH);
                    // 无滚动：bar 高度自适应内容，完整显示所有选项（用户要求 2026-09-22）。
                    ui.label(fill(
                        t(lang, T::PropsSelectedCount),
                        &[ids.len().to_string()],
                    ));
                    ui.separator();

                            // 叠放顺序：Frame 不参与，选区含非 Frame item 时显示
                            let has_reorderable = ids.iter().any(|id| {
                                !self.scene.get_item(id).is_some_and(|i| i.is_frame())
                            });
                            if has_reorderable {
                                ui.label(t(lang, T::PropsSectionZOrder));
                                self.render_zorder_section(ui, lang);
                                ui.separator();
                            }

                            // 对齐 / 分布（plan #6）：≥2 项才有意义，放在各类型节之前（对所有类型通用）
                            if ids.len() >= 2 {
                                ui.label(t(lang, T::PropsSectionAlign));
                                self.render_align_section(ui, lang, &ids);
                                ui.separator();
                            }

                            let shape_ids: Vec<ItemId> = ids
                                .iter()
                                .copied()
                                .filter(|id| {
                                    matches!(self.scene.get_item(id), Some(item) if matches!(item.kind, ItemKind::Shape { .. }))
                                })
                                .collect();
                            if !shape_ids.is_empty() {
                                ui.label(t(lang, T::PropsSectionShape));
                                self.render_shape_props(ui, lang, dark, &shape_ids);
                                ui.separator();
                            }

                            let freedraw_ids: Vec<ItemId> = ids
                                .iter()
                                .copied()
                                .filter(|id| {
                                    matches!(self.scene.get_item(id), Some(item) if matches!(item.kind, ItemKind::Freedraw { .. }))
                                })
                                .collect();
                            if !freedraw_ids.is_empty() {
                                ui.label(t(lang, T::PropsSectionFreedraw));
                                self.render_freedraw_props(ui, lang, dark, &freedraw_ids);
                                ui.separator();
                            }

                            let mut text_ids: Vec<ItemId> = ids
                                .iter()
                                .copied()
                                .filter(|id| {
                                    matches!(self.scene.get_item(id), Some(item) if matches!(item.kind, ItemKind::Text { .. }))
                                })
                                .collect();
                            // 选中图形时把其绑定文字纳入文字节（Excalidraw 同款入口）：
                            // 绑定文字不可独立选中，此前选中图形时侧栏无任何文字样式控件
                            //（用户反馈"看不到文字对齐选项 / 文字样式不随主体改"）。
                            for id in &ids {
                                if matches!(
                                    self.scene.get_item(id),
                                    Some(item) if matches!(item.kind, ItemKind::Shape { .. })
                                ) {
                                    for item in &self.scene.items {
                                        if let ItemKind::Text {
                                            container_id: Some(cid),
                                            ..
                                        } = &item.kind
                                        {
                                            if cid == id && !text_ids.contains(&item.id) {
                                                text_ids.push(item.id);
                                            }
                                        }
                                    }
                                }
                            }
                            if !text_ids.is_empty() {
                                ui.label(t(lang, T::PropsSectionText));
                                self.render_text_props(ui, lang, dark, &text_ids);
                                ui.separator();
                            }

                            let pixmap_ids: Vec<ItemId> = ids
                                .iter()
                                .copied()
                                .filter(|id| {
                                    matches!(self.scene.get_item(id), Some(item) if matches!(item.kind, ItemKind::Pixmap { .. }))
                                })
                                .collect();
                            if !pixmap_ids.is_empty() {
                                ui.label(t(lang, T::PropsSectionPixmap));
                                self.render_pixmap_props(ui, lang, &pixmap_ids);
                                ui.separator();
                            }

                            let frame_ids: Vec<ItemId> = ids
                                .iter()
                                .copied()
                                .filter(|id| {
                                    matches!(self.scene.get_item(id), Some(item) if matches!(item.kind, ItemKind::Frame { .. }))
                                })
                                .collect();
                            if !frame_ids.is_empty() {
                                ui.label(t(lang, T::PropsSectionFrame));
                                self.render_frame_props(ui, lang, &frame_ids);
                            }

                });
            });
    }

    /// 叠放顺序按钮组：上移一层 / 置于顶层 / 下移一层 / 置于底层。
    ///
    /// Frame 不参与（调用前已过滤）。不显示 z 数值（z 是实现细节，对用户无意义）。
    fn render_zorder_section(&mut self, ui: &mut egui::Ui, lang: Lang) {
        ui.horizontal(|ui| {
            const BUTTONS: [(&str, T); 4] = [
                ("\u{2191}", T::MoveForward),
                ("\u{23EB}", T::BringToFront),
                ("\u{2193}", T::MoveBackward),
                ("\u{23EC}", T::SendToBack),
            ];
            for (icon, key) in BUTTONS {
                let btn = egui::Button::new(icon).min_size(egui::vec2(34.0, 24.0));
                if ui.add(btn).on_hover_text(t(lang, key)).clicked() {
                    match key {
                        T::MoveForward => self.move_forward(),
                        T::BringToFront => self.bring_to_front(),
                        T::MoveBackward => self.move_backward(),
                        T::SendToBack => self.send_to_back(),
                        _ => {}
                    }
                }
            }
        });
    }

    /// 对齐 / 分布按钮组（plan #6）：属性栏顶部，选中 ≥2 项时显示。
    ///
    /// 参考系为**选区包围盒**（见 `plan_align` / `plan_distribute`）：对齐贴向该框的
    /// 对应边/中线，分布保持首尾元素不动、只调整中间项。分布要求 ≥3 项，不足时禁用。
    pub(crate) fn render_align_section(&mut self, ui: &mut egui::Ui, lang: Lang, ids: &[ItemId]) {
        // 6 向对齐：两行（水平 3 个 + 垂直 3 个），图标按钮 + tooltip 说明。
        // 图标必须是内嵌思源黑体**已有字形**的字符：⇤⇥⇡⇣（U+21E1/21E3/21E4/21E5）
        // 不在字体字符集里且无 emoji 回落，会渲染成豆腐块（2026-09-22 修复）。
        const ALIGNS: [(AlignMode, &str, T); 6] = [
            (AlignMode::Left, "\u{2190}", T::AlignLeft),       // ←
            (AlignMode::HCenter, "\u{2194}", T::AlignHCenter), // ↔
            (AlignMode::Right, "\u{2192}", T::AlignRight),     // →
            (AlignMode::Top, "\u{2191}", T::AlignTop),         // ↑
            (AlignMode::VCenter, "\u{2195}", T::AlignVCenter), // ↕
            (AlignMode::Bottom, "\u{2193}", T::AlignBottom),   // ↓
        ];
        for row in ALIGNS.chunks(3) {
            ui.horizontal(|ui| {
                for (mode, icon, key) in row {
                    let btn = egui::Button::new(*icon).min_size(egui::vec2(34.0, 24.0));
                    if ui.add(btn).on_hover_text(t(lang, *key)).clicked() {
                        self.align_selected(*mode);
                    }
                }
            });
        }

        // 分布：轴向 × 基准 = 4 个（等距 = 边界空隙相等；等心 = 中心距相等）
        ui.add_space(2.0);
        ui.label(t(lang, T::Distribute));
        let can_distribute = ids.len() >= 3;
        const DISTRIBUTES: [(DistributeAxis, DistributeMode, T); 4] = [
            (
                DistributeAxis::Horizontal,
                DistributeMode::Gap,
                T::DistributeHGap,
            ),
            (
                DistributeAxis::Horizontal,
                DistributeMode::Centers,
                T::DistributeHCenters,
            ),
            (
                DistributeAxis::Vertical,
                DistributeMode::Gap,
                T::DistributeVGap,
            ),
            (
                DistributeAxis::Vertical,
                DistributeMode::Centers,
                T::DistributeVCenters,
            ),
        ];
        for row in DISTRIBUTES.chunks(2) {
            ui.horizontal(|ui| {
                for (axis, mode, key) in row {
                    let resp = ui.add_enabled(can_distribute, egui::Button::new(t(lang, *key)));
                    if resp.clicked() {
                        self.distribute_selected(*axis, *mode);
                    } else if !can_distribute {
                        resp.on_disabled_hover_text(t(lang, T::FlashDistributeNeedThree));
                    }
                }
            });
        }
    }

    /// 「新建元素默认样式」侧栏：绘制工具激活且无选中时显示（原底部样式面板）。
    ///
    /// 控件直接改 `default_*` 字段（非 item 属性，不走 undo 栈）；
    /// 填充节仅对能产生封闭图形的工具显示（线 / 箭头无填充）。
    pub(crate) fn render_defaults_panel(&mut self, ctx: &egui::Context) {
        let lang = self.lang;
        let dark = self.theme.is_dark(ctx);
        let show_fill = matches!(self.tool, Tool::Shape(_) | Tool::Polygon);
        // 与 render_props_panel 共用 props_anim 滑入滑出（直接线性映射，不套 smoothstep）。
        let slide_dist = chrome::PROPS_BAR_WIDTH
            + 2.0 * chrome::BAR_INNER_MARGIN as f32
            + chrome::BAR_MARGIN
            + 32.0;
        let offset_x = -chrome::BAR_MARGIN + (1.0 - self.props_anim) * slide_dist;
        egui::Area::new(egui::Id::new("defaults_panel"))
            .anchor(
                egui::Align2::RIGHT_TOP,
                egui::vec2(offset_x, chrome::BAR_MARGIN),
            )
            .order(egui::Order::Foreground)
            .constrain(false)
            .interactable(true)
            .show(ctx, |ui| {
                chrome::floating_bar_frame(ui.style()).show(ui, |ui| {
                    ui.set_min_width(chrome::PROPS_BAR_WIDTH);
                    // 无滚动：bar 高度自适应内容，完整显示所有选项（用户要求 2026-09-22）。
                    ui.label(t(lang, T::PropsDefaultsTitle));
                    ui.separator();
                    // 描边颜色（Excalidraw 式调色板）
                    let mut stroke = self.default_stroke.color;
                    if palette::color_palette_button(ui, &mut stroke, dark, lang) {
                        self.default_stroke.color = stroke;
                    }
                    ui.add_space(4.0);
                    // 描边宽度
                    ui.label(t(lang, T::StyleStrokeWidth));
                    stepper(
                        ui,
                        &mut self.default_stroke.width,
                        &[1.0, 2.0, 4.0, 8.0, 16.0],
                        &["XS", "S", "M", "L", "XL"],
                        1.0..=64.0,
                        None,
                    );
                    // 线型
                    ui.label(t(lang, T::StyleDashLabel));
                    ui.horizontal(|ui| {
                        for (dash, label) in [
                            (DashStyle::Solid, T::StyleDashSolid),
                            (DashStyle::Dashed, T::StyleDashDashed),
                            (DashStyle::Dotted, T::StyleDashDotted),
                        ] {
                            let active = self.default_stroke.dash == dash;
                            if ui.selectable_label(active, t(lang, label)).clicked() {
                                self.default_stroke.dash = dash;
                            }
                        }
                    });
                    // 填充（闭合图形类工具）
                    if show_fill {
                        ui.add_space(4.0);
                        ui.label(t(lang, T::StyleFillLabel));
                        ui.horizontal(|ui| {
                            if let Some(new_style) =
                                palette::fill_style_picker(ui, lang, self.default_fill_style)
                            {
                                self.default_fill_style = new_style;
                            }
                        });
                        if self.default_fill_style.is_some() {
                            // 未显式选过填充色时按钮显示描边色（实际创建时同样跟随描边色）
                            let mut fill = self.default_fill.unwrap_or(self.default_stroke.color);
                            if palette::fill_color_palette_button(ui, &mut fill, dark, lang) {
                                self.default_fill = Some(fill);
                            }
                        }
                    }
                    // 手绘风：新建形状的默认档位（plan #3，对齐 Excalidraw sloppiness）
                    ui.add_space(4.0);
                    ui.label(t(lang, T::StyleRough));
                    ui.horizontal(|ui| {
                        let opts = [
                            (Sloppiness::Off, T::SloppinessOff),
                            (Sloppiness::Architect, T::SloppinessArchitect),
                            (Sloppiness::Artist, T::SloppinessArtist),
                            (Sloppiness::Cartoonist, T::SloppinessCartoonist),
                        ];
                        for (val, label) in opts {
                            let selected = self.default_sloppiness == val;
                            if ui.selectable_label(selected, t(lang, label)).clicked() && !selected
                            {
                                self.default_sloppiness = val;
                            }
                        }
                    });
                });
            });
    }

    /// 墨迹节（plan #10）：颜色 + 基准笔宽。逐点速度锥形形状不可在侧栏编辑（属绘制
    /// 结果），故只提供这两个通用样式；线型 / 填充 / 圆角等对墨迹无意义，不出现。
    pub(crate) fn render_freedraw_props(
        &mut self,
        ui: &mut egui::Ui,
        lang: Lang,
        dark: bool,
        ids: &[ItemId],
    ) {
        // 颜色（与 Shape 描边同调色板）
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Freedraw { color, .. } => Some(*color),
            _ => None,
        }) {
            let mut col = p.value();
            if palette::color_palette_button(ui, &mut col, dark, lang) {
                let new = col;
                self.apply_continuous(
                    ids,
                    PropKind::Freedraw,
                    |k| match k {
                        ItemKind::Freedraw {
                            color,
                            stroke_width,
                            ..
                        } => Some(PropValue::Freedraw(FreedrawStyle {
                            color: *color,
                            stroke_width: *stroke_width,
                        })),
                        _ => None,
                    },
                    |k| {
                        if let ItemKind::Freedraw { color, .. } = k {
                            *color = new;
                        }
                    },
                );
            }
            if p.is_mixed() {
                ui.label(t(lang, T::PropsMixedValue));
            }
        }
        // 粗细（基准笔宽；逐点相对乘子保持不变，整条墨迹等比缩放）
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Freedraw { stroke_width, .. } => Some(*stroke_width),
            _ => None,
        }) {
            ui.label(t(lang, T::StyleStrokeWidth));
            let mut w = p.value();
            if stepper(
                ui,
                &mut w,
                &[1.0, 2.0, 4.0, 8.0, 16.0],
                &["XS", "S", "M", "L", "XL"],
                1.0..=64.0,
                None,
            ) {
                self.apply_continuous(
                    ids,
                    PropKind::Freedraw,
                    |k| match k {
                        ItemKind::Freedraw {
                            color,
                            stroke_width,
                            ..
                        } => Some(PropValue::Freedraw(FreedrawStyle {
                            color: *color,
                            stroke_width: *stroke_width,
                        })),
                        _ => None,
                    },
                    |k| {
                        if let ItemKind::Freedraw { stroke_width, .. } = k {
                            *stroke_width = w;
                        }
                    },
                );
            }
            if p.is_mixed() {
                ui.label(t(lang, T::PropsMixedValue));
            }
        }
    }

    /// 形状节：描边 / 填充 / 圆角 / 曲线 / 闭合 / 箭头 / 手绘风（Phase H）。
    pub(crate) fn render_shape_props(
        &mut self,
        ui: &mut egui::Ui,
        lang: Lang,
        dark: bool,
        ids: &[ItemId],
    ) {
        // 描边颜色（Excalidraw 式调色板）
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Shape { stroke, .. } => Some(stroke.color),
            _ => None,
        }) {
            let mut col = p.value();
            if palette::color_palette_button(ui, &mut col, dark, lang) {
                let new = col;
                self.apply_continuous(
                    ids,
                    PropKind::Stroke,
                    |k| match k {
                        ItemKind::Shape { stroke, .. } => Some(PropValue::Stroke(*stroke)),
                        _ => None,
                    },
                    |k| {
                        if let ItemKind::Shape { stroke, .. } = k {
                            stroke.color = new;
                        }
                    },
                );
                // 联动（用户拍板 2026-09-10）：改描边色时填充色跟随（保留 alpha）、
                // 绑定文字色跟随； undo 由 prop_cmd 打包成一条 MultiCommand。
                self.sync_stroke_color_side_effects(ids, new);
            }
            if p.is_mixed() {
                ui.label(t(lang, T::PropsMixedValue));
            }
        }
        // 描边宽度
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Shape { stroke, .. } => Some(stroke.width),
            _ => None,
        }) {
            ui.label(t(lang, T::StyleStrokeWidth));
            let mut w = p.value();
            if stepper(
                ui,
                &mut w,
                &[1.0, 2.0, 4.0, 8.0, 16.0],
                &["XS", "S", "M", "L", "XL"],
                1.0..=64.0,
                None,
            ) {
                self.apply_continuous(
                    ids,
                    PropKind::Stroke,
                    |k| match k {
                        ItemKind::Shape { stroke, .. } => Some(PropValue::Stroke(*stroke)),
                        _ => None,
                    },
                    |k| {
                        if let ItemKind::Shape { stroke, .. } = k {
                            stroke.width = w;
                        }
                    },
                );
            }
            if p.is_mixed() {
                ui.label(t(lang, T::PropsMixedValue));
            }
        }
        // 线型
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Shape { stroke, .. } => Some(stroke.dash),
            _ => None,
        }) {
            ui.label(t(lang, T::StyleDashLabel));
            ui.horizontal(|ui| {
                for (dash, label) in [
                    (DashStyle::Solid, T::StyleDashSolid),
                    (DashStyle::Dashed, T::StyleDashDashed),
                    (DashStyle::Dotted, T::StyleDashDotted),
                ] {
                    let active = p.value() == dash;
                    if ui.selectable_label(active, t(lang, label)).clicked() {
                        self.apply_continuous(
                            ids,
                            PropKind::Stroke,
                            |k| match k {
                                ItemKind::Shape { stroke, .. } => Some(PropValue::Stroke(*stroke)),
                                _ => None,
                            },
                            |k| {
                                if let ItemKind::Shape { stroke, .. } = k {
                                    stroke.dash = dash;
                                }
                            },
                        );
                    }
                }
            });
        }
        // 填充（Excalidraw 四态：无 / 纯色 / 斜线 / 交叉线）。
        // 线 / 箭头（未闭合 Polyline）无填充节；闭合折线（多边形）有。
        let fill_ids: Vec<ItemId> = ids
            .iter()
            .copied()
            .filter(|id| {
                matches!(self.scene.get_item(id), Some(item) if match &item.kind {
                    ItemKind::Shape { shape_type, closed, .. } => {
                        !matches!(shape_type, ShapeType::Polyline) || *closed
                    }
                    _ => false,
                })
            })
            .collect();
        if let Some(p) = prop(&self.scene, &fill_ids, |it| match &it.kind {
            ItemKind::Shape {
                fill,
                fill_style,
                fill_follow_stroke,
                ..
            } => Some(FillState {
                color: *fill,
                style: *fill_style,
                follow_stroke: *fill_follow_stroke,
            }),
            _ => None,
        }) {
            let state = p.value();
            let current_style = if state.color.is_some() {
                Some(state.style)
            } else {
                None
            };
            ui.label(t(lang, T::StyleFillLabel));
            if let Some(new_style) = palette::fill_style_picker(ui, lang, current_style) {
                self.apply_continuous(&fill_ids, PropKind::Fill, fill_snap, |k| {
                    if let ItemKind::Shape {
                        fill,
                        fill_style,
                        fill_follow_stroke,
                        stroke,
                        ..
                    } = k
                    {
                        match new_style {
                            Some(s) => {
                                *fill_style = s;
                                // 无填充 → 有填充：默认跟随描边色（Excalidraw 语义），
                                // 套默认 50% 不透明度（plan #2）
                                if fill.is_none() {
                                    let c = stroke.color;
                                    *fill = Some([c[0], c[1], c[2], palette::FILL_DEFAULT_ALPHA]);
                                    *fill_follow_stroke = true;
                                }
                            }
                            None => {
                                *fill = None;
                                *fill_style = FillStyle::Solid;
                            }
                        }
                    }
                });
            }
            // 填充颜色（仅有填充时显示；带「跟随形状」格，plan #2 与描边共用 5 色）
            if let Some(c) = state.color {
                // 边框色：跟随格底色 + 开启跟随时吸附目标（取代表描边色）
                let border = prop(&self.scene, &fill_ids, |it| match &it.kind {
                    ItemKind::Shape { stroke, .. } => Some(stroke.color),
                    _ => None,
                })
                .map(|pp| pp.value())
                .unwrap_or([0, 0, 0, 255]);
                let border_c = egui::Color32::from_rgba_unmultiplied(
                    border[0], border[1], border[2], border[3],
                );
                match palette::fill_color_palette_button_follow(
                    ui,
                    &c,
                    dark,
                    lang,
                    palette::FollowCtx {
                        on: state.follow_stroke,
                        border: border_c,
                    },
                ) {
                    palette::ColorPick::Color(new) => {
                        self.apply_continuous(&fill_ids, PropKind::Fill, fill_snap, |k| {
                            if let ItemKind::Shape {
                                fill,
                                fill_follow_stroke,
                                ..
                            } = k
                            {
                                *fill = Some(new);
                                *fill_follow_stroke = false;
                            }
                        });
                    }
                    palette::ColorPick::Follow(true) => {
                        // 开启跟随：填充 RGB 吸附到各自描边色（保留 alpha）
                        self.apply_continuous(&fill_ids, PropKind::Fill, fill_snap, |k| {
                            if let ItemKind::Shape {
                                fill,
                                stroke,
                                fill_follow_stroke,
                                ..
                            } = k
                            {
                                *fill_follow_stroke = true;
                                if let Some(f) = fill {
                                    let a = f[3];
                                    let s = stroke.color;
                                    *f = [s[0], s[1], s[2], a];
                                }
                            }
                        });
                    }
                    palette::ColorPick::Follow(false) => {
                        self.apply_continuous(&fill_ids, PropKind::Fill, fill_snap, |k| {
                            if let ItemKind::Shape {
                                fill_follow_stroke, ..
                            } = k
                            {
                                *fill_follow_stroke = false;
                            }
                        });
                    }
                    palette::ColorPick::None => {}
                }
                // 不透明度（plan #2）：5 档 + 输入框，仅改 alpha 保 RGB
                let mut pct = (c[3] as f32 / 255.0 * 100.0).round();
                ui.label(t(lang, T::StyleFillOpacity));
                let op_labels = [
                    t(lang, T::OpMin),
                    t(lang, T::OpLow),
                    t(lang, T::OpMed),
                    t(lang, T::OpHigh),
                    t(lang, T::OpMax),
                ];
                if stepper(
                    ui,
                    &mut pct,
                    &[15.0, 30.0, 50.0, 75.0, 100.0],
                    &op_labels,
                    0.0..=100.0,
                    Some("%"),
                ) {
                    let a = (pct / 100.0 * 255.0).round() as u8;
                    self.apply_continuous(&fill_ids, PropKind::Fill, fill_snap, |k| {
                        if let ItemKind::Shape { fill: Some(f), .. } = k {
                            f[3] = a;
                        }
                    });
                }
            }
        }
        // 圆角（矩形族 + elbow 折线，plan #23：共用 roundness 字段与半径语义）
        let roundable_ids: Vec<ItemId> = ids
            .iter()
            .copied()
            .filter(|id| {
                matches!(self.scene.get_item(id), Some(item) if matches!(&item.kind,
                    ItemKind::Shape { shape_type: ShapeType::Rectangle, .. }
                    | ItemKind::Shape { shape_type: ShapeType::Polyline, curve_type: CurveType::Elbow, .. })
                )
            })
            .collect();
        if !roundable_ids.is_empty() {
            if let Some(p) = prop(&self.scene, &roundable_ids, |it| match &it.kind {
                ItemKind::Shape { roundness, .. } => Some(*roundness),
                _ => None,
            }) {
                ui.label(t(lang, T::StyleRoundness));
                let mut r = p.value();
                if stepper(
                    ui,
                    &mut r,
                    &[0.0, 0.25, 0.5, 0.75, 1.0],
                    &["None", "S", "M", "L", "XL"],
                    0.0..=1.0,
                    None,
                ) {
                    self.apply_continuous(
                        &roundable_ids,
                        PropKind::Roundness,
                        |k| match k {
                            ItemKind::Shape { roundness, .. } => Some(PropValue::Float(*roundness)),
                            _ => None,
                        },
                        |k| {
                            if let ItemKind::Shape { roundness, .. } = k {
                                *roundness = r;
                            }
                        },
                    );
                }
                if p.is_mixed() {
                    ui.label(t(lang, T::PropsMixedValue));
                }
            }
        }
        // 手绘风档位（plan #3，离散四档，整批一条命令）
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Shape { sloppiness, .. } => Some(*sloppiness),
            _ => None,
        }) {
            let current = p.value();
            let opts = [
                (Sloppiness::Off, T::SloppinessOff),
                (Sloppiness::Architect, T::SloppinessArchitect),
                (Sloppiness::Artist, T::SloppinessArtist),
                (Sloppiness::Cartoonist, T::SloppinessCartoonist),
            ];
            ui.label(t(lang, T::StyleRough));
            ui.horizontal(|ui| {
                for (val, label) in opts {
                    let selected = current == val;
                    if ui.selectable_label(selected, t(lang, label)).clicked() && !selected {
                        let items: Vec<(ItemId, Sloppiness, Sloppiness)> = ids
                            .iter()
                            .filter_map(|id| {
                                self.scene.get_item(id).and_then(|it| match &it.kind {
                                    ItemKind::Shape { sloppiness, .. } => {
                                        Some((*id, *sloppiness, val))
                                    }
                                    _ => None,
                                })
                            })
                            .collect();
                        if !items.is_empty() {
                            self.push_cmd(Box::new(SetSloppiness::new_batch(items)));
                        }
                    }
                }
            });
        }
        // 线性对象专用：曲线 / 闭合 / 箭头
        let poly_ids: Vec<ItemId> = ids
            .iter()
            .copied()
            .filter(|id| {
                matches!(self.scene.get_item(id), Some(item) if matches!(item.kind, ItemKind::Shape { shape_type: ShapeType::Polyline, .. }))
            })
            .collect();
        if !poly_ids.is_empty() {
            if let Some(p) = prop(&self.scene, &poly_ids, |it| match &it.kind {
                ItemKind::Shape { curve_type, .. } => Some(*curve_type),
                _ => None,
            }) {
                let cur = p.value();
                ui.label(t(lang, T::StyleCurve));
                ui.horizontal(|ui| {
                    for (variant, label) in [
                        (CurveType::Straight, t(lang, T::StyleCurveStraight)),
                        (CurveType::Curved, t(lang, T::StyleCurveCurved)),
                        (CurveType::Elbow, t(lang, T::StyleCurveElbow)),
                    ] {
                        if ui.selectable_label(cur == variant, label).clicked() && cur != variant {
                            self.push_poly_curve(&poly_ids, variant);
                        }
                    }
                });
            }
            if let Some(p) = prop(&self.scene, &poly_ids, |it| match &it.kind {
                ItemKind::Shape { closed, .. } => Some(*closed),
                _ => None,
            }) {
                let mut checked = p.value();
                if ui.checkbox(&mut checked, t(lang, T::StyleClosed)).changed()
                    && checked != p.value()
                {
                    let items: Vec<(ItemId, bool, bool)> = poly_ids
                        .iter()
                        .filter_map(|id| {
                            self.scene.get_item(id).and_then(|it| match &it.kind {
                                ItemKind::Shape { closed, .. } => Some((*id, *closed, checked)),
                                _ => None,
                            })
                        })
                        .collect();
                    if !items.is_empty() {
                        self.push_cmd(Box::new(SetClosed::new_batch(items)));
                    }
                }
            }
            // 起/终点箭头（仅开放折线；闭合图形首尾相连，箭头无意义）。
            // Excalidraw 同款：两端各自独立选样式（无 / Arrow / Triangle / Dot / Bar）。
            let all_closed = poly_ids.iter().all(|id| {
                matches!(self.scene.get_item(id), Some(item) if matches!(item.kind, ItemKind::Shape { closed: true, .. }))
            });
            if !all_closed {
                if let Some(p) = prop(&self.scene, &poly_ids, |it| match &it.kind {
                    ItemKind::Shape {
                        start_arrow,
                        end_arrow,
                        closed: false,
                        ..
                    } => Some((*start_arrow, *end_arrow)),
                    _ => None,
                }) {
                    let (start, end) = p.value();
                    if p.is_mixed() {
                        ui.label(t(lang, T::PropsMixedValue));
                    }
                    ui.label(t(lang, T::StyleArrowStart));
                    if let Some(new) = arrowhead_selector(ui, lang, start) {
                        self.push_arrowhead(&poly_ids, true, new);
                    }
                    ui.label(t(lang, T::StyleArrowEnd));
                    if let Some(new) = arrowhead_selector(ui, lang, end) {
                        self.push_arrowhead(&poly_ids, false, new);
                    }
                }
            }
        }
    }

    /// 批量设置开放折线某一端的箭头样式（`start: bool` 选端；另一端保持各 item 原值）。
    /// `new: Option<ArrowHeadStyle>` 中 `None` 表示「无箭头」。读当前值合成一条 undo 命令。
    fn push_arrowhead(&mut self, ids: &[ItemId], start: bool, new: Option<ArrowHeadStyle>) {
        let items: Vec<(ItemId, ArrowHeads, ArrowHeads)> = ids
            .iter()
            .filter_map(|id| {
                self.scene.get_item(id).and_then(|it| match &it.kind {
                    ItemKind::Shape {
                        start_arrow,
                        end_arrow,
                        closed: false,
                        ..
                    } => Some((
                        *id,
                        ArrowHeads {
                            start: *start_arrow,
                            end: *end_arrow,
                        },
                        ArrowHeads {
                            start: if start { new } else { *start_arrow },
                            end: if start { *end_arrow } else { new },
                        },
                    )),
                    _ => None,
                })
            })
            .collect();
        if !items.is_empty() {
            self.push_cmd(Box::new(SetArrowHeads::new_batch(items)));
        }
    }

    /// 文字节：字号 / 颜色（Phase H）。
    ///
    /// 文字背景色（`background` 字段）已从面板移除：Excalidraw 的文字属性面板不暴露
    /// 背景选择器（绑定文字的背景由容器填充承担），用户亦不使用。字段与渲染路径保留，
    /// 旧存档带背景仍能正常显示，只是不再提供编辑入口。
    pub(crate) fn render_text_props(
        &mut self,
        ui: &mut egui::Ui,
        lang: Lang,
        dark: bool,
        ids: &[ItemId],
    ) {
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Text { font_size, .. } => Some(*font_size),
            _ => None,
        }) {
            ui.label(t(lang, T::StyleFontSize));
            let mut fs = p.value();
            if stepper(
                ui,
                &mut fs,
                &[16.0, 24.0, 32.0, 48.0, 72.0],
                &["XS", "S", "M", "L", "XL"],
                6.0..=200.0,
                None,
            ) {
                self.apply_continuous(
                    ids,
                    PropKind::TextStyle,
                    |k| k.text_style().map(PropValue::Text),
                    |k| {
                        if let ItemKind::Text { font_size, .. } = k {
                            *font_size = fs;
                        }
                    },
                );
            }
            if p.is_mixed() {
                ui.label(t(lang, T::PropsMixedValue));
            }
        }
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Text { color, .. } => Some(*color),
            _ => None,
        }) {
            ui.label(t(lang, T::StyleTextColor));
            let cur = p.value();
            // 选中里的绑定文字（有容器才能"跟随形状"）
            let bound_ids: Vec<ItemId> = ids
                .iter()
                .copied()
                .filter(|id| {
                    matches!(
                        self.scene.get_item(id).map(|it| &it.kind),
                        Some(ItemKind::Text {
                            container_id: Some(_),
                            ..
                        })
                    )
                })
                .collect();
            if bound_ids.is_empty() {
                // 纯自由文字：无容器可跟随，不显示跟随格
                let mut col = cur;
                if palette::color_palette_button(ui, &mut col, dark, lang) {
                    self.apply_text_color(ids, col);
                }
            } else {
                let follow_on = prop(&self.scene, &bound_ids, |it| match &it.kind {
                    ItemKind::Text { follow_stroke, .. } => Some(*follow_stroke),
                    _ => None,
                })
                .is_some_and(|pp| pp.value());
                let border = bound_ids
                    .iter()
                    .find_map(|id| self.container_stroke_of(id))
                    .unwrap_or(cur);
                let border_c = egui::Color32::from_rgba_unmultiplied(
                    border[0], border[1], border[2], border[3],
                );
                match palette::color_palette_button_follow(
                    ui,
                    &cur,
                    dark,
                    lang,
                    palette::FollowCtx {
                        on: follow_on,
                        border: border_c,
                    },
                ) {
                    palette::ColorPick::Color(new) => self.apply_text_color(ids, new),
                    palette::ColorPick::Follow(true) => {
                        // 开启跟随：绑定文字色吸附到各自容器边框色，follow=true
                        let mut changes: Vec<(ItemId, TextStyle, TextStyle)> = Vec::new();
                        for id in &bound_ids {
                            if let Some(old) =
                                self.scene.get_item(id).and_then(|it| it.kind.text_style())
                            {
                                let b = self.container_stroke_of(id).unwrap_or(old.color);
                                if old.follow_stroke && old.color == b {
                                    continue;
                                }
                                let mut new = old;
                                new.follow_stroke = true;
                                new.color = b;
                                changes.push((*id, old, new));
                            }
                        }
                        if !changes.is_empty() {
                            self.push_cmd(Box::new(SetTextStyle::new_batch(changes)));
                        }
                    }
                    palette::ColorPick::Follow(false) => {
                        // 关闭跟随：保持当前颜色，转为独立
                        let mut changes: Vec<(ItemId, TextStyle, TextStyle)> = Vec::new();
                        for id in ids {
                            if let Some(old) =
                                self.scene.get_item(id).and_then(|it| it.kind.text_style())
                            {
                                if !old.follow_stroke {
                                    continue;
                                }
                                let mut new = old;
                                new.follow_stroke = false;
                                changes.push((*id, old, new));
                            }
                        }
                        if !changes.is_empty() {
                            self.push_cmd(Box::new(SetTextStyle::new_batch(changes)));
                        }
                    }
                    palette::ColorPick::None => {}
                }
            }
            // 绑定文字且仍跟随：给出提示（一旦手选即消失）
            let any_following = ids.iter().any(|id| {
                matches!(
                    self.scene.get_item(id).map(|it| &it.kind),
                    Some(ItemKind::Text {
                        container_id: Some(_),
                        follow_stroke: true,
                        ..
                    })
                )
            });
            if any_following {
                ui.label(
                    egui::RichText::new(t(lang, T::TextColorFollowingStroke))
                        .weak()
                        .small(),
                );
            }
            if p.is_mixed() {
                ui.label(t(lang, T::PropsMixedValue));
            }
        }
        // 字体族（plan #1）：黑体 / 伪手写（逐字微抖，零体积增量）
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Text { font_family, .. } => Some(*font_family),
            _ => None,
        }) {
            let current = p.value();
            ui.label(t(lang, T::StyleFontLabel));
            ui.horizontal(|ui| {
                for (val, label) in [
                    (FontFamily::Normal, T::FontNormal),
                    (FontFamily::Handwriting, T::FontHandwriting),
                ] {
                    if ui
                        .selectable_label(current == val, t(lang, label))
                        .clicked()
                        && current != val
                    {
                        self.apply_continuous(
                            ids,
                            PropKind::TextStyle,
                            |k| k.text_style().map(PropValue::Text),
                            |k| {
                                if let ItemKind::Text { font_family, .. } = k {
                                    *font_family = val;
                                }
                            },
                        );
                    }
                }
            });
        }
        // 对齐（plan #1）：仅绑定文字显示；自由文本单行无框恒 top-left
        let has_bound_text = ids.iter().any(|id| {
            matches!(
                self.scene.get_item(id),
                Some(it) if matches!(
                    &it.kind,
                    ItemKind::Text {
                        container_id: Some(_),
                        ..
                    }
                )
            )
        });
        if has_bound_text {
            if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
                ItemKind::Text { align_h, .. } => Some(*align_h),
                _ => None,
            }) {
                let current = p.value();
                ui.label(t(lang, T::StyleAlignH));
                ui.horizontal(|ui| {
                    for (val, label) in [
                        (TextAlignH::Left, T::TextAlignLeft),
                        (TextAlignH::Center, T::TextAlignCenter),
                        (TextAlignH::Right, T::TextAlignRight),
                    ] {
                        if ui
                            .selectable_label(current == val, t(lang, label))
                            .clicked()
                            && current != val
                        {
                            self.apply_continuous(
                                ids,
                                PropKind::TextStyle,
                                |k| k.text_style().map(PropValue::Text),
                                |k| {
                                    if let ItemKind::Text { align_h, .. } = k {
                                        *align_h = val;
                                    }
                                },
                            );
                        }
                    }
                });
            }
            if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
                ItemKind::Text { align_v, .. } => Some(*align_v),
                _ => None,
            }) {
                let current = p.value();
                ui.label(t(lang, T::StyleAlignV));
                ui.horizontal(|ui| {
                    for (val, label) in [
                        (TextAlignV::Top, T::TextAlignTop),
                        (TextAlignV::Middle, T::TextAlignMiddle),
                        (TextAlignV::Bottom, T::TextAlignBottom),
                    ] {
                        if ui
                            .selectable_label(current == val, t(lang, label))
                            .clicked()
                            && current != val
                        {
                            self.apply_continuous(
                                ids,
                                PropKind::TextStyle,
                                |k| k.text_style().map(PropValue::Text),
                                |k| {
                                    if let ItemKind::Text { align_v, .. } = k {
                                        *align_v = val;
                                    }
                                },
                            );
                        }
                    }
                });
            }
        }
    }

    /// 图片节：不透明度 / 灰度（Phase H）。
    pub(crate) fn render_pixmap_props(&mut self, ui: &mut egui::Ui, lang: Lang, ids: &[ItemId]) {
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Pixmap { opacity, .. } => Some(*opacity),
            _ => None,
        }) {
            let o0 = p.value();
            // 下限 0.15（与背景透明度一致）：避免太低导致图片几乎不可见。
            // 用百分比中间变量（0–100），档位 [15,30,50,75,100]，与填充不透明度统一。
            ui.label(t(lang, T::StyleFillOpacity));
            let op_labels = [
                t(lang, T::OpMin),
                t(lang, T::OpLow),
                t(lang, T::OpMed),
                t(lang, T::OpHigh),
                t(lang, T::OpMax),
            ];
            let mut pct = (o0 * 100.0).round();
            if stepper(
                ui,
                &mut pct,
                &[15.0, 30.0, 50.0, 75.0, 100.0],
                &op_labels,
                15.0..=100.0,
                Some("%"),
            ) {
                let o = pct / 100.0;
                self.apply_continuous(
                    ids,
                    PropKind::Pixmap,
                    |k| match k {
                        ItemKind::Pixmap {
                            opacity, grayscale, ..
                        } => Some(PropValue::Pixmap(PixmapStyle {
                            opacity: *opacity,
                            grayscale: *grayscale,
                        })),
                        _ => None,
                    },
                    |k| {
                        if let ItemKind::Pixmap { opacity, .. } = k {
                            *opacity = o;
                        }
                    },
                );
            }
            if p.is_mixed() {
                ui.label(t(lang, T::PropsMixedValue));
            }
        }
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Pixmap { grayscale, .. } => Some(*grayscale),
            _ => None,
        }) {
            let mut checked = p.value();
            if ui
                .checkbox(&mut checked, t(lang, T::StyleGrayscale))
                .changed()
                && checked != p.value()
            {
                self.apply_continuous(
                    ids,
                    PropKind::Pixmap,
                    |k| match k {
                        ItemKind::Pixmap {
                            opacity, grayscale, ..
                        } => Some(PropValue::Pixmap(PixmapStyle {
                            opacity: *opacity,
                            grayscale: *grayscale,
                        })),
                        _ => None,
                    },
                    |k| {
                        if let ItemKind::Pixmap { grayscale, .. } = k {
                            *grayscale = checked;
                        }
                    },
                );
            }
        }
    }

    /// 画框节：编号（Phase H）+ 比例下拉（plan #3，「跟随全局比例」为其中一项）。
    pub(crate) fn render_frame_props(&mut self, ui: &mut egui::Ui, lang: Lang, ids: &[ItemId]) {
        ui.label(t(lang, T::PropsFrameNumber));
        if let Some(p) = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Frame { number, .. } => Some(*number),
            _ => None,
        }) {
            let mut n = p.value();
            if ui
                .add(egui::DragValue::new(&mut n).range(0..=9999))
                .changed()
            {
                let items: Vec<(ItemId, u32, u32)> = ids
                    .iter()
                    .filter_map(|id| {
                        self.scene.get_item(id).and_then(|it| match &it.kind {
                            ItemKind::Frame { number, .. } => Some((*id, *number, n)),
                            _ => None,
                        })
                    })
                    .collect();
                if !items.is_empty() {
                    self.push_cmd(Box::new(SetFrameNumber::new_batch(items)));
                }
            }
            if p.is_mixed() {
                ui.label(t(lang, T::PropsMixedValue));
            }
        }

        // 比例下拉（plan #3 + 跟随全局合并，2026-09-22）：原「跟随全局比例」勾选框并进同一
        // 菜单，不再单独占一个 bool——选「跟随全局」= 联动全局（改全局比例时按新比例重算本框
        // 尺寸，保持有效长边、中心锚定）；选任一具体比例 / 纸张 = 套用该尺寸并解除「跟随全局」
        // （覆盖全局）。预设仍是一次性动作，故非跟随时占位项恒显示。全局为「自由」时无可跟随，
        // 该项禁用并在下拉下方提示。
        ui.add_space(6.0);
        ui.label(t(lang, T::FramePresetLabel));
        let follows_prop = prop(&self.scene, ids, |it| match &it.kind {
            ItemKind::Frame {
                follow_global_ratio,
                ..
            } => Some(*follow_global_ratio),
            _ => None,
        });
        let global_ratio = self.frame_ratio.map(|(w, h)| format!("{w}:{h}"));
        let mixed = follows_prop.is_some_and(Prop::is_mixed);
        let follows = !mixed && follows_prop.is_some_and(|p| p.value());
        let follow_item = match &global_ratio {
            Some(r) => fill(t(lang, T::PropsFollowGlobalRatio), std::slice::from_ref(r)),
            None => t(lang, T::PropsFollowGlobal).to_string(),
        };
        let selected_text = if mixed {
            t(lang, T::PropsMixedValue).to_string()
        } else if follows {
            follow_item.clone()
        } else {
            t(lang, T::FramePresetPick).to_string()
        };
        let mut follow_clicked = false;
        let mut preset_clicked: Option<FramePreset> = None;
        egui::ComboBox::from_id_salt("frame_preset")
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                if ui
                    .add_enabled_ui(global_ratio.is_some(), |ui| {
                        ui.selectable_label(follows, follow_item.as_str())
                    })
                    .inner
                    .clicked()
                {
                    follow_clicked = true;
                    ui.close();
                }
                ui.separator();
                for preset in FRAME_PRESETS {
                    if ui.button(t(lang, preset.label())).clicked() {
                        preset_clicked = Some(preset);
                        ui.close();
                    }
                }
            });
        // 已跟随时再点该项是幂等 no-op（不产生空 undo）。
        if follow_clicked && !follows {
            self.set_frames_follow_global(ids, true);
        }
        if let Some(preset) = preset_clicked {
            self.apply_frame_preset(preset, ids);
        }
        if global_ratio.is_none() {
            ui.label(egui::RichText::new(t(lang, T::PropsFollowGlobalHint)).small());
        }
    }

    /// 批量把选中画框切到「跟随全局比例」（属性栏比例下拉的「跟随全局」项）。
    ///
    /// 跟随中的画框按全局比例**立即重算尺寸**（保持有效长边、中心锚定）；
    /// 尺寸重算与状态翻转打包成一条 undo。全局为「自由」时调用方不应进来（无可跟随）。
    pub(crate) fn set_frames_follow_global(&mut self, ids: &[ItemId], follow: bool) {
        let mut follows: Vec<(ItemId, bool, bool)> = Vec::new();
        let mut sizes: Vec<(ItemId, FrameGeom, FrameGeom)> = Vec::new();
        let rf = self.frame_ratio.map(|(w, h)| (w as f32, h as f32));
        for id in ids {
            let Some(it) = self.scene.get_item(id) else {
                continue;
            };
            if !it.is_frame() {
                continue;
            }
            let old_follow = it.frame_follows_global_ratio();
            if old_follow != follow {
                follows.push((*id, old_follow, follow));
            }
            // 恢复跟随时立即按全局比例重算尺寸；解除跟随时不动几何
            if follow {
                if let Some(rf) = rf {
                    let g = match &it.kind {
                        ItemKind::Frame { base_size, .. } => FrameGeom {
                            pos: (it.transform.pos.x, it.transform.pos.y),
                            base: *base_size,
                            scale: (it.transform.scale.x, it.transform.scale.y),
                        },
                        _ => continue,
                    };
                    let new = frame_geom_for_ratio(g, rf);
                    if new != g {
                        sizes.push((*id, g, new));
                    }
                }
            }
        }
        let mut cmds: Vec<Box<dyn Command>> = Vec::new();
        if !sizes.is_empty() {
            cmds.push(Box::new(SetFrameSize::new_batch(sizes)));
        }
        if !follows.is_empty() {
            cmds.push(Box::new(SetFrameFollowGlobal::new_batch(follows)));
        }
        if !cmds.is_empty() {
            self.push_cmd(Box::new(MultiCommand::new(cmds)));
        }
    }

    /// 把某个比例/纸张预设套用到选中的画框（plan #3），打包成一条 [`SetFrameSize`]。
    ///
    /// - `Ratio` 预设：保持画框当前**有效长边**长度，另一条边按 ratio 调整（只改形状不改观感大小）。
    /// - `Paper` 预设：套用固定像素尺寸（A4 按 96 DPI 换算）。
    ///
    /// 两种都以**画框中心**为锚点重算左上角位置；把 scale 归一到 1、尺寸写进 base_size，
    /// 使结果确定且后续手柄缩放从干净状态开始。套用前 `scale≠1` 的旧几何由命令快照精确还原。
    pub(crate) fn apply_frame_preset(&mut self, preset: FramePreset, ids: &[ItemId]) {
        let mut items: Vec<(ItemId, FrameGeom, FrameGeom)> = Vec::new();
        let mut follows: Vec<(ItemId, bool, bool)> = Vec::new();
        for id in ids {
            let Some(it) = self.scene.get_item(id) else {
                continue;
            };
            let (bw, bh) = match &it.kind {
                ItemKind::Frame { base_size, .. } => *base_size,
                _ => continue,
            };
            let (sx, sy) = (it.transform.scale.x, it.transform.scale.y);
            let (px, py) = (it.transform.pos.x, it.transform.pos.y);
            let (ew, eh) = (bw * sx, bh * sy);
            let (cx, cy) = (px + ew / 2.0, py + eh / 2.0);
            let old = FrameGeom {
                pos: (px, py),
                base: (bw, bh),
                scale: (sx, sy),
            };
            let new = match preset {
                // 比例预设：保持有效长边、按目标比例重算，几何规划走 core 纯函数
                FramePreset::Ratio { w, h, .. } => frame_geom_for_ratio(old, (w, h)),
                // 纸张预设：固定像素绝对尺寸（A4 按 96 DPI 换算）
                FramePreset::Paper { w, h, .. } => FrameGeom {
                    pos: (cx - w / 2.0, cy - h / 2.0),
                    base: (w, h),
                    scale: (1.0, 1.0),
                },
            };
            items.push((*id, old, new));
            // 套用预设 = 覆盖全局：解除「跟随全局比例」（快照旧值保证 undo 精确）
            follows.push((*id, it.frame_follows_global_ratio(), false));
        }
        if items.is_empty() {
            return;
        }
        let label = t(self.lang, preset.label()).to_string();
        self.push_cmd(Box::new(MultiCommand::new(vec![
            Box::new(SetFrameSize::new_batch(items)),
            Box::new(SetFrameFollowGlobal::new_batch(follows)),
        ])));
        self.flash(fill(t(self.lang, T::FlashFramePresetApplied), &[label]));
    }

    /// 线性对象批量切换曲线模式（离散，整批改命令）。
    pub(crate) fn push_poly_curve(&mut self, ids: &[ItemId], new_curve: CurveType) {
        let items: Vec<(ItemId, CurveType, CurveType)> = ids
            .iter()
            .filter_map(|id| {
                self.scene.get_item(id).and_then(|it| match &it.kind {
                    ItemKind::Shape { curve_type, .. } => Some((*id, *curve_type, new_curve)),
                    _ => None,
                })
            })
            .collect();
        if items.is_empty() {
            return;
        }
        // plan #19：切 elbow 时闭合折线转开放（顶点保留 + 末段连回首点视觉闭环），
        // closed 变更与 curve_type 变更打包成一条 undo。
        let mut cmds: Vec<Box<dyn Command>> = vec![Box::new(SetCurveType::new_batch(items))];
        if new_curve == CurveType::Elbow {
            let closed_changes: Vec<(ItemId, bool, bool)> = ids
                .iter()
                .filter_map(|id| {
                    self.scene.get_item(id).and_then(|it| match &it.kind {
                        ItemKind::Shape { closed: true, .. } => Some((*id, true, false)),
                        _ => None,
                    })
                })
                .collect();
            if !closed_changes.is_empty() {
                cmds.push(Box::new(SetClosed::new_batch(closed_changes)));
            }
        }
        let cmd: Box<dyn Command> = if cmds.len() == 1 {
            cmds.into_iter().next().unwrap()
        } else {
            Box::new(MultiCommand::new(cmds))
        };
        self.push_cmd(cmd);
    }

    /// 应用文字颜色（调色板选色）：写色并脱离"跟随形状"。走连续编辑合并路径（一条 undo）。
    fn apply_text_color(&mut self, ids: &[ItemId], new: [u8; 4]) {
        self.apply_continuous(
            ids,
            PropKind::TextStyle,
            |k| k.text_style().map(PropValue::Text),
            |k| {
                if let ItemKind::Text {
                    color,
                    follow_stroke,
                    ..
                } = k
                {
                    *color = new;
                    *follow_stroke = false;
                }
            },
        );
    }

    /// 绑定文字所属容器的描边色（"跟随形状"的边框色）；非绑定文字或容器已删则 `None`。
    fn container_stroke_of(&self, text_id: &ItemId) -> Option<[u8; 4]> {
        let cid = match self.scene.get_item(text_id).map(|it| &it.kind) {
            Some(ItemKind::Text {
                container_id: Some(c),
                ..
            }) => *c,
            _ => return None,
        };
        match self.scene.get_item(&cid).map(|it| &it.kind) {
            Some(ItemKind::Shape { stroke, .. }) => Some(stroke.color),
            _ => None,
        }
    }

    /// 连续编辑的「开帧」入口：记录变更前快照并标记本帧有变更（Phase H）。
    ///
    /// 若已有不同种类的待合并编辑，先结算它，再开启本次编辑；同种类则保留首次快照
    /// （拖动期间整段只取一处 old 值），由 [`Self::update`] 帧末统一结算成一条命令。
    pub(crate) fn ensure_prop_edit(&mut self, kind: PropKind, snapshot: Vec<(ItemId, PropValue)>) {
        if let Some(p) = self.prop_edit_pending.take() {
            if p.kind != kind {
                if let Some(cmd) = prop_cmd(p, &self.scene) {
                    self.push_cmd(cmd);
                }
            } else {
                self.prop_edit_pending = Some(p);
            }
        }
        if self.prop_edit_pending.is_none() {
            self.prop_edit_pending = Some(PropEdit {
                kind,
                items: snapshot,
            });
        }
        self.prop_changed_this_frame = true;
    }

    /// 连续控件通用路径：取变更前快照 → 直接改 item（不入栈）→ 标记待合并（Phase H）。
    ///
    /// `snap` 从每个选中项的当前状态抽出旧值（用于合成 undo 的旧端）；`apply` 把新值
    /// 写到每个选中项。整段拖动在 [`Self::update`] 帧末被合成为**一条**批量命令。
    pub(crate) fn apply_continuous(
        &mut self,
        ids: &[ItemId],
        kind: PropKind,
        snap: impl Fn(&ItemKind) -> Option<PropValue>,
        apply: impl Fn(&mut ItemKind),
    ) {
        let items: Vec<(ItemId, PropValue)> = ids
            .iter()
            .filter_map(|id| {
                self.scene
                    .get_item(id)
                    .and_then(|it| snap(&it.kind).map(|v| (*id, v)))
            })
            .collect();
        self.ensure_prop_edit(kind, items);
        for id in ids {
            if let Some(item) = self.scene.get_item_mut(id) {
                apply(&mut item.kind);
            }
        }
    }

    /// 描边色联动同步（改描边色时调用）：填充色跟随（保留 alpha）、绑定文字色跟随。
    ///
    /// 直接改 kind 作预览，并把联动目标的旧值快照追加进当前 Stroke 待合并编辑
    /// ——`prop_cmd` 会把混合快照分流成三条批量命令并打包为一条 [`MultiCommand`]，
    /// 撤销时整体回到改色前。按 id 去重：pending 已有的条目是首次快照，不覆盖
    /// （与 [`Self::ensure_prop_edit`] 的"整段拖动只取一处 old 值"语义一致）。
    pub(crate) fn sync_stroke_color_side_effects(&mut self, ids: &[ItemId], color: [u8; 4]) {
        let mut extra: Vec<(ItemId, PropValue)> = Vec::new();
        for id in ids {
            // 一次性提取不可变数据后立即释放借用（后续预览阶段要可变借 scene）
            let (is_shape, fill_target) = match self.scene.get_item(id).map(|it| &it.kind) {
                Some(ItemKind::Shape {
                    fill,
                    fill_style,
                    fill_follow_stroke,
                    ..
                }) => (true, fill.map(|f| (f, *fill_style, *fill_follow_stroke))),
                Some(_) => (false, None),
                None => continue,
            };
            if !is_shape {
                continue;
            }
            // 该图形的绑定文字：old = 当前整份 TextStyle，new = 仅换 color
            let text_ids: Vec<ItemId> = self
                .scene
                .items
                .iter()
                .filter_map(|t| match &t.kind {
                    ItemKind::Text {
                        container_id: Some(cid),
                        ..
                    } if cid == id => Some(t.id),
                    _ => None,
                })
                .collect();
            for tid in text_ids {
                let Some(t) = self.scene.get_item(&tid) else {
                    continue;
                };
                let Some(old_style) = t.kind.text_style() else {
                    continue;
                };
                // 仅"跟随边框"中的文字随描边变色；用户手选过独立色的文字保持不动。
                if !old_style.follow_stroke {
                    continue;
                }
                if old_style.color != color {
                    extra.push((tid, PropValue::Text(old_style)));
                }
                // 预览：直接改文字颜色
                if let Some(t) = self.scene.get_item_mut(&tid) {
                    if let ItemKind::Text { color: c, .. } = &mut t.kind {
                        *c = color;
                    }
                }
            }
            // 填充：old = 当前 FillState，new = 换 color 保 alpha；无填充不联动
            if let Some((fill, fill_style, fill_follow)) = fill_target {
                // 仅"跟随形状"的填充随描边变色；已独立设色的填充保持不动。
                if fill_follow {
                    if fill[0] != color[0] || fill[1] != color[1] || fill[2] != color[2] {
                        extra.push((
                            *id,
                            PropValue::Fill(FillState {
                                color: Some(fill),
                                style: fill_style,
                                follow_stroke: fill_follow,
                            }),
                        ));
                    }
                    // 预览：直接改填充颜色（保留 alpha）
                    if let Some(it) = self.scene.get_item_mut(id) {
                        if let ItemKind::Shape { fill: Some(f), .. } = &mut it.kind {
                            f[0] = color[0];
                            f[1] = color[1];
                            f[2] = color[2];
                        }
                    }
                }
            }
        }
        // 追加快照（按 id 去重，保留首次快照）
        if !extra.is_empty() {
            if let Some(p) = self.prop_edit_pending.as_mut() {
                if p.kind == PropKind::Stroke {
                    for (id, v) in extra {
                        if !p.items.iter().any(|(eid, _)| *eid == id) {
                            p.items.push((id, v));
                        }
                    }
                }
            }
        }
    }
}
