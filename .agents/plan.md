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
> **2026-09-24 调研完成 #20**：RoughStyler ↔ rough.js 4.6.6 + Excalidraw 实际用法逐行对照，发现六类不对齐（抖动幅度基准差 1.5–10 倍为大头），拍板分期与决策点见细节 §20。

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
| 16 | ✅ elbow E1 单段 bar 可拖（2026-09-23 交付，待人工验收；E2 避让路由**留待未来**） | E1 单段 bar 可拖：Shape 加 `elbow_mid_offset`（serde default 零迁移），`elbow_polyline_offset` 接受交叉轴偏移（短轴偏移+clamp+dedup），拖 bar 挪走线、端点/绑定重算后偏移保持；编辑护栏从「不吐手柄」改为「仅暴露 bar 手柄」（`Handle::ElbowBar` 带状命中+小方块绘制）；`DragState::ElbowBar` 拖拽态（begin/update/end）；`SetElbowOffset` 命令（skip_first_redo 预览惯例）。E2 障碍避让路由（留待未来）：移植 Excalidraw `elbowArrow.ts` 的 grid+A\*（`routeElbowArrow`/`calculateGrid`/`astar`），连线绕开节点 | ✅ 已拍板（2026-09-23）：E1 单段 bar 先行——偏移存**交叉轴单字段**而非 points 编码（E2 可作初值）；E2 **留待未来**，「连线穿过节点」痛点真实出现再评估立期（届时 points 完整存路由 vs 固定段列表派生一并定）。undo 命令=新 `SetElbowOffset` Copy 快照命令 ✅；offset clamp 至两端点内侧（`bar_axis` 兜底）✅；bar 手柄命中宽度=`max(stroke_width, 8px/zoom)`（`ELBOW_BAR_HIT_PX=8.0`）✅ |
| 17 | 🔶 mermaid 语法完整化（#9 后续，代码交付待人工验收） | 把受限子集扩到「实用流程图」：①**边标签** `-->|是|` / `-- 是 -->`；②**一行多分支 `&`**（`a --> b & c`、`a & b --> c & d` 交叉积）；③**箭头族** `---`/`-.->`/`==>`/`<-->` 等；④**更多节点外形** `((圆))`/`([体育场])`/`[(柱)]`/`{{六边}}`/`[[子程序]]`/`>旗]` + 引号标签 `"…"`。边距/分层布局已支持分叉（非本轮缺口）。subgraph、sequence/class/state **不做** | ✅ 已拍板（3 DP 全按推荐，2026-09-23）：DP1 全部近似映射（零 `ShapeType` 改动）；DP2 边内建 `label` 字段、渲染期画在曲线中点随重路由跟随（零 `.prz` 迁移）；DP3 `()`→Rectangle、`((…))`→Ellipse 校正为标准语义。批次 A/B 已交付，批次 C（barycenter 减交叉 + 边改 elbow）排后待评估 |
| 18 | 🔵 相乘叠合模式（Multiply 正片叠底 / 荧光马克笔）——**先不做**（2026-09-23 拍板；评估存档见细节 §18，将来重启可直接沿用） | M1（小）荧光马克笔预设近似：荧光色板 + 低不透明度默认填充（Excalidraw highlighter 同款半透明路线）；M2（大）真 multiply：数据模型 `BlendMode{Normal,Multiply}` + 导出侧 CPU 逐像素相乘（依赖导出重写为正向合成）+ 屏幕侧 `Shape::Callback`+glow 自绘 hack 或等 egui 上游 | ✅ 拍板（2026-09-23）：**先不做**（评估已完成并存档）。将来重启时的关键前提不变：egui 0.36.2 无 per-shape blend（epaint 无 BlendMode、glow 固定预乘 alpha）——屏幕实时真 multiply 是唯一硬点；M2 导出侧前置依赖导出管线重写（矢量元素目前不导出） |
| 19 | ✅ elbow 多顶点逐段展开（E1.5，2026-09-24 交付，待人工验收） | 闭合折线切 elbow → 转 open + 顶点全保留 + 每段居中正交展开 + 末段连回首点（视觉闭环，首尾重合保留）；多段不消费 `elbow_mid_offset`（仅两点线有意义）；SegmentMid 手柄对多顶点 elbow 恢复、两点线仍压制；bar 手柄仅两点线出现 | ✅ 已拍板（2026-09-24）：**不跟随 Excalidraw 的 line/arrow 类型分裂**（`elbowed` 仅 arrow、UI 三态仅 arrow 可见、切 elbow 丢中间点），保持统一 Polyline + `CurveType` 三态；三项决策全按推荐 |
| 20 | 🔶 RoughStyler 对齐 rough.js/Excalidraw 打磨（2026-09-24 调研完成，待启动） | 抖动幅度基准改 rough.js 公式（固定 2 画布px × roughnessGain × amp_scale × zoom，替 6%/8px）+ bowing 随机化 + preserveVertices 端点语义 + hachure 四件套（角度 -41→-49、线宽减半、斜线改完整抖动、随机相位+去 4px 下限）+ 箭头注释更正 | ✅ 已拍板（2026-09-24）：按四级优先级分期（① 幅度基准 → ② bowing/端点 → ③ hachure → ④ 其余按需）；待拍板 DP1–DP4：Architect 是否对齐 roughness 0、solid fill 是否顶点抖、箭头是否改抖、adjustRoughness 与 amp_scale 复合方式 |
| 21 | 🔶 elbow 多顶点改「顶点锚定 bar」纯函数推导（**取代 #19 的逐段居中 Z 展开**，2026-09-24 代码交付，待人工验收） | 中间顶点 = 正交 bar 的锚点（bar 过顶点、取向垂直于邻居对主导轴），路径=纯函数 `f(points)`；拖顶点即平移整根横/竖 bar，坐标对齐时路径自动直化（免费获得 Excalidraw 的对齐合并），段中点拖拽加点废止 | ✅ 已拍板（2026-09-24）：**不引入 Excalidraw `fixedSegments`**——elbow 与 polyline 同类型可互转、顶点必须保留，改为推导模型零新增存储；交付见 [CHANGELOG §elbow 多顶点改顶点锚定 bar](CHANGELOG.md)，细节与决策点见 §21 |

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
  - **箭头 elbow（2026-09-22 追加）**：`CurveType` 加第三态 `Elbow`（渲染期把两端点即时展开成正交折线，与 `Curved` 的 Catmull-Rom 同策，`points` 仍只存两点、`.prz` 零迁移）。core `item::elbow_polyline`：沿较小 Δ 轴先走短腿再垂直转折——对流程图连线恒正确（右/左连线水平先走、上/下连线垂直先走）。渲染（`outline_points`）/ 命中（`contains_canvas_point`）/ 逐像素导出 三处共用，一致。`add_connected_shape` 新建箭头默认 `curve_type=Elbow`。UI「边角」选择器 2→3 态。详见 [CHANGELOG §折线新增直角折线 elbow](CHANGELOG.md)。**编辑护栏**：elbow 线不暴露段中点加点手柄（`transform_handles::is_elbow_line` 在命中+绘制两处跳过 `SegmentMid`）——否则拖对角弦中点会塞进真顶点使两点不变量失效、退化成多段线且切不回；故 elbow 恒两点、只拖端点 + 面板自由来回切。**仍不做**自由多拐点重路由（naive「每段各自正交化」抓拐点瞬间会把另一根 bar 顶偏、形状跳变，需真正的直角路由器），列入 Phase L 候选、暂不做。
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

