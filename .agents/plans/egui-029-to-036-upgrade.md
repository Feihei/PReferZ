# Plan: egui/eframe 0.29 → 0.36 一步到位升级

- **状态**: ✅ **已结项**（2026-09-28）——代码迁移完成、静态门（fmt / clippy `-D warnings` / cargo test / build）全绿并已提交；§6 的 GUI 全功能手测与视觉回归**已通过**。交付归档见 [`.agents/CHANGELOG.md`](../CHANGELOG.md) §依赖大版本升级 / §人工复验清账（本文档保留作风险清单与回归清单参考）
- **创建日期**: 2026-09-17
- **目标版本**: egui 0.36.2 / eframe 0.36.2
- **当前版本**: egui 0.29.1 / eframe 0.29.1（Cargo.lock 锁定）
- **策略**: **单次 bump，直接以 0.36 API 重写调用点**，不经过中间版本，不保留任何兼容写法。理由：用户基数小、无向后兼容包袱；中间版本引入的 deprecated API 在 0.36 已全部移除，走中间版本只是徒增工作。
- **前置评估**: 已基于 egui 官方 CHANGELOG 0.30-0.36 与全仓 API 使用扫描完成（§3 风险清单）

---

## 1. 背景与目标

egui 是本项目唯一的 GUI 框架，直接影响渲染质量与交互性能。0.29（2024-09）落后上游 7 个 minor 版本；0.30-0.36 期间上游完成 tessellation 重写、字体渲染重做（skrifa + hinting + harfrust kerning）、`App::update` → `App::ui`、Panel API 统一等重大演进。

**目标**：一次升级至 0.36.2，全部调用点直接使用 0.36 的新 API，功能与现状等价（不借机改行为），获得字体/渲染质量提升。

**非目标**：不改任何用户可见功能/交互；不引入新依赖；`egui::Scene`、`egui_kittest` 落地不在本计划内（§8）。

## 2. 收益摘要

| 收益 | 版本 | 对本项目的价值 |
|---|---|---|
| 字体渲染重做：skrifa + vello_cpu，支持 hinting | 0.34 | 文字更清晰，支持字体 variations |
| harfrust kerning/ligatures | 0.35 | 中文字体（SourceHanSansCN）排印更好 |
| tessellation 质量改进（更少顶点/overdraw） | 0.31 | 项目有 3 处手写 mesh quad + 大量 Shape，画质性能双升 |
| 文字/图形清晰度全面提升（gamma 空间纹理过滤等） | 0.32 | 直接改善观感 |
| `egui::Scene`（可平移/缩放 canvas 容器） | 0.31 | 后续可简化手写视口代码（另立计划） |
| `egui_kittest` + `egui_inspection` + `egui_mcp` | 0.30/0.35 | GUI 回归测试 + agent 程序化自测（后续机会） |
| IME 组合显示改进 | 0.35/0.36 | 中文输入法体验 |
| Atoms / Popup / Menu 重写 | 0.32 | 更强 widget 布局与弹出菜单 |
| `Response`/`Sense` bitfield、`Margin`/`Shadow` 缩小 | 0.31 | 内存占用降低 |

## 3. 风险与改动总清单（~49 个调用点）

以下全部按 **0.36 API 目标形态** 列出，执行时以 docs.rs（egui 0.36 / eframe 0.36）为准。

### A. App 入口（1 处，影响面最大）
| 现状（0.29） | 目标（0.36） | 位置 |
|---|---|---|
| `fn update(&mut self, ctx: &Context, frame: &mut Frame)` | `fn ui(&mut self, ui: &mut Ui, frame: &mut Frame)`（0.34 改名换签名，0.35 移除 `update`，**0.36 强制**） | `preferz_app.rs:1098` |

迁移要点：
- `Ui: Deref<Target = Context>`（0.34+），`update` 函数体内绝大多数 `ctx.foo()` 可直接改为 `ui.foo()`（含 `ctx.input(|i|...)` → `ui.input(|i|...)`）。
- 传给后台线程的 `ctx.clone()`（6 处：271/316/347/7360/7483/7578）改 `ui.ctx().clone()` 取真 `Context`。
- `clear_color` 重写（1094）确认 0.36 是否仍存在，若移除改用对应配置替代。
- `CreationContext`（main.rs:35-41 `cc.egui_ctx`）与 `NativeOptions`/`ViewportBuilder`（main.rs:23-30）0.29-0.36 稳定，预期不动。

