# PReferZ — SPEC

> Rust 实现的极简参考图聚合桌面应用，设计参考 BeeRef，技术栈 egui + eframe。

## 1. 项目概述

| 项目 | 说明 |
|------|------|
| 名称 | **PReferZ**（Picture Reference Zoomer） |
| 语言 | Rust（stable，建议 >= 1.75） |
| GUI | egui + eframe（pure Rust，不引入 Qt） |
| 渲染后端 | eframe 默认 glow，后续可选 wgpu |
| 许可 | MIT（见 §8） |
| 文件格式 | 兼容 BeeRef 的 `.bee`（SQLite + sqlar），并引入自有 `.prz` 格式 |

**一句话定位**：BeeRef 的 Rust 精神继承者——启动更快、包更小、交互更跟手。

---

## 2. 功能清单

### 2.1 核心功能（MVP — Phase 1 ~ 4）

> 状态标注：✅ 已实现 / 🔶 部分实现 / ❌ 未实现

| 功能 | 状态 | 说明 | BeeRef 对应 |
|------|------|------|------------|
| 无限画布 | ✅ | 鼠标中键平移、滚轮缩放、F 键 fit | QGraphicsView 缩放/平移 |
| 图片导入 | ✅ | 拖放文件、菜单打开、剪贴板粘贴（Ctrl+V） | `view.do_insert_images()` |
| 图片项 | ✅ | 在画布上自由移动、缩放、旋转 | BeePixmapItem |
| 文本便签 | ✅ | 双击空白创建，TextEdit overlay 编辑，Enter/失焦提交，Esc 取消 | BeeTextItem |
| 选中与手柄 | ✅ | 单击选中 + 四角缩放 + 旋转手柄 + 翻转边 + 框选 | SelectableMixin |
| 撤销/重做 | ✅ | Ctrl+Z / Ctrl+Shift+Z（自定义 Command trait） | QUndoStack |
| 图层 | ✅ | 置顶/置底，Z 序管理（ReorderItems） | `max_z`/`min_z` + `Z_STEP` |
| 保存/加载 | ✅ | `.prz`/`.bee` SQLite 持久化（items + sqlar 图片 + metadata 视口），后台线程加载/保存 | SQLiteIO |

### 2.2 扩展功能（Phase 5）

| 功能 | 说明 |
|------|------|
| 反转/翻转 | 水平/垂直翻转（变换矩阵 m11/m22） |
| 裁剪 | Crop 模式，保留临时裁剪状态 |
| 透明度 | 图片整体透明度滑块 |
| 灰度 | 实时灰度滤镜（自实现 BT.601 亮度矩阵，未引入 palette crate） |
| 颜色采样 | 取色器模式，显示 RGB/HEX |
| 批量操作 | 归一化尺寸（宽/高/面积）、批量排列（线形/最优/方形） |

### 2.3 增强功能（Phase 6+）

| 功能 | 说明 |
|------|------|
| 导出 | 场景 → PNG/JPG/SVG，图片 → 目录 |
| 始终置顶 | 窗口置顶 + 无边框悬浮模式 |
| 欢迎页 | 空场景时的最近文件列表 |
| 键鼠映射 | 可自定义快捷键/鼠标行为 |
| 暗色主题 | 默认暗色，仅暗色（用户要求取消亮色选项） |

---

## 3. 技术架构

### 3.1 Crate 依赖树

```
PReferZ (binary)
├── eframe          —— 窗口管理、事件循环
├── egui            —— 即时模式 UI 渲染
├── preferz-core    —— 核心领域模型
│   ├── euclid      —— 2D 几何：Rect, Transform2D, 空间类型标签
│   ├── uuid        —— Item ID 生成
│   ├── serde(+serde_json) —— 序列化
│   └── undo        —— Cargo.toml 声明但代码未使用（撤销栈为自定义实现，见 §3.5）
├── preferz-fileio  —— 文件读写
│   ├── rusqlite    —— SQLite .bee/.prz 读写
│   ├── image       —— 图片编解码
│   ├── rayon       —— Cargo.toml 声明但代码未使用（后台解码用 std::thread 实现）
│   ├── thiserror/log —— 错误处理/日志
│   └── serde(+serde_json) —— 序列化
├── rfd             —— 原生文件对话框
├── arboard         —— 剪贴板读取（Ctrl+V 粘贴图片）
├── image           —— 图片解码（PReferZApp 直接调用，同步）
└── env_logger/log  —— 日志
```

