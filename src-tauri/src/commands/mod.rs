//! 薄 IPC 适配层；核心类型保持独立，后续耗时命令交给任务服务。

mod mutations;
mod native_drop;
pub(crate) use native_drop::handle_native_drop;
#[cfg(test)]
mod drop_tests;
#[cfg(test)]
mod output_tests;
#[cfg(test)]
mod tests;

use crate::ipc::{
    MutationError, NativeDropRelease, NativeImportGrant, NativeSelectionKind,
    NativeSelectionRequest, TaskMutationAccepted, TaskMutationRequest, TaskStreamMessage,
};
use crate::{
    ipc::{
        self, QueryError, SubscriptionError, TaskChangeAck, TaskChangeNotice, TaskPageRequest,
        TaskSnapshotDto, TaskSubscriptionRequest,
    },
    lifecycle::DesktopTasks,
};
use pixofold_core::model::AppInfo;
use tauri_plugin_dialog::{DialogExt, FilePath};

// 生产装配与mock使用相同路由，避免测试独立注册命令后掩盖接线遗漏。
pub(crate) fn register<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            get_log_status,
            open_log_directory,
            get_task_snapshot,
            subscribe_task_changes,
            acknowledge_task_changes,
            unsubscribe_task_changes,
            select_native_import,
            select_output_directory,
            release_output_directory,
            release_native_drop,
            apply_task_mutation
        ])
}

