//! 生成内置帮助文档 `assets/help.prz`（中文）与 `assets/help-en.prz`（英文）。
//!
//! 运行：`cargo run -p preferz --example gen_help_doc`
//!
//! 帮助页按基色空间设计：元素颜色存亮色基值，暗色主题由绘制期
//! `apply_dark_mode_filter` 整体翻转（文本/形状一致），无需垫纸张底版。
//! 上面排版警示横幅 + 七张功能卡片。画布文本为单行 Text item，逐行摆放。
//!
//! **两种语言共用同一套版位**（[`SLOTS`]），只有文案在 [`ZH`] / [`EN`] 里分开：
//! 翻译不会把布局改歪，改版式也只需动一处。因此两张卡片允许的条目数由版位高度
//! 决定（普通卡最多 6 行、`y + 52 + 5 * 30` 仍留白），改文案时别加行。
//!
//! 不要再手工在 PReferZ 里改画布然后「另存为」覆盖这两个文件——那样下次跑
//! 生成器就会被冲掉，而且中文版改了英文版不会跟着改。

use preferz_core::item::{Item, ItemKind};
use preferz_core::scene::Scene;
use preferz_core::shape::{DashStyle, FontFamily, ShapeType, Sloppiness, StrokeStyle};
use preferz_fileio::{PrzFile, ViewportMeta};

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 主题无关的基色配色（暗色主题由绘制期滤镜统一翻转）。
const INK: [u8; 4] = [50, 48, 44, 255];
const FAINT: [u8; 4] = [128, 124, 116, 255];
const ACCENT: [u8; 4] = [40, 92, 170, 255];
const WARN_TEXT: [u8; 4] = [172, 42, 34, 255];
const WARN_STROKE: [u8; 4] = [206, 74, 62, 255];
const WARN_FILL: [u8; 4] = [252, 236, 232, 255];
const CARD_STROKE: [u8; 4] = [191, 186, 176, 255];
const CARD_FILL: [u8; 4] = [255, 255, 255, 255];

fn add_text(
    scene: &mut Scene,
    content: &str,
    x: f32,
    y: f32,
    size: f32,
    color: [u8; 4],
    family: FontFamily,
) {
    let mut item = Item::new_text(content.to_string(), x, y, size, color);
    if let ItemKind::Text { font_family, .. } = &mut item.kind {
        *font_family = family;
    }
    scene.add_item(item);
}

#[allow(clippy::too_many_arguments)]
fn add_rect(
    scene: &mut Scene,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    stroke_color: [u8; 4],
    stroke_width: f32,
    fill: [u8; 4],
    roundness: f32,
    sloppiness: Sloppiness,
) {
    let mut item = Item::new_shape(
        ShapeType::Rectangle,
        (w, h),
        x,
        y,
        StrokeStyle {
            color: stroke_color,
            width: stroke_width,
            dash: DashStyle::Solid,
        },
        Some(fill),
    );
    if let ItemKind::Shape {
        roundness: r,
        sloppiness: s,
        ..
    } = &mut item.kind
    {
        *r = roundness;
        *s = sloppiness;
    }
    scene.add_item(item);
}

/// 卡片的版位（两种语言共用）。第 7 张是全宽 CLI·MCP 卡。
struct Slot {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

const SLOTS: [Slot; 7] = [
    Slot {
        x: 48.0,
        y: 264.0,
        w: 408.0,
        h: 260.0,
    },
    Slot {
        x: 504.0,
        y: 264.0,
        w: 408.0,
        h: 260.0,
    },
    Slot {
        x: 48.0,
        y: 548.0,
        w: 408.0,
        h: 260.0,
    },
    Slot {
        x: 504.0,
        y: 548.0,
        w: 408.0,
        h: 260.0,
    },
    Slot {
        x: 48.0,
        y: 832.0,
        w: 408.0,
        h: 260.0,
    },
    Slot {
        x: 504.0,
        y: 832.0,
        w: 408.0,
        h: 260.0,
    },
    Slot {
        x: 48.0,
        y: 1116.0,
        w: 864.0,
        h: 350.0,
    },
];

/// 一张卡片的文案。
struct CardCopy {
    title: &'static str,
    lines: &'static [&'static str],
}

