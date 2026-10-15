//! CLI 模式（Blender 式单二进制多模式）：`preferz` 的无头命令接口
//! （JSONL / MCP stdio，面向 agent / 脚本）。
//!
//! 由 main.rs 派发：`--mcp` / `--exec` / `--headless` 任一出现即进入本模块，
//! 不触碰 GUI（eframe 事件循环根本不启动）。
//!
//! 用法：
//! - `preferz --headless [file.prz]`     打开文件（可选）后进入 stdin/stdout JSONL 会话
//! - `preferz --exec '<json>' [file]`    只执行一条命令并退出
//! - `preferz --mcp [file.prz]`          MCP stdio 服务器：优先**代理**到已打开的
//!   GUI 实例（工具实时操作窗口画布），无 GUI 时降级为 headless 单文档服务器
//!
//! JSONL 协议：stdin 每行一个 JSON 请求 `{"cmd": "...", ...}`，stdout 每行一个
//! JSON 响应。可选 `req_id`（任意 JSON 值）会原样回显，用于配对请求/响应。
//! 命令失败只产生 `{"ok": false, ...}`，不退出会话。
//!
//! MCP 模式：stdio JSON-RPC，双栈协议——新栈 2026-07-28 无状态 spec 与旧栈
//! （≤2025-11-25 `initialize` 握手）按请求特征分流；17 个工具与 JSONL 命令
//! 一一对应，共享同一 undo 通道。协议核心在 preferz-core 的 `rpc` 模块，
//! 代理与 headless 两种模式共享同一套语义。stdout 只走协议帧，日志走 stderr。
//!
//! 示例（PowerShell）：
//! ```text
//! '{"cmd":"open","path":"board.prz"}' | preferz --headless
//! '{"cmd":"list"}' | preferz --headless board.prz
//! preferz --exec '{"cmd":"add_shape","shape":"rect","x":0,"y":0,"w":10,"h":10}' board.prz
//! preferz --mcp board.prz
//! ```

mod doc;
mod mcp;

use std::io::{self, BufRead, Write};

use preferz_core::rpc::{dispatch, protocol};
use serde_json::{json, Value};

use crate::cli::doc::CliDoc;

const HELP: &str = "\
preferz — headless JSONL / MCP stdio interface (Blender-style single binary)

USAGE:
    preferz --headless [OPTIONS] [FILE]
    preferz --exec <JSON> [FILE]
    preferz --mcp [FILE]
    preferz [FILE]              (no flags: open the GUI window)

OPTIONS:
    --headless                  Run the JSONL REPL on stdin/stdout (no GUI)
    --exec <JSON>               Execute one request and exit
    --mcp                       Run as a stdio MCP server (JSON-RPC 2.0). Proxies
                                to a running PReferZ GUI window when one is up
                                (tools then act on the opened canvas live);
                                falls back to a headless single-document server
                                otherwise.
    -h, --help                  Print this help
    -V, --version               Print version

ARGS:
    <FILE>              .prz file to open before starting the session