### B. Panel 层（5 处）
| 现状 | 目标 | 位置 |
|---|---|---|
| `SidePanel::left` | 统一 `Panel` API（0.34 引入，0.36 唯一选择；0.35 有方法重命名，注意 `resizable`/`exact_width`/`default_width` 等对应关系） | 1166 |
| `SidePanel::right` ×2 | 同上 | 4784/4945 |
| `CentralPanel::default().show(...)` | `Panel` central 形态（0.34 起 `CentralPanel::show` deprecated，0.35 移除） | 1228/3438 |

### C. 绘制原语（~35 处，工作量主体）
| 现状 | 目标 | 位置 |
|---|---|---|
| `Rounding::same(0.0/1.0/2.0/3.0)`、`Rounding::ZERO` | `CornerRadius::same(0/1/2/3)`、`CornerRadius::ZERO`（f32→u8，0.31） | ~14 处：preferz_app.rs 1300/1480/3195/3319/3674/3709/3775/3993 + transform_handles.rs 261/273/300/313/325 |
| `rect_stroke(rect, f32, stroke)` | 加 `StrokeKind` 参数（0.31）：矩形描边用 `StrokeKind::Inside`，开放路径用 `Middle`，逐个确认 | ~11 处：preferz_app.rs 1323/1346/1426/3335/3697/3764/8759 + palette.rs 149/154/160/331 |
| `rect_filled(rect, Rounding, color)` | `Rounding` 参数 → `CornerRadius` | 与上一条同批处理（3775/3993 + transform_handles.rs 5 处） |
| `epaint::Mesh { .. }` / `Vertex { pos: [x,y].into(), uv, color }` 字面量（0.31 `RectShape` 参数重构，官方推荐 builder） | 改 builder/显式构造，`Vertex` 字段类型以 0.36 docs 为准 | 3 处：preferz_app.rs 3244-3256/3599-3611/3911-3923 |
| `epaint::PathShape { points, closed, fill, stroke }` 字面量 | 若新增字段补 `..Default::default()` 或改 builder；`PathStroke::new`/`NONE` 确认 | stylers.rs 325/335/329/426/578 |
| `CubicBezierShape::from_points_stroke(...)` | 确认签名未变 | stylers.rs 574 |
| `Painter::galley(pos, Arc<Galley>, color)` | 0.30+ 签名可能去 `Arc`，按 docs 调整 | 3 处：preferz_app.rs 3200/3712/3735 |

### D. 杂项 API（~9 处）
| 现状 | 目标 | 位置 |
|---|---|---|
| `ctx.screen_rect()` | `ctx.content_rect()`（0.33 deprecated→0.35 移除；桌面端语义等价） | 4 处：1634/3426/6025/6965 |
| `ctx.style()` | `ctx.global_style()`（0.34） | 1 处：1226 |
| `ComboBox::from_id_source` | `ComboBox::from_id_salt`，并删除现有 `#[allow(deprecated)]` | 2 处：8527/8546 |
| `ui.allocate_new_ui(UiBuilder, ...)` | `ui.scope_builder(...)`（0.32 deprecated→0.35 移除） | 1 处：4746 |
| `ctx.output_mut(\|o\| o.cursor_icon = ...)` | `ctx.set_cursor_icon(...)`（0.32+ 推荐） | 1 处：1536 |
| `ui.menu_button` 默认点击关闭（0.32 行为变化） | 确认每个菜单项点击后是否应关闭；需保持打开的改 `PopupCloseBehavior` | 全部 menu_button 调用点 |

