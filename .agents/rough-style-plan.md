# PReferZ Phase F：手绘风描边（RoughStyler）实施计划

> **状态（2026-08-29 更新）**：**代码已合入 main**，提交 `3a21aef`。自动化测试全绿（工作区 50 个测试），`cargo fmt` / `cargo clippy -D warnings` 均通过。
> **剩余**：§6「手工验收清单」需 Feihei 本人跑 GUI 确认观感（7 项未勾）。
>
> **配套文档**：`.agents/shapes-and-frame-slides-design.md`（§4 风格生成器、§16 非目标、§附录 B roughjs 对照）、`.agents/shapes-and-frame-slides-plan.md`（Phase A–E 已全部合入）。
> **决策（2026-08-29）**：本轮只做**手绘描边**，填充保持纯色（不做 hachure 斜线阴影）。

**Goal:** 让 Shape（矩形/椭圆/菱形/折线）可切换为手绘风描边，观感接近 Excalidraw；抖动确定性可复现，缩放/平移/存盘重开后形状不变。

**Architecture:** `preferz-core` 新增确定性 PRNG（`SeededRng`）与 `ItemKind::Shape.rough` 开关；binary 层新增 `RoughStyler` 实现既有 `ShapeStyler` trait，与 `CleanStyler` 并列；渲染层按 `rough` 分发。沿项目既有惯例——**零新依赖**（与自实现 BT.601 灰度、自实现 MaxRects 装箱同一路子），不引入 `roughr`。

**Tech Stack:** Rust 2021、egui/eframe 0.29（`epaint::CubicBezierShape`）、euclid、serde。**Phase F 零新依赖。**

---

## 1. 目标与非目标

### 目标

| # | 目标 |
|---|---|
| G1 | 矩形/椭圆/菱形/折线可切换手绘描边，双击笔画质感（双描边 + 端点/控制点抖动） |
| G2 | 抖动**确定性**：同 `seed` 恒得同一结果（缩放、平移、存盘重开均不变） |
| G3 | 样式面板开关：绘制工具激活时设默认值；选中 Shape 时可 per-item 切换（走 undo） |
| G4 | 旧存档兼容：无 `rough` 字段的 JSON 正常加载（默认 `false`） |
| G5 | `cargo fmt` / `clippy -D warnings` / `cargo test --workspace` 全绿 |

### 非目标（本轮明确不做）

- **hachure 斜线阴影填充**：填充保持纯色凸多边形，仅描边手绘化
- **roughness 数值可调**：常量写死（1.0），不进 UI
- **corner overshoot（笔画出头）**：rough.js 有，本轮不做
- **手绘风文本框/Frame 边框**：不在范围内

---

## 2. 算法：rough.js `_line` 移植

对每条边 `a → b`，重复 `PASSES = 2` 次，每次生成一条三次贝塞尔：

```
len_screen  = |b - a|                        // 屏幕像素
len_canvas  = len_screen / zoom              // 还原到画布单位
max_offset  = min(len_canvas * 0.06, 8.0) * zoom     // 抖动幅度（屏幕像素）
half        = max_offset * 0.5

// 弓形位移：垂直于边，幅度随边长增大（rough.js 的 bowing）
bow_k       = min(len_canvas / 200.0, 1.0)
mid_disp    = ( bowing * max_offset * bow_k / len_screen ) * (-(b.y-a.y), (b.x-a.x))

diverge     = 0.2 + rng.f32() * 0.2          // 控制点沿边的位置

p0 = a + rng.offset(half)                    // 端点抖动（x/y 各独立随机）
p3 = b + rng.offset(half)
m1 = a + (b - a) * diverge                   // 控制点 1 基准
m2 = a + (b - a) * (2 * diverge)             // 控制点 2 基准
c1 = m1 + mid_disp + rng.offset(half)
c2 = m2 + mid_disp + rng.offset(half)

→ CubicBezierShape::from_points_stroke([p0, c1, c2, p3], false, TRANSPARENT, stroke)
```

**为什么幅度要除以 zoom 再乘回来**：保证抖动在**画布单位**下恒定（`min(len*0.06, 8)` 画布像素），与 `stroke.width * zoom` 的缩放语义一致。放大画布时抖动同步放大 —— 与真实手绘稿被放大的观感一致（Excalidraw 同款行为）。

