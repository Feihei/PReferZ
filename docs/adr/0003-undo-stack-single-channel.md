# ADR-0003: undo 栈是 item 变更的唯一通道

- 状态：已接受
- 日期：2026-07（项目立项），2026-08-30 补 preview 语义
- 参考：AGENTS.md「Must-Know Conventions」、`preferz-core/src/commands.rs`

## 背景

egui 是 immediate mode，交互预览（拖拽中）希望"直接改、所见即所得"，但 undo 又要求
每次变更可逆。若 UI 层随手改 `Item.transform`，undo 链必然漏记。

## 决策

- **所有 item 变更必须走 undo 栈（`push_cmd`）**，UI 层不得直接改 `Item.transform`
  或 `Scene.items`
- 全手写 undo：`preferz-core::commands::Command` trait + binary 层 `UndoStack`；
  `undo` crate 虽在依赖里但**零引用**（历史遗留，待清理）
- **preview 模式**：交互拖拽直接改 item（渲染即时反馈），释放时 `push(cmd)` 并
  `skip_first_redo: true`（预览已应用，重做不能重复应用一次）。`undo` crate 无此字段，
  这是我们手写的核心原因之一

## 后果

- ✅ 拖拽流畅（无命令对象分配在拖拽路径上）且 undo 完整
- ✅ 命令可合并（滑块连续修改合并为单条命令，Phase H 侧栏沿用）
- ⚠️ 每个 UI 交互都要想清楚"命令是什么"，不能图省事直改
