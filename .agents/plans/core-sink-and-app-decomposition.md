# Plan: core 下沉 + preferz_app.rs 拆分（架构整固）

- **状态**: 🔶 Step 0 已完成（安全网 + ADR-0008 已立）。**D1 已拍板（2026-09-17）：`ViewportState` 归 core，新增 ADR-0008 修订 ADR-0002**。下一步 = Step 1（零纠缠纯决策下沉）。
- **创建日期**: 2026-09-17
- **策略**: 一步到位重排分层，但**分步落地、每步独立跑质量门**；全程**零行为改动**（不借重构改交互/观感）。早期无向后兼容包袱，实在不行可 revert 到本分支基线。
- **前置评估**: 已完成全仓代码事实核查（行数量、依赖方向、egui 耦合点、纯函数候选），详见 §1/§2。

---

## 1. 背景与目标

**触发**：`preferz_app.rs` 单文件 9230 行 = 全库（~21.4k 行 src）的 43%，是装了 9 类职责的上帝对象；同时 binary crate（13681 行）远厚于 core（6870 行），core 显得偏薄。

**规模事实**（src 计数，HEAD `152b139`）：

| crate | 行数 | 依赖 | `#[test]` | 角色 |
|---|---|---|---|---|
| `preferz-core` | 6870 | euclid/uuid/serde（**无 GUI**，已逐条核实） | 104 | 领域模型 |
| `preferz-fileio` | 829 | core + rusqlite + image | 7 | .prz 存取 + 图片解码 |
| `preferz`(bin) | 13681 | 两者 + eframe/egui | 70 | 全部 UI/交互 |

依赖方向**干净、无环、无反向引用**：`bin→core+fileio`、`fileio→core`、`core→仅 euclid/uuid/serde`。

**关键证据（决定"下沉能走多远"的卡点）**：

- core 的 `spaces.rs` **早已定义** `ScreenPoint`/`ScreenRect`/`CanvasToScreen`/`ScreenToCanvas`（带 `ScreenSpace` 标签的 euclid 类型），**但 `viewport.rs` 住在 binary、绕过这些别名直接用 `egui::Rect`/`Pos2`/`Vec2`** → 这是一道本不该存在的坐标边界泄漏。
- `begin_drag(&mut self, screen_pos: egui::Pos2, …)` 一进去就 `viewport.screen_to_canvas(screen_pos)` 转画布系；`begin_drag`→`finish_create_shape` 约 1000 行里 **`egui::` 仅出现 2 次**——耦合基本只是 `Pos2` + 访问 `self.viewport`。
- `compute_fit`（`preferz_app.rs:8076`）/ `present_compute_fit`（`:3411`）是纯数学，只拿 `egui::Rect` 的 `width()/height()/center()`。
- `arrange.rs` 的 `plan_align`/`plan_distribute` **已经是本计划要推广的样板**：纯决策、返回 `(ItemId, old_pos, new_pos)` 数据，UI 层包成 `ArrangeItems` 入 undo。下沉 L1 决策的模式在本仓已被验证可行。

**目标**：core 承接"无副作用、只吃领域数据的决策"，binary 收薄成 egui 渲染 + 编排壳；`preferz_app.rs` 从 9230 行降到结构性合理。

**非目标**：不改任何用户可见功能/交互/观感；不引入新依赖；不合并/新增 crate；不把 L2 编排塞进 core（见 §2 与你的拍板）。

## 2. 分层判据（三类，不是两类）

判据**不是"签名里有没有 egui"**，而是"**是不是无副作用、只吃领域数据的决策**"：

- **L1 纯领域/决策 → `preferz-core`**：坐标/视口变换数学、fit/snap、bbox 并集、Prop 混合值合并、"手势 → 生成哪个 Command"、arrange planner。`(core 类型) → (core 类型 / Command)`，无 egui、无副作用，`cargo test -p preferz-core` 可无头覆盖。
- **L2 egui↔core 编排 → 留在 binary**：读 egui 事件（PointerButton/modifiers/Pos2）、持活的 `ViewportState`/`UndoStack`/瞬时态（`DragState`/`CropMode`/`EditingText`）、`request_repaint`、`BackgroundOps` 线程、config 落盘、键盘派发。**这块多数签名不带 egui，但它是 app 控制流与副作用归属，硬塞进 core 只会把 egui/副作用所有权拽进 core，毁掉可测性——明确留在 app（已拍板）。**
- **L3 egui 渲染 → 留在 binary（已有 `ui/`）**：面板、绘制、widget、context menu、放映画面。