> **未引入的依赖**（spec 原列但 Cargo.toml 实际没有）：`palette`（灰度滤镜，已自实现 BT.601 矩阵替代）、`exif`（EXIF 方向）、`resvg+usvg`（SVG 导出）、`confy`（配置持久化）、`rect_packer`（矩形装箱，已自实现 MaxRects 替代）。Phase 5 灰度与装箱均改为自实现，未引入对应 crate。

### 3.2 核心模块架构

```
┌────────────────────────────────────────────┐
│  eframe::App (PReferZApp)                  │
│  • 持有 Scene、UndoStack、ViewportState    │
│  • update() 每帧调度                        │
├────────────────────────────────────────────┤
│  UI Layer (内联在 preferz_app.rs)           │
│  ├── StatusBar     (底部状态栏 + flash 消息)│
│  ├── ContextMenu    (右键菜单，Area + 按钮) │
│  ├── DebugPanel    (Debug 窗口)             │
│  └── TransformHandles (ui/widgets/，手柄)   │
├────────────────────────────────────────────┤
│  Canvas Layer (画布渲染，CentralPanel)      │
│  ├── ViewportState (平移/缩放/坐标转换)     │
│  ├── render_scene (item 遍历、视口剔除、Z序)│
│  ├── Interaction   (get_item_at 命中检测)   │
│  └── DragState     (Idle/HandleTransform/  │
│                     MoveItems 状态机)       │
├────────────────────────────────────────────┤
│  Domain Layer (preferz-core)                │
│  ├── Item          (PixmapItem / TextItem)  │
│  ├── Transform     (位置/缩放/旋转/翻转)    │
│  ├── Scene         (items + selection +     │
│  │                 next_z，命中与 Z 序)     │
│  ├── Selection     (struct 保留但未启用)    │
│  ├── Commands      (自定义 Command trait)   │
│  ├── Arrange       (排布算法，返回 moves)   │
│  └── Spaces        (CanvasSpace/ScreenSpace/│
│                     ItemLocalSpace 类型标签)│
├────────────────────────────────────────────┤
│  I/O Layer (preferz-fileio)                 │
│  ├── BeeFile       (.bee/.prz 读写)         │
│  ├── ImageLoader   (同步解码 + 缓存，未启用)│
│  ├── Export        (空实现，未启用)         │
│  └── Schema        (SQLite schema 常量)     │
└────────────────────────────────────────────┘
```

> **纹理管理**：无独立 Assets 模块，`texture_cache: HashMap<u64, egui::TextureHandle>` 内联在 `PReferZApp` 上。

### 3.3 画布渲染策略

egui 是 immediate-mode，所有可见物每帧重画。规避性能问题的策略：

1. **Viewport Culling**：只画屏幕范围内（加 margin）的 item。
2. **纹理缓存**：所有图片预先上传为 `egui::TextureHandle`，渲染时只引用 ID。
3. **LOD**：缩小到一定阈值用缩略图纹理，避免大图缩放浪费。
4. **脏标记**：内容未变时跳过纹理重新上传。
5. **若 egui 渲染成瓶颈**：可用 `eframe` 的 `glow` 后端直接注入自定义 OpenGL 渲染，画布区绕过 egui 自己画，egui 只管 UI 面板。

### 3.4 数据模型（核心 struct）

> **坐标空间类型系统**（`spaces.rs`）：用 euclid 空间标签避免裸 f32 跨空间换算。
> - `CanvasSpace`：画布世界坐标
> - `ScreenSpace`：屏幕像素坐标
> - `ItemLocalSpace`：item 局部坐标（原点为 transform.pos，未旋转/缩放的左上角）
>
> 类型别名：`CanvasPoint`/`CanvasVector`/`CanvasRect`、`ScreenPoint`/`ScreenVector`/`ScreenRect`、`CanvasToScreen`/`ScreenToCanvas`、`ItemLocalToCanvas`/`CanvasToItemLocal`。

