# PReferZ Roadmap（plan.md）

> 本文档是 PReferZ 的**前瞻路线图**：顶部是已交付阶段一览（索引），主体是待验收项与下一步计划。
> 各阶段**完整交付清单**见 [`CHANGELOG.md`](CHANGELOG.md)；设计规格见 [`specs/`](specs/)，架构决策见 [`adr/`](adr/)。
>
> 工作流惯例：**规划文档先提交，实现拆独立 commit**（Conventional Commits）；质量门槛
> `cargo fmt --check` / `clippy -D warnings` / `cargo test --workspace` 全绿才算交付。

**整体目标**：BeeRef 的 Rust 精神继承者——启动更快、包更小的极简参考图聚合桌面应用（egui + eframe），交互向 Excalidraw 对齐。

---

## 状态图例

- ✅ 已完成
- 🔶 部分完成（代码已落地，剩人工验收）
- ⏳ 计划中（未开始）

> 条目勾选框语义：**`[x]` = 代码已落地并验收**，`[ ]` = 尚有未完部分。

---

## 已交付阶段一览

| 阶段 | 状态 | 一句话目标 | 交付清单 |
|---|---|---|---|
| Phase 1–4 | ✅ | MVP：无限画布 / 图片导入变换 / 布局整理 / .prz 存取 | [CHANGELOG.md](CHANGELOG.md) §Phase 1–4 |
| Phase A–E | ✅ | Shape 基础集 / 线性对象 / 文本入形 / Frame 编号 / Slide 演示 | [CHANGELOG.md](CHANGELOG.md) §Phase A–E |
| Phase F | ✅ | 手绘风描边 RoughStyler（rough.js 同款，确定性种子） | [CHANGELOG.md](CHANGELOG.md) §Phase F |
| Phase 6 收尾 | ✅ | 键鼠映射可配置（keymap + 派发层 + 设置面板）。2026-09-01 拍板：不再投入自定义，默认键位改对齐 Excalidraw（[ADR-0007](adr/0007-keymap-no-customization.md)） | [CHANGELOG.md](CHANGELOG.md) §Phase 6 |
| 体验修复批次 | ✅ | 文字无背景 / 手绘椭圆光滑曲线 / Enter 编辑文字 / 绑定文本不可独立选中 | [CHANGELOG.md](CHANGELOG.md) §修复批次 |
| Phase G | ✅ | 明暗两套样式主题（Light/Dark/Auto）+ D6 移除键鼠改绑设置入口（架构保留） | [CHANGELOG.md](CHANGELOG.md) §Phase G |

---

## 人工验收（GUI）

> 自动化测试全绿 ≠ 观感正确，需 Feihei 跑 `cargo run` 确认。

**2026-09-01 全部通过**：旋转图片裁剪框跟随旋转 / Phase F 手绘风七项 / 文本三项反馈
（`ed538cf`+`d6177b8`+`c05d1ae`）/ 绑定文本不可独立选中（`8eb996a`）/ 键鼠改绑 /
配置持久化 / Slide 演示（E4）/ **裁剪遮罩闪黑已根除**（`74a5869`，旋转图拖动裁剪框不再闪）。

新增：**手工验收反馈批次（2026-09-02，Feihei 实测 5 项）——✅ 全部交付**，见 [CHANGELOG §手工验收反馈批次](CHANGELOG.md)。待用户 `cargo run` 手工复验。

新增：**多元素对齐 / 分布（plan #6，2026-09-05 `cad908b`）** —— ✅ 2026-09-07 Feihei 复验通过：
右侧属性栏「对齐」节 + 分布 4 按钮正常，横向等距/等心首尾不动、中间均分，`Ctrl+Z` 一步撤回。

新增：**样式面板批次（#1/2/3，2026-09-10 `0aec50d`）** —— 🔶 代码交付，首轮人工验收反馈 4 项修复（2026-09-10）：伪手写自由文本漏加 origin 偏移（文字跑屏幕左上角）+ 抖动观感放大（幅度 10% 字号 / 字号与间距 ±8%）；`apply_dark_mode_filter` 矩阵行错位修正（dark 色板现为 Excalidraw 同款亮色五档，含 4 个回归测试）；曲线抖动去掉 ×0.5 衰减（Sloppiness 档位差异可感知）；选中图形时绑定文字纳入侧栏文字节（对齐/字体/字号/颜色入口）。待复验。

