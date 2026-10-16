//! `#[cfg(test)]` 内存版 [`RpcDoc`]：core 不依赖 preferz-fileio，
//! open/save 无法真做，返回结构化错误即可（dispatch 测试不覆盖持久化，
//! 真实 open/save 由 CLI 侧集成测试把关）。

use std::path::{Path, PathBuf};

use super::doc::{RpcDoc, RpcImage, ScreenshotReply};
use crate::session::Session;

pub(crate) struct TestDoc {
    pub session: Session,
    path: Option<PathBuf>,
    /// add_image 测试用：预置的图片资产加载结果；None 时 load_image 报错
    /// （core 测试无法触碰真实文件系统）。
    pub canned_image: Option<RpcImage>,
    /// screenshot 测试用：预置的截图结果；None 时 capture_screenshot 报错。
    pub canned_screenshot: Option<ScreenshotReply>,
}

impl TestDoc {
    pub(crate) fn new() -> Self {
        Self {
            session: Session::new(),
            path: None,
            canned_image: None,
            canned_screenshot: None,
        }
    }
}

impl RpcDoc for TestDoc {
    fn session(&mut self) -> &mut Session {
        &mut self.session
    }

    fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn reset(&mut self) {
        self.session = Session::new();
        self.path = None;
        self.canned_image = None;
    }

    fn load_image(&mut self, _path: &Path) -> Result<RpcImage, String> {
        self.canned_image
            .clone()
            .ok_or_else(|| "load_image is not available in core tests".to_string())
    }

    fn open_path(&mut self, _path: &Path) -> Result<usize, String> {
        Err("open is not available in core tests".to_string())
    }

    fn save_path(&mut self, _target: Option<&Path>) -> Result<PathBuf, String> {
        Err("save is not available in core tests".to_string())
    }

    fn capture_screenshot(&mut self, max_width: Option<u32>) -> Result<ScreenshotReply, String> {
        let mut shot = self
            .canned_screenshot
            .clone()
            .ok_or_else(|| "screenshot is not available in core tests".to_string())?;
        // max_width 降采样语义与 GUI 实现同源：仅宽度超限时等比缩小。
        if let Some(max_w) = max_width {
            if shot.width > max_w {
                let ratio = max_w as f32 / shot.width as f32;
                shot.height = ((shot.height as f32 * ratio) as u32).max(1);
                shot.width = max_w;
            }
        }
        Ok(shot)
    }
}