16. **✅ elbow E1 单段 bar 可拖（2026-09-23 交付，待人工验收；E2 避让路由留待未来）**
- **拍板（2026-09-23）**：E1 启动实施；E2（障碍避让路由）**留待未来**——待「连线穿过节点」痛点真实出现再评估立期。
- 现状：`CurveType::Elbow` 第三态已交付（渲染期把两端点展开为固定 Z 形正交折线，core `item::elbow_polyline`：沿较小 Δ 轴中点转折 3 段；渲染/命中/逐像素导出三处共用；编辑护栏=elbow 线不吐段中点手柄）。**评估结论**：对齐 Excalidraw 需补三块能力，差距不在「画」，在「编辑与路由」。对照 `.ref/excalidraw` `elbowArrow.ts`（2309 行）分层：
  1. **走线不可定制**：恒中点 Z 形，端点移动/绑定重算时形态单一，无法表达用户偏好的走线位置（现 `elbow_polyline` 只认两端点、无任何用户状态参与）。
  2. **无 bar 拖动**：Excalidraw 可拖中间正交段（bar）平移走线——`handleSegmentRenormalization`(113)/`handleSegmentMove`(465)/`handleSegmentRelease`(282) 三兄弟：拖动时保持 bar 段位置、只伸缩相邻正交段（不推歪整线），释放把偏移写回；`handleEndpointDrag`(706) 端点拖动时 bar 相对位置尽量保持。
  3. **无障碍感知**：Excalidraw `routeElbowArrow`(1439) = `calculateGrid`(1853) 动态网格 + `generateDynamicAABBs`(1668) 障碍 AABB 合并（`commonAABB`）+ **A\* 寻路**（`astar` 1537 + `getNeighbors`/`pathTo`/`m_dist`）+ `removeElbowArrowShortSegments`(2186)/`getElbowArrowCornerPoints`(2158) 后处理——连线自动绕开节点。
  4. **端点 heading**：`getBindPointHeading`(2253)/`offsetFromHeading`(1509)——绑定时按对方方位决定出口边与出线偏移；现实现为边中点锚 + 几何推导，对流程图连线够用。
- **分期**：
  - **E1（已交付）单段 bar 可拖**：
    - 存储：`ItemKind::Shape` 加 `elbow_mid_offset: f32`（`#[serde(default)]`=0，`.prz` 零迁移）。定义为**交叉轴偏移**：以两端点连线为纵轴、bar 沿横轴偏离中点的有符号距离——旋转时随端点局部轴系天然跟随；将来 E2 可作初值。
    - 几何：`elbow_polyline_offset(pts, offset)`：转折点沿交叉轴平移；offset=0 退化为现行为（`elbow_polyline` 薄封装）；`bar_axis` clamp 防止 bar 越过任一端点；`dedup()` 去零长段（防箭头方向 NaN）。三处消费点（`outline_points` / `contains_canvas_point` / 逐像素导出经 contains 间接）统一传现值，保持一致。
    - 交互：`DragState::ElbowBar { item_id, start_canvas, start_offset }`；`Handle::ElbowBar` 带状命中（`ELBOW_BAR_HIT_PX=8.0`，`max(stroke_width, 8px/zoom)`）+ 小方块绘制；拖动期 live 直改预览（逆变换到局部取短轴分量）、释放入 `SetElbowOffset`（`skip_first_redo: true`）。
    - 联动：`resolve_bindings` 重算端点后用现 offset 重建走线——端点动、bar 相对位置保持（Excalidraw renormalization 的单段退化）。
  - **E2（后做、可选）障碍避让路由**：完整移植 grid + A\*（估算 core 新 `elbow.rs` 600–1000 行；障碍收集复用 #7 `connected_flowchart_rects` 的连通子图 BFS 思路扩展到任意形状）。解决「连线穿过节点」。若 E1 后无此痛点可无限期缓。届时单 offset 字段不够，需升级存储——**方案 X**：points 存完整路由结果 + renormalize 语义（Excalidraw 同构，改动面大）；**方案 Y**：两点 + 「用户固定段列表」派生。heading 语义随 E2 评估。
