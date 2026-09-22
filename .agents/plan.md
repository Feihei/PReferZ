# PReferZ Roadmap（plan.md）

> 本文档是 PReferZ 的**前瞻路线图**：主体是待验收项与下一步计划；**已交付内容一律见**
> [`CHANGELOG.md`](CHANGELOG.md)（本文件不重复维护阶段索引）；设计规格见 [`specs/`](specs/)，架构决策见 [`adr/`](adr/)。
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

> 全部已交付阶段与批次的完整清单统一见 [`CHANGELOG.md`](CHANGELOG.md)——本文件不再重复维护阶段索引表（避免双处更新失真）。

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

新增：**流程图 Ctrl+方向创建 / Alt+方向导航（plan #7，2026-09-11 `56dd3d4`）** —— 🔶 代码交付，待 `cargo run` 复验：
- **创建**：单选一个矩形/椭圆/菱形，按 `Ctrl+→`——右侧 100px 处出现**同类型同大小同配色**的克隆节点，两者之间一条**两端带箭头连接的直线箭头**（端头朝新节点）；选区自动跳新节点，连按 `Ctrl+→` 应横向接龙；`↑↓←` 同理四向。多选、选中直线/箭头/图片/文字时按 Ctrl+方向应**无动作**。
- **联动**：移动任一节点，箭头端点应钉在边中点跟随（#5/#14 绑定模型）；`Ctrl+Z` 一步撤回整对（节点+箭头一起消失），`Ctrl+Y` 恢复。
- **导航**：`Alt+→` 把选区跳到右方直接相连的邻居节点（经箭头绑定），`Alt+←` 跳回；无该方向邻居时选区保持不动；导航不进 undo。
- **设置面板**：新增「流程图：添加连接图形 / 沿连接导航」两个可改绑动作，默认 Ctrl+方向 / Alt+方向；改绑后方向按实际命中的方向键解释（绑到非方向键则不响应）。
- **回归**：Present 模式下裸方向键翻页不受影响（修饰键严格匹配）。

新增：**验收反馈批次（2026-09-14，Feihei 实测 3 项，`8ba4ac5`）—— 🔶 代码交付，待 `cargo run` 复验**：
1. **#4 更正：闭合=合并视作一个点**（原实现保留两个重叠点，圆滑（spline）模式接缝处出现反向小环）。
   现渲染/采样层在 `closed` 且首尾**精确重合**时视作单点（数据结构不变，仍留两端点）。复验：
   开放曲线端点吸回起点闭合后，**曲线风格=圆滑**下接缝应平滑无回钩；填充、绑文字、端点联动照常。
   且把重合端点中**任一个**拖开（屏幕 >8px）松开 → 应**自动恢复开放**（flash「已恢复开放」），
   无需去侧栏勾掉闭合；`Ctrl+Z` 一步还原闭合态。拖普通（非重合）闭合多边形的顶点仍只移点、**不**解闭合。
2. **#7 更正：同向已有邻居时分叉避让（2026-09-22 二次更正，取代原"邻居旁外推"）**。选中 A 已连
   B（B 在 A 右），再按 `Ctrl+→` → 新节点 C 主轴恒相对 A（与 B **同列 x**），交叉轴在该列带内滑到
   离 A 中心**最近空位**、错到 B 的**上方/下方**成上下**分叉**（原"放到 B 右侧 100px 外推"在反复
   同向创建时新节点全叠进 B 的后续格子→重叠，故弃用）；箭头仍 **A→C**。落位收进 core
   `flowchart::place_node`（移植 Excalidraw `placeCluster`，含 `mergeIntervals`/`findNearestFreeSlot`）。
   无邻居时行为不变（源旁 100px、交叉轴居中）。`Alt+→/←` 导航仍共用 `find_connected_neighbor`，不受影响；
   新增障碍收集 `connected_flowchart_rects`（同连通子图 BFS）。
