use std::io::Write;
use std::path::Path;

/// 思源黑体源文件。build.rs 运行时 CWD 是包根目录 `crates/preferz`，
/// 因此是 `../../assets`（不是 src/main.rs 里的 `../../../assets`）。
const FONT_SRC: &str = "../../assets/SourceHanSansCN-Regular.ttf";

/// 压缩产物在 OUT_DIR 中的文件名；main.rs 用 include_bytes! 嵌入。
const FONT_ZLIB: &str = "SourceHanSansCN-Regular.ttf.zlib";

/// 851远星夜行手写体源文件（基于 851手写杂书体改作，作者 Lakejason0，
/// 原作者 8:51:22 pm，免费商用许可——详见 assets/FONT_LICENSES.md）。
const HANDWRITING_FONT_SRC: &str = "../../assets/851LakeusNightWriting-Regular.ttf";

const HANDWRITING_FONT_ZLIB: &str = "851LakeusNightWriting-Regular.ttf.zlib";

fn main() {
    // 仅 Windows 目标嵌入 exe 资源图标（Linux/macOS 无需此步骤）。
    // 注意必须用 #[cfg] 条件编译而非运行时 if —— build.rs 编译期若不启用
    // embed-resource 依赖，非 Windows host 上引用该 crate 会直接报 E0433。
    #[cfg(windows)]
    {
        // embed-resource 自动通过 vswhere/Windows SDK 查找 rc.exe，
        // 生成的 .res 通过 cargo:rustc-link-arg-bin 直接传给链接器，
        // 比 winres 的 +nostartfiles lib 机制更可靠（修 winres 在 MSVC 上 .rsrc 不生效的问题）
        // v3 起 compile() 返回 #[must_use] 的 CompilationResult（描述本次动作）；嵌入本身照常
        // 执行，此处不关心返回值，用 let _ 显式忽略以满足 unused_must_use。
        let _ = embed_resource::compile("icon.rc", embed_resource::NONE);
    }

    compress_font(FONT_SRC, FONT_ZLIB, "FONT_RAW_SIZE");
    compress_font(
        HANDWRITING_FONT_SRC,
        HANDWRITING_FONT_ZLIB,
        "HANDWRITING_FONT_RAW_SIZE",
    );
}

/// 把字体 deflate 压缩后写进 OUT_DIR，并把原始字节数经 `cargo:rustc-env`
/// 暴露给 main.rs（解压时据此预分配缓冲，省掉反复扩容）。
///
/// 思源黑体 9.92MB 压缩后约 6.7MB；851远星夜行 28MB 压缩后约 18-22MB。
/// 这是**不损失字形覆盖面**（子集化会让生僻字变豆腐块）前提下唯一
/// 零风险的瘦身手段。详见 .issues/issues.md #2。
fn compress_font(src: &str, zlib_name: &str, env_var: &str) {
    println!("cargo:rerun-if-changed={src}");

    let raw = std::fs::read(src).unwrap_or_else(|e| panic!("读取字体 {src} 失败：{e}"));

    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    encoder
        .write_all(&raw)
        .unwrap_or_else(|e| panic!("deflate 压缩字体失败：{e}"));
    let compressed = encoder
        .finish()
        .unwrap_or_else(|e| panic!("deflate 收尾失败：{e}"));

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR 未设置");
    std::fs::write(Path::new(&out_dir).join(zlib_name), &compressed)
        .unwrap_or_else(|e| panic!("写入 {zlib_name} 到 OUT_DIR 失败：{e}"));

    // 解压后应有的字节数，main.rs 用它做 with_capacity + 一致性断言
    println!("cargo:rustc-env={env_var}={}", raw.len());
}
