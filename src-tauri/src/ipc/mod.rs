//! 任务只读IPC：捕获一次权威视图，再在锁外转换有界页面。
//! 展示名不等于路径授权；不读取文件、不创建线程、不改变任务或保存历史快照。

mod convert;
mod dto;
#[cfg(test)]
mod tests;

use crate::tasks::TaskControl;
pub(crate) use dto::{QueryError, TaskPageRequest, TaskSnapshotDto};

pub(crate) fn query(
    control: &TaskControl,
    request: TaskPageRequest,
) -> Result<TaskSnapshotDto, QueryError> {
    request.validate()?;
    // 此处只克隆Arc与小型状态。行转换/字符串分配不持有任务锁。
    convert::snapshot(&control.snapshot(), &request)
}

#[cfg(feature = "bindings")]
pub(crate) fn declarations() -> String {
    dto::declarations()
}