新增：**直线/箭头端点吸附图形边缘（plan #5，2026-09-07 `a803bbe`）** —— 🔶 代码交付，首验未触发；
根因为拖拽预览的吸附查询点误用另一端点位置，修复 `8d2465e`（查询点改被拖端点当前位置）。
**✅ 2026-09-08 Feihei 复验通过**（吸附 + 高亮 + 联动 + undo 均正常）。

新增：**元素编组 / 解组（plan #13，2026-09-08 `4033726`）** —— 待 `cargo run` 复验：
框选 ≥2 项按 `Ctrl+G` 编组（或右键菜单「编组」）→ 点击组内任一成员应选**整组**、
拖动整体移动、多选外框为组包围盒；`Ctrl+D` 复制整组后点击副本应选副本组（不是原组）；
删除组内一个成员其余应仍编组；`Ctrl+Shift+G` 解组、`Ctrl+Z` 一步撤回编组/解组；
保存重开后组关系保留（旧 `.prz` 文件应能正常打开）。

新增：**多边形节点增删 + 自动闭合（plan #4，2026-09-11 `43576da`）** —— 🔶 代码交付，待 `cargo run` 复验：
- **Alt+单击删顶点**：选中折线/多边形，光标停在**中间顶点**上 Alt+单击即删；Alt+按住**端点**不移动直接松开也删该端点；两点直线删端点、闭合三角形删任一顶点应**拒绝**并 flash 保底提示；`Ctrl+Z` 一步还原（含被解除的端点绑定）。
- **Alt+拖端点延伸**：Alt+按住端点拖出几像素后线条"延长"——原端点留在原位成为中间顶点、新顶点跟手；延伸出的新顶点可正常吸附图形边缘建立绑定；松开后 `Ctrl+Z` 一步撤回整个延伸。
- **自动闭合**：开放折线（≥3 顶点）把端点拖回**起点附近（屏幕 8px 内）**松开 → 端点吸附到起点并闭合成多边形（flash「已闭合成多边形」），侧栏填充/可绑文字随即生效；`Ctrl+Z` 一步还原开放态与端点位置；闭合优先于同次吸附绑定。
- **回归**：不带 Alt 的端点拖拽/段中点加点/端点吸附联动（plan #5/#14）行为不变。

---

## 下一步：Excalidraw 打磨批次（Phase L 候选）

> **状态**：G / I / H / K 已全部交付（见 [CHANGELOG](CHANGELOG.md) §Phase G/I/H/K + §手工验收反馈批次）；
> 决策点 D1–D6 / I1–I4 已归档（见 [CHANGELOG §决策点归档](CHANGELOG.md)）。
> 本批次为对齐 Excalidraw 的观感/交互打磨项。5/6/11/12/13 已交付；
> **2026-09-10 拍板：启动 1/2/3 样式面板批次**（实施顺序 #2 → #3 → #1），决策点见表格「关键依赖 / 决策点」列。后续依次：4 → 7 → 8/9 → 10。
> **2026-09-11 启动 #4**：经 .ref/excalidraw 调研更正原倾向（Alt 在现行版是加顶点手势、删除走选中+Del），拍板见细节 §4。