```rust
// === preferz-core ===

pub struct Transform {
    pub pos: CanvasVector,         // 视口中心在画布空间的坐标
    pub scale: CanvasVector,       // 1.0 = 原始
    pub rotation: f32,             // 弧度，<-- 选中手柄调整
    pub flip_h: bool,
    pub flip_v: bool,
}

pub enum ItemKind {
    Pixmap {
        texture_id: u64,               // 逻辑 ID，由 PReferZApp.texture_cache 映射到 egui::TextureHandle
        filename: Option<String>,
        original_size: (u32, u32),
        opacity: f32,                  // 0.0 ~ 1.0
        grayscale: bool,
        crop: Option<CropRect>,
    },
    Text {
        content: String,
        font_size: f32,
        color: [u8; 4],                // RGBA，UI 层转 egui::Color32（preferz-core 不依赖 egui）
        editing: bool,
    },
}

pub struct Item {
    pub id: ItemId,          // UUID
    pub kind: ItemKind,
    pub transform: Transform,
    pub z: i32,              // Z 序
}

pub struct Scene {
    pub items: Vec<Item>,
    pub selection: HashSet<ItemId>,
    pub next_z: i32,
}

// === preferz (binary) ===

pub struct ViewportState {
    pub pan: CanvasVector,    // 视口中心在画布空间的坐标（非 ScreenSpace）
    pub zoom: f32,
    pub min_zoom: f32,
    pub max_zoom: f32,
    pub screen_rect: egui::Rect,  // 画布面板的屏幕矩形（CentralPanel 每帧更新）
}
```

### 3.5 撤销栈设计

> **不使用 `undo` crate**。Cargo.toml 声明了 `undo = "0.4"` 但代码零引用——实际为自定义 `Command` trait + `UndoStack` struct，以支持 `skip_first_redo` 预览模式（`undo` crate 无此内置机制）。

```rust
// === preferz-core: commands.rs ===

/// 命令 trait。skip_first_redo 用于交互预览模式（见 §5.1 注意事项 #5）。
pub trait Command {
    fn redo(&mut self, scene: &mut Scene);
    fn undo(&mut self, scene: &mut Scene);
    fn skip_first_redo(&self) -> bool { false }
}

// 已实现的 Command：
// - TransformItem   (单 item 的 old/new transform，预览模式)
// - MoveItems       (多 item 平移 delta，预览模式)
// - ScaleItems      (无锚点批量缩放 factor，键盘快捷键用)
// - RotateItems     (无锚点批量旋转 angle，键盘快捷键用)
// - FlipItems       (水平/垂直翻转，自反)
// - DeleteItems     (首次 redo 抓快照，undo 用 add_item_preserve_z 恢复)
// - AddItem         (redo: add_item；undo: remove_item)
// - ReorderItems    (置顶/置底，记录 old_z/new_z)
// - ArrangeItems    (moves: Vec<(id, old_pos, new_pos)>，预览模式)

// === preferz: preferz_app.rs ===

struct UndoStack {
    undo: Vec<Box<dyn Command>>,
    redo: Vec<Box<dyn Command>>,
}

impl UndoStack {
    fn push(&mut self, mut cmd: Box<dyn Command>, scene: &mut Scene) {
        let skip = cmd.skip_first_redo();
        if !skip {
            cmd.redo(scene);  // 普通命令：push 时立即应用
        }
        // 预览命令：交互中已直接改 item，跳过首次 redo 避免二次应用
        self.redo.clear();
        self.undo.push(cmd);
    }
    fn undo(&mut self, scene: &mut Scene) -> bool { /* ... */ }
    fn redo(&mut self, scene: &mut Scene) -> bool { /* ... */ }
}
```

`UndoStack` 与 QUndoStack 语义 1:1：`push(cmd)` / `undo()` / `redo()`。预览模式的 `skip_first_redo` 字段在各 Command struct 上（命名为 `preview_already_applied: bool`），通过 `skip_first_redo()` 方法返回。

---

## 4. 开发计划

### Phase 1 · 骨架（预计 1–2 周）

- [x] `cargo init preferz`，引入 eframe + egui
- [x] 基本窗口：标题 "PReferZ"，默认分辨率 1200x800，暗色主题
- [x] 中央画布区域：`CentralPanel`，支持鼠标中键平移 + 滚轮缩放
- [x] 菜单栏骨架：文件（打开/保存/退出）、编辑（撤销/重做）、视图（fit/重置缩放）

**可交付**：跑起来一个窗口，能平移缩放一张空白画布。

**完成时间**：2026-07-07

### Phase 2 · 图片项（预计 2–3 周）

