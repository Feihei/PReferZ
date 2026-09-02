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

新增待验收：（暂无）

---

## 下一步：对齐 Excalidraw（G / I / H）

**状态**：✅ D1–D6 已拍板（2026-09-01）；**Phase G 已交付**（`8c0bac4`）；**Phase I 已拍板待实施**（设计文档 [specs/phase-i-shape-unification.md](specs/phase-i-shape-unification.md)）；H/K 待实施。

三个方向，实施顺序 **G → I → H**（侧栏属性项依赖 I 定型的字段集合，先做 H 会返工；G 完全独立可先行）：

- [x] **Phase G — 明暗两套样式主题**（小，独立）：`ThemeMode` 持久化 + palette 模块集中管理主题化颜色；UI chrome（egui Visuals）+ 画布语义（底色与新建元素默认色随主题翻转）。D6 键鼠改绑设置入口已移除（架构保留），见 CHANGELOG §Phase G
- [x] **Phase I — 图形元素类型统一**（大，动数据模型）：`CurveType { Straight, Curved }`（Catmull-Rom 推广出开曲线版本）；不规则多边形 = 闭合 Polyline（`closed: bool`，不新增类型）；`ArrowHeadStyle` 扩展（Arrow/Dot）；矩形族 roundness；`.prz` 仅 `#[serde(default)]` 迁移（`USER_VERSION` 不变）。**已拍板**（2026-09-01），设计文档 [specs/phase-i-shape-unification.md](specs/phase-i-shape-unification.md)。**子阶段 A（数据模型）已交付**（`ddc6209`），B/C/D 待实施。

  **Phase I 决策点（2026-09-01 拍板）**

  | # | 问题 | 结论 |
  |---|---|---|
  | I1 | 不规则多边形是否纳入 Phase I | ✅ 纳入：新增「多边形」工具（点击加点、双击/回车闭合），存为 closed Polyline |
  | I2 | 曲线切换入口 | ✅ 样式面板「直线/曲线」分段控件（选中线性对象，走 undo），不新增工具按钮 |
  | I3 | ArrowHeadStyle 范围 | ✅ 仅 Arrow + Dot |
  | I4 | .prz 迁移策略 | ✅ 新字段 `#[serde(default)]`，`USER_VERSION` 不变 |
  | — | 圆角范围 | 仅矩形族（按计划） |
  | — | 排后项 | D4 elbow 折线 / D5 hachure 填充 不纳入 Phase I |
- [x] **Phase H — 选中弹出属性侧栏**（中）：右侧 SidePanel 按 ItemKind 分节（Shape/Text/Pixmap/Frame）；改动全走 undo 栈，滑块连续修改合并命令；Text 节消费已预留的 `background` 字段。已交付（`2026-09-02`），见 CHANGELOG §Phase H
- [ ] **Phase K — 默认快捷键对齐 Excalidraw**（小，可与任一阶段并行）：以 Excalidraw 官方键位为基准调整 `default_map()` 出厂默认；依据见 [ADR-0007](adr/0007-keymap-no-customization.md)（不做用户自定义，keymap 派发架构保留）。**D6 设置入口移除已完成**，仅剩默认键位对齐待做

### 决策点（2026-09-01 已拍板）

> 全部按「倾向」列拍板：D1 预留 `Auto` 先做手动切换 / D2 画布底色跟主题走 / D3 多选显示交集可批量改 / D4 elbow 排后 / D5 hachure 排后 / D6 移除键鼠改绑设置入口（架构保留）。

| # | 问题 | 选项 | 倾向 | 结论 |
|---|---|---|---|---|
| D1 | 主题三态 | 仅 Light/Dark vs 增 `Auto`（跟随系统） | 字段预留 `Auto`，第一版只做手动切换 | ✅ 按倾向 |
| D2 | 画布底色 | 跟主题走 vs 独立可设 | 跟主题走 | ✅ 按倾向 |
| D3 | 多选属性面板 | 显示交集 vs 禁用面板 | 显示交集可批量改（Excalidraw 同款） | ✅ 按倾向 |
| D4 | elbow 折线 | 本轮做 vs 排后 | 排后 | ✅ 排后 |
| D5 | hachure 填充 | 本轮做 vs 排后 | 排后，纯色先统一数据模型 | ✅ 排后 |
| D6 | keymap 设置面板去留 | 保留（已交付可用）vs Phase K 顺手移除入口 | 移除入口，`Action`/`Keymap` 派发架构保留 | ✅ 移除入口（已实施） |

---

## 远期 / 未决

> 详细记录在本地 `.issues/issues.md`（gitignore 排除、不入库）。

- [ ] ttf 字体体积优化（assets/simhei.ttf 全量嵌入）
- [ ] 放弃 `.bee` 兼容后可做的架构简化盘点
- [ ] 视口裁剪已有，LOD 缩略图（Phase 5+）待评估