JSONL PROTOCOL (one JSON object per line):
    request:   {\"cmd\": \"<name>\", ...args, \"req_id\": <optional echo>}
    response:  {\"ok\": true, \"cmd\": \"<name>\", \"req_id\": <echo>, ...data}
               {\"ok\": false, \"cmd\": \"<name>\", \"req_id\": <echo>, \"error\": \"<message>\"}

COMMANDS:
    ping, version, new, open, save, stats, list,
    select, select_all, deselect,
    add_shape, add_text, add_item, add_mermaid, add_chart, add_image,
    arrange, delete, move, undo, redo

NOTES:
    - list: {\"ids\": [..]} filters, \"full\": true returns full item JSON
      (replaces the old get command).
    - select with an empty ids array deselects everything; select_all is a
      convenience wrapper.

Send `{\"cmd\":\"ping\"}` to verify the connection.
";

/// CLI 模式入口：解析参数并执行，返回进程退出码。
/// 仅在 main.rs 判定 `--mcp` / `--exec` / `--headless` 存在时调用。
pub fn run(args: &[String]) -> i32 {
    console::attach_parent_console_if_interactive();

    let mut exec: Option<String> = None;
    let mut mcp = false;
    let mut headless = false;
    let mut open_path: Option<String> = None;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--exec" => match iter.next() {
                Some(json) => exec = Some(json.clone()),
                None => return fatal_exit("--exec requires a JSON argument"),
            },
            "--mcp" => mcp = true,
            "--headless" => headless = true,
            other if other.starts_with("--") => {
                return fatal_exit(&format!("unknown option '{other}' (see --help)"));
            }
            other => {
                if open_path.is_some() {
                    return fatal_exit("only one file path may be given");
                }
                open_path = Some(other.to_string());
            }
        }
    }
    if mcp && exec.is_some() {
        return fatal_exit("--exec and --mcp are mutually exclusive");
    }
    if mcp && headless {
        return fatal_exit("--headless and --mcp are mutually exclusive");
    }
    // --exec 隐含 headless（单条执行本就是无头语义）。

    run_inner(exec.as_deref(), mcp, open_path.as_deref())
}

/// 打印 CLI 帮助（main.rs 在判定 CLI 模式前处理 -h/--help 时调用）。
pub fn print_help() {
    print!("{HELP}");
}

/// 返回进程退出码（0 成功 / 非 0 失败）。
fn run_inner(exec: Option<&str>, mcp: bool, open_path: Option<&str>) -> i32 {
    // MCP 优先代理到已打开的 GUI 实例（OpenCADStudio 模式）：握手文件存在
    // 且 connect+token 验活成功 → 纯转发模式，工具实时操作窗口画布。
    // 失败 → stderr 说明后降级 headless。文件参数在代理模式下无意义（GUI
    // 有自己的当前文档），忽略之。
    if mcp {
        if let Some(stream) = mcp::try_connect_gui() {
            if open_path.is_some() {
                eprintln!("preferz: FILE argument is ignored in GUI proxy mode");
            }
            return mcp::proxy_loop(stream);
        }
        eprintln!("preferz: no running GUI instance found; using headless MCP");
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
                    "preferz: open failed: {}",
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

fn fatal_exit(msg: &str) -> i32 {
    eprintln!("preferz: {msg}");
    2
}

/// release 构建（windows 子系统，无 console）下，若 stdin/stdout 均未重定向
/// （即从真实终端交互式调用 `preferz --headless` / `--exec`），补挂父进程
/// console 并重定向 std 句柄，使命令行输出可见、stdin 可输入。
///
/// - agent / `|` 管道场景：std 句柄已是有效管道，跳过——不能动它们；
/// - debug 构建（console 子系统）不编译本模块，行为不变。
///
/// 必须在任何 std IO 之前调用：Rust std 的 std 句柄是首次使用时经
/// `GetStdHandle` 惰性获取的，先 SetStdHandle 再用即可生效。零新依赖
/// （直接声明 kernel32 extern，与项目零依赖纪律一致）。
#[cfg(all(windows, not(debug_assertions)))]
mod console {
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    const STD_INPUT_HANDLE: u32 = -10i32 as u32;
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const OPEN_EXISTING: u32 = 3;
    const FILE_SHARE_READ_WRITE: u32 = 0x3;
    const INVALID_HANDLE_VALUE: isize = -1;

    extern "system" {
        fn AttachConsole(dwProcessId: u32) -> i32;
        fn GetStdHandle(nStdHandle: u32) -> isize;
        fn SetStdHandle(nStdHandle: u32, hHandle: isize) -> i32;
        fn CreateFileA(
            lpFileName: *const u8,
            dwDesiredAccess: u32,
            dwShareMode: u32,
            lpSecurityAttributes: isize,
            dwCreationDisposition: u32,
            dwFlagsAndAttributes: u32,
            hTemplateFile: isize,
        ) -> isize;
    }

    /// 重新打开 console 设备并接管对应 std 句柄。
    fn reopen(name: &[u8], access: u32, std_handle: u32) -> bool {
        unsafe {
            let h = CreateFileA(
                name.as_ptr(),
                access,
                FILE_SHARE_READ_WRITE,
                0,
                OPEN_EXISTING,
                0,
                0,
            );
            if h == 0 || h == INVALID_HANDLE_VALUE {
                return false;
            }
            SetStdHandle(std_handle, h) != 0
        }
    }

    pub fn attach_parent_console_if_interactive() {
        unsafe {
            // 任一 std 句柄有效（管道/已重定向）→ 不需要也不应该补挂。
            let out = GetStdHandle(STD_OUTPUT_HANDLE);
            let err = GetStdHandle(STD_ERROR_HANDLE);
            let redirected = (out != 0 && out != INVALID_HANDLE_VALUE)
                || (err != 0 && err != INVALID_HANDLE_VALUE);
            if redirected {
                return;
            }
            if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
                return; // 无父 console（双击启动等）→ 静默放弃
            }
            let _ = reopen(b"CONIN$\0", GENERIC_READ, STD_INPUT_HANDLE);
            let _ = reopen(b"CONOUT$\0", GENERIC_WRITE, STD_OUTPUT_HANDLE);
            let _ = reopen(b"CONOUT$\0", GENERIC_WRITE, STD_ERROR_HANDLE);
        }
    }
}

#[cfg(not(all(windows, not(debug_assertions))))]
mod console {
    /// 非 release-Windows（debug / 其他平台）console 子系统天然可用，空操作。
    pub fn attach_parent_console_if_interactive() {}
}
