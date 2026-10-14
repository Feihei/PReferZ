//! `--mcp` stdio 端到端集成测试：拉起 `preferz-cli --mcp` 子进程，
//! 按真实客户端行为走一遍双栈协议（旧栈握手 → 工具调用 → undo/redo；
//! 新栈 discover → _meta 校验 → -32022 拒绝）。

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};

use serde_json::{json, Value};

struct McpProc {
    child: Child,
    reader: BufReader<std::process::ChildStdout>,
}

impl McpProc {
    fn spawn(args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_preferz-cli"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn preferz-cli --mcp");
        let reader = BufReader::new(child.stdout.take().expect("stdout piped"));
        Self { child, reader }
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

    // 3) tools/list：15 个工具 + 缓存元数据
    p.send(&rpc(3, "tools/list", json!({})));
    let resp = p.read();
    let tools = resp["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 15);
    assert!(tools.iter().any(|t| t["name"] == "add_mermaid"));
    assert!(tools.iter().any(|t| t["name"] == "add_chart"));

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
