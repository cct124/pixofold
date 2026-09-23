//! 薄 IPC 适配层；核心类型保持独立，后续耗时命令交给任务服务。

#[cfg(test)]
mod tests;

use crate::{
    ipc::{self, QueryError, TaskPageRequest, TaskSnapshotDto},
    lifecycle::DesktopTasks,
};
use pixofold_core::model::AppInfo;

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