- 涉及面：core `item.rs`（字段 / 几何签名 / 三消费点）+ `commands.rs`（`SetElbowOffset` Copy 快照命令）；binary `drag.rs`（ElbowBar 态 begin/update/end）+ `transform_handles.rs`（bar 手柄命中+绘制）+ `stylers.rs`（`ShapeData.elbow_mid_offset` + `outline_points` 传偏移）；fileio 零表结构改动（kind JSON blob 内 serde default）；i18n 零新词条（E1 无 flash 无面板输入）。
- **已拍板**：① undo 命令=新 `SetElbowOffset` Copy 快照命令 ✅；② offset clamp 至两端点内侧（`bar_axis` 兜底）✅；③ bar 手柄命中宽度=`max(stroke_width, 8px/zoom)`（`ELBOW_BAR_HIT_PX=8.0`）✅；④ E2 留待未来 ✅。
- **手工验收清单**：
  - [ ] 选中 elbow 连线，鼠标移到中间正交段（bar）上出现 Grab 光标
  - [ ] 拖动 bar 平移走线，bar 沿短轴移动、端点不动、预览实时跟随
  - [ ] 拖到接近端点时 clamp 不出回钩（bar 恒在两端点内侧）
  - [ ] 释放后 undo 一步恢复原位、redo 一步恢复拖后位
  - [ ] 拖动绑定端点（移动被连形状）后 bar 偏移保持（走线跟随重算）
  - [ ] 保存 `.prz` 重开偏移保持；旧档（无 `elbow_mid_offset` 字段）默认 0 不报错

17. **🔶 mermaid 语法完整化（#9 后续，代码交付待人工验收）**

- **现状校准（先纠正一个误解）**：`mermaid.rs` 的分层布局**已支持分叉/汇合**——`a --> b` / `a --> c` 分写两行即被 `intern_node` 复用同一 `a`、`layout_flowchart` 按最长路径把 b/c 摆进同一层（交叉轴并排），`flowchart::tests::layout_levels_follow_longest_path` 已在把关。**所以「不能画侧链」不是布局缺陷**。真正让侧链「不可读/不可用」的是三处**显式报错**：
  1. **边标签** `-->|是|`（`mermaid.rs:90` 直接 `Err`）——决策分支的「是/否」标注，缺了它菱形分叉看不出走哪条边，这是侧链不可用的**首要原因**；
  2. **一行多分支 `&`**（`a --> b & c`）——写同向侧链最省事的语法，现在 `b & c` 被当非法 id（含空格/`&`）报错；
  3. **箭头族 / 多外形**——`---` 无向、`-.->` 虚线、`==>` 粗线、`<-->` 双向、`((圆))`/`([体育场])`/`[(柱)]`/`{{六边}}`/`[[子程序]]`/`>旗]`、引号标签 `"…"` 全部落不到解析（`mermaid.rs:105`）。

- **目标模型硬约束（决定「映射」的成本，务必先看）**：
  - `shape.rs:5` `ShapeType` **只有** `Rectangle / Ellipse / Diamond / Polyline`——**没有**圆角矩形、体育场、六边形、柱体、子程序、旗形。→ 额外外形只能「映射到近似」或「新增 `ShapeType`」（见 DP1）。
  - `shape.rs:42` `DashStyle = {Solid, Dashed, Dotted}` + `StrokeStyle.width` + `ArrowHeadStyle = {Arrow, Dot}`（`Option=None` 表无箭头）——**箭头族全部可零成本承载**（见下表），无需新枚举（`--x` 叉形头除外，见下）。
  - `()→Ellipse` 现状与 mermaid 语义**冲突**：mermaid `()` 是圆角矩形、`((…))` 才是圆。本轮要么校正（`()`→Rectangle 近似、`((…))`→Ellipse），要么保留（观感差异，需明说）。
  - `ItemKind::Shape`（含 Polyline）整体存 JSON blob → 加字段**零 `.prz` 迁移**（I4）；穷尽 `match` 仅三处（`render.rs` ×2、`export.rs::sample_item_pixel`、`fileio item_kind_str`），新增 `ShapeType` 变体须同步这四处 + 几何/命中。

- **语法覆盖表（→ 目标映射，批次 A 解析 + 批次 B 落地）**：

  | mermaid 写法 | 语义 | start→end 箭头 | dash | width |
  |---|---|---|---|---|
  | `-->` | 实线箭头（已有） | None→Arrow | Solid | 常规 |
  | `---` | 无箭头连线 | None→None | Solid | 常规 |
  | `-.->` / `-.-` | 虚线箭头 / 虚线 | (Arrow)/None | Dashed | 常规 |
  | `==>` / `===` | 粗线箭头 / 粗线 | (Arrow)/None | Solid | ×2 |
  | `<-->` | 双向 | Arrow→Arrow | Solid | 常规 |
  | `-->|文本|` / `-- 文本 -->` | 带标签 | 同上 | 同上 | 同上 |

  节点外形：`[…]`→Rectangle、`{…}`→Diamond（已有）；`((…))`→Ellipse、`(…)`/`([…])`→Rectangle（圆角近似，DP1 若通过则 Stadium 独立）；`{{…}}` 六边 / `[(…)]` 柱 / `[[…]]` 子程序 / `>…]` 旗 / `[/…/]` 平行四边形——**取决于 DP1**。`--x`/`--o` 叉形/圆点端：`--o`→`Dot`（现成）、`--x` 无对应头 → 降级为 `Arrow` 并注明。

  `&` 交叉积：一行按箭头切段，每段再按 `&` 拆成节点列表，相邻段做笛卡尔积成边（`a & b --> c & d` = a→c,a→d,b→c,b→d；`A --> B & C --> D` = A→B,A→C,B→D,C→D），与 mermaid 一致。引号标签 `["含 空格"]` 与首行前预处理（智能引号 `“”`→`"`、容忍行尾 `;`）参照 `.ref/excalidraw` `mermaidAutoFix.ts` 的归一化思路。