- [x] `preferz-core` crate：`Item`、`Transform`、`Scene` 定义
- [x] 纹理管理：内联在 `PReferZApp.texture_cache`（简化实现，使用色块占位）
- [x] 图片导入：通过 `rfd::FileDialog` 打开文件 → `image` 解码 → 创建 `PixmapItem`
- [x] 画布渲染：遍历 scene.items，显示色块（简化）
- [x] 拖放支持：`eframe` 的拖放回调 → 导入图片
- [x] 点击选中 item：hit test（Transform 逆变换 + 矩形包含判断）

**可交付**：能导入图片、在画布上显示（色块）、单击选中。

**当前状态**：已完成
**开始时间**：2026-07-07
**完成时间**：2026-07-11

### Phase 3 · 交互（预计 3–4 周）

- [x] 选中手柄：四角缩放、旋转手柄、翻转边（FlipH/FlipV 手柄点击即触发）
- [x] 框选：空白处左键拖拽成矩形框，Shift 加选
- [x] 多选：Scene.selection HashSet + selection_bounding_rect + 统一外框渲染
- [x] 缩放/旋转数学：以对角为锚点缩放、以中心为锚点旋转（自由函数 apply_scale_drag / apply_rotate_drag）
- [x] 文本便签：双击空白创建 `TextItem`，TextEdit overlay 编辑，Enter/失焦提交，空内容丢弃
- [x] 图层操作：置顶/置底、Z 序重排（ReorderItems 命令）
- [x] 删除选中项（Delete 键）

**可交付**：画布上能做 BeeRef 能做的基本交互。

### Phase 4 · 持久化（预计 2–3 周）

- [x] `preferz-fileio` crate：`BeeFile` struct
- [x] 读取 .bee/.prz：`rusqlite` 解析 items + sqlar 表，按 `items` 数据重建 scene，sqlar 返回图片字节映射
- [x] 保存 .bee/.prz：事务内全量替换 items + 增量同步 sqlar（写入新图 + 清理孤儿），最后 VACUUM
- [x] 图片加载器：`std::thread::spawn` 后台解码（导入 + 加载均不阻塞 UI），`ctx.request_repaint()` 通信
- [x] 进度条对话框：`BackgroundOps` 管理导入/加载/保存三类后台任务，pending>0 时显示 Spinner
- [x] 视口状态持久化：`.prz` metadata 表存 pan/zoom，加载时恢复

**可交付**：能保存/加载 `.prz` 项目文件（含图片 + 文本 + 视口），后台线程不阻塞 UI。

**完成时间**：2026-07-11

### Phase 5 · 撤销与高级操作（预计 2–3 周）

- [x] 集成自定义 Command trait + UndoStack（`MoveItems`/`TransformItem`/`ScaleItems`/`RotateItems`/`FlipItems`/`DeleteItems`/`AddItem`/`ReorderItems`/`ArrangeItems`/`SetPixmapProps`/`CropItems`/`NormalizeItems`）
- [x] 交互过程：拖拽/缩放时为"预览模式"（直接改 item），松手时 push undo command
- [x] Ctrl+Z / Ctrl+Shift+Z 绑定
- [x] 灰度、透明度、裁剪（Crop）
  - 灰度：懒生成灰度纹理缓存，BT.601 亮度矩阵 `Y = 0.299R + 0.587G + 0.114B`（自实现，未引入 palette crate）
  - 透明度：egui `painter().image()` 的 `tint_color` alpha 通道
  - 裁剪：UV 坐标叠加 crop 子矩形 + 交互式 4 角手柄拖拽（CropMode 状态机，Enter 应用 / Esc 取消），`CropItems` 命令支持 undo
- [x] 批量操作：排布算法 `plan_arrange`（Linear/Grid/Optimal），UI 已接入右键菜单（R/G/O 快捷键）
- [x] 排布算法：最大矩形装箱（自实现 MaxRects + BSSF 启发式 + split + prune，未引入 rect_packer）
- [x] 颜色采样：取色器模式（I 键切换），local_to_canvas 逆变换 → 像素索引 → RGBA 读取，overlay 显示 RGB/HEX/位置
- [x] 归一化尺寸：`NormalizeItems` 命令（Width/Height/Area 三模式，保持宽高比的等比 factor）+ 右键菜单

**可交付**：完整的撤销栈 + 高级变换功能 + 图片属性编辑（灰度/透明度/裁剪）+ 批量排布与归一化 + 颜色采样。

**完成时间**：2026-07-20

### Phase 6 · 打磨（预计 2 周）