3. **新增：左下 HUD 主题切换按钮**（语言按钮右侧，20px 图标，手绘形状——字体无月牙字形）。
   light 下显黑色月牙、dark 下显白色太阳（☀），hover 有底色高亮；Auto 主题按**当前生效外观**显示，
   点击转为显式 Light/Dark 并即时翻色（默认描边色随主题、写 config 持久化，与设置面板一致）。

新增：**验收反馈批次（2026-09-16，Feihei 实测 2 项）—— 🔶 代码交付，待 `cargo run` 复验**：

1. **绿色 flash toast 有时塌成一竖条**（每行一个字），有时正常单行。根因：flash 用固定 Id 的
   `egui::Area`，而 Area 会把内容尺寸记忆在 `AreaState` 并在下一帧当作 `max_rect`
   （egui `Area::end()` 里 `state.size = content_ui.min_size()`）；中文文案没有 ASCII 空格，
   默认 `TextWrapMode::Words` 把整句当一个超长"单词"按字符硬拆，于是可用宽度逐帧收缩，
   只要画布还在重绘就收敛成正一列；无重绘（只画了一两帧）时看起来正常——这解释了"有时"。
   修法：显式把排版上限钉死为「屏宽 − 两侧留白」（新常量 `FLASH_TOAST_SIDE_MARGIN = 24`，
   `ui.set_max_width`）——可用宽度不再来自记忆尺寸，收缩链被切断：短文案恒单行，
   超长文案（带路径的错误）稳定折行且留在屏内（比 `Extend` 更好，`Extend` 会让它横向
   溢出被两侧裁掉）；后台进度条浮层同处理。另给 Area Id 加 flash 序号 `flash_seq`，
   让每条新提示重走一次 sizing pass（居中位置不再慢一帧）。
   复验：连续做移动 / 缩放 / 删除 / 对齐 / 排列 / 保存等会出提示的操作，toast 应恒为
   **水平居中的一整块**——常规短文案单行贴在画布底部；超长文案（带路径的「已保存」）
   在屏宽内折成两三行、左右各留 24px，既不塌成竖条也不溢出屏幕两侧。
2. **切到 EN 界面后弹的信息仍是中文**。根因：flash 与后台进度条的一批文案直接写死在
   `preferz_app.rs`（`已保存: …` / `已删除 3 项` / `水平翻转` / `排列：线形` / `导入图片: 路径` 等），
   未经过 `t(self.lang, …)`。修法：新增 29 个词条（文件操作、翻转/变换/移动/删除、创建图形/
   直线/箭头/画框/改号、导出无图两类、排列/对齐/分布/归一化模板与排列模式短名、进度条三条），
   全部改走 `t` / `fill`；`BackgroundOps::start_import` / `start_load` / `start_save` 增 `lang`
   参数以本地化进度消息；对齐/分布原先用全角冒号硬拼（英文会得 `Align：Align left`），改为整句模板。
   复验：HUD 左下角切到 EN 后，删除 / 保存 / 导入 / 导出 / 排列 / 对齐 / 翻转 / 拖动缩放
   的 toast 与后台进度条文字全为英文；再切回「中」应与改动前中文文案一致（措辞未变）。
   回归闸：`i18n::tests::english_table_contains_no_cjk`（扫英文表禁 CJK）+
   `preferz_app::tests::flash_messages_follow_selected_language`（EN 下跑常用动作断言提示无中文）。

新增：**图表/流程图/自动保存三件套（2026-09-20，`fafd262`+`edb6a6a`+`96cf8ac`）——
🔶 代码交付，待 `cargo run` 复验**（详见 [CHANGELOG](CHANGELOG.md) 对应三节）：

1. **#8 图表粘贴**：Excel 里框选两列（标签+数值）复制 → 画布 `Ctrl+V` → 弹「柱状/折线/取消」
   居中浮层（附数据预览，>5 行有省略提示）→ 柱状/折线生成于视口中心、一条 undo；
   纯文本粘贴行为不变（图片路径回退）。复验项：柱/折线观感（网格/轴/标签/负值零线浮动）、
   移动缩放后渲染跟随、保存重开图表还在、切 EN 文案。