- **拟分三批（各自可独立验收、可拆 commit）**：
  - **批次 A｜解析层（`mermaid.rs`，core L1、纯函数、零依赖）**：`MermaidEdge` 加 `kind: MermaidArrow`（枚举）+ `label: Option<String>`；`MermaidShape` 扩变体（视 DP1）；`parse_node_token` 支持双括号嵌套（`((`/`([`/`[[`/`[(`) 与引号体；`&` 拆段 + 笛卡尔积；`%%`/空行/行尾 `;`/智能引号预处理；错误仍带行号。→ 单测矩阵（每种箭头/外形/`&`/标签/引号各一条 + 非法回退）。
  - **批次 B｜映射层（`shape.rs`/`item.rs`/`actions.rs::generate_mermaid_flowchart`）**：`MermaidArrow`→(start/end `ArrowHeadStyle`, `DashStyle`, `width`)；`MermaidShape`→`ShapeType`（按 DP1 结论）；边标签按 DP2 结论落地；`()`→ 校正映射（若采纳）。→ 无头单测断言生成的 item 字段。
  - **批次 C｜布局质量（`mermaid.rs::layout_flowchart`，可选/排后）**：`&` 展开后同层变宽 → 现「按出现顺序」排布易交叉，可加 **重心排序（barycenter）** 减少边交叉；边默认改 **elbow**（复用 #7 `CurveType::Elbow`）更贴 mermaid 正交观感。→ 视 A/B 验收后再定是否本轮做。

- **决策点（✅ 已拍板 2026-09-23，全按推荐列）**：
  - **DP1｜额外节点外形**：✅ **(a)+(c)**——解析期把冷门形近似收敛（`((…))`/`([…])`→Ellipse、`(…)`/`[[…]]`/`[(…)]`/`{{…}}`/`>…]`/`[/…/]`→Rectangle、`{…}`→Diamond），零 `ShapeType` 改动、`.prz` 零迁移。新增 `ShapeType::{Stadium,Cylinder,Hexagon}` 忠实渲染留验收反馈后再评估。
  - **DP2｜边标签形态**：✅ **(b)**——给线性 `ItemKind::Shape` 加 `label: Option<String>` 字段（`#[serde(default)]`，`.prz` 零迁移），渲染期 `draw_edge_label` 每帧把标签画在**线段中点**（两点直边中点=包围盒中心），端点重路由时自动跟随；`generate_mermaid_flowchart` 生成时把 `MermaidEdge.label` 写入该字段，整批 `AddItems` 一条 undo。
  - **DP3｜`()→圆` 语义校正**：✅ **校正**——`()` 圆角矩形→Rectangle、`((…))` 圆→Ellipse（解析期收敛），与 mermaid 文档一致。

- **交付（2026-09-23，批次 A/B）**：core `mermaid.rs` 重写为手写扫描器（`scan_link`/`scan_node`/`parse_shape_body`）——箭头族 `MermaidArrow{Arrow,Open,Dotted,DottedOpen,Thick,ThickOpen,Double}`、`MermaidEdge{kind,label}`、`&` 交叉积、双括号外形（DP1 近似收敛）、pipe `-->|t|` 与 inline `-- t -->`（仅实线）、引号标签、`%%`/行尾 `;` 预处理、`subgraph`/`style`/… 显式报错带行号；15 单测（矩阵 + 非法回退 + 布局）。binary `item.rs` 加 `label` 字段 + `with_label`/`label()`；`actions.rs::generate_mermaid_flowchart` 按 `MermaidArrow`→(start/end 头, `DashStyle`, width×) 映射并落边标签；`render.rs` `draw_edge_label`（两条渲染路径共用）。**未做**：批次 C（barycenter 减交叉 + 边改 elbow）排后待评估；导出为矢量软栅格、Shape 恒 `None`，标签无需改导出。质量门全绿（fmt / clippy -D / core 181 + fileio 9 + binary lib 76，唯一失败 `tick_autosave_writes_sidecar` 经 git stash 验证为**先前已存在的无关 flaky**）；无头冒烟无 panic。待 `cargo run` 人工验收（观感：侧链标签跟随、箭头族线型、特殊形近似）。

- **不在本轮范围（明确划出，避免蔓延）**：`subgraph` 分组框（嵌套布局 + 容器落位，最贵，用户已选「不纳入」）；sequence / class / state / ER / gantt 等**非 flowchart 图种**（用户已选「预留扩展但本轮不做」）。**架构预留**：解析入口 `parse_mermaid_flowchart` 首行按图种关键字分派，未来加图种 = 新 `match` 臂 + 独立布局模块，不改 flowchart 分支；建议在函数 doc 注明此为图种分派点。