- [ ] 导出：场景 → PNG/JPG/SVG（resvg）
- [ ] 始终置顶 + 无边框模式
- [ ] 欢迎页 + 最近文件列表
- [x] 设置面板（简化版：排列间距 + 快捷键说明；存储格式/内存上限未实现）
- [ ] 键鼠映射可配置（confy 持久化）
- [x] 主题：仅暗色（用户要求去掉亮色选项，`main.rs` 中固定 `egui::Visuals::dark()`）

**可交付**：面向用户发布的首个 alpha 版本。

---

## 5. 注意事项（坑与约定）

### 5.1 技术风险

1. **egui immediate-mode 性能**：数百张图时每帧全画不可行，必须 viewport culling。MVP 阶段先实现裁剪就行，后续按需加 LOD。
2. **纹理管理**：egui 的 `TextureHandle` 绑定 `egui::Context`，需注意生命周期——在 `update()` 中创建/释放纹理，不要跨帧持有裸 `TextureId` 不注册。
3. **画布坐标系统**：三种坐标空间——`CanvasSpace`（画布世界）、`ScreenSpace`（屏幕像素）、`ItemLocalSpace`（item 局部，原点为 transform.pos）。用 `euclid` 的空间类型标签确保类型安全，严禁裸 f32 跨空间换算。`ViewportState` 提供 `screen_to_canvas`/`canvas_to_screen` 互逆转换，`Item::local_to_canvas()` 提供 item 局部到画布的仿射变换（含 flip/scale/rotate/translate）。
4. **文本编辑**：egui 对嵌入式富文本编辑支持有限。`BeeTextItem` 的编辑态需用 egui 的 `TextEdit` widget 在正确的画布位置 overlay 绘制。
5. **撤销栈的"预览"模式**：BeeRef 遵循"交互中预览（直接改 item）→ 松手 push undo command（skip first redo）"。项目不使用 `undo` crate，而是自定义 `Command` trait（`skip_first_redo()` 方法）+ `UndoStack`（`push` 时读取该方法决定是否跳过首次 redo）。各 Command struct 上的字段命名为 `preview_already_applied: bool`。
6. **.bee 兼容性**：BeeRef 的 `USER_VERSION=2`，需正确实现 SQLite 迁移逻辑。`sqlar` 表的 `sz`（压缩前大小）和 `data`（压缩后 blob）字段要处理对。

### 5.2 工程约定

1. **所有 Item 变换走 undo 栈**，不直接改 item 属性。
2. **Item ID 用 `uuid::Uuid`**，不依赖 BeeRef 的整数自增 id（uuid 更安全，且 Rust 生态习惯）。
3. **图片解码进后台线程**：`PReferZApp::BackgroundOps` 用 `std::thread::spawn` + `mpsc::channel` + `ctx.request_repaint()` 实现导入解码 / 文件加载 / 文件保存三类后台任务，UI 线程在 `update` 开头 `poll_background` 取结果。`preferz-fileio::ImageLoader` 有同步缓存实现但未被二进制引用（后台逻辑直接在 app 层实现）。
4. **单测覆盖核心逻辑**：transform 数学、hit test、scene 操作、bee 文件读写。
5. **Dependency 尽量少**：Rust 生态优势之一就是依赖树浅，别引入不必要的重量级框架。

### 5.3 与 BeeRef 的差异

| 方面 | BeeRef (PyQt6) | PReferZ (egui) |
|------|---------------|----------------|
| 渲染方式 | Retained mode (QGraphicsView) | Immediate mode (egui) |
| 性能策略 | Qt 自动裁剪/缓存 | 需手动 culling + LOD |
| GUI 控件 | QMenu/QDialog 原生 OS 风格 | egui 自绘，跨平台一致 |
| 文件对话框 | 原生 QFileDialog | rfd（也是原生） |
| 撤销栈 | QUndoStack（内置预览模式） | undo crate（需手动预览） |
| 文本编辑 | QGraphicsTextItem 原生编辑 | egui TextEdit overlay |
| 默认文件格式 | .bee | 兼容 .bee + 自有 .prz |
| 包体积 | 50–100MB+ | 目标 5–15MB |

### 5.4 自有格式 .prz

兼容 BeeRef `.bee` 作为读取来源，同时设计自有格式：

- 格式：与 `.bee` 同为 SQLite 数据库
- 扩展列：`USER_VERSION` 从 3 开始，增加 `metadata` 表（视口状态、主题、最后编辑时间等）
- 查看方式一致：`sqlite3 xxx.prz -Axv`

