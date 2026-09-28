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

> 决策点 D1/D2 拍板（见下方 §决策点归档）：主题三态预留 `Auto`（第一版仅手动切换）；画布底色与
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

> 决策点 I1–I4 拍板（见下方 §决策点归档）；设计文档 [`specs/phase-i-shape-unification.md`](specs/phase-i-shape-unification.md)。
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

---

## 拖拽修饰键 + Ctrl+D 原位复制（plan #11，2026-09-03）

Excalidraw 打磨批次快赢项 #11，三处协同改动：

- **core**：新增 `AddItems` 批量命令（`preview_already_applied` 跳过首次 redo，用于预览已落地的副本）；
  `Scene::duplicate_items(ids, offset)` 克隆选中项、生成新 UUID、修正绑定文本的 `container_id`
  （容器内副本改绑容器副本，容器外的绑定文本退化为自由文本）。3 个单测覆盖 id 重映射。
- **Shift 轴约束**（`DragState::MoveItems` 增 `pending_deselect`）：Shift+拖动取 `delta` 两轴中绝对值
  较大者为约束轴、另一轴清零（PowerPoint/Excalidraw 同款）。Shift+点击**已选中**项不再立即取消选中，
  改为释放时判定——没移动则取消选中、移动则轴约束移动（否则 Shift+拖动永远触发不到约束）。
- **Ctrl+拖动复制移动**：`begin_drag` 检测 Ctrl 即按"起点"克隆副本入场景、改选副本集，预览移动副本；
  释放时按**最终位置** push `AddItems(preview_applied)`，一次 undo 撤掉整个「复制+移动」。
  Ctrl+点击未移动则不建副本（删掉重叠副本 + 恢复原件选区），避免误生成隐身副本。
- **Ctrl+D 原位复制**：新增 `Action::DuplicateInPlace` 绑 `Ctrl+D`（裸 D 仍是菱形工具，修饰键严格匹配不冲突）；
  `duplicate_in_place()` 带容器联动复制、偏移 10px 画布、选区切到副本、push `AddItems(preview_applied)`。
- i18n 增 `FlashDuplicated`（三语，含计数 `{0}`）。

---

## 缩放范围限制（plan #12，2026-09-03）

- `ViewportState` 默认 `zoom=1.0`、`min_zoom=0.1`（10%）、`max_zoom=10.0`（1000%）；
  原默认 1%–10000% 过于极端（缩到底剩几个像素、放到顶糊成一片，滚轮要转很久才有可见反应）。
- `.prz` 元数据 `ViewportMeta` 加载时按新范围 `clamp`——旧文件可能存越界值，不收敛会在越界区间"空转"好几圈。
- Present 模式仍不 clamp 上限（小 frame 需放大填充），退出演示恢复 `saved_zoom`，不受新范围影响。
- 单测覆盖 clamp 边界。

---

## 多元素对齐与分布（plan #6，2026-09-05，`cad908b`）

> 决策点 2026-09-05 拍板（`67ef930`），同日实现交付。人工验收待 Feihei `cargo run` 确认。

- **core（`preferz-core/src/arrange.rs`）**：
  - 新增 `AlignMode`（Left / HCenter / Right / Top / VCenter / Bottom 六向）、
    `DistributeAxis`（Horizontal / Vertical）、`DistributeMode`（`Gap` 等边界间距 /
    `Centers` 等中心距）；
  - `plan_align(scene, ids, mode)` / `plan_distribute(scene, ids, axis, mode)` 均返回
    `(ItemId, old_pos, new_pos)`，与既有 `plan_arrange` 同构，**复用 `ArrangeItems` 命令**
    入 undo 栈（未新增命令类型）。
  - 参考系 = 参与项 AABB 并集（选区包围盒）：对齐贴向该框对应边/中线；分布**保持首尾
    元素不动**，仅调整中间项（Gap：`(span - Σ尺寸)/(n-1)`；Centers：首尾中心等分步长）。
  - 边界处理：参与项过滤 `Scene::is_bound_text`（绑定文本位置由容器决定，独立平移会与
    容器错位，且不参与包围盒计算）；对齐要求 ≥2 项、分布要求 ≥3 项；位移 ≤ `1e-4`
    不产生 undo 条目（避免"已经在位"的噪音历史）。
  - 8 个单测覆盖：六向对齐位移、分布两种基准的间距/中心距、轴向互不影响、
    ≥2 / ≥3 数量门槛、绑定文本排除。
- **UI（`preferz` binary）**：
  - 右侧属性栏在**选中 ≥2 项**时顶部新增「对齐」节：两行 6 个图标按钮（⇤ ↔ ⇥ / ⇡ ↕ ⇣，
    带 tooltip）+ 「分布」4 个按钮（横向等距 / 横向等心 / 纵向等距 / 纵向等心）；
    <3 项时分布按钮禁用并 hover 提示「分布需要至少 3 个元素」。
  - 右键菜单「排列」子菜单内追加「对齐」与「分布」两个嵌套子菜单（同一套动作）。
  - i18n 新增 13 条中英文案。
- 决策点结论：**分布两种基准都做**；参考系 = **选区包围盒**；UI = **属性栏 + 右键菜单**
  （不做画布浮动工具条）。

---

## 直线/箭头端点吸附图形边缘（plan #5，2026-09-07，`a803bbe`）

> 决策点 2026-09-06 拍板（plan.md §5），同日实现交付。人工验收待 Feihei `cargo run` 确认。
> 与 #7 连接符共用绑定模型。

- **吸附**：拖 `LineEndpoint` 时，仅真实端点（`points[0]` / `points[last]`）可吸附；查询点取
  **另一端**的画布位置（Excalidraw 风格，吸附发生在靠近目标的一侧）。`Scene::find_snap_target`
  遍历所有非绑定文本 item，离散化其轮廓（`outline_segments`：矩形/菱形 4 边、椭圆 64 边采样、
  多段线用自身顶点），求最近点；落屏 **10px** 阈值（`10 / zoom`，随缩放保持手感）内才吸附，
  超出则拖离并解除该端点绑定。命中目标时高亮其轮廓（`snap_highlight` + `FlashSnappedToShape`）。
- **绑定存储**：`ItemKind::Shape` 增 `start_binding` / `end_binding: Option<ItemId>`
  （`#[serde(default)]`，旧 `.prz` 自动按未绑定加载，向后兼容）。**吸附点不存绝对坐标**，
  每次形状移动时由 `Scene::resolve_bindings` 按「另一端点的方向」动态重算轮廓最近点——
  比 plan 原写的 `(ItemId, side)` 更贴近 Excalidraw 动态绑定语义，且天然兼容多段线。
- **联动**：`resolve_bindings(moved_ids)` 两遍（只读计算 → 应用）→ 仅重定位 `moved_ids` 中
  形状的绑定端点，并同步 `base_size`；绑定目标被删除时自动清绑（`DeleteItems` 的 redo/undo
  亦触发）。注入 `TransformItem` / `MoveItems` / `ScaleItems` / `RotateItems` / `FlipItems` /
  `ArrangeItems` 的 redo+undo，以及 `LineEndpoint` 释放、`MoveItems` / `HandleTransform`
  拖拽预览——端点在形状移动/缩放/旋转时实时跟随。
- **core 新增**：`snap.rs`（`outline_segments` / `project_point_on_segment` /
  `nearest_point_on_segments` / `nearest_outline_point`，6 个几何单测）；`item.rs` 加
  `canvas_to_local_point` / `local_point_to_canvas` / `start_binding` / `set_*_binding`；
  `EditShapePoints::with_binding_change`（绑定变更与顶点变更同入一条 undo）；
  `Scene::find_snap_target` / `resolve_bindings`（2 个联动单测）。
- **UI**：`SNAP_THRESHOLD_PX=10` 常量；`PReferZApp` 增 `pending_endpoint_binding` /
  `snap_highlight`；`render_scene` 高亮吸附目标轮廓；i18n 增 `FlashSnappedToShape`
  （中「已吸附到图形边缘」/ 英「Snapped to shape」）。**直线/箭头均可绑**。
- 质量门全绿：`cargo fmt --check` / `clippy -D warnings` / `cargo test --workspace`（96 测试，
  +8 为本次新增）。

---

## 元素编组 / 解组（plan #13，2026-09-08，`4033726`）

> 决策点 G1–G4 2026-09-08 拍板（全按倾向列，见 plan.md）。人工验收待 Feihei `cargo run` 确认。

- **core**：`Item.group_id: Option<Uuid>`（`#[serde(default)]`，旧存档无损）；
  `Scene::group`（≥2 项赋同一新组 id）/ `ungroup`（清空指定项）/ `group_members_of`
  （全组成员含自身）/ `expand_to_groups`（命中集 → 整组扩展）；`SetGroup` 命令快照每项
  `(old, new)` 组 id，换组/解组 undo 一步还原。
