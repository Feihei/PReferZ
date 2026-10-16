//! RPC 层（L1，egui-free）：JSONL 协议 + MCP stdio JSON-RPC 的共享实现。
//!
//! CLI 与 GUI 通过 [`RpcDoc`] 适配器复用同一条 dispatch 通道：
//! - `preferz-cli`：headless `CliDoc`（单文档进程内会话）
//! - GUI（`preferz`）：`mcp_serve::GuiDoc`（已打开窗口的 `Session`）
//!
//! 所有变更走 `session.push(Command)` undo 通道；协议层永不 panic——
//! 任何错误都收敛成结构化失败响应。
//!
//! - [`protocol`] / [`dispatch`]：JSONL 协议（`{"cmd": ...}` 请求/响应）
//! - [`mcp`] / [`tools`]：MCP stdio JSON-RPC 2.0 双栈（2026-07-28 无状态
//!   spec + ≤2025-11-25 旧栈握手兜底），18 个工具一一转投 JSONL dispatch
//!   （`screenshot` 例外：GUI 侧异步截图，见 `RpcDoc::capture_screenshot`）

mod dispatch;
mod doc;
mod mcp;
pub mod protocol;
mod tools;

pub use dispatch::dispatch;
pub use doc::{RpcDoc, RpcImage, ScreenshotReply};
pub use mcp::{handle_message, PROTOCOL_VERSION};
pub use tools::tools_list_result;

/// 测试专用：内存版 [`RpcDoc`]（core 无法依赖 preferz-fileio，open/save 不可用）。
#[cfg(test)]
pub(crate) mod test_support;
