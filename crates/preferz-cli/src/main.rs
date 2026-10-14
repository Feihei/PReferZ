//! preferz-cli — PReferZ 的无头命令接口（JSONL，面向 agent / 脚本）。
//!
//! 用法：
//! - `preferz-cli [file.prz]`            打开文件（可选）后进入 stdin/stdout JSONL 会话
//! - `preferz-cli --exec '<json>' [file]` 只执行一条命令并退出
//! - `preferz-cli --help` / `--version`
//!
//! 协议：stdin 每行一个 JSON 请求 `{"cmd": "...", ...}`，stdout 每行一个 JSON
//! 响应。可选 `req_id`（任意 JSON 值）会原样回显，用于配对请求/响应。
//! 命令失败只产生 `{"ok": false, ...}`，不退出会话。
//!
//! 示例（PowerShell）：
//! ```text
//! '{"cmd":"open","path":"board.prz"}' | preferz-cli
//! '{"cmd":"list"}' | preferz-cli board.prz
//! preferz-cli --exec '{"cmd":"add_shape","shape":"rect","x":0,"y":0,"w":10,"h":10}' board.prz
//! ```

mod dispatch;
mod doc;
mod protocol;

use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

use crate::doc::CliDoc;

const HELP: &str = "\
preferz-cli — headless JSONL interface for PReferZ

USAGE:
    preferz-cli [OPTIONS] [FILE]

ARGS:
    <FILE>              .prz file to open before starting the session

OPTIONS:
    --exec <JSON>       Execute one request and exit
    -h, --help          Print this help
    -V, --version       Print version

PROTOCOL (one JSON object per line):
    request:   {\"cmd\": \"<name>\", ...args, \"req_id\": <optional echo>}
    response:  {\"ok\": true, \"cmd\": \"<name>\", \"req_id\": <echo>, ...data}
               {\"ok\": false, \"cmd\": \"<name>\", \"req_id\": <echo>, \"error\": \"<message>\"}

COMMANDS:
    ping, version, new, open, save, stats, list, get,
    select, select_all, deselect,
    add_shape, add_text, add_item, delete, move, undo, redo

Send `{\"cmd\":\"ping\"}` to verify the connection.
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut exec: Option<String> = None;
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

    let exit_code = run(exec.as_deref(), open_path.as_deref());
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

/// 返回进程退出码（0 成功 / 非 0 失败）。
fn run(exec: Option<&str>, open_path: Option<&str>) -> i32 {
    let mut doc = CliDoc::new();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    // 启动时打开文件（失败：输出一行失败响应并退出）。
    if let Some(path) = open_path {
        let req = json!({ "cmd": "open", "path": path });
        let resp = dispatch::dispatch(&mut doc, &req);
        if !resp["ok"].as_bool().unwrap_or(false) {
            write_line(&mut out, &resp);
            return 2;
        }
        // --exec 模式下打开成功不必回显，直接执行请求；REPL 模式下回显打开结果。
        if exec.is_none() {
            write_line(&mut out, &resp);
        }
    }

    match exec {
        // 单条模式：执行 → 输出 → 按 ok 决定退出码。
        Some(json_str) => {
            let resp = match serde_json::from_str::<Value>(json_str) {
                Ok(req) => dispatch::dispatch(&mut doc, &req),
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
                    Ok(req) => dispatch::dispatch(&mut doc, &req),
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
