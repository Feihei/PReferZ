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
> 后由 #24「完整对齐」拍板**推翻**，见下方 #24 专节；L9 曾取消取向滞回，
> 2026-10-02 由 L22 **推翻**落地）。

| # | 打磨项 | 现状 / 剩余工作 | 关键约束 |
|---|---|---|---|
| 17 C | mermaid 布局质量 | ⏳ 批次 A（解析）/ B（映射）已交付。剩余批次 C：`&` 展开后同层变宽，现「按出现顺序」排布易交叉 → 加**重心排序（barycenter）**减边交叉；边默认改 **elbow** 更贴 mermaid 正交观感 | 批次 A/B 已于 2026-09-28 复验通过；是否做批次 C 待定（#24 落地后映射目标为 `ShapeType::Elbow`） |
| 20 | RoughStyler 剩余缺口 | ⏳ 四批已交付且人工验收通过。剩余划出的项：zigzag / dots 填充（[ADR-0005](adr/0005-shape-styler-rough-seeded.md) 注明后续自移植）、圆角矩形 `_bezierTo` 平滑抖动、椭圆 `overlap` 收笔重叠段 | 均属 rough.js 已有能力自移植，零新依赖 |
| 21 DP-B | elbow 取向翻转滞回 | 🔶 **已落地**（2026-10-02，⛔ 推翻 L9，归档 L22 / CHANGELOG「elbow 取向稳定化」节）：滞回（1.3× 稳定带）+ **绑定锚点驱动取向**（锚点所在边法线决定首/末段走向，与 Δ 关系解耦）。#24 阶段 C 的 `elbow_route` 直接复用其取向输入语义 | `elbow_axis: Option<ElbowAxis>`（serde default None = 旧档推断，行为不变）；取向变更随 `EditShapePoints` 同条 undo |
| 18 | 相乘叠合模式（Multiply / 荧光马克笔） | ⏳ **先不做**（L19），评估已存档。重启前提：egui 0.36.2 无 per-shape blend（epaint 无 `BlendMode`、glow 固定预乘 alpha），屏幕实时真 multiply 是唯一硬点（需 `Shape::Callback` + GL 状态 hack）；导出侧需先重写为正向合成。分 M1（荧光色板 + 低透明填充，近似）/ M2（真 multiply） | 导出管线重写本身是独立大项（顺带解决矢量元素导出缺失），M2 排其后 |
| 24 | elbow 连接器独立类型化 + A\* 自动路由（对齐 Excalidraw） | 🔶 阶段 A（类型拆分+迁移）/ B（Elbow 工具+烘焙）已交付待人工验收；C（A\* 避障）/ D（fixedSegments）未开始。详见下方 #24 专节 | **推翻 L21**；取代 #21 多顶点机制与 #23 中 elbow 专属部分；工具栏 elbow 图标 `↴` 需人工确认内嵌字体有字形 |

---

## #24 elbow 连接器独立类型化 + A\* 自动路由（对齐 Excalidraw）

- 拍板日期：2026-09-28；状态：🔶 阶段 A/B/C/D 代码已交付待人工验收（A/B：`8724e05` /
  `749a4a9` 2026-09-29；C/D + DP-5/DP-6 拍板：2026-10-02），见下方「阶段 C/D 实现」
- 关系：**推翻 L21**（原取消 elbow 自动避障路由）；#21 多顶点「顶点锚定 bar」模型与 #23 倒角机制中 elbow 专属部分被本项**取代**（CHANGELOG 保留历史归档）；17 C mermaid「边默认改 elbow」直接受益（映射目标改为新类型）。
- **阶段 C/D 已实现**（2026-10-02，DP-5/DP-6 同日拍板）：core `routing.rs` 非均匀
  网格 A\*（两端点/障碍边线交点 + 转弯罚 m³ + 禁反向 + 首末步 heading 约束）+
  `fixed_segments` 坐标锚定缝合 + 确定性回退；`Scene::elbow_route_local` 为唯一
  派生入口（命中注入 / 渲染 / 手柄 / 导出同源）；`Handle::ElbowSegment` 拖任意
  中间段即固定（原 bar 偏移拖拽移除，`elbow_mid_offset` 保留为未绑定档语义）。
  实现要点与测试见 CHANGELOG「elbow A\* 避障路由 + 固定段」节。
