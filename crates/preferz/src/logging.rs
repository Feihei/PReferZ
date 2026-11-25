//! 日志初始化（`log` 门面的后端装配）。
//!
//! 此前 `env_logger` 只是躺在依赖里、从未 `init()`，于是所有 `log::warn!` /
//! `log::error!` 都被静默丢弃——用户报问题时没有任何可回传的线索。
//!
//! 两个平台事实决定了这里的写法：
//!
//! 1. **release 的 GUI 没有可见 stderr**：Windows 侧是 `windows_subsystem =
//!    "windows"`（不分配控制台），macOS 侧由 Finder / LaunchServices 启动时
//!    stderr 只进系统日志（Console.app），普通用户拿不到。所以默认落文件。
//! 2. **开发期要在终端里立刻看到输出**：只要显式设了 `RUST_LOG` 就保持 stderr，
//!    不再写文件——避免开发时一半日志在终端、一半在文件里。

use std::fs::{self, OpenOptions};
use std::path::Path;

/// 日志文件名，落在 `~/.preferz/preferz.log`（与 config.json 同目录）。
const LOG_FILE: &str = "preferz.log";
/// 单文件上限。超过就在**启动时**轮转成 `preferz.log.1`（只保留一份历史），
/// 免得长期使用后无限增长。
const MAX_LOG_BYTES: u64 = 1024 * 1024;
/// 未设 `RUST_LOG` 时的默认级别。
///
/// 取 `warn`：既能收到全部 `warn!` / `error!`，又不会被 info 级噪声淹掉。
const DEFAULT_FILTER: &str = "warn";

/// 装配日志后端。可重复调用（第二次静默失败，不 panic）。
pub fn init() {
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(DEFAULT_FILTER));

    // RUST_LOG 显式设置 = 开发者在看终端；否则写文件供用户回传。
    if std::env::var_os("RUST_LOG").is_none() {
        if let Some(file) = open_log_file() {
            builder.target(env_logger::Target::Pipe(Box::new(file)));
        }
    }

    // 已经初始化过（例如将来的测试）时忽略错误，而不是 `init()` 那样 panic。
    let _ = builder.try_init();
}

/// 以追加模式打开 `~/.preferz/preferz.log`；目录不可写时返回 `None`，
/// 调用方保持默认的 stderr 输出即可。
fn open_log_file() -> Option<fs::File> {
    let path = crate::paths::preferz_dir()?.join(LOG_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok()?;
    }
    rotate_if_large(&path);
    OpenOptions::new().create(true).append(true).open(path).ok()
}

/// 超过 [`MAX_LOG_BYTES`] 就把当前文件挪成 `.1` 并重新开始。
///
/// Windows 上 `rename` 不会覆盖已存在的目标，所以先删旧的 `.1`；两步都失败
/// 也不影响主流程（继续往原文件追加，最坏是文件偏大）。
fn rotate_if_large(path: &Path) {
    let too_big = fs::metadata(path)
        .map(|m| m.len() > MAX_LOG_BYTES)
        .unwrap_or(false);
    if !too_big {
        return;
    }
    let old = path.with_extension("log.1");
    let _ = fs::remove_file(&old);
    let _ = fs::rename(path, old);
}
