//! 键盘快捷键映射：可序列化、可重绑定、可持久化。
//!
//! 设计要点见 `.agents/CHANGELOG.md` §Phase 6（原 keymap-config-plan 已归档）。三条容易踩的约束：
//!
//! 1. [`KeyBind::on_release`] —— egui-winit 0.29 会在 `pressed=true` 时拦截
//!    `Ctrl+V`（`is_paste_command`），导致 `key_pressed(V)` 永不触发。粘贴必须
//!    用释放沿检测，所以绑定要能表达「按下触发 / 释放触发」两种语义。
//! 2. 修饰键**严格匹配**（ctrl/shift/alt 三者全等），否则 `Ctrl+Alt+S` 会误触发
//!    绑定在 `Ctrl+S` 上的动作。
//! 3. `Esc` 有硬兜底（D1-a）：无论怎么改绑，都能退出裁剪 / 绘制工具 / 取色器。
//!    兜底在 [`crate::preferz_app::PReferZApp::handle_shortcuts`] 里先于本模块查表。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// 可绑定的按键。自有枚举而非复用 `egui::Key`——后者不实现 `Serialize`，
/// 且落盘需要一个稳定的字面表示（用户改绑、升级后仍要能读回来）。
///
/// 只覆盖「可绑定」的子集，不必镜像 `egui::Key` 的全部变体。
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum BindKey {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Space,
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
}

impl BindKey {
    pub fn to_egui(self) -> egui::Key {
        use BindKey::*;
        match self {
            A => egui::Key::A,
            B => egui::Key::B,
            C => egui::Key::C,
            D => egui::Key::D,
            E => egui::Key::E,
            F => egui::Key::F,
            G => egui::Key::G,
            H => egui::Key::H,
            I => egui::Key::I,
            J => egui::Key::J,
            K => egui::Key::K,
            L => egui::Key::L,
            M => egui::Key::M,
            N => egui::Key::N,
            O => egui::Key::O,
            P => egui::Key::P,
            Q => egui::Key::Q,
            R => egui::Key::R,
            S => egui::Key::S,
            T => egui::Key::T,
            U => egui::Key::U,
            V => egui::Key::V,
            W => egui::Key::W,
            X => egui::Key::X,
            Y => egui::Key::Y,
            Z => egui::Key::Z,
            Num0 => egui::Key::Num0,
            Num1 => egui::Key::Num1,
            Num2 => egui::Key::Num2,
            Num3 => egui::Key::Num3,
            Num4 => egui::Key::Num4,
            Num5 => egui::Key::Num5,
            Num6 => egui::Key::Num6,
            Num7 => egui::Key::Num7,
            Num8 => egui::Key::Num8,
            Num9 => egui::Key::Num9,
            F1 => egui::Key::F1,
            F2 => egui::Key::F2,
            F3 => egui::Key::F3,
            F4 => egui::Key::F4,
            F5 => egui::Key::F5,
            F6 => egui::Key::F6,
            F7 => egui::Key::F7,
            F8 => egui::Key::F8,
            F9 => egui::Key::F9,
            F10 => egui::Key::F10,
            F11 => egui::Key::F11,
            F12 => egui::Key::F12,
            Space => egui::Key::Space,
            Enter => egui::Key::Enter,
            Escape => egui::Key::Escape,
            Tab => egui::Key::Tab,
            Backspace => egui::Key::Backspace,
            Delete => egui::Key::Delete,
            Insert => egui::Key::Insert,
            Home => egui::Key::Home,
            End => egui::Key::End,
            PageUp => egui::Key::PageUp,
            PageDown => egui::Key::PageDown,
            ArrowUp => egui::Key::ArrowUp,
            ArrowDown => egui::Key::ArrowDown,
            ArrowLeft => egui::Key::ArrowLeft,
            ArrowRight => egui::Key::ArrowRight,
        }
    }

