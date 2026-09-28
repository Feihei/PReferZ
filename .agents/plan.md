# PReferZ Roadmap（plan.md）

> 本文档是 PReferZ 的**前瞻路线图**：只保留**待人工复验**、**未完成**与**远期**三类内容；
> **已交付内容的唯一归档在 [`CHANGELOG.md`](CHANGELOG.md)**（含每项的实现要点、测试与决策点）。
> 设计规格见 [`specs/`](specs/)，架构决策见 [`adr/`](adr/)。
>
> 工作流惯例：**规划文档先提交，实现拆独立 commit**（Conventional Commits）；质量门槛
> `cargo fmt --check` / `clippy -D warnings` / `cargo test --workspace` 全绿才算交付。
> **一条已交付项从本文件清空的条件**：质量门全绿 + commit 落库 + CHANGELOG 有对应小节。

**整体目标**：BeeRef 的 Rust 精神继承者——启动更快、包更小的极简参考图聚合桌面应用（egui + eframe），交互向 Excalidraw 对齐。

---

## 状态图例

- ✅ 已完成并已归档（→ CHANGELOG）
- 🔶 代码已交付，待人工验收
- ⏳ 计划中（未开始）

---

## 待人工复验（代码已交付，`cargo run` 确认观感）

> 自动化测试全绿 ≠ 观感正确。逐项 `cargo run` 过一遍；通过后本节删除该行、CHANGELOG 标注
> 复验日期。复验要点摘自各 CHANGELOG 小节，**验收清单全文见对应小节**。

| # | 条目 | 复验要点 | CHANGELOG |
|---|---|---|---|
| 1 | 样式面板批次（#1/#2/#3，`0aec50d`+`a4a464a`+`f8257d7`） | 5 色描边/填充调色板、填充 50% 默认透明、sloppiness 四档差异、文字对齐 + 手写体 | §样式面板批次 |
| 2 | 多边形节点增删 + 自动闭合（#4，`43576da`+`8ba4ac5`） | Alt+单击删顶点 / Alt+拖端点延伸 / 端点拖回起点自动闭合；重合点视作一点、拖开 >8px 自动恢复开放 | §多边形节点增删 |
| 3 | 流程图创建与导航（#7，`56dd3d4`+`8ba4ac5`+`c89448b`+`7f97f47`） | Ctrl+方向同源克隆 + 绑定箭头（同向自动上下分叉）、Alt+方向沿连接导航、箭头默认 elbow | §流程图 Ctrl+方向创建 / §分叉避让 / §elbow 曲线模式 |
| 4 | 编组 / 解组（#13，`4033726`） | Ctrl+G 编组后点击任一成员选整组、Ctrl+D 复制整组、删成员其余仍编组、Ctrl+Shift+G 解组、`.prz` 组关系往返 | §元素编组 / 解组 |
| 5 | 视口缩放套件（#15，`f509287`） | Shift+2 缩放到选中（无选区仅提示）、Shift+3 回 100%（视口中心不动） | §视口缩放套件 |
| 6 | 画框演示比例预设（#3，`796c0d5`） | 5 演示比例 + A4 竖/横套用后中心不变地变形、Ctrl+Z 一步还原 | §画框演示比例预设 |
| 7 | 图表粘贴（#8，`fafd262`） | Excel 框选两列 → Ctrl+V 弹柱状/折线选择；网格/轴/标签/负值零线浮动、移动缩放跟随、重开存档还在 | §两列数据粘贴成柱状/折线图 |
| 8 | mermaid 转流程图（#9，`edb6a6a`+`b171fc4`） | 分层布局 + 边标签跟随重路由 + 箭头族线型 + `&` 多分支 + 特殊形近似；解析错误 flash 行号且输入保留 | §mermaid 代码转流程图 |
| 9 | 自动保存（#5，`96cf8ac`） | 静置 30s 生成 `.prz.autosave` 且无 toast、标题脏标记仍在；重开弹恢复/忽略；设置面板开关 + 秒数生效 | §自动保存 |
| 10 | 默认风格总开关（`69a3b8a`+`1225a31`） | 全新 config 启动显示草绘（新建形状中档手绘 + 文字手写体）→ 切规整后重启仍是规整 → 右侧四选一微调仍生效 → mermaid / 流程图分叉随开关同档 → 旧 config 加载默认草绘 | §默认风格总开关 |
| 11 | egui 0.29 → 0.36 迁移手测（`54a9f07` 起） | GUI 全功能回归：字体渲染质量、`Shift+数字` 快捷键、滚轮缩放手感、侧栏动画。计划全文见 [`plans/egui-029-to-036-upgrade.md`](plans/egui-029-to-036-upgrade.md) §6/§10 | §依赖大版本升级 |
| 12 | rusqlite 0.40 迁移手测（`a72c508`） | 真实 `.prz` 往返（旧档打开 / 存后重开 / 附件 `sqlar` 完好）。计划全文见 [`plans/rusqlite-031-to-040-upgrade.md`](plans/rusqlite-031-to-040-upgrade.md) | §依赖大版本升级 |

