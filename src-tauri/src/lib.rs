//! 桌面装配层：注册窗口权限与薄 IPC 命令，不执行图片处理。

mod commands;
pub(crate) mod ipc;
pub(crate) mod lifecycle;
pub(crate) mod subscriptions;
pub mod tasks;

use tauri::Manager;

// 生产与mock共用一次宏展开；macOS开发构建会生成唯一的嵌入plist符号。
fn app_context<R: tauri::Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}

/// 只在bindings构建中为生成工具提供权威任务DTO声明；不启动桌面或任务。
#[cfg(feature = "bindings")]
pub fn task_type_declarations() -> String {
    ipc::declarations()
}

/// 启动桌面事件循环。
///
/// # Errors
/// 任务线程、窗口、WebView或Tauri运行时初始化失败时返回原始错误链。
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tasks::TaskRuntime::new(tasks::TaskConfig::default())?;
    let builder = tauri::Builder::default().manage(lifecycle::DesktopTasks::new(runtime)?);
    let app = commands::register(builder)
        .on_window_event(|window, event| {
            if window.label() == "main"
                && let tauri::WindowEvent::CloseRequested { api, .. } = event
                && !window.state::<lifecycle::DesktopTasks>().ready()
            {
                api.prevent_close();
                lifecycle::request_exit(window.app_handle(), 0);
            }
        })
        .build(app_context())?;
    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { api, code, .. } = event
            && !app.state::<lifecycle::DesktopTasks>().ready()
        {
            api.prevent_exit();
            lifecycle::request_exit(app, code.unwrap_or(0));
        }
    });
    Ok(())
}