| # | 打磨项 | 一句话方案 | 关键依赖 / 决策点 |
|---|---|---|---|
| 1 | ✅ 绑定文字对齐 + 字号 + 字体切换（2026-09-10 代码交付，待人工验收） | Text 加 H/V 对齐枚举 + 字体族（黑体/手写），侧栏加对齐分段与字号滑块 | ✅ 已拍板（2026-09-10）：手写体=**伪手写渲染**（复用 RoughStyler 思路，不嵌 TTF）；字号沿用现有滑块范围；垂直对齐仅绑定文字，自由文字固定 top-left |
| 2 | ✅ 调色板对齐 + 填充半透明（2026-09-10 代码交付，待人工验收） | `FILL_PICKS` 改为与描边同 5 色（黑红绿蓝橙）；填充加不透明度滑块（RGBA alpha） | ✅ 已拍板（2026-09-10）：top picks 描边/填充共用同组 5 色；填充默认 alpha **50%**（选填充样式但未显式改 alpha 时） |
| 3 | ✅ stroke width / sloppiness / edges 倒角（2026-09-10 代码交付，待人工验收；edges 经调研已被现有 roundness + curve_type 覆盖，无需新 UI） | `rough: bool` → `Sloppiness{Architect,Artist,Cartoonist}` 三档；edges（sharp/round）对齐 Excalidraw | ✅ 已拍板（2026-09-10）：sloppiness 对齐 Excalidraw 3 档；edges **对齐 Excalidraw 语义**——矩形→倒角，多边形/折线→顶点处切换曲线平滑（spline 类，具体插值实现时定） |
| 4 | ✅ 多边形节点增删 + 首尾重合自动闭合（2026-09-11 `43576da` 代码交付，待人工验收） | 顶点删除（**Alt+单击顶点即删**）+ 端点追加（**Alt+拖真实端点**，越阈值在外侧追加顶点后拖新点）；端点拖拽释放首尾距 ≤ 屏 8px 自动 `closed` | ✅ 已拍板（2026-09-11，调研更正见细节 §4）：守卫=开放 ≥2 / 闭合 ≥3 顶点；删首尾顶点连该端解绑；命令层**复用 `EditShapePoints`**（加可选 closed 变更字段），不新增 DeleteVertex/AppendVertex |
| 5 | ✅ 直线/箭头端点吸附图形边缘（2026-09-07 `a803bbe` + `8d2465e` 修复） | 拖端点邻近 Shape 轮廓吸附 + 绑定模型（端点随形状移动） | 已拍板并交付（2026-09-08 复验通过）：阈值屏 10px；绑定=两端 `Option<ItemId>` 不存绝对坐标，`resolve_bindings` 动态重算；直线/箭头均可绑，与 #7 共用 |
| 6 | ✅ 多元素对齐、分布（2026-09-05 `cad908b`） | `arrange.rs` 增 `plan_align`（6 向）+ `plan_distribute`（等距/等心 × 横/纵）；属性栏「对齐」节 + 右键菜单 | 已拍板并交付：两种分布都做；参考系=选区包围盒；UI=属性栏+右键菜单 |
| 7 | Ctrl+箭头 添加连接符 + 下一元素 | 选中 Shape + Ctrl+方向 → 生成绑定 Arrow + 新 Shape（流程图） | 新元素类型/间距；复用 #5 绑定 |
| 8 | 两列数据粘贴成柱状/折线图 | 剪贴板 2 列 TSV/CSV → 生成 Chart item（柱状/折线） | 图表用新 ItemKind+ChartStyler vs Pixmap 位图；单/多系列 |
| 9 | mermaid 代码转图表 | mermaid 子集 → nodes+edges（复用 #5/#6/#7） | 解析器：受限自研 Rust（零依赖，倾向）vs WASM mermaid |
| 10 | 徒手绘制（7 快捷键）墨迹模仿 | 新增 `Tool::Freehand` + `Num7`；`ItemKind::Freedraw` 平滑墨迹 | 压感（egui 无，倾向恒定宽+抖动）；点抽稀（RDP） |
| 11 | ✅ 选中拖动修饰键 + Ctrl+D 原位复制 | Ctrl+拖动=复制并移动副本；Shift+拖动=水平/垂直约束（PowerPoint 风）；Ctrl+D=原位复制 | 已交付（2026-09-03）：按下即建副本；约束基准=画布轴（视口无旋转，与屏幕轴同向）；Ctrl+D 偏移 10px |
| 12 | ✅ 最大/最小缩放限制 | 默认 100%，最小 10%（0.1x），最大 1000%（10x）；`min_zoom`/`max_zoom` 改默认值 | 已交付（2026-09-03）：默认值 0.1/10.0；`.prz` 元数据越界时 clamp |
| 13 | ✅ 元素编组 / 解组（2026-09-08 `4033726`，待人工验收） | `Item.group_id: Option<Uuid>`（单组，持久化）；点击组成员全选、移动整体；`Ctrl+G` 编组 / `Ctrl+Shift+G` 解组 | 已拍板（G1–G4 全按倾向列）：单组；点击整组；删/拖出成员其余保持编组；组粒度复制/对齐/分布 |
| 14 | ✅ 移动整条线贴合图形时建立端点绑定（2026-09-10 `38de45b`/`aa928e1`，已验收 ✅） | **锚点绑定模型**：`Option<ItemId>` → `EndpointBinding{target, anchor}`（锚点=贴合点在目标局部系坐标）；`resolve_bindings` 按锚点重算，端点钉同一表面点（修复矩形移动时端点沿边滑动、直线被拉平）；旧存档纯 uuid serde untagged 自动迁移（回退最近轮廓点）。**整线绑定**：MoveItems 预览对组内每条 Polyline 两端做 snap 查询（排除移动组自身），命中→贴边+绑定直改，未命中→端点随线自由+解绑；释放用 `MultiCommand` 打包 MoveItems + EditShapePoints，一条 undo 记录。另修复 skip_first_redo 回归（绑定字段释放直改） | 已按倾向拍板：① 整线贴合默认绑定（Excalidraw=是）✅；② 预览吸附视觉与端点编辑模式一致（同阈值/同高亮）✅；③ 端点同时贴近多目标取最近（find_snap_target 语义）✅ |
| 15 | ✅ 视口缩放套件（View all / 缩放到选中 / 100%）（2026-09-10 本批，待人工验收） | Excalidraw 同款三键：Shift+1 = Zoom to fit（**已有** `FitToScreen`，本轮确认保留）；新增 Shift+2 = 缩放到选中（`zoom_to_selection`，选中 AABB 并集 `fit_to_content`，无选中仅 flash"未选中元素"）；新增 Shift+3 = 缩放回 100%（视口中心 pan 不动，仅改 zoom）。两 Action 均入 `ALL` 列表可重绑定；i18n 补 Action 名与 flash 词条 | 已按倾向拍板：① 快捷键对齐 Excalidraw Shift+1/2/3 ✅；② Shift+2 无选中时提示而非退化成 fit all（与 Excalidraw 一致不改变视口）✅ |

