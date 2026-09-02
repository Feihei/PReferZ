# PReferZ Changelog

> 本文档是各阶段的**交付归档**（原各 plan 文档中已完成部分的收拢）。
> 前瞻路线图见 [`plan.md`](plan.md)；设计规格见 [`specs/`](specs/)，架构决策见 [`adr/`](adr/)。
> 全部 commit 遵循 Conventional Commits；质量门 `cargo fmt --check` / `clippy -D warnings` / `cargo test --workspace`。

---

## Phase 1–4：MVP（参考图板基础）

无限画布（中键平移 / 滚轮缩放 / F 适应）、图片导入（拖放 / 菜单 / Ctrl+V 粘贴）、
图片项移动 / 缩放 / 旋转、布局整理（自实现 MaxRects 装箱）、`.prz` 存取（SQLite + sqlar，
背景线程解码）、灰度（自实现 BT.601）、取色器、裁剪。零 Qt 依赖，纯 egui/eframe。

## Phase A–E：图形绘制 + Frame Slides（2026-08-29 合入）

> 设计文档 [`specs/shapes-and-frame-slides-design.md`](specs/shapes-and-frame-slides-design.md)；
> 实施计划原 `.agents/shapes-and-frame-slides-plan.md`（已归档删除）。13 个 commit 合入。

- **A Shape 基础集**：`ItemKind::Shape`（Rectangle/Ellipse/Diamond）+ `ShapeStyler` trait
  （`CleanStyler` 精确几何）+ 工具切换快捷键 + 样式面板；命中走 OBB（修正旧 AABB 旋转误命中）
- **B 线性对象**：Line/Arrow 统一重构为 `Polyline` + `start_arrow`/`end_arrow` 属性
  （设计见 [`specs/linear-object-design.md`](specs/linear-object-design.md)，不向后兼容，移除 `ShapeType::Arrow`）
- **C 文本入形**：`ItemKind::Text` + 绑定封闭形状（`container_id`），容器移动/缩放/删除联动
- **D Frame 编号**：画框 + 编号角标编辑（插入式顺移重编号），框选连带成员
- **E Slide 演示**：`AppMode` 状态机 + fit-to-screen 视口动画，F5 进出，方向键翻页

## Phase F：手绘风描边 RoughStyler（2026-08-29，`3a21aef`）

> 设计决策：**零新依赖**，不引入 roughr；自移植 rough.js `_line` 算法
> （原 `.agents/rough-style-plan.md` 已归档删除）。

- `preferz-core` 新增确定性 PRNG `SeededRng`（xorshift64*，seed=0 黄金比例兜底），
  `ItemKind::Shape` 新增 `seed` / `rough` 字段（serde default，旧存档无损）
- `RoughStyler`：逐边抖动三次贝塞尔 + 双描边笔触；抖动幅度以画布像素计量乘 zoom，
  放大观感与真实手绘稿一致；箭头不抖（Excalidraw 同款）
- 后续修正：
  - `d6177b8` 手绘椭圆改为光滑曲线——整圈采样点抖动 + Catmull-Rom 插值成 C1 连续
    贝塞尔闭合环（rough.js ellipse 同款），不再有直线段折角；段数 16→24
  - `ed538cf` 文字默认无背景——`ItemKind::Text.background: Option<[u8;4]>`（默认 None
    全透明，字段为将来侧栏「文字背景」开关预留）；背景矩形改取本次 galley 实际排版结果；
    两条渲染路径合并进 `draw_text_item()`
  - `8eb996a` 绑定文本不可独立选中/拖动——命中重定向容器（Excalidraw 语义：点 label
    即点容器）+ 框选过滤；`Scene::is_bound_text()` 判定，孤儿引用按自由文本处理

## Phase 6 收尾：键鼠映射可配置（2026-08-30，`c8caddd`）

> 原 `.agents/keymap-config-plan.md` 已归档删除；D1/D2/D3 已拍板。

- `BindKey` / `KeyBind` / `Action` / `Keymap` 四类型（binary 层 `keymap.rs`），41 处
  硬编码 `egui::Key` 全部换 `keymap.pressed(Action::X, ctx)` 派发
