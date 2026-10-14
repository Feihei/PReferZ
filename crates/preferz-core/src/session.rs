//! Headless 文档会话：把 [`Scene`]、[`UndoStack`]、[`ViewportState`] 组装成
//! 一个与 UI 无关的编辑会话，供 egui 前端与 CLI 前端共享。
//!
//! 归属 L1（core-sink discipline）：本模块零 egui / 零 I/O 依赖，可在无头环境
//! 下直接驱动（CLI、集成测试）。文件持久化由 `preferz-fileio` 在会话之外完成。

use crate::commands::Command;
use crate::scene::Scene;
use crate::viewport::ViewportState;

/// Undo 栈。`push` 会读 [`Command::skip_first_redo`]：
/// - 交互预览命令（拖拽中已直接改 item）返 `true` 跳过首次 redo；
/// - 普通命令返 `false`，push 时立即 redo 应用变更。
///
/// 这让 AGENTS.md Gotcha #5（skip_first_redo）真正生效（S5/M7）。
///
/// 历史上该类型住在 binary 层 `preferz_app/mod.rs`；CLI 化改造时下沉到 core，
/// 因为它只依赖 `Command` + `Scene`，是纯领域逻辑。
pub struct UndoStack {
    undo: Vec<Box<dyn Command>>,
    redo: Vec<Box<dyn Command>>,
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new()
    }
}

impl UndoStack {
    pub fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn push(&mut self, mut cmd: Box<dyn Command>, scene: &mut Scene) {
        let skip = cmd.skip_first_redo();
        if !skip {
            cmd.redo(scene);
        }
        self.redo.clear();
        self.undo.push(cmd);
    }

    pub fn undo(&mut self, scene: &mut Scene) -> bool {
        if let Some(mut cmd) = self.undo.pop() {
            cmd.undo(scene);
            self.redo.push(cmd);
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self, scene: &mut Scene) -> bool {
        if let Some(mut cmd) = self.redo.pop() {
            cmd.redo(scene);
            self.undo.push(cmd);
            true
        } else {
            false
        }
    }

    /// 可撤销步数（测试 / UI 状态指示用）。
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// 可重做步数。
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// 清空全部历史（打开 / 新建文件后调用）。
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

/// 一份可编辑的文档会话：场景数据 + 撤销历史 + 视口状态。
///
/// UI（`PReferZApp`）持有它并在外围包一层交互状态（工具、纹理缓存、弹窗等）；
/// CLI 直接持有它，用命令驱动后经 `preferz-fileio` 落盘。所有 item 变更仍须走
/// [`Session::push`]（即 undo 栈），与 UI 的硬约束一致。
pub struct Session {
    pub scene: Scene,
    pub viewport: ViewportState,
    pub undo_stack: UndoStack,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Self {
        Self {
            scene: Scene::new(),
            viewport: ViewportState::default(),
            undo_stack: UndoStack::new(),
        }
    }

    /// 用已加载的场景与视口组装会话（undo 历史为空——刚打开的文件无可撤销项）。
    pub fn with_loaded(scene: Scene, viewport: ViewportState) -> Self {
        Self {
            scene,
            viewport,
            undo_stack: UndoStack::new(),
        }
    }

    /// 入栈一条命令：普通命令立即 redo，预览命令（skip_first_redo）跳过。
    pub fn push(&mut self, cmd: Box<dyn Command>) {
        self.undo_stack.push(cmd, &mut self.scene);
    }

    /// 撤销一步，返回是否实际撤销。
    pub fn undo(&mut self) -> bool {
        self.undo_stack.undo(&mut self.scene)
    }

    /// 重做一步，返回是否实际重做。
    pub fn redo(&mut self) -> bool {
        self.undo_stack.redo(&mut self.scene)
    }

    /// 清空 undo/redo 历史（新建 / 打开文件时）。
    pub fn clear_history(&mut self) {
        self.undo_stack.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::MoveItems;
    use crate::shape::{ShapeType, StrokeStyle};
    use crate::spaces::CanvasVector;

    /// undo/redo 往返：移动一个矩形后撤销应回到原位，重做再到新位置。
    #[test]
    fn undo_redo_roundtrip() {
        let mut session = Session::new();
        let item = crate::item::Item::new_shape(
            ShapeType::Rectangle,
            (100.0, 100.0),
            0.0,
            0.0,
            StrokeStyle::default(),
            None,
        );
        let id = item.id;
        session.push(Box::new(crate::commands::AddItem::new(item)));

        let pos = session.scene.get_item(&id).unwrap().transform.pos;
        // 注意：MoveItems::new 默认 preview_already_applied=true（拖拽预览语义），
        // headless 调用方没有"预览直改"阶段，必须显式关闭，否则 push 会跳过 redo。
        session.push(Box::new(
            MoveItems::new(vec![id], CanvasVector::new(10.0, 20.0)).with_preview_applied(false),
        ));
        let moved = session.scene.get_item(&id).unwrap().transform.pos;
        assert_eq!((moved.x - pos.x).round() as i32, 10);
        assert_eq!((moved.y - pos.y).round() as i32, 20);

        assert!(session.undo());
        let back = session.scene.get_item(&id).unwrap().transform.pos;
        assert!((back.x - pos.x).abs() < 1e-3);
        assert!((back.y - pos.y).abs() < 1e-3);

        assert!(session.redo());
        let again = session.scene.get_item(&id).unwrap().transform.pos;
        assert!((again.x - moved.x).abs() < 1e-3);
        assert!((again.y - moved.y).abs() < 1e-3);
    }

    /// 空栈 undo/redo 返回 false 且不出错。
    #[test]
    fn empty_stack_returns_false() {
        let mut session = Session::new();
        assert!(!session.undo());
        assert!(!session.redo());
        assert_eq!(session.undo_stack.undo_len(), 0);
    }
}
