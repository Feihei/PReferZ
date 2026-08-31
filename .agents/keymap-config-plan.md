# PReferZ Phase 6 收尾：键鼠映射可配置 实施计划

> **状态（2026-08-30）**：已定稿（D1/D2/D3 已拍板），待实施。文档先提交，实现拆 commit。
> **配套**：`.agents/preferz-spec.md` §2.3（增强功能「键鼠映射」）、§6 开发计划 Phase 6 最后一项。
> **惯例**：零新依赖（与自实现 BT.601 灰度、MaxRects 装箱、deflate 字体压缩同一路子）。

**Goal:** 键盘快捷键可由用户重绑定，持久化到 `~/.preferz/config.json`；
设置面板内可视化修改、检测冲突、可恢复默认。**本次只做键盘**，鼠标行为（滚轮、
中键平移、双击）保持硬编码不动。

**Architecture:** 新增 `BindKey`（自有可序列化按键枚举）/ `KeyBind`（键 + 修饰键）/
`Action`（可绑定动作枚举）/ `Keymap`（双向映射）四个类型，放在 binary 层
（`preferz/src/keymap.rs`）；派发层把 41 处硬编码 `egui::Key::*` 换成
`keymap.pressed(Action::X, ctx)`；`UserConfig` 增加 `keymap` 字段走既有手写 JSON
持久化；设置面板新增「快捷键」分组，说明文案由绑定动态生成。

---

## 1. 现状盘点

### 1.1 键盘：41 处硬编码，集中在 3 个函数

| 位置 | 行 | 绑定 |
|---|---|---|
| `handle_shortcuts` | 3492 | `F5` 进入/退出 Present |
| | 3519 | `Esc` 绘制工具 → 回 Select |
| | 3527/3531 | `Enter` 应用裁剪 / `Esc` 取消裁剪 |
| | 3541 | `Ctrl+Shift+P` 上下文菜单 |
| | 3550 | `Esc` 关菜单 / 退出取色器 |
| | 3559 | `Delete` 删除选中 |
| | 3564 | `Ctrl+Z` 撤销 |
| | 3572 | `Ctrl+Y` / `Ctrl+Shift+Z` 重做 |
| | 3583–3597 | `Ctrl+S` / `Ctrl+Shift+S` / `Ctrl+O` / `Ctrl+I` / `Ctrl+N` |
| | 3607 | `Ctrl+V` 粘贴（用 `key_released`，见 §1.3） |
| | 3612 | `F` 适应画布 |
| | 3620 | `C` 裁剪模式 |
| | 3626 | `I` 取色器 |
| `tool_switch_shortcut` | 3643–3658 | `V` Select / `R` 矩形 / `O` 椭圆 / `D` 菱形 / `L` 线 / `A` 箭头 / `M` Frame |
| `handle_present_input` | 2189–2197 | `→`/`Space`/`PageDown` 下一页，`←`/`PageUp` 上一页，`Home` 首页，`End` 末页，`Esc`/`F5` 退出 |

### 1.2 鼠标：结构性手势，不宜全量重映射

| 行为 | 位置 | 现状 |
|---|---|---|
| 中键拖拽平移 | 1062 | `response.dragged_by(PointerButton::Middle)` |
| 滚轮缩放（锚定指针） | 1067–1072 | `raw_scroll_delta` → `viewport.zoom_at` |
| 左键按下/拖拽/释放 | 1174–1249 | 选择（`Shift` 加选）、移动、`Ctrl` 自由缩放 |
| 双击 | 1077 | Text 编辑 / 闭合 Shape 绑文本 / 空白建便签 |
| 右键 | — | 上下文菜单 |

### 1.3 两个必须绕开的坑

1. **`Ctrl+V` 无法用 `key_pressed` 检测**。egui-winit 0.29 的 `is_paste_command`
   在 `pressed=true` 时拦截 V：剪贴板有文本 → 发 `Event::Paste`；是图片 →
   **什么都不发**。故现有代码用 `key_released(V) && modifiers.ctrl`。
   → `KeyBind` 必须支持「按下触发」和「释放触发」两种语义，否则 V 改绑后会失效。
2. **修饰键语义**。现代码是「含该修饰键即匹配」（`Ctrl+Z` 只额外排除了 shift，
   不排除 alt）。可配置后必须改成**严格匹配**（ctrl/shift/alt 三者全等），
   否则 `Ctrl+Alt+S` 会误触发 `Ctrl+S`。这是行为变更，需写进验收清单。

### 1.4 会「说谎」的静态文案

`i18n.rs` 有 4 组（中英各 4 条）**硬编码**快捷键说明：

```
T::SettingsShortcutArrange => "V/R/O/D 工具切换 · C 裁剪 · I 取色器 · F 适应画布"
T::SettingsShortcutUndo    => "Ctrl+Z 撤销 · Ctrl+Shift+Z 重做"
T::SettingsShortcutFile    => "Ctrl+N 新建 · Ctrl+O 打开 · Ctrl+I 载入图片"
T::SettingsShortcutPaste   => "Ctrl+V 粘贴图片 · Ctrl+S 保存 · Ctrl+Shift+S 另存为"
```

一旦可配置这些都变成错的。改为**按当前绑定动态拼接**：
`t(lang, T::ActionLabel(action))` + `bind.display()`。需要给 `Action` 各变体
补一对中英文标签。

另：右键菜单项也显示快捷键后缀（`i18n.rs:32` 注释「含快捷键后缀」），同样要动态化。

---

