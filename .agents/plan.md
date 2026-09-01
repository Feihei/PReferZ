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

---

## 人工验收（GUI）

> 自动化测试全绿 ≠ 观感正确，需 Feihei 跑 `cargo run` 确认。

**2026-09-01 全部通过**：旋转图片裁剪框跟随旋转 / Phase F 手绘风七项 / 文本三项反馈
（`ed538cf`+`d6177b8`+`c05d1ae`）/ 绑定文本不可独立选中（`8eb996a`）/ 键鼠改绑 /
配置持久化 / Slide 演示（E4）。

新增待验收：

- [ ] **裁剪遮罩闪烁黑影**：`bace60d` 仅跳过零宽梯形未根治（旋转图仍闪）；`74a5869` 改为单凸多边形压暗整图 + 亮区重绘，请复验旋转图进入裁剪、拖动裁剪框时不再闪黑

---

## 下一步：对齐 Excalidraw（G / I / H）

**状态**：⏳ 规划定稿待拍板（2026-09-01）

三个方向，实施顺序 **G → I → H**（侧栏属性项依赖 I 定型的字段集合，先做 H 会返工；G 完全独立可先行）：

- [ ] **Phase G — 明暗两套样式主题**（小，独立）：`ThemeMode` 持久化 + palette 模块集中管理主题化颜色；UI chrome（egui Visuals）+ 画布语义（底色与新建元素默认色随主题翻转）
- [ ] **Phase I — 图形元素类型统一**（大，动数据模型）：`CurveType { Straight, Curved }`（Catmull-Rom 推广出开曲线版本）；不规则多边形 = 闭合 Polyline 不新增类型；`ArrowHeadStyle` 扩展（Arrow/Dot）；矩形族 roundness；`.prz` serde default 迁移
- [ ] **Phase H — 选中弹出属性侧栏**（中）：右侧 SidePanel 按 ItemKind 分节（Shape/Text/Pixmap/Frame）；改动全走 undo 栈，滑块连续修改合并命令；Text 节消费已预留的 `background` 字段
- [ ] **Phase K — 默认快捷键对齐 Excalidraw**（小，可与任一阶段并行）：以 Excalidraw 官方键位为基准调整 `default_map()` 出厂默认；依据见 [ADR-0007](adr/0007-keymap-no-customization.md)（不做用户自定义，keymap 派发架构保留）

### 决策点（待拍板）

| # | 问题 | 选项 | 倾向 |
|---|---|---|---|
| D1 | 主题三态 | 仅 Light/Dark vs 增 `Auto`（跟随系统） | 字段预留 `Auto`，第一版只做手动切换 |
| D2 | 画布底色 | 跟主题走 vs 独立可设 | 跟主题走 |
| D3 | 多选属性面板 | 显示交集 vs 禁用面板 | 显示交集可批量改（Excalidraw 同款） |
| D4 | elbow 折线 | 本轮做 vs 排后 | 排后 |
| D5 | hachure 填充 | 本轮做 vs 排后 | 排后，纯色先统一数据模型 |
| D6 | keymap 设置面板去留 | 保留（已交付可用）vs Phase K 顺手移除入口 | 移除入口，`Action`/`Keymap` 派发架构保留 |

---

## 远期 / 未决

> 详细记录在本地 `.issues/issues.md`（gitignore 排除、不入库）。

- [ ] ttf 字体体积优化（assets/simhei.ttf 全量嵌入）
- [ ] 放弃 `.bee` 兼容后可做的架构简化盘点
- [ ] 视口裁剪已有，LOD 缩略图（Phase 5+）待评估
