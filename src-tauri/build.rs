fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["get_app_info", "get_task_snapshot"]),
    ))
    .expect("无法构建 PixoFold 桌面资源与权限清单");

    // Tauri默认仅给bin链接resource.lib。mock测试也引用TaskDialogIndirect，
    // 需要相同的Common Controls v6 manifest，否则Windows在进入测试前就拒绝加载。
    // 显式desktop测试target复用已生成资源；不改变发行程序或放宽测试权限。
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo缺少OUT_DIR"));
        println!(
            "cargo:rustc-link-arg-tests={}",
            out.join("resource.lib").display()
        );
    }
}
