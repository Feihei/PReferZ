# PReferZ Roadmap（plan.md）

> 本文档是 PReferZ 的**前瞻路线图**：顶部是已交付阶段一览（索引），主体是待验收项与下一步计划。
> 各阶段**完整交付清单**见 [`CHANGELOG.md`](CHANGELOG.md)；设计规格见 [`specs/`](specs/)，架构决策见 [`adr/`](adr/)，阶段实施计划见 [`plans/`](plans/)。
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
| Phase F | 🔶 | 手绘风描边 RoughStyler（rough.js 同款，确定性种子） | [CHANGELOG.md](CHANGELOG.md) §Phase F |
| Phase 6 收尾 | ✅ | 键鼠映射可配置（keymap + 派发层 + 设置面板） | [CHANGELOG.md](CHANGELOG.md) §Phase 6 |
| 体验修复批次 | ✅ | 文字无背景 / 手绘椭圆光滑曲线 / Enter 编辑文字 / 绑定文本不可独立选中 | [CHANGELOG.md](CHANGELOG.md) §修复批次 |

---

## 待人工验收（GUI）

> 自动化测试全绿 ≠ 观感正确，以下各项需 Feihei 跑 `cargo run` 确认。

- [ ] **旋转图片裁剪框跟随旋转**（代码已修 `d821f37`+`d737f9c`，详见本地 `.issues/`）
- [ ] **Phase F 手绘风观感**（原 rough-style-plan §6 七项，文件已归档）：
  - [ ] 工具栏切矩形工具 → 勾「手绘」→ 拖拽画矩形，观感接近 Excalidraw
  - [ ] 椭圆/菱形/直线/箭头同样生效；**手绘椭圆应为光滑曲线**（`d6177b8` 后无折角）
  - [ ] 缩放画布：抖动随缩放同步放大，形状不跳变（确定性）
  - [ ] 选中已有形状 → 样式面板切换手绘，Ctrl+Z 可撤销
  - [ ] Ctrl+S 保存 → 重开 → 手绘形状与关闭前完全一致
  - [ ] 关闭手绘：回到精确几何描边
  - [ ] F5 演示模式下手绘形状渲染一致
- [ ] **文本三项反馈**（2026-08-31）：文字默认无背景（`ed538cf`）/ 手绘椭圆光滑（`d6177b8`）/ Enter 编辑选中项文字（`c05d1ae`）
- [ ] **绑定文本不可独立选中/拖动**（`8eb996a`）：点文字区域选中容器图形，拖图形文字跟随；框选只中容器
- [ ] **键鼠改绑**：设置面板改键 → 冲突检测 → 持久化重启生效
- [ ] **配置持久化**：`~/.preferz/config.json` 与 `recent.json`
- [ ] **Slide 演示（E4）**：F5 进入，方向键翻页，Frame 编号角标可编辑

---

## 下一步：对齐 Excalidraw（G / I / H）

**状态**：⏳ 规划定稿待拍板（2026-09-01，[详细计划](plans/2026-09-01-excalidraw-alignment.md)）

三个方向，实施顺序 **G → I → H**（侧栏属性项依赖 I 定型的字段集合，先做 H 会返工；G 完全独立可先行）：

- [ ] **Phase G — 明暗两套样式主题**（小，独立）：`ThemeMode` 持久化 + palette 模块集中管理主题化颜色；UI chrome（egui Visuals）+ 画布语义（底色与新建元素默认色随主题翻转）
- [ ] **Phase I — 图形元素类型统一**（大，动数据模型）：`CurveType { Straight, Curved }`（Catmull-Rom 推广出开曲线版本）；不规则多边形 = 闭合 Polyline 不新增类型；`ArrowHeadStyle` 扩展（Arrow/Dot）；矩形族 roundness；`.prz` serde default 迁移
- [ ] **Phase H — 选中弹出属性侧栏**（中）：右侧 SidePanel 按 ItemKind 分节（Shape/Text/Pixmap/Frame）；改动全走 undo 栈，滑块连续修改合并命令；Text 节消费已预留的 `background` 字段

### 决策点（待拍板）

| # | 问题 | 选项 | 倾向 |
|---|---|---|---|
| D1 | 主题三态 | 仅 Light/Dark vs 增 `Auto`（跟随系统） | 字段预留 `Auto`，第一版只做手动切换 |
| D2 | 画布底色 | 跟主题走 vs 独立可设 | 跟主题走 |
| D3 | 多选属性面板 | 显示交集 vs 禁用面板 | 显示交集可批量改（Excalidraw 同款） |
| D4 | elbow 折线 | 本轮做 vs 排后 | 排后 |
| D5 | hachure 填充 | 本轮做 vs 排后 | 排后，纯色先统一数据模型 |

---

## 远期 / 未决

> 详细记录在本地 `.issues/issues.md`（gitignore 排除、不入库）。

- [ ] ttf 字体体积优化（assets/simhei.ttf 全量嵌入）
- [ ] 放弃 `.bee` 兼容后可做的架构简化盘点
- [ ] 视口裁剪已有，LOD 缩略图（Phase 5+）待评估