**椭圆降采样**：CleanStyler 用 64 段近似椭圆；手绘风下 64 段 × 2 passes = 128 条贝塞尔，开销过大。RoughStyler 用 **16 段**，抖动本身会掩盖折线感。

**虚线**：`PathStroke` 不支持 dash，故 rough + dash 时把贝塞尔采样为 16 个点的折线，再走 `Shape::dashed_line`。实线直接用贝塞尔（更省数据、曲线更顺）。

**箭头**：起点/终点箭头不抖（抖动的箭头会认不出方向），复用 CleanStyler 的箭头构造逻辑 —— 抽成模块级 `push_arrow_heads()` 供两个 styler 共用。

---

## 3. 数据模型

### 3.1 `preferz-core/src/shape.rs` — 确定性 PRNG

```rust
/// 确定性 PRNG（xorshift64*）。同一 seed 必得同一序列。
pub struct SeededRng { state: u64 }

impl SeededRng {
    pub fn new(seed: u64) -> Self;   // seed == 0 时用黄金比例常数兜底
    pub fn next_u64(&mut self) -> u64;
    pub fn next_f32(&mut self) -> f32;   // [0, 1)
    pub fn signed(&mut self) -> f32;     // [-1, 1]
}
```

放在 core（不依赖 egui），便于单测确定性。

### 3.2 `ItemKind::Shape` 新增字段

```rust
    /// 手绘风描边开关（Phase F）。true 时由 RoughStyler 渲染。
    /// `#[serde(default)]`：旧存档无此字段时按 false 加载。
    #[serde(default)]
    rough: bool,