### 各项细节与决策点

1. **✅ 绑定文字对齐 + 字号 + 字体切换（2026-09-10 代码交付，待人工验收）**
- 现状：`ItemKind::Text`（TextStyle）已有 `font_size`/`color`/`background`，但**无对齐字段**；绑定文字（容器封闭 Shape）由渲染层自动居中，无 H/V 控制；字体仅内嵌 simhei（黑体）一种。
- 方案：core 加 `TextAlignH{Left,Center,Right}` + `TextAlignV{Top,Middle,Bottom}`（`#[serde(default)]=Center`）；字体族 `FontFamily{Handwriting,Normal}`——手写体需新增第二种嵌入 TTF（体积评估，见 `.issues`）或伪手写渲染（复用 RoughStyler 思路）。侧栏 Text 节加对齐分段控件 + 字号滑块（已有）+ 字体族切换。
- 决策点已拍板（2026-09-10）：手写体用**伪手写渲染**（复用 RoughStyler 思路，不嵌 TTF，零体积增量）；字号沿用现有滑块范围；垂直对齐仅绑定文字，自由文字固定 top-left。
- 交付（2026-09-10）：`TextStyle` 扩 6 字段（+align_h/align_v/font_family，`#[serde(default)]` 向后兼容）；`layout_handwritten` 逐字符排版 + SeededRng 确定性抖动（字号 ±4%、幅度 0.045×font_px，兼容换行）；`draw_text_item` 按对齐枚举定位（默认即历史居中行为）；侧栏 Text 节新增字体族两档分段（所有文字）+ H/V 对齐分段（仅绑定文字）；全部走 `SetTextStyle` 整份快照入 undo。

2. **✅ 调色板对齐 + 填充半透明（2026-09-10 代码交付，待人工验收）**
- 现状：`STROKE_PICKS` 已是 `["#1e1e1e","#e03131","#2f9e44","#1971c2","#f08c00"]`（黑红绿蓝橙，恰好对齐 Excalidraw）；`FILL_PICKS` 是 4 个粉彩色，**与 5 标准色不对齐**。
- 方案：`FILL_PICKS` 改为与 `STROKE_PICKS` 同 5 色；填充增**不透明度滑块**（fill 已是 RGBA，暴露 alpha 0–100%，默认取 Excalidraw 观感值）。侧栏填充节走 `apply_continuous` + `FillState`。
- 决策点已拍板（2026-09-10）：top picks 描边/填充共用同组 5 色（黑红绿蓝橙）；填充默认 alpha 50%（用户选填充样式但未显式调 alpha 时）。
- 交付（2026-09-10）：`FILL_PICKS = STROKE_PICKS`；新建形状/属性栏 None→Some 填色套 `FILL_DEFAULT_ALPHA`(128)；侧栏填充色后新增 0–100% 不透明度滑块（`apply_continuous` 仅改 alpha 通道）。