- 持久化进 `~/.preferz/config.json`（既有手写 JSON 路径，零新依赖）；设置面板可视化
  改绑 + 冲突检测 + 恢复默认
- 实现中修复两处错误：漏 import `KeyBind`（clippy 编译失败）；`rebind` 未沿用槽位
  `on_release` 导致 Ctrl+V 改绑后失效
- 2026-09-01 复盘拍板（[ADR-0007](adr/0007-keymap-no-customization.md)）：**不做用户
  自定义**，默认键位对齐 Excalidraw；后续见 plan.md Phase K 与决策点 D6（面板去留）

## 修复与工程批次（2026-08-29 ~ 2026-09-01）

- `c05d1ae` `feat(ui)`: Enter 编辑选中项文字（`Action::EditText`，双击/Enter 共用
  `start_text_edit`；画完图形立即选中——Excalidraw 语义）
- `f207aaf` `fix`: preferz_app.rs 损坏注释修复（U+FFFD mojibake，少量原字符丢失不可恢复）
- `68394fa` `chore`: 忽略 `.workbuddy/` 项目数据目录
- `0696777` `refactor`: main.rs 复用 lib 导出，消除重复编译同一份模块树（编译单元减半）
- `f341f74` `chore`: 跟踪 Cargo.lock（二进制 crate 需可复现构建）
- `8862638` `docs`: 对齐 Excalidraw 三方向实施规划（G/I/H 概要与决策点 D1-D5 现收录在 [`plan.md`](plan.md)）
- `bace60d` `fix(ui)`: 裁剪遮罩跳过亚像素退化梯形（第一版，未根治旋转图闪黑）
- `74a5869` `fix(ui)`: 裁剪遮罩改单凸多边形压暗整图 + 亮区重绘，根除旋转图闪黑
  （遮罩改由「图片边→裁剪边」4 梯形拼合为「单凸多边形压暗 + 裁剪框内亮图重绘」，
  旋转下对角线不再逐帧重排抗锯齿；UV 以 `current_crop` 为基准避免已裁剪图二次偏移）

## Phase G：明暗两套样式主题（2026-09-01，`8c0bac4`）

> 决策点 D1/D2 拍板（见 plan.md）：主题三态预留 `Auto`（第一版仅手动切换）；画布底色与
> 新建元素默认色随主题翻转。D6 拍板：移除键鼠改绑设置入口，派发架构保留。

- 新增 `theme` 模块：`ThemeMode { Dark, Light, Auto }`（serde `lowercase`，缺省 `Dark`）；
  `Auto` 运行时经 `ctx.system_theme()` 解析为具体模式，系统不可用回退 `Dark`
- palette 语义色集中在 `theme` 模块：画布底色 `canvas_bg()`、新建元素默认描边色
  `default_stroke_color()`，均随主题翻转；启动时用 `default_stroke_color_static()`（Auto 按 Dark）
- `build_visuals(mode, bg_alpha, ctx)`：按主题构造 egui `Visuals`，并把 `bg_alpha` 重新施加到
  panel/window/faint 填充，透明窗口效果在明暗两套主题下都生效（每帧在 `update()` 重建）
- `UserConfig` 增 `theme` 字段（带 `#[serde(default)]`），持久化进 `~/.preferz/config.json`；
  设置面板新增「主题」下拉（Dark/Light/Auto）
- **D6**：移除设置面板里键鼠改绑入口（含 `poll_rebind_capture` 捕获逻辑与 `rebinding`/
  `rebind_notice` 字段），`Action`/`Keymap` 查表派发层完整保留——后续 `default_map()` 出厂默认
  对齐 Excalidraw（Phase K 剩余项）即可，无需用户自定义入口

## Phase I：图形元素类型统一（实施中，2026-09-01 起）

> 决策点 I1–I4 拍板（见 plan.md）；设计文档 [`specs/phase-i-shape-unification.md`](specs/phase-i-shape-unification.md)。
> 分四子阶段交付：A 数据模型 / B 渲染 / C 工具与 UI / D i18n+fileio+验收。

### 子阶段 A — 数据模型（已交付，`ddc6209`）