2. **#9 mermaid 流程图**：右键菜单「Mermaid 图表…」→ 输入 `flowchart TD` + `a[开始] --> b{判断}`
   等 → 生成 → 分层布局、节点=矩形/椭圆/菱形+绑定文字、边=两端绑定箭头（移动节点箭头跟随）、
   整批一条 undo；解析错误（如 `---`/`|标签|`/subgraph）flash 行号且输入保留。
3. **#5 自动保存**：打开某 `.prz` 编辑后静置 ≥30s（默认）→ 同目录出现 `.prz.autosave`、
   无「已保存」toast、标题脏标记仍在；把 autosave 改新后重开原文件 → 弹「恢复/忽略」；
   恢复后 current_file 仍指原文件且内容未保存；设置面板「自动保存」节开关+秒数滑块生效。

---

## 下一步：Excalidraw 打磨批次（Phase L 候选）

> **状态**：G / I / H / K 已全部交付（见 [CHANGELOG](CHANGELOG.md) §Phase G/I/H/K + §手工验收反馈批次）；
> 决策点 D1–D6 / I1–I4 已归档（见 [CHANGELOG §决策点归档](CHANGELOG.md)）。
> 本批次为对齐 Excalidraw 的观感/交互打磨项。5/6/11/12/13 已交付；
> **2026-09-10 拍板：启动 1/2/3 样式面板批次**（实施顺序 #2 → #3 → #1），决策点见表格「关键依赖 / 决策点」列。后续依次：4 → 7 → 8/9 → 10。
> **2026-09-11 启动 #4**：经 .ref/excalidraw 调研更正原倾向（Alt 在现行版是加顶点手势、删除走选中+Del），拍板见细节 §4。
> **2026-09-11 启动 #7**：调研更正——现行版新节点=同源克隆（非默认矩形+文字占位）、"下一元素"是独立的 Alt+方向导航；拍板见细节 §7。