/// 一种语言的整页文案。
struct LangCopy {
    /// 写进 `assets/` 的文件名。
    file: &'static str,
    title: &'static str,
    banner: &'static str,
    banner_sub: &'static str,
    /// 主键修饰的换算说明。帮助文档编译期烘进二进制、全平台共用一份，
    /// 所以这里不能写 `⌘`（Windows 用户看不懂），只能说明换算关系。
    platform_note: &'static str,
    cards: [CardCopy; 7],
    footer: &'static str,
}

const ZH: LangCopy = LangCopy {
    file: "help.prz",
    title: "PReferZ 帮助",
    banner: "※ 本窗口是内置帮助文档：对它的修改不会被保存，关闭后重开会还原原始内容。",
    banner_sub: "想保留修改：菜单 文件 → 另存为（Ctrl+Shift+S），即可存成普通 .prz 项目。",
    platform_note: "macOS 用户：文中 Ctrl 即 ⌘，Alt 即 Option；触控板双指平移、捏合缩放。",
    cards: [
        CardCopy {
            title: "视图",
            lines: &[
                "滚轮 缩放 · 中键拖拽 平移",
                "触控板 双指平移 · 捏合缩放",
                "Shift+1 适应画布内容",
                "Shift+2 缩放到选中 · Shift+3 缩放 100%",
                "F5 演示模式 · 方向键翻页",
                "Home / End 演示跳到首尾页",
            ],
        },
        CardCopy {
            title: "工具",
            lines: &[
                "V/1 选择 · R/2 矩形 · D/3 菱形",
                "O/4 椭圆 · A/5 直角箭头 · L/6 直线",
                "Shift+P 多边形 · P 徒手画",
                "8 文字 · F 画框",
                "C 裁剪 · I 取色器",
            ],
        },
        CardCopy {
            title: "编辑",
            lines: &[
                "Ctrl+A 全选 · Ctrl+Shift+A 取消全选",
                "Ctrl+C / X / V 复制 · 剪切 · 粘贴",
                "Ctrl+D 原位复制 · Delete/Backspace 删除",
                "Enter 编辑文字 · Esc 取消",
                "Ctrl+G 编组 · Ctrl+Shift+G 解组",
                "Ctrl+K 编辑超链接（网页 / .prz）",
            ],
        },
        CardCopy {
            title: "文件",
            lines: &[
                "Ctrl+N 新建 · Ctrl+O 打开",
                "Ctrl+I 导入图片 · Ctrl+S 保存",
                "Ctrl+Shift+S 另存为",
                "Ctrl+Shift+E 导出选区",
                "Ctrl+Q 退出 · Ctrl+W 关闭窗口",
                "拖放图片文件到画布即可导入",
            ],
        },
        CardCopy {
            title: "撤销 · 层级",
            lines: &[
                "Ctrl+Z 撤销 · Ctrl+Shift+Z / Ctrl+Y 重做",
                "Ctrl+] / Ctrl+[ 上移 / 下移一层",
                "Ctrl+Shift+] / Ctrl+Shift+[ 置顶 / 置底",
                "拖拽、缩放、改色都可撤销，放心试验",
            ],
        },
        CardCopy {
            title: "连接 · 面板",
            lines: &[
                "Ctrl+方向键 沿该向克隆节点并连线",
                "Alt+方向键 沿连接跳到相邻节点",
                "右键菜单 直角箭头烘焙为多段线",
                "点击元素右上 ↗ 角标打开其链接",
                "T 工具栏显隐 · N 属性栏显隐",
                "右键菜单 查看全部功能",
            ],
        },
        CardCopy {
            title: "CLI · MCP（面向 agent / AI 编程客户端）",
            lines: &[
                "preferz --mcp 启动 MCP 服务器：GUI 已打开时自动代理，工具实时操作当前画布窗口",
                "· 无已打开的 GUI 时自动降级为无头单文档模式，行为不变",
                "preferz --headless 进入 JSONL 会话（stdin/stdout，每行一个 JSON）",
                "preferz --exec '<json>' 执行单条命令后退出",
                "18 个工具与命令一一对应：open / save / list / add_shape / add_text",
                "  / add_mermaid / add_chart / add_image / arrange / move / undo / redo",
                "  / screenshot（把画布截图回传给 LLM 视觉确认成果）",
                "  …（详见 preferz --help）",
                "MCP 开关在左侧栏（帮助按钮之上）：绿点=本窗口正被控制，灰点=待命，空心=已关闭",
                "多开窗口时只有一个受控；点其它窗口的开关即切换，原窗口自动让位",
            ],
        },
    ],
    footer: "本窗口的内容可以随意涂改、试用——关闭重开会还原。",
};

