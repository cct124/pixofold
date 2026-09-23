//! 任务IPC：只读分页与受控变更分别适配应用服务，路径和原始错误不跨WebView。
//! 展示名不等于路径授权；变更只接纳短命令，不在此执行文件I/O或创建任务服务。

mod convert;
mod dto;
mod mutation_dto;
#[cfg(test)]
mod tests;

use crate::tasks::TaskControl;
pub(crate) use convert::task_error;
pub(crate) use dto::{
    DecimalU64, SubscriptionError, TASK_PROTOCOL_VERSION, TaskChangeAck, TaskChangeNotice,
    TaskSubscriptionRequest,
};
pub(crate) use dto::{QueryError, TaskPageRequest, TaskSnapshotDto};
pub(crate) use mutation_dto::*;

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
    dto::declarations() + &mutation_dto::declarations()
}
