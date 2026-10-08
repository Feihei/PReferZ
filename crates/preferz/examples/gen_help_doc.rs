//! 生成内置帮助文档 `assets/help.prz`（plan #25）。
//!
//! 运行：`cargo run -p preferz --example gen_help_doc`
//!
//! 帮助页是一张「纸」：浅色底版矩形保证任意主题下文字可读（画布文本为固定
//! 颜色，不随明暗主题翻转），上面排版警示横幅 + 六张功能卡片。画布文本为
//! 单行 Text item，逐行摆放。正式维护可直接在 PReferZ 里绘制后「另存为」
//! 覆盖本文件，重编译即嵌入（build.rs 对该文件声明了 rerun-if-changed）。

use preferz_core::item::{Item, ItemKind};
use preferz_core::scene::Scene;
use preferz_core::shape::{DashStyle, FontFamily, ShapeType, Sloppiness, StrokeStyle};
use preferz_fileio::{PrzFile, ViewportMeta};

use std::collections::HashMap;
use std::path::Path;

/// 主题无关的固定配色（以浅色纸张为前提设计）。
const INK: [u8; 4] = [50, 48, 44, 255];
const FAINT: [u8; 4] = [128, 124, 116, 255];
const ACCENT: [u8; 4] = [40, 92, 170, 255];
const WARN_TEXT: [u8; 4] = [172, 42, 34, 255];
const WARN_STROKE: [u8; 4] = [206, 74, 62, 255];
const WARN_FILL: [u8; 4] = [252, 236, 232, 255];
const PAPER_FILL: [u8; 4] = [250, 248, 242, 255];
const PAPER_STROKE: [u8; 4] = [219, 213, 199, 255];
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

/// 一张功能卡片：圆角矩形 + 标题 + 若干条目行。
struct Card {
    x: f32,
    y: f32,
    title: &'static str,
    lines: &'static [&'static str],
}

impl Card {
    const W: f32 = 408.0;
    const H: f32 = 260.0;

    fn render(&self, scene: &mut Scene) {
        add_rect(
            scene,
            self.x,
            self.y,
            Self::W,
            Self::H,
            CARD_STROKE,
            2.0,
            CARD_FILL,
            0.06,
            Sloppiness::Architect,
        );
        add_text(
            scene,
            self.title,
            self.x + 20.0,
            self.y + 14.0,
            21.0,
            ACCENT,
            FontFamily::Normal,
        );
        for (i, line) in self.lines.iter().enumerate() {
            add_text(
                scene,
                line,
                self.x + 20.0,
                self.y + 52.0 + (i as f32) * 30.0,
                17.0,
                INK,
                FontFamily::Normal,
            );
        }
    }
}

fn main() {
    let mut scene = Scene::new();

    // —— 纸张底版（最先加入 = z 最底；精确几何，不抖动）——
    add_rect(
        &mut scene,
        0.0,
        0.0,
        960.0,
        1200.0,
        PAPER_STROKE,
        1.0,
        PAPER_FILL,
        0.0,
        Sloppiness::Off,
    );

    // —— 标题区 ——
    add_text(
        &mut scene,
        "PReferZ 帮助",
        48.0,
        32.0,
        44.0,
        INK,
        FontFamily::Handwriting,
    );
    add_text(
        &mut scene,
        "参考图板 · 把参考图收进一块画布",
        52.0,
        92.0,
        16.0,
        FAINT,
        FontFamily::Normal,
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
        "※ 本窗口是内置帮助文档：对它的修改不会被保存，关闭后重开会还原原始内容。",
        68.0,
        146.0,
        20.0,
        WARN_TEXT,
        FontFamily::Normal,
    );
    add_text(
        &mut scene,
        "想保留修改：菜单 文件 → 另存为（Ctrl+Shift+S），即可存成普通 .prz 项目。",
        68.0,
        182.0,
        17.0,
        INK,
        FontFamily::Normal,
    );

    // —— 功能卡片（2 列 × 3 行）——
    let cards = [
        Card {
            x: 48.0,
            y: 264.0,
            title: "视图",
            lines: &[
                "滚轮 缩放 · 中键拖拽 平移",
                "Shift+1 适应画布内容",
                "Shift+2 缩放到选中 · Shift+3 缩放 100%",
                "F5 演示模式 · 方向键翻页",
                "Home / End 演示跳到首尾页",
            ],
        },
        Card {
            x: 504.0,
            y: 264.0,
            title: "工具",
            lines: &[
                "V/1 选择 · R/2 矩形 · D/3 菱形",
                "O/4 椭圆 · A/5 直角箭头 · L/6 直线",
                "Shift+P 多边形 · P 徒手画",
                "8 文字 · F 画框",
                "C 裁剪 · I 取色器",
            ],
        },
        Card {
            x: 48.0,
            y: 548.0,
            title: "编辑",
            lines: &[
                "Ctrl+A 全选 · Ctrl+Shift+A 取消全选",
                "Ctrl+C / X / V 复制 · 剪切 · 粘贴",
                "Ctrl+D 原位复制 · Delete 删除",
                "Enter 编辑文字 · Esc 取消",
                "Ctrl+G 编组 · Ctrl+Shift+G 解组",
                "Ctrl+K 编辑超链接（网页 / .prz）",
            ],
        },
        Card {
            x: 504.0,
            y: 548.0,
            title: "文件",
            lines: &[
                "Ctrl+N 新建 · Ctrl+O 打开",
                "Ctrl+I 导入图片 · Ctrl+S 保存",
                "Ctrl+Shift+S 另存为",
                "Ctrl+Shift+E 导出选区",
                "拖放图片文件到画布即可导入",
            ],
        },
        Card {
            x: 48.0,
            y: 832.0,
            title: "撤销 · 层级",
            lines: &[
                "Ctrl+Z 撤销 · Ctrl+Shift+Z / Ctrl+Y 重做",
                "Ctrl+] / Ctrl+[ 上移 / 下移一层",
                "Ctrl+Shift+] / Ctrl+Shift+[ 置顶 / 置底",
                "拖拽、缩放、改色都可撤销，放心试验",
            ],
        },
        Card {
            x: 504.0,
            y: 832.0,
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
    ];
    for card in &cards {
        card.render(&mut scene);
    }

    // —— 页脚 ——
    add_text(
        &mut scene,
        "本窗口的内容可以随意涂改、试用——关闭重开会还原。",
        48.0,
        1116.0,
        17.0,
        FAINT,
        FontFamily::Normal,
    );
    add_text(
        &mut scene,
        "左侧工具栏最下方的 ? 按钮可随时再开一个帮助窗口；帮助实例里改的设置只在内存中，下次启动以最后一次保存的配置为准。",
        48.0,
        1148.0,
        15.0,
        FAINT,
        FontFamily::Normal,
    );

    // —— 写盘 + 自检（重新打开应能读回全部 item）——
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/help.prz");
    // 容错：目标文件存在但不是合法 SQLite（如占位/损坏）时先删掉再重建。
    let _ = std::fs::remove_file(&out);
    let mut prz = PrzFile::create(&out).expect("创建 help.prz 失败");
    prz.save_scene(&scene, &HashMap::new(), ViewportMeta::default())
        .expect("写入 help.prz 失败");

    let reopened = PrzFile::open(&out).expect("回读 help.prz 失败");
    let loaded = reopened.load_scene().expect("解析 help.prz 失败");
    assert_eq!(
        loaded.0.items.len(),
        scene.items.len(),
        "回读 item 数与写入数不一致"
    );
    println!("已生成 {}（{} 个 item）", out.display(), scene.items.len());
}