const EN: LangCopy = LangCopy {
    file: "help-en.prz",
    title: "PReferZ Help",
    banner: "※ This window is the built-in help: edits here are not saved — reopen to restore the original.",
    banner_sub: "To keep changes: File → Save As (Ctrl+Shift+S) stores it as a regular .prz project.",
    platform_note:
        "On macOS: Ctrl means Cmd, Alt means Option. Trackpad: two-finger pan, pinch to zoom.",
    cards: [
        CardCopy {
            title: "View",
            lines: &[
                "Wheel zoom · Middle-drag pan",
                "Trackpad two-finger pan · pinch zoom",
                "Shift+1 zoom to fit content",
                "Shift+2 zoom to selection · Shift+3 100%",
                "F5 present mode · Arrows turn pages",
                "Home / End jump to first/last page",
            ],
        },
        CardCopy {
            title: "Tools",
            lines: &[
                "V/1 select · R/2 rectangle · D/3 diamond",
                "O/4 ellipse · A/5 arrow · L/6 line",
                "Shift+P polygon · P freedraw",
                "8 text · F frame",
                "C crop · I eyedropper",
            ],
        },
        CardCopy {
            title: "Edit",
            lines: &[
                "Ctrl+A select all · Ctrl+Shift+A deselect",
                "Ctrl+C / X / V copy · cut · paste",
                "Ctrl+D duplicate · Delete/Backspace remove",
                "Enter edit text · Esc cancel",
                "Ctrl+G group · Ctrl+Shift+G ungroup",
                "Ctrl+K edit link (web / .prz)",
            ],
        },
        CardCopy {
            title: "File",
            lines: &[
                "Ctrl+N new · Ctrl+O open",
                "Ctrl+I import image · Ctrl+S save",
                "Ctrl+Shift+S save as",
                "Ctrl+Shift+E export selection",
                "Ctrl+Q quit · Ctrl+W close window",
                "Drop image files onto the canvas to import",
            ],
        },
        CardCopy {
            title: "Undo · Z-order",
            lines: &[
                "Ctrl+Z undo · Ctrl+Shift+Z / Ctrl+Y redo",
                "Ctrl+] / Ctrl+[ raise / lower one",
                "Ctrl+Shift+] / Ctrl+Shift+[ front / back",
                "Drag, resize, recolor — all undoable",
            ],
        },
        CardCopy {
            title: "Links · Panels",
            lines: &[
                "Ctrl+Arrow clone & link that way",
                "Alt+Arrow jump to the linked neighbor",
                "Right-click bakes elbow arrows to polylines",
                "Click the ↗ badge to open an item's link",
                "T toggle toolbar · N toggle props",
                "Context menu lists all actions",
            ],
        },
        CardCopy {
            title: "CLI · MCP (for agents / AI coding clients)",
            lines: &[
                "preferz --mcp starts the MCP server: with a GUI open it proxies and drives the live canvas",
                "· No GUI open? Falls back to headless single-doc mode, same behavior",
                "preferz --headless starts a JSONL session (stdin/stdout, one JSON per line)",
                "preferz --exec '<json>' runs one command and exits",
                "18 tools map 1:1 to commands: open / save / list / add_shape / add_text",
                "  / add_mermaid / add_chart / add_image / arrange / move / undo / redo",
                "  / screenshot (returns a canvas shot for LLM visual check)",
                "  … (see preferz --help for details)",
                "MCP switch sits in the sidebar (above Help): green = controlled, gray = standby, hollow = off",
                "Only one window is controlled; click the switch on another window to take over",
            ],
        },
    ],
    footer: "Feel free to scribble and try things — closing restores the original.",
};

