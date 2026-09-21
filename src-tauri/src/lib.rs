//! 桌面装配层：注册窗口权限与薄 IPC 命令，不执行图片处理。

mod commands;

/// 启动桌面事件循环。
///
/// # Errors
/// 窗口、WebView 或 Tauri 运行时初始化失败时返回原始错误。
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::get_app_info])
        .run(tauri::generate_context!())
}
