# ADR-0008: ViewportState 归 core 与坐标边界收口

- 状态：已接受
- 日期：2026-09-17
- 参考：[ADR-0001](0001-workspace-crate-layout.md)、[ADR-0002](0002-coordinate-systems-euclid.md)、[CHANGELOG.md §架构整固](../CHANGELOG.md)（原计划文档 `plans/core-sink-and-app-decomposition.md` 完成后已归档删除）
- 修订：本 ADR **修订 ADR-0002 的 "Viewport → binary 层" 归属行**，将其改为 core 层。

## 背景

ADR-0001 立三 crate 的核心理由是 "core 不依赖 egui、几何/命令可独立单测"。ADR-0002 同时把
坐标系用 euclid 标签类型钉死、并明列 `ViewportState` 归 binary 层。

实践中出现一道与两条 ADR 初衷相悖的**坐标边界泄漏**：

- core 的 `spaces.rs` **早已定义** `ScreenPoint`/`ScreenRect`/`CanvasToScreen`/`ScreenToCanvas`
  （带 `ScreenSpace` 标签），但 binary 的 `viewport.rs` 绕过这些别名，字段/签名直接用
  `egui::Rect`/`Pos2`/`Vec2`。这正是 ADR-0001「跨层类型须在 core 定义、binary 复用，禁止复制」
  所预警、却未落实的情形。
- 后果：`ViewportState` 是**全 app 最易出坐标 bug 的一段**（缩放锚点漂移、侧栏开合内容平移、
  `screen↔canvas` 互逆、量纲错误——历史修过 W3/W4/B2/W2），却因住在 binary 而**不可无头单测**。
  它一旦带 egui 类型，吃它的 drag/fit/snap/zoom 决策就被迫留在 binary，core 显薄、可测面被压制。

关键判断：`ViewportState` 的"状态性"（每帧更新的 `screen_rect`、`rect_initialized` 标志、
`pan`/`zoom`）**不需要 egui，只需要一个 `ScreenRect` 值**。它对 egui 的依赖纯属"类型选错"，
而非本质。它是 L1（无副作用、只吃领域坐标数据的变换数学），不是 L2 编排。

## 决策

将 `ViewportState` 连同其坐标变换数学下沉到 `preferz-core`：

1. `ViewportState` 移入 core（新 `viewport` 模块），`pan`/`zoom`/`min_zoom`/`max_zoom`/
   `screen_rect: ScreenRect`/`rect_initialized` 全为 core 字段；方法签名用 core 已有的
   `ScreenPoint`/`ScreenRect`/`ScreenVector`/`CanvasToScreen`，**不出现任何 egui 类型**。
2. egui↔core 边界收口为**一对一对一转换**：`egui::Pos2 ↔ ScreenPoint`、`egui::Rect ↔ ScreenRect`，
   以 `From`/`.into()` 或极小 helper 实现，**只在 L2 输入边（读指针/面板矩形）与 L3 绘制边（画到屏幕）
   各点一次**。binary 侧不再有坐标变换逻辑。
3. `set_screen_rect` 由 CentralPanel 每帧喂入，调用点做 `egui_rect.into()` → core `ScreenRect`。
4. 由此解锁：drag/fit/snap/zoom 中"给定(起点,当前点,rect)→ 变换/命令"的纯决策可成建制进 core，
   由 `cargo test -p preferz-core` 无头覆盖。

修订范围**仅限** ADR-0002 的归属表 Viewport 那一行（binary→core）；ADR-0002 其余决策（三坐标系
euclid 标签、禁裸 f32 跨系、跨系只走显式矩阵、命中用 OBB、物理量以画布像素定义 × zoom）**全部保留、
不受影响**。

## 后果

- ✅ 兑现 ADR-0001：最易出 bug 的坐标数学进 core、秒级无头测试；CI 无 GUI 依赖面扩大。
- ✅ 封上 ADR-0001「禁止复制跨层类型」当初预警的泄漏：`ScreenSpace` 别名在 core 唯一定义、binary 复用。
- ✅ L1/L2 分层从此有清晰的物理边界（egui↔core 只剩两个类型转换点）。
- ⚠️ `canvas_to_screen`/`screen_to_canvas`/`canvas_to_screen_rect` 调用点波及广，改类型是机械但量大；
  列为架构整固计划的**独立高风险步**，单独跑质量门 + 手测（缩放锚点、平移、侧栏开合、Shift+1/2/3）。
- ⚠️ ADR-0002 不再"字面自洽"：其归属表 Viewport 行以删除线指向本 ADR，阅读时需并读 ADR-0008。
