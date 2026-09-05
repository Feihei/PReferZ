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

新增：**多元素对齐 / 分布（plan #6，2026-09-05 `cad908b`）** —— 待 `cargo run` 复验：
选中 ≥2 项看右侧属性栏是否出现「对齐」节（6 图标按钮 + 分布 4 按钮）；
框选 3 个以上错位元素试「横向等距 / 横向等心」，确认首尾不动、中间均分；
按 `Ctrl+Z` 应一步撤回；右键菜单「排列」内的对齐/分布嵌套子菜单能否正常展开。

---

## 下一步：Excalidraw 打磨批次（Phase L 候选）

> **状态**：G / I / H / K 已全部交付（见 [CHANGELOG](CHANGELOG.md) §Phase G/I/H/K + §手工验收反馈批次）；
> 决策点 D1–D6 / I1–I4 已归档（见 [CHANGELOG §决策点归档](CHANGELOG.md)）。
> 本批次为对齐 Excalidraw 的观感/交互打磨项，**尚未实施**，按下方编号逐步评审、集体拍板后开工。
> 实施顺序建议：**5/6/13 先（编辑器基础：吸附 / 对齐分布 / 编组解组）→ 1/2/3（样式面板补全）→ 4（多边形编辑）→ 7（连接符，依赖 5）→ 8/9（数据/图表，较重）→ 10（徒手，独立）**；**11/12 为独立快赢（拖拽修饰键 / 缩放钳制默认值），可随时插入，不阻塞其他项**。

| # | 打磨项 | 一句话方案 | 关键依赖 / 决策点 |
|---|---|---|---|
| 1 | 绑定文字对齐 + 字号 + 字体切换 | Text 加 H/V 对齐枚举 + 字体族（黑体/手写），侧栏加对齐分段与字号滑块 | 手写字体：真实嵌入 TTF vs 伪手写渲染（体积）；垂直对齐是否仅绑定文字 |
| 2 | 调色板对齐 + 填充半透明 | `FILL_PICKS` 改为与描边同 5 色（黑红绿蓝橙）；填充加不透明度滑块（RGBA alpha） | 填充默认透明度取值；top picks 描边/填充共用同组 5 色 |
| 3 | stroke width / sloppiness / edges 倒角 | `rough: bool` → `Sloppiness{Architect,Artist,Cartoonist}` 三档；edges（sharp/round）限定矩形族 | sloppiness 档位；edges 是否扩展到非矩形（倾向否） |
| 4 | 多边形节点增删 + 首尾重合自动闭合 | 顶点删除（Alt+拖出）+ 端点追加（拖末端点延伸）；首尾距 < 阈值自动 `closed` | 删除手势；阈值复用 `POLYLINE_CLOSE_DISTANCE` |
| 5 | 直线/箭头端点吸附图形边缘 | 拖端点邻近 Shape 轮廓吸附 + 绑定模型（端点随形状移动） | 吸附阈值；绑定存储位置；与 #7 共用 |
| 6 | ✅ 多元素对齐、分布（2026-09-05 `cad908b`） | `arrange.rs` 增 `plan_align`（6 向）+ `plan_distribute`（等距/等心 × 横/纵）；属性栏「对齐」节 + 右键菜单 | 已拍板并交付：两种分布都做；参考系=选区包围盒；UI=属性栏+右键菜单 |
| 7 | Ctrl+箭头 添加连接符 + 下一元素 | 选中 Shape + Ctrl+方向 → 生成绑定 Arrow + 新 Shape（流程图） | 新元素类型/间距；复用 #5 绑定 |
| 8 | 两列数据粘贴成柱状/折线图 | 剪贴板 2 列 TSV/CSV → 生成 Chart item（柱状/折线） | 图表用新 ItemKind+ChartStyler vs Pixmap 位图；单/多系列 |
| 9 | mermaid 代码转图表 | mermaid 子集 → nodes+edges（复用 #5/#6/#7） | 解析器：受限自研 Rust（零依赖，倾向）vs WASM mermaid |
| 10 | 徒手绘制（7 快捷键）墨迹模仿 | 新增 `Tool::Freehand` + `Num7`；`ItemKind::Freedraw` 平滑墨迹 | 压感（egui 无，倾向恒定宽+抖动）；点抽稀（RDP） |
| 11 | ✅ 选中拖动修饰键 + Ctrl+D 原位复制 | Ctrl+拖动=复制并移动副本；Shift+拖动=水平/垂直约束（PowerPoint 风）；Ctrl+D=原位复制 | 已交付（2026-09-03）：按下即建副本；约束基准=画布轴（视口无旋转，与屏幕轴同向）；Ctrl+D 偏移 10px |
| 12 | ✅ 最大/最小缩放限制 | 默认 100%，最小 10%（0.1x），最大 1000%（10x）；`min_zoom`/`max_zoom` 改默认值 | 已交付（2026-09-03）：默认值 0.1/10.0；`.prz` 元数据越界时 clamp |
| 13 | 元素编组 / 解组 | `Item.group_id: Option<Uuid>`（单组，持久化）；点击组成员全选、移动整体；`Ctrl+G` 编组 / `Ctrl+Shift+G` 解组 | 单组 vs 多组嵌套；点击命中即选整组；编组前先建选区 |

### 各项细节与决策点

