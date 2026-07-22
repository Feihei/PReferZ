fn main() {
    // 仅 Windows 目标嵌入 exe 资源图标（Linux/macOS 无需此步骤）
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // embed-resource 自动通过 vswhere/Windows SDK 查找 rc.exe，
        // 生成的 .res 通过 cargo:rustc-link-arg-bin 直接传给链接器，
        // 比 winres 的 +nostartfiles lib 机制更可靠（修 winres 在 MSVC 上 .rsrc 不生效的问题）
        embed_resource::compile("icon.rc", embed_resource::NONE);
    }
}