## 3. 枢纽动作：ViewportState 下沉 core + 封坐标漏

把 `ViewportState` 移到 `preferz-core`，字段/签名里所有 `egui::Rect`/`Pos2`/`Vec2` 换成 core 已有的 `ScreenRect`/`ScreenPoint`/`ScreenVector`。egui 边界收缩成一对一对一转换（`egui::Pos2 ↔ ScreenPoint`、`egui::Rect ↔ ScreenRect`），只在 L2 输入边、L3 绘制边各点一次。

**收益（杠杆级）**：viewport 一旦是 core 类型，前述 drag/fit/zoom/snap 从"访问 self.viewport + egui::Pos2"降级为纯 core 计算，L1 决策成建制搬进 core 并无头可测。

**代价（诚实）**：`canvas_to_screen`/`screen_to_canvas`/`canvas_to_screen_rect` 调用点波及一大片（改类型机械，`From`/`.into()` + helper 兜住）；故单独成一步、单独跑门。**这是全计划风险最高的一步。**

## 4. 决策点（需你点头，阻塞 Step 2）

| # | 决策点 | 冲突/背景 | 我的倾向 |
|---|---|---|---|
| D1 | **`ViewportState` 归 core 还是留 binary** | **ADR-0002 明列 "Viewport → binary 层"**；下沉它 = 改一条已接受 ADR | ✅ **已拍板（2026-09-17）：走 D1，归 core**。理由：其状态性（每帧 `screen_rect`）不需要 egui、只需一个 `ScreenRect` 值；egui 耦合纯属"类型选错"非本质。ADR-0008 已立、修订 ADR-0002 该行。（退路 D1' 作废，不再需要。） |
| D2 | L2 是否进 core | — | **不进**（你已拍板） |
| D3 | drag 决策劈分深度 | Step 4 | 先只做 §1 里"零纠缠"的（fit/snap/prop 合并），端点/延伸/闭合那套交互态留 app，最晚、按需再劈 |

## 5. 分步落地与测试闸

质量门（每步都跑，全绿才算完成）：`cargo fmt --all --check` → `cargo clippy --workspace --all-targets -- -D warnings` → `cargo test --workspace`；涉 egui 边界的步补一次 `cargo run` 手测点（列在各步）。

| 步 | 内容 | 归属变化 | 测试闸 |
|---|---|---|---|
| **Step 0** | 安全网：本分支基线三件套全绿；`.agents/plan.md` 加一行指向本文档；（若 D1 通过）起草 ADR-0008 | 文档 | 三件套绿；本文档 + 指针入库 |
| **Step 1** | 零 egui 纠缠的纯决策下沉 core：`snap_polygon_point`→`core::snap`；`fit_to_screen`/`zoom_to_selection` 里的 **bbox 并集 → rect 决策**（算 rect 进 core，`viewport.fit_to_content`+`flash` 留 app）；`Prop` 混合值合并纯部分（仿 `arrange.rs::plan_*` 样板，返回数据、UI 包命令） | L1 → core | 每簇补 core headless 单测；`test -p preferz-core` 增，`preferz` 对应逻辑变薄；三件套绿 |
| **Step 2**（枢纽，阻塞于 D1） | `ViewportState` 下沉 core + 封坐标漏；`viewport.rs` 现 3 个测试随之搬入 core（`egui::Rect`→`ScreenRect`） | L1 数学 → core，边界收口 | core 无头测变换/夹取/侧栏开合补偿；`--workspace` 全绿；**手测：滚轮缩放锚点不漂、中键平移、侧栏开合内容不跳变、Shift+1/2/3 视口动作** |
| **Step 3** | `preferz_app.rs` 模块拆分：残留 L1 继续删+进 core；L2/L3 拆进 `crates/preferz/src/app/` 子模块，`preferz_app.rs` 退化为 struct + `ui()` 派发壳 | L2/L3 重排（同 crate） | 逐簇搬，每搬一簇三件套绿；行为零改 |
| **Step 4**（可选，最晚） | 劈 `update_drag_preview`：`给定(起点,当前点,aspect,snap,mode)→新 Transform/Command` 抽成 core 决策；读指针+`screen_to_canvas`+改 `self.drag`+`request_repaint` 的壳留 app | 再抽一层 L1 → core | **先写 core 决策单测钉死语义再动壳**；三件套绿 |

