//! preferz-cli — PReferZ 的无头命令接口（JSONL / MCP stdio，面向 agent / 脚本）。
//!
//! 用法：
//! - `preferz-cli [file.prz]`            打开文件（可选）后进入 stdin/stdout JSONL 会话
//! - `preferz-cli --exec '<json>' [file]` 只执行一条命令并退出
//! - `preferz-cli --mcp [file.prz]`      MCP stdio 服务器：优先**代理**到已打开的
//!   GUI 实例（工具实时操作窗口画布），无 GUI 时降级为 headless 单文档服务器
//! - `preferz-cli --help` / `--version`
//!
//! JSONL 协议：stdin 每行一个 JSON 请求 `{"cmd": "...", ...}`，stdout 每行一个
//! JSON 响应。可选 `req_id`（任意 JSON 值）会原样回显，用于配对请求/响应。
//! 命令失败只产生 `{"ok": false, ...}`，不退出会话。
//!
//! MCP 模式：stdio JSON-RPC，双栈协议——新栈 2026-07-28 无状态 spec
//! （`server/discover` + `_meta` 版本校验 + `resultType`）与旧栈
//! （≤2025-11-25 `initialize` 握手）按请求特征分流；15 个工具与 JSONL
//! 命令一一对应，共享同一 undo 通道。协议核心下沉在 preferz-core 的
//! `rpc` 模块，代理与 headless 两种模式共享同一套语义。stdout 只走协议帧，
//! 日志全走 stderr。
//!
//! 示例（PowerShell）：
//! ```text
//! '{"cmd":"open","path":"board.prz"}' | preferz-cli
//! '{"cmd":"list"}' | preferz-cli board.prz
//! preferz-cli --exec '{"cmd":"add_shape","shape":"rect","x":0,"y":0,"w":10,"h":10}' board.prz
//! preferz-cli --mcp board.prz
//! ```

mod doc;
mod mcp;

use std::io::{self, BufRead, Write};

use preferz_core::rpc::{dispatch, protocol};
use serde_json::{json, Value};

use crate::doc::CliDoc;

const HELP: &str = "\
preferz-cli — headless JSONL / MCP stdio interface for PReferZ

USAGE:
    preferz-cli [OPTIONS] [FILE]

ARGS:
    <FILE>              .prz file to open before starting the session

OPTIONS:
    --exec <JSON>       Execute one request and exit
    --mcp               Run as a stdio MCP server (JSON-RPC 2.0). Proxies to a
                        running PReferZ GUI window when one is up (tools then
                        act on the opened canvas live); falls back to a
                        headless single-document server otherwise.
    -h, --help          Print this help
    -V, --version       Print version

JSONL PROTOCOL (one JSON object per line):
    request:   {\"cmd\": \"<name>\", ...args, \"req_id\": <optional echo>}
    response:  {\"ok\": true, \"cmd\": \"<name>\", \"req_id\": <echo>, ...data}
               {\"ok\": false, \"cmd\": \"<name>\", \"req_id\": <echo>, \"error\": \"<message>\"}

COMMANDS:
    ping, version, new, open, save, stats, list,
    select, select_all, deselect,
    add_shape, add_text, add_item, add_mermaid, add_chart,
    delete, move, undo, redo

NOTES:
    - list: {\"ids\": [..]} filters, \"full\": true returns full item JSON
      (replaces the old get command).
    - select with an empty ids array deselects everything; select_all is a
      convenience wrapper.

Send `{\"cmd\":\"ping\"}` to verify the connection.
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut exec: Option<String> = None;
    let mut mcp = false;
    let mut open_path: Option<String> = None;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return;
            }
            "-V" | "--version" => {
                println!("preferz-cli {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--exec" => match iter.next() {
                Some(json) => exec = Some(json.clone()),
                None => fatal_exit("--exec requires a JSON argument"),
            },
            "--mcp" => mcp = true,
            other if other.starts_with("--") => {
                fatal_exit(&format!("unknown option '{other}' (see --help)"));
            }
            other => {
                if open_path.is_some() {
                    fatal_exit("only one file path may be given");
                }
                open_path = Some(other.to_string());
            }
        }
    }
    if mcp && exec.is_some() {
        fatal_exit("--exec and --mcp are mutually exclusive");
    }

    let exit_code = run(exec.as_deref(), mcp, open_path.as_deref());
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

/// 返回进程退出码（0 成功 / 非 0 失败）。
fn run(exec: Option<&str>, mcp: bool, open_path: Option<&str>) -> i32 {
    // MCP 优先代理到已打开的 GUI 实例（OpenCADStudio 模式）：握手文件存在
    // 且 connect+token 验活成功 → 纯转发模式，工具实时操作窗口画布。
    // 失败 → stderr 说明后降级 headless。文件参数在代理模式下无意义（GUI
    // 有自己的当前文档），忽略之。
    if mcp {
        if let Some(stream) = mcp::try_connect_gui() {
            if open_path.is_some() {
                eprintln!("preferz-cli: FILE argument is ignored in GUI proxy mode");
            }
            return mcp::proxy_loop(stream);
        }
        eprintln!("preferz-cli: no running GUI instance found; using headless MCP");
    }

    let mut doc = CliDoc::new();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    // 启动时打开文件（失败：输出一行失败响应并退出）。
    if let Some(path) = open_path {
        let req = json!({ "cmd": "open", "path": path });
        let resp = dispatch(&mut doc, &req);
        if !resp["ok"].as_bool().unwrap_or(false) {
            // MCP 模式 stdout 只走协议帧，启动失败走 stderr。
            if mcp {
                eprintln!(
                    "preferz-cli: open failed: {}",
                    resp["error"].as_str().unwrap_or("unknown error")
                );
            } else {
                write_line(&mut out, &resp);
            }
            return 2;
        }
        // --exec 模式下打开成功不必回显，直接执行请求；REPL 模式下回显打开结果。
        if !mcp && exec.is_none() {
            write_line(&mut out, &resp);
        }
    }

    if mcp {
        return mcp::headless_loop(&mut doc);
    }

    match exec {
        // 单条模式：执行 → 输出 → 按 ok 决定退出码。
        Some(json_str) => {
            let resp = match serde_json::from_str::<Value>(json_str) {
                Ok(req) => dispatch(&mut doc, &req),
                Err(e) => protocol::fail("<parse>", &Value::Null, format!("invalid JSON: {e}")),
            };
            let ok = resp["ok"].as_bool().unwrap_or(false);
            write_line(&mut out, &resp);
            if ok {
                0
            } else {
                1
            }
        }
        // REPL：逐行读取直到 EOF。
        None => {
            let stdin = io::stdin();
            for line in stdin.lock().lines() {
                let Ok(line) = line else { break };
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let resp = match serde_json::from_str::<Value>(trimmed) {
                    Ok(req) => dispatch(&mut doc, &req),
                    Err(e) => protocol::fail("<parse>", &Value::Null, format!("invalid JSON: {e}")),
                };
                write_line(&mut out, &resp);
            }
            0
        }
    }
}

/// 单行紧凑 JSON 输出并立即 flush（agent 在等响应，不能攒在缓冲区里）。
fn write_line(out: &mut impl Write, resp: &Value) {
    if let Ok(line) = serde_json::to_string(resp) {
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }
}

fn fatal_exit(msg: &str) -> ! {
    eprintln!("preferz-cli: {msg}");
    std::process::exit(2);
}
