use super::*;
use std::path::PathBuf;

// ─────────────────────────── 最近文件 ───────────────────────────

/// 最近文件列表的持久化路径：`~/.preferz/recent.json`。
pub(crate) fn recent_files_path() -> Option<PathBuf> {
    let home = dirs_or_home()?;
    Some(home.join(".preferz").join("recent.json"))
}

/// 用户配置的持久化路径：`~/.preferz/config.json`。
pub(crate) fn config_path() -> Option<PathBuf> {
    let home = dirs_or_home()?;
    Some(home.join(".preferz").join("config.json"))
}

/// 自动保存默认开关（plan #5）。
fn default_autosave_enabled() -> bool {
    true
}

/// 自动保存默认间隔秒数（plan #5）。
fn default_autosave_interval() -> u32 {
    30
}

/// 用户配置（语言 + 快捷键 + 主题；均带 `#[serde(default)]` 以便老配置兼容）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct UserConfig {
    #[serde(default)]
    pub(crate) lang: Lang,
    /// 旧版本遗留：不再消费（启动恒用 `Keymap::new()`，见 `PReferZApp::new`）。
    /// 保留字段是为了老配置文件能正常解析；序列化时写空表，避免把过期默认值
    /// 继续散播给未来的版本。
    #[serde(default)]
    pub(crate) keymap: KeymapMap,
    /// 主题模式（Light/Dark/Auto），缺省回退 `Dark`。
    #[serde(default)]
    pub(crate) theme: ThemeMode,
    /// 自动保存开关（plan #5），默认开。
    #[serde(default = "default_autosave_enabled")]
    pub(crate) autosave_enabled: bool,
    /// 自动保存 debounce 间隔秒数（plan #5），默认 30，最小 10。
    #[serde(default = "default_autosave_interval")]
    pub(crate) autosave_interval: u32,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            lang: Lang::default(),
            keymap: KeymapMap::default(),
            theme: ThemeMode::default(),
            autosave_enabled: default_autosave_enabled(),
            autosave_interval: default_autosave_interval(),
        }
    }
}

/// 从 `~/.preferz/config.json` 加载配置。文件不存在或解析失败时返回默认值。
pub(crate) fn load_config() -> UserConfig {
    let path = match config_path() {
        Some(p) => p,
        None => return UserConfig::default(),
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str::<UserConfig>(&content).unwrap_or_default(),
        Err(_) => UserConfig::default(),
    }
}

/// 保存配置到 `~/.preferz/config.json`。
pub(crate) fn save_config(cfg: &UserConfig) {
    let path = match config_path() {
        Some(p) => p,
        None => return,
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(path, json);
    }
}

/// 获取用户 home 目录（跨平台）。
pub(crate) fn dirs_or_home() -> Option<PathBuf> {
    // 优先用 std::env，回退到常见环境变量
    if let Some(home) = std::env::var_os("HOME") {
        return Some(PathBuf::from(home));
    }
    if let Some(userprofile) = std::env::var_os("USERPROFILE") {
        return Some(PathBuf::from(userprofile));
    }
    None
}

/// 从 `~/.preferz/recent.json` 加载最近文件列表。
/// 文件不存在或解析失败时返回空列表。
pub(crate) fn load_recent_files() -> Vec<PathBuf> {
    let path = match recent_files_path() {
        Some(p) => p,
        None => return Vec::new(),
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    #[derive(serde::Deserialize)]
    struct RecentFile {
        path: String,
    }
    #[derive(serde::Deserialize)]
    struct RecentFiles {
        files: Vec<RecentFile>,
    }
    match serde_json::from_str::<RecentFiles>(&content) {
        Ok(parsed) => parsed
            .files
            .into_iter()
            .map(|f| PathBuf::from(f.path))
            .filter(|p| p.exists())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// 保存最近文件列表到 `~/.preferz/recent.json`。
pub(crate) fn save_recent_files(files: &[PathBuf]) {
    let path = match recent_files_path() {
        Some(p) => p,
        None => return,
    };
    // 确保父目录存在
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    #[derive(serde::Serialize)]
    struct RecentFile {
        path: String,
    }
    #[derive(serde::Serialize)]
    struct RecentFiles {
        files: Vec<RecentFile>,
    }
    let recent = RecentFiles {
        files: files
            .iter()
            .map(|p| RecentFile {
                path: p.to_string_lossy().into_owned(),
            })
            .collect(),
    };
    if let Ok(json) = serde_json::to_string_pretty(&recent) {
        let _ = std::fs::write(path, json);
    }
}

/// 把路径添加到最近文件列表头部，去重，限制最多 10 条。
pub(crate) fn add_recent_file(files: &mut Vec<PathBuf>, path: PathBuf) {
    files.retain(|p| p != &path);
    files.insert(0, path);
    if files.len() > 10 {
        files.truncate(10);
    }
    save_recent_files(files);
}