### E. 行为/视觉变化（不编译断，需人工检查）
- `Frame` 把 stroke width 算进 padding（0.31）— 约 10 处 `Frame::popup/central_panel/none`（1226/3439/4122/4256/4321/5910/5928/6028/8605/8621）。
- 默认文字大小 12.5 → 13.0（0.33）。
- tessellation 坐标取整到 1/32（0.31）— 细线/圆角观感可能微变。
- 0.36 "press that leaves a widget 算 drag" + `Sense::drag` 修复（0.36.1）— 画布框选/拖拽、面板 resize 手柄回归。
- `Modifiers` 从 `RawInput` 移到 `Event`（0.36）— 项目经 `InputState::modifiers` 访问，预期不受影响，快捷键全表回归即可。
- thin angled rectangle 渲染修复（0.36.2）— 斜向细线（变换手柄连线等）观感确认。

### F. 已规避的大坑（无需处理）
项目未使用：`tex_manager()`/`TexturesDelta`、`Memory::data`/`TypeMap`、`ImageButton`、egui 剪贴板（用 `arboard`）、`raw_input_hook`/`post_render`/`auto_save`、deferred viewports、`egui_extras`/`egui_plot`。core/fileio 两个 crate 不依赖 egui。

## 4. 前置条件

1. **网络**：本机无 `.cargo/config.toml` 镜像配置，`cargo update` 需访问 crates.io；失败则临时配置镜像（如 rsproxy），完成后还原。
2. **工作区干净**：开独立分支 `chore/egui-0.36-upgrade`。
3. **基线验证**：升级前在当前 HEAD 跑通 `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo run -p preferz`，截图记录当前 UI 外观（主题面板/画布/HUD/调色板/菜单）作为视觉回归基准。
4. **工具链**：rustc 1.98.1 ≥ 0.36 MSRV 1.95，无需变更。
5. **API 参考**：执行中随时查 docs.rs（egui 0.36 / eframe 0.36），尤其是 `App::ui` 精确签名、`Panel` API、`Vertex`/`PathShape` 字段；可参照 egui 仓库 0.36 分支的 eframe demo 源码。

## 5. 实施步骤（单分支、按类别分 commit）

> 一步到位不等于一个大 commit：按下列顺序逐类别修复，每类别一个 commit——既可回滚定位，又不在任何 commit 里维护中间版本兼容代码（中途 commit 编译不过属预期，最终收尾 commit 保证全绿）。

1. **步骤 1 — bump**：`Cargo.toml` workspace.dependencies 改 `eframe = "0.36"`、`egui = "0.36"`，`cargo update`。commit: `chore: bump egui/eframe to 0.36`。
2. **步骤 2 — App 入口**：`update` → `ui` + 函数体 `ctx.foo()` → `ui.foo()` + 后台线程 `ui.ctx().clone()`。commit: `refactor: migrate App::update to App::ui`。
3. **步骤 3 — Panel**：5 处迁移到统一 `Panel` API。commit: `refactor: migrate panels to unified Panel API`。
4. **步骤 4 — 绘制原语**：`CornerRadius`（14 处）→ `StrokeKind`（11 处）→ epaint 字面量（6 处）→ `Painter::galley`（3 处）。commit: `refactor: migrate paint primitives to 0.36 API`。
5. **步骤 5 — 杂项**：`content_rect`、`global_style`、`id_salt`、`scope_builder`、`set_cursor_icon`、菜单 `PopupCloseBehavior`。commit: `refactor: migrate misc APIs`。
6. **步骤 6 — 收尾**：全仓 `rg "Rounding|from_id_source|screen_rect|allocate_new_ui|allow\(deprecated\)"` 确认零残留；`cargo fmt`、`clippy -D warnings`、`cargo test`。commit: `chore: cleanup after egui 0.36 migration`。

## 6. 验证

每类别改完即跑 `cargo check --workspace` 聚焦该类错误；全部完成后：

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo run -p preferz` 全功能手测：新建/导入/拖放文件、画布平移缩放、选中/框选/拖拽、Undo/Redo、快捷键全表（`keymap.rs` 898 行绑定）、文字编辑 overlay、变换手柄、灰度模式、调色板 popup、菜单、Present 模式、导出、多语言切换、窗口关闭/全屏/置顶/无边框、后台导入线程完成后画面刷新。
- 视觉回归：对照基线截图逐面板比对（重点：Frame padding、文字大小 13.0、细线/圆角、字体 hinting）。

## 7. 验收清单

- [ ] 全部调用点使用 0.36 API，无 deprecated 写法
- [ ] `cargo fmt --all --check` 干净
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` 零警告
- [ ] `cargo test --workspace` 全绿
- [ ] 全功能手测通过（§6 清单）
- [ ] 视觉回归确认（§6 清单）
- [ ] 无 `#[allow(deprecated)]` / `#[allow(...)]` 残留（项目约定禁止用 allow 绕过 lint）
- [ ] `Cargo.toml` 与 `Cargo.lock` 中 egui/eframe 版本一致且为 0.36.x