- **G1 单组**：一元素至多一组；已属别组的项与其它项一起编组即自动换组（旧组剩余成员保留）。
- **G2 点击整组**：点击组内任一成员即选整组；框选收实际命中成员；Shift+点击已选整组
  → 释放未拖动时整组取消选中（`pending_deselect` 存代表成员，释放时扩展回组）。
- **G3 部分操作**：删除/拖出某成员，其余成员保持编组；彻底解组需显式 `Ctrl+Shift+G`。
- **G4 组粒度操作**：`Ctrl+D` / 对齐 / 分布作用于整组（选区=整组）；选中整组走现有多选
  统一外框（不逐成员画手柄）。
- **复制语义**：整组复制 → 副本共享**新**组 id（组结构保留）；部分复制 → 副本退化为
  未编组（避免点击副本选中原组的悬空引用 bug）。
- **UI**：`Ctrl+G` / `Ctrl+Shift+G`（Excalidraw 同款）+ 右键菜单「编组/解组」入口；
  i18n 增 4 条文案与 2 个动作标签。
- **fileio**：`.prz` items 表加 `group_id` 可空列；`PrzFile::open` 用 `pragma_table_info`
  检测旧文件缺列就 `ALTER TABLE` 补列（就地迁移，一次完成）；保存/加载组 id 往返。
- 8 个新单测（6 scene：编组/换组/部分解组/命令往返/整组复制/部分复制 + 2 fileio：
  组往返/旧 5 列 schema 迁移）；工作区 157 测试。

---

## 画框演示比例预设（plan #3，2026-09-17）

> Excalidraw 无此功能（`.ref/excalidraw` 画框仅自由缩放、无比例/纸张预设），为 PReferZ 产品特性。
> 设计沿用 `.issues` #3 并做保守取舍：入口收在选中画框的属性栏，不在创建期做候选面板。

- **core**：新增批量命令 `SetFrameSize`（`commands.rs`）+ `FrameGeom{pos, base, scale}` 几何快照，
  一次套用 = 一条 undo。redo 写新几何（base_size 落尺寸、scale 归 1、pos 按中心重算），
  undo 精确还原（含套用前 `scale≠1` 的情形）。1 个往返单测。
- **app**：`FramePreset` 枚举 + `FRAME_PRESETS` 清单（演示比例 `16:9/16:10/4:3/3:2/1:1`
  + 纸张 `A4 竖/横`）；`apply_frame_preset` 几何换算——
  - 比例类：保持画框当前**有效长边**长度、只调短边到目标比（缩放观感稳定，不引入 DPI 假设）；
  - 纸张类：固定像素绝对值（A4 按 **96 DPI** 换算 210×297mm → 794×1123）；
  - 两种均以**画框中心**为锚点重算左上角。`render_frame_props` 编号节加标签 + 新增「比例/纸张预设」
    `ComboBox`，点击即套用并 flash；多选画框一次批量应用。
- **i18n**：新增 `PropsFrameNumber` + 预设标签/`FramePresetLabel`/`FramePresetPick`/
  `FlashFramePresetApplied` 共 10 词条（比例名为 ASCII，A4 用 "A4 portrait/landscape"，
  英文表无 CJK 回归闸通过）。
- **取舍**：自定义比例输入本轮不做（留后续）；仅改画框边框几何，框内成员按既有几何归属
  在演示进入时重算（与手柄缩放同口径）。
- 待 `cargo run` 人工验收（选中画框→属性栏套预设→中心不变地变形→`Ctrl+Z` 一步还原）。

---

## 架构整固：core 下沉 + preferz_app.rs 拆分（Steps 0–3，2026-09-17~18）

> 框架级重构，**全程零行为改动**、分步落地、每步独立跑质量门；测试计数恒定
> （bin 62 / core 113 / fileio 7）。原 `.agents/plans/core-sink-and-app-decomposition.md`
> 已归档删除，本节即其交付收拢。分层不变量已固化进 `AGENTS.md`「Architecture /
> Must-Know」与 [ADR-0008](adr/0008-viewport-in-core.md)。

- **触发**：`preferz_app.rs` 单文件 9230 行（全库 src 的 43%）= 装了 9 类职责的上帝对象；
  binary（13681 行）远厚于 core（6870 行），core 显薄。**边界结论 = 保留三 crate 不合并**
  （兑现而非推翻 [ADR-0001](adr/0001-workspace-crate-layout.md)）：真问题是"太多 L1 被困在
  binary"，方向是往 core 下沉，不是往上合并。
- **分层判据（L1/L2/L3）**：core 只收"无副作用、只吃领域数据的决策"（L1）；egui↔core 编排
  （L2：读事件、活 `ViewportState`/`UndoStack`/`DragState`、`request_repaint`、`BackgroundOps`、
  config 落盘、键位派发）与 egui 渲染（L3）明确留 binary（D2 已拍板）。
- **Step 1**（`9462e8f`）：纯决策下沉——`snap_polygon_point`→`preferz_core::snap`（+4 单测迁）、
  新增 `Scene::content_bounding_rect`（+2 单测），`fit_to_screen`/`zoom_to_selection` 改委托 core。
  `Prop`/`prop`/`prop_cmd` 属性面板经代码事实判定为 L2 机器，**不下沉**。
- **Step 2（枢纽）**（`7b70c2a` + ADR-0008 `a27704d`）：`ViewportState` 下沉 `preferz_core::viewport`
  （纯 `Screen*`/`Canvas*` euclid 类型，3 测试随之迁入）；`app/viewport.rs` 收为**桥接层**
  （重导出 + 本地扩展 trait `ViewportEgui`）。egui↔core 坐标边界收缩成唯一两个转换点
  （`Pos2↔ScreenPoint`、`Rect↔ScreenRect`），只落在 L2 输入边 / L3 绘制边。修订
  [ADR-0002](adr/0002-coordinate-systems-euclid.md) 的 "Viewport→binary" 归属行。
- **Step 3**（`20ef824`/`b62bc96`/`cb0b304`/`8df4c14`）：`preferz_app.rs`→`preferz_app/mod.rs`，
  拆为 **12 个同级子模块**（export/config/background_ops/render/present/text_edit/context_menu/
  file_io/actions/settings/drag/props）；mod.rs **9230→2839**，只留 struct + 共享类型/常量 +
  `ui()` 派发 + infra + 快捷键。机制：子模块 `use super::*;` 够到父私有项，搬出方法加 `pub(crate)`；
  逐字搬迁、每簇三件套全绿。
- **Step 4（按 D2/D3 判定不做）**：`update_drag_preview` 的 `(起点,当前点,aspect,snap,mode)→
  Transform/Command` 与端点/延伸/闭合交互态强耦合 egui 事件与瞬时 `DragState`，属 L2 编排，
  留 app；如未来要无头测，先写 core 决策单测钉死语义再动壳。
- **文档面**（`97bc505`）：`AGENTS.md` 同步架构布局、L1/L2/L3 分层与 `ViewportEgui` 坐标边界约定。
- 自动化门全绿 + `cargo run` 启动无 panic；GUI 交互动画手感为 Feihei 手工验收项。

---

## 徒手绘制（freedraw）速度锥形墨迹（plan #10，2026-09-18~20）

> 向 Excalidraw 对齐的徒手笔迹：新增 `Tool::Freehand` + `ItemKind::Freedraw`，用**运笔速度**
> 模拟压感（egui 无指针压感输入）给出「收笔尖、运笔粗」的锥形墨迹。交付四步：
> `1a8cda6`（墨迹主体）→ `f869b86`（改逐段描边，修「画成区域而非描边」）→
> `b70c2a3`（选中可改属性）→ `e1f46d8`（Catmull-Rom 平滑）。✅ 2026-09-20 Feihei `cargo run` 复验通过。

- **数据模型**：`ItemKind::Freedraw { points(局部坐标，AABB 左上原点), pressures(相对笔宽乘子
  0..1，速度锥形), stroke_width(基准宽，画布单位), color }`。**形状与粗细分离**——`pressures`
  只承载「每点多细」的相对乘子，绝对宽 = `stroke_width × pressure × scale`；改粗细只动一个标量，
  属性 undo 快照因此保持 `Copy`（不随点携带 `Vec`）。
- **core（L1，无头可测）** `freedraw.rs`：
  - `pressures_from_spacing(points, zoom)`——相邻点间距 / zoom 当速度代理（快→点稀→细），
    三点滑动平均 + 首尾 `apply_end_taper` 收笔锥形，乘子恒 ∈ (`MIN_PRESSURE`, 1]，zoom 稳定；
  - `smooth_centerline(points, pressures, samples)`——Catmull-Rom 逐段加密重采样（`SMOOTH_SAMPLES=8`）
    + 逐点压力线性插值；端点严格保留；少于 3 点或点/压力长度不匹配时原样直通；
  - `Item::new_freedraw`（AABB 局部归一，`pos` = AABB 左上）+ `base_size`/`bounding_rect`/
    `contains_canvas_point`（到中心线距离 ≤ 该点半宽 + `HIT_SLOP`）加 Freedraw 分支；
  - 批量命令 `SetFreedrawStyle` + `FreedrawStyle{color, stroke_width}`（`Copy` 快照，只回写
    宽/色两点，点集与压力不动）。