- **阶段 C 前置过渡已落地**（2026-10-02，#21 DP-B / L22）：`ElbowAxis` 取向存储 +
  滞回解析（`elbow_axis_hysteresis`）+ 绑定锚点驱动（`elbow_axis_from_anchor`）——
  即 `elbow_route(start, end, fixed_segments, obstacles)` 中「起点出发方向 / 终点
  进入方向」约束的语义来源，阶段 C 换 A\* 时取向输入直接平移，锚点法线继续作为
  网格路由的初始方向约束（A\* 本身不解决对角翻转，仍需该状态）。

### 动机（2026-09-28 调研结论）

elbow 现挂在 `CurveType::Elbow`（`ShapeType::Polyline` 之上），与 Straight/Curved 的本质差异：后两者顶点即自由数据，elbow 是**正交性不变量**。错位导致：

- **双推导模型并存**：`points.len()==2`（`elbow_polyline_offset` 偏移推导）vs `>2`（`elbow_vertex_polyline` 顶点锚定），分支散布命中 / 渲染 / 手柄 / 拖拽 / 插入 5+ 处
- **字段有效性纠缠**：`elbow_mid_offset` 仅 2 点有效；`closed`×elbow、roundness×elbow 均需特判（#23 断线 bug 即源于此）
- props 面板 Straight↔Elbow 互切时顶点语义重解释（自由点 ↔ bar 锚点），切换即变义
- 历史成本：#21 前后 6 轮验收补丁、#23 修复均在为混合模型打补丁

Excalidraw / tldraw 的共识：**连接器中间几何是派生值（derived）而非存储值（baked）**——存 2 端点（+固定段），路径由路由函数推导；多段线保持自由编辑，两者类型隔离（Excalidraw `elbowArrow` / tldraw `TLArrowInfo` 判别联合）。

### 已拍板决策

| # | 决策 | 内容 |
|---|---|---|
| DP-1 | 工具入口 | **Arrow 工具改为 Elbow 工具**（`A` 键不变，默认终点箭头保留）；Line 工具对应多段线，箭头走样式面板起/终点开关（已有能力），语义清晰 |
| DP-2 | 路由范围 | **完整对齐 Excalidraw**：A\* 网格避障 + fixedSegments 段固定，绑定形状移动时自动重算（**推翻 L21**） |
| DP-3 | 互转 | 仅做 **Elbow → Polyline 烘焙**（右键 / 属性面板，当前路由顶点固化为自由点）；反向（Polyline → Elbow）不做 |
| DP-4 | 删除 | N 顶点 elbow 机制**删干净**：`ElbowInsertCandidate`、`clamp_elbow_vertex_drag`、`elbow_vertex_polyline`、双击插入 elbow 段、`len==2` 特判及相关测试（git 历史可溯） |

### 待拍板（实施中定）