| # | 打磨项 | 一句话方案 | 关键依赖 / 决策点 |
|---|---|---|---|
| 1 | ✅ 绑定文字对齐 + 字号 + 字体切换（2026-09-10 代码交付，待人工验收） | Text 加 H/V 对齐枚举 + 字体族（黑体/手写），侧栏加对齐分段与字号滑块 | ✅ 已拍板（2026-09-10）：手写体=**伪手写渲染**（复用 RoughStyler 思路，不嵌 TTF）；字号沿用现有滑块范围；垂直对齐仅绑定文字，自由文字固定 top-left |
| 2 | ✅ 调色板对齐 + 填充半透明（2026-09-10 代码交付，待人工验收） | `FILL_PICKS` 改为与描边同 5 色（黑红绿蓝橙）；填充加不透明度滑块（RGBA alpha） | ✅ 已拍板（2026-09-10）：top picks 描边/填充共用同组 5 色；填充默认 alpha **50%**（选填充样式但未显式改 alpha 时） |
| 3 | ✅ stroke width / sloppiness / edges 倒角（2026-09-10 代码交付，待人工验收；edges 经调研已被现有 roundness + curve_type 覆盖，无需新 UI） | `rough: bool` → `Sloppiness{Architect,Artist,Cartoonist}` 三档；edges（sharp/round）对齐 Excalidraw | ✅ 已拍板（2026-09-10）：sloppiness 对齐 Excalidraw 3 档；edges **对齐 Excalidraw 语义**——矩形→倒角，多边形/折线→顶点处切换曲线平滑（spline 类，具体插值实现时定） |
| 4 | ✅ 多边形节点增删 + 首尾重合自动闭合（2026-09-11 `43576da` 代码交付；2026-09-14 反馈更正 `8ba4ac5`：闭合首尾重合点渲染**合并视作一个点**、拖开 >8px **自动恢复开放**，见验收节） | 顶点删除（**Alt+单击顶点即删**）+ 端点追加（**Alt+拖真实端点**，越阈值在外侧追加顶点后拖新点）；端点拖拽释放首尾距 ≤ 屏 8px 自动 `closed` | ✅ 已拍板（2026-09-11，调研更正见细节 §4）：守卫=开放 ≥2 / 闭合 ≥3 顶点；删首尾顶点连该端解绑；命令层**复用 `EditShapePoints`**（加可选 closed 变更字段），不新增 DeleteVertex/AppendVertex |
| 5 | ✅ 直线/箭头端点吸附图形边缘（2026-09-07 `a803bbe` + `8d2465e` 修复） | 拖端点邻近 Shape 轮廓吸附 + 绑定模型（端点随形状移动） | 已拍板并交付（2026-09-08 复验通过）：阈值屏 10px；绑定=两端 `Option<ItemId>` 不存绝对坐标，`resolve_bindings` 动态重算；直线/箭头均可绑，与 #7 共用 |
| 6 | ✅ 多元素对齐、分布（2026-09-05 `cad908b`） | `arrange.rs` 增 `plan_align`（6 向）+ `plan_distribute`（等距/等心 × 横/纵）；属性栏「对齐」节 + 右键菜单 | 已拍板并交付：两种分布都做；参考系=选区包围盒；UI=属性栏+右键菜单 |
| 7 | ✅ Ctrl+箭头 添加连接符 + Alt+箭头 沿连接导航（流程图）（2026-09-11 `56dd3d4` 代码交付；2026-09-14 `8ba4ac5` 一次更正"邻居旁外推"；2026-09-22 **二次更正**：改分叉避让——移植 Excalidraw `placeCluster` 到 core `flowchart::place_node`，主轴恒相对源、交叉轴滑到最近空位，同向已有邻居时上下分叉不再重叠，见验收节） | 单选矩形/椭圆/菱形按 Ctrl+方向 = **按下即提交**一对：同源同风格克隆节点 + 两端绑定直箭头（一条 undo，选区跳新节点）；Alt+方向沿绑定邻居跳转选区 | ✅ 已拍板（2026-09-11，调研更正见细节 §7）：克隆非"默认矩形+文字占位"；不做 pending 簇预览/簇增长，**但移植交叉轴避障分叉**（2026-09-22，见细节 §7「落位」）；主轴间距 100px；箭头风格跟源、端头默认 Arrow；导航用 Alt+方向（现行 Excalidraw 同款分键） |
| 8 | 🔶 两列数据粘贴成柱状/折线图（2026-09-20 代码交付 `fafd262`，待人工验收） | 剪贴板 2 列 TSV/CSV → 生成 Chart item（柱状/折线） | ✅ 已拍板（2026-09-20）：**新 `ItemKind::Chart` + 矢量渲染**（逐段 painter，同 freedraw 路线，非 Pixmap）；**单系列**（2 列 = label+value）；**粘贴触发**——先检文本（Excel 复制同时带位图+文本，图片优先会截错）→ 非 2 列数值回退图片路径；弹「柱状/折线/取消」选择浮层；手绘风渲染/多系列/数据编辑留后续。交付见 [CHANGELOG §两列数据粘贴成柱状/折线图](CHANGELOG.md) |
| 9 | 🔶 mermaid 代码转流程图（2026-09-20 代码交付 `edb6a6a`，待人工验收） | mermaid 子集 → nodes+edges（复用 #5/#6/#7） | ✅ 已拍板（2026-09-20）：解析器选 **(b) 受限自研 Rust**（零依赖，支持 `flowchart`/`graph` TD/LR 的 node/edge/label 子集）；节点形状 `[]` 矩形 / `()` 椭圆 / `{}` 菱形；分层布局生成 Shape + 两端绑定 Arrow（复用 #5/#14 `EndpointBinding` + #7 `edge_anchor_local`）；入口=弹窗输入 mermaid 文本（生成按钮），错误 flash。交付见 [CHANGELOG §mermaid 代码转流程图](CHANGELOG.md) |
| 10 | ✅ 徒手绘制（freedraw）速度锥形墨迹（2026-09-18 拍板 B 档，交付 `1a8cda6`→`f869b86`→`b70c2a3`→`e1f46d8`；2026-09-20 Feihei 复验通过 ✅） | 新增 `Tool::Freehand`（绑裸 P + Num7）+ `ItemKind::Freedraw{points,pressures,stroke_width,color}`；按运笔速度（点间距/zoom）给每点算相对宽度乘子，落笔时 Catmull-Rom 重采样平滑，**逐段描边**渲染（line_segment + 圆帽，非 ribbon 填充）；选中可改颜色/粗细。详见 [CHANGELOG §徒手绘制](CHANGELOG.md) | 已拍板：D1=速度锥形（非等宽复用，egui 无压感→点间距模拟）；D2=P+Num7 双绑；D3=最小距离阈值采点、宽度平滑+锥形+Catmull-Rom 重采样、一条 AddItem undo；属性面维持颜色+粗细（对齐 Excalidraw freedraw，不吃 roughness） |
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