---

## 6. 仓库结构

```
preferz/
├── Cargo.toml              # workspace
├── Cargo.lock
├── README.md
├── LICENSE                 # MIT
├── .gitignore
├── AGENTS.md               # agent 工作约定
├── REVIEW-2026-07-11.md    # 代码审查报告 + 修复清单
├── .agents/
│   └── preferz-spec.md     # 本文档
├── assets/
│   └── simhei.ttf          # 中文字体
├── crates/
│   ├── preferz/            # 二进制入口 (eframe app)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs             # eframe::run_native()
│   │       ├── lib.rs              # 模块声明
│   │       ├── preferz_app.rs      # PReferZApp: eframe::App impl + UndoStack + DragState
│   │       ├── viewport.rs         # ViewportState + 坐标转换
│   │       ├── interaction.rs      # get_item_at 命中检测
│   │       └── ui/
│   │           ├── mod.rs
│   │           └── widgets/
│   │               ├── mod.rs
│   │               └── transform_handles.rs  # 变换手柄
│   ├── preferz-core/       # 领域模型
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── item.rs             # Item / ItemKind / ItemLocalSpace
│   │       ├── transform.rs        # Transform
│   │       ├── spaces.rs           # CanvasSpace / ScreenSpace / ItemLocalSpace 类型标签
│   │       ├── scene.rs            # Scene (items + selection + next_z)
│   │       ├── selection.rs        # Selection struct（保留但未启用）
│   │       ├── commands.rs         # 自定义 Command trait + 9 个 Command 实现
│   │       └── arrange.rs          # plan_arrange 排布算法
│   └── preferz-fileio/     # 持久化
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── bee.rs              # .bee / .prz 读写
│           ├── schema.rs           # SQLite schema 常量
│           ├── export.rs           # Exporter（空实现）
│           └── image.rs            # ImageLoader（同步缓存，未启用）
└── tests/                  # 集成测试
```

---

## 7. 关键技术方案细节

### 7.1 Viewport 坐标转换

`ViewportState` 的方法（非自由函数），`pan` 为画布空间：

```rust
// 画布点 → 屏幕点（egui::Pos2）
fn canvas_to_screen(&self, canvas_pos: CanvasPoint) -> egui::Pos2 {
    let center = self.screen_rect.center();
    let scaled = (canvas_pos.to_vector() - self.pan) * self.zoom;
    egui::Pos2::new(center.x + scaled.x, center.y + scaled.y)
}

// 屏幕点 → 画布点（与 canvas_to_screen 严格互逆）
fn screen_to_canvas(&self, screen_pos: egui::Pos2) -> CanvasPoint {
    let center = self.screen_rect.center();
    let unscaled = (screen_pos - center) / self.zoom;
    CanvasPoint::new(unscaled.x + self.pan.x, unscaled.y + self.pan.y)
}

// 以屏幕点为锚点缩放（量纲：screen_pos - center 是屏幕像素，factor 是 1/zoom 差）
fn zoom_at(&mut self, delta: f32, screen_pos: egui::Pos2) {
    let new_zoom = (self.zoom * 1.02_f32.powf(delta.clamp(-1.0, 1.0)))
        .clamp(self.min_zoom, self.max_zoom);
    let center = self.screen_rect.center();
    let factor = 1.0 / self.zoom - 1.0 / new_zoom;
    self.pan.x += (screen_pos.x - center.x) * factor;
    self.pan.y += (screen_pos.y - center.y) * factor;
    self.zoom = new_zoom;
}
```

### 7.2 Hit Testing

命中检测为 `Item::contains_canvas_point` 方法（OBB，正确处理旋转/翻转/缩放）。
`interaction::get_item_at` 按 Z 序倒序查找命中的最顶层 item：

```rust
// Item::contains_canvas_point —— OBB 命中
pub fn contains_canvas_point(&self, canvas_pos: CanvasPoint) -> bool {
    let inv = match self.local_to_canvas().inverse() {
        Some(inv) => inv,
        None => return false,
    };
    let local = inv.transform_point(canvas_pos);
    let size = self.base_size();
    // 局部空间下的命中矩形：[0, size.x] x [0, size.y]
    local.x >= 0.0 && local.x <= size.x
        && local.y >= 0.0 && local.y <= size.y
}

// interaction::get_item_at —— 从顶层到底层查找
pub fn get_item_at<'a>(
    screen_pos: egui::Pos2,
    scene: &'a Scene,
    viewport: &ViewportState,
) -> Option<&'a Item> {
    let canvas_pos = viewport.screen_to_canvas(screen_pos);
    for item in scene.items_by_z_order().iter().rev() {
        if item.contains_canvas_point(canvas_pos) {
            return Some(item);
        }
    }
    None
}
```

