//! GUI 内嵌 MCP 监听（L2）：本进程起本地 TCP 服务，`preferz-cli --mcp` 代理桥
//! 把 agent 客户端（如 Trae MCP 通道）的请求转发到这里，经 core 的
//! `rpc::handle_message` 直接操作**已打开窗口**的 `Session`，画布实时渲染。
//!
//! 握手（OpenCADStudio 模式）：
//! - 启动时写 `~/.preferz/mcp-endpoint.json`：`{"port","token","pid"}`；
//!   token 防本机无关进程误连（pid 仅参考，验活靠 connect + token）。
//! - 连接线程先收一行 `{"token":"..."}`，校验通过回 `ok` 才进入转发循环；
//!   每个请求经 mpsc 投递 UI 线程，`ui()` 每帧轮询处理（复用 dispatch 语义，
//!   所有变更走同一条 undo 通道），回复经 per-request 通道回连接线程。
//! - 退出（Drop）时删除握手文件——仅当文件内容仍是本实例的 token
//!   （多实例场景后启动者覆盖文件，先退出者不得误删后来者的文件）。
//!
//! 线程唤醒：连接线程通过 `ctx_slot`（首帧由 `ui()` 填充的 `OnceLock`）
//! `request_repaint()` 唤醒可能休眠的 egui 事件循环。
//!
//! 测试隔离：环境变量 `PREFERZ_MCP_ENDPOINT` 覆盖握手文件路径（与 CLI 代理桩同源）。

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use egui::{ColorImage, Context, Event, UserData, ViewportCommand};
use preferz_core::rpc::{handle_message, RpcDoc, RpcImage, ScreenshotReply};
use preferz_fileio::{PrzFile, ViewportMeta};
use serde_json::{json, Value};

use super::*;

// ─────────────────────────── 握手文件 ───────────────────────────

/// 握手文件路径：`PREFERZ_MCP_ENDPOINT` 优先，否则 `~/.preferz/mcp-endpoint.json`。
fn endpoint_file_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PREFERZ_MCP_ENDPOINT") {
        return Some(PathBuf::from(p));
    }
    let home = super::config::dirs_or_home()?;
    Some(home.join(".preferz").join("mcp-endpoint.json"))
}

/// 写握手文件（父目录不存在则创建）。原子性要求不高：内容一行 JSON。
fn write_endpoint_file(path: &Path, port: u16, token: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let doc = json!({
        "port": port,
        "token": token,
        "pid": std::process::id(),
    });
    std::fs::write(path, doc.to_string())
}

/// 删除握手文件——仅当内容仍是本实例的 token（多实例：先退出者不误删后来者的）。
/// 静默忽略一切错误（退出路径，无意义重试）。
fn remove_endpoint_file_if_owned(path: &Path, token: &str) {
    if let Ok(content) = std::fs::read_to_string(path) {
        if content.contains(token) {
            let _ = std::fs::remove_file(path);
        }
    }
}

// ─────────────────────────── 服务器 ───────────────────────────

/// 一条从连接线程投递到 UI 线程的请求：原始 JSON 行 + 回包通道。
/// 回包为空字符串表示"通知，无协议帧"。
pub(crate) struct McpRequest {
    line: String,
    reply: Sender<String>,
}

/// GUI 内嵌 MCP 服务器句柄。`PReferZApp` 持有 `Option<McpServe>`；
/// Drop 删除握手文件。
pub(crate) struct McpServe {
    /// UI 线程每帧 `try_recv` 的请求队列。
    rx: Receiver<McpRequest>,
    /// 首帧由 `ui()` 填充的 egui Context，连接线程用它 request_repaint 唤醒 UI。
    ctx_slot: Arc<OnceLock<Context>>,
    endpoint_path: PathBuf,
    token: String,
}

