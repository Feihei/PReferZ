//! link — 元素超链接（plan #6）的打开 / 编辑动作段。
//! `use super::*;` 取 mod.rs 的模块级词汇与私有项（子模块可访问父模块私有）。
use super::*;

impl PReferZApp {
    /// 打开 item 的链接（badge 点击入口）。按 [`classify_link`] 分流：
    /// web URL → 系统默认浏览器；本地 `.prz` → spawn 新窗口（H2）；
    /// 无法识别 / 目标不存在 → flash 提示后放弃。
    pub(crate) fn open_item_link(&mut self, id: ItemId) {
        let Some(item) = self.scene.get_item(&id) else {
            return;
        };
        let Some(link) = item.link.clone() else {
            return;
        };
        // 相对路径按当前 .prz 所在目录解析（H3）；无当前文件时按进程工作目录。
        let base_dir = self
            .current_file
            .as_ref()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf);
        match classify_link(&link, base_dir.as_deref()) {
            LinkTarget::Web(url) => {
                if open_url_in_browser(&url) {
                    self.flash(fill(t(self.lang, T::FlashLinkBrowser), &[url]));
                } else {
                    self.flash(fill(t(self.lang, T::FlashLinkInvalid), &[url]));
                }
            }
            LinkTarget::LocalPrz(path) => match spawn_project_window(&path) {
                Ok(_child) => {
                    self.flash(fill(
                        t(self.lang, T::FlashLinkNewWindow),
                        &[path.display().to_string()],
                    ));
                }
                Err(e) => {
                    self.flash(fill(t(self.lang, T::FlashLinkInvalid), &[e.to_string()]));
                }
            },
            LinkTarget::Invalid => {
                self.flash(fill(t(self.lang, T::FlashLinkInvalid), &[link]));
            }
        }
    }

    /// Ctrl+K / 右键菜单「添加/编辑链接」入口：确保属性栏可见并聚焦链接
    /// 输入框（输入框本帧渲染时认领焦点，见 props.rs 的 `link_input` Id）。
    pub(crate) fn focus_link_input(&mut self, ctx: &egui::Context) {
        self.props_visible = true;
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("link_input")));
        ctx.request_repaint();
    }

    /// 右键菜单「移除链接」：恰选中 1 项且有链接时清空（一条 undo 记录）。
    pub(crate) fn remove_selected_link(&mut self) {
        let Some(id) = self.single_selected_id() else {
            return;
        };
        let Some(old) = self.scene.get_item(&id).and_then(|it| it.link.clone()) else {
            return;
        };
        self.push_cmd(Box::new(SetLink::new(id, Some(old), None)));
        self.flash(t(self.lang, T::FlashLinkRemoved).to_string());
    }
}

/// 用系统默认浏览器打开 URL（零新依赖）：Windows 走 explorer.exe
///（不闪 console 窗口，系统按协议分派默认浏览器）；macOS 走 open；
/// Linux 走 xdg-open。
fn open_url_in_browser(url: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer.exe")
            .arg(url)
            .spawn()
            .is_ok()
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn().is_ok()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .is_ok()
    }
}

/// spawn 新窗口打开本地 `.prz`（H2）：运行自身 exe + 路径位置参数，
/// 与 `spawn_help_instance`（plan #25）同一机制。spawn 异步，新窗口稍后弹出。
fn spawn_project_window(path: &Path) -> std::io::Result<std::process::Child> {
    let exe = std::env::current_exe()?;
    std::process::Command::new(exe).arg(path).spawn()
}
