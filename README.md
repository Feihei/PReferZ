# PReferZ

![icon](./assets/icon.png)

[中文readme](#chinese)

A minimalist reference image aggregator desktop app written in Rust. Infinite canvas, pan/zoom, image import, Excalidraw-style hand-drawn shapes, freehand ink, text notes, flowcharts, charts, frame slides, undo/redo, and `.prz` project save/load.

For design / architecture details (crate layout, data models, phases), see [`.agents/specs/preferz-spec.md`](./.agents/specs/preferz-spec.md). Roadmap & delivery archive: [`.agents/plan.md`](./.agents/plan.md) / [`.agents/CHANGELOG.md`](./.agents/CHANGELOG.md).

---

### Quick Start

```bash
cargo run -p preferz             # run the app
cargo check --workspace          # fast type check
cargo test --workspace           # unit tests
cargo build --release -p preferz # release binary
```

### Tech Stack

- **Rust** (stable, >= 1.88)
- **GUI**: `egui` + `eframe` 0.36 (glow backend)
- **2D geometry**: `euclid`
- **File I/O**: `rusqlite` (`.prz` = SQLite + sqlar), `image`
- **Utilities**: `rfd` (file dialogs), `arboard` (clipboard), `serde_json`
- **Fonts**: built-in Source Han Sans CN + 851 handwriting font (GB2312+ASCII subset), deflate-compressed into the binary at build time

### Features

- Infinite canvas: pan (middle-drag), zoom at cursor (10%–1000%), box-select
- Images: import (drag & drop / file dialog / paste), crop, rotate & scale handles, opacity, grayscale, eyedropper
- Hand-drawn-style shapes: rectangle / ellipse / diamond / line / arrow / polygon / frame, hachure & semi-transparent fills, sloppiness levels, line endpoints snap to shape edges
- Freehand ink with velocity-tapered strokes (Catmull-Rom smoothing)
- Text notes — standalone or bound to shapes; sans + handwriting fonts, H/V alignment
- Flowcharts: `Ctrl+Arrows` clones a connected node, `Alt+Arrows` navigates along connections; import mermaid flowcharts (right-click menu)
- Charts: paste two-column spreadsheet data → bar / line chart
- Align (6-way) / distribute (gap / centers) / arrange (linear, grid, optimal packing); group & ungroup
- Frame slides presentation (`F5`) with aspect-ratio / paper presets
- Autosave (`.prz.autosave`) with restore prompt; every edit goes through undo/redo
- Dark / Light / Auto themes, English / 中文 UI
- Always-on-top / frameless / background-transparent floating-board mode

### Basic Usage

**Mouse**

| Action | Effect |
| --- | --- |
| Scroll wheel | Zoom at cursor |
| Middle-button drag | Pan the canvas |
| Left drag on an item | Move it (`Ctrl`+drag moves a copy, `Shift`+drag constrains to axis) |
| Drag corner / edge handle | Scale (hold `Ctrl` to break aspect ratio) |
| Drag rotation handle | Rotate |
| Drag line / arrow endpoint | Move endpoint; snaps to nearby shape edges and stays bound to them |
| `Alt`+click a polyline / polygon vertex | Delete that vertex |
| `Alt`+drag an endpoint | Extend the line with a new vertex |
| Left drag on empty canvas | Box-select |
| Double-click empty canvas | Create a text note |
| Double-click a shape (or press `Enter`) | Edit its text |
| Right-click | Open context menu (arrange / align / distribute, group, mermaid, …) |

**Keyboard**

| Shortcut | Action |
| --- | --- |
| `Ctrl+N` | New canvas |
| `Ctrl+O` | Open `.prz` project |
| `Ctrl+I` | Import image onto canvas |
| `Ctrl+S` / `Ctrl+Shift+S` | Save / save as |
| `Ctrl+Z` / `Ctrl+Shift+Z` (or `Ctrl+Y`) | Undo / redo |
| `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | Copy / cut / paste (two-column text data opens a chart picker) |
| `Ctrl+D` | Duplicate selection in place |
| `Delete` | Delete selected items |
| `Ctrl+G` / `Ctrl+Shift+G` | Group / ungroup |
| `V` `R` `D` `O` `A` `L` `P` | Tools: select / rectangle / diamond / ellipse / arrow / line / freehand (same tools also on number keys `1`–`7`) |
| `Shift+P` / `F` | Polygon tool / frame tool |
| `Shift+1` / `Shift+2` / `Shift+3` | Zoom to fit / zoom to selection / zoom to 100% |
| `I` | Toggle color picker |
| `C` | Enter crop mode (single image selected) — `Enter` apply, `Esc` cancel |
| `Enter` | Edit text of selection / confirm crop |
| `F5` | Toggle slide presentation (`→` `Space` `PgDn` next, `←` `PgUp` prev, `Home` / `End` first / last) |
| `Ctrl+Arrows` | Create connected flowchart node (single rect / ellipse / diamond selected) |
| `Alt+Arrows` | Navigate along connections |
| `Ctrl+Shift+P` | Show context menu |
| `Esc` | Cancel / close menu |

### Autosave

While an opened `.prz` has unsaved changes, a silent backup `foo.prz.autosave` is written next to it after the idle interval (default 30 s, configurable 10–300 s). Reopening the original file offers to restore the newer backup; manual `Ctrl+S` semantics are unchanged.

### Settings

The in-app settings panel covers arrange spacing, always-on-top / frameless / background transparency, autosave (toggle + interval), default frame aspect ratio, UI language, and theme (Dark / Light / Auto).

Welcome page lists recent projects (persisted at `~/.preferz/recent.json`, up to 10 entries).

### License

MIT. Bundled fonts: Source Han Sans CN (SIL OFL 1.1) and the 851 handwriting font (free for commercial use) — see [`assets/FONT_LICENSES.md`](./assets/FONT_LICENSES.md).

---

<a id="chinese"></a>

## PReferZ

Picture Reference Z - 一个用 Rust 编写的极简参考图聚合桌面应用。无限画布，平移/缩放，图片导入，Excalidraw 风格手绘图形，徒手笔迹，文本便签，流程图，图表，画框演示，撤销/重做，`.prz` 工程存档。

设计与架构细节（Crate 划分、数据模型、阶段计划）见 [`.agents/specs/preferz-spec.md`](./.agents/specs/preferz-spec.md)；路线图与交付归档见 [`.agents/plan.md`](./.agents/plan.md) / [`.agents/CHANGELOG.md`](./.agents/CHANGELOG.md)。

### 快速开始

```bash
cargo run -p preferz             # 运行应用
cargo check --workspace          # 快速类型检查
cargo test --workspace           # 单元测试
cargo build --release -p preferz # 构建发布版本
```

### 技术栈

- **Rust** (stable, >= 1.88)
- **GUI**: `egui` + `eframe` 0.36 (glow 后端)
- **2D geometry**: `euclid`
- **File I/O**: `rusqlite` (`.prz` = SQLite + sqlar), `image`
- **工具库**: `rfd`（文件对话框）, `arboard`（剪贴板）, `serde_json`
- **字体**: 内置思源黑体 + 851 手写体（GB2312+ASCII 子集），构建期 deflate 压缩嵌入二进制

### 功能特性

- 无限画布：中键平移、光标处缩放（10%–1000%）、框选
- 图片：导入（拖放 / 对话框 / 粘贴）、裁剪、旋转与缩放手柄、不透明度、灰度、取色器
- 手绘风图形：矩形 / 椭圆 / 菱形 / 直线 / 箭头 / 多边形 / 画框，斜线填充与半透明填充，手绘档位，线段端点吸附图形边缘
- 徒手笔迹：按运笔速度锥形变宽（Catmull-Rom 平滑）
- 文本便签：独立或绑定图形；黑体 + 手写体，水平/垂直对齐
- 流程图：`Ctrl+方向` 克隆连接节点、`Alt+方向` 沿连接导航；右键菜单导入 mermaid 流程图
- 图表：粘贴两列表格数据 → 柱状 / 折线图
- 对齐（6 向）/ 分布（等距 / 等心）/ 排列（线形、网格、最优装箱）；编组与解组
- 画框幻灯片演示（`F5`），比例 / 纸张预设
- 自动保存（`.prz.autosave`）与恢复提示；所有编辑均可撤销/重做
- 深色 / 浅色 / 跟随系统主题，中英双语界面
- 窗口置顶 / 无边框 / 背景透明的悬浮看图板模式

### 基本用法

**鼠标**

| 操作 | 效果 |
| --- | --- |
| 滚轮 | 以光标位置为锚点缩放 |
| 中键拖拽 | 平移画布 |
| 左键拖拽元素 | 移动（`Ctrl`+拖动移动副本，`Shift`+拖动锁定水平/垂直轴） |
| 拖拽角点/边缘手柄 | 缩放（按住 `Ctrl` 解除等比） |
| 拖拽旋转手柄 | 旋转 |
| 拖拽直线/箭头端点 | 移动端点；靠近图形边缘时吸附并保持绑定跟随 |
| `Alt`+单击折线/多边形顶点 | 删除该顶点 |
| `Alt`+拖拽端点 | 延伸出新顶点 |
| 左键在空白处拖拽 | 框选 |
| 双击空白处 | 新建文本便签 |
| 双击图形（或按 `Enter`） | 编辑其文字 |
| 右键 | 打开上下文菜单（排列/对齐/分布、编组、mermaid 等） |

**键盘**

| 快捷键 | 动作 |
| --- | --- |
| `Ctrl+N` | 新建画布 |
| `Ctrl+O` | 打开 `.prz` 工程 |
| `Ctrl+I` | 载入图片到画布 |
| `Ctrl+S` / `Ctrl+Shift+S` | 保存 / 另存为 |
| `Ctrl+Z` / `Ctrl+Shift+Z`（或 `Ctrl+Y`） | 撤销 / 重做 |
| `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | 复制 / 剪切 / 粘贴（两列文本数据会弹出图表选择浮层） |
| `Ctrl+D` | 原位复制选中项 |
| `Delete` | 删除选中项 |
| `Ctrl+G` / `Ctrl+Shift+G` | 编组 / 解组 |
| `V` `R` `D` `O` `A` `L` `P` | 工具：选择 / 矩形 / 菱形 / 椭圆 / 箭头 / 直线 / 徒手（数字键 `1`–`7` 等效） |
| `Shift+P` / `F` | 多边形工具 / 画框工具 |
| `Shift+1` / `Shift+2` / `Shift+3` | 缩放适应画布 / 缩放到选中 / 缩放回 100% |
| `I` | 切换取色器 |
| `C` | 进入裁剪模式（仅单张图片可触发）——`Enter` 应用，`Esc` 取消 |
| `Enter` | 编辑选中项文字 / 确认裁剪 |
| `F5` | 切换幻灯片演示（`→` `Space` `PgDn` 下一页，`←` `PgUp` 上一页，`Home` / `End` 首末页） |
| `Ctrl+方向键` | 创建连接的流程图节点（单选矩形 / 椭圆 / 菱形时） |
| `Alt+方向键` | 沿连接箭头导航 |
| `Ctrl+Shift+P` | 唤出上下文菜单 |
| `Esc` | 取消当前操作 / 关闭菜单 |

### 自动保存

打开的 `.prz` 有未保存改动时，静置超过间隔（默认 30 秒，可调 10–300 秒）即在同目录静默写 `foo.prz.autosave` 备份；重新打开原文件时若备份更新会提示恢复。手动 `Ctrl+S` 语义不变。

### 设置

应用内设置面板提供：排列间距、窗口置顶 / 无边框 / 背景透明度、自动保存（开关 + 间隔）、新建画框默认比例、界面语言、主题（深色 / 浅色 / 跟随系统）。

欢迎页会列出最近打开的工程（持久化于 `~/.preferz/recent.json`，最多 10 项）。

### 许可

MIT。内置字体：思源黑体（SIL OFL 1.1）、851 手写字体（免费商用）——详见 [`assets/FONT_LICENSES.md`](./assets/FONT_LICENSES.md)。