impl McpServe {
    /// 启动监听并写握手文件。失败返回 `None`（端口/文件系统问题，记日志即可，
    /// 不影响 GUI 其余功能）。
    pub(crate) fn start() -> Option<McpServe> {
        let endpoint_path = endpoint_file_path()?;
        let listener = TcpListener::bind(("127.0.0.1", 0)).ok()?;
        let port = listener.local_addr().ok()?.port();
        // token：uuid v4 简单格式 = 32 hex 字符（无 rand 依赖，core 已带 uuid）。
        let token = ItemId::new_v4().simple().to_string();
        if write_endpoint_file(&endpoint_path, port, &token).is_err() {
            log::warn!("MCP 监听握手文件写入失败: {}", endpoint_path.display());
            return None;
        }

        let (tx, rx) = mpsc::channel::<McpRequest>();
        let ctx_slot: Arc<OnceLock<Context>> = Arc::new(OnceLock::new());
        let accept_ctx = ctx_slot.clone();
        let accept_token = token.clone();

        std::thread::Builder::new()
            .name("mcp-accept".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { break };
                    let tx = tx.clone();
                    let ctx_slot = accept_ctx.clone();
                    let token = accept_token.clone();
                    std::thread::Builder::new()
                        .name("mcp-conn".into())
                        .spawn(move || handle_connection(stream, &token, tx, ctx_slot))
                        .ok();
                }
            })
            .ok()?;

        Some(McpServe {
            rx,
            ctx_slot,
            endpoint_path,
            token,
        })
    }

    /// 首帧填入 egui Context（连接线程据此 request_repaint）。
    pub(crate) fn set_ctx(&self, ctx: &Context) {
        let _ = self.ctx_slot.set(ctx.clone());
    }

    /// UI 线程每帧轮询：取走请求 → GuiDoc 处理 → 回包 → repaint。
    pub(crate) fn poll(&self) -> Vec<McpRequest> {
        let mut out = Vec::new();
        while let Ok(req) = self.rx.try_recv() {
            out.push(req);
        }
        out
    }
}

impl Drop for McpServe {
    fn drop(&mut self) {
        remove_endpoint_file_if_owned(&self.endpoint_path, &self.token);
    }
}

/// 单连接循环：token 验活 → 逐行转发。UI 线程消失（应用退出）时
/// `reply.send` 失败，连接自然收尾。
fn handle_connection(
    stream: std::net::TcpStream,
    token: &str,
    tx: Sender<McpRequest>,
    ctx_slot: Arc<OnceLock<Context>>,
) {
    let _ = stream.set_nodelay(true);
    let mut reader = match stream.try_clone() {
        Ok(r) => BufReader::new(r),
        Err(_) => return,
    };

    // auth：第一行 {"token":"..."}；不符回 "denied" 并断开。
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(n) if n > 0 => {}
        _ => return,
    }
    let auth: Value = serde_json::from_str(line.trim()).unwrap_or(Value::Null);
    if auth.get("token").and_then(Value::as_str) != Some(token) {
        let _ = writeln!(&stream, "denied");
        return;
    }
    if writeln!(&stream, "ok").is_err() {
        return;
    }

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let (reply_tx, reply_rx) = mpsc::channel::<String>();
        let req = McpRequest {
            line: line.trim_end().to_string(),
            reply: reply_tx,
        };
        if tx.send(req).is_err() {
            break;
        }
        // 唤醒可能休眠的 egui 事件循环（首帧前 ctx 未填充则跳过）。
        if let Some(ctx) = ctx_slot.get() {
            ctx.request_repaint();
        }
        match reply_rx.recv() {
            // 通知：无协议帧，直接继续读下一行
            Ok(resp) if resp.is_empty() => {}
            Ok(resp) => {
                if writeln!(&stream, "{resp}").is_err() {
                    break;
                }
            }
            // UI 线程已关闭（应用退出）
            Err(_) => break,
        }
    }
}

// ─────────────────────────── RpcDoc 适配器 ───────────────────────────

/// GUI 侧的 [`RpcDoc`] 实现：把 core dispatch 接到已打开窗口的
/// `Session` / `current_file` / 纹理缓存上。open 复用 `finish_load`
/// （flash / recent / autosave 检测全同步），save 同步写（MCP 响应需要
/// 真实成败，不走后台保存）。
struct GuiDoc<'a> {
    app: &'a mut PReferZApp,
    ctx: Context,
}

