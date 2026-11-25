//! 画布压力测试（手动运行，默认 `#[ignore]`，不进 CI）。
//!
//! 目的：在「要不要换渲染框架」这类决策之前，先量出每帧 CPU 成本随元素数的
//! 增长曲线，而不是凭手感判断。分两段计时：
//!
//! 1. `Context::run_ui` —— 重建 `Shape`、布局、交互处理（egui 立即模式的固有成本，
//!    含 rough 抖动重算、z 序排序、视口剔除）
//! 2. `Context::tessellate` —— epaint 三角化（单线程）
//!
//! GPU 绘制**不在**测量范围内：这里没有渲染器，只测 CPU。顶点/图元数一并打印，
//! 用来判断瓶颈是「CPU 算不过来」还是「数据量上传太重」。
//!
//! 运行：
//! ```text
//! cargo test -p preferz --release perf_stress -- --ignored --nocapture
//! ```

use super::*;
use eframe::App as _;
use std::time::Instant;

/// 屏幕尺寸取 1600×900（典型桌面窗口）。
const SCREEN_W: f32 = 1600.0;
const SCREEN_H: f32 = 900.0;
/// 预热帧不计入（字体图集、首帧布局、viewport 初始化都很贵）。
const WARMUP: usize = 3;
/// 每档取平均的帧数。
const FRAMES: usize = 10;

struct Row {
    ui_ms: f64,
    tess_ms: f64,
    prims: usize,
    verts: usize,
}

/// 生成 `n` 个形状，按近似正方形网格平铺，互不重叠。
///
/// 三种形状轮换（矩形/椭圆/菱形）+ 一半带填充，贴近真实图稿的图形构成；
/// `sloppiness` 决定走 `CleanStyler` 还是 `RoughStyler`。
fn stress_scene(n: usize, sloppiness: Sloppiness) -> Scene {
    const W: f32 = 120.0;
    const H: f32 = 80.0;
    const GAP: f32 = 24.0;

    let cols = (n as f64).sqrt().ceil().max(1.0) as usize;
    let mut scene = Scene::new();
    for i in 0..n {
        let (col, row) = (i % cols, i / cols);
        let shape_type = match i % 3 {
            0 => ShapeType::Ellipse,
            1 => ShapeType::Diamond,
            _ => ShapeType::Rectangle,
        };
        let fill = (i % 2 == 0).then_some([120, 170, 240, 40]);
        let mut item = Item::new_shape(
            shape_type,
            (W, H),
            col as f32 * (W + GAP),
            row as f32 * (H + GAP),
            StrokeStyle::default(),
            fill,
        );
        if let ItemKind::Shape {
            sloppiness: s,
            seed,
            roundness,
            ..
        } = &mut item.kind
        {
            *s = sloppiness;
            *seed = i as u64 + 1;
            *roundness = 12.0;
        }
        item.z = i as i32;
        scene.add_item(item);
    }
    scene
}

fn make_app(scene: Scene) -> (PReferZApp, egui::Context) {
    let mut app = PReferZApp::new();
    app.session.scene = scene;
    let ctx = egui::Context::default();
    // 与 main.rs 启动路径一致：缺了字体家族，文字项会 panic；布局宽度也依赖真字体
    ctx.set_fonts(crate::app_font_definitions());
    (app, ctx)
}

fn run_frame(app: &mut PReferZApp, ctx: &egui::Context, time: f64) -> Row {
    let raw = egui::RawInput {
        time: Some(time),
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(SCREEN_W, SCREEN_H),
        )),
        ..egui::RawInput::default()
    };

    let t0 = Instant::now();
    let mut out = ctx.run_ui(raw, |ui| {
        let mut frame = eframe::Frame::_new_kittest();
        app.ui(ui, &mut frame);
    });
    let ui_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let t1 = Instant::now();
    let prims = ctx.tessellate(std::mem::take(&mut out.shapes), out.pixels_per_point);
    let tess_ms = t1.elapsed().as_secs_f64() * 1000.0;
    // 无渲染器：字体图集等纹理增量无法上 GPU，显式清掉避免 Drop panic
    out.textures_delta.clear();

    let mut verts = 0;
    for prim in &prims {
        if let egui::epaint::Primitive::Mesh(mesh) = &prim.primitive {
            verts += mesh.vertices.len();
        }
    }

    Row {
        ui_ms,
        tess_ms,
        prims: prims.len(),
        verts,
    }
}