- 新增 `CurveType` 枚举（Straight/Curved，Catmull-Rom 开/闭曲线）、`ArrowHeadStyle::Dot`
- `ItemKind::Shape` 加 `curve_type` / `roundness` 字段（均 `#[serde(default)]`，旧 `.prz` 自动取默认，I4 决策；`USER_VERSION` 不变）
- 命中检测：闭合多边形支持点在内部（ray casting）选中；曲线经 core 内置 Catmull-Rom 采样成折线再测距（open/closed 两版，供渲染复用）
- 新增命令 `SetCurveType` / `SetRoundness`（`SetClosed` 在 Phase B 已存在）；`ShapeData` 预置 `curve_type`/`roundness` 供子阶段 B 渲染消费
- 单测：曲线/点箭头 serde 往返、闭合多边形内部命中、Catmull-Rom 端点保持与闭合、构造器默认字段；全工作区 `cargo test` 绿

### 子阶段 B — 渲染（已交付，`b9a2248`）

- `stylers.rs` 曲线采样：`Straight` 折线 / `Curved` 经 Catmull-Rom（开/闭两版）；闭合路径 + `fill`；矩形族 `roundness` 经 `radius = min(w,h)*roundness`
- 箭头头：`Arrow` 三角（沿用）+ `Dot` 末端实心圆（rough 下抖动点圆）

### 子阶段 C — 工具与 UI（已交付，`c67f799`）

- 多边形工具 `Tool::Polygon`：`DragState::CreatingPolygon` 多点点击 + 橡皮筋预览，双击/Enter 闭合（≥3 点），Esc 取消，Shift 锁角度
- 选中线性对象显示曲线/闭合/起终点箭头控件（走 `SetCurveType`/`SetClosed`/`SetArrowHeads`）；矩形族圆角滑块（走 `SetRoundness`）

### 子阶段 D — i18n + fileio 旧文件校验（已交付，2026-09-02）

- i18n：Phase H 属性侧栏 key 顺带补全（`Props*` / `StyleTextBackground` / `StyleGrayscale`）
- fileio 旧文件校验：`closed` 补 `#[serde(default)]`（旧 .prz 缺该字段按 `false` 加载，落实 I4「新字段全 default」）；新增单测 `shape_serde_backward_compat_without_phase_i_fields`（缺 curve_type/closed/roundness → Straight/false/0.0）
- 注：手工 GUI 验收由用户跑 `cargo run` 完成（自动化覆盖不到观感）

## Phase H：选中弹出属性侧栏（2026-09-02）

- 右侧 `SidePanel` 按 `ItemKind` 分节（Shape / Text / Pixmap / Frame），选中项 per-item 编辑（plan.md `H` 项）
- D3 多选语义：显示交集值，不一致时仍显示代表值但修改批量应用到所有选中项（Excalidraw 同款，不禁用控件）
- 连续控件（描边色/宽/线型、填充、圆角、字号/文字色/背景、图片不透明度/灰度）拖动期间每帧直接改 item，
  帧末合成为**一条**批量 undo 命令（`PropEdit` + `ensure_prop_edit` + `apply_continuous`），避免 undo 历史被冲垮
- 离散控件直接整批改命令：`SetCurveType`（曲线/尖角）、`SetClosed`（闭合）、`SetArrowHeads`（起/终点箭头）、
  `SetRough`（手绘风）、`SetFrameNumber`（画框编号）
- core 新增命令 `SetPixmapStyle`（不透明度/灰度整份快照）、`SetFrameNumber`；`PixmapStyle` 数据载体（opacity/grayscale）
- 底部样式面板收敛为仅「新建元素默认样式」；per-item 编辑统一收到右侧栏，消除两套控件重复编辑同一字段
- 质量门全绿：`cargo fmt --check` / `clippy -D warnings` / `cargo test --workspace`（67 core + 5 fileio）

## Phase K：默认快捷键对齐 Excalidraw（2026-09-02）

> 依据 [ADR-0007](adr/0007-keymap-no-customization.md)（不做用户自定义，派发架构保留）。
> 出厂默认现状见 `crates/preferz/src/keymap.rs` `Action::default_binds()`。