impl RpcDoc for GuiDoc<'_> {
    fn session(&mut self) -> &mut Session {
        &mut self.app.session
    }

    fn path(&self) -> Option<&Path> {
        self.app.current_file.as_deref()
    }

    fn reset(&mut self) {
        // 与 reset_canvas 同语义的同步版（无 flash / 无视口 reset 以外的
        // ctx 交互；视口同样归位，与「新建画布」一致）。
        self.app.session = Session::new();
        self.app.texture_cache.clear();
        self.app.grayscale_texture_cache.clear();
        self.app.image_data_cache.clear();
        self.app.rgba_pixel_cache.clear();
        self.app.rgba_size_cache.clear();
        self.app.current_file = None;
        self.app.dirty = false;
        self.app.crop_mode = None;
        self.app.editing_text = None;
        self.app.color_picker_active = false;
        self.app.color_sample = None;
        self.app.pending_save_prompt = None;
        self.app.pending_autosave_restore = None;
        self.app.restoring_from_autosave = None;
        self.app.autosave_dirty_since = None;
        self.app.session.viewport.reset();
    }

    fn open_path(&mut self, path: &Path) -> Result<usize, String> {
        // 同步加载（SQLite 读 + 图片解码在本线程）：MCP 调用频度低，
        // 换取响应即真实结果的简单语义。
        let outcome = PrzFile::open(path)
            .and_then(|prz| prz.load_scene())
            .map_err(|e| format!("open failed: {e}"));
        match outcome {
            Ok((scene, images, viewport_meta)) => {
                self.app.finish_load(
                    &self.ctx,
                    LoadOutcome {
                        path: path.to_path_buf(),
                        result: Ok((scene, images, viewport_meta)),
                    },
                );
                Ok(self.app.session.scene.items.len())
            }
            Err(e) => {
                // 失败也走 finish_load：复用 FlashOpenFailed 提示。
                self.app.finish_load(
                    &self.ctx,
                    LoadOutcome {
                        path: path.to_path_buf(),
                        result: Err(e.clone()),
                    },
                );
                Err(e)
            }
        }
    }

    fn save_path(&mut self, target: Option<&Path>) -> Result<PathBuf, String> {
        let target_path = match target {
            Some(p) => p.to_path_buf(),
            None => self
                .app
                .current_file
                .clone()
                .ok_or_else(|| "no current file; pass a path to save".to_string())?,
        };
        // 同 start_save_common 的图片收集（key 转字符串以匹配 sqlar name）。
        let mut images: HashMap<String, Vec<u8>> = HashMap::new();
        for item in &self.app.session.scene.items {
            if let ItemKind::Pixmap { texture_id, .. } = &item.kind {
                if let Some(bytes) = self.app.image_data_cache.get(texture_id) {
                    images.insert(texture_id.to_string(), bytes.clone());
                }
            }
        }
        let viewport = ViewportMeta {
            pan_x: self.app.session.viewport.pan.x,
            pan_y: self.app.session.viewport.pan.y,
            zoom: self.app.session.viewport.zoom,
        };
        // 同后台保存的连接策略：文件已存在则接管，否则新建初始化 schema。
        let result = (|| {
            let mut prz = if target_path.exists() {
                PrzFile::open(&target_path).map_err(|e| format!("open failed: {e}"))?
            } else {
                PrzFile::create(&target_path).map_err(|e| format!("create failed: {e}"))?
            };
            prz.save_scene(&self.app.session.scene, &images, viewport)
                .map_err(|e| format!("save failed: {e}"))
        })();
        match result {
            Ok(()) => {
                self.app.flash(fill(
                    t(self.app.lang, T::FlashSaved),
                    &[target_path.display().to_string()],
                ));
                self.app.current_file = Some(target_path.clone());
                self.app.dirty = false;
                self.app.autosave_dirty_since = None;
                remove_stale_autosave(&target_path);
                Ok(target_path)
            }
            Err(e) => {
                self.app.flash(fill(
                    t(self.app.lang, T::FlashSaveFailed),
                    std::slice::from_ref(&e),
                ));
                Err(e)
            }
        }
    }

    /// 同步读取 + 解码 + 纹理注册：与 `finish_import` 的资产接线一致
    /// （纹理 + bytes/rgba/size 三缓存），但不在宿主里创建 item——由
    /// dispatch 的 add_image 走 AddItem undo 通道创建。MCP 调用频度低，
    /// 同步解码可接受（open_path 同语义）。
    fn load_image(&mut self, path: &Path) -> Result<RpcImage, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("read image failed: {e}"))?;
        let img = image::load_from_memory(&bytes).map_err(|e| format!("decode failed: {e}"))?;
        let (width, height) = img.dimensions();
        let rgba = img.to_rgba8().into_vec();

        let texture_id = self.app.next_texture_id;
        self.app.next_texture_id += 1;
        let color_image =
            egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &rgba);
        let handle = self.ctx.load_texture(
            format!("img_{}", texture_id),
            color_image,
            Default::default(),
        );
        self.app.texture_cache.insert(texture_id, handle);
        self.app.image_data_cache.insert(texture_id, bytes);
        self.app.rgba_pixel_cache.insert(texture_id, rgba);
        self.app.rgba_size_cache.insert(texture_id, (width, height));
        Ok(RpcImage {
            texture_id,
            filename: Some(path.to_string_lossy().to_string()),
            width,
            height,
        })
    }

    /// 同步 capture 契约下无法实现：截图回图在后续帧的 `Event::Screenshot`
    /// 才到达。真实截图请求由 `poll_mcp` 截获走两拍异步（MCP `tools/call`
    /// 框架）；裸 JSONL `{"cmd":"screenshot"}` 会落到这里，结构化报错指路。
    fn capture_screenshot(&mut self, _max_width: Option<u32>) -> Result<ScreenshotReply, String> {
        Err(
            "screenshot is only available via the MCP 'screenshot' tool \
             (tools/call framing)"
                .to_string(),
        )
    }
}