    /// 反查；`egui::Key` 中属于「不可绑定」集合的变体返回 `None`。
    pub fn from_egui(key: egui::Key) -> Option<Self> {
        use BindKey::*;
        Some(match key {
            egui::Key::A => A,
            egui::Key::B => B,
            egui::Key::C => C,
            egui::Key::D => D,
            egui::Key::E => E,
            egui::Key::F => F,
            egui::Key::G => G,
            egui::Key::H => H,
            egui::Key::I => I,
            egui::Key::J => J,
            egui::Key::K => K,
            egui::Key::L => L,
            egui::Key::M => M,
            egui::Key::N => N,
            egui::Key::O => O,
            egui::Key::P => P,
            egui::Key::Q => Q,
            egui::Key::R => R,
            egui::Key::S => S,
            egui::Key::T => T,
            egui::Key::U => U,
            egui::Key::V => V,
            egui::Key::W => W,
            egui::Key::X => X,
            egui::Key::Y => Y,
            egui::Key::Z => Z,
            egui::Key::Num0 => Num0,
            egui::Key::Num1 => Num1,
            egui::Key::Num2 => Num2,
            egui::Key::Num3 => Num3,
            egui::Key::Num4 => Num4,
            egui::Key::Num5 => Num5,
            egui::Key::Num6 => Num6,
            egui::Key::Num7 => Num7,
            egui::Key::Num8 => Num8,
            egui::Key::Num9 => Num9,
            egui::Key::F1 => F1,
            egui::Key::F2 => F2,
            egui::Key::F3 => F3,
            egui::Key::F4 => F4,
            egui::Key::F5 => F5,
            egui::Key::F6 => F6,
            egui::Key::F7 => F7,
            egui::Key::F8 => F8,
            egui::Key::F9 => F9,
            egui::Key::F10 => F10,
            egui::Key::F11 => F11,
            egui::Key::F12 => F12,
            egui::Key::Space => Space,
            egui::Key::Enter => Enter,
            egui::Key::Escape => Escape,
            egui::Key::Tab => Tab,
            egui::Key::Backspace => Backspace,
            egui::Key::Delete => Delete,
            egui::Key::Insert => Insert,
            egui::Key::Home => Home,
            egui::Key::End => End,
            egui::Key::PageUp => PageUp,
            egui::Key::PageDown => PageDown,
            egui::Key::ArrowUp => ArrowUp,
            egui::Key::ArrowDown => ArrowDown,
            egui::Key::ArrowLeft => ArrowLeft,
            egui::Key::ArrowRight => ArrowRight,
            _ => return None,
        })
    }

    /// UI 显示名。字母/数字/Fn 用裸名，其余用可读名。
    pub fn display(self) -> &'static str {
        use BindKey::*;
        match self {
            A => "A",
            B => "B",
            C => "C",
            D => "D",
            E => "E",
            F => "F",
            G => "G",
            H => "H",
            I => "I",
            J => "J",
            K => "K",
            L => "L",
            M => "M",
            N => "N",
            O => "O",
            P => "P",
            Q => "Q",
            R => "R",
            S => "S",
            T => "T",
            U => "U",
            V => "V",
            W => "W",
            X => "X",
            Y => "Y",
            Z => "Z",
            Num0 => "0",
            Num1 => "1",
            Num2 => "2",
            Num3 => "3",
            Num4 => "4",
            Num5 => "5",
            Num6 => "6",
            Num7 => "7",
            Num8 => "8",
            Num9 => "9",
            F1 => "F1",
            F2 => "F2",
            F3 => "F3",
            F4 => "F4",
            F5 => "F5",
            F6 => "F6",
            F7 => "F7",
            F8 => "F8",
            F9 => "F9",
            F10 => "F10",
            F11 => "F11",
            F12 => "F12",
            Space => "Space",
            Enter => "Enter",
            Escape => "Esc",
            Tab => "Tab",
            Backspace => "Backspace",
            Delete => "Del",
            Insert => "Insert",
            Home => "Home",
            End => "End",
            PageUp => "PgUp",
            PageDown => "PgDn",
            ArrowUp => "↑",
            ArrowDown => "↓",
            ArrowLeft => "←",
            ArrowRight => "→",
        }
    }
}

/// 一个完整绑定：按键 + 修饰键 + 触发沿。
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct KeyBind {
    pub key: BindKey,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// `true` = 释放时触发。目前只有 `Ctrl+V` 需要，见模块文档第 1 条。
    #[serde(default)]
    pub on_release: bool,
}

impl KeyBind {
    const fn new(key: BindKey) -> Self {
        Self {
            key,
            ctrl: false,
            shift: false,
            alt: false,
            on_release: false,
        }
    }

    const fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    const fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    const fn on_release(mut self) -> Self {
        self.on_release = true;
        self
    }

