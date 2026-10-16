//! `--mcp` stdio 端到端集成测试：拉起 `preferz-cli --mcp` 子进程，
//! 按真实客户端行为走一遍双栈协议（旧栈握手 → 工具调用 → undo/redo；
//! 新栈 discover → _meta 校验 → -32022 拒绝）。
//!
//! 另覆盖代理桥（GUI 内嵌监听）：
//! - 假 GUI（本地 TCP + token 验活）→ `--mcp` 进入纯转发模式；
//! - 握手文件缺失 / 端口不可达 → 降级 headless。
//!
//! 所有 headless 测试用 `PREFERZ_MCP_ENDPOINT` 指向不存在的路径强制降级，
//! 与开发机上真实运行的 GUI 实例隔离。

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use serde_json::{json, Value};

/// 强制 headless 的隔离环境变量：指向一个必定不存在的文件。
fn isolated_endpoint_env() -> String {
    std::env::temp_dir()
        .join(format!(
            "preferz-cli-no-such-endpoint-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
        .to_string_lossy()
        .into_owned()
}

struct McpProc {
    child: Child,
    reader: BufReader<std::process::ChildStdout>,
}

impl McpProc {
    /// spawn 并注入 `PREFERZ_MCP_ENDPOINT`（None = 隔离到不存在的路径，
    /// 保证 headless 降级，不受开发机上真实 GUI 影响）。
    fn spawn_env(args: &[&str], endpoint: Option<String>) -> Self {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_preferz"));
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        match endpoint {
            Some(p) => cmd.env("PREFERZ_MCP_ENDPOINT", p),
            None => cmd.env("PREFERZ_MCP_ENDPOINT", isolated_endpoint_env()),
        };
        let mut child = cmd.spawn().expect("spawn preferz-cli --mcp");
        let reader = BufReader::new(child.stdout.take().expect("stdout piped"));
        Self { child, reader }
    }

    fn spawn(args: &[&str]) -> Self {
        Self::spawn_env(args, None)
    }

    fn send(&mut self, v: &Value) {
        let stdin = self.child.stdin.as_mut().expect("stdin piped");
        writeln!(stdin, "{v}").expect("write request");
        stdin.flush().expect("flush request");
    }

    fn send_raw(&mut self, line: &str) {
        let stdin = self.child.stdin.as_mut().expect("stdin piped");
        writeln!(stdin, "{line}").expect("write raw request");
        stdin.flush().expect("flush raw request");
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        let n = self
            .reader
            .read_line(&mut line)
            .expect("read response line");
        assert!(n > 0, "server closed stdout before responding");
        serde_json::from_str(&line).expect("response is valid JSON")
    }

    fn close_stdin(&mut self) {
        self.child.stdin.take().expect("stdin piped");
    }
}

impl Drop for McpProc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn rpc(id: i64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

fn error_code(resp: &Value) -> i64 {
    resp["error"]["code"]
        .as_i64()
        .unwrap_or_else(|| panic!("error frame: {resp}"))
}

#[test]
fn legacy_stack_full_session() {
    let mut p = McpProc::spawn(&["--mcp"]);

    // 1) 旧栈握手
    p.send(&rpc(
        1,
        "initialize",
        json!({"protocolVersion": "2025-06-18"}),
    ));
    let resp = p.read();
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(resp["result"]["serverInfo"]["name"], "preferz");
    assert!(resp["result"]["capabilities"]["tools"].is_object());

    // 2) notifications/initialized：不回包。下一个响应必须是 ping 的，
    //    借此隐式验证通知被静默消费。
    p.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    p.send(&rpc(2, "ping", json!({})));
    let resp = p.read();
    assert_eq!(resp["id"], 2);
    assert_eq!(resp["result"], json!({}));

    // 3) tools/list：18 个工具 + 缓存元数据
    p.send(&rpc(3, "tools/list", json!({})));
    let resp = p.read();
    let tools = resp["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 18);
    assert!(tools.iter().any(|t| t["name"] == "add_mermaid"));
    assert!(tools.iter().any(|t| t["name"] == "add_chart"));
    assert!(tools.iter().any(|t| t["name"] == "add_image"));
    assert!(tools.iter().any(|t| t["name"] == "arrange"));
    assert!(tools.iter().any(|t| t["name"] == "screenshot"));

    // 4) tools/call add_shape → 成功 result
    p.send(&rpc(
        4,
        "tools/call",
        json!({"name": "add_shape", "arguments": {"shape": "rect", "w": 100, "h": 50}}),
    ));
    let resp = p.read();
    assert_eq!(resp["result"]["isError"], false);
    let inner: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(inner["ok"], true);
    let shape_id = inner["id"].as_str().unwrap().to_string();

    // 5) tools/call add_mermaid → 杀手锏工具走通
    p.send(&rpc(
        5,
        "tools/call",
        json!({"name": "add_mermaid", "arguments": {"mermaid": "flowchart TD\n a[开始] --> b[结束]"}}),
    ));
    let resp = p.read();
    assert_eq!(resp["result"]["isError"], false, "{}", resp);
    let inner: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(inner["ok"], true);
    assert_eq!(inner["nodes"], 2);
    assert_eq!(inner["edges"], 1);

    // 6) tools/call add_chart
    p.send(&rpc(
        6,
        "tools/call",
        json!({"name": "add_chart", "arguments": {"labels": ["a", "b"], "values": [1, 2]}}),
    ));
    let resp = p.read();
    let inner: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(inner["ok"], true);
    assert_eq!(inner["points"], 2);

    // 7) list 确认状态（7 = 1 形状 + 5 流程图元素 + 1 图表）
    p.send(&rpc(
        7,
        "tools/call",
        json!({"name": "list", "arguments": {}}),
    ));
    let resp = p.read();
    let inner: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(inner["items"].as_array().unwrap().len(), 7, "{inner}");

    // 8) undo（撤掉 add_chart）→ 6；add_shape 的 id 仍在
    p.send(&rpc(
        8,
        "tools/call",
        json!({"name": "undo", "arguments": {}}),
    ));
    let resp = p.read();
    let inner: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(inner["performed"], true);
    p.send(&rpc(
        9,
        "tools/call",
        json!({"name": "list", "arguments": {"ids": [shape_id], "full": true}}),
    ));
    let resp = p.read();
    let inner: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(inner["items"].as_array().unwrap().len(), 1);
    // 撤掉的图表不再出现
    p.send(&rpc(
        10,
        "tools/call",
        json!({"name": "stats", "arguments": {}}),
    ));
    let resp = p.read();
    let inner: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(inner["items"], 6, "{inner}");

    // 9) 工具级失败 → isError result，会话不死
    p.send(&rpc(
        11,
        "tools/call",
        json!({"name": "add_shape", "arguments": {"shape": "hexagon", "w": 1, "h": 1}}),
    ));
    let resp = p.read();
    assert_eq!(resp["result"]["isError"], true);

    // 10) EOF 退出
    p.close_stdin();
    let status = p.child.wait().expect("wait after EOF");
    assert!(status.success(), "exit code after EOF should be 0");
}

#[test]
fn new_stack_discover_and_meta_validation() {
    let mut p = McpProc::spawn(&["--mcp"]);

    // discover（新栈入口，无需握手）
    p.send(&rpc(1, "server/discover", json!({})));
    let resp = p.read();
    assert_eq!(resp["result"]["protocolVersions"], json!(["2026-07-28"]));
    assert_eq!(resp["result"]["resultType"], "complete");

    // 新栈 tools/call（带 _meta 版本）成功
    let pv = json!({"io.modelcontextprotocol/protocolVersion": "2026-07-28"});
    p.send(&rpc(
        2,
        "tools/call",
        json!({"_meta": pv, "name": "add_shape", "arguments": {"shape": "ellipse", "w": 40, "h": 40}}),
    ));
    let resp = p.read();
    assert_eq!(resp["result"]["isError"], false, "{}", resp);
    assert_eq!(resp["result"]["resultType"], "complete");

    // 不认识的 _meta 版本 → -32022
    p.send(&rpc(
        3,
        "tools/call",
        json!({"_meta": {"io.modelcontextprotocol/protocolVersion": "2025-11-25"}, "name": "stats", "arguments": {}}),
    ));
    let resp = p.read();
    assert_eq!(error_code(&resp), -32022);

    // 新栈 ping 已移除 → -32601
    p.send(&rpc(
        4,
        "ping",
        json!({"_meta": {"io.modelcontextprotocol/protocolVersion": "2026-07-28"}}),
    ));
    let resp = p.read();
    assert_eq!(error_code(&resp), -32601);

    // 未知 method → -32601
    p.send(&rpc(5, "resources/list", json!({})));
    let resp = p.read();
    assert_eq!(error_code(&resp), -32601);
}

#[test]
fn robustness_parse_error_and_batch_reject() {
    let mut p = McpProc::spawn(&["--mcp"]);
    p.send_raw("this is not json");
    let resp = p.read();
    assert_eq!(error_code(&resp), -32700);

    p.send_raw(r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#);
    let resp = p.read();
    assert_eq!(error_code(&resp), -32600);

    // 坏帧后服务器仍活着
    p.send(&rpc(1, "ping", json!({})));
    let resp = p.read();
    assert_eq!(resp["result"], json!({}));
}

// ─────────────────────────── 代理桥（GUI 内嵌监听）───────────────────────────

/// 假 GUI：accept 一条连接，验 token，回 ok，之后把每个 JSON-RPC 请求原样
/// 回一个带 `fake_gui: true` 的 result（模拟"协议处理发生在 GUI 进程"）。
/// listener 移入线程持有；`--mcp` 总是按握手文件里的 token 发送，token 恒匹配，
/// "denied" 分支是防御性代码，端到端测不到。
struct FakeGui {
    endpoint: PathBuf,
}

impl FakeGui {
    fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind fake gui");
        let port = listener.local_addr().unwrap().port();
        let token = "abcdef01".repeat(4);
        let dir = std::env::temp_dir().join(format!(
            "preferz-cli-proxy-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let endpoint = dir.join("mcp-endpoint.json");
        std::fs::write(
            &endpoint,
            json!({ "port": port, "token": token, "pid": 0 }).to_string(),
        )
        .expect("write endpoint file");
        std::thread::spawn(move || {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let auth: Value = serde_json::from_str(line.trim()).unwrap_or(Value::Null);
            let authorized = auth.get("token").and_then(Value::as_str) == Some(token.as_str());
            let mut stream = stream;
            let _ = writeln!(stream, "{}", if authorized { "ok" } else { "denied" });
            if !authorized {
                return;
            }
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                let Ok(req) = serde_json::from_str::<Value>(line.trim()) else {
                    break;
                };
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": req.get("id").cloned().unwrap_or(Value::Null),
                    "result": {
                        "fake_gui": true,
                        "method": req.get("method").cloned().unwrap_or(Value::Null),
                    },
                });
                if writeln!(stream, "{resp}").is_err() {
                    break;
                }
            }
        });
        FakeGui { endpoint }
    }
}

impl Drop for FakeGui {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.endpoint);
        if let Some(parent) = self.endpoint.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
}

#[test]
fn proxy_forwards_requests_to_gui() {
    let gui = FakeGui::start();
    let mut p = McpProc::spawn_env(
        &["--mcp"],
        Some(gui.endpoint.to_string_lossy().into_owned()),
    );

    // 请求被原样转发：响应由假 GUI 生成（fake_gui 标记），CLI 纯管道。
    p.send(&rpc(
        1,
        "initialize",
        json!({"protocolVersion": "2025-11-25"}),
    ));
    let resp = p.read();
    assert_eq!(resp["result"]["fake_gui"], true, "{resp}");
    assert_eq!(resp["result"]["method"], "initialize");

    p.send(&rpc(
        2,
        "tools/call",
        json!({"name": "stats", "arguments": {}}),
    ));
    let resp = p.read();
    assert_eq!(resp["result"]["fake_gui"], true);
    assert_eq!(resp["result"]["method"], "tools/call");

    // EOF：stdin 半关后 CLI 退出码 0
    p.close_stdin();
    let status = p.child.wait().expect("wait after EOF");
    assert!(status.success(), "proxy exit after EOF should be 0");
}

#[test]
fn proxy_unreachable_port_falls_back_to_headless() {
    // 握手文件有效，但端口没人听 → connect 失败 → headless
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind to reserve port");
    let port = listener.local_addr().unwrap().port();
    drop(listener); // 释放端口但保留地址值

    let dir = std::env::temp_dir().join(format!(
        "preferz-cli-fallback-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let endpoint = dir.join("mcp-endpoint.json");
    std::fs::write(
        &endpoint,
        json!({ "port": port, "token": "ff00ff00ff00ff00ff00ff00ff00ff00", "pid": 0 }).to_string(),
    )
    .expect("write endpoint file");

    let mut p = McpProc::spawn_env(&["--mcp"], Some(endpoint.to_string_lossy().into_owned()));
    // 降级后是 headless：协议处理在 CLI 自身，响应不带 fake_gui
    p.send(&rpc(
        1,
        "initialize",
        json!({"protocolVersion": "2025-11-25"}),
    ));
    let resp = p.read();
    assert_eq!(resp["result"]["protocolVersion"], "2025-11-25", "{resp}");
    assert!(resp["result"]["fake_gui"].is_null());
    p.close_stdin();
    let _ = p.child.wait();
    let _ = std::fs::remove_file(&endpoint);
    let _ = std::fs::remove_dir(dir);
}

#[test]
fn proxy_denied_token_falls_back_to_headless() {
    // 握手文件指向的端口接受连接但验活失败（回 denied）→ CLI 视为不可用 → headless。
    // CLI 发送的是文件里的 token，正常 GUI 不会拒绝；这里手动写一个错位 token
    // 无法构造（CLI 读同一文件）——该路径由假 GUI 防御分支覆盖，端到端仅保证
    // "验活失败 → 降级" 的读侧行为：让假 GUI 的 accept 线程直接关闭连接。
    // 构造方式：握手文件指向一个 accept 后立即关闭的端口。
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        // accept 后立即 drop 连接（不回 ok），CLI 验活读超时/EOF → 降级。
        let _ = listener.accept();
    });

    let dir = std::env::temp_dir().join(format!(
        "preferz-cli-denied-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let endpoint = dir.join("mcp-endpoint.json");
    std::fs::write(
        &endpoint,
        json!({ "port": port, "token": "11223344112233441122334411223344", "pid": 0 }).to_string(),
    )
    .expect("write endpoint file");

    let mut p = McpProc::spawn_env(&["--mcp"], Some(endpoint.to_string_lossy().into_owned()));
    p.send(&rpc(
        1,
        "initialize",
        json!({"protocolVersion": "2025-11-25"}),
    ));
    let resp = p.read();
    assert_eq!(resp["result"]["protocolVersion"], "2025-11-25", "{resp}");
    assert!(resp["result"]["fake_gui"].is_null());
    p.close_stdin();
    let _ = p.child.wait();
    let _ = std::fs::remove_file(&endpoint);
    let _ = std::fs::remove_dir(dir);
}

/// add_image headless 往返：临时 PNG → tools/call add_image（w-only 宽高比）
/// → save 落盘 → 新会话 open + list 确认 Pixmap 项与 sqlar 资产保真。
#[test]
fn headless_add_image_roundtrip_via_prz() {
    let dir = std::env::temp_dir().join(format!(
        "preferz-add-image-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let png_path = dir.join("pic.png");
    let prz_path = dir.join("board.prz");

    // 64×32 红色 PNG
    let img = image::RgbaImage::from_pixel(64, 32, image::Rgba([255, 0, 0, 255]));
    img.save(&png_path).expect("write temp png");

    let tools_call = |id: i64, name: &str, args: Value| {
        rpc(id, "tools/call", json!({ "name": name, "arguments": args }))
    };
    // 握手 + 通知（旧栈）
    let handshake = |p: &mut McpProc| {
        p.send(&rpc(
            1,
            "initialize",
            json!({"protocolVersion": "2025-06-18"}),
        ));
        let resp = p.read();
        assert_eq!(resp["result"]["protocolVersion"], "2025-06-18");
        p.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    };
    // tools/call → 解出内层 JSONL 响应
    let call = |p: &mut McpProc, v: Value| -> Value {
        p.send(&v);
        let resp = p.read();
        assert_eq!(resp["result"]["isError"], false, "{resp}");
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap())
            .expect("inner JSONL response")
    };

    // 会话 1：导入（只给 w → 宽高比推 h）+ 保存
    let mut p = McpProc::spawn(&["--mcp"]);
    handshake(&mut p);
    let inner = call(
        &mut p,
        tools_call(
            2,
            "add_image",
            json!({"path": png_path.to_string_lossy(), "x": 5.0, "y": 6.0, "w": 128.0}),
        ),
    );
    assert_eq!(inner["ok"], true, "{inner}");
    assert_eq!(inner["width"], 64);
    assert_eq!(inner["height"], 32);
    assert_eq!(inner["w"], 128.0);
    assert_eq!(inner["h"], 64.0, "h 应按 2:1 宽高比推出");
    let inner = call(
        &mut p,
        tools_call(3, "save", json!({"path": prz_path.to_string_lossy()})),
    );
    assert_eq!(inner["ok"], true, "{inner}");
    p.close_stdin();
    let _ = p.child.wait();
    assert!(prz_path.exists(), "save 应落盘");

    // 会话 2：重新打开 → Pixmap 项仍在（sqlar 资产保真由 load 成功佐证）
    let mut p2 = McpProc::spawn(&["--mcp"]);
    handshake(&mut p2);
    let inner = call(
        &mut p2,
        tools_call(2, "open", json!({"path": prz_path.to_string_lossy()})),
    );
    assert_eq!(inner["ok"], true, "{inner}");
    assert_eq!(inner["items"], 1);
    let inner = call(&mut p2, tools_call(3, "list", json!({"full": true})));
    let items = inner["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["kind"]["Pixmap"]["original_size"], json!([64, 32]));
    p2.close_stdin();
    let _ = p2.child.wait();

    let _ = std::fs::remove_file(&png_path);
    let _ = std::fs::remove_file(&prz_path);
    let _ = std::fs::remove_dir(dir);
}