- **stylers（L3）** `freedraw_stroke_shapes`：沿中心线**逐段** `line_segment`（段宽 = 两端点半宽之和）
  + 每点 `circle_filled` 圆帽拼描边。**关键教训**：不能把中心线两侧偏移闭合成 ribbon 轮廓再填充——
  弯曲/回环处两侧边界自交，非零环绕三角化会把内部填成实心区域（即用户所报「画出区域而非描边」）。
  `build_freedraw_visuals` 把局部点/宽经 `to_screen` 变到屏幕（宽随 item scale + zoom 自然缩放），
  终稿与实时预览共用同一套描边函数。
- **app（L2）**：`Tool::Freehand` / `Action::ToolFreehand`（裸 `P` + `Num7` 双绑，对齐 Excalidraw
  字母位与本项目数字工具行）；`DragState::Drawing{raw}` 按下采集、移动按屏幕 2px 阈值过滤追点、
  释放 `finish_create_freedraw`（raw → `pressures_from_spacing` → `smooth_centerline` →
  `new_freedraw` → **一条** `AddItem` undo，<2 点丢弃、回 Select、Esc 丢笔）；实时预览同样走
  `smooth_centerline`，与落笔定型完全一致（WYSIWYG，命中检测与绘制同源）。
- **props（选中可编辑）**：`render_props_panel` 按 kind 分节，Freedraw 单列「墨迹」节
  `render_freedraw_props`——颜色调色板 + 粗细滑块（`0.5..=12` 对数），均走 `apply_continuous`
  + 新 `PropKind::Freedraw`/`PropValue::Freedraw`。dash/fill/roundness 对笔迹无意义故不给。
- **fileio**：kind 整体存于 `items.data` JSON blob，**无 `.prz` schema 迁移**（version 仍 3），
  仅 `item_kind_str` 补判别串 `"freedraw"`；存读往返单测。
- **i18n**：`ToolFreehand`（徒手 / Freedraw）、`FlashFreedrawCreated`（已绘制墨迹 / Ink created）、
  `PropsSectionFreedraw`（墨迹 / Ink）、`Action::ToolFreehand` 标签；英文表无 CJK 回归闸通过。
- **与 Excalidraw 对齐核实**：Excalidraw 的 freedraw **不吃 roughness/sloppiness**（那是几何形状级
  属性），笔迹面板暴露的是 stroke width / color / `pressure`（恒定↔变速切换）/ strokeStyle / opacity。
  故本项属性面维持「颜色 + 粗细」与之一致，**未加**手绘风格档；后续若要对齐，有意义的候选是
  pressure 开关（`pressures` 已存，切换成本低）与 strokeStyle 虚线，而非 roughness。
- 测试：core（速度→压力 5 + 平滑 3 + item 3 + 命令 1）、stylers（描边非填充 + 退化/非 Freedraw
  返回空 2）、bin 手势（锥形墨迹 + 退化丢弃仍回 Select 2）、fileio 往返 1；三件套计数
  bin 62→66 / core 113→125 / fileio 7→8。

---

## 两列数据粘贴成柱状/折线图（plan #8，2026-09-20，`fafd262`）

> 🔶 代码交付，待 `cargo run` 人工复验。拍板：新 `ItemKind::Chart` 矢量渲染（非 Pixmap）、
> 单系列、粘贴触发。

- **数据模型**：`ItemKind::Chart { chart_type(Bar/Line), base_size, labels, values, color,
  stroke_width }`（core `item.rs`，`ChartType` 枚举 + `CHART_DEFAULT_SIZE`(440×300) /
  `CHART_DEFAULT_COLOR`(Excalidraw 蓝) 常量 + `Item::new_chart` 构造器）；`base_size()` 走
  Frame 同款分支，变换框/命中/排列天然可用。
- **粘贴检测顺序**：`paste_from_clipboard` **先 `get_text()`**——Excel/表格软件复制单元格时
  剪贴板同时带位图+文本，图片优先会把数据表截成位图；文本解析为 2 列数值 → 暂存
  `pending_chart` 弹「柱状/折线/取消」选择浮层；非 2 列数值回退图片路径（行为不变）。
- **core（L1，无头可测）** `chart.rs`：`parse_two_column_data`——TSV/CSV、CRLF、首尾空行、
  ≥2 数据行、仅首行可作表头丢弃（表头+单数据行拒绝）、恰 2 列否则拒绝、支持负值；8 单测。
- **渲染**：`draw_chart_item`（render.rs）逐段矢量绘制——横向网格 + 数值刻度 + 左/下坐标轴 +
  柱（凸多边形，零线随数据范围浮动，支持负值）或折线（含圆点标记）+ 底部类别标签；线宽/
  字号乘 `scale`（item scale × zoom）随视图缩放；文字不随 item 旋转（首轮取舍，标签恒水平）；
  明暗主题各自取轴/网格/标签色。
- **fileio**：`item_kind_str` 补 `"chart"`；kind JSON blob 无 schema 迁移；存读往返单测。
- **取色器**：`sample_item_pixel` Chart 分支与 Text 同款简化采样（命中返回系列色）。
- **i18n**：`ChartChooser*` 4 词条 + `FlashChartCreated`；`create_chart_from_pending`
  落点=视口中心（同图片导入），一条 `AddItem` undo。
- 测试：core 解析 8 + fileio 往返 1（中文标签/负值/尺寸/颜色断言）。

## mermaid 代码转流程图（plan #9，2026-09-20，`edb6a6a`）

> 🔶 代码交付，待 `cargo run` 人工复验。拍板：受限自研 Rust 解析器（零依赖，否决 WASM）。

- **core（L1，无头可测）** `mermaid.rs`：
  - `parse_mermaid_flowchart`——首行 `flowchart|graph TD|TB|BT|LR|RL`；节点 `id[标签]`(矩形)/
    `id(标签)`(椭圆)/`id{标签}`(菱形)/裸 `id`；边仅 `-->`（可链式 `a --> b --> c`，重复引用
    合并同节点、定义覆盖裸引用默认）；边标签 `|text|`、无向线/虚线/粗线箭头、subgraph 均
    **报错并带行号**；`%%` 注释与空行跳过；8 单测。
  - `layout_flowchart`——分层布局：层级=最长路径深度（边松弛至多 n 轮，环自然收敛不挂起），
    同层按出现顺序排布、整行交叉轴居中；层间距/同层间距 `MERMAID_GAP=100`（对齐 plan #7
    `FLOWCHART_GAP` 语义）；节点尺寸按标签估宽（CJK 14px/ASCII 8px，菱形加宽防文字出界）。
- **app 生成** `generate_mermaid_flowchart`（actions.rs）：解析+布局 → 每节点一个 Shape +
  `Item::new_text_in` 绑定文字（随容器联动），每边一条两端 `EndpointBinding` 绑定的直箭头
  （复用 plan #7 `edge_anchor_local`，锚点=双方相对的边中点、主轴取中心差主导轴）→ 整批
  `AddItems` 一条 undo、生成后全选节点；落点=布局包围盒居中到视口中心。解析失败**不关窗、
  保留输入**，错误经 flash 指出行号。
- **入口**：右键菜单「Mermaid 图表…」（🔷）→ 居中弹窗：多行 code editor 输入 + 支持语法提示
  + 生成/取消。
- **i18n**：`MenuMermaid`/`MermaidTitle`/`MermaidPlaceholder`/`MermaidGenerate`/
  `FlashMermaidParseFailed`/`FlashMermaidCreated`。
- 测试：core 解析 5 + 布局 3；bin 生成（8 item = 3 形状+3 绑定文字+2 箭头、两端绑定、
  一条 undo 一步撤回/redo 恢复）+ 解析失败保留输入 2。

## 自动保存（plan #5，2026-09-20，`96cf8ac`）

> 🔶 代码交付，待 `cargo run` 人工复验。拍板：独立 `.prz.autosave`（不覆盖原文件）+
> 30s debounce + 打开文件时恢复提示。

- **触发**：`mark_dirty()`（收口 push_cmd / perform_undo / perform_redo 的 dirty 标记）重置
  debounce 计时起点 `autosave_dirty_since`；update 循环 `tick_autosave`——启用 && dirty &&
  打开着 `.prz` && 无保存任务在途 && 距上次变更 ≥ 间隔（默认 30s，最小 10s）→ 后台写
  `foo.prz.autosave`（`autosave_path_for`：原路径追加 `.autosave`）。**不进 undo、不清除文档
  未保存状态、不改 current_file**（原文件 Ctrl+S 语义不变）；手动保存成功时计时清零。
