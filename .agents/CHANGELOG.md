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

## 决策点归档（D1–D6 / I1–I4）

> 原列于 plan.md，G/I/H/K 交付后蒸馏归档于此，使 CHANGELOG 自包含、plan.md 仅保留前瞻内容。

### D 系列（2026-09-01 拍板，主题 / 多选 / 折线 / 填充 / 键位）

| # | 问题 | 选项 | 倾向 | 结论 |
|---|---|---|---|---|
| D1 | 主题三态 | 仅 Light/Dark vs 增 `Auto`（跟随系统） | 字段预留 `Auto`，第一版只做手动切换 | ✅ 按倾向 |
| D2 | 画布底色 | 跟主题走 vs 独立可设 | 跟主题走 | ✅ 按倾向 |
| D3 | 多选属性面板 | 显示交集 vs 禁用面板 | 显示交集可批量改（Excalidraw 同款） | ✅ 按倾向 |
| D4 | elbow 折线 | 本轮做 vs 排后 | 排后 | ✅ 排后 |
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
