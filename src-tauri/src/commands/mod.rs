//! 薄 IPC 适配层；核心类型保持独立，后续耗时命令交给任务服务。

#[cfg(test)]
mod tests;

use crate::{
    ipc::{self, QueryError, TaskPageRequest, TaskSnapshotDto},
    lifecycle::DesktopTasks,
};
use pixofold_core::model::AppInfo;

// 生产装配与mock使用相同路由，避免测试独立注册命令后掩盖接线遗漏。
pub(crate) fn register<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![get_app_info, get_task_snapshot])
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