    /// `"Ctrl+Shift+S"` 这样的展示串。顺序固定为 Ctrl → Alt → Shift → 键。
    pub fn display(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        s.push_str(self.key.display());
        s
    }
}

/// 可绑定的动作。变体即配置文件的 key，改名会破坏既有用户的配置。
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Action {
    // 文件
    NewCanvas,
    OpenProject,
    LoadImage,
    Save,
    SaveAs,
    // 编辑
    Undo,
    Redo,
    Paste,
    DeleteSelected,
    // 视图
    FitToScreen,
    TogglePresent,
    // 工具
    ToolSelect,
    ToolRect,
    ToolEllipse,
    ToolDiamond,
    ToolLine,
    ToolArrow,
    ToolFrame,
    ToolPolygon,
    // 模式
    Crop,
    ColorPicker,
    ContextMenu,
    // Present 导航（TogglePresent 进入后生效）
    PresentNext,
    PresentPrev,
    PresentFirst,
    PresentLast,
    // 通用取消（Esc 语义；另有硬兜底，见模块文档第 3 条）
    Cancel,
    // 通用确认（裁剪模式应用 / 文本编辑提交）
    Confirm,
    /// 编辑选中项的文本：Text 直接改内容，封闭 Shape 编辑/新建其绑定文本。
    ///
    /// 与 `Confirm` 默认同为 Enter —— 裁剪模式下 Enter 由 `Confirm` 先消费
    /// （`handle_shortcuts` 的分支顺序保证），两者不冲突，也能各自改绑。
    EditText,
}

impl Action {
    /// 全部动作，供设置面板列出与「恢复默认」重建。
    pub const ALL: &'static [Action] = &[
        Action::NewCanvas,
        Action::OpenProject,
        Action::LoadImage,
        Action::Save,
        Action::SaveAs,
        Action::Undo,
        Action::Redo,
        Action::Paste,
        Action::DeleteSelected,
        Action::FitToScreen,
        Action::TogglePresent,
        Action::ToolSelect,
        Action::ToolRect,
        Action::ToolEllipse,
        Action::ToolDiamond,
        Action::ToolLine,
        Action::ToolArrow,
        Action::ToolFrame,
        Action::ToolPolygon,
        Action::Crop,
        Action::ColorPicker,
        Action::ContextMenu,
        Action::PresentNext,
        Action::PresentPrev,
        Action::PresentFirst,
        Action::PresentLast,
        Action::Cancel,
        Action::Confirm,
        Action::EditText,
    ];

    /// 出厂默认绑定。一个动作可有多个绑定（如翻页的三组键）。
    fn default_binds(self) -> Vec<KeyBind> {
        use Action::*;
        use BindKey::*;
        match self {
            NewCanvas => vec![KeyBind::new(N).ctrl()],
            OpenProject => vec![KeyBind::new(O).ctrl()],
            LoadImage => vec![KeyBind::new(I).ctrl()],
            Save => vec![KeyBind::new(S).ctrl()],
            SaveAs => vec![KeyBind::new(S).ctrl().shift()],
            Undo => vec![KeyBind::new(Z).ctrl()],
            // 重做保留两个绑定：Ctrl+Shift+Z 与 Ctrl+Y（与改造前行为一致）
            Redo => vec![KeyBind::new(Z).ctrl().shift(), KeyBind::new(Y).ctrl()],
            // 粘贴必须用释放沿，见模块文档第 1 条
            Paste => vec![KeyBind::new(V).ctrl().on_release()],
            DeleteSelected => vec![KeyBind::new(Delete)],
            FitToScreen => vec![KeyBind::new(F)],
            TogglePresent => vec![KeyBind::new(F5)],
            ToolSelect => vec![KeyBind::new(V)],
            ToolRect => vec![KeyBind::new(R)],
            ToolEllipse => vec![KeyBind::new(O)],
            ToolDiamond => vec![KeyBind::new(D)],
            ToolLine => vec![KeyBind::new(L)],
            ToolArrow => vec![KeyBind::new(A)],
            ToolFrame => vec![KeyBind::new(M)],
            // 多边形（Phase I）：Excalidraw 没有独立的多边形工具（它是折线的闭合态），
            // 故无官方键位可对。裸 P 在 Excalidraw 是 freedraw（本项目未实现），
            // 占它会让 Excalidraw 用户按错，故用 Shift+P 让开那个字母位。
            ToolPolygon => vec![KeyBind::new(P).shift()],
            Crop => vec![KeyBind::new(C)],
            ColorPicker => vec![KeyBind::new(I)],
            ContextMenu => vec![KeyBind::new(P).ctrl().shift()],
            // Present 导航：三个键都翻下一页（与改造前一致）
            PresentNext => vec![
                KeyBind::new(ArrowRight),
                KeyBind::new(Space),
                KeyBind::new(PageDown),
            ],
            PresentPrev => vec![KeyBind::new(ArrowLeft), KeyBind::new(PageUp)],
            PresentFirst => vec![KeyBind::new(Home)],
            PresentLast => vec![KeyBind::new(End)],
            Cancel => vec![KeyBind::new(Escape)],
            Confirm => vec![KeyBind::new(Enter)],
            // Excalidraw 语义：选中对象后按 Enter 直接进入文字编辑
            EditText => vec![KeyBind::new(Enter)],
        }
    }
}