18. **🔵 相乘叠合模式（Multiply 正片叠底 / 荧光马克笔）——先不做（2026-09-23 拍板，评估存档备将来重启）**
- **拍板（2026-09-23）**：先不做。以下评估与决策点存档，将来重启可直接沿用。
- 目标观感：荧光马克笔——黄色相乘叠在白底→黄、叠在黑字上→**字仍黑**（multiply 不遮字）；对比半透明黄（normal blend）文字会被罩灰发闷。
- **技术现状核实（2026-09-23，egui 0.36.2 源码 + 本仓库导出实现）**：
  1. **egui 0.36 不支持 per-shape 混合模式**：epaint 0.36.2 全源码无 BlendMode 概念，Shape 展平进共享 mesh 后无 per-item blend 通道；egui_glow 每帧固定 `blend_func_separate(ONE, ONE_MINUS_SRC_ALPHA, …)`（预乘 alpha，`egui_glow/src/painter.rs:316`）。上游 per-shape blend 属长期未落地需求。**屏幕实时真 multiply 是本功能唯一硬点**。
  2. **导出管线无叠加语义**：`export_scene_to_file` 为逐像素「Z 序倒序首个命中项采样」（每像素只取一个 item 颜色混白底即 break），且 Shape/Freedraw 采样返回 `None`（矢量元素不导出）——半透明叠加在导出侧本身就不正确，multiply 无从谈起。**注意**：Excalidraw 无 per-element blend 功能（只有 opacity），此项属产品自定特性、无对齐基线可参照，观感基准自行定义。
- **分期与决策**：
  - **M1（小，可与 M2 独立交付）荧光马克笔预设（近似观感）**：不做真 multiply。荧光色板（黄/绿/粉/蓝/橙，高饱和荧光色）+ 选中即套低不透明度默认填充（复用 #2 的 alpha 机制与分档惯例）；范围限 Shape 填充 + Freedraw。局限：文字上方仍 normal blend 罩灰——但 Excalidraw 的荧光观感实为半透明方案，成本 ≈ 色板 + i18n，零渲染风险。
  - **M2（大）真 multiply**：
    - 数据模型：`blend: BlendMode{Normal, Multiply}`（`#[serde(default)]`=Normal，`.prz` 零迁移）挂 Shape（填充）/Freedraw（/Pixmap 可选）；侧栏「叠合」2 态分段控件。
    - **导出侧（容易）**：导出重写为**正向 painter 合成**（Z 正序逐 item 累积）后，multiply = CPU 逐像素 `out *= src/255` 免费真值；PNG 天然支持、JPEG 白底下等价。**前置依赖 = 导出管线重写**（矢量元素软件光栅化 + 正向合成）——该重写本身是独立大项（顺带解决矢量元素导出缺失），M2 排其后。
    - **屏幕侧（难，风险高）**：唯一通路 `Shape::Callback`（PaintCallback）+ glow 自绘：对该 item 单独 tessellate 出 mesh，回调内临时 `blendFunc(DST_COLOR, ONE_MINUS_SRC_ALPHA)` 画完恢复（egui_glow 回调入参含 `&glow::Context`，通路存在）。代价：自维护 mesh→glow 绘制路径（~200–300 行）+ GL 状态恢复脆弱（scissor/SRGB/纹理绑定），egui 升级易碎；或**等上游** per-shape blend 落地后回归常规路径。
    - 建议：M2 拆「模型+导出」与「屏幕实时预览」两步；屏幕侧是否接受 PaintCallback hack 为核心决策点。
- **待拍板**：① 范围（仅填充 / +freedraw / +图片）；② M1 是否独立先上（推荐：可先上缓解 80% 观感需求）；③ 屏幕侧 PaintCallback hack 接受与否（vs 等上游 vs 屏幕近似/导出真值的过渡不一致）；④ 荧光色默认不透明度档位（对齐现有 [15,30,50,75,100]% 分档）。

19. **✅ elbow 多顶点逐段展开（E1.5，2026-09-24 交付，待人工验收）**
- **背景**：E1 交付后对照 Excalidraw 发现其 line/arrow 类型分裂——`elbowed` 字段仅 `ExcalidrawArrowElement`（`types.ts:355-359`）、UI 三态面板仅 arrow 可见（`actionProperties.tsx:2212` 谓词 `isArrowElement`）、切 elbow 时丢中间点（`actionProperties.tsx:2002-2013` 重置 points 到起点）。**拍板不跟随**，保持 PReferZ 统一设计（全 Polyline + `CurveType` 三态），理由：① line/arrow 本就同类型仅差 `end_arrow` 属性、随时互切，绑死 elbow→arrow 会制造「线加箭头 elbow 才出现」的状态耦合；② Excalidraw 分裂是产品定位产物（elbow 与 fixedSegments+fixedPointBinding+heading 连接线机制耦合），非更优建模；③ 统一已付成本（E1 命中/渲染/导出三源、`.prz` 零迁移）。
- **护栏语义升级为多顶点 elbow**：闭合折线切 elbow → `closed` 转 false、顶点全保留、每段正交展开（短轴中点转折，与 E1 同款几何）、末段连回首点保持视觉闭环（闭环由显式点列保证、不依赖 `closed` 标志）。
- **已拍板（2026-09-24）**：
  1. **闭合→开放+末段回首点**：`closed=false` + 几何末段显式连回首点（视觉闭环）
  2. **多段居中**：多顶点 elbow 每段居中展开、不消费 `elbow_mid_offset`；该字段仅两点线有意义、bar 手柄（E1）仅两点线出现；将来逐段偏移再升级存储（`#[serde(default)]` 兼容）
  3. **SegmentMid 恢复**：多顶点 elbow 恢复段中点加顶点手柄（新顶点与相邻顶点正交连接）；两点线仍压制段中点（拖它必破两点语义）
