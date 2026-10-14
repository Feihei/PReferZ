//! CLI 侧的文档句柄：headless [`Session`] + 当前 `.prz` 文件连接 + 内嵌图片字节。
//!
//! 与 GUI 的分工：所有领域变更走 `session.push(Command)`（与 UI 同一条 undo 通道），
//! 持久化复用 `preferz-fileio`，CLI 自身只持有"当前文件是谁"这层薄状态。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use preferz_core::spaces::CanvasVector;
use preferz_core::{Session, ViewportState};
use preferz_fileio::{PrzFile, ViewportMeta};

pub(crate) struct CliDoc {
    /// 领域会话：场景 + 视口 + undo 栈（preferz-core L1）。
    pub session: Session,
    /// 当前文件连接；内存中的新文档为 `None`。
    file: Option<PrzFile>,
    /// 内嵌图片字节（texture_id → bytes），打开时从 sqlar 读出，保存时原样回写。
    /// CLI 当前不支持导入图片，但必须在 round-trip 中保住既有图片。
    images: HashMap<String, Vec<u8>>,
    /// 当前文件路径（保存成功后更新）。
    path: Option<PathBuf>,
}

impl Default for CliDoc {
    fn default() -> Self {
        Self::new()
    }
}

impl CliDoc {
    pub(crate) fn new() -> Self {
        Self {
            session: Session::new(),
            file: None,
            images: HashMap::new(),
            path: None,
        }
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// 重置为空白文档（等价于 GUI 的"新建画布"，undo 历史一并清空）。
    pub(crate) fn reset(&mut self) {
        self.session = Session::new();
        self.images.clear();
        self.file = None;
        self.path = None;
    }

    /// 打开 `.prz`：加载场景、图片字节与视口状态；替换当前会话。
    pub(crate) fn open(&mut self, path: &Path) -> Result<usize, String> {
        let prz = PrzFile::open(path).map_err(|e| format!("open failed: {e}"))?;
        let (mut scene, images, meta) =
            prz.load_scene().map_err(|e| format!("load failed: {e}"))?;
        scene.cleanup_orphan_containers();

        // 视口元数据防御性还原（与 GUI finish_load 同策略）。
        let mut viewport = ViewportState::default();
        viewport.pan = CanvasVector::new(meta.pan_x, meta.pan_y);
        if meta.zoom.is_finite() && meta.zoom > 0.0 {
            viewport.zoom = meta.zoom;
        }

        let count = scene.items.len();
        self.session = Session::with_loaded(scene, viewport);
        self.images = images;
        self.file = Some(prz);
        self.path = Some(path.to_path_buf());
        Ok(count)
    }

    /// 保存。
    /// - `target == None`：存回当前文件（从未保存过则报错，要求显式路径）；
    /// - `target == Some(p)` 且与当前路径不同：在该路径新建/接管 `.prz`（另存为）。
    ///
    /// 返回实际保存路径。
    pub(crate) fn save(&mut self, target: Option<&Path>) -> Result<PathBuf, String> {
        let target_path = match target {
            Some(p) => p.to_path_buf(),
            None => self
                .path
                .clone()
                .ok_or_else(|| "no current file; pass a path to save".to_string())?,
        };

        // 另存为新路径（或尚无连接的新文档）：创建连接并初始化 schema。
        let need_new_conn =
            self.file.is_none() || self.path.as_deref().is_some_and(|p| p != target_path);
        if need_new_conn {
            let prz = PrzFile::create(&target_path).map_err(|e| format!("create failed: {e}"))?;
            self.file = Some(prz);
        }

        let viewport = &self.session.viewport;
        let meta = ViewportMeta {
            pan_x: viewport.pan.x,
            pan_y: viewport.pan.y,
            zoom: viewport.zoom,
        };
        let file = self.file.as_mut().expect("connection ensured above");
        file.save_scene(&self.session.scene, &self.images, meta)
            .map_err(|e| format!("save failed: {e}"))?;

        self.path = Some(target_path.clone());
        Ok(target_path)
    }
}
