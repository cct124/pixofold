//! 薄 IPC 适配层；核心类型保持独立，后续耗时命令交给任务服务。

mod mutations;
#[cfg(test)]
mod tests;

use crate::ipc::{
    MutationError, NativeImportGrant, NativeSelectionKind, NativeSelectionRequest,
    TaskMutationAccepted, TaskMutationRequest,
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
            get_task_snapshot,
            subscribe_task_changes,
            acknowledge_task_changes,
            unsubscribe_task_changes,
            select_native_import,
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

#[tauri::command]
pub(crate) fn get_app_info() -> AppInfo {
    pixofold_core::app_info()
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
    on_change: tauri::ipc::Channel<TaskChangeNotice>,
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
    tasks.subscriptions.unsubscribe(request.subscription_id)
}