// ─────────────────────────── 主题默认色注入 ───────────────────────────

/// 把主题当前默认色注入 `tools/call` 请求——仅当调用方**未显式给色**时：
/// MCP 画的东西观感与手绘新元素一致（`default_stroke` 随主题切换，暗色主题
/// 浅描边、亮色主题深描边），显式颜色一律不覆盖。纯函数，单测直接驱动。
///
/// 覆盖：`add_shape`/`add_mermaid` 注入 `stroke`+`stroke_width`，
/// `add_text` 注入 `color`。headless 降级模式不经过这里（协议层固定深色）。
pub(crate) fn inject_theme_defaults(line: &str, default_stroke: &StrokeStyle) -> String {
    let Ok(mut req) = serde_json::from_str::<Value>(line) else {
        return line.to_string(); // 坏帧原样透传，handle_message 负责报 parse error
    };
    // 工具名在 params.name（不在 arguments 里）；不在目标集 / 无 arguments → 透传
    let Some(name) = req
        .get("params")
        .and_then(|p| p.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return line.to_string();
    };
    if !matches!(name.as_str(), "add_shape" | "add_mermaid" | "add_text") {
        return line.to_string();
    }
    let Some(args) = req
        .get_mut("params")
        .and_then(|p| p.get_mut("arguments"))
        .and_then(Value::as_object_mut)
    else {
        return line.to_string();
    };
    let color = json!([
        default_stroke.color[0],
        default_stroke.color[1],
        default_stroke.color[2],
        default_stroke.color[3],
    ]);
    // add_text 用 color 字段；shape/mermaid 用 stroke + stroke_width
    let mut pairs: Vec<(&str, Value)> = Vec::new();
    if name == "add_text" {
        pairs.push(("color", color));
    } else {
        pairs.push(("stroke", color));
        pairs.push(("stroke_width", json!(default_stroke.width as f64)));
    }
    let mut touched = false;
    for (key, value) in pairs {
        if !args.contains_key(key) || args.get(key).is_some_and(Value::is_null) {
            args.insert(key.to_string(), value);
            touched = true;
        }
    }
    if !touched {
        return line.to_string();
    }
    serde_json::to_string(&req).unwrap_or_else(|_| line.to_string())
}

// ─────────────────────────── MCP 截图（两拍异步） ───────────────────────────

/// 一条等待回图的截图请求（两拍异步的"拍间"状态）。
///
/// `poll_mcp` 截获 `tools/call screenshot` 后：发
/// `ViewportCommand::Screenshot`（`UserData` = token）并入队；本帧渲染完成后
/// eframe（glow）把像素推回下一帧的 `Event::Screenshot`，
/// `harvest_screenshots` 按 token 匹配 → PNG 编码 → 回包。超时兜底：窗口
/// 最小化时 eframe 不渲染、事件永不到达，回结构化错误而非挂死连接线程。
pub(crate) struct PendingScreenshot {
    token: u64,
    req_id: Value,
    max_width: Option<u32>,
    reply: Sender<String>,
    deadline: Instant,
}

/// 截图超时上限。可见窗口下一帧即回图，只有最小化/不渲染才等满。
const SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(3);

