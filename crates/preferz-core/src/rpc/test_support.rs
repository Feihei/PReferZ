//! `#[cfg(test)]` 内存版 [`RpcDoc`]：core 不依赖 preferz-fileio，
//! open/save 无法真做，返回结构化错误即可（dispatch 测试不覆盖持久化，
//! 真实 open/save 由 CLI 侧集成测试把关）。

use std::path::{Path, PathBuf};

use super::doc::RpcDoc;
use crate::session::Session;

pub(crate) struct TestDoc {
    pub session: Session,
    path: Option<PathBuf>,
}

impl TestDoc {
    pub(crate) fn new() -> Self {
        Self {
            session: Session::new(),
            path: None,
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
    }

    fn open_path(&mut self, _path: &Path) -> Result<usize, String> {
        Err("open is not available in core tests".to_string())
    }

    fn save_path(&mut self, _target: Option<&Path>) -> Result<PathBuf, String> {
        Err("save is not available in core tests".to_string())
    }
}