7. **Ctrl+箭头 添加连接符 + Alt+箭头 沿连接导航（流程图）**
- 现状：无。方向键仅裸键被 Present 导航占用（keymap 严格修饰匹配，Ctrl/Alt+方向空闲）；#5/#14 的 `EndpointBinding{target,anchor}` + `resolve_bindings` 钉点模型可直接承接双端绑定箭头。
- **调研更正（2026-09-11，.ref/excalidraw）**：现行版 Ctrl+方向（单选一个矩形/椭圆/菱形时）= 流程图创建，新节点是**同源同风格的克隆**（同类型/尺寸/颜色/风格，`flowchart.ts:228-247`），**不是** plan 原方案的"默认矩形+绑定文字占位"；连一条两端自动绑定的箭头（elbow、风格跟源、端头跟当前设置，`createBindingArrow:356-369`）；主轴间距固定 100px（`flowchart.ts:53-54`）。按住 Ctrl 连按=**pending 簇预览**（同方向簇增长+避障、松 Ctrl 一次提交、Esc 取消，`App.flowchart.ts:103-165`）。**"选中下一个元素"不在 Ctrl+方向**——现行版是独立的 **Alt+方向**沿箭头绑定导航（`App.flowchart.ts:133-147` + `FlowChartNavigator`）。
- **决策点已拍板（2026-09-11）**：
  - **新元素形态**：✅ 同源同风格克隆（源集合对齐 Excalidraw `isFlowchartNodeElement` = 矩形/椭圆/菱形，Polyline 不作源；不克隆绑定文字、不加占位）。
  - **提交方式**：✅ **按下即提交**——每按一次 = 一条 undo（`AddItems`[新形状+绑定箭头]），选区跳新节点，同方向连按自然接链。**不移植** pending 簇预览/簇增长（需新预览渲染态，成本不对等）。**但移植交叉轴避障**（2026-09-22 更正，见下）。
  - **导航**：✅ 另做 **Alt+方向**：从单选元素出发，找该方向**直接绑定邻居**（两终端 `binding.target` 恰一端=当前、另一端几何在该方向、取最近）→ 跳选区；不产生命令（与 Excalidraw 一致不进 undo）。Ctrl+方向恒创建，不做"有邻居则跳"复合。
  - **落位（2026-09-22 更正，取代原"邻居旁外推"）**：移植 Excalidraw `placeCluster`（`flowchart.ts:157-226`）的单节点退化，收进 core 纯函数 `preferz_core::flowchart::place_node`（egui-free、可无头单测）。**主轴**恒固定在**源**边界外 `FLOWCHART_GAP`（不再随邻居外推）；**交叉轴**把新节点在该列带内滑到离源中心**最近的空位**，障碍集=与源同连通子图（沿双端绑定 Polyline 双向 BFS，binary `connected_flowchart_rects`，对齐 Excalidraw `getConnectedFlowchartNodes` #8518）的其它节点包围盒。效果：同向已有下一节点时自动**上下分叉**而非重叠；无邻居（或带内无占用）时退化为原始"源旁 100px、交叉轴居中"单链行为。箭头走**直角折线 elbow**（见下），分叉时正交路由。
  - **箭头 elbow（2026-09-22 追加）**：`CurveType` 加第三态 `Elbow`（渲染期把两端点即时展开成正交折线，与 `Curved` 的 Catmull-Rom 同策，`points` 仍只存两点、`.prz` 零迁移）。core `item::elbow_polyline`：沿较小 Δ 轴先走短腿再垂直转折——对流程图连线恒正确（右/左连线水平先走、上/下连线垂直先走）。渲染（`outline_points`）/ 命中（`contains_canvas_point`）/ 逐像素导出 三处共用，一致。`add_connected_shape` 新建箭头默认 `curve_type=Elbow`。UI「边角」选择器 2→3 态。详见 [CHANGELOG §折线新增直角折线 elbow](CHANGELOG.md)。**仍不做**绕障碍的 elbow 路由（决策 D4，排后）。
  - **几何/样式**：间距=主轴 100px 画布（新节点边到源边）、同尺寸；连接=两端绑定的 Polyline，`curve_type=Elbow`（直角折线）、`end_arrow=Arrow`、`start_arrow=None`，stroke/手绘风参数跟源形状；两端 anchor=各自朝向对方的**边中点**（目标局部坐标），初始点位置即边中点（不加 Excalidraw 的 6px elbow padding，与 `resolve_bindings` 重算结果一致）；z 序=新节点在源之上、箭头最上（`add_item` 递增 z 天然满足）。