1. **绑定文字对齐 + 字号 + 字体切换**
- 现状：`ItemKind::Text`（TextStyle）已有 `font_size`/`color`/`background`，但**无对齐字段**；绑定文字（容器封闭 Shape）由渲染层自动居中，无 H/V 控制；字体仅内嵌 simhei（黑体）一种。
- 方案：core 加 `TextAlignH{Left,Center,Right}` + `TextAlignV{Top,Middle,Bottom}`（`#[serde(default)]=Center`）；字体族 `FontFamily{Handwriting,Normal}`——手写体需新增第二种嵌入 TTF（体积评估，见 `.issues`）或伪手写渲染（复用 RoughStyler 思路）。侧栏 Text 节加对齐分段控件 + 字号滑块（已有）+ 字体族切换。
- 决策点：手写体实现方式（真实 TTF vs 伪手写）；字号范围；垂直对齐是否仅绑定文字（自由文字默认 top-left）。

2. **调色板对齐 + 填充半透明**
- 现状：`STROKE_PICKS` 已是 `["#1e1e1e","#e03131","#2f9e44","#1971c2","#f08c00"]`（黑红绿蓝橙，恰好对齐 Excalidraw）；`FILL_PICKS` 是 4 个粉彩色，**与 5 标准色不对齐**。
- 方案：`FILL_PICKS` 改为与 `STROKE_PICKS` 同 5 色；填充增**不透明度滑块**（fill 已是 RGBA，暴露 alpha 0–100%，默认取 Excalidraw 观感值）。侧栏填充节走 `apply_continuous` + `FillState`。
- 决策点：填充默认透明度；top picks 描边/填充共用同组 5 色（Excalidraw 是共用）。

3. **stroke width / sloppiness / edges 倒角**
- 现状：`StrokeStyle.width` 已有（0.5–12 连续）；`rough: bool` 仅开/关；矩形 `roundness` 已有（仅矩形族）。**缺 sloppiness 多档**与通用 edges 倒角。
- 方案：`rough: bool` → `Sloppiness{Architect,Artist,Cartoonist}`（3 档映射到 RoughStyler 抖动幅度；`#[serde(default)]`=Architect 即原 false 语义），`SetRough` 改 `SetSloppiness`；edges（sharp/round）分段控件，仅矩形显示。
- 决策点：sloppiness 档位（对齐 Excalidraw 3 档）；edges 是否扩展非矩形（倾向否）。

4. **多边形节点增删 + 首尾重合自动闭合**
- 现状：反馈 #5 已做段中点拖拽**插入**顶点（SegmentMid）；但删顶点、端点追加、首尾重合自动闭合缺失。
- 方案：transform_handles 增顶点删除（Alt+拖出画布即删，≥3 点保持）；端点追加（拖末端点超阈值即 append）；首尾距 < `POLYLINE_CLOSE_DISTANCE`(8px) 自动 `closed=true`（保留手动 closed 选项）。core 加 `DeleteVertex`/`AppendVertex` 命令。
- 决策点：删除手势（Alt+click vs 拖出）；阈值复用现有常量。

5. **直线/箭头端点吸附图形边缘**
- 现状：LineEndpoint 拖拽无吸附、未绑定形状。
- 方案：拖端点邻近 Shape 轮廓（点到轮廓距离 < 阈值）吸附到最近点，core 记 `binding: Option<(ItemId, side)>`；形状移动时联动更新绑定端点（Scene 遍历）。Excalidraw 语义：箭头绑 shape，移动 shape 箭头跟随。
- 决策点：吸附阈值；绑定存储（端点元数据 vs 独立表）；直线是否也可绑（倾向是）。

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

13. **元素编组 / 解组（Group / Ungroup）**
- 现状：`Scene.items` 为扁平列表；`selection` 为 `ItemId` 集合；**无 group 概念**（grep `group/parent/children` 仅命中无关项）。点击命中单个元素即选单个；移动走 `DragState::MoveItems` 对 `selection` 整集平移。
- 方案：
  - core 加 `Item.group_id: Option<Uuid>`（`#[serde(default)]=None`，向后兼容旧 .prz）；`Scene` 增 `group(&[ItemId])`（给选中项赋同一新 `Uuid`）、`ungroup(&[ItemId])`（清空）、`group_members_of(item_id)`（同组其余项）。
  - 点击命中：若命中项 `group_id.is_some()`，选区扩展为**整组**（含命中项），使 `MoveItems` 自然整体平移；`transform_handles` 仅渲染组包围盒（不逐成员画手柄，避免杂乱）。
  - 动作：`Action::Group` 绑 `Ctrl+G`、`Action::Ungroup` 绑 `Ctrl+Shift+G`（Excalidraw 同款）；经新命令 `SetGroup(ids, Option<Uuid>, Option<Uuid>)` 入 undo 栈（编组=赋新 id，解组=置 None，组内单元素删/拖出如何处置为决策点）。
  - 序列化：`.prz` items 多一列 `group_id`（TEXT/UUID 字符串，可空）；`BeeFile` 写/读补该列；旧文件缺该列按 `None` 加载（`#[serde(default)]` 已覆盖）。
- 决策点：
  - **单组 vs 多组嵌套**：Excalidraw 允许元素属多组（嵌套），但实现复杂；**倾向单组**（一个元素至多一个 group_id），覆盖绝大多数用例，编组前需先 `ungroup` 旧组。
  - **点击整组 vs 点单成员**：倾向"点击组内任一成员即选整组"（Excalidraw 默认），双击穿透选单成员可作为排后增强。
  - **组内元素的删除/拖出**：移动整组不变；若删/拖出某成员，默认保留其余成员的 group_id（同组继续存在）；彻底解组需显式 `Ctrl+Shift+G`。
  - **与 11/6 的关系**：编组后 `Ctrl+D` 原位复制、对齐/分布均作用于整组（选区=整组），无需特殊处理；建议 13 排在 6 之后实施，使多元素操作统一以组为粒度。

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