/// 按一份文案排版出整页。
fn build_scene(copy: &LangCopy) -> Scene {
    let mut scene = Scene::new();

    // —— 标题区 ——
    add_text(
        &mut scene,
        copy.title,
        48.0,
        32.0,
        44.0,
        INK,
        FontFamily::Handwriting,
    );

    // —— 警示横幅 ——
    add_rect(
        &mut scene,
        48.0,
        128.0,
        864.0,
        100.0,
        WARN_STROKE,
        2.0,
        WARN_FILL,
        0.04,
        Sloppiness::Architect,
    );
    add_text(
        &mut scene,
        copy.banner,
        68.0,
        146.0,
        20.0,
        WARN_TEXT,
        FontFamily::Normal,
    );
    add_text(
        &mut scene,
        copy.banner_sub,
        68.0,
        182.0,
        17.0,
        INK,
        FontFamily::Normal,
    );
    add_text(
        &mut scene,
        copy.platform_note,
        48.0,
        236.0,
        17.0,
        FAINT,
        FontFamily::Normal,
    );

    // —— 功能卡片（2 列 × 3 行 + 1 张全宽 CLI·MCP 卡）——
    for (slot, card) in SLOTS.iter().zip(copy.cards.iter()) {
        add_rect(
            &mut scene,
            slot.x,
            slot.y,
            slot.w,
            slot.h,
            CARD_STROKE,
            2.0,
            CARD_FILL,
            0.06,
            Sloppiness::Architect,
        );
        add_text(
            &mut scene,
            card.title,
            slot.x + 20.0,
            slot.y + 14.0,
            21.0,
            ACCENT,
            FontFamily::Normal,
        );
        for (i, line) in card.lines.iter().enumerate() {
            add_text(
                &mut scene,
                line,
                slot.x + 20.0,
                slot.y + 52.0 + (i as f32) * 30.0,
                17.0,
                INK,
                FontFamily::Normal,
            );
        }
    }

    // —— 页脚 ——
    add_text(
        &mut scene,
        copy.footer,
        48.0,
        1496.0,
        17.0,
        FAINT,
        FontFamily::Normal,
    );

    scene
}

/// 写盘 + 自检（重新打开应能读回全部 item）。
fn write_help(path: &Path, scene: &Scene) {
    // 容错：目标文件存在但不是合法 SQLite（如占位/损坏）时先删掉再重建。
    let _ = std::fs::remove_file(path);
    let mut prz =
        PrzFile::create(path).unwrap_or_else(|e| panic!("创建 {} 失败: {e}", path.display()));
    prz.save_scene(scene, &HashMap::new(), ViewportMeta::default())
        .unwrap_or_else(|e| panic!("写入 {} 失败: {e}", path.display()));

    let reopened =
        PrzFile::open(path).unwrap_or_else(|e| panic!("回读 {} 失败: {e}", path.display()));
    let loaded = reopened
        .load_scene()
        .unwrap_or_else(|e| panic!("解析 {} 失败: {e}", path.display()));
    assert_eq!(
        loaded.0.items.len(),
        scene.items.len(),
        "{} 回读 item 数与写入数不一致",
        path.display()
    );
    println!("已生成 {}（{} 个 item）", path.display(), scene.items.len());
}

fn main() {
    let assets: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    for copy in [&ZH, &EN] {
        let scene = build_scene(copy);
        write_help(&assets.join(copy.file), &scene);
    }
}