/// 原生对话框只能返回Rust内授权标识；前端不能传入或取回路径。
/// SDK的取消/关闭返回None，系统对话框错误也可能表现为None；不据此宣称I/O成功。
#[tauri::command]
pub(crate) async fn select_native_import<R: tauri::Runtime>(
    window: tauri::WebviewWindow<R>,
    tasks: tauri::State<'_, DesktopTasks>,
    request: NativeSelectionRequest,
) -> Result<Option<NativeImportGrant>, MutationError> {
    let permit = tasks
        .subscriptions
        .with_ready(request.subscription_id, || {
            mutations::ensure_can_import(&tasks.control)?;
            tasks.imports.reserve(request.subscription_id)
        })
        .map_err(|error| MutationError::Subscription { error })??;
    let subscriptions = tasks.subscriptions.clone();
    // 单槽先占位才spawn；等待原生UI不占事件循环/异步executor线程，不持任何服务锁。
    tauri::async_runtime::spawn_blocking(move || {
        let dialog = window.dialog().file().set_parent(&window);
        let selected = match request.kind {
            NativeSelectionKind::Files => dialog.add_filter("PNG", &["png"]).blocking_pick_files(),
            NativeSelectionKind::Folder => dialog.blocking_pick_folder().map(|path| vec![path]),
        };
        let paths = selected
            .map(|paths| {
                paths
                    .into_iter()
                    .map(|path| match path {
                        FilePath::Path(path) => Ok(path),
                        FilePath::Url(_) => Err(MutationError::InvalidSelection),
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        // 对话框期间重载/断开/退出后不得把旧结果授权给新页面。
        subscriptions
            .with_ready(request.subscription_id, || permit.complete(paths))
            .map_err(|error| MutationError::Subscription { error })?
    })
    .await
    .map_err(|_| MutationError::NativeDialogFailed)?
}

/// 输出选择仅改变草稿，Ready可修正、运行中选择也不会改变已有任务。
#[tauri::command]
pub(crate) async fn select_output_directory<R: tauri::Runtime>(
    window: tauri::WebviewWindow<R>,
    tasks: tauri::State<'_, DesktopTasks>,
    request: TaskSubscriptionRequest,
) -> Result<Option<ipc::NativeOutputDirectory>, MutationError> {
    let permit = tasks
        .subscriptions
        .with_ready(request.subscription_id, || {
            if matches!(
                tasks.control.snapshot().phase,
                crate::tasks::TaskPhase::Closing | crate::tasks::TaskPhase::Closed
            ) {
                return Err(MutationError::Closed);
            }
            tasks.imports.reserve(request.subscription_id)
        })
        .map_err(|error| MutationError::Subscription { error })??;
    let subscriptions = tasks.subscriptions.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let selected = window
            .dialog()
            .file()
            .set_parent(&window)
            .blocking_pick_folder();
        // 目录规范化/身份句柄在原生选择线程创建，不持订阅或授权锁。
        let directory = selected
            .map(|path| match path {
                FilePath::Path(path) if path.is_absolute() => {
                    pixofold_core::model::OutputDirectory::open(&path)
                        .map_err(|_| MutationError::InvalidOutputDirectory)
                }
                _ => Err(MutationError::InvalidOutputDirectory),
            })
            .transpose()?;
        subscriptions
            .with_ready(request.subscription_id, || {
                permit.complete_output(directory)
            })
            .map_err(|error| MutationError::Subscription { error })?
    })
    .await
    .map_err(|_| MutationError::NativeDialogFailed)?
}

#[tauri::command]
pub(crate) fn release_output_directory(
    tasks: tauri::State<'_, DesktopTasks>,
    request: ipc::ReleaseOutputDirectory,
) -> Result<(), MutationError> {
    tasks
        .subscriptions
        .with_ready(request.subscription_id, || {
            tasks
                .imports
                .release_output(request.subscription_id, request.directory_id)
        })
        .map_err(|error| MutationError::Subscription { error })?
}

/// 接纳结果不等于处理完成；最终结果继续经有界快照/通知恢复。
#[tauri::command]
pub(crate) fn apply_task_mutation(
    tasks: tauri::State<'_, DesktopTasks>,
    request: TaskMutationRequest,
) -> Result<TaskMutationAccepted, MutationError> {
    tasks
        .subscriptions
        .with_ready(request.subscription_id, || {
            mutations::mutate(&tasks.control, &tasks.imports, request)
        })
        .map_err(|error| MutationError::Subscription { error })?
}

/// 幂等释放已收到的拖放票据；不接受路径，不启动/取消任务，也不影响后续原生选择。
#[tauri::command]
pub(crate) fn release_native_drop(
    tasks: tauri::State<'_, DesktopTasks>,
    request: NativeDropRelease,
) -> Result<(), MutationError> {
    tasks
        .subscriptions
        .with_ready(request.subscription_id, || {
            tasks
                .imports
                .release_drop(request.subscription_id, request.offer_id)
        })
        .map_err(|error| MutationError::Subscription { error })?
}

#[tauri::command]
pub(crate) fn get_app_info() -> AppInfo {
    pixofold_core::app_info()
}

/// 无路径只读诊断状态；日志错误不会影响任务服务。
#[tauri::command]
pub(crate) fn get_log_status() -> crate::diagnostics::LogStatus {
    crate::diagnostics::status()
}

/// 只打开Rust掌握的日志目录，无路径/命令参数；不授权通用shell或文件访问。
#[tauri::command]
pub(crate) async fn open_log_directory() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(crate::diagnostics::open_directory)
        .await
        .map_err(|_| "open_logs_failed".to_owned())?
        .map_err(str::to_owned)
}

/// 只读、最多100行；不读文件或启动工作，序列化不持有任务锁。
#[tauri::command]
pub(crate) async fn get_task_snapshot(
    tasks: tauri::State<'_, DesktopTasks>,
    request: TaskPageRequest,
) -> Result<TaskSnapshotDto, QueryError> {
    ipc::query(&tasks.control, request)
}

/// 替换只读订阅，先返回票据；前端查询快照并ACK后才开始投递变化。
#[tauri::command]
pub(crate) fn subscribe_task_changes(
    on_change: tauri::ipc::Channel<TaskStreamMessage>,
    tasks: tauri::State<'_, DesktopTasks>,
) -> Result<TaskChangeNotice, SubscriptionError> {
    tasks.subscriptions.subscribe(on_change)
}

#[tauri::command]
pub(crate) fn acknowledge_task_changes(
    tasks: tauri::State<'_, DesktopTasks>,
    request: TaskChangeAck,
) -> Result<(), SubscriptionError> {
    tasks.subscriptions.acknowledge(request)
}

#[tauri::command]
pub(crate) fn unsubscribe_task_changes(
    tasks: tauri::State<'_, DesktopTasks>,
    request: TaskSubscriptionRequest,
) -> Result<bool, SubscriptionError> {
    let removed = tasks.subscriptions.unsubscribe(request.subscription_id)?;
    tasks.imports.revoke_output_session(request.subscription_id);
    Ok(removed)
}