- **结果分流**：`poll_background` 按目标路径后缀 `.autosave` 识别——自动保存静默完成（失败才
  flash），不触发「已保存」/ pending 关闭新建动作；手动保存分支原样。
- **恢复**：`finish_load` 打开成功后 `newer_autosave_for`——同目录 `.autosave` 存在且 mtime
  晚于原文件（原文件 mtime 不可得时有备份即提示）→ 弹「恢复/忽略」浮层；恢复=载入备份内容
  但 current_file 仍指原 `.prz`、内容视作未保存（dirty），不污染最近文件列表；忽略=保留备份
  不删。
- **设置**：面板新增「自动保存」节——开关 checkbox（默认开）+ 无操作秒数滑块（10–300），
  变更即持久化；`UserConfig` 增 `autosave_enabled`(serde default fn=**true**，手写
  `Default` 防止缺省 false)/`autosave_interval`(default 30)。
- **fileio 重构**：`start_save` 拆出 `start_save_common`（文案参数化），新增 `start_autosave`
  （进度条文案「自动保存: {path}」）；`BackgroundOps::start_save_msg` 承接（原 `start_save`
  删除，文案上移调用方）。
- **i18n**：`SettingsAutosave*` 3 / `ProgressAutosave` / `AutosaveRestorePrompt` /
  `AutosaveRestore` / `AutosaveDismiss` / `FlashAutosaveFailed`。
- 测试：bin 5（路径后缀、备份检测新/旧/无、tick 触发写盘且保持 dirty/计时清零、未命名与
  未到时不触发、分流谓词）。

## 流程图同向创建改分叉避让（plan #7 二次更正，2026-09-22，待提交）

> 反馈：选中已有下一节点的图形再按 `Ctrl+同向`，新节点全叠进"邻居远边"同一格 → 重叠，
> 未形成分叉。原 2026-09-14 `8ba4ac5` 的"邻居旁外推"（主轴=邻居远边 + GAP）根治不了
> （`find_connected_neighbor` 恒返回同一最近邻居，反复同向创建必然叠位）。

- **落位算法移植**：新增 core 纯函数模块 `preferz_core::flowchart::place_node`（egui-free、
  L1、可无头单测）——移植 Excalidraw `flowchart.ts::placeCluster` 的单节点退化：`mergeIntervals`
  + `intervalIsFree` + `findNearestFreeSlot`（两侧搜索、平票偏正方向）。
- **语义**：**主轴**恒固定在**源**边界外 `FLOWCHART_GAP=100`（不随邻居外推）；**交叉轴**把新节点
  在该列带内滑到离源中心**最近空位**。同向已有下一节点 → 自动上下**分叉**；无邻居 → 退化为原
  "源旁 100px、交叉轴居中"单链（回归零变化）。
- **障碍集**：binary `connected_flowchart_rects` 沿双端绑定 Polyline 双向 BFS，取与源同**连通子图**
  的节点包围盒（对齐 Excalidraw `getConnectedFlowchartNodes` #8518，无关散落图形不干扰）。
- **仍不做** pending 簇预览/簇增长/抬起提交（保持"按下即提交 + 选区跳新节点"）；仍用直箭头（无
  elbow），分叉时箭头=源边中点→新节点对边斜线。
- **代码面**：`actions.rs::add_connected_shape` 落位块换成 `flowchart::place_node`；binary 的 `FlowDir`
  在调用点映射到 core `flowchart::FlowDir`；`find_connected_neighbor`（仅供 `Alt+方向` 导航）不变。
- **测试**：core `flowchart.rs` 6 项（居中/占用滑下方/第三个取最近上方空位/带外障碍忽略/向上方向…）；
  原 binary `add_connected_shape_places_sibling_next_to_existing_neighbor` 改写为
  `add_connected_shape_forks_around_existing_neighbor`（C=(230,190) 上下分叉，替代旧 (450,10) 外推）。
  质量门全绿（fmt / `clippy -D warnings` / `cargo test --workspace`：core 171 + lib 75 + main 2 + fileio 9）。

## 折线新增直角折线 elbow 曲线模式（plan #7 配套，2026-09-22，待提交）

> 承接上一节：分叉的斜向箭头不够好看。Excalidraw 的 elbow **不是新元素类型**，而是线性
> 元素上的一条路径模式（`type:"arrow"` + `elbowed:bool`，`types.ts:357`；子类型由
> `getLinearElementSubType` 从 `elbowed`/`roundness`/端点 派生，`typeChecks.ts:359-372`）。
> 本仓库线性对象统一在 `ShapeType::Polyline`，路径模式走 `CurveType`（`Straight`/`Curved`）——
> elbow 就是给它加第三态。

- **零 schema**：`CurveType` 加 `Elbow`（serde `"elbow"`，`#[serde(default)]` 旧档回落 `Straight`）；
  `ItemKind::Shape` 不加字段——elbow 是**渲染期即时展开**（与 `Curved` 的 Catmull-Rom 同策），
  `points` 仍只存两端点。
- **core `item::elbow_polyline`**（L1、命中与渲染共用、可无头单测）：两点 → 正交折线；启发式
  **沿较小 Δ 的轴先走短腿再垂直转折**。对流程图连线恒正确（主轴间距固定 = gap、叉距总更大
  → 首末段沿两端朝向边界的法线离开/进入：右/左连线水平先走、上/下连线垂直先走）；近共轴 → 直线。
  无需存 lead-axis。
- **接缝（三处一致）**：渲染 `ui::stylers::outline_points` 的 `Elbow` 分支、命中
  `item::contains_canvas_point` 的 `Elbow` 分支——二者调同一 `elbow_polyline`；**逐像素导出**走
  命中测试，自动跟随。`RoughStyler::is_smooth` 对 Elbow 返回 false（当直边逐段抖动，可接受）。
- **UI**：属性栏线性对象「边角」选择器 2 态 → 3 态（尖角/圆滑/直角），i18n `StyleCurveElbow`
  （EN `Elbow` / ZH `直角`）。`SetCurveType` 命令与 `push_poly_curve` 天然支持第三态，零改。
- **默认**：`add_connected_shape` 新建的流程图箭头 `curve_type = Elbow`（分叉自动正交；两端共线的
  首链仍是直线）。
- **本轮仍不做**：绕节点障碍的 elbow 路由（Excalidraw 完整版 A*）——列为后续；见 plan.md 决策 D4。
- **测试**：core 新增 3 项（较小 Δ 轴选向 / 正交+端点保持 / 共轴与非两点回退）；
  质量门全绿（fmt / `clippy -D warnings` / `cargo test --workspace`：core 174 + lib 75 + main 2 + fileio 9）+ 无头冒烟。
- **编辑护栏（同批修复）**：elbow 线**不暴露段中点加点手柄**——`transform_handles` 的命中
  与绘制两处按 `is_elbow_line` 跳过 `SegmentMid`。原因：段中点手柄按**存储两点（对角弦）**取位，
  拖它会 `points.insert` 一个真顶点使 `points.len()!=2` → `elbow_polyline` 判非两点、直接返回原始
  折线 → 退化成多段线且属性栏切回“直角”也不重路由（改不回）。护栏后 elbow 恒两点、只拖端点 +
  面板可自由来回切。**多拐点自由重路由**需真正的直角路由器（naive「每段各自正交化」会在抓拐点瞬间
  把另一根 bar 顶偏、形状跳变），登记为 plan 前瞻项、暂不做。新增谓词回归测试 1 项（lib 76）。

---

## elbow 直角折线 bar 可拖（plan #16 E1，2026-09-23，待提交）

> 承接上一节：elbow 曲线模式已交付但走线恒居中、不可定制。本轮补 **E1 单段 bar 可拖**
> （E2 障碍避让路由留待未来）。设计见 `plan.md` §16。

- **存储**：`ItemKind::Shape` 加 `elbow_mid_offset: f32`（`#[serde(default)]`=0，`.prz` 零迁移）。
  定义为**交叉轴偏移**：以两端点连线为纵轴、bar 沿横轴偏离中点的有符号距离——旋转时随端点
  局部轴系天然跟随；将来 E2 可作初值。偏移存原值不 clamp（用户意图），clamp 属几何层职责。
- **几何** `elbow_polyline_offset(pts, offset)`：转折点沿交叉轴平移；offset=0 退化为现行为
  （`elbow_polyline` 薄封装保留向后兼容）；`bar_axis` clamp 在两端点之间防回钩；`dedup()` 去零长段
  （防箭头方向 NaN）。三处消费点同源：core 命中 `contains_canvas_point`、binary 渲染
  `stylers::outline_points`、导出经 contains 间接共用。
- **命令** `SetElbowOffset`（Copy 快照命令）：单项 `new` / 批量 `new_batch` / `with_preview_applied`；
  undo/redo 写回原值，redo 不 clamp（命令只存用户意图原值）。