- 交付（2026-09-11 `56dd3d4`）：keymap 新增 `Action::AddConnectedShape`（Ctrl+方向×4）/`Action::NavigateConnected`（Alt+方向×4，`KeyBind::alt()` 构造器）+ `pressed_bind` API（方向取实际命中绑定键，改绑仍可用；`pressed` 变薄封装）；app `add_connected_shape`（复用 `duplicate_items` 得新 uuid/未编组/不带绑定文字的克隆 + `Item::new_polyline` 双端 `EndpointBinding`，`AddItems` preview 一条 undo，选区跳新节点）、`navigate_connected`（两端绑定箭头 + 主轴投影 `prim>0 && prim>=|orth|` 取最近，`expand_to_groups` 展开，不入 undo）；模块级 `FlowDir`/`FLOWCHART_GAP`/`edge_anchor_local`；i18n action_label×2；测试 5 项（锚点矩阵、克隆几何/绑定/z 序/undo-redo、非节点源静默、导航双向+无邻居保持）。

8. **✅ 两列数据粘贴成柱状/折线图（2026-09-20 代码交付 `fafd262`，待人工验收）**
- 现状：粘贴仅图片（Ctrl+V 释放沿）；无数据→图表。
- 方案：检测剪贴板文本为 2 列（TSV/CSV，≥2 行）弹「柱状/折线」选择，生成 `ItemKind::Chart`（新增）+ 矢量渲染（逐段 painter，零依赖）。先单系列，多系列排后。
- **决策点已拍板（2026-09-20）**：
  - **实现**：✅ 新 `ItemKind::Chart`（数据存 item：`chart_type`/`labels`/`values` + 描边样式），**矢量逐段渲染**（同 freedraw 路线），否决 Pixmap 位图（不可编辑、放大糊、.prz 体积大）。
  - **范围**：✅ 首轮**单系列**（2 列 = label+value）；粘贴后弹「柱状/折线/取消」选择浮层；手绘风渲染、多系列、图表数据再编辑均留后续。
  - **触发**：✅ 仅粘贴触发。**关键顺序**：`paste_from_clipboard` 先 `get_text()`——Excel/表格软件复制单元格时剪贴板**同时带位图和文本**，若先试图片会把数据表截成位图；文本非 2 列数值时再回退 `get_image()` 图片路径（行为不变）。

9. **✅ mermaid 代码转流程图（2026-09-20 代码交付 `edb6a6a`，待人工验收）**
- 现状：无。
- 方案：文本框/菜单输入 mermaid → 解析为 nodes（Shape）+ edges（Arrow，含 #5 绑定）。解析器选型：
  - (a) WASM mermaid（重，违零依赖/小包目标）；
  - (b) 受限自研 Rust 解析器（支持 `graph TD`/`flowchart` 的 node/edge/label 子集，零依赖，可控但覆盖有限）；
