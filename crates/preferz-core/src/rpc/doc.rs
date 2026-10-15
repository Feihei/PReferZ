//! RPC 调用方的文档抽象。

use std::path::{Path, PathBuf};

use crate::session::Session;

/// 一次图片资产加载的结果：宿主分配的纹理句柄 + 解码出的尺寸。
///
/// `texture_id` 是宿主侧的资产键（GUI = egui 纹理 + 四张缓存的键，
/// CLI = 内嵌字节表的键），core 不解释它，只原样写进 `Pixmap` 项；
/// 保存时实现方负责把字节写进 `.prz` 的 sqlar。
#[derive(Debug, Clone, PartialEq)]
pub struct RpcImage {
    pub texture_id: u64,
    /// 原始文件路径字符串（存进 `Pixmap.filename`，与 GUI 导入同语义）。
    pub filename: Option<String>,
    pub width: u32,
    pub height: u32,
}

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

    /// 读取并解码一张图片资产（同步；MCP 调用频度低，与 open 同语义）。
    /// 实现方负责把字节保存在可持久化的资产缓存里（保存时写 sqlar）。
    /// 解码 / 读文件失败返回结构化错误。
    fn load_image(&mut self, path: &Path) -> Result<RpcImage, String>;

    /// 打开 `.prz`：加载场景、图片字节与视口状态并替换当前会话；
    /// 返回元素数。实现方负责纹理/图片资产接线（GUI 侧）或字节保真（CLI 侧）。
    fn open_path(&mut self, path: &Path) -> Result<usize, String>;

    /// 保存。`None` = 存回当前文件（从未保存过则报错，要求显式路径）；
    /// `Some(p)` 且与当前路径不同 = 另存为。返回实际保存路径。
    fn save_path(&mut self, target: Option<&Path>) -> Result<PathBuf, String>;
}
