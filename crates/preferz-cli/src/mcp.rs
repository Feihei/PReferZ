//! `--mcp` 模式：stdio JSON-RPC 2.0 MCP 服务器。
//!
//! **本地优先设计**（D 决策）：只做 stdio 传输，不做 HTTP/SSE；保持隐式进程
//! 状态（单文档服务器，`new`/`open`/`save` 直接作用于唯一文档），所有工具
//! 直接转投 [`crate::dispatch::dispatch`]，与 JSONL REPL 共享同一条 undo 通道。
//!
//! **双栈协议**（按请求特征分流，无共享握手状态）：
//! - 新栈（2026-07-28 无状态 spec）：无 initialize 握手；`server/discover` 声明
//!   能力；每个请求的 `params._meta["io.modelcontextprotocol/protocolVersion"]`
//!   必须是本服务器认识的版本，否则回 -32022；`ping` 已移除（回 -32601）；
//!   所有 result 带 `resultType: "complete"`。
//! - 旧栈（≤2025-11-25 兜底）：`initialize` + `notifications/initialized` 握手、
//!   `ping` 回空对象、不校验 `_meta`。
//!
//! stdout 只走协议帧（每行一个 JSON，立即 flush）；一切诊断日志走 stderr。

use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

use crate::doc::CliDoc;
use crate::mcp_tools;

/// 本服务器实现的新栈协议版本。
pub(crate) const PROTOCOL_VERSION: &str = "2026-07-28";
/// 旧栈握手兜底版本（客户端报未知版本时协商到此）。
const LEGACY_FALLBACK_VERSION: &str = "2025-11-25";
/// 旧栈认识的版本集合（`initialize` 时可原样回显）。
const LEGACY_KNOWN_VERSIONS: &[&str] = &[
    "2024-11-05",
    "2025-03-26",
    "2025-06-18",
    "2025-11-25",
    PROTOCOL_VERSION,
];

const META_VERSION_KEY: &str = "io.modelcontextprotocol/protocolVersion";

// JSON-RPC 2.0 保留错误码 + 2026-07-28 spec 保留段（-32020..-32099）。
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;

/// `--mcp` 主循环：stdin 逐行读，stdout 逐行写协议帧。
pub(crate) fn run_mcp(doc: &mut CliDoc) -> i32 {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if let Some(resp) = handle_message(doc, &line) {
            if let Ok(text) = serde_json::to_string(&resp) {
                let _ = writeln!(out, "{text}");
                let _ = out.flush();
            }
        }
    }
    0
}