- **决策点已拍板（2026-09-20）**：
  - **解析器**：✅ (b) 受限自研 Rust（零依赖）。支持首行 `flowchart TD|LR` / `graph TD|LR`；行语法 `id[标签] --> id2[标签2]`、`a --> b`（复用已有定义）；节点形状 `[]`=矩形、`()`=椭圆、`{}`=菱形；边标签 `|text|`、`---` 无向线等暂不支持（报错 flash 指出行号）。
  - **布局**：分层布局——TD 自上而下 / LR 自左向右；层级=最长路径深度，同层按出现顺序排布，层间距/同层间距对齐 #7 的 100px 语义。
  - **生成物**：每节点一个 Shape（同 Excalidraw isFlowchartNodeElement 三类）、每边一条 Polyline 直箭头，两端 `EndpointBinding{target, anchor}` 复用 #7 `edge_anchor_local`（锚点=双方相对的边中点）；整批 `AddItems` 一条 undo；入口=右键菜单/命令弹窗输入 mermaid 文本 + 生成按钮，解析失败 flash 行号。

10. **✅ 徒手绘制（freedraw）速度锥形墨迹（2026-09-18 拍板，2026-09-18~20 交付，✅ 2026-09-20 Feihei 复验通过）** — 完整交付见 [CHANGELOG §徒手绘制（freedraw）速度锥形墨迹](CHANGELOG.md)。拍板：D1=**B 速度锥形墨迹**（新增 `ItemKind::Freedraw`，egui 无压感→以相邻点间距/zoom 当速度代理算每点相对宽度乘子，`pressures` 与标量 `stroke_width` 分离使属性 undo 保持 `Copy`）；D2=**裸 P + Num7 双绑**；D3=屏幕 2px 阈值采点、移动平均 + 首尾收笔锥形 + 落笔时 **Catmull-Rom 重采样平滑**、一条 `AddItem` undo。渲染经逐段描边（`line_segment` + 圆帽）而非 ribbon 填充；属性面维持「颜色 + 粗细」，与 Excalidraw freedraw 一致（不吃 roughness/sloppiness）。

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

> issues.md 中 **#5** 尚未实施。#1/#2 已交付，#3 于 2026-09-17 代码交付（待 `cargo run` 复验），
> #4 实为已由视口动作覆盖（见下），可直接勾掉。从此以 plan.md 为唯一工作视图。
> 验收后从 issues.md 勾掉对应项并补 [CHANGELOG.md](CHANGELOG.md)。

| # | 项 | 一句话方案 | 关键决策点 |
|---|---|---|---|
| 3 | ✅ Frame 常用演示比例预设（16:9 / 16:10 / 4:3 / 3:2 / 1:1 + A4 竖/横）（2026-09-17 代码交付，待人工验收） | 选中 Frame 时属性栏提供比例/纸张下拉，中心锚定套用；比例保持长边长度、A4 按 96 DPI 换算像素 | 预设清单=5 比例+A4 双取向；A4 绑 96 DPI；自定义比例输入本轮不做（留后续）；入口仅选中态、不做创建期面板 |
| 4 | ✅ 显示所有元素（Show All / Zoom to Fit）——**已由视口动作覆盖** | `Action::FitToScreen`（`Shift+1`，裸 `F` 让位给画框后仍保留 Shift+1）union **全部 item** AABB 后 `fit_to_content`，等价"显示所有"；`compute_fit`/`fit_to_content` 单点实现 | 无需新代码：已计入所有 item、空场景回退默认视口、快捷键 Shift+1 对齐 Excalidraw |
| 5 | 🔶 自动保存（Autosave）（2026-09-20 代码交付 `96cf8ac`，待人工验收） | 变更后 30s 无操作写 `.prz.autosave`（独立文件不动原文件），打开文件时检测较新 autosave 弹恢复提示 | ✅ 已拍板（2026-09-20）：**独立 `.prz.autosave` 文件**（不覆盖原文件，防写入中断损坏）；**30s debounce**（变更后无操作计时，有操作重置）；打开 `.prz` 时若同目录 autosave 比原文件新 → 弹「恢复/忽略」提示；不进 undo、不改变当前文档的未保存状态；设置面板开关（默认开）+ 间隔可调；未命名文档（从未存过盘）不自动保存。交付见 [CHANGELOG §自动保存](CHANGELOG.md) |