```

`seed: u64` 字段 A 期已预埋，本轮启用：开启手绘时若 `seed == 0` 则赋随机值。

### 3.3 `Item` 方法

| 方法 | 说明 |
|---|---|
| `Item::with_rough(mut self, rough: bool) -> Self` | builder，供创建路径使用；`rough=true` 且 `seed==0` 时生成随机 seed |
| `Item::set_rough(&mut self, rough: bool)` | 供 `SetRough` 命令使用；同上补 seed |
| `Item::rough(&self) -> bool` | 只读访问，供渲染分发与选中态查询 |

> **不改 `new_shape` / `new_polyline` 签名**：二者在 core 单测、scene 单测、fileio 中共有 11 处调用，加参数属无谓 churn。`with_rough()` 只在 preferz_app.rs 的 2 处创建点链式调用。

### 3.4 `SetRough` 命令（`commands.rs`）

照 `SetClosed` 模式：`{ item_id, old_rough, new_rough }`，`redo`/`undo` 写回 `ItemKind::Shape { rough }`。

---

## 4. 渲染层改动（`crates/preferz/src/ui/stylers.rs`）

- `ShapeData` 增加 `seed: u64` 字段（styler 输入的一部分，CleanStyler 忽略）。
- 抽出模块级 `fn push_arrow_heads(...)`：原 CleanStyler 内联的起/终点箭头代码，两个 styler 共用。
- 新增 `pub struct RoughStyler;` 实现 `ShapeStyler`：
  1. 按 shape_type 生成轮廓点（矩形 4 / 菱形 4 / 椭圆 16 / 折线 N）
  2. 经 `to_screen` 变换到屏幕
  3. 有填充 → 先 push `Shape::convex_polygon(fill, PathStroke::NONE)`（纯色，不抖）
  4. 逐边 × 2 passes 生成抖动贝塞尔
  5. 开放折线补箭头（`push_arrow_heads`）
- 新增 `pub fn build_shape_visuals(kind, to_screen, zoom) -> Vec<egui::Shape>`：按 `rough` 在 `RoughStyler` / `CleanStyler` 间分发，供 `render_scene` 与 Present 模式共用（消除现有两处 ~18 行重复）。

---

## 5. UI 改动（`preferz_app.rs` / `i18n.rs`）

- `PReferZApp` 新增 `default_rough: bool`（默认 `false`）。
- 样式面板「绘制工具激活」行尾加「手绘」复选框 → 改 `default_rough`。
- 新增 `selected_shape_rough() -> Option<(ItemId, bool)>` helper（照 `selected_linear_arrows` 模式）：返回选中态中第一个 Shape 的 id 与 rough 值。
- 样式面板新增第三块：选中 Shape 时显示「手绘」复选框 → push `SetRough` 命令（走 undo）。
- 样式面板显示条件由 `tool != Select || selected_linear.is_some()` 扩展为 `|| selected_shape.is_some()`。
- `finish_create_shape` 两处创建点链式加 `.with_rough(self.default_rough)`。
- `render_scene` / Present 的 Shape 分支改用 `build_shape_visuals`，删掉重复的 `ShapeData` 组装。
- `i18n.rs` 新增 `T::StyleRough`（En: "Hand-drawn" / Zh: "手绘"）。

---

## 6. 测试

**core（`preferz-core`）** — 全部已实现（+11 测试，共 35 通过）
- [x] `SeededRng`：同 seed 序列一致、不同 seed 序列不同、`next_f32()` 落在 [0,1)、`signed()` 落在 [-1,1]
- [x] `rough` serde：旧 JSON 无 `rough` 字段 → 默认 `false`；写入 true 后往返一致
- [x] `with_rough(true)` 使 `seed != 0`；`with_rough(false)` 不改 seed
- [x] `SetRough` redo/undo 往返（含 undo/redo 后 seed 不变、非 Shape item 为空操作）

**binary（`preferz`）** — 全部已实现（+11 测试）
- [x] `RoughStyler` 输出确定性：同 seed 两次 `build_shapes` 得到等价的 `Vec<Shape>`（用 `format!("{:?}")` 比对）
- [x] 不同 seed 输出不同
- [x] 矩形 rough 输出 shape 数 = 1 fill(可选) + 4 边 × 2 passes
- [x] `build_shape_visuals` 按 `rough` 分发；非 Shape item 返回空

**质量门槛** — 全部通过
```
cargo fmt --all --check                                  ✔
cargo clippy --workspace --all-targets -- -D warnings    ✔ 零警告
cargo test --workspace                                   ✔ 50 passed
```

> **顺带修复（无关 Phase F，单独提交 `5bd8f13`）**：clippy 1.98 新增 `chunks_exact_to_as_chunks`
> lint 命中了既有代码三处（welcome logo 纹理、导出白底填充、RGBA→RGB 转换），
> 改用 `as_chunks::<4>()`。不修会导致 CI 因 `-D warnings` 失败。

**手工验收清单**（交给 Feihei）
- [ ] 工具栏切矩形工具 → 勾「手绘」→ 拖拽画矩形，观感接近 Excalidraw
- [ ] 椭圆/菱形/直线/箭头同样生效
- [ ] 缩放画布：抖动随缩放同步放大，形状不跳变（确定性）
- [ ] 选中已有形状 → 样式面板切换手绘，Ctrl+Z 可撤销
- [ ] Ctrl+S 保存 → 重开 → 手绘形状与关闭前完全一致
- [ ] 关闭手绘：回到精确几何描边
- [ ] F5 演示模式下手绘形状渲染一致

---

## 7. 涉及文件

| 文件 | 改动 |
|---|---|
| `crates/preferz-core/src/shape.rs` | 新增 `SeededRng` + 单测 |
| `crates/preferz-core/src/item.rs` | `ItemKind::Shape` 加 `rough`；`with_rough`/`set_rough`/`rough`；构造处补 `rough: false`；单测 |
| `crates/preferz-core/src/commands.rs` | 新增 `SetRough` + 单测 |
| `crates/preferz/src/ui/stylers.rs` | `ShapeData.seed`；`RoughStyler`；`push_arrow_heads`；`build_shape_visuals`；单测 |
| `crates/preferz/src/preferz_app.rs` | `default_rough`；样式面板两块；`selected_shape_rough`；2 处创建点；2 处渲染分支合并 |
| `crates/preferz/src/i18n.rs` | `T::StyleRough` 中英文案 |

---

## 8. 风险与缓解

| 风险 | 缓解 |
|---|---|
| 抖动在不同缩放级别下"形状漂移" | 幅度按画布单位计算再乘 zoom；`max_offset` 上限 8 画布像素 |
| 椭圆 64 段 × 2 passes 渲染开销过大 | RoughStyler 用 16 段降采样，抖动掩盖折线感 |
| rough + dash 组合语义含糊 | dash 时把贝塞尔采样 16 点走 `dashed_line`，保留用户设置不静默丢弃 |
| 手绘描边与命中检测不一致（视觉歪了、点击还是精确矩形） | 可接受：抖动幅度 ≤ 8 画布像素，远小于命中框；不因此改 `contains_canvas_point` |
| `Shape::convex_polygon` 只支持凸形 | 与 CleanStyler 既有行为一致，不引入新问题 |
