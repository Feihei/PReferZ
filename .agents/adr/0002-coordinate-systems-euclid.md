# ADR-0002: 三坐标系与 euclid 类型约束

- 状态：已接受
- 日期：2026-07（项目立项）
- 参考：[spec §4](../specs/preferz-spec.md)（spaces）、`crates/preferz-core/src/spaces.rs`

## 背景

画布应用最常见的一类 bug 是像素坐标与世界坐标混用：缩放后命中偏移、抖动幅度不随
zoom 放大、手柄位置错位。裸 `f32` 传参在编译期无法发现这类错误。

## 决策

三个坐标系统一用 euclid 参数化类型表达，**禁止裸 f32 强转跨系**：

| 空间 | 单位 | 归属 |
|---|---|---|
| `ScreenSpace` | 屏幕像素 | egui 绘制 |
| Viewport | 缩放/平移状态（`ViewportState`） | binary 层 |
| `CanvasSpace` | 世界坐标（画布像素） | 场景数据 |

- 跨系变换只走 `canvas_to_screen_transform()` / `screen_to_canvas()` 等显式矩阵
- 抖动幅度、松弛阈值等"物理量"以画布像素定义、乘 `zoom` 落到屏幕（Phase F 实测有效）
- 命中检测用 OBB（`contains_canvas_point`），不用 AABB（曾致旋转后命中错误，修 B2/W2）

## 后果

- ✅ 编译期拦截坐标系混用；zoom 语义一处定义
- ⚠️ 类型略啰嗦（`Point2D<f32, CanvasSpace>`），属可接受成本
