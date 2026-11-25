//! 跨平台路径解析。
//!
//! 只负责「用户主目录」这一层：配置、最近文件、MCP 握手文件都必须落到同一处，
//! 所以优先级只能有一份定义，不允许各调用点各自拼环境变量。

use std::path::PathBuf;

/// 用户主目录。
///
/// Windows 优先 `USERPROFILE`，Unix 优先 `HOME`（另一项仅作兜底）。
///
/// **不能无条件优先 `HOME`**：Windows 上的 Git Bash / MSYS 会把 `HOME` 设成
/// `/c/Users/foo` 这类 POSIX 路径，`PathBuf` 收到后按「当前盘符 + `\c\Users\foo`」
/// 解释——配置会漂移到磁盘根目录下的 `\c\...`，而且与资源管理器里的真实家目录
/// 分叉（两处各写一份，用户改了一处另一处不生效）。
pub(crate) fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
    }
}

/// `~/.preferz` 目录：`config.json` / `recent.json` / `mcp-endpoint.json` 都放在这里。
pub(crate) fn preferz_dir() -> Option<PathBuf> {
    Some(home_dir()?.join(".preferz"))
}