/// 处理一行输入，返回需要写回的响应帧；通知 / 空行返回 `None`（不回包）。
/// 纯函数（相对 stdio），单测直接驱动。
pub(crate) fn handle_message(doc: &mut CliDoc, line: &str) -> Option<Value> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let req: Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(e) => {
            return Some(error_response(
                Value::Null,
                PARSE_ERROR,
                &format!("parse error: {e}"),
            ))
        }
    };
    // 2026-07-28 spec 移除 JSON-RPC 批处理；旧栈客户端也几乎不用 → 一律拒绝。
    if req.is_array() {
        return Some(error_response(
            Value::Null,
            INVALID_REQUEST,
            "batch requests are not supported",
        ));
    }
    if !req.is_object() {
        return Some(error_response(
            Value::Null,
            INVALID_REQUEST,
            "request must be a JSON object",
        ));
    }
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = req.get("method").and_then(Value::as_str) else {
        return Some(error_response(id, INVALID_REQUEST, "missing 'method'"));
    };
    // 通知（无 id 字段）与通知方法一律不回包——协议版本不匹配的通知同样静默
    // 丢弃（服务器无从向通知方协商）。
    if method.starts_with("notifications/") || req.get("id").is_none() {
        return None;
    }
    let params = req.get("params").cloned().unwrap_or_else(|| json!({}));

    // 新栈 _meta 协议版本校验：带了该字段即视为新栈客户端。
    let meta_version = params
        .get("_meta")
        .and_then(|m| m.get(META_VERSION_KEY))
        .and_then(Value::as_str);
    if let Some(v) = meta_version {
        if v != PROTOCOL_VERSION {
            return Some(error_response(
                id,
                UNSUPPORTED_PROTOCOL_VERSION,
                &format!("unsupported protocol version '{v}' (server speaks '{PROTOCOL_VERSION}')"),
            ));
        }
    }
    let new_style = meta_version.is_some();

    let result = match method {
        // 旧栈握手。新栈客户端不会调 initialize（无状态 spec 已移除）；
        // 即便带 _meta 调用也按握手响应处理（宽松兜底）。
        "initialize" => {
            let client_version = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(LEGACY_FALLBACK_VERSION);
            let negotiated = if LEGACY_KNOWN_VERSIONS.contains(&client_version) {
                client_version
            } else {
                LEGACY_FALLBACK_VERSION
            };
            Ok(json!({
                "protocolVersion": negotiated,
                "capabilities": { "tools": {} },
                "serverInfo": server_info(),
                "instructions": "Headless single-document PReferZ canvas. Tools act on the \
            current document directly (new/open/save manage files; list reads state; \
            add_* / delete / move / undo / redo mutate through the undo stack).",
            }))
        }
        "ping" => {
            if new_style {
                Err((
                    METHOD_NOT_FOUND,
                    "ping was removed in the 2026-07-28 protocol".to_string(),
                ))
            } else {
                Ok(json!({}))
            }
        }
        "server/discover" => Ok(json!({
            "protocolVersions": [PROTOCOL_VERSION],
            "capabilities": { "tools": {} },
            "serverInfo": server_info(),
            "resultType": "complete",
        })),
        "tools/list" => {
            let mut result = mcp_tools::tools_list_result();
            if let Value::Object(map) = &mut result {
                map.insert("resultType".to_string(), json!("complete"));
            }
            Ok(result)
        }
        "tools/call" => tools_call(doc, &params),
        other => Err((METHOD_NOT_FOUND, format!("unknown method '{other}'"))),
    };

    Some(match result {
        Ok(v) => json!({ "jsonrpc": "2.0", "id": id, "result": v }),
        Err((code, message)) => error_response(id, code, &message),
    })
}

/// `tools/call`：`name` 当 `cmd`、`arguments` 当请求体直接转投 dispatch。
/// 工具级失败（未知工具 / 参数错误 / 领域错误）按 MCP 惯例回
/// `isError: true` 的 result，而非 JSON-RPC error——会话不被单次调用杀死。
fn tools_call(doc: &mut CliDoc, params: &Value) -> Result<Value, (i64, String)> {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return Err((INVALID_PARAMS, "missing 'name'".to_string()));
    };
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !args.is_object() {
        return Err((INVALID_PARAMS, "'arguments' must be an object".to_string()));
    }
    let mut request = args;
    if let Value::Object(map) = &mut request {
        map.insert("cmd".to_string(), json!(name));
    }
    let resp = crate::dispatch::dispatch(doc, &request);
    let ok = resp.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let text =
        serde_json::to_string(&resp).unwrap_or_else(|e| format!("\"serialize failed: {e}\""));
    Ok(json!({
        "content": [ { "type": "text", "text": text } ],
        "isError": !ok,
        "resultType": "complete",
    }))
}