- **交互**：
  - `Handle::ElbowBar` 手柄：`elbow_bar_screen_segment` 取 bar 两端屏幕坐标；带状命中
    （`ELBOW_BAR_HIT_PX=8.0`，`max(stroke_width, 8px/zoom)`）；render 画 bar 中点小方块。
  - `DragState::ElbowBar { item_id, start_canvas, start_offset }`：begin_drag 读现 offset 进态；
    update_drag_preview 算 `delta_local = inv.transform_vector(current - start)`，按 `|dx|<=|dy|`
    取 `.x` 或 `.y`，`new_offset = start_offset + d` 直改 `item.kind.elbow_mid_offset`；
    end_drag 若有变化 push `SetElbowOffset::new(...).with_preview_applied(true)`，无变化不产生命令。
  - 编辑护栏升级：elbow 线从「不吐段中点手柄」改为「吐 bar 手柄」（拖 bar ≠ 加顶点，两点不变量不破）。
- **联动**：`resolve_bindings` 重算端点后用现 offset 重建走线——端点动、bar 相对位置保持
  （Excalidraw renormalization 的单段退化）。
- **i18n**：E1 无 flash 无面板输入，零新词条。
- **测试**：core 新增 4 项（偏移沿短轴平移 bar / clamp 不出回钩 / serde 往返+旧档默认 /
  offset 影响 contains_canvas_point）+ `SetElbowOffset` 批量 undo/redo 1 项；质量门全绿
  （fmt / `clippy -D warnings` / core 186 passed）。
- **E2（留待未来）**：障碍避让路由——移植 Excalidraw `elbowArrow.ts` 的 grid+A\*，待「连线穿过节点」
  痛点真实出现再评估立期。

---

## elbow 多顶点逐段展开（plan #19 E1.5，2026-09-24，待提交）

> 承接 E1：elbow bar 可拖交付后，对照 Excalidraw 发现其 line/arrow 类型分裂（`elbowed` 仅 arrow、
> UI 三态仅 arrow 可见、切 elbow 丢中间点）。**拍板不跟随**，保持 PReferZ 统一设计（全 Polyline +
> `CurveType` 三态），把多顶点折线切 elbow 的「静默无效」歧义升级为**多顶点逐段正交展开**。

- **几何** `elbow_multi_polyline(pts, closed)`：每段相邻顶点独立经 `elbow_polyline_offset` 居中
  正交化（offset=0），拼接成整体折线；段间共享顶点拼接时跳过下段首点去重；`closed` 时补末段回首点，
  **保留首尾重合**（闭环折线点列首尾相连，windows(2) 覆盖末段→首点，视觉闭环由显式末段保证）。
  退化（<2 点）原样返回；两点等价 `elbow_polyline`（居中）；近共轴段返回直线拼接天然正确。
- **分流**：`contains_canvas_point` 与 `stylers::outline_points` 的 Elbow 分支按点数分流——
  两点走 `elbow_polyline_offset`（带 offset，E1 行为不变）、多顶点走 `elbow_multi_polyline`。
- **护栏升级** `is_two_point_elbow_line`：段中点手柄（SegmentMid）对多顶点 elbow **恢复**（加顶点后
  与相邻顶点正交连接）、两点线仍压制（对角弦中点破坏两点不变量）；bar 手柄仅两点线出现
  （多段居中不消费 `elbow_mid_offset`）。
- **切换入口** `push_poly_curve`：切 elbow 时若 `closed=true` 打包 `SetClosed(false)`（`MultiCommand`
  一条 undo）——闭合折线转开放、顶点全保留、末段连回首点视觉闭环。
- **决策（2026-09-24，全按推荐）**：① 闭合→开放+末段回首点（视觉闭环）；② 多段居中（offset 仅两点线）；
  ③ SegmentMid 恢复（多顶点）、两点线压制。
- **测试**：core 新增 5 项（退化/两点等价/多顶点正交+顶点保留/闭合末段回首点/共轴段拼接）；
  binary `is_two_point_elbow_line` 测试更新（多顶点 elbow 不压制段中点）；质量门全绿
  （fmt / `clippy -D warnings` / core 191 passed）。
- **不做**：逐段独立偏移（拍板居中，将来 `#[serde(default)]` 升级兼容）；E2 避让路由仍留待未来。

## elbow 多顶点改「顶点锚定 bar」纯函数推导（plan #21，2026-09-24，待提交）

> 用户反馈 #19 逐段居中 Z 展开两大问题：顶点一多折线碎乱（3 顶点出 7 段）、段中点拖拽=插顶点
> 与两点线「拖 bar=平移走线」体验割裂。调研 Excalidraw `elbowArrow.ts`（fixedSegments +
> `handleSegmentRenormalization` 共线合并/短段折叠/索引重编号）后拍板**不引入 fixedSegments**——
> PReferZ 的 elbow 与 polyline 同类型可互转、顶点必须保留，改为「顶点锚定 bar」：顶点仍是唯一
> 用户数据，路径降级为纯函数 `f(points)`，零新增存储、零 `.prz` 迁移。

- **几何** `elbow_vertex_polyline(pts, closed)`（**替换** `elbow_multi_polyline`）：每个中间顶点
  锚定一根正交 bar——bar 过顶点本身、取向垂直于 (前邻, 后邻) 主导轴（`|dx|<=|dy|` → 竖 bar，
  与两点线短轴优先启发式一致）；跑段在相邻 bar 间垂直连接，端点处沿轴进入。3 顶点只出 3 段
  （V 形穿点 → 一座"桥"，顶点恰为 bar 中点）。拖顶点=平移整根 bar（单自由度），相邻坐标对齐时
  零长段被 dedup 吃掉、折线自动直化——**对齐合并免费获得**，无需 Excalidraw 的重编号簿记。
  `closed=true` 末段连回首点；两点开放链等价 `elbow_polyline`（E1 行为不变）。
- **分流**：`contains_canvas_point` 与 `stylers::outline_points` 同步换用新函数（两点仍走
  `elbow_polyline_offset` 带 offset）。
- **护栏** `is_elbow_line`：段中点手柄对**任意顶点数 elbow 一律压制**（拖拽加点手势废止；
  加顶点留待显式手势 DP-A，临时可切 polyline 编辑后切回）；bar 手柄仍仅两点线。
- **已接受特性**：①顶点严格处于 bar「中点」仅对称情形成立，一般情形顶点在 bar 上、bar 遍历
  范围由相邻几何决定；②bar 取向取决于邻居对主导轴，拖动越过对角阈值时取向 90° 翻转
  （滞回 DP-B 先不做，观察反馈）。
- **测试**：core 新增/替换 8 项（退化/两点等价+闭环/V 形桥/台阶/对齐自动直化/顶点保留+正交/
  闭合末段回首点/共轴段）；binary 手柄测试更新（多顶点 elbow 同样压制段中点）；质量门全绿
  （fmt / `clippy -D warnings` / core 194 passed；`tick_autosave_writes_sidecar_and_keeps_doc_dirty`
  经干净 HEAD 复现为先前已存在的无关失败）。
- **不做**：Excalidraw `fixedSegments` 模型与 A* 避让路由（E2 留待未来）；`elbow_mid_offset`
  推广到多顶点（多顶点用户意图由顶点位置表达）。

---

## elbow 多顶点双击插入顶点（plan #21 DP-A/DP-B 拍板，2026-09-24，待提交）

> 承接 #21 主体交付：DP-A 拍板「双击段插入顶点」、DP-B 拍板「取向翻转不加滞回」。

- **双击插入**：多顶点 elbow 的段中点手柄恢复，但**只认双击**（单击吞掉不动作）、位置改为
  **双击插入候选点**——core 新增 `elbow_insert_candidates(pts, closed)` 纯函数：候选点取该段在
  推导路径上覆盖的笔直小段（跑段 / bar 半段，含锚点把 bar 一分为二的段归属标注）中点，并过两条
  硬校验：①**视觉不变**——插入后 `elbow_vertex_polyline` 共线简化与原路径逐点一致（插入会改变
  相邻 bar 取向判定，中点未必安全；不安全时沿小段换采样 0.25/0.75，仍不安全则该小段无候选）；
  ②**可拖动**——新顶点小幅移动改变路径（滤掉 bar ⊥ 小段的惰性顶点）。已知覆盖缺口：端点侧
  小段主导轴取向不利时可能整段无候选（可接受退化）。
- **交互闭环（首版，后被「修正二」替换）**：插入后进入端点拖拽（复用 `LineEndpoint`），未拖动
  释放**保留**顶点
  （新增 `keep_inserted` 标志，区别于普通段中点加点「未拖动即移除」）；一条 `EditShapePoints`
  undo。双击判定在 press 沿自跟踪（`last_primary_press`，0.3s / 6px）——egui
  `button_double_clicked` 释放沿才置位，按下沿用不上；双击插入进行中短路「双击线/空白 →
  新建文本便签」与 `drag = Idle` 重置（防预览顶点成孤儿）。两点 elbow 不受影响（仍无段中点、
  仅 bar 手柄）。