- 画框（Frame）默认键 `M` → `F`（对齐 Excalidraw frame = F；裸 F 从 FitToScreen 让出）
- 适应画布（FitToScreen）默认键 `F` → `Shift+1`（对齐 Excalidraw「Zoom to fit」；释放出的 F 交给画框）
- 6 个工具对齐 Excalidraw 字母 + 数字双绑定（Excalidraw 1–6 选工具）：
  `Select V/1`、`Rect R/2`、`Diamond D/3`、`Ellipse O/4`、`Arrow A/5`、`Line L/6`
- 刻意保留：`ToolPolygon` = `Shift+P`（Excalidraw 无独立多边形工具，裸 P 是 freedraw 本项目未实现，让开该字母位）；
  `Paste` = `Ctrl+V` 释放沿、`Confirm`/`EditText` = `Enter`（白名单共用）、`Redo` 双绑、`PresentNext` 三绑均不变
- 不变式维持：`no_duplicate_bindings_in_defaults` 测试绿——`Shift+1`（Fit）与 `Num1`（Select）因修饰键严格匹配不冲突
- 质量门全绿：`cargo fmt --check` / `clippy -D warnings` / `cargo test --workspace`

## 手工验收反馈批次（2026-09-02，Feihei 实测 5 项）

- **数字快捷键 1–6 不生效**（`7e840db`）：根因为旧 `~/.preferz/config.json` 持久化了
  Phase K 之前的全量默认表，`Keymap::from_partial` 只补缺失动作 → 新数字绑定永不加载。
  D6 移除改绑入口后持久化 keymap 只可能是过期默认值，故加载时忽略 keymap 字段
  恒用出厂默认（`Keymap::new()`，字段保留兼容解析）
- **绘制工具状态侧栏**（`073a4f1`）：移除底部 style_panel；无选中且绘制工具激活时
  右侧栏显示「新建元素默认样式」（描边色/宽/线型/填充/手绘风；Frame 不显示，
  线/箭头无填充节）。选中 item 时仍显示原 per-item 属性侧栏
- **Excalidraw 式调色板**（`073a4f1`）：新控件 `ui/widgets/palette.rs`——
  open-color 五档全色板 + top picks 行（描边 `STROKE_PICKS` / 填充 `FILL_PICKS`）+
  自定义取色入口；暗色主题套 Excalidraw 同款 `invert(93%) hue-rotate(180°)` 纯数学滤镜
  （逐式移植自 `.ref/excalidraw` colors.ts）。侧栏描边/填充色与默认样式面板共用
- **填充四态 + 立即生效**（`783c96f` + `073a4f1`）：core 新增 `FillStyle { Solid, Hachure,
  CrossHatch }`（`#[serde(default)]`，旧文件 fill=Some → Solid 语义不变），
  `SetShapeFill` 扩展为颜色+样式整份快照（`FillState`）；Item 增 `with_fill_style`。
  UI 四态图标选择器（无/纯色/斜线/交叉线）替代旧 "No fill" checkbox；选非 None 样式时
  fill 为空则立即取**各自描边色**（多选逐 item 判断），不再要求先选颜色。
  渲染：hachure = rough.js 同款 -41° 平行线（扫描线求交，gap=4×线宽），
  cross-hatch = ±90° 两组；CleanStyler 填充层走精确几何，RoughStyler 抖动端点
  （独立 xorshift 种子，确定性）。CleanStyler 填充层在 `fill: None` 时跳过
  （修复 3 处单测回归：无填充回到单轮廓形状）
- **线类段中点拖拽加点**（`57aa574`）：`Handle::SegmentMid(usize)`——选中线/箭头/多边形时
  段中点显示小号浅黄手柄，拖拽即在段中间插入顶点并进入既有 LineEndpoint 拖拽
  （Straight → 折线；Curved → Catmull-Rom 控制点）；闭合多边形含收尾段。
  `DragState::LineEndpoint` 新增 `base_pos`（被拖顶点拖拽起始位置）：
  加点拖拽的 undo orig 用**插入前**点集，undo 一步即移除新顶点；
  点击中点未拖动则静默移除插入顶点、不产生空命令
- 质量门全绿：`cargo fmt --check` / `clippy -D warnings` / `cargo test --workspace`（125 测试）