## 2. 决策记录（2026-08-30 拍板）

### D1 · `Esc` / `F5` 这类上下文相关键 → **D1-a：可改绑 + 硬兜底**

允许把 `Esc` 绑给别的动作，但保留一条硬编码兜底：**无论怎么改，`Esc` 永远能
退出裁剪模式 / 绘制工具 / 取色器**。理由：这三个模式一旦进得去出不来就只能
重启，风险不对称。

实现：`handle_shortcuts` 里这三处取消分支在查 `Keymap` **之前**先无条件检查
`key_pressed(Escape)`。即 `Esc` 是「取消」动作的默认绑定，同时额外兜底。

### D2 · 冲突处理 → **D2-a：提示 + 允许覆盖**

改绑时若新组合已被别的动作占用：UI 标红并写明占用者，确认后覆盖，被覆盖的
动作变为「未绑定」。不自动交换（用户未必预期），不硬性拒绝（太繁琐）。

### D3 · 鼠标映射 → **D3-c：本次只做键盘**

滚轮缩放、中键平移、双击建便签等鼠标行为**保持硬编码不动**。理由：双击/拖拽与
画布交互状态机深度耦合，重映射易引入回归，收益不抵风险。spec §2.3 的
「鼠标行为」部分留待将来，本计划 §1.2 的盘点表即为那时的起点。

---

## 3. 数据模型

```rust
/// 自有可序列化按键——egui::Key 不实现 Serialize，且需要稳定落盘表示。
/// 只需覆盖「可绑定」的子集，不必镜像 egui::Key 全部变体。
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum BindKey {
    A, B, /* … */ Z,
    Num0, /* … */ Num9,
    F1, /* … */ F12,
    Space, Enter, Escape, Tab, Backspace, Delete, Insert,
    Home, End, PageUp, PageDown,
    ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
}

/// 修饰键严格匹配（ctrl/shift/alt 三者全等），见 §1.3 坑 2。
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct KeyBind {
    pub key: BindKey,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// true = 释放时触发。目前只有 Ctrl+V 需要（egui-winit 拦截 key_pressed）。
    pub on_release: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Action { /* 见 §1.1 表，约 27 个变体 */ }

pub struct Keymap {
    forward: HashMap<Action, KeyBind>,
    /// 反查索引：(KeyBind, on_release) -> Action，改绑时重建
    reverse: HashMap<(KeyBind /* 忽略 on_release */), Action>,
}
```

`UserConfig` 扩展（按 D3-c 不含鼠标项）：

```rust
#[derive(Serialize, Deserialize, Default)]
struct UserConfig {
    #[serde(default)] lang: Lang,
    #[serde(default)] keymap: HashMap<Action, KeyBind>,   // 缺省项用内置默认值补
}
```

用 `HashMap` 而非具名 struct：新增 `Action` 变体时老配置文件不会解析失败，
缺失项回落到内置默认值。

---

## 4. 实施步骤

- [ ] **S1 数据模型**：新增 `preferz/src/keymap.rs`，含 `BindKey` / `KeyBind` /
      `Action` / `Keymap` / `MousePrefs`，默认表 + `to_egui()` / `from_egui()` 双向转换
- [ ] **S2 持久化**：`UserConfig` 加 `keymap` / `mouse` 字段；加载时缺失项用默认值补齐；
      单测：序列化往返、老配置（无 keymap 字段）能正确加载
- [ ] **S3 派发改造**：41 处硬编码 → `keymap.pressed(Action::X, ctx)`，
      修饰键改严格匹配；保留现有模式守卫顺序（文本编辑 → 工具 → 裁剪 → 通用）
- [ ] **S4 文案动态化**：4 条 `SettingsShortcut*` + 右键菜单后缀改为按绑定生成；
      给 `Action` 各变体补中英文标签
- [ ] **S5 设置面板 UI**：快捷键分组（滚动列表，每行「动作 + 当前绑定 + 改绑按钮」）；
      改绑用 `ctx.input(|i| i.events)` 捕获 `Event::Key`（避免轮询 60 个键）；
      冲突标红提示占用者（D2-a），确认后覆盖；「恢复默认」按钮
- [ ] **S6 `Esc` 硬兜底**：按 D1-a，三处取消分支先无条件检查 `Escape` 再查 Keymap
- [ ] ~~S7 鼠标映射~~：**本次不做**（D3-c）

---

## 5. 验收清单

1. 改绑 `Ctrl+S` → `Ctrl+Alt+S`，重启后仍生效；`Ctrl+S` 不再触发保存
2. 改绑后**未重启**立即生效
3. `Ctrl+Alt+S` **不会**误触发仍绑定 `Ctrl+S` 的动作（严格匹配，§1.3 坑 2）
4. `Ctrl+V` 粘贴图片仍可用（释放触发语义，§1.3 坑 1）
5. 设置面板与右键菜单显示的快捷键与实际绑定一致（不再是硬编码文案）
6. 恢复默认后所有绑定回到出厂值
7. 删除 `~/.preferz/config.json` 后启动，全部用默认值且不报错
8. 老配置（只有 `lang` 字段）能正常加载，keymap 回落默认
9. 改绑成冲突组合时 UI 标红并写明占用者，确认后原动作变「未绑定」（D2-a）
10. 把 `Esc` 绑给别的动作后，仍能退出裁剪模式 / 绘制工具 / 取色器（D1-a）
11. 鼠标行为未受影响：滚轮缩放、中键平移、双击建便签仍照旧（D3-c 回归项）