3. **✅ stroke width / sloppiness / edges 倒角（2026-09-10 代码交付，待人工验收）**
- 现状：`StrokeStyle.width` 已有（0.5–12 连续）；`rough: bool` 仅开/关；矩形 `roundness` 已有（仅矩形族）。**缺 sloppiness 多档**与通用 edges 倒角。
- 方案：`rough: bool` → `Sloppiness{Architect,Artist,Cartoonist}`（3 档映射到 RoughStyler 抖动幅度；`#[serde(default)]`=Architect 即原 false 语义），`SetRough` 改 `SetSloppiness`；edges（sharp/round）分段控件，仅矩形显示。
- 决策点已拍板（2026-09-10）：sloppiness 对齐 Excalidraw 3 档；edges 对齐 Excalidraw 语义——矩形→倒角，多边形/折线→顶点处切换曲线平滑（spline 类，具体插值实现时定，倾向 Catmull-Rom 与现有手绘曲线一致）。
- 交付（2026-09-10）：`Sloppiness` 四档（Off/Architect/Artist/Cartoonist，amp_scale 0/0.5/1/1.8）+ `deserialize_sloppiness` 兼容旧 bool（true→Artist）；侧栏 checkbox 改四档分段；`SetRough`→`SetSloppiness` 批量命令。**edges 经调研无需新 UI**：矩形倒角=已有 roundness 滑块，多边形曲线切换=已有 curve_type Straight/Curved（Catmull-Rom）。

4. **多边形节点增删 + 首尾重合自动闭合**
- 现状：反馈 #5 已做段中点拖拽**插入**顶点（SegmentMid）；但删顶点、端点追加、首尾重合自动闭合缺失。
- **调研更正（2026-09-11，.ref/excalidraw）**：plan 原倾向「删除=Alt+拖出」与现行 Excalidraw 不符——现行版 **Alt 是加顶点手势**（`linearElementEditor.ts:1216-1273` Alt+拖拽在末点追加"未提交"顶点、`1072-1112` Alt+click 落点并拖走），**删顶点走"点选 + Delete 键"**（`actionDeleteSelected.tsx:215-261`，Alt+click 删除是旧版手势）；**自动闭合** = 拖首/尾顶点释放时首尾距 ≤ `LINE_CONFIRM_THRESHOLD`(8px)/zoom 且 ≥3 顶点 → 置 polygon 并把被拖点吸附到对端（`linearElementEditor.ts:714-751`、`utils.ts:477-491`、`constants.ts:21`）。本仓库无"顶点选中态"（手柄只有 hover/drag），忠实照搬"点选+Del"需新增 selectedPointsIndices 状态机，弃用。
- **决策点已拍板（2026-09-11）**：
  - **删除手势**：✅ **Alt+单击顶点即删**（内部顶点按下即删；端点=延后到释放且未越追加阈值时删，与 Alt+拖追加共用按下入口、按移动区分）。守卫：开放折线保 ≥2、闭合多边形保 ≥3 顶点，不满足拒绝并 flash。删首/尾顶点连带解除该端绑定（中间顶点无绑定不动）。
  - **追加分支**：✅ **Alt+拖真实端点（0/末点）=延伸**：越过屏幕 4px 阈值后在该端外侧插入一个复制顶点（原端点成为中间顶点），被拖的是新顶点；追加一个点，不做 Excalidraw 的 Alt+拖连续链式加点。未越阈值释放 = Alt+单击 → 删所点顶点。
  - **自动闭合**：✅ 任何端点顶点拖拽**释放**时，若当前开放、≥3 顶点且首尾距 ≤ 屏幕 8px（画布距 × zoom；复用 `POLYLINE_CLOSE_DISTANCE`，与 Excalidraw 同值）→ `closed=true` + 被拖端点吸附至对端 + 该端绑定解除，与点变更同占一条 undo。侧栏手动 closed 勾选保留。
  - **命令层**：实现更正——不新增 `DeleteVertex`/`AppendVertex`，**复用 `EditShapePoints`**（本质=点集替换，绑定变更已内建），加可选 closed 变更字段承载"拖回起点即闭合"。