- **实施要点**：
  - core `item.rs`：新 `elbow_multi_polyline(pts, closed) -> Vec<(f32,f32)>`（逐段调 `elbow_polyline_offset` 居中拼接 + 去重；closed 时补末段回首点）；`contains_canvas_point` elbow 分支按点数分流——两点走 `elbow_polyline_offset`（带 offset）、多顶点走 `elbow_multi_polyline`
  - binary `stylers.rs`：`outline_points` elbow 分支同步分流
  - binary `transform_handles.rs`：`is_elbow_line` 护栏改为「两点线压制 SegmentMid、多顶点放行」；bar 手柄条件加 `points.len()==2`
  - 切换入口 `props.rs::push_poly_curve`：切 elbow 时若 `closed=true` 打包 `SetClosed(false)`（一条 undo，`MultiCommand` 或 `SetCurveType` 带 closed 变更）
  - 测试：多段展开正交性 / 末段回首点视觉闭环 / 切换 closed 打包 undo / SegmentMid 恢复后加顶点仍 elbow / 两点线行为不变（E1 回归）
- **不做**：逐段独立偏移（拍板居中）；E2 避让路由仍留待未来

20. **🔶 RoughStyler 对齐 rough.js/Excalidraw 打磨（2026-09-24 调研完成，待启动）**

- **背景**：RoughStyler 是 rough.js 4.6.6 的 Rust 自移植（[ADR-0005](adr/0005-shape-styler-rough-seeded.md)：零新依赖、SeededRng、双线笔触、画布像素×zoom 抖动）。本轮以 roughjs 4.6.6 源码（`generator.js`/`renderer.js`/`hachure-filler.js`/`scan-line-hachure.js`/`hachure.js`）+ Excalidraw 实际用法（`generateRoughOptions`/`_generateElementShape`）逐行对照，发现六类不对齐（按观感影响排序）。**已对齐、保留不动**：PASSES=2 双线、divergePoint 公式、Catmull-Rom 公式（curveTightness=0）、cross-hatch 双角结构、闭合收尾边、抖动随 zoom 缩放语义（同 Excalidraw 局部坐标生成+画布变换）、确定性种子。

- **不对齐清单（rough.js 公式均经源码核实）**：

  1. **抖动幅度基准不同（最大项）**。rough.js `_line`（renderer.js:246-318）固定 `maxRandomnessOffset = 2` 画布 px × `roughnessGain`（边长 <200 → 1；200–500 线性 0.9→0.4；>500 → 0.4），underlay 端点/控制点 ±2px·gain、overlay ±1px·gain；PReferZ 走 `min(边长×6%, 8px)`（`OFFSET_RATIO`/`MAX_OFFSET_CANVAS`，stylers.rs:468-470、620-622）。zoom=1 Artist 档 underlay 对比：

     | 边长 | rough.js ± | PReferZ ± | 倍数 |
     |---|---|---|---|
     | 100px | 2.0 | 3.0 | 1.5× |
     | 200px | 1.8 | 6.0 | 3.3× |
     | 300px | 1.5 | 8.0 | 5.3× |
     | 1000px | 0.8 | 8.0 | 10× |

     另缺 Excalidraw `adjustRoughness` 小图衰减（maxSize<10 → roughness/3、<20 → /2，excalidraw-shape.ts:172-193）。
  2. **bowing 方向恒定**。`sketch_edge`（stylers.rs:624-627）`mid_disp = (-d.y, d.x)·bow` 符号由边方向唯一决定 → 矩形四边一致外凸成"吹气感"；rough.js `midDisp` 过 `_offsetOpt`（renderer.js:267-268）**每次随机符号**。幅度也偏大：PReferZ = max_offset×bow_k（100px 边 1.5px、300px 边 8px）vs rough.js = |dy|/100×gain（100px 边 1px、300px 边 2.5px）。
  3. **端点抖动语义相反**。Excalidraw architect/artist 档 `preserveVertices=true`（excalidraw-shape.ts:224-225）**端点不抖**（rough.js renderer.js:272/277-313 同款开关）；PReferZ 全档位端点 ±half（长边 ±4px），直接影响绑定/拼接接头（端点对不齐）。
  4. **Hachure 四处偏差**。① **角度**：rough.js `hachureAngle + 90` 语义（scan-line-hachure.js:4），默认 -41 实际产出 **49° 仰角**斜线；PReferZ 直接把 -41 当旋转参数产出 41° 仰角（stylers.rs:269、288-290），cross-hatch 两组随之与 rough.js 互换；② **线宽**：Excalidraw `fillWeight = strokeWidth/2`（excalidraw-shape.ts:220），PReferZ 填充线用满 `line_width`（stylers.rs:328、710）；③ **斜线太直**：rough.js 每条填充线走 `doubleLineOps` 完整双线抖动（hachure-filler.js:17），PReferZ 仅端点 ±0.15·gap（stylers.rs:695-707）；④ **相位/gap**：rough.js roughness≥1 时 ~30% 概率 `skipOffset=gap` 跳首线 + `round(max(gap,0.1))`（scan-line-hachure.js:9-15），PReferZ 恒定 `ymin+gap/2` + 4px 下限（stylers.rs:273、297）。
  5. **椭圆/曲线抖动公式问题**。`curve_jitter_amp`（stylers.rs:556-571）按 `6%×平均采样间距`：Ø400 椭圆 avg≈52px → 3.14px，而 rough.js 椭圆点经 `_curveWithOffset(offset=1/2)` 只抖 ±1px/±2px（renderer.js:319-344），**约 3 倍**；Curved 折线仅 0.2–0.4px（采样密）过于平滑，与历史反馈"曲线手绘样式都一样"同源。另缺 rough.js 随机起始相位（`radOffset`，renderer.js:410）与自适应采样数（`generateEllipseParams`，renderer.js:73-83）。
  6. **其他**。① stylers.rs:173-175 注释"Excalidraw 同样只在笔画上抖、箭头保持规整"**与事实不符**——Excalidraw 箭头是 rough polygon、`roughness: min(1, roughness)` 会抖；② dash 模式未 `disableMultiStroke` + `strokeWidth+0.5`（excalidraw-shape.ts:210-216）；③ solid fill rough.js 顶点也抖 ±2px（`solidFillPolygon`），PReferZ 精确（ADR-0005 有意决策）；④ 圆角矩形粗糙抖动走直边采样而非 rough.js `_bezierTo` 平滑抖动；⑤ zigzag/dots 填充为功能缺口（ADR-0005 已注明后续自移植）。

