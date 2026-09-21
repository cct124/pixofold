#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = pixofold_desktop_lib::run() {
        eprintln!("PixoFold 启动失败：{error}");
        std::process::exit(1);
    }
}