- 交付（2026-09-11 `43576da`）：core `EditShapePoints` 增 `with_closed`（点集+closed 同条 undo，redo/undo 一并写回）；app `try_delete_vertex`（守卫 + 删端点解绑 + 预览直改）、`begin_drag` 接 Alt（内部顶点按下即删 / 端点进 alt_extend 拖拽）、`update_drag_preview` 延迟延伸插入（屏幕 4px 触发；**未触发前不动原端点**，防单击抖动误提交微移动）、`end_drag` 未越阈释放=删顶点 + 提交时自动闭合（吸附对端、该端解绑、闭合优先于吸附）；i18n 4 词条；测试：`should_auto_close` 判定矩阵、删除守卫/解绑还原、`with_closed` 往返。

5. **✅ 直线/箭头端点吸附图形边缘（2026-09-07 交付：`a803bbe`，待人工验收）**
- 现状：LineEndpoint 拖拽无吸附、未绑定形状。
- 方案：拖端点邻近 Shape 轮廓（点到轮廓距离 < 阈值）吸附到最近点，core 记 `binding`；形状移动时联动更新绑定端点（Scene 遍历）。Excalidraw 语义：箭头绑 shape，移动 shape 箭头跟随。
- **决策点已拍板（2026-09-06）**：
  - **吸附阈值**：屏幕 **10px**（画布阈值 = `10 / zoom`，随缩放保持手感一致）；落在阈值内吸附到形状轮廓最近点，超出则拖离并解除该端点绑定。
  - **绑定存储**：**线性对象两端点各一字段** `start_binding: Option<ItemId>` / `end_binding: Option<ItemId>`（存于 `ItemKind::Shape`，`#[serde(default)]` 自动落盘）。吸附点**不存绝对坐标**，每次形状移动时由 `Scene::resolve_bindings` 按「另一端点的方向」动态重算轮廓最近点——比 plan 原写的 `(ItemId, side)` 更贴近 Excalidraw 动态绑定语义，且天然兼容多段线（仅 0 / 末端点可绑）。
  - **直线/箭头均可绑**：是。绑定模型与 #7 连接符共用。
  - **联动触发**：`resolve_bindings(moved_ids)` 注入 `TransformItem`/`MoveItems`/`ScaleItems`/`RotateItems`/`FlipItems`/`ArrangeItems` 的 redo/undo，且拖拽预览阶段（MoveItems、HandleTransform 分支）实时调用，保证端点跟随形状。绑定目标被删除时自动清绑。

6. **✅ 多元素对齐、分布（2026-09-05 交付：`cad908b`，待人工验收）**
- 现状：`arrange.rs` 只有 `ArrangeMode::{Linear,Optimal,Grid}`（装箱），**无 align/distribute**。
  排列入口在右键菜单（选中 ≥2 项时的「排列」子菜单），App 无顶部工具条。
- 方案：core 增 `plan_align(scene, ids, AlignMode)`（6 向）+ `plan_distribute(scene, ids, axis, mode)`，
  均返回 `(ItemId, old_pos, new_pos)`，由 UI 层包成现有 `ArrangeItems` 命令入 undo 栈（不新增命令类型）。
- **决策点已拍板（2026-09-05）**：
  - **分布基准**：**两种都做** —— `Gap`（相邻边界空隙相等，Excalidraw / PowerPoint 默认）
    与 `Centers`（相邻中心距相等）；UI 各给一个按钮，共 4 个（横/纵 × 等距/等心）。
  - **参考系**：**选区包围盒**（选中项 AABB 的并集）。对齐时元素贴向该框的对应边/中线；
    分布时首尾元素保持不动，中间均分（Gap：首尾外边界之间；Centers：首尾中心之间）。
  - **UI 位置**：**右侧属性栏新增「对齐」节**（选中 ≥2 项时显示在属性栏顶部，
    6 个对齐图标按钮 + 4 个分布按钮），**右键菜单的「排列」子菜单内追加一份**（对齐 6 项 + 分布 4 项）。
    不做画布浮动工具条。
- 边界约定：参与计算前过滤 `Scene::is_bound_text` 的绑定文本（其位置由容器决定，
  独立平移会与容器错位）；对齐要求 ≥2 项、分布要求 ≥3 项，不足时不做无意义命令。

