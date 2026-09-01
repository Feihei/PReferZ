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
- `bace60d` `fix(ui)`: 裁剪遮罩跳过亚像素退化梯形，消除旋转图片裁剪时贴边闪烁黑影