/// `poll_mcp` 对一行请求的识别结果。
#[derive(Debug)]
enum ScreenshotIntent {
    /// 非截图请求：走常规 `handle_message` 同步路径。
    None,
    /// 合法截图请求：发 ViewportCommand 后等回图帧。
    Capture {
        req_id: Value,
        max_width: Option<u32>,
    },
    /// 是截图请求但参数非法：立即回 isError 帧（不占用等待队列）。
    Reject { req_id: Value, message: String },
}

/// 识别一行是否为 `tools/call screenshot` 并校验参数。坏帧（非 JSON 等）
/// 返回 [`ScreenshotIntent::None`] 原样透传给 `handle_message` 报协议错误。
fn parse_screenshot_intent(line: &str) -> ScreenshotIntent {
    let Ok(req) = serde_json::from_str::<Value>(line) else {
        return ScreenshotIntent::None;
    };
    if req.get("method").and_then(Value::as_str) != Some("tools/call") {
        return ScreenshotIntent::None;
    }
    if req
        .get("params")
        .and_then(|p| p.get("name"))
        .and_then(Value::as_str)
        != Some("screenshot")
    {
        return ScreenshotIntent::None;
    }
    let req_id = req.get("id").cloned().unwrap_or(Value::Null);
    // max_width：可省略；给了必须是正有限数（u32 范围内）。
    let raw = req
        .get("params")
        .and_then(|p| p.get("arguments"))
        .and_then(|a| a.get("max_width"));
    let max_width = match raw {
        None | Some(Value::Null) => None,
        Some(v) => match v.as_f64() {
            Some(n) if n.is_finite() && n > 0.0 && n <= f64::from(u32::MAX) => Some(n as u32),
            _ => {
                return ScreenshotIntent::Reject {
                    req_id,
                    message: "'max_width' must be a positive finite number".to_string(),
                }
            }
        },
    };
    ScreenshotIntent::Capture { req_id, max_width }
}

/// 手写 base64（标准字母表 + padding）：PNG 字节 → MCP image content。
/// 零新依赖惯例，正确性由 round-trip 单测把关。
fn base64_encode(data: &[u8]) -> String {
    const TBL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let (chunks, rem) = data.as_chunks::<3>();
    for c in chunks {
        let n = (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]);
        for shift in [18, 12, 6, 0] {
            out.push(TBL[((n >> shift) & 63) as usize] as char);
        }
    }
    match *rem {
        [a] => {
            let n = u32::from(a) << 16;
            out.push(TBL[((n >> 18) & 63) as usize] as char);
            out.push(TBL[((n >> 12) & 63) as usize] as char);
            out.push('=');
            out.push('=');
        }
        [a, b] => {
            let n = (u32::from(a) << 16) | (u32::from(b) << 8);
            out.push(TBL[((n >> 18) & 63) as usize] as char);
            out.push(TBL[((n >> 12) & 63) as usize] as char);
            out.push(TBL[((n >> 6) & 63) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

/// `Event::Screenshot` 的帧缓冲 RGBA（glow `glReadPixels` + Y 翻转，已含
/// 背景合成）→ PNG 字节。可选 `max_width` 等比降采样（Triangle 滤镜）控制
/// 回给 LLM 的体积。
fn encode_screenshot_png(
    image: &ColorImage,
    max_width: Option<u32>,
) -> Result<ScreenshotReply, String> {
    let (w, h) = (image.size[0] as u32, image.size[1] as u32);
    if w == 0 || h == 0 {
        return Err("screenshot is empty (window not rendered?)".to_string());
    }
    let mut rgba: Vec<u8> = Vec::with_capacity(image.pixels.len() * 4);
    for c in &image.pixels {
        rgba.extend_from_slice(&[c.r(), c.g(), c.b(), c.a()]);
    }
    let (w, h, rgba) = match max_width {
        Some(max_w) if max_w > 0 && w > max_w => {
            let src = image::RgbaImage::from_raw(w, h, rgba)
                .ok_or_else(|| "screenshot buffer size mismatch".to_string())?;
            let new_h = ((h as f32 * max_w as f32 / w as f32) as u32).max(1);
            let resized =
                image::imageops::resize(&src, max_w, new_h, image::imageops::FilterType::Triangle);
            (max_w, new_h, resized.into_raw())
        }
        _ => (w, h, rgba),
    };
    let mut png = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut png));
    image::ImageEncoder::write_image(encoder, &rgba, w, h, image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("png encode failed: {e}"))?;
    Ok(ScreenshotReply {
        png,
        width: w,
        height: h,
    })
}

/// 截图成功 → MCP result：image content（base64 PNG）+ 尺寸摘要 text。
fn screenshot_result_frame(req_id: &Value, shot: &ScreenshotReply) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": req_id,
        "result": {
            "content": [
                {
                    "type": "image",
                    "data": base64_encode(&shot.png),
                    "mimeType": "image/png",
                },
                {
                    "type": "text",
                    "text": format!("{}x{} PNG, {} bytes", shot.width, shot.height, shot.png.len()),
                },
            ],
            "isError": false,
            "resultType": "complete",
        },
    })
    .to_string()
}