7. **Ctrl+箭头 添加连接符 + 下一元素（流程图）**
- 现状：无。
- 方案：选中单一 Shape 时 `Ctrl+方向`沿该向生成绑定 Arrow + 新 Shape（默认矩形，带绑定文本占位）；新 Shape 位 = 源边界 + 间距。复用 #5 绑定模型。新增 `Action::AddConnectedShape` + 方向键。
- 决策点：新元素类型（默认矩形 vs 当前工具默认）；间距；是否自动命名。

8. **两列数据粘贴成柱状/折线图**
- 现状：粘贴仅图片（Ctrl+V 释放沿）；无数据→图表。
- 方案：检测剪贴板文本为 2 列（TSV/CSV，≥2 行）弹「柱状/折线」选择，生成 `ItemKind::Chart`（新增）+ 独立 `ChartStyler` 离屏绘制（egui painter，零依赖）。先单系列，多系列排后。
- 决策点：新 ItemKind+渲染器 vs 生成 Pixmap 位图（编辑性 vs 简单）；坐标轴/图例范围；粘贴触发 vs 工具栏。

9. **mermaid 代码转图表**
- 现状：无。
- 方案：文本框/菜单输入 mermaid → 解析为 nodes（Shape）+ edges（Arrow，含 #5 绑定）。解析器选型：
  - (a) WASM mermaid（重，违零依赖/小包目标）；
  - (b) 受限自研 Rust 解析器（支持 `graph TD`/`flowchart` 的 node/edge/label 子集，零依赖，可控但覆盖有限）——**倾向 (b)**；
  - 复用 #5/#6/#7 生成结果。
- 决策点：解析器选 (b) 受限自研 vs 接受 WASM；先支持 flowchart 子集。

10. **徒手绘制（7 快捷键）墨迹模仿**
- 现状：`Tool` 无 Freehand；Phase K 注释裸 P 让开给 freedraw、数字 7 空闲。
- 方案：新增 `Tool::Freehand` + `Action::ToolFreehand` 绑 `Num7`（Excalidraw freedraw=7）；core 新增 `ItemKind::Freedraw`（点序列 + 宽度）；渲染为平滑墨迹（Catmull-Rom 穿采样点，手绘抖动可选）；按下采集、移动追加、松开定型走 undo；点序列 RDP 抽稀。
- 决策点：压感（egui 无输入，倾向恒定宽 + 末端收笔）；点采样密度/抽稀。

11. **选中拖动修饰键 + Ctrl+D 原位复制（PowerPoint 风）** — ✅ 已交付，见 [CHANGELOG §拖拽修饰键 + Ctrl+D 原位复制](CHANGELOG.md)。

12. **最大/最小缩放限制** — ✅ 已交付，见 [CHANGELOG §缩放范围限制](CHANGELOG.md)。

13. **✅ 元素编组 / 解组（Group / Ungroup）（2026-09-08 交付：`4033726`，待人工验收）**
- 现状：`Scene.items` 为扁平列表；`selection` 为 `ItemId` 集合；**无 group 概念**（grep `group/parent/children` 仅命中无关项）。点击命中单个元素即选单个；移动走 `DragState::MoveItems` 对 `selection` 整集平移。
- 方案：
  - core 加 `Item.group_id: Option<Uuid>`（`#[serde(default)]=None`，向后兼容旧 .prz）；`Scene` 增 `group(&[ItemId])`（给选中项赋同一新 `Uuid`）、`ungroup(&[ItemId])`（清空）、`group_members_of(item_id)`（同组其余项）。
  - 点击命中：若命中项 `group_id.is_some()`，选区扩展为**整组**（含命中项），使 `MoveItems` 自然整体平移；`transform_handles` 仅渲染组包围盒（不逐成员画手柄，避免杂乱）。
  - 动作：`Action::Group` 绑 `Ctrl+G`、`Action::Ungroup` 绑 `Ctrl+Shift+G`（Excalidraw 同款）；经新命令 `SetGroup(ids, Option<Uuid>, Option<Uuid>)` 入 undo 栈。
  - 序列化：`.prz` items 多一列 `group_id`（TEXT/UUID 字符串，可空）；`BeeFile` 写/读补该列；旧文件缺该列按 `None` 加载。