## 8. 后续机会（升级完成后另立 plan）

1. **`egui::Scene` 评估**：验证 Scene 的 pan/zoom 能否承载画布主视图，减少 `viewport.rs` + 手写交互（注意自定义 LOD、culling、transform handles 未必全覆盖）。
2. **`egui_kittest` 测试基建**：核心交互 GUI 回归测试接入 CI。
3. **`egui_inspection`/`egui_mcp`**：agent 驱动的 GUI 自测。
4. **Atoms 迁移**：`palette.rs` 等图标+文本按钮改 `IntoAtoms`。
5. 清理 preferz-core 中零引用的 `undo` crate 依赖（历史遗留，可顺手处理）。

## 9. 回滚策略

单分支多 commit，任一步骤出问题 `git reset` 到上一个类别 commit 重来；整分支不合并即等效回滚。若最终发现 0.36 存在无法绕过的 blocker（如某 epaint 结构变化导致渲染输出不可接受且无等价 API），整分支放弃回 main；兜底方案为停在 0.34（已含 skrifa 字体等主要收益，但需接受部分 deprecated 写法），届时另议。

## 10. 执行结果记录（2026-09-17，分支 `chore/egui-0.36-upgrade`）

### 10.1 静态验证（全绿）

- `cargo fmt --all --check` ✅
- `cargo clippy --workspace --all-targets -- -D warnings` ✅ 零警告
- `cargo test --workspace` ✅ 180 测试全通过（68+1+104+7）
- `cargo build -p preferz` ✅ 可执行文件链接成功（glow 后端，debug exe）
- 残留扫描：`Rounding / from_id_source / screen_rect() / allocate_new_ui / allow(deprecated) / SidePanel / popup_below_widget / toggle_popup / close_menu / Frame::none / raw_scroll_delta / ctx.style()` 全部零命中

### 10.2 提交切分（3 个）

1. `chore: bump egui/eframe to 0.36` — 版本 bump + Cargo.lock
2. `build: switch eframe to glow backend, drop wgpu` — 后端切换（见 10.3-A）
3. `refactor: migrate egui/eframe call sites to 0.36 API` — 全部调用点迁移

### 10.3 与计划的偏差 / 补充（计划 §3 未预见或需现场定夺）

- **A. 渲染后端 wgpu → glow（环境强制，非计划项）**：本机 `wgpu 30` 在 `windows 0.62`/MSVC 下编译失败（wgpu-hal dx12 绑定 trait bound 不满足），而 eframe 0.36 默认 features 含 wgpu 且**不再含 glow**。因项目代码零 wgpu 引用、AGENTS.md 本就记 glow，故 `eframe = { default-features = false, features = [accesskit, default_fonts, glow, links, wayland, web_screen_reader, x11] }`。⚠ 这意味着运行时渲染器从 0.29 时代的 wgpu 变为 glow——**透明窗口 alpha 合成**（项目曾精调 clear_color/单层中央填充）在两套渲染器下可能不同，是本次最高优先级的视觉回归点。
- **B. `InputState::raw_scroll_delta` 已在上游移除**，仅剩 `smooth_scroll_delta`。画布滚轮缩放与 Present 滚轮翻页改用 `smooth_scroll_delta`（行为近似但带平滑，非逐帧原始值）。需实跑确认缩放/翻页手感可接受。
- **C. `Painter::rect_stroke` 新增 `StrokeKind` 参数**：为与 0.29 的居中描边视觉等价，全部选 `StrokeKind::Middle`（计划建议矩形用 `Inside`，但 `Inside` 会使选中框/画框边线相对旧版内移半个线宽，故选 `Middle` 保持等价）。
- **D. 计划未列出的额外变更点**（编译器暴露）：`ColorImage` 新增必填 `source_size`；`Frame::none()` → `Frame::new()`；`Ui::close_menu()` → `Ui::close()`；`DroppedFile.path` 字段 → `path()` 方法（去 Option）；`Margin` 由 f32 → i8（3 处 `symmetric`）；`FontDefinitions.font_data` 值类型 → `Arc<FontData>`；palette 的 `popup_below_widget`+`Memory::toggle_popup`（后者已私有）→ `egui::Popup::from_toggle_button_response(..).close_behavior(..).show(..)`。
- **E. `App::ui` 内 ctx 的保留方式**：未按计划逐行 `ctx.foo()→ui.foo()`（9211 行大函数，风险高），改为在函数体顶 `let ctx = &ui.ctx().clone();`（Arc 克隆得独立所有权、不借 `ui`），使 `ctx.input/clone/…` 与背景线程 `ctx.clone()` 全部零改动即可编译；仅需 `&mut Ui` 的 Panel/popup 调用点直接传 `ui`。等价且改动面最小。