- **验收反馈修正（2026-09-24，用户实测「双击没法创建新节点」）**：首版命中区仅为候选点上的
  6px 小方块，必须精确点中方块、且单击被整段吞掉 → 实际很难触发。改为 **整段推导路径带状命中**
  （core 候选带 `span` = 该段覆盖的完整折线，容差复用 bar 手柄 8px）：双击线上任意处即在该段
  通过校验的候选点插入，浅黄方块退化为「落点提示」；**单击不吞**（`continue` 穿透到常规
  选中/移动）；press 沿双击阈值放宽到 0.5s / 12px。补两项 binary 交互测试：双击候选带内 →
  加点 + 一条 undo + undo 还原、单击同点 → 不加点且进入移动拖拽。
- **验收反馈修正二（2026-09-24，用户追问「双击是不是被添加文字占用了」→ 命中）**：确实被占用，
  根因是**判定放错了事件沿**。egui 的 `double_clicked()` 在**释放沿**才成形；首版在按下沿自跟踪
  (`last_primary_press`，0.5s / 12px) 判双击，阈值比 egui 严 → 我的判定漏、egui 仍判为双击 →
  落进「双击线/空白 → 新建文本便签」分支。现改为释放沿双击分支**先**调
  `insert_elbow_vertex_at(pos)`（命中即消费本次双击、不再建文本），并删除按下沿全套机制
  （`last_primary_press` 字段、`begin_drag` 的 `double_click` 参数、`LineEndpoint.keep_inserted`
  及 end_drag 的未拖动移除豁免）。取舍：失去"加点即拖"合一手势（落点后由顶点手柄继续拖），
  换来单一可信双击信号 + 少一套自维护计时状态。命中改为不依赖选中态（取落点顶层 item），
  编组内的 elbow 线双击同样成立。
- **落点偏好（同轮改进）**：`elbow_insert_candidates(pts, closed, click)` 加可选落点提示（同一
  局部坐标系）——小段按离落点远近排序、优先试落点在该小段上的投影，投影不安全（改道 / 惰性）才
  退回中点 / 偏侧点，即"双击哪儿就长在哪儿"。手柄几何侧传 `None`（候选方块位置须与鼠标无关）。
- **测试**：core 新增 4 项（两点/退化空候选、V 形 on-path+live 复核、回钩形改道采样点淘汰、
  闭合三角形插入）+ 落点偏好 1 项；binary 交互测试 2 项（释放沿带内插入 + undo 还原、单击穿透到
  移动不加点；带外/两点 elbow/尖角折线不消费）；binary 手柄测试更新。质量门全绿（fmt /
  `clippy -D warnings` / core 199 + binary 78 passed；`tick_autosave_writes_sidecar_and_keeps_doc_dirty`
  为先前已存在的无关失败，干净 HEAD 复现）。

---

## 文字工具 + 双击职责归位（plan #22，2026-09-25，待提交）

> #21 做 elbow「双击段加点」时暴露：画布双击一直被「新建文本便签」占用（spec 早期定的
> P2-5 入口），两个手势抢同一信号。**拍板（用户提议）**：双击属于元素编辑，新建文本属于
> 工具——照 Excalidraw 那样给文本一个独立工具入口。

- **文字工具**：`Tool::Text` + `Action::ToolText`（默认绑 `Num8`）+ 工具栏「T」按钮（角标 8）
  + i18n 标签/动作描述（中英）。`begin_drag` 的 Text 分支 = 落点 `begin_free_text_at(canvas_pos)`
  起自由文本，并**立即回 Select**（一次性工具）；提交/取消沿用既有 Enter / Esc / 失焦路径
  （编辑期间 `begin_drag` 顶部的 `editing_text` 守卫挡住画布）。
- **删除双击建文本**：`ui()` 双击分支中「线/箭头/空白 → 新建 `EditingText`」的兜底臂改为空操作。
  保留的双击语义：Pixmap → 视口适配、Text / 封闭 Shape → 编辑其文本（编辑既有 ≠ 新建）、
  elbow 段 → 插入顶点（#21 DP-A，同分支优先消费）。
- **键位**：不照搬 Excalidraw 裸 `T`——该位是本项目 `ToggleToolbar`（Blender 同款，ADR-0007 保留）；
  续 1–7 数字工具行取 `Num8`，零冲突、设置面板可另绑。
- **测试**：binary `text_tool_click_starts_free_text_and_returns_to_select`（进编辑态 + 回 Select +
  自由文本无宿主/容器 + 不起拖拽）；`Action::ALL` 覆盖测试自动纳入 ToolText（绑位唯一性/标签
  完整性由既有测试把关）。质量门全绿（fmt / `clippy -D warnings` / core 199 + binary 79 passed；
  `tick_autosave_writes_sidecar_and_keeps_doc_dirty` 仍为先前已存在的无关失败）。

## RoughStyler 对齐 rough.js/Excalidraw（plan #20，2026-09-25，待提交）

> 2026-09-24 调研：RoughStyler ↔ rough.js 4.6.6 + Excalidraw `generateRoughOptions`
> 逐行对照，发现六类不对齐（抖动幅度基准差 1.5–10 倍为大头）。四决策点拍板：
> DP1 Architect 维持 0.5×（不改四档 UI）、DP2 solid 填充顶点跟随 rough.js 抖动、
> DP3 箭头跟随 Excalidraw 改抖、DP4 小图衰减与 amp_scale 相乘。

- **批次1 抖动幅度基准**：`sketch_edge` 的 `max_offset` 由 `min(边长×6%, 8px)·zoom·amp_scale`
  改为 rough.js `_line` 公式——`2 画布px × roughnessGain(边长) × amp_scale × zoom`
  （<200→1、200–500 线性 0.9→0.4、>500→0.4；边长 <20 画布px 短线衰减 `len/10`）；
  `curve_jitter_amp` 由"平均采样间距×6%"改为 rough.js `curve()` 的
  `(1+amp_scale×0.2)×amp_scale×zoom`（与曲线尺寸/采样密度解耦）；新增
  `small_size_roughness_scale` 移植 Excalidraw `adjustRoughness` 小图衰减
  （max<10→÷3、<20→÷2，三种例外不衰减），与 amp_scale **相乘**（DP4）。
- **批次2 bowing + 端点语义**：`mid_disp` 符号/幅度改 SeededRng 随机（rough.js 无饱和
  `/200` 线性，修"矩形四边一致外凸吹气感"）；新增 `preserve_vertices`（Excalidraw 同名
  选项）——Architect/Artist 档端点精确不抖、仅 Cartoonist 抖（修绑定/拼接接头脱开）。
- **批次3 hachure 四件套**：① `HACHURE_ANGLE_DEG` -41→-49（rough.js `hachureAngle+90`
  有效 49° 仰角；cross-hatch 第二组自动 +41）；② 填充线宽 = 描边一半
  （Excalidraw `fillWeight = strokeWidth/2`，Clean/Rough 一致）；③ 斜线段改走
  `sketch_edge` 完整双线抖动（rough.js `doubleLineOps`，取代旧端点 ±0.15·gap）；
  ④ 随机相位（roughness≥1 时 ~30% 概率跳首线，`scan-line-hachure.ts`）+ gap 下限
  4px → `round(max(gap, 0.1))`。
- **批次4 其余（已拍板项）**：solid 填充顶点抖动 ±2 画布px × amp_scale（DP2，
  rough.js `solidFillPolygon`）；箭头两翼改抖（DP3，`push_arrow_heads_rough`，
  roughness 封顶 min(1, roughness)，翼尖 preserveVertices 锚定，更正 stylers.rs 错误注释，
  ADR-0005 补录修订）；dash 模式 disableMultiStroke（单笔）+ strokeWidth+0.5；
  椭圆自适应采样数（rough.js `generateEllipseParams`，Ø100→12 段、Ø400→23 段）
  + 随机起始相位（`radOffset`，独立种子 `seed^0x5EED_11C5`）。
- **不做**（plan #20 明确划出）：zigzag/dots 填充（ADR-0005 注明后续自移植）、
  圆角矩形 `_bezierTo` 平滑抖动、椭圆 `overlap` 收笔重叠段。
- **测试**：stylers 36 项全绿——新增 roughnessGain 分段公式、adjustRoughness 矩阵、
  Cartoonist 端点抖动逐轴上界（100/300/1000px）、Architect/Artist 顶点精确、
  bowing 符号跨 seed 随机、跳首线=线表去首、49° 仰角方向、填充半宽；更新
  椭圆段数/箭头翼数/solid 填充抖动上界等既有断言。质量门：fmt / `clippy -D warnings`
  全绿；`cargo test --workspace` 除 `tick_autosave_writes_sidecar_and_keeps_doc_dirty`
  （经 git stash 验证为先前已存在的无关失败）全绿。待 `cargo run` 人工验收。

