//! 薄 IPC 适配层；核心类型保持独立，后续耗时命令交给任务服务。

use pixofold_core::model::AppInfo;

#[tauri::command]
pub(crate) fn get_app_info() -> AppInfo {
    pixofold_core::app_info()
}