- **分期（四级优先级，各自可独立验收、可拆 commit）**：
  - **批次 1｜抖动幅度基准改 rough.js 公式**：`sketch_edge`/`curve_jitter_amp` 的 `max_offset` 由 `min(len×6%, 8)·zoom·amp_scale` 改为 `2 画布px × roughnessGain(len) × amp_scale × zoom`（roughnessGain 按 rough.js 分段：<200→1、200–500 线性至 0.4、>500→0.4；顺带继承 rough.js 短线衰减 `offset=len/10`（len<20px）），修不对齐 #1 与 #5 的幅度部分；`adjustRoughness` 小图衰减的复合方式见 DP4
  - **批次 2｜bowing 随机化 + preserveVertices 端点语义**：`mid_disp` 符号改 SeededRng 随机；`sketch_edge` 增"端点是否抖动"入参——Architect/Artist（roughness < Cartoonist）端点不抖、仅 Cartoonist 抖（修 #2、#3）
  - **批次 3｜hachure 四件套**：① `HACHURE_ANGLE_DEG` -41 → **-49**（对齐 rough.js 有效 49° 仰角；cross-hatch 第二组自动变 +41，与 rough.js 一致）；② 填充线宽 `stroke_width/2`；③ 斜线段改走 `sketch_edge` 复用完整双线抖动（端点精确语义按批次 2 结论）；④ 随机相位（roughness≥1 时 30% 概率跳首线）+ gap 下限 4px → `round(max(gap, 0.1))`（修 #4）
  - **批次 4｜其余按需**：更正 stylers.rs:173-175 箭头注释（是否改抖见 DP3）；dash 模式 `disableMultiStroke`+`strokeWidth+0.5`；椭圆自适应采样数（`generateEllipseParams`）+ 随机起始相位（radOffset）；DP1 Architect 语义、DP2 solid fill 顶点抖动；圆角矩形 `_bezierTo` 平滑抖动；zigzag/dots 填充

- **决策点（待拍板）**：
  - **DP1｜Architect 档语义**：Excalidraw architect = roughness 0（描边干净、仅 fill 有 hachure）；PReferZ Architect = 0.5× 抖动。维持产品现状 or 对齐 Excalidraw？（影响 sloppiness 四档命名/默认与 plan #3 已交付 UI 文案）
  - **DP2｜solid fill 顶点抖动**：ADR-0005 有意保持精确几何；rough.js `solidFillPolygon` 顶点抖 ±2px。维持 or 跟随？
  - **DP3｜箭头是否改抖**：ADR-0005 有意保持规整；Excalidraw 箭头 = rough polygon 会抖（`roughness: min(1, roughness)`）。维持 or 跟随？
  - **DP4｜adjustRoughness 复合方式**：小图衰减（maxSize<10→/3、<20→/2）与 `Sloppiness::amp_scale` 相乘 or 覆盖（取 min）？

- **涉及面**：binary `stylers.rs`（常量 `OFFSET_RATIO`/`MAX_OFFSET_CANVAS`/`HACHURE_ANGLE_DEG`/`ELLIPSE_SEGMENTS`、函数 `sketch_edge`/`curve_jitter_amp`/`hachure_gap`/`hachure_segments`/`hachure_shapes`/Rough hachure 填充块 L683-714、`push_arrow_heads` 注释；**既有测试数值断言需同步更新**——矩形 8 shapes、椭圆 48=24×2、开放线 4、带填充 9、400px 椭圆控制点距中心 100..300 等）；core `shape.rs` SeededRng 复用（无改动预期）；DP1–DP3 若改变 ADR-0005 原决策需补录 ADR。

- **手工验收清单**：
  - [ ] 与 Excalidraw 同 seed 同尺寸矩形/椭圆/折线并排观感对比（抖动幅度、bowing 是否还有"吹气感"）
  - [ ] 各 Sloppiness 档差异可感知（Off/Architect/Artist/Cartoonist）
  - [ ] 小元素（<20px）抖动衰减自然、大元素（>500px）不过抖（roughnessGain 生效）
  - [ ] 端点接头/绑定场景（直线贴形状边缘）不因端点抖动脱开
  - [ ] hachure/cross-hatch 角度（49°/41°）、密度、线宽（半宽）观感对齐
  - [ ] 缩放/保存 `.prz` 重开不跳变（种子确定性保持，无 NaN）

- **质量门**：`cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`（零警告）、`cargo test --workspace` 全绿；新增单测断言抖动幅度落入 rough.js 公式区间（如 100px 边 underlay ≤ ±2px·zoom、300px 边 ≤ ±1.5px·zoom）。