### 验收反馈批次（2026-09-25 同日，Feihei 实测 4 项）

1. **sloppiness 三档观感偏弱**：`amp_scale` 整体加倍——Architect 0.5→1.0、Artist 1.0→2.0、
   Cartoonist 1.8→3.6（曲线 / solid 填充 / hachure 抖动联动放大）；preserveVertices 规则不变
   （仅 Cartoonist 端点抖）。
2. **描边宽度档偏粗**：三处 stepper 档位 `[2,4,8,16,32]` → `[1,2,4,8,16]`（XS/S/M/L/XL），
   默认 4.0 由 S 变 M。
3. **圆角档位偏大**：`roundness_radius` 减半为 `short × roundness × 0.5`——roundness=1.0
   才是短边全圆弧，M 档 0.5 ≈ 1/4 短边（原 0.5 即满圆弧、上两档无意义）。
4. **圆角矩形手绘风碎短线**：`is_smooth` 把带圆角矩形并入平滑路线（整圈抖动 + Catmull-Rom，
   与椭圆同策；直边段共线插值后仍直），新增回归测试
   `rough_styler_rounded_rect_is_smooth_not_fragmented`。
5. **小矩形 + Cartoonist 圆角出乱线 / 小倒角角部打结**（同日二次+三次反馈，附图）：固定密度
   采样下相邻点距仅几像素，Cartoonist 曲线抖动 ±6px 令相邻点互越、被 Catmull-Rom 放大成
   自交小环；全局平均间距上限在「直边疏 + 圆弧密」轮廓上失守。修法：`jitter_points` 改
   **逐点局部上限**——每点幅度 = min(rough 幅度, 0.35 × 较短相邻段)（0.35 ≈ 0.5/√2，
   x/y 独立抽样下欧氏位移可达 amp×√2），相邻两点位移之和恒小于段长，结构上杜绝互越；
   新增测试 `smooth_jitter_respects_local_sample_spacing_at_corners`。

---

## elbow 锚点拖拽约束 + 插入点不落拐角（plan #21 三次验收反馈，2026-09-25，待提交）

> 用户实测两处：①直角模式的锚点可以被拖到走线之外；②双击新增的锚点是**角点**而不是正交段
> 中间的滑点。两者都出在「顶点锚定 bar」的第二个自由度上。

- **锚点离线**：顶点只有**垂直于 bar** 的分量被推导消费，沿 bar 轴的分量是空转——不约束时手柄
  会被拖着离开可见走线（看着像"锚点漂浮"）。core 新增 `clamp_elbow_vertex_drag(pts, closed,
  idx, probe)`：按推导同式算出该顶点 bar 的实际绘制区间 `[entry, exit]`，把沿轴分量钳进区间
  （=锚点只能沿自己的 bar 滑动、不许离线），交叉分量原样通过（平移 bar 的语义不变）。无 bar 的
  顶点（开放线首末点、闭合线接缝点 0）与两点线不约束。binary 在 `update_drag_preview` 的
  `LineEndpoint` 分支对 `CurveType::Elbow` 且 >2 点的应用之；吸附路径（端点绑定）不受影响。
  残留：仅被拖顶点保证在线上，拖邻居仍可能让别的锚点落到其 bar 之外（无约束求解器，纯函数固有）。
  首轮实现把闭合线的 `idx == n-1` 当端点漏约束，被性质测试抓到（闭合链里除接缝 0 外每个顶点都有 bar）。
- **插入点变角点**：上一轮加「双击哪儿就长在哪儿」的落点投影时，`t` clamp 到 `[0,1]`——点靠近
  拐角外侧时投影正好钉在小段端点（拐角）上，新锚点表现成角点。改为只允许小段**内部**
  `[0.15, 0.85]`（`elbow_insert_candidates`）。
- **测试**：core 新增 4 项——约束后的锚点恒在重推导路径上（∩ 形单例 + 三组几何 × 全部顶点 ×
  25 个探针方向的性质扫描，正是它抓出闭合线接缝漏约束）、候选点不等于任何拐角且在线上、
  落点偏好不回归；binary 新增预览拖拽约束测试（沿轴钳回 bar 端、垂直分量不动）。质量门全绿
  （fmt / `clippy -D warnings` / core 202 + binary 90 passed；
  `tick_autosave_writes_sidecar_and_keeps_doc_dirty` 仍为先前已存在的无关失败）。

---

## 两点 elbow 纳入双击插入（plan #21 四次验收反馈，2026-09-25，待提交）

> 用户实测：多段线 elbow 双击能加点，但**直线/箭头转成的 elbow**、**Ctrl+方向键建的流程图
> 连线**（都是两点 elbow）双击无作用。

- **重新推导推翻旧排除**：此前两点线被排除是担心插入会改道。实际上在 bar 线上插一点后，
  三点顶点模型（`elbow_vertex_polyline`）重推导的走线与两点展开（`elbow_polyline_offset`）
  **完全一致**——同一条 Z，视觉 no-op（顶点模型的 bar 取向启发式与两点展开分支同式）。
- **实现**：core `elbow_insert_candidates` 增加 `mid_offset` 参数并新增两点线分支——靶区 =
  展开路径的 bar 段（唯一与 bar 同向的段；退化直线 = 整条线），插入点取落点投影（钳小段内部
  防落拐角），no-op / live 校验同多顶点。binary `insert_elbow_vertex_at` 兼收两点线的
  `ElbowBar` 命中（bar 带与插入靶区同区：**单击拖 bar、双击插入**）；插入同时取消 press 沿
  误起的 bar 拖拽（偏移还原 `start_offset`、不产生 `SetElbowOffset`），候选点也按还原后的
  偏移计算——第二次按下期间指针漂几像素仍算双击，按预览偏移算会让落点带跳变。插入后
  `elbow_mid_offset` 不再被消费（bar 位置由顶点坐标接管，polyline ↔ elbow 往返仍无损）。
  闭合两点线折叠退化、不支持。
- **测试**：core 新增两点线专项（中点候选 / 腿上落点钳进 bar / 带偏移 no-op / 退化直线插入
  仍直 / 闭合两点线空候选）203 passed；binary 新增「双击 bar 带插入 + 取消 bar 拖拽 + undo
  还原」91 passed。质量门全绿（fmt / `clippy -D warnings`；
  `tick_autosave_writes_sidecar_and_keeps_doc_dirty` 仍为先前已存在的无关失败）。

---

## elbow 插入候选取向稳定性（plan #21 五次验收反馈，2026-09-25，待提交）

> 用户实测：双击能加点后，**插入的节点有时候会变成角点而不是线段锚点**。

- **根因在段级、不在采样点**：插入顶点的 bar 取向 =（前邻，后邻）主导轴，与采样位置无关；
  段两端角连线接近对角（`|dx|≈|dy|`）时，微小拖动任一邻居就翻转主导轴、bar 转成垂直于所在
  小段——锚点当场退化成角点。此前两条校验（视觉不变 / 可拖动）都只看插入瞬间，拦不住后续
  几何微动。
- **修复**：core 新增段级闸门 `bar_orientation_stable(prev, next, delta)`——两个邻居各沿 x/y
  扰动 ±5，主导轴取向必须全程不变；不稳定的段**整段无候选**（含两点线分支，只排除贴近对角
  约 ±10px 的窄带，不影响常规横平竖直的连线）。属既已接受的覆盖缺口族（取向不利的段无双击
  靶区）。
- **测试**：`elbow_insert_candidates_rejects_near_diagonal_segments`（余量 5 的 seg 被拒、
  余量 50 的保留、邻居拖 10px 后锚点仍直线穿越不出角；两点线精确对角空候选、余量 20 恢复）。
  core 207 passed；质量门全绿（无关失败 `tick_autosave_*` 同前）。

---

## elbow 倒角参数（plan #23，2026-09-25，待提交）

> 用户提议：elbow 也跟矩形一样有倒角参数（Excalidraw 的 roundness 同样作用于直角连线）。

- **复用 `Shape.roundness` 字段**（I4：`#[serde(default)]` 已有，`.prz` 零迁移）——同一
  None/S/M/L/XL 档位在矩形与 elbow 上观感一致。
- **core**：`roundness_radius` 从 binary stylers 上收 core（矩形/elbow 共用
  `min(w,h) × roundness × 0.5`）；新纯函数 `round_orthogonal_corners(pts, closed, radius)`——
  每个直角拐角按 8 段圆弧替换，半径逐角钳到相邻半段长（短段相接两角在段中点相汇后 dedup）、
  退化/非正交拐角原样通过、开放链首末点不倒角、闭合链环绕取邻；切点精确推入（三角函数在
  0/90° 的 ~1e-8 误差会让相汇点 dedup 失效）。