### Step 3 拆分地图（9230 行的去向）

| 抽出模块 | 约行 | 内容 | 层 | 风险 |
|---|---|---|---|---|
| `export.rs` | ~430 | `ExportFormat`/`export_scene_to_file`/`export_pixmaps_to_dir`/`sample_item_pixel` | L1 多/L2 薄 | 极低（近自由函数） |
| `background_ops.rs` | ~200 | `BackgroundOps` + `*Outcome`（异步 import/load/save/export 线程） | **L2** | 低（已自封装） |
| `present.rs` | ~230 | `present_*` 放映 | L2+L3 | 低 |
| `props.rs` | ~1730 | `Prop` 机制 + `render_*_props` + defaults/align + `apply_frame_preset` | L1(合并决策)+L3(渲染) | 中 |
| `drag.rs` | ~1470 | `begin_drag`/`update_drag_preview`/`end_drag`/`finish_create_*` | L2(+可劈 L1) | 中高，Step 4 对象 |
| `render.rs` | ~700 | `draw_item_visual`/`draw_text_item`/`render_scene`/`ensure_*_textures`/`render_crop_overlay` | L3 | 中 |
| `text_edit.rs` | ~440 | `start_text_edit`/`render_text_editor`/`render_frame_number_editor` | L2+L3 | 低 |
| `context_menu.rs` | ~350 | `render_context_menu` | L3 | 低 |
| `actions.rs` | ~700 | group/delete/dup/connected/reorder/fit/zoom/crop/arrange/align/distribute/normalize | L2(+L1 planner) | 中 |
| `file_io.rs` | ~700 | `finish_import`/`save_file*`/`open_project_file`/`paste_from_clipboard`/`start_export*` | **L2** | 中 |
| `settings.rs` | ~250 | `render_settings_window`/`persist_config`/`render_save_prompt`/颜色选择器 | L2+L3 | 低 |

（`handle_shortcuts` 可并入现有 `keymap.rs`；~600 行 `mod tests` 随各簇走。）

## 6. crate 边界结论（回应"要不要并成单 crate"）

**保留三 crate，不合并**——方向对，只做对了一半：问题不是"3 还是 1"，是**太多 L1 被困在 binary**。`fileio` 把 `rusqlite(bundled)`+`image` 两个编译黑洞隔离在 core/GUI 之外；core 无 egui 让 104 测试脱离 eframe 秒级跑——这些正是 ADR-0001 立下的价值，本计划是**兑现**它、不是推翻。补正方向 = 继续往 core 下沉，非往上合并。

## 7. 风险与回退

- **主风险**：Step 2 类型波及广、Step 4 交互回归。缓解：各自独立成步、独立门、Step 4 先测后改。
- **回退**：每步一个可 revert commit；任一 Step 三件套或手测不过 → revert 该 Step，不影响已落地步。
- **纪律**：提交只 add 本次改动文件，禁 `git add -A`；动手前 `git log/status` 对齐树。

## 8. 交付时文档面同步

- Step 0/2：`ADR-0008`（若 D1 通过，修订 ADR-0002 的 Viewport 归属行）+ `.agents/plan.md` 指针。
- 收尾：`AGENTS.md` 的 "Architecture" 小节与 "Must-Know Conventions" 更新坐标边界收口约定；本计划完成项提炼进 `CHANGELOG.md`，本文档随后归档/移除（对齐 AGENTS.md 文档流）。

## 9. 验收清单（人工，Step 2/4 后各一次 `cargo run`）

- [ ] 缩放以鼠标为锚，缩放前后同画布点屏幕位置不变（修 W4 语义未退化）
- [ ] 中键/空白拖拽平移顺滑；侧栏开合内容不整体平移（`set_screen_rect` 补偿未退化）
- [ ] Shift+1 适配全部 / Shift+2 缩放到选中 / Shift+3 回 100% 行为同前
- [ ] 移动/缩放/旋转端点，形状-箭头绑定联动同前（#5/#14）
- [ ] 多边形 Alt+删顶点 / Alt+拖延伸 / 首尾重合自动闭合同前（#4）
- [ ] 导入/保存/导出/复制粘贴与 undo/redo 同前
- [ ] EN/中 双语 toast 与进度条文案同前（i18n 回归闸仍绿）
