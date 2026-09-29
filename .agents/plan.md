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
> 决策点 L1–L21 已归档在 CHANGELOG §决策点归档（L21 曾取消 elbow 自动避障路由，
> 后由 #24「完整对齐」拍板**推翻**，见下方 #24 专节）。

| # | 打磨项 | 现状 / 剩余工作 | 关键约束 |
|---|---|---|---|
| 17 C | mermaid 布局质量 | ⏳ 批次 A（解析）/ B（映射）已交付。剩余批次 C：`&` 展开后同层变宽，现「按出现顺序」排布易交叉 → 加**重心排序（barycenter）**减边交叉；边默认改 **elbow** 更贴 mermaid 正交观感 | 批次 A/B 已于 2026-09-28 复验通过；是否做批次 C 待定（#24 落地后映射目标为 `ShapeType::Elbow`） |
| 20 | RoughStyler 剩余缺口 | ⏳ 四批已交付且人工验收通过。剩余划出的项：zigzag / dots 填充（[ADR-0005](adr/0005-shape-styler-rough-seeded.md) 注明后续自移植）、圆角矩形 `_bezierTo` 平滑抖动、椭圆 `overlap` 收笔重叠段 | 均属 rough.js 已有能力自移植，零新依赖 |
| 21 DP-B | elbow 取向翻转滞回 | ⏳ **不加**（L9）——bar 取向过对角阈值时 90° 翻转属确定性规则固有行为。观察验收反馈再定是否引入滞回 | — |
| 18 | 相乘叠合模式（Multiply / 荧光马克笔） | ⏳ **先不做**（L19），评估已存档。重启前提：egui 0.36.2 无 per-shape blend（epaint 无 `BlendMode`、glow 固定预乘 alpha），屏幕实时真 multiply 是唯一硬点（需 `Shape::Callback` + GL 状态 hack）；导出侧需先重写为正向合成。分 M1（荧光色板 + 低透明填充，近似）/ M2（真 multiply） | 导出管线重写本身是独立大项（顺带解决矢量元素导出缺失），M2 排其后 |
| 24 | elbow 连接器独立类型化 + A\* 自动路由（对齐 Excalidraw） | ⏳ **已拍板待实施**，DP1–DP4 已定、DP5–DP6 实施中拍，详见下方 #24 专节 | **推翻 L21**；取代 #21 多顶点机制与 #23 中 elbow 专属部分 |

---

## #24 elbow 连接器独立类型化 + A\* 自动路由（对齐 Excalidraw）

- 拍板日期：2026-09-28；状态：⏳ 已拍板待实施
- 关系：**推翻 L21**（原取消 elbow 自动避障路由）；#21 多顶点「顶点锚定 bar」模型与 #23 倒角机制中 elbow 专属部分被本项**取代**（CHANGELOG 保留历史归档）；17 C mermaid「边默认改 elbow」直接受益（映射目标改为新类型）。

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

- **DP-5** fixedSegments 精确存储形态：先通读 Excalidraw `packages/element/src/elbowArrow.ts`（`routeElbowArrow` A\*、`fixedSegments`、`startIsSpecial`/`endIsSpecial`）再定（段索引+坐标 vs 长度数组）
- **DP-6** A\* 参数：障碍范围（仅两端绑定形状 vs 全场景可碰撞形状）、网格粒度、膨胀 margin、网格规模上限与缓存策略——建议先做小样性能验证

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
| C | A\* 网格避障路由 + 绑定移动全路径重算 + 失败回退 L/Z/S + 性能预算 | 拖动绑定形状自动绕行 |
| D | fixedSegments 段固定：拖段固定 / 解除、A\* 尊重固定段、`SetElbowOffset` 泛化 | 段级手动控制 |

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

## #25 帮助文档实例（help.prz 内嵌 + spawn 真实例）

- 拍板日期：2026-09-29；状态：⏳ 已拍板待实施
- 参考：Excalidraw 右下角「?」帮助入口；PureData 帮助补丁（帮助文档即原生格式 .pd）。

### 设计

- **入口**：底部 HUD（缩放条左侧）加 `?` 按钮，点击 spawn 一个**真 PReferZ 进程**：
  `current_exe() --help-doc`。不走 egui viewport 多窗口（deferred/immediate 的
  平台风险大，两进程各跑各的事件循环，零 viewport 坑）。
- **help.prz 内嵌**：`assets/help.prz` 经 build.rs deflate 压缩进二进制（照抄字体管线，
  `rerun-if-changed`）；`PrzFile::from_bytes()`（rusqlite `serialize` feature，
  `Connection::deserialize` 内存直载，零临时文件）在启动时同步加载。
- **修改语义（沙箱）**：帮助实例是完整可编辑的真 App——undo/拖拽/快捷键全可用；
  但 `current_file = None`（嵌入内容无磁盘路径），改动只活在内存里，下次打开还原。
  提示写进 help.prz 内容本身（醒目位置注明"修改不会保存，可另存为"），零代码门禁。
- **另存为**：`save_file()` 已有 fallthrough（`current_file=None` → `save_file_as`），
  天然满足；另存出的文件成为普通 `.prz`。
- **递归**：帮助实例里 `?` 按钮照常可用（同 PureData），可再开一层。
- **config**：接受 last-writer-wins——两进程各自读改 `config.json`，以最后一次写盘为准；
  帮助页可注明"已打开窗口的配置在内存里"。
- **免费行为（无需门禁）**：recent.json 不污染（嵌入加载不走 `add_recent_file`）；
  `.autosave` 不触发（`tick_autosave` 要求 `current_file=Some`）；关闭时 dirty 静默
  （内容已自述）；窗口标题 `PReferZ Help` 以示区分。

### 实施拆分

1. `preferz-fileio`：`PrzFile::from_bytes()`（格式校验 + group_id 迁移复用 open 逻辑，
   抽公共 helper）+ 往返测试；rusqlite 加 `serialize` feature
2. `build.rs`：`assets/help.prz` → OUT_DIR `help.prz.zlib` + `HELP_PRZ_RAW_SIZE`
3. `lib.rs`：`help_doc_prz_bytes()` 解压函数 + SQLite 魔数回归测试
4. `main.rs`：解析 `--help-doc`；标题切换；`PReferZApp::new_help_doc(ctx)`
5. `mod.rs`：从 `finish_load` 抽出纹理重映射/上传公共路径；帮助场景启动装载
6. HUD `?` 按钮 + spawn；i18n `T::OpenHelp` / `T::FlashHelpSpawnFailed`
7. `crates/preferz/examples/gen_help_doc.rs`：生成占位 `assets/help.prz`（正式内容
   人工在 PReferZ 里画好后覆盖保存即可，重编译生效）

### 验收

- 质量门全绿；手工：点 `?` 开新窗（内容为 help.prz）→ 随意涂改 → 关闭重开还原；
  另存为产出合法 `.prz`；主实例 recent/autosave 不受影响；帮助窗内再点 `?` 递归可开。

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
