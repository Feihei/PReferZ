use super::*;
use eframe::egui;
use preferz_core::Scene;
use preferz_fileio::PrzFile;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

/// 后台图片导入解码结果（线UI 线程）
/// 线程负责读取文件字节 + 解码；UI 线程负责上传纹理 + 创建 item
pub(crate) struct ImportOutcome {
    pub(crate) path: PathBuf,
    /// 原始图片字节（写sqlar 用）
    pub(crate) bytes: Vec<u8>,
    /// 解码后的图片尺寸
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// RGBA 像素数据（上传纹理用
    pub(crate) rgba: Vec<u8>,
    /// 解码错误（若存在
    pub(crate) error: Option<String>,
}

/// 后台 .prz 加载结果（线UI 线程）
pub(crate) struct LoadOutcome {
    pub(crate) path: PathBuf,
    pub(crate) result: Result<preferz_fileio::LoadResult, String>,
}

/// 后台保存结果（线UI 线程）
pub(crate) struct SaveOutcome {
    pub(crate) path: PathBuf,
    pub(crate) result: Result<(), String>,
}

/// 后台导出结果（线程 → UI 线程）。
pub(crate) struct ExportOutcome {
    pub(crate) path: PathBuf,
    pub(crate) result: Result<String, String>,
}

/// 后台任务状态。`loading`/`saving` 为 true 时显示进度条。
#[derive(Default)]
pub(crate) struct BackgroundOps {
    /// 图片导入解码通道（单条队列，每次导入一条）。
    pub(crate) import_rx: Option<Receiver<ImportOutcome>>,
    /// .prz 文件加载通道。
    pub(crate) load_rx: Option<Receiver<LoadOutcome>>,
    /// 文件保存通道。
    pub(crate) save_rx: Option<Receiver<SaveOutcome>>,
    /// 场景导出通道。
    pub(crate) export_rx: Option<Receiver<ExportOutcome>>,
    /// 当前进行的后台任务数量（>0 时显示进度条）。
    pub(crate) pending: usize,
    /// 进度消息。
    pub(crate) msg: Option<String>,
}

impl BackgroundOps {
    pub(crate) fn start_import(&mut self, ctx: &egui::Context, path: PathBuf, lang: Lang) {
        let (tx, rx) = mpsc::channel();
        self.import_rx = Some(rx);
        self.pending += 1;
        self.msg = Some(fill(
            t(lang, T::ProgressImportImage),
            &[path.display().to_string()],
        ));
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let outcome = match (std::fs::read(&path), image::open(&path)) {
                (Ok(bytes), Ok(img)) => {
                    let (w, h) = img.dimensions();
                    let rgba = img.to_rgba8().into_vec();
                    ImportOutcome {
                        path,
                        bytes,
                        width: w,
                        height: h,
                        rgba,
                        error: None,
                    }
                }
                (Err(e), _) => ImportOutcome {
                    path,
                    bytes: Vec::new(),
                    width: 0,
                    height: 0,
                    rgba: Vec::new(),
                    error: Some(format!("读取失败: {}", e)),
                },
                (_, Err(e)) => ImportOutcome {
                    path,
                    bytes: Vec::new(),
                    width: 0,
                    height: 0,
                    rgba: Vec::new(),
                    error: Some(format!("解码失败: {}", e)),
                },
            };
            let _ = tx.send(outcome);
            ctx2.request_repaint();
        });
    }

    pub(crate) fn start_load(&mut self, ctx: &egui::Context, path: PathBuf, lang: Lang) {
        let (tx, rx) = mpsc::channel();
        self.load_rx = Some(rx);
        self.pending += 1;
        self.msg = Some(fill(
            t(lang, T::ProgressOpenFile),
            &[path.display().to_string()],
        ));
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let prz = PrzFile::open(&path)?;
                prz.load_scene()
            })();
            let outcome = LoadOutcome {
                path: path.clone(),
                result: result.map_err(|e| e.to_string()),
            };
            let _ = tx.send(outcome);
            ctx2.request_repaint();
        });
    }

    pub(crate) fn start_save(
        &mut self,
        ctx: &egui::Context,
        path: PathBuf,
        scene: Scene,
        images: HashMap<String, Vec<u8>>,
        viewport: ViewportMeta,
        lang: Lang,
    ) {
        let (tx, rx) = mpsc::channel();
        self.save_rx = Some(rx);
        self.pending += 1;
        self.msg = Some(fill(
            t(lang, T::ProgressSaveFile),
            &[path.display().to_string()],
        ));
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let mut prz = if path.exists() {
                    PrzFile::open(&path)?
                } else {
                    PrzFile::create(&path)?
                };
                prz.save_scene(&scene, &images, viewport)
            })();
            let _ = tx.send(SaveOutcome {
                path: path.clone(),
                result: result.map_err(|e| e.to_string()),
            });
            ctx2.request_repaint();
        });
    }

    /// 取出并处理已完成的导入结果（PReferZApp::poll_background 调用）
    pub(crate) fn take_import(&mut self) -> Option<ImportOutcome> {
        if let Some(rx) = &self.import_rx {
            if let Ok(outcome) = rx.try_recv() {
                self.pending = self.pending.saturating_sub(1);
                if self.pending == 0 {
                    self.msg = None;
                }
                self.import_rx = None;
                return Some(outcome);
            }
        }
        None
    }

    /// 取出并处理已完成的加载结果（PReferZApp::poll_background 调用）
    pub(crate) fn take_load(&mut self) -> Option<LoadOutcome> {
        if let Some(rx) = &self.load_rx {
            if let Ok(outcome) = rx.try_recv() {
                self.pending = self.pending.saturating_sub(1);
                if self.pending == 0 {
                    self.msg = None;
                }
                self.load_rx = None;
                return Some(outcome);
            }
        }
        None
    }

    /// 取出并处理已完成的保存结果（由 PReferZApp::poll_background 调用）。
    pub(crate) fn take_save(&mut self) -> Option<SaveOutcome> {
        if let Some(rx) = &self.save_rx {
            if let Ok(outcome) = rx.try_recv() {
                self.pending = self.pending.saturating_sub(1);
                if self.pending == 0 {
                    self.msg = None;
                }
                self.save_rx = None;
                return Some(outcome);
            }
        }
        None
    }

    /// 取出并处理已完成的导出结果（由 PReferZApp::poll_background 调用）。
    pub(crate) fn take_export(&mut self) -> Option<ExportOutcome> {
        if let Some(rx) = &self.export_rx {
            if let Ok(outcome) = rx.try_recv() {
                self.pending = self.pending.saturating_sub(1);
                if self.pending == 0 {
                    self.msg = None;
                }
                self.export_rx = None;
                return Some(outcome);
            }
        }
        None
    }
}