- **渲染**：`stylers::outline_points` 的 Elbow 分支在派生路径后按 roundness 倒角，两点 Z 与
  多顶点顶点模型都生效；RoughStyler 直接吃弧采样点列（与圆角矩形同路线，手绘圆角）。
- **命中同源**：`contains_canvas_point` 的 Elbow 分支同样先倒角再测距——弧内缩最深
  0.41×r，直角路径测距会漏点圆弧（"看着圆过的角一定点得中"）。
- **属性栏**：圆角节从「仅矩形族」扩展到 elbow 折线；`SetRoundness` undo 链路本就通用，零改动。
- **边界**：手柄方块 / 双击候选点 / 拖拽钳制仍在直角几何上（倒角是纯视觉后处理，不改 points
  数据，圆角处手柄与渲染线有 ≤0.41×r 视觉偏差，Excalidraw 同性质）；范围仅 `CurveType::Elbow`。
- **测试**：core `roundness_radius_matches_rect_semantics` /
  `round_orthogonal_corners_l_shape_arc_and_clamp`（L 形弧心切点 + 短段钳制 + 闭合正方形 36 点）/
  `elbow_roundness_affects_contains_canvas_point`（弧上点命中、直角路径同点不命中）。

---

## elbow 倒角末段修复 + 辅助点移除/离角闸门（plan #21/#23 六次验收反馈，2026-09-25，待提交）

> 用户实测两项：①elbow 加圆角后**终点前的一段直段消失**；②elbow 选中态仍出现"像多段线
> 那样的线段中点辅助点"（误以为可拖动加点、与锚点混淆），且新顶点有时仍长在角点上。

- **①末段丢失根因**：`round_orthogonal_corners` 开放链只推了首点，末角圆弧切出后**忘了补推
  终点** `pts[n-1]`——末段直线整段没画（首个单测还把该错误行为断言进去了）。修复：开放链
  循环后补推终点；闭合链由调用方收口不受影响。断言改为 `out.last() == 终点` + 弧切出点
  独立校验。
- **②a 辅助点移除**：多顶点 elbow 选中态的候选小方块与多段线弦中点加点手柄同款式，被当成
  "可拖动加点"且与锚点混淆——**渲染移除**；双击带状命中不受影响（hit_test 的 SegmentMid
  分支保留）。
- **②b 离角距离闸门**：仍有"新顶点长在角点上"——短小段上 `[0.15,0.85]` 投影钳制的 15% 处
  只有几像素（0.15×段长）。新增采样闸门 `MIN_INSERT_CORNER_DIST`（8 局部px）：离**未简化**
  推导路径任一拐角不足 8px 的采样淘汰（换采样点/换小段）。测试
  `elbow_insert_candidates_keep_off_corners`（助手直测 + 四组几何 × 多落点的 ≥8px 属性断言）。
- core 208 passed；质量门全绿（无关失败 `tick_autosave_*` 同前）。

---

## elbow 倒角半径全局统一（plan #23 七次验收反馈，2026-09-25，待提交）

> 用户实测：圆角算法仍有问题——同一根 elbow 线上会出现**半径不一致**的圆角。

- **根因**：逐角独立钳制 `min(radius, 相邻两半段长)`——多顶点 elbow 派生路径常含短小段
  （短跑段 / bar 半段），把相邻拐角压成小半径，其余拐角保持请求半径，同一线上大小不一。
- **修复**：与矩形族同语义——**全部拐角同一个半径**。先按各段约束求全局 cap（段被几个
  可倒角拐角共用就除以几：两端共用 ≤ 半段长、单端 ≤ 全段长；退化段 <1px 不参与 cap），
  生成时统一使用；个别退化段的端点拐角由逐角 min 兜底。
- **测试**：`round_orthogonal_corners_radius_is_uniform`（100/30/100/70 路径 radius=20 →
  三角全 15，旧逐角算法会给出 15/15/20；30px 短段两弧在中点精确相汇）。core 209 passed；
  质量门全绿（无关失败 `tick_autosave_*` 同前）。

---

## 导出选区对话框（Ctrl+Shift+E，2026-09-28）

> 对齐 Excalidraw `Ctrl+Shift+E`：选区导出预览 + 透明背景 + PNG / SVG / 复制到剪贴板。

- **快捷键**：`Action::ExportSelection`（Ctrl+Shift+E，可改绑）；无选区只提示不弹窗；`Esc` / X 关闭。
- **对话框**（新 `export_dialog.rs`）：实时预览（临时视口复用 `draw_item_visual`，透明底显示
  棋盘格纹）+ 尺寸信息（导出分辨率 = 画布尺寸 ×2，Excalidraw @2x 同款）+ 透明背景开关（默认开）。
- **离屏栅格化管线**（新 `offscreen.rs`）：离屏 `egui::Context`（`begin_pass` 初始化字体；
  复用 `app_font_definitions` 与主窗口主题样式）+ 临时视口收集形状 → `ctx.tessellate` →
  软件三角光栅化（预乘 alpha over 合成、字体图集采样、近似 top-left 规则防共享边双重混合、
  逆预乘输出直通 RGBA）。文字字形 / 手绘风曲线 / 图表与屏上渲染同源，补上旧逐像素采样
  导出「形状不渲染」的质量短板。
- **SVG 导出**（新 `svg_export.rs`）：zoom=1 收集的形状逐个转 SVG 元素（手绘风抖动、箭头、
  贝塞尔曲线保真）；Pixmap 按原始字节 base64 内嵌（魔数嗅探 MIME，支持裁剪 / 灰度 / 透明度）；
  文字逐行 `<text>` 近似（字号取排版段字体定义，字体不嵌入）。
- **剪贴板**：后台线程光栅化完成后 UI 线程写 arboard（`BackgroundOps` 新增 `clipboard_rx` 通道）。
- **修复（同源）**：图形绑定文字不进 `scene.selection`（随容器联动），导出直接快照 selection
  会把文字漏掉——导出集合改为 `expanded_export_ids` 双向联动（形状→绑定文本、绑定文本→容器，
  同 delete/duplicate 语义），`Ctrl+Shift+E` 与右键菜单「导出选中 PNG/JPG」一并修复。
- 字体定义构建收口 `lib.rs::app_font_definitions`（main.rs 与离屏 Context 共用，应用内缓存
  避免重复解压）。
- 测试：栅格化（白块 / 背景模式 / 预乘 over 混合）、base64 / XML 转义、离屏管线端到端
  （virgin Context 建 Ui → PaintList → tessellate → 光栅化）、选区联动扩展回归；质量门全绿。

---

## 决策点归档（D1–D6 / I1–I4）

> 原列于 plan.md，G/I/H/K 交付后蒸馏归档于此，使 CHANGELOG 自包含、plan.md 仅保留前瞻内容。

### D 系列（2026-09-01 拍板，主题 / 多选 / 折线 / 填充 / 键位）

| # | 问题 | 选项 | 倾向 | 结论 |
|---|---|---|---|---|
| D1 | 主题三态 | 仅 Light/Dark vs 增 `Auto`（跟随系统） | 字段预留 `Auto`，第一版只做手动切换 | ✅ 按倾向 |
| D2 | 画布底色 | 跟主题走 vs 独立可设 | 跟主题走 | ✅ 按倾向 |
| D3 | 多选属性面板 | 显示交集 vs 禁用面板 | 显示交集可批量改（Excalidraw 同款） | ✅ 按倾向 |
| D4 | elbow 折线 | 本轮做 vs 排后 | 排后 | 最小版 2026-09-22 落地（`CurveType::Elbow`，见 §折线新增直角折线 elbow）；绕障碍的完整 elbow 路由仍排后 |
| D5 | hachure 填充 | 本轮做 vs 排后 | 排后，纯色先统一数据模型 | ✅ 排后 |
| D6 | keymap 设置面板去留 | 保留（已交付可用）vs Phase K 顺手移除入口 | 移除入口，`Action`/`Keymap` 派发架构保留 | ✅ 移除入口（已实施） |

### I 系列（2026-09-01 拍板，图形元素类型统一）

| # | 问题 | 结论 |
|---|---|---|
| I1 | 不规则多边形是否纳入 Phase I | ✅ 纳入：新增「多边形」工具（点击加点、双击/回车闭合），存为 closed Polyline |
| I2 | 曲线切换入口 | ✅ 样式面板「直线/曲线」分段控件（选中线性对象，走 undo），不新增工具按钮 |
| I3 | ArrowHeadStyle 范围 | ✅ 仅 Arrow + Dot |
| I4 | .prz 迁移策略 | ✅ 新字段 `#[serde(default)]`，`USER_VERSION` 不变 |
| — | 圆角范围 | 仅矩形族（按计划） |
| — | 排后项 | D4 elbow 折线 / D5 hachure 填充 不纳入 Phase I |