21. **🔶 elbow 多顶点改「顶点锚定 bar」纯函数推导（取代 #19 的逐段居中 Z 展开，2026-09-24 代码交付，待人工验收）**
- **背景（2026-09-24 用户反馈）**：#19 交付的逐段居中 Z 展开实测两大问题——①每段独立 Z 形导致顶点一多折线碎乱（3 顶点出 7 段）；②段中点手柄拖拽=插顶点（与两点线「拖 bar=平移走线」体验割裂），难控制。调研 Excalidraw elbowArrow.ts 确认其模型（不存中间顶点、拖段=写 `fixedSegments`、`handleSegmentRenormalization` 做共线合并+短段折叠+索引重编号）。
- **关键约束（决定不照搬 Excalidraw）**：PReferZ 的 elbow 与 polyline 是**同一种元素**（统一 Polyline + `CurveType` 三态，#19 拍板），互相转换必须无损往返 → **顶点必须保留为用户数据**。Excalidraw 的 `fixedSegments` + 派生模型会让顶点失去数据地位，切回 polyline 无法还原。因此改为「顶点锚定 bar」：顶点仍是唯一用户数据，**路径降级为纯函数 `f(points) → 折线点列`**，零新增存储、零 `.prz` 迁移。
- **推导规则（已拍板 2026-09-24）**：
  1. **中间顶点 Pi 锚定一根 bar**：bar 过 Pi 本身，取向**垂直于 (P(i-1), P(i+1)) 的主导轴**——`|dx| <= |dy|`（水平主导/平）→ 竖 bar（x=Pi.x），否则横 bar（y=Pi.y），与两点线 `elbow_polyline_offset` 的「短轴优先」启发式一致。
  2. **相邻 bar 之间用垂直跑段连接**（同向相邻 bar 亦成立：跑段取后一 bar 的交叉轴坐标）；端点处跑段从 P0/Pn 沿轴向进入（首末段不可钉，与 Excalidraw 同）。
  3. **拖顶点 = 平移整根 bar**：仅垂直于 bar 的一个自由度有效（bar 过 Pi，横/竖坐标即 bar 位置）；沿 bar 方向拖动是 no-op（bar 范围由相邻几何决定）——与 Excalidraw fixed segment 单自由度一致。
  4. **对齐自动合并免费获得**：路径纯推导 + `dedup()`，把顶点拖到与邻居坐标对齐的瞬间零长段被吃掉、折线自动直化——**无需** Excalidraw 的共线检测 + fixed 索引重编号簿记（其 renormalization 是最容易出 bug 的部分）。
  5. **两点线行为零变化**：n=2 无中间顶点，直接落回现有 `elbow_polyline_offset`（含 `elbow_mid_offset`、bar 手柄）。
  6. **闭合折线**：沿用 #19 语义——切 elbow 时 `closed` 转 false + 末段显式连回首点保持视觉闭环；推导按开放链处理（首尾重合点由 dedup 收口）。
- **几何示例**（单测基准）：
  - V 形穿点 P0(0,0) P1(50,40) P2(100,0) → `(0,0)→(0,40)→(100,40)→(100,0)`（P1 恰是横 bar 中点；拖 P1 上下移整座桥）
  - 台阶 P0(0,0) P1(50,50) P2(100,100) → `(0,0)→(50,0)→(50,100)→(100,100)`（P1 是竖 bar 中点；拖 P1 左右移整根竖段）
  - 共线三点 → 推导退化成直线（无转折）
- **已接受的特性**（非缺陷，明确记录）：①顶点严格处于 bar「中点」仅在对称情形成立，一般情形顶点在 bar 上、另一维范围由相邻几何决定；②bar 取向取决于邻居对主导轴，拖动越过对角阈值时取向 90° 翻转（确定性规则固有，与两点线短轴翻转同性质，先观察是否需要滞回）。
- **实施要点**：
  - core `item.rs`：新 `elbow_vertex_polyline(pts, closed) -> Vec<(f32,f32)>`（顶点锚定 bar 推导 + dedup）**替换** `elbow_multi_polyline`（删除旧函数及其逐段 Z 测试）；`outline_points` / `contains_canvas_point` / 逐像素导出三消费点同步分流（两点 → `elbow_polyline_offset`，多顶点 → 新函数）
  - binary `transform_handles.rs`：多顶点 elbow **废止段中点拖拽加点**（拖 SegmentMid 不再插入顶点）；加顶点手势改显式动作（见 DP-A）
  - binary `drag.rs`：顶点拖拽语义不变（仍走 EditShapePoints 自由拖点）——路径随 `points` 重推导自然跟随，**无需新 DragState**；undo/redo、绑定联动零改动
  - 测试：三几何示例正交性/端点保持/顶点在 bar 上；对齐自动直化（拖齐后路径无转折）；closed 末段回首点；两点线回归（E1 全套）；polyline↔elbow 往返 points 不变
- **决策点**：
  - **DP-A｜加顶点手势**：段中点拖拽加点废止后，多顶点 elbow 如何加点？推荐「**双击段插入顶点**」（与 #4 删顶点 Alt+单击对称；顶点=bar 锚，插入后相邻两 bar 自动成立）；备选：保留 SegmentMid 手柄但仅响应单击/双击、拖拽忽略。
  - **DP-B｜取向翻转滞回**：是否给主导轴判定加滞回带（防拖动在对角线附近取向抖动）？推荐先不加，验收反馈再定。
- **不做**：Excalidraw `fixedSegments` 模型与 A* 避让路由（E2 仍留待未来，见 #16）；`elbow_mid_offset` 推广到多顶点（多顶点的用户意图由顶点位置表达）。
- **手工验收清单**：
  - [ ] 三顶点 V 形/台阶线：每个中间顶点拖动只平移一根横/竖段，相邻段自动跟随伸缩，不再出现逐段 Z 形碎乱
  - [ ] 把中间顶点拖到与邻居坐标对齐 → 折线自动变直（合并无需额外操作）
  - [ ] 沿 bar 方向拖顶点：路径不变（单自由度），手柄不跟走该轴属预期
  - [ ] 两点 elbow 线：bar 手柄拖动、`elbow_mid_offset`、undo 全部与 E1 一致（回归）
  - [ ] 闭合折线切 elbow：末段连回首点视觉闭环；顶点拖动后闭环保持
  - [ ] polyline ↔ elbow 来回切换：顶点位置无损往返
  - [ ] 保存 `.prz` 重开：走线由顶点重推导一致；旧档打开不报错

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