fn server_info() -> Value {
    json!({
        "name": "preferz",
        "title": "PReferZ",
        "version": env!("CARGO_PKG_VERSION"),
    })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::from_str;

    fn handle(doc: &mut CliDoc, line: &str) -> Option<Value> {
        handle_message(doc, line)
    }

    fn result_of(resp: &Value) -> &Value {
        resp.get("result").expect("result frame")
    }

    fn error_code_of(resp: &Value) -> i64 {
        resp["error"]["code"].as_i64().expect("error frame")
    }

    #[test]
    fn legacy_initialize_negotiates_version() {
        let mut doc = CliDoc::new();
        let resp = handle(
            &mut doc,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        )
        .unwrap();
        assert_eq!(result_of(&resp)["protocolVersion"], "2025-06-18");
        assert_eq!(result_of(&resp)["serverInfo"]["name"], "preferz");
        // 未知版本 → 协商到兜底版本
        let resp = handle(
            &mut doc,
            r#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#,
        )
        .unwrap();
        assert_eq!(result_of(&resp)["protocolVersion"], LEGACY_FALLBACK_VERSION);
    }

    #[test]
    fn notifications_produce_no_response() {
        let mut doc = CliDoc::new();
        assert!(handle(
            &mut doc,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
        )
        .is_none());
        // 无 id 的任何请求按通知处理，不回包
        assert!(handle(&mut doc, r#"{"jsonrpc":"2.0","method":"tools/list"}"#).is_none());
    }

    #[test]
    fn ping_split_by_stack() {
        let mut doc = CliDoc::new();
        // 旧栈：回空对象
        let resp = handle(&mut doc, r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#).unwrap();
        assert_eq!(result_of(&resp), &json!({}));
        // 新栈（带 _meta 版本）：ping 已移除 → -32601
        let resp = handle(
            &mut doc,
            r#"{"jsonrpc":"2.0","id":2,"method":"ping","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}"#,
        )
        .unwrap();
        assert_eq!(error_code_of(&resp), METHOD_NOT_FOUND);
    }

    #[test]
    fn discover_declares_protocol_versions() {
        let mut doc = CliDoc::new();
        let resp = handle(
            &mut doc,
            r#"{"jsonrpc":"2.0","id":1,"method":"server/discover"}"#,
        )
        .unwrap();
        let result = result_of(&resp);
        assert_eq!(
            result["protocolVersions"],
            json!([PROTOCOL_VERSION]),
            "{result}"
        );
        assert_eq!(result["serverInfo"]["name"], "preferz");
        assert_eq!(result["resultType"], "complete");
    }

    #[test]
    fn meta_version_mismatch_is_rejected() {
        let mut doc = CliDoc::new();
        let resp = handle(
            &mut doc,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2025-11-25"}}}"#,
        )
        .unwrap();
        assert_eq!(error_code_of(&resp), UNSUPPORTED_PROTOCOL_VERSION);
    }

    #[test]
    fn tools_list_is_complete_and_sorted() {
        let mut doc = CliDoc::new();
        let resp = handle(
            &mut doc,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        )
        .unwrap();
        let result = result_of(&resp);
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 15);
        let mut names: Vec<&str> = tools
            .iter()
            .map(|t| t["name"].as_str().expect("tool name"))
            .collect();
        let sorted = names.clone();
        names.sort_unstable();
        assert_eq!(names, sorted);
        assert_eq!(result["resultType"], "complete");
        assert_eq!(result["cacheScope"], "private");
    }

    #[test]
    fn tools_call_roundtrip_and_is_error() {
        let mut doc = CliDoc::new();
        // 成功：add_shape → content 文本里带 JSONL 响应（ok: true + id）
        let resp = handle(
            &mut doc,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"add_shape","arguments":{"shape":"rect","w":100,"h":50}}}"#,
        )
        .unwrap();
        let result = result_of(&resp);
        assert_eq!(result["isError"], false);
        assert_eq!(result["resultType"], "complete");
        let inner: Value =
            from_str(result["content"][0]["text"].as_str().unwrap()).expect("inner JSONL resp");
        assert_eq!(inner["ok"], true);
        assert!(inner["id"].as_str().is_some());

        // 工具级失败：isError result 而非 JSON-RPC error
        let resp = handle(
            &mut doc,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"add_shape","arguments":{"shape":"hexagon","w":1,"h":1}}}"#,
        )
        .unwrap();
        assert_eq!(result_of(&resp)["isError"], true);

        // 未知工具：同样 isError（会话不死）
        let resp = handle(
            &mut doc,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#,
        )
        .unwrap();
        assert_eq!(result_of(&resp)["isError"], true);

        // 缺 name → JSON-RPC -32602
        let resp = handle(
            &mut doc,
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{}}"#,
        )
        .unwrap();
        assert_eq!(error_code_of(&resp), INVALID_PARAMS);
    }

    #[test]
    fn unknown_method_and_parse_error() {
        let mut doc = CliDoc::new();
        let resp = handle(&mut doc, r#"{"jsonrpc":"2.0","id":1,"method":"bogus/x"}"#).unwrap();
        assert_eq!(error_code_of(&resp), METHOD_NOT_FOUND);
        let resp = handle(&mut doc, "not json").unwrap();
        assert_eq!(error_code_of(&resp), PARSE_ERROR);
        // 批处理拒绝
        let resp = handle(&mut doc, r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#).unwrap();
        assert_eq!(error_code_of(&resp), INVALID_REQUEST);
        // 空行不回包
        assert!(handle(&mut doc, "   ").is_none());
    }
}