### 7.3 背景线程读取 .bee

```rust
// 伪代码：用 egui_ctx.request_repaint 驱动画布刷新
fn open_bee_file(app: &mut PReferZApp, path: &Path, egui_ctx: &egui::Context) {
    let path = path.to_path_buf();
    let tx = app.loader_tx.clone();
    let ctx = egui_ctx.clone();
    std::thread::spawn(move || {
        let result = BeeFile::read(&path);
        tx.send(LoadResult { path, result }).ok();
        ctx.request_repaint(); // 通知 egui 刷新
    });
    app.loading = true;
}
```

---

## 8. 开源许可建议

### 推荐：**MIT**

| 因素 | 分析 |
|------|------|
| 与 GPL 关系 | BeeRef 是 GPL-3.0，但 PReferZ 是独立实现（未复制代码），不受 GPL 传染。MIT 更宽松，不影响其他项目的引用。 |
| Rust 生态惯例 | Rust 生态默认 MIT 或 MIT/Apache-2.0 双许可。`egui`、`winit`、`rusqlite` 都是 MIT/Apache-2.0。 |
| 工具兼容 | `cargo init` 默认模板用 MIT 或 Apache-2.0。 |
| 企业友好 | MIT 无 copyleft 条款，企业可以直接集成。 |
| 简单 | 协议全文只有几行，没有律师语言，适合小团队/个人项目。 |

**备选**：MIT + Apache-2.0 双许可（Rust 社区最流行）。Apache-2.0 增加了专利授权保护，作为双许可的补充项几乎无成本。

**不推荐 GPL**：BeeRef 是 GPL-3.0，但 PReferZ 不是衍生作品——它是独立设计。选 GPL 会限制集成场景（比如有人想在你的渲染层上做插件系统），对 Rust 桌面应用通常不划算。

**结论**：用 MIT，简单又干净。如果想紧跟 Rust 社区惯例，加 Apache-2.0 做双许可也完全可以。

---

## 9. Cargo.toml 骨架参考

实际使用 workspace 结构，根 `Cargo.toml` 声明 workspace 成员和共享依赖版本：

```toml
# 根 Cargo.toml
[workspace]
members = [
    "crates/preferz",
    "crates/preferz-core",
    "crates/preferz-fileio",
]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
description = "Picture Reference Zoomer — a minimal reference image board"

[workspace.dependencies]
# Core
euclid = { version = "0.22", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
undo = "0.4"                    # 声明但未使用（撤销栈为自定义实现）

# GUI
eframe = "0.29"
egui = "0.29"

# File I/O
rusqlite = { version = "0.31", features = ["bundled"] }
image = "0.25"
rayon = "1.10"                  # 声明但未使用（后台解码为 Phase 4+ 任务）

# Utilities
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
log = "0.4"
rfd = "0.14"
arboard = "3"                   # 剪贴板粘贴图片（Ctrl+V）
```

各子 crate 用 `dependency.workspace = true` 引用。`preferz` 二进制额外有 `env_logger = "0.11"`。

> **未引入的依赖**（Phase 5/6 计划）：`palette`（灰度，已自实现 BT.601 矩阵替代）、`exif`（EXIF 方向）、`resvg+usvg`（SVG 导出）、`confy`（配置持久化）、`rect_packer`（矩形装箱，已自实现 MaxRects 替代）。

---

## 附录 A. 参考资源

| 项目 | 链接 | 说明 |
|------|------|------|
| BeeRef | https://github.com/rbreu/beeref | 原始参考项目 |
| egui | https://github.com/emilk/egui | GUI 框架 |
| eframe | https://docs.rs/eframe | egui 的桌面后端 |
| euclid | https://docs.rs/euclid | Mozilla 2D 几何库 |
| undo | https://docs.rs/undo | 撤销重做栈 |
| rusqlite | https://docs.rs/rusqlite | SQLite 绑定 |
| rfd | https://docs.rs/rfd | 原生文件对话框 |
| cargo-dist | https://opensource.axo.dev/cargo-dist | 跨平台分发工具 |
