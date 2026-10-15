# PReferZ Roadmap（plan.md）

> 本文档是 PReferZ 的**前瞻路线图**：只保留**未完成**与**远期**两类内容；
> **已交付内容的唯一归档在 [`CHANGELOG.md`](CHANGELOG.md)**（含每项的实现要点、测试、复验与
> 决策点）。设计规格见 [`specs/`](specs/)，架构决策见 [`adr/`](adr/)。
>
> 工作流惯例：**规划文档先提交，实现拆独立 commit**（Conventional Commits）；质量门槛
> `cargo fmt --check` / `clippy -D warnings` / `cargo test --workspace` 全绿**且**人工 `cargo run`
> 确认观感，才算交付。**一条已交付项从本文件清空的条件**：质量门全绿 + commit 落库 +
> CHANGELOG 有对应小节（并标注复验日期）。`plan.md` 的「待人工复验」清单已于 2026-09-28
> 全部通过并清空（见 CHANGELOG §人工复验清账）；2026-10-09 二次清账：#17 批次 C、
> #20、#6 元素超链接复验通过并归档。

**整体目标**：BeeRef 的 Rust 精神继承者——启动更快、包更小的极简参考图聚合桌面应用（egui + eframe），交互向 Excalidraw 对齐。

---

## 状态图例

- ✅ 已完成并已归档（→ CHANGELOG）
- 🔶 代码已交付，待人工验收
- ⏳ 计划中（未开始）

---

## Excalidraw 打磨批次：未完成项

> 已交付的 1–23 号打磨项全部归档在 CHANGELOG（2026-09-03 ~ 10-09 各节），本节只列**剩余**。
> 决策点 L1–L22 已归档在 CHANGELOG §决策点归档（L21 曾取消 elbow 自动避障路由，
> 后由 #24「完整对齐」拍板**推翻**、已于 2026-10-04 交付归档；L9 曾取消取向滞回，
> 2026-10-02 由 L22 **推翻**落地）。

| # | 打磨项 | 现状 / 剩余工作 | 关键约束 |
|---|---|---|---|
| 18 | 相乘叠合模式（Multiply / 荧光马克笔） | ⏳ **先不做**（L19），评估已存档。重启前提：egui 0.36.2 无 per-shape blend（epaint 无 `BlendMode`、glow 固定预乘 alpha），屏幕实时真 multiply 是唯一硬点（需 `Shape::Callback` + GL 状态 hack）；导出侧需先重写为正向合成。分 M1（荧光色板 + 低透明填充，近似）/ M2（真 multiply） | 导出管线重写本身是独立大项（顺带解决矢量元素导出缺失），M2 排其后 |

---

## 远期 / 未决

> 详细记录在本地 `.issues/issues.md`（gitignore 排除、不入库）。

- [ ] ttf 字体体积优化（内嵌字体 deflate 压缩已做，全量子集是否够用待评估）
- [ ] 放弃 `.bee` 兼容后可做的架构简化盘点（`PrzFile` 命名与 `prz.rs` 路径已改，schema 简化空间待盘）
- [ ] 视口裁剪已有，LOD 缩略图（Phase 5+）待评估
- [ ] CLI 命令扩展（V1 已于 2026-10-14 交付、`--mcp` stdio MCP
      服务器已交付（双栈协议 + add_mermaid/add_chart 工具，见 CHANGELOG）、
      GUI 内嵌监听 + `--mcp` 代理桥接已交付（2026-10-15，工具实时操作已打开
      窗口的画布；图片类操作仍不支持））：
      set_style / transform（旋转/缩放/绝对定位）、编组（group/ungroup）、
      z 序调整、连线与绑定创建；headless PNG/SVG 导出（硬点：渲染在 binary
      层 egui Context，需先抽离或接 `Shape::Callback` 离屏上下文）