/// 动作 → 绑定列表。序列化时就是这张表，缺失的动作回落到默认值。
pub type KeymapMap = HashMap<Action, Vec<KeyBind>>;

#[derive(Debug, Clone, Default)]
pub struct Keymap {
    map: KeymapMap,
}

impl Keymap {
    /// 出厂默认。
    pub fn default_map() -> KeymapMap {
        Action::ALL
            .iter()
            .map(|a| (*a, a.default_binds()))
            .collect()
    }

    pub fn new() -> Self {
        Self {
            map: Self::default_map(),
        }
    }

    /// 从持久化的部分表构造：已存的照用，缺失或为空的补默认值。
    /// 老配置文件里没有 `keymap` 字段时也会走到这里。
    pub fn from_partial(mut partial: KeymapMap) -> Self {
        for action in Action::ALL {
            match partial.get(action) {
                Some(binds) if !binds.is_empty() => {}
                _ => {
                    partial.insert(*action, action.default_binds());
                }
            }
        }
        Self { map: partial }
    }

    pub fn bindings(&self, action: Action) -> &[KeyBind] {
        self.map.get(&action).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// 该动作当前是否被触发。修饰键**严格匹配**，见模块文档第 2 条。
    pub fn pressed(&self, action: Action, ctx: &egui::Context) -> bool {
        let binds = self.bindings(action);
        if binds.is_empty() {
            return false;
        }
        ctx.input(|i| {
            binds.iter().any(|b| {
                let edge = if b.on_release {
                    i.key_released(b.key.to_egui())
                } else {
                    i.key_pressed(b.key.to_egui())
                };
                edge && i.modifiers.ctrl == b.ctrl
                    && i.modifiers.shift == b.shift
                    && i.modifiers.alt == b.alt
            })
        })
    }

    /// 把某动作改绑为 `bind`，并按 D2-a 覆盖策略清理冲突：
    /// 其它动作若占用同一组合（且不是本动作），移除之，返回被挤掉的动作列表。
    pub fn rebind(&mut self, action: Action, index: usize, bind: KeyBind) -> Vec<Action> {
        let mut evicted = Vec::new();
        for other in Action::ALL {
            if *other == action {
                continue;
            }
            if let Some(binds) = self.map.get_mut(other) {
                let before = binds.len();
                binds.retain(|b| b != &bind);
                if binds.len() < before {
                    evicted.push(*other);
                }
            }
        }
        if let Some(binds) = self.map.get_mut(&action) {
            if index < binds.len() {
                // 触发沿是动作/槽位的属性（受 egui-winit 拦截约束），改绑只换键不换沿
                let mut new = bind;
                new.on_release = binds[index].on_release;
                binds[index] = new;
            } else {
                binds.push(bind);
            }
        } else {
            self.map.insert(action, vec![bind]);
        }
        evicted
    }

    /// 用「捕获到的按键 + 修饰键」改绑，触发沿沿用该位置原有绑定。
    ///
    /// 触发沿是**动作**的属性而非按键的属性：`Ctrl+V` 被 egui-winit 拦截按下事件，
    /// 只能用释放沿（见模块文档第 1 条）。若捕获时一律置为按下沿，用户把粘贴改绑回
    /// `Ctrl+V` 就会得到一个永远触发不了的绑定。
    pub fn rebind_captured(
        &mut self,
        action: Action,
        index: usize,
        key: BindKey,
        m: egui::Modifiers,
    ) -> Vec<Action> {
        let on_release = self
            .bindings(action)
            .get(index)
            .or_else(|| self.bindings(action).first())
            .map(|b| b.on_release)
            .unwrap_or(false);
        self.rebind(
            action,
            index,
            KeyBind {
                key,
                ctrl: m.ctrl,
                shift: m.shift,
                alt: m.alt,
                on_release,
            },
        )
    }

    /// 该组合当前被哪个动作占用（供冲突提示）。
    pub fn owner_of(&self, bind: &KeyBind) -> Option<Action> {
        self.map
            .iter()
            .find(|(_, binds)| binds.contains(bind))
            .map(|(a, _)| *a)
    }

    /// 用于持久化的裸表。
    pub fn as_map(&self) -> &KeymapMap {
        &self.map
    }

    /// 恢复出厂默认（保留当前对象身份）。
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_map_covers_every_action() {
        let map = Keymap::default_map();
        for action in Action::ALL {
            assert!(
                !map.get(action).map(Vec::is_empty).unwrap_or(true),
                "{action:?} 缺少默认绑定"
            );
        }
    }

    /// 允许共用同一默认组合的动作对。
    ///
    /// `Confirm`（裁剪应用）与 `EditText`（编辑选中项文本）都默认挂在 Enter 上，
    /// 因为它们永远不会同时待命：裁剪模式下 Enter 由 `Confirm` 消费后立刻 return，
    /// 走不到 `EditText`（见 `handle_shortcuts` 的分支顺序）。
    /// 放在白名单里，是为了让"默认表不许抢键"这条不变量对其它动作依然成立。
    const SHARED_DEFAULT_BINDS: &[(Action, Action)] = &[(Action::Confirm, Action::EditText)];

    fn allows_shared(a: Action, b: Action) -> bool {
        SHARED_DEFAULT_BINDS
            .iter()
            .any(|(x, y)| (*x == a && *y == b) || (*x == b && *y == a))
    }

    #[test]
    fn no_duplicate_bindings_in_defaults() {
        // 出厂默认里若有两个动作抢同一组合，说明默认表本身有 bug
        // （唯一例外见 SHARED_DEFAULT_BINDS，且必须在文档里写明为何安全）
        let km = Keymap::new();
        let mut seen: HashMap<KeyBind, Action> = HashMap::new();
        for action in Action::ALL {
            for bind in km.bindings(*action) {
                if let Some(prev) = seen.insert(*bind, *action) {
                    assert!(
                        allows_shared(prev, *action),
                        "默认绑定冲突: {bind:?} 同时属于 {prev:?} 和 {action:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn edit_text_defaults_to_enter() {
        // Excalidraw 语义：选中对象后按 Enter 直接进文字编辑
        let km = Keymap::new();
        let binds = km.bindings(Action::EditText);
        assert_eq!(binds.len(), 1);
        assert_eq!(binds[0].key, BindKey::Enter);
        assert!(
            !binds[0].ctrl && !binds[0].shift && !binds[0].alt,
            "裸 Enter"
        );
    }

    #[test]
    fn paste_uses_release_edge() {
        // Ctrl+V 必须用释放沿，否则 egui-winit 拦截后永远触发不了
        let km = Keymap::new();
        let paste = km.bindings(Action::Paste);
        assert_eq!(paste.len(), 1);
        assert!(paste[0].on_release, "Ctrl+V 必须是释放触发");
        assert!(paste[0].ctrl);
        assert_eq!(paste[0].key, BindKey::V);
    }

    #[test]
    fn redo_and_present_nav_keep_multiple_bindings() {
        let km = Keymap::new();
        assert_eq!(km.bindings(Action::Redo).len(), 2, "Ctrl+Shift+Z 与 Ctrl+Y");
        assert_eq!(
            km.bindings(Action::PresentNext).len(),
            3,
            "→/Space/PgDn 都翻下一页"
        );
        assert_eq!(km.bindings(Action::PresentPrev).len(), 2);
    }

    #[test]
    fn from_partial_backfills_missing_actions() {
        // 模拟老配置：只有 lang，没有 keymap
        let mut partial: KeymapMap = HashMap::new();
        partial.insert(Action::Save, vec![KeyBind::new(BindKey::G).ctrl()]);
        let km = Keymap::from_partial(partial);
        // 自定义项保留
        assert_eq!(km.bindings(Action::Save)[0].key, BindKey::G);
        // 缺失项回落到默认
        assert_eq!(km.bindings(Action::Undo)[0].key, BindKey::Z);
        for action in Action::ALL {
            assert!(!km.bindings(*action).is_empty(), "{action:?} 未补齐");
        }
    }

    #[test]
    fn from_partial_ignores_empty_vec() {
        let mut partial: KeymapMap = HashMap::new();
        partial.insert(Action::Undo, vec![]);
        let km = Keymap::from_partial(partial);
        assert_eq!(km.bindings(Action::Undo)[0].key, BindKey::Z);
    }

    #[test]
    fn rebind_evicts_conflicting_owner() {
        let mut km = Keymap::new();
        // 把「适应画布」改绑到 Ctrl+S，应挤掉「保存」
        let evicted = km.rebind(Action::FitToScreen, 0, KeyBind::new(BindKey::S).ctrl());
        assert_eq!(evicted, vec![Action::Save]);
        assert!(km.bindings(Action::Save).is_empty(), "保存应变为未绑定");
        assert_eq!(
            km.owner_of(&KeyBind::new(BindKey::S).ctrl()),
            Some(Action::FitToScreen)
        );
    }

    #[test]
    fn rebind_same_action_does_not_self_evict() {
        let mut km = Keymap::new();
        // Redo 有两个绑定，改其中一个不应把另一个挤掉
        let evicted = km.rebind(Action::Redo, 0, KeyBind::new(BindKey::K).ctrl());
        assert!(evicted.is_empty());
        assert_eq!(km.bindings(Action::Redo).len(), 2);
    }

    #[test]
    fn captured_rebind_inherits_release_edge() {
        // 把 Paste 改绑到 Ctrl+V：触发沿必须从原绑定继承，否则永远触发不了
        let mut km = Keymap::new();
        km.rebind(Action::Paste, 0, KeyBind::new(BindKey::X).ctrl());
        assert!(
            km.bindings(Action::Paste)[0].on_release,
            "改绑后应沿用释放沿"
        );
        // 捕获式改绑（模拟 UI 捕获到 Ctrl+V）
        let evicted = km.rebind_captured(
            Action::Paste,
            0,
            BindKey::V,
            egui::Modifiers {
                ctrl: true,
                ..egui::Modifiers::NONE
            },
        );
        assert!(evicted.is_empty());
        let bind = km.bindings(Action::Paste)[0];
        assert_eq!(bind.key, BindKey::V);
        assert!(bind.ctrl && bind.on_release);
    }

    #[test]
    fn captured_rebind_does_not_force_release_edge() {
        // 普通动作（按下沿）改绑后不应被污染成释放沿
        let mut km = Keymap::new();
        km.rebind_captured(Action::Save, 0, BindKey::G, egui::Modifiers::CTRL);
        assert!(!km.bindings(Action::Save)[0].on_release);
    }

    #[test]
    fn reset_restores_defaults() {
        let mut km = Keymap::new();
        km.rebind(Action::Undo, 0, KeyBind::new(BindKey::Q).ctrl());
        assert_eq!(km.bindings(Action::Undo)[0].key, BindKey::Q);
        km.reset();
        assert_eq!(km.bindings(Action::Undo)[0].key, BindKey::Z);
    }

    #[test]
    fn bindkey_roundtrips_through_egui() {
        for action in Action::ALL {
            for bind in action.default_binds() {
                let back = BindKey::from_egui(bind.key.to_egui())
                    .unwrap_or_else(|| panic!("{:?} 无法反查", bind.key));
                assert_eq!(back, bind.key);
            }
        }
    }

    #[test]
    fn display_includes_modifiers_in_fixed_order() {
        let bind = KeyBind::new(BindKey::S).ctrl().shift();
        assert_eq!(bind.display(), "Ctrl+Shift+S");
        let bind2 = KeyBind {
            alt: true,
            ..KeyBind::new(BindKey::End)
        };
        assert_eq!(bind2.display(), "Alt+End");
    }

    #[test]
    fn serde_roundtrip_of_keymap_map() {
        let km = Keymap::new();
        let json = serde_json::to_string(km.as_map()).unwrap();
        let back: KeymapMap = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), km.as_map().len());
        for action in Action::ALL {
            assert_eq!(
                back.get(action),
                km.as_map().get(action),
                "{action:?} 往返不一致"
            );
        }
    }
}
