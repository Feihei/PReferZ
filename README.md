# PReferZ

![icon](./assets/icon.png)

[中文readme](#chinese)

A minimalist reference image aggregator desktop app written in Rust. Infinite canvas, pan/zoom, image import, transform handles, undo/redo, sticky notes, and `.prz` project save/load.

For design / architecture details (crate layout, data models, phases), see [`.agents/preferz-spec.md`](./.agents/preferz-spec.md).

---

### Quick Start

```bash
cargo run -p preferz             # run the app
cargo check --workspace          # fast type check
cargo test --workspace           # unit tests
cargo build --release -p preferz # release binary
```

### Tech Stack

- **Rust** (stable, >= 1.75)
- **GUI**: `egui` + `eframe` (glow backend)
- **2D geometry**: `euclid`
- **File I/O**: `rusqlite`, `image`, `rayon`

### Basic Usage

**Mouse**

| Action | Effect |
| --- | --- |
| Scroll wheel | Zoom at cursor |
| Middle-button drag | Pan the canvas |
| Left drag on an item | Move it |
| Drag corner / edge handle | Scale (hold `Ctrl` to break aspect ratio) |
| Drag rotation handle | Rotate |
| Left drag on empty canvas | Box-select |
| Double-click empty canvas | Create a text note |
| Right-click | Open context menu |

**Keyboard**

| Shortcut | Action |
| --- | --- |
| `Ctrl+N` | New canvas |
| `Ctrl+O` | Open `.prz` / `.bee` project |
| `Ctrl+I` | Import image onto canvas |
| `Ctrl+V` | Paste image from clipboard |
| `Ctrl+S` | Save |
| `Ctrl+Shift+S` | Save as |
| `Ctrl+Z` / `Ctrl+Shift+Z` (or `Ctrl+Y`) | Undo / redo |
| `Delete` | Delete selected items |
| `F` | Fit selection (or whole canvas) to screen |
| `I` | Toggle color picker |
| `C` | Enter crop mode (single image selected) — `Enter` apply, `Esc` cancel |
| `R` / `G` / `O` | Arrange selection: linear / grid / optimal pack (≥ 2 items) |
| `Ctrl+Shift+P` | Show context menu |
| `Esc` | Close menu / cancel |

The full shortcut list is also shown in the in-app **Settings** panel (toggles language and displays every keybinding).

The welcome page lists recent projects (persisted at `~/.preferz/recent.json`, up to 10 entries).

### License

MIT

---

<a id="chinese"></a>

## PReferZ

Picture Reference Z - 一个用 Rust 编写的极简参考图聚合桌面应用。无限画布，平移/缩放，图片导入，变换手柄，撤销/重做，文本便签，`.prz` 工程存档。

设计与架构细节（Crate 划分、数据模型、阶段计划）见 [`.agents/preferz-spec.md`](./.agents/preferz-spec.md)。

### 快速开始

```bash
cargo run -p preferz             # 运行应用
cargo check --workspace          # 快速类型检查
cargo test --workspace           # 单元测试
cargo build --release -p preferz # 构建发布版本
```

### 技术栈

- **Rust** (stable, >= 1.75)
- **GUI**: `egui` + `eframe` (glow backend)
- **2D geometry**: `euclid`
- **File I/O**: `rusqlite`, `image`, `rayon`

### 基本用法

**鼠标**

| 操作 | 效果 |
| --- | --- |
| 滚轮 | 以光标位置为锚点缩放 |
| 中键拖拽 | 平移画布 |
| 左键拖拽元素 | 移动 |
| 拖拽角点/边缘手柄 | 缩放（按住 `Ctrl` 解除等比） |
| 拖拽旋转手柄 | 旋转 |
| 左键在空白处拖拽 | 框选 |
| 双击空白处 | 新建文本便签 |
| 右键 | 打开上下文菜单 |

**键盘**

| 快捷键 | 动作 |
| --- | --- |
| `Ctrl+N` | 新建画布 |
| `Ctrl+O` | 打开 `.prz` / `.bee` 工程 |
| `Ctrl+I` | 载入图片到画布 |
| `Ctrl+V` | 从剪贴板粘贴图片 |
| `Ctrl+S` | 保存 |
| `Ctrl+Shift+S` | 另存为 |
| `Ctrl+Z` / `Ctrl+Shift+Z`（或 `Ctrl+Y`） | 撤销 / 重做 |
| `Delete` | 删除选中项 |
| `F` | 适配选中（或全画布）到屏幕 |
| `I` | 切换取色器 |
| `C` | 进入裁剪模式（仅单张图片可触发）——`Enter` 应用，`Esc` 取消 |
| `R` / `G` / `O` | 排列选中：线形 / 网格 / 最优装箱（需 ≥ 2 项） |
| `Ctrl+Shift+P` | 唤出上下文菜单 |
| `Esc` | 关闭菜单 / 取消当前操作 |

完整快捷键也展示在应用内 **Settings** 面板里（可切换语言并列出全部按键）。

欢迎页会列出最近打开的工程（持久化于 `~/.preferz/recent.json`，最多 10 项）。

### 许可

MIT