### 10.4 人工执行项（无头环境无法完成，需 `cargo run -p preferz` 实跑）—— ✅ 2026-09-28 全部通过

§6 全功能手测 + 视觉回归。高优先验证清单（按风险排序）：

1. **透明窗口**：无边框 + bg_alpha 各档（0.1/0.5/1.0）观感是否与升级前一致（受 10.3-A 渲染器切换影响最大）。
2. **滚轮**：画布缩放手感、Present 翻页（10.3-B）。
3. **描边**：选中框、画框边线、多选统一外框、端点/顶点小方块的粗细与位置（10.3-C）。
4. **调色板弹层**：描边/填充色按钮点击弹出与「点外部关闭」（popup 重写）。
5. **设置面板 ComboBox**（语言/主题/画框预设）下拉可正常展开选择。
6. **menu_button**（0.32 起默认点击即关闭，计划 §E）逐项确认弹出菜单点击行为。
7. **文字**：中文清晰度/hinting（0.34 skrifa 红利，应变好）、文字编辑 overlay、字号默认 12.5→13.0 的观感。
8. 快捷键全表 / 导入导出 / 拖放 / 撤销重做 / 变换手柄 / 灰度 / Present —— 常规回归。

## 11. 首轮实跑回归与处置（2026-09-17）

用户 `cargo run` 手测反馈两处异常，已各自定位并修复：

- **Shift+数字 快捷键失效（FitToScreen/ZoomToSelection/Zoom100）**：根因是 egui 0.36 把 `Event::Key.key` 改为「逻辑键」，按 Shift+数字时逻辑键被解析成符号而不再等于 `Key::Num1`，`key_pressed` 只比对逻辑键故失配；`Shift+字母`/`Ctrl+…` 因逻辑键不变仍正常，右键菜单不涉按键也正常。修复：`keymap::pressed_bind` 改为扫描本帧 `events`，命中放宽为「逻辑 key 或 `physical_key` 等于目标」，修饰键仍按 `i.modifiers` 严格全等（裸数字与 Shift+数字仍靠修饰键区分、不误触发），触发沿语义等价。附回归测试 `shift_digit_binding_matches_via_physical_key`。commit `fix: restore Shift+digit shortcuts under egui 0.36`。
- **透明窗口同值更透（0.1 几乎不可见）**：根因即 §10.3-A 的后端切换——glow 为单层线性 alpha 合成（帧缓冲 alpha == bg_alpha），而 0.29 运行时用 wgpu、同值观感更不透明。我们的 alpha 代码逐字节未变。处置（按用户决定）：把背景透明度与图片不透明度两条滑块的**最低值 0.1 → 0.15**，二者保持一致。注意这只抬升控制下限，此前存到 config 里的更低值在用户再次拖动前仍按旧值渲染。commit `tune: raise background/image alpha slider floor 0.1 -> 0.15`。

其余项用户手测未见异常。§10.4 清单里尚未覆盖到的项（Present 细节、灰度、导出、多语言切换等）可继续抽查；本分支已可正常构建运行。**2026-09-28 补完剩余抽查项，本计划结项**（见 CHANGELOG §人工复验清账）。