- **DP-5** fixedSegments 精确存储形态（✅ 2026-10-02 已拍板）：**`Vec<ElbowFixedSegment { start, end }>` 局部坐标、按路径顺序排列，不存 index**。与 Excalidraw 的 `{index, start, end}` 方案刻意偏离：index 是对「上一帧路由结果」的引用，重路由后漂移，需要额外的 renormalize 步骤；坐标锚定 + 顺序表自洽（段顺序只在用户拖段时变化）。首/末段不可固定（绑定端恒垂直进出边界，Excalidraw 同款约束）。`#[serde(default)]` 零迁移。
- **DP-6** A\* 参数（✅ 2026-10-02 已拍板，依据通读 Excalidraw `elbowArrow.ts` master + [mtolmacs 算法博客](https://plus.excalidraw.com/blog/building-elbow-arrows-part-one)）：
  - **障碍范围 = 仅两端绑定形状**（各自取旋转 AABB、四边膨胀 `ELBOW_PADDING = 40` 画布px，对齐 Excalidraw `BASE_PADDING`）。**不做全场景绕行**——Excalidraw master 的 A\* 障碍同样只有两端动态 AABB（`generateDynamicAABBs`），绕开其它元素至今未实现（issue [#8635](https://github.com/excalidraw/excalidraw/issues/8635) 开放中）；待验收反馈再议。
  - **网格 = 非均匀网格**（Excalidraw `calculateGrid` 同思路）：坐标集 = 两端点（及绑定端沿法线外推 padding 的虚拟节点）+ 各障碍 AABB 边线 ± union 外扩一档，取 x/y 笛卡尔交点为节点，节点数 O(k²)（典型 <150）。**每帧重算、不做缓存**——纯函数同源不变量免费保持，性能由网格规模兜底。
  - **A\* 细节**：邻格步进（步长 = 相邻网格线）；首步/末步受 heading 约束（绑定端 = 锚点边法线，自由端 = 存储取向）；禁止立即反向；转弯罚 = 曼哈顿距离³（对齐 Excalidraw bendPenalty 量级，最少转弯优先、路程次之）；碰撞判据 = 段中点落障（障碍边恒在网格线上 → 判据精确）。
  - **分层路由**：任一端绑定 → 恒走 A\*（首/末腿 = 沿法线 padding 长的"插座腿"，对齐 Excalidraw dongle 观感）；两端自由 → 确定性 L/Z/S（`elbow_mid_offset` bar 语义保持，旧档视觉不变）。A\* 失败 / 退化 → 回退确定性规则。

### 目标模型（preferz-core）

```rust
pub enum ShapeType { Rectangle, Ellipse, Diamond, Polyline, Elbow }  // Elbow 新增
pub enum CurveType { Straight, Curved }                              // 移除 Elbow
```

- `ItemKind::Shape { shape_type: Elbow, .. }`：`points` 恒 2 端点（绑定 / 吸附复用现有逻辑）；`fixed_segments`（形态待 DP-5，`#[serde(default)]`，首版等价现 `elbow_mid_offset`）；`roundness` / 起终点箭头沿用；**无** `closed` / `curve_type` 语义
- 路由纯函数 `elbow_route(start, end, fixed_segments, obstacles) -> Vec<Point>`：阶段 A 首版 = 现有确定性 L/Z/S 规则（`elbow_polyline_offset`），阶段 C 换 A\*；命中 / 渲染 / 手柄全消费同一函数（保持现状同源）
- 正交性由路由函数输出保证，无运行时防御；通用 polyline 操作（加点 / 闭合 / 切曲线）经 enum match **编译期排除**
- 重路由是派生结果**不进 undo 栈**；仅 `fixed_segments` 变更走命令（`SetElbowOffset` 泛化）

### `.prz` 迁移（I4 惯例，加载时转换）

- **建议方案**：fileio 读入后对 item JSON 做预处理 shim（`serde_json::Value` 识别 `curve_type:"elbow"` → 改写 → 反序列化），core 枚举彻底移除 Elbow 变体。备选：保留 legacy 变体仅 serde 用（缺点：core 全部 match 需兜底 arm）
- N 顶点 elbow（>2 点）→ `Polyline + Straight`，顶点原样保留——渲染路径本就是正交折线，**视觉无损**，仅顶点变自由点
- 2 点 elbow → `ShapeType::Elbow`，`elbow_mid_offset` 平移到新字段
- 旧文件 fixture 单测覆盖两类迁移 + 保存重开后不再出现 legacy 形态

### 分期（每期独立可合入）

| 期 | 内容 | 交付物 |
|---|---|---|
| A | core 模型拆分 + fileio 迁移 shim + DP-4 删除 + `elbow_route`（L/Z/S 首版） | 旧 `.prz` 可开且视觉不变；`cargo test` 绿 |
| B | 工具 / UI：Elbow 工具替换 Arrow（`Tool::Linear` 收敛为直线）、样式面板分流（elbow：箭头 / 倒角；polyline：直线 / 曲线 / 闭合）、右键「转为多段线」（DP-3）、手柄（端点 + bar） | 可画 / 可编辑 / 可烘焙，undo 全通 |
| C | 🔶 A\* 网格避障路由 + 绑定移动全路径重算 + 失败回退 L/Z/S（2026-10-02 交付：非均匀网格，DP-6 拍板参数，无缓存每帧重算） | 拖动绑定形状自动绕行 |
| D | 🔶 fixedSegments 段固定：拖中间段即固定 / 拖已固定段改位、路由尊重固定段、`SetElbowOffset` 泛化为 `SetElbowFixedSegments`（2026-10-02 交付） | 段级手动控制 |

> C/D 顺序可对调：若先定 DP-5 存储形态，D 先行可让 A\* 落地即带固定段约束，避免二次返工——实施时拍。

### 测试与验收

- **core 单测**：新类型 serde 往返；`elbow_route` 确定性输出；A\* 典型避障场景（环绕 / 死角 / 无路回退）；迁移两类转换
- **手工验收**：旧 `.prz`（含 2 点 / N 点 elbow）打开视觉不变；Elbow 工具创建 → 绑定 → 拖端点 → 自动绕行 → 拖段固定 → 烘焙为多段线 → undo/redo；手绘风倒角回归（#23 管线复用）；mermaid 边回归
- **质量门**：`cargo fmt --all --check` / `clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace` 全绿 + 人工 `cargo run` 观感确认

### 涉及文件

| 文件 | 改动 |
|---|---|
| `crates/preferz-core/src/shape.rs` | `ShapeType::Elbow`；`CurveType` 收两值；单测 |
| `crates/preferz-core/src/item.rs` | Elbow 命中 / AABB / 构造器；`elbow_route`；删 `ElbowInsertCandidate` / `elbow_vertex_polyline` / `clamp_elbow_vertex_drag` 及相关测试 |
| `crates/preferz-core/src/commands.rs` | `SetElbowOffset` 泛化；`SetCurveType` 收窄 |
| `crates/preferz-core/src/routing.rs`（新，名可调） | 阶段 C：A\* 网格路由（L1 纯函数，headless 可测） |
| `crates/preferz/src/ui/stylers.rs` | Elbow 渲染分支（倒角 / 手绘风管线复用）；删 `len==2` 特判 |
| `crates/preferz/src/ui/widgets/transform_handles.rs` | Elbow 手柄（端点 + bar）；删特判 |
| `crates/preferz/src/preferz_app/{mod,drag,props,actions,render}.rs` | `Tool::Elbow`；删双击插入 elbow 段 / `clamp_elbow_vertex_drag` 调用；样式面板分流；右键烘焙；i18n |
| `crates/preferz/src/i18n.rs` | `ToolElbow` 等文案 |
| `crates/preferz-fileio/src/{prz,schema}.rs` | JSON 迁移 shim + 旧文件 fixture 测试 |

---

## 功能性待办（尚未实施）

> 源自 `.issues`（本地目录，gitignore 排除）。#1–#5 均已交付（#4 由 `Action::FitToScreen` 覆盖，
> union 全部 item AABB，等价"显示所有元素"）。**当前无未实施项。**

---

## 远期 / 未决

> 详细记录在本地 `.issues/issues.md`（gitignore 排除、不入库）。

- [ ] ttf 字体体积优化（内嵌字体 deflate 压缩已做，全量子集是否够用待评估）
- [ ] 放弃 `.bee` 兼容后可做的架构简化盘点（`PrzFile` 命名与 `prz.rs` 路径已改，schema 简化空间待盘）
- [ ] 视口裁剪已有，LOD 缩略图（Phase 5+）待评估
