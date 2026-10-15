//! RPC 调用方的文档抽象。

use std::path::{Path, PathBuf};

use crate::session::Session;

/// dispatch 只认这层接口，不关心宿主是 CLI 进程还是 GUI 窗口。
///
/// 实现方负责"当前文件是谁"这层薄状态与持久化（复用 preferz-fileio），
/// core 保持零 fileio 依赖。
pub trait RpcDoc {
    /// 领域会话（Scene + ViewportState + UndoStack）。所有变更走 `push(Command)`。
    fn session(&mut self) -> &mut Session;

    /// 当前文件路径（`stats` 回显；从未保存过为 `None`）。
    fn path(&self) -> Option<&Path>;

    /// 重置为空白文档（等价 GUI 的"新建画布"，undo 历史一并清空）。
    fn reset(&mut self);

    /// 打开 `.prz`：加载场景、图片字节与视口状态并替换当前会话；
    /// 返回元素数。实现方负责纹理/图片资产接线（GUI 侧）或字节保真（CLI 侧）。
    fn open_path(&mut self, path: &Path) -> Result<usize, String>;

    /// 保存。`None` = 存回当前文件（从未保存过则报错，要求显式路径）；
    /// `Some(p)` 且与当前路径不同 = 另存为。返回实际保存路径。
    fn save_path(&mut self, target: Option<&Path>) -> Result<PathBuf, String>;
}
