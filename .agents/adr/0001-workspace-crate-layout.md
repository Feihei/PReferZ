# ADR-0001: 三 crate 工作区分层

- 状态：已接受
- 日期：2026-07（项目立项）
- 参考：[spec §4](../specs/preferz-spec.md)、AGENTS.md「Architecture」

## 背景

单 crate 会把 eframe 渲染、几何核心、文件 IO 焊死在一起：单元测试无法脱离窗口环境跑，
文件格式演化会牵连渲染层。

## 决策

```
preferz (binary, eframe::App)
  ├── preferz-core        Item / Transform / Scene / Selection / Commands / Arrange
  └── preferz-fileio      .prz 存取、图片加载、导出
```

- **core 不依赖 egui**：几何、命中、命令全部可独立单测（当前 50+ 单测的根基）
- binary 层持有 `PReferZApp { Scene, UndoStack, ViewportState }`，每帧 `update()` 派发
- fileio 独立演化，`.prz` 格式变更不触碰渲染

## 后果

- ✅ core 测试秒级跑完，CI 无 GUI 依赖
- ✅ 渲染/格式两条演化线解耦
- ⚠️ 跨层类型（如 `ItemLocalSpace`）需在 core 定义、binary 复用，禁止复制