---

## Excalidraw 打磨批次：未完成项

> 已交付的 1–16 / 19–23 号打磨项全部归档在 CHANGELOG（2026-09-03 ~ 09-28 各节），本节只列**剩余**。
> 决策点 L1–L20 已归档在 CHANGELOG §决策点归档。

| # | 打磨项 | 现状 / 剩余工作 | 关键约束 |
|---|---|---|---|
| 16 E2 | elbow 障碍避让路由 | ⏳ **留待未来**——E1（bar 可拖）已交付且人工验收通过。剩余为 Excalidraw `elbowArrow.ts` 的 grid + A\* 全量移植（`routeElbowArrow` / `calculateGrid` / `astar`），连线绕开节点。待「连线穿过节点」痛点真实出现再立期 | 届时单 `elbow_mid_offset` 字段不够：方案 X = points 存完整路由结果 + renormalize（Excalidraw 同构，改动面大）／方案 Y = 两点 + 用户固定段列表派生。障碍收集可复用 #7 `connected_flowchart_rects` 的连通子图 BFS 思路 |
| 17 C | mermaid 布局质量 | ⏳ 批次 A（解析）/ B（映射）已交付。剩余批次 C：`&` 展开后同层变宽，现「按出现顺序」排布易交叉 → 加**重心排序（barycenter）**减边交叉；边默认改 **elbow** 更贴 mermaid 正交观感 | 视批次 A/B 人工验收结果再定是否本轮做（见上表 #8） |
| 20 | RoughStyler 剩余缺口 | ⏳ 四批已交付且人工验收通过。剩余划出的项：zigzag / dots 填充（[ADR-0005](adr/0005-shape-styler-rough-seeded.md) 注明后续自移植）、圆角矩形 `_bezierTo` 平滑抖动、椭圆 `overlap` 收笔重叠段 | 均属 rough.js 已有能力自移植，零新依赖 |
| 21 DP-B | elbow 取向翻转滞回 | ⏳ **不加**（L9）——bar 取向过对角阈值时 90° 翻转属确定性规则固有行为。观察验收反馈再定是否引入滞回 | — |
| 18 | 相乘叠合模式（Multiply / 荧光马克笔） | ⏳ **先不做**（L19），评估已存档。重启前提：egui 0.36.2 无 per-shape blend（epaint 无 `BlendMode`、glow 固定预乘 alpha），屏幕实时真 multiply 是唯一硬点（需 `Shape::Callback` + GL 状态 hack）；导出侧需先重写为正向合成。分 M1（荧光色板 + 低透明填充，近似）/ M2（真 multiply） | 导出管线重写本身是独立大项（顺带解决矢量元素导出缺失），M2 排其后 |

---

## 功能性待办（尚未实施）

> 源自 `.issues`（本地目录，gitignore 排除）。#1/#2 已交付，#3 见上表，#4 已由 `Action::FitToScreen`
> 覆盖（union 全部 item AABB），#5 见上表。**当前无未实施项。**

---

## 远期 / 未决

> 详细记录在本地 `.issues/issues.md`（gitignore 排除、不入库）。

- [ ] ttf 字体体积优化（内嵌字体 deflate 压缩已做，全量子集是否够用待评估）
- [ ] 放弃 `.bee` 兼容后可做的架构简化盘点（`PrzFile` 命名与 `prz.rs` 路径已改，schema 简化空间待盘）
- [ ] 视口裁剪已有，LOD 缩略图（Phase 5+）待评估