/// 截图失败 → isError result（工具级失败语义，会话不被单次调用杀死）。
fn screenshot_error_frame(req_id: &Value, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": req_id,
        "result": {
            "content": [ { "type": "text", "text": message } ],
            "isError": true,
            "resultType": "complete",
        },
    })
    .to_string()
}

// ─────────────────────────── 逻辑钩子接线 ───────────────────────────

impl PReferZApp {
    /// `App::logic()` 每帧调用（含最小化/隐藏窗口——eframe 0.36 在 UI pass
    /// 不跑时仍调 logic，`request_repaint` 可唤醒）：处理 MCP 请求。
    /// 放在 logic 而非 ui，保证 agent 在窗口最小化时也能操作画布。
    pub(crate) fn poll_mcp(&mut self, ctx: &Context) {
        let Some(serve) = &self.mcp else { return };
        // 每帧无条件填充（OnceLock 幂等）：连接线程要靠它 request_repaint
        // 唤醒休眠的事件循环；若等"有请求时"才填，首个请求恰好无人唤醒。
        serve.set_ctx(ctx);
        for req in serve.poll() {
            // 截图请求截获：不走同步 handle_message——回图在后续帧
            // （ViewportCommand::Screenshot → 渲染完成 → Event::Screenshot）。
            match parse_screenshot_intent(&req.line) {
                ScreenshotIntent::Capture { req_id, max_width } => {
                    let token = self.next_screenshot_token;
                    self.next_screenshot_token += 1;
                    ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::new(token)));
                    self.mcp_pending_shots.push(PendingScreenshot {
                        token,
                        req_id,
                        max_width,
                        reply: req.reply,
                        deadline: Instant::now() + SCREENSHOT_TIMEOUT,
                    });
                    ctx.request_repaint();
                }
                ScreenshotIntent::Reject { req_id, message } => {
                    let _ = req.reply.send(screenshot_error_frame(&req_id, &message));
                }
                ScreenshotIntent::None => {
                    // 主题默认色注入（在借用 self 之前取出快照）。
                    let default_stroke = self.default_stroke;
                    let line = inject_theme_defaults(&req.line, &default_stroke);
                    let mut doc = GuiDoc {
                        app: self,
                        ctx: ctx.clone(),
                    };
                    let resp = handle_message(&mut doc, &line);
                    let text = match resp {
                        Some(v) => serde_json::to_string(&v).unwrap_or_default(),
                        None => String::new(),
                    };
                    let _ = req.reply.send(text);
                }
            }
        }
        // 收割等待中的截图（无论本帧有没有新请求）。
        self.harvest_screenshots(ctx);
    }

    /// 匹配本帧到达的 `Event::Screenshot` 与等待队列：按 token 命中 → PNG
    /// 编码回包；超时（窗口最小化时 eframe 不渲染、事件永不到达）→ 结构化
    /// 错误回包；都没等到 → 留在队列等下一帧。
    fn harvest_screenshots(&mut self, ctx: &Context) {
        if self.mcp_pending_shots.is_empty() {
            return;
        }
        let arrived: Vec<(u64, Arc<ColorImage>)> = ctx.input(|i| {
            i.raw
                .events
                .iter()
                .filter_map(|e| match e {
                    Event::Screenshot {
                        user_data, image, ..
                    } => {
                        let token = user_data
                            .data
                            .as_ref()
                            .and_then(|a| a.downcast_ref::<u64>())
                            .copied()?;
                        Some((token, image.clone()))
                    }
                    _ => None,
                })
                .collect()
        });
        let now = Instant::now();
        let mut remaining = Vec::new();
        for pending in self.mcp_pending_shots.drain(..) {
            let reply_line = match arrived.iter().find(|(t, _)| *t == pending.token) {
                Some((_, image)) => match encode_screenshot_png(image, pending.max_width) {
                    Ok(shot) => screenshot_result_frame(&pending.req_id, &shot),
                    Err(e) => screenshot_error_frame(&pending.req_id, &e),
                },
                None if now >= pending.deadline => screenshot_error_frame(
                    &pending.req_id,
                    "screenshot timed out: the window is minimized or not rendering; \
                     restore it and retry",
                ),
                None => {
                    remaining.push(pending);
                    continue;
                }
            };
            let _ = pending.reply.send(reply_line);
        }
        self.mcp_pending_shots = remaining;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_file_roundtrip_and_ownership() {
        let dir = std::env::temp_dir().join(format!("preferz-mcp-test-{}", std::process::id()));
        let path = dir.join("mcp-endpoint.json");
        let _ = std::fs::remove_file(&path);

        write_endpoint_file(&path, 12345, "aaaa").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let doc: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(doc["port"], 12345);
        assert_eq!(doc["token"], "aaaa");
        assert!(doc["pid"].as_u64().is_some());

        // 不是自己的 token → 不删
        remove_endpoint_file_if_owned(&path, "bbbb");
        assert!(path.exists());
        // 是自己的 token → 删
        remove_endpoint_file_if_owned(&path, "aaaa");
        assert!(!path.exists());
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn env_override_sets_endpoint_path() {
        std::env::set_var("PREFERZ_MCP_ENDPOINT", "Z:/fake-path/endpoint.json");
        assert_eq!(
            endpoint_file_path().as_deref(),
            Some(Path::new("Z:/fake-path/endpoint.json"))
        );
        std::env::remove_var("PREFERZ_MCP_ENDPOINT");
    }

    #[test]
    fn theme_defaults_injected_only_when_absent() {
        use preferz_core::shape::StrokeStyle;
        let ds = StrokeStyle {
            color: [200, 200, 210, 255],
            width: 2.5,
            ..StrokeStyle::default()
        };

        // add_shape 缺省 → 注入 stroke + stroke_width
        let line = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"add_shape","arguments":{"shape":"rect","w":10,"h":5}}}"#;
        let out: Value = serde_json::from_str(&inject_theme_defaults(line, &ds)).unwrap();
        let args = &out["params"]["arguments"];
        assert_eq!(args["stroke"], json!([200, 200, 210, 255]));
        assert_eq!(args["stroke_width"], 2.5);

        // 显式给色 → 不覆盖（stroke_width 未给仍注入）
        let line = r##"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"add_shape","arguments":{"shape":"rect","w":10,"h":5,"stroke":"#ff0000"}}}"##;
        let out: Value = serde_json::from_str(&inject_theme_defaults(line, &ds)).unwrap();
        let args = &out["params"]["arguments"];
        assert_eq!(args["stroke"], "#ff0000");
        assert_eq!(args["stroke_width"], 2.5);

        // add_text 缺省 → 注入 color；显式给色不覆盖
        let line = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add_text","arguments":{"text":"hi"}}}"#;
        let out: Value = serde_json::from_str(&inject_theme_defaults(line, &ds)).unwrap();
        assert_eq!(
            out["params"]["arguments"]["color"],
            json!([200, 200, 210, 255])
        );
        let line = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"add_text","arguments":{"text":"hi","color":[1,2,3,255]}}}"#;
        let out: Value = serde_json::from_str(&inject_theme_defaults(line, &ds)).unwrap();
        assert_eq!(out["params"]["arguments"]["color"], json!([1, 2, 3, 255]));

        // add_mermaid 缺省 → 注入 stroke + stroke_width
        let line = r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"add_mermaid","arguments":{"mermaid":"flowchart TD\n a-->b"}}}"#;
        let out: Value = serde_json::from_str(&inject_theme_defaults(line, &ds)).unwrap();
        let args = &out["params"]["arguments"];
        assert_eq!(args["stroke"], json!([200, 200, 210, 255]));
        assert_eq!(args["stroke_width"], 2.5);

        // 非 add_* 工具 / 非法帧 → 原样透传
        let line = r#"{"jsonrpc":"2.0","id":6,"method":"tools/list"}"#;
        assert_eq!(inject_theme_defaults(line, &ds), line);
        assert_eq!(inject_theme_defaults("not json", &ds), "not json");
    }

    #[test]
    fn base64_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn screenshot_intent_parsing() {
        // 合法：无参 / 带 max_width / arguments 省略
        let line = r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"screenshot","arguments":{"max_width":1024}}}"#;
        match parse_screenshot_intent(line) {
            ScreenshotIntent::Capture { req_id, max_width } => {
                assert_eq!(req_id, json!(7));
                assert_eq!(max_width, Some(1024));
            }
            other => panic!("expected Capture, got {other:?}"),
        }
        let line = r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"screenshot","arguments":{}}}"#;
        match parse_screenshot_intent(line) {
            ScreenshotIntent::Capture { req_id, max_width } => {
                assert_eq!(req_id, json!(8));
                assert_eq!(max_width, None);
            }
            other => panic!("expected Capture, got {other:?}"),
        }
        let line =
            r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"screenshot"}}"#;
        assert!(matches!(
            parse_screenshot_intent(line),
            ScreenshotIntent::Capture {
                max_width: None,
                ..
            }
        ));

        // 非法 max_width → Reject（0 / 负数 / 非数字 / 超 u32 范围）
        for bad in ["0", "-1", "\"wide\"", "4294967296"] {
            let line = format!(
                r#"{{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{{"name":"screenshot","arguments":{{"max_width":{bad}}}}}}}"#
            );
            assert!(
                matches!(
                    parse_screenshot_intent(&line),
                    ScreenshotIntent::Reject { .. }
                ),
                "max_width={bad}"
            );
        }

        // 非 screenshot 工具 / 非 tools/call / 坏帧 → None（透传 handle_message）
        let line = r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"add_shape","arguments":{}}}"#;
        assert!(matches!(
            parse_screenshot_intent(line),
            ScreenshotIntent::None
        ));
        let line = r#"{"jsonrpc":"2.0","id":12,"method":"tools/list"}"#;
        assert!(matches!(
            parse_screenshot_intent(line),
            ScreenshotIntent::None
        ));
        assert!(matches!(
            parse_screenshot_intent("not json"),
            ScreenshotIntent::None
        ));
    }

    #[test]
    fn screenshot_png_encode_and_downscale() {
        // 2x1 纯色图（红、蓝）
        let img = ColorImage::from_rgba_unmultiplied([2, 1], &[255, 0, 0, 255, 0, 0, 255, 255]);
        let shot = encode_screenshot_png(&img, None).unwrap();
        assert_eq!((shot.width, shot.height), (2, 1));
        // PNG 魔数
        assert_eq!(
            &shot.png[..8],
            &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
        );
        // 解码回读像素
        let decoded = image::load_from_memory(&shot.png).unwrap().to_rgba8();
        assert_eq!(decoded.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(decoded.get_pixel(1, 0).0, [0, 0, 255, 255]);

        // max_width 降采样：4x2 → 2x1 等比（红蓝各两列）
        let img = ColorImage::from_rgba_unmultiplied(
            [4, 2],
            &[
                255, 0, 0, 255, 0, 0, 255, 255, 255, 0, 0, 255, 0, 0, 255, 255, //
                255, 0, 0, 255, 0, 0, 255, 255, 255, 0, 0, 255, 0, 0, 255, 255,
            ],
        );
        let shot = encode_screenshot_png(&img, Some(2)).unwrap();
        assert_eq!((shot.width, shot.height), (2, 1));

        // 空图 → 结构化错误
        let img = ColorImage::new([0, 0], Vec::new());
        assert!(encode_screenshot_png(&img, None).is_err());
    }

    #[test]
    fn screenshot_frames_are_valid_mcp_results() {
        let shot = ScreenshotReply {
            png: vec![1, 2, 3],
            width: 320,
            height: 240,
        };
        let frame: Value =
            serde_json::from_str(&screenshot_result_frame(&json!(42), &shot)).unwrap();
        assert_eq!(frame["jsonrpc"], "2.0");
        assert_eq!(frame["id"], 42);
        let result = &frame["result"];
        assert_eq!(result["isError"], false);
        assert_eq!(result["resultType"], "complete");
        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], base64_encode(&[1, 2, 3]));
        assert_eq!(result["content"][1]["type"], "text");
        assert_eq!(result["content"][1]["text"], "320x240 PNG, 3 bytes");

        let frame: Value =
            serde_json::from_str(&screenshot_error_frame(&json!(43), "boom")).unwrap();
        assert_eq!(frame["id"], 43);
        assert_eq!(frame["result"]["isError"], true);
        assert_eq!(frame["result"]["content"][0]["text"], "boom");
    }
}