fn measure(app: &mut PReferZApp, ctx: &egui::Context) -> Row {
    let mut time = 0.0;
    for _ in 0..WARMUP {
        run_frame(app, ctx, time);
        time += 1.0 / 60.0;
    }
    // 预热后再 fit：此时 viewport 的 screen_rect 已由 CentralPanel 填入真值。
    // 全部元素都落在视口内 = 最坏情况（视口剔除不帮忙）。
    if let Some(rect) = app.session.scene.content_bounding_rect() {
        app.session.viewport.fit_to_content(rect);
    }

    let mut acc = Row {
        ui_ms: 0.0,
        tess_ms: 0.0,
        prims: 0,
        verts: 0,
    };
    for _ in 0..FRAMES {
        let row = run_frame(app, ctx, time);
        time += 1.0 / 60.0;
        acc.ui_ms += row.ui_ms;
        acc.tess_ms += row.tess_ms;
        acc.prims += row.prims;
        acc.verts += row.verts;
    }
    let n = FRAMES as f64;
    Row {
        ui_ms: acc.ui_ms / n,
        tess_ms: acc.tess_ms / n,
        prims: (acc.prims as f64 / n).round() as usize,
        verts: (acc.verts as f64 / n).round() as usize,
    }
}

#[test]
#[ignore = "手动性能压测：cargo test -p preferz --release perf_stress -- --ignored --nocapture"]
fn perf_stress_grid() {
    println!(
        "\n{:<7} {:<6} {:>9} {:>9} {:>9} {:>7} {:>7} {:>9} {:>8}",
        "items", "style", "run_ui", "tess", "total", "fps", "prims", "verts", "µs/item"
    );
    println!("{}", "-".repeat(78));

    for &n in &[100usize, 500, 1000, 2000, 5000, 10000] {
        for (label, sloppiness) in [("clean", Sloppiness::Off), ("rough", Sloppiness::Artist)] {
            let (mut app, ctx) = make_app(stress_scene(n, sloppiness));
            let row = measure(&mut app, &ctx);
            let total = row.ui_ms + row.tess_ms;
            let per_item = total * 1000.0 / n as f64;
            println!(
                "{:<7} {:<6} {:>8.2}ms {:>8.2}ms {:>8.2}ms {:>6.0} {:>7} {:>9} {:>8.1}",
                n,
                label,
                row.ui_ms,
                row.tess_ms,
                total,
                if total > 0.0 { 1000.0 / total } else { 0.0 },
                row.prims,
                row.verts,
                per_item
            );
        }
    }
    println!();
}

/// 把压测场景落成 `.prz`，方便在真机上用鼠标感受（拖拽/缩放/选中）。
///
/// 输出到 `<workspace>/target/perf/`。
#[test]
#[ignore = "手动性能压测：cargo test -p preferz --release perf_stress_write_files -- --ignored --nocapture"]
fn perf_stress_write_files() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/perf");
    std::fs::create_dir_all(&dir).expect("创建 target/perf 失败");

    for n in [1000usize, 5000, 10000] {
        for (label, sloppiness) in [("clean", Sloppiness::Off), ("rough", Sloppiness::Artist)] {
            let scene = stress_scene(n, sloppiness);
            let path = dir.join(format!("stress-{n}-{label}.prz"));
            let mut file = PrzFile::create(&path).expect("创建 .prz 失败");
            file.save_scene(&scene, &HashMap::new(), ViewportMeta::default())
                .expect("写场景失败");
            file.close().expect("关闭 .prz 失败");
            println!("wrote {}", path.display());
        }
    }
}
