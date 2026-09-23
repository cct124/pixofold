//! 薄 IPC 适配层；核心类型保持独立，后续耗时命令交给任务服务。

#[cfg(test)]
mod tests;

use crate::{
    ipc::{
        self, QueryError, SubscriptionError, TaskChangeAck, TaskChangeNotice, TaskPageRequest,
        TaskSnapshotDto, TaskSubscriptionRequest,
    },
    lifecycle::DesktopTasks,
};
use pixofold_core::model::AppInfo;

// 生产装配与mock使用相同路由，避免测试独立注册命令后掩盖接线遗漏。
pub(crate) fn register<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        get_app_info,
        get_task_snapshot,
        subscribe_task_changes,
        acknowledge_task_changes,
        unsubscribe_task_changes
    ])
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
