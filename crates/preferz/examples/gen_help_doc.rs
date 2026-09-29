//! 生成占位帮助文档 `assets/help.prz`（plan #25）。
//!
//! 运行：`cargo run -p preferz --example gen_help_doc`
//!
//! 正式帮助内容建议直接在 PReferZ 里绘制后「另存为」覆盖 `assets/help.prz`，
//! 重编译即嵌入（build.rs 对该文件声明了 rerun-if-changed）。本示例只负责
//! 产出一个包含必要提示文案的起点版本。

use preferz_core::item::Item;
use preferz_core::scene::Scene;
use preferz_fileio::{PrzFile, ViewportMeta};

use std::collections::HashMap;
use std::path::Path;

fn main() {
    // 画布文本为固定颜色（不随主题），默认按浅色画布配深色字。
    let ink = [40, 40, 40, 255];
    let warn = [190, 40, 40, 255];
    let faint = [120, 120, 120, 255];

    // (文案, 字号, 颜色, 行距系数)
    let lines: &[(&str, f32, [u8; 4], f32)] = &[
        ("PReferZ 使用帮助", 36.0, ink, 1.8),
        (
            "注意：本窗口是内置帮助文档，对它的修改不会被保存，",
            22.0,
            warn,
            1.5,
        ),
        ("关闭后再次打开会还原为原始内容。", 22.0, warn, 2.0),
        (
            "想保留修改：文件 - 另存为，即可存成普通 .prz 项目。",
            20.0,
            ink,
            2.0,
        ),
        (
            "本窗口里改动的设置只保留在内存中；下次启动以最后一次保存的配置为准。",
            16.0,
            faint,
            2.4,
        ),
        (
            "滚轮缩放 / 空格加拖拽平移 / 右键菜单查看全部功能。",
            18.0,
            ink,
            1.6,
        ),
        (
            "各工具按钮右下角有快捷键角标，帮助窗口里也可以直接试用。",
            16.0,
            faint,
            1.0,
        ),
    ];

    let mut scene = Scene::new();
    let mut y = 40.0;
    for (text, size, color, line_gap) in lines {
        scene.add_item(Item::new_text((*text).to_string(), 40.0, y, *size, *color));
        y += size * line_gap;
    }

    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/help.prz");
    // 容错：目标文件存在但不是合法 SQLite（如占位/损坏）时先删掉再重建。
    let _ = std::fs::remove_file(&out);
    let mut prz = PrzFile::create(&out).expect("创建 help.prz 失败");
    prz.save_scene(&scene, &HashMap::new(), ViewportMeta::default())
        .expect("写入 help.prz 失败");
    println!("已生成 {}", out.display());
}