### 细节

3. **Frame 常用演示比例预设**
- 现状：创建 Frame 时尺寸自由，无预设比例/纸张快捷选择。
- 方案：Frame 创建/选中时提供常见演示比例与纸张预设（16:9、4:3、3:2、A4 等），选定后按当前单位/DPI 换算像素尺寸；可存默认偏好。
- 决策点：预设清单取舍；A4 等物理尺寸是否绑定 DPI 假设；自定义比例如何输入。
- **决策点已拍板（2026-09-17）**：预设清单 = 5 演示比例（16:9/16:10/4:3/3:2/1:1）+ A4 竖/横；
  **A4 绑 96 DPI**（794×1123）；纯比例保持当前**有效长边**长度不涉 DPI；入口只做**选中态属性栏下拉**
  （不做创建期候选面板）；**自定义比例输入本轮不做**（留后续）。调研：Excalidraw 无对应功能，属产品自定特性。
- 交付（2026-09-17）：core `SetFrameSize`（`FrameGeom` 快照批量命令，一步 undo，含 scale≠1 还原）+
  app `FramePreset`/`FRAME_PRESETS`/`apply_frame_preset`（中心锚定换算）+ 属性栏 `ComboBox` 入口 +
  flash + 10 i18n 词条；1 core 单测（几何往返）。待 `cargo run` 复验。

4. **✅ 显示所有元素（Zoom to Fit All）——已由视口动作覆盖，无需新代码**
- 现状：无独立"显示全部"按钮/入口，但 `Action::FitToScreen`（默认 `Shift+1`，Phase 1 的裸 `F`
  在 Phase K 让位给"选中画框"后仍保留 Shift+1）已 **union 全部 item 的 AABB** 再 `fit_to_content`。
- 结论：其语义即"适配全部内容"（`compute_fit`/`fit_to_content` 与"缩放到选中" `ZoomToSelection` 共用
  同一套公式），计入所有 item、空场景回退默认视口、Shift+1 对齐 Excalidraw。功能待办 #4 就此满足，勾掉。

5. **✅ 自动保存（Autosave）（2026-09-20 代码交付 `96cf8ac`，待人工验收）**
- 现状：仅手动 `Ctrl+S` 写 `.prz`，长时间编辑无自动落盘，崩溃丢工作。
- **决策点已拍板（2026-09-20）**：
  - **保存目标**：✅ 独立 `.prz.autosave` 文件（与打开的 `.prz` 同目录同名）——不覆盖原文件，防写入中断损坏主档。
  - **触发**：✅ **30s debounce**——任一变更（push_cmd）后计时，期间无新变更且达 30s 即写一次 autosave；新变更重置计时。定时器驱动复用 `request_repaint`，autosave 不进 undo 历史、**不清除文档的未保存状态**（原文件的 Ctrl+S 语义不变）。
  - **恢复**：✅ 打开某个 `.prz` 时若同目录存在较新的 `.prz.autosave`（mtime 晚于原文件）→ 弹「恢复 / 忽略」浮层；恢复=载入 autosave 内容且当前路径仍指向原 `.prz`；忽略=保留 autosave 文件不删。未命名文档（从未存过盘）不自动保存。
  - **设置**：✅ 设置面板开关（默认开）+ 间隔秒数（默认 30，最小 10）；持久化进 `config.json`。

---

## 远期 / 未决

> 详细记录在本地 `.issues/issues.md`（gitignore 排除、不入库）。

- [ ] ttf 字体体积优化（assets/simhei.ttf 全量嵌入）
- [ ] 放弃 `.bee` 兼容后可做的架构简化盘点
- [ ] 视口裁剪已有，LOD 缩略图（Phase 5+）待评估
