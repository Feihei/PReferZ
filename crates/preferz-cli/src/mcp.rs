//! `--mcp` 模式：优先代理到已打开的 GUI 实例，无实例时降级 headless stdio。
//!
//! **代理桥（OpenCADStudio 模式）**：GUI 启动时在 `~/.preferz/mcp-endpoint.json`
//! 写入 `{port, token, pid}` 并监听 127.0.0.1。本进程启动时读握手文件 →
//! connect + 发 token 验活 → 成功则进入**纯转发模式**（stdin 行 → socket →
//! stdout 行），实际协议处理发生在 GUI 进程内，工具直接操作已打开窗口的
//! 画布并实时渲染。
//!
//! - 握手文件缺失 / 连接失败 / token 不符 → 降级为 headless stdio MCP
//!   （旧行为不变），stderr 说明降级原因，stdout 只走协议帧。
//! - 测试隔离：环境变量 `PREFERZ_MCP_ENDPOINT` 覆盖握手文件路径。

use std::io::{self, BufRead, BufReader, Write};
use std::net::{Shutdown, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use crate::doc::CliDoc;
use preferz_core::rpc::handle_message;

/// token 验活的读超时：GUI 未响应即视为不可用。
const AUTH_TIMEOUT: Duration = Duration::from_secs(2);

/// 握手文件路径：`PREFERZ_MCP_ENDPOINT` 优先，否则 `~/.preferz/mcp-endpoint.json`。
fn endpoint_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PREFERZ_MCP_ENDPOINT") {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)?;
    Some(home.join(".preferz").join("mcp-endpoint.json"))
}

/// 读握手文件 + connect + token 验活。任何一步失败都返回 `None`（降级）。
pub(crate) fn try_connect_gui() -> Option<TcpStream> {
    let raw = std::fs::read_to_string(endpoint_path()?).ok()?;
    let data: Value = serde_json::from_str(&raw).ok()?;
    let port = data.get("port")?.as_u64()? as u16;
    let token = data.get("token")?.as_str()?.to_string();

    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(AUTH_TIMEOUT)).ok()?;
    // auth 帧：GUI 校验 token 后回 "ok" 或关闭连接。
    writeln!(stream, "{{\"token\":\"{token}\"}}").ok()?;
    let mut line = String::new();
    // 用 clone + 独立 BufReader，读完即弃，避免缓冲吃掉后续协议帧。
    BufReader::new(stream.try_clone().ok()?)
        .read_line(&mut line)
        .ok()?;
    if line.trim() != "ok" {
        return None;
    }
    stream.set_read_timeout(None).ok()?;
    Some(stream)
}

/// 纯转发模式：stdin 行 → socket；socket 行 → stdout。行分隔 JSON，
/// 本进程不解析协议内容（协议处理在 GUI 侧，与 headless 完全同源）。
pub(crate) fn proxy_loop(stream: TcpStream) -> i32 {
    let mut writer = match stream.try_clone() {
        Ok(w) => w,
        Err(_) => return 1,
    };
    let reader = match stream.try_clone() {
        Ok(r) => BufReader::new(r),
        Err(_) => return 1,
    };

    // socket → stdout。GUI 退出（socket EOF）时整个进程随之退出——
    // agent 客户端对 GUI 的会话到此结束，不该残留在 headless 模式里。
    let pump = std::thread::spawn(move || {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if writeln!(out, "{line}").and_then(|_| out.flush()).is_err() {
                break;
            }
        }
        std::process::exit(0);
    });

    // stdin → socket。stdin EOF：半关写端，让 GUI 侧收尾，等剩余响应吐完。
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if writeln!(writer, "{line}")
            .and_then(|_| writer.flush())
            .is_err()
        {
            break;
        }
    }
    let _ = stream.shutdown(Shutdown::Write);
    // GUI 回完剩余响应后进程在 pump 线程里 exit(0)；join 只是兜底。
    let _ = pump.join();
    0
}

/// headless stdio 循环（降级路径，行为与代理桥之前完全一致）：
/// stdin 逐行读，stdout 逐行写协议帧。
pub(crate) fn headless_loop(doc: &mut CliDoc) -> i32 {
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
