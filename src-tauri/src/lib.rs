//! 桌面装配层：注册窗口权限与薄 IPC 命令，不执行图片处理。

pub(crate) mod assets;
mod commands;
pub(crate) mod diagnostics;
pub(crate) mod ingress;
pub(crate) mod ipc;
mod jpeg_bundle;
pub(crate) mod lifecycle;
pub(crate) mod resources;
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

// 生产与mock复用命令和事件装配，回归通过同一个窗口分发函数进入拖放链路。
fn configure_app<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    commands::register(builder)
        .on_page_load(|webview, payload| {
            webview
                .state::<lifecycle::DesktopTasks>()
                .page_load(webview.label(), payload.event());
        })
        .on_window_event(handle_window_event)
}

fn handle_window_event<R: tauri::Runtime>(window: &tauri::Window<R>, event: &tauri::WindowEvent) {
    if window.label() != "main" {
        return;
    }
    match event {
        // 当前main是单WebviewWindow。锁定runtime-wry把WindowContent拖放转换为
        // WindowEvent，只有子WebView才走WebviewEvent；不能只监听后者或双路转发。
        tauri::WindowEvent::DragDrop(event) => commands::handle_native_drop(
            &window.state::<lifecycle::DesktopTasks>(),
            window.label(),
            event,
        ),
        tauri::WindowEvent::CloseRequested { api, .. }
            if !window.state::<lifecycle::DesktopTasks>().ready() =>
        {
            api.prevent_close();
            lifecycle::request_exit(window.app_handle(), 0);
        }
        _ => {}
    }
}

/// 启动桌面事件循环。
///
/// # Errors
/// 任务线程、窗口、WebView或Tauri运行时初始化失败时返回原始错误链。
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    diagnostics::initialize();
    struct LogGuard;
    impl Drop for LogGuard {
        fn drop(&mut self) {
            diagnostics::shutdown();
        }
    }
    let _logs = LogGuard;
    let result = run_application();
    if result.is_err() {
        tracing::error!(target: "pixofold", event = "application_start_failed");
    }
    result
}

fn run_application() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tasks::TaskRuntime::new(resources::task_config())?;
    let builder = tauri::Builder::default()
        .manage(lifecycle::DesktopTasks::new(runtime)?)
        .setup(|app| {
            let loaded = app.path().resource_dir()
                .map_err(|_| "resource_directory")
                .and_then(|root| jpeg_bundle::load(&root).map_err(|error| match error {
                    pixofold_core::jpeg::JpegError::ToolIo { .. } => "tool_io",
                    _ => "tool_identity",
                }));
            let engines = match loaded {
                Ok(engine) => {
                    tracing::info!(target: "pixofold", event = "jpeg_engine_verified");
                    pixofold_core::batch::ImageEngines::with_jpeg(engine)
                }
                Err(code) => {
                    tracing::warn!(target: "pixofold", event = "jpeg_engine_unavailable", error_code = code);
                    pixofold_core::batch::ImageEngines::default()
                }
            };
            // 应用持有可信能力；v9工作流仍PNG-only，避免不完整DTO接受JPEG任务。
            app.manage(engines);
            Ok(())
        });
    let app = configure_app(builder).build(app_context())?;
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