- **决策点已拍板（2026-09-08，全按倾向列）**：
  - **G1 单组 vs 多组嵌套**：✅ **单组**（一个元素至多一个 group_id）；编组前已是别组成员的项自动离开旧组（赋新组 id 即完成"换组"，无需先手动解组）。
  - **G2 点击整组 vs 点单成员**：✅ **点击组内任一成员即选整组**（Excalidraw 默认）；框选按实际命中成员收进选区（框选部分成员时选中那些成员，操作时命中扩展仍生效）；双击穿透选单成员排后增强。
  - **G3 组内元素的删除/拖出**：✅ 删/拖出某成员，**保留其余成员的 group_id**（同组继续存在）；彻底解组需显式 `Ctrl+Shift+G`。
  - **G4 与 11/6 的关系**：✅ 编组后 `Ctrl+D`、对齐/分布均作用于整组（选区=整组），无特殊处理；组整体移动走现有 `MoveItems`；组包围盒手柄：选中**整组**（且选区恰为完整一组）时只画组包围盒，其余多选场景维持现状。

---

## 功能性待办（来自 .issues，尚未实施）

> issues.md 中 **#3 / #4 / #5** 尚未修复（#1/#2 已交付），现纳入本 plan 作为下一轮工作项，从此以 plan.md 为唯一工作视图。
> 验收后从 issues.md 勾掉对应项并补 [CHANGELOG.md](CHANGELOG.md)。

| # | 项 | 一句话方案 | 关键决策点 |
|---|---|---|---|
| 3 | Frame 常用演示比例预设（16:9 / A4 / 3:2 / 4:3） | 创建/选中 Frame 时提供常见比例与纸张下拉，按单位/DPI 换算像素尺寸 | 预设清单取舍；A4 等物理尺寸是否绑定 DPI；自定义比例输入方式 |
| 4 | 显示所有元素（Show All / Zoom to Fit） | 一键适配全部可见 item 包围盒（复用 `compute_fit`），按钮 + 快捷键 | 是否计入隐藏/锁定元素；快捷键对齐 Excalidraw；空场景回退 |
| 5 | 自动保存（Autosave） | 定时/debounce 自动写 `.prz`（原文件或 `.autosave`），启动提示恢复 | 覆盖原文件 vs 独立 autosave；恢复 UX；间隔默认；不污染 undo |

### 细节

3. **Frame 常用演示比例预设**
- 现状：创建 Frame 时尺寸自由，无预设比例/纸张快捷选择。
- 方案：Frame 创建/选中时提供常见演示比例与纸张预设（16:9、4:3、3:2、A4 等），选定后按当前单位/DPI 换算像素尺寸；可存默认偏好。
- 决策点：预设清单取舍；A4 等物理尺寸是否绑定 DPI 假设；自定义比例如何输入。

4. **显示所有元素（Zoom to Fit All）**
- 现状：无全局适配入口；当前双击图片仅适配单张（#2），无"显示全部"。
- 方案：新增"显示所有/适配全部"动作（按钮 + 快捷键，倾向对齐 Excalidraw `Ctrl+0` 或 `Shift+1`），计算所有可见 item 包围盒，复用 `compute_fit` 适配视口；与 #2 共用逻辑。
- 决策点：是否计入隐藏/锁定元素；快捷键选型；空场景处理（回退默认视口）。

5. **自动保存（Autosave）**
- 现状：仅手动 `Ctrl+S` 写 `.prz`，长时间编辑无自动落盘，崩溃丢工作。
- 方案：定时（如每 60s）或变更后 debounce（如 30s）自动写入 `.prz`（同路径或 `.prz.autosave` 临时文件）；启动检测 autosave 文件并提示恢复；设置面板可开关 + 调间隔。
- 决策点：自动保存目标（覆盖原文件 vs 独立 autosave 文件）；恢复提示 UX；间隔默认值；与 Command/Undo 栈解耦（autosave 不进 undo 历史）。

---

## 远期 / 未决

> 详细记录在本地 `.issues/issues.md`（gitignore 排除、不入库）。

- [ ] ttf 字体体积优化（assets/simhei.ttf 全量嵌入）
- [ ] 放弃 `.bee` 兼容后可做的架构简化盘点
- [ ] 视口裁剪已有，LOD 缩略图（Phase 5+）待评估
