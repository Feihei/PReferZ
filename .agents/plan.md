# PReferZ Roadmap（plan.md）

> 本文档是 PReferZ 的**前瞻路线图**：只保留**未完成**与**远期**两类内容；
> **已交付内容的唯一归档在 [`CHANGELOG.md`](CHANGELOG.md)**（含每项的实现要点、测试、复验与
> 决策点）。设计规格见 [`specs/`](specs/)，架构决策见 [`adr/`](adr/)。
>
> 工作流惯例：**规划文档先提交，实现拆独立 commit**（Conventional Commits）；质量门槛
> `cargo fmt --check` / `clippy -D warnings` / `cargo test --workspace` 全绿**且**人工 `cargo run`
> 确认观感，才算交付。**一条已交付项从本文件清空的条件**：质量门全绿 + commit 落库 +
> CHANGELOG 有对应小节（并标注复验日期）。`plan.md` 的「待人工复验」清单已于 2026-09-28 全部
> 通过并清空（见 CHANGELOG §人工复验清账）。

**整体目标**：BeeRef 的 Rust 精神继承者——启动更快、包更小的极简参考图聚合桌面应用（egui + eframe），交互向 Excalidraw 对齐。

---

## 状态图例

- ✅ 已完成并已归档（→ CHANGELOG）
- 🔶 代码已交付，待人工验收
- ⏳ 计划中（未开始）

---

## Excalidraw 打磨批次：未完成项

> 已交付的 1–23 号打磨项全部归档在 CHANGELOG（2026-09-03 ~ 09-28 各节），本节只列**剩余**。
> 决策点 L1–L22 已归档在 CHANGELOG §决策点归档（L21 曾取消 elbow 自动避障路由，
> 后由 #24「完整对齐」拍板**推翻**、已于 2026-10-04 交付归档；L9 曾取消取向滞回，
> 2026-10-02 由 L22 **推翻**落地）。

| # | 打磨项 | 现状 / 剩余工作 | 关键约束 |
|---|---|---|---|
| 17 C | mermaid 布局质量 | 🔶 **代码已交付**（2026-10-04）：`layout_flowchart` 层内加**重心排序（barycenter）**减 `&` 展开同层交叉；边默认改 `Item::new_elbow`（映射 `ShapeType::Elbow`，绑定期走 #24 A\* 正交路由）。待人工 `cargo run` 验收后清空归档 | 批次 A/B 已于 2026-09-28 复验通过 |
| 20 | RoughStyler 剩余缺口 | 🔶 **代码已交付**（2026-10-05）：zigzag / dots 填充（rough.js 自移植，`FillStyle` 新增两态）+ 椭圆 `overlap` 收笔重叠段（开放点环重构）；「圆角矩形 `_bezierTo`」经核对已随 2026-10-01 Segmented 路线交付（见 CHANGELOG §plan #20 说明）。待人工 `cargo run` 验收后清空归档 | dots 以抖动实心圆近似逐点 rough 椭圆（1 shape/点）；zigzag 起点溢出边界为 rough.js 同款行为 |
| 18 | 相乘叠合模式（Multiply / 荧光马克笔） | ⏳ **先不做**（L19），评估已存档。重启前提：egui 0.36.2 无 per-shape blend（epaint 无 `BlendMode`、glow 固定预乘 alpha），屏幕实时真 multiply 是唯一硬点（需 `Shape::Callback` + GL 状态 hack）；导出侧需先重写为正向合成。分 M1（荧光色板 + 低透明填充，近似）/ M2（真 multiply） | 导出管线重写本身是独立大项（顺带解决矢量元素导出缺失），M2 排其后 |

---

## 功能性待办（尚未实施）

> 源自 `.issues`（本地目录，gitignore 排除）。#1–#5 均已交付（#4 由 `Action::FitToScreen` 覆盖，
> union 全部 item AABB，等价"显示所有元素"）。

### #6 元素超链接（Element Link）

参考 Excalidraw 的 element link（元素根级 `link: string | null` + hover 右上角图标点击打开），
给 item 增加超链接能力：**web URL（http/https）** 与 **本地 `.prz` 文件**。决策点 H1–H4
（2026-10-06 拍板，均为倾向列）：

| # | 决策 | 拍板 |
|---|---|---|
| H1 | 数据模型 | `Item` 顶层新增 `link: Option<String>`（`#[serde(default)]`）；`.prz` items 表加 `link` 列（照 `group_id` 先例：`pragma_table_info` 检测 + `ALTER TABLE` 就地迁移，不动 USER_VERSION） |
| H2 | 本地 .prz 打开方式 | **spawn 新窗口**（自身 exe + 路径参数，照 `spawn_help_instance` 先例）；`main.rs` 增加位置参数解析为待打开路径 |
| H3 | 路径存储 | 存用户输入**原文**；打开时相对路径优先（相对当前 `.prz` 所在目录），不存在再按绝对路径解析；无当前文件时按工作目录 |
| H4 | 编辑入口 | 精简版 + Ctrl+K：① props 面板通用 section（恰选中 1 项时，链接输入框 + 清除按钮）；② 右键菜单 Add/Edit link + Remove link；③ keymap 新增 `Action::EditLink`（Ctrl+K，聚焦链接输入框） |

> 🔶 **代码已交付（2026-10-06）**：A–D 全部落地——`Item.link` + `SetLink` 命令 +
> `sanitize_link`/`classify_link`（含协议注入拒绝，headless 测试 5 个）；
> `prz` v4 加 `link` 列（group_id 同款迁移，roundtrip/迁移测试覆盖）；
> `main.rs` 位置参数启动加载（复用 pending_open_recent 流程）；hover badge（↗ U+2197，
> 内嵌字体 cmap 已核验）+ 按下守卫不启拖拽 + props 链接节（Enter/失焦提交，持焦屏蔽
> 快捷键派发）+ 右键菜单两入口 + Ctrl+K 聚焦。质量门 fmt/clippy/test 全绿
> （357 测试）。web 打开零新依赖（explorer/open/xdg-open 三平台 cfg）。
> 待人工 `cargo run` 验收后清空归档 CHANGELOG。

实施拆分（checkbox 跟踪）：

- [x] A core：`Item.link` 字段 + `SetLink` 命令 + 链接分类/路径解析纯函数（headless 测试）
- [x] B fileio：save/load 加 `link` 列 + 旧库迁移 + roundtrip/迁移测试
- [x] C 启动：`main.rs` 位置参数 → 启动时加载指定 `.prz`
- [x] D UI：hover badge 打开 + props 面板 section + 右键菜单 + Ctrl+K + i18n 词条
- [ ] E 人工 `cargo run` 复验后归档 CHANGELOG

---

## 远期 / 未决

> 详细记录在本地 `.issues/issues.md`（gitignore 排除、不入库）。

- [ ] ttf 字体体积优化（内嵌字体 deflate 压缩已做，全量子集是否够用待评估）
- [ ] 放弃 `.bee` 兼容后可做的架构简化盘点（`PrzFile` 命名与 `prz.rs` 路径已改，schema 简化空间待盘）
- [ ] 视口裁剪已有，LOD 缩略图（Phase 5+）待评估
