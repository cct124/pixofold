//! 变更入口只接收领域参数与会话/授权标识；不允许路径字符串或资源预算覆盖。

use super::{DecimalU64, SubscriptionError, dto::TaskFailureDto};
use pixofold_core::model::PngMode;
use serde::{Deserialize, Serialize};

pub(crate) const MAX_NATIVE_IMPORT_ROOTS: usize = pixofold_core::import::ScanOptions::MAX_ROOTS;
pub(crate) const MAX_RETRY_JOBS: usize = 1000;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum NativeSelectionKind {
    Files,
    Folder,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct NativeSelectionRequest {
    pub subscription_id: DecimalU64,
    pub kind: NativeSelectionKind,
}

/// 单次、会话绑定的原生授权；rootCount不是扫描结果，grantId不是路径或保密凭据。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct NativeImportGrant {
    pub grant_id: DecimalU64,
    pub root_count: u32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum TaskOutput {
    Overwrite,
    CopyBeside,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct TaskSettingsDto {
    pub mode: PngMode,
    pub output: TaskOutput,
}

// 严格校验放在variant载荷结构，ts-rs可表达同一来源，避免忽略enum属性或复制Wire枚举。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct ImportTask {
    pub grant_id: DecimalU64,
    pub settings: Option<TaskSettingsDto>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct StartTask {
    pub selection_id: DecimalU64,
    pub settings: TaskSettingsDto,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct SelectTask {
    pub selection_id: DecimalU64,
}
/// 仅重试指定失败/取消行，输出沿用原行Rust路径；jobIds是稳定ID，不是页面或数组索引。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct RetryTask {
    pub selection_id: DecimalU64,
    pub expected_batch_revision: DecimalU64,
    pub job_ids: Vec<u32>,
    pub mode: PngMode,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum CredentialsConsent {
    RemoveContentCredentials,
}

/// 确认弹窗专用输出策略，不允许普通导入通过此枚举关闭备份。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum CredentialsOutput {
    CopyBeside,
    OverwriteWithBackup,
    OverwriteWithoutBackup,
}

/// 点击确认即明确同意，只对这些失败行/本批次版本有效；不接受任意路径。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct ConfirmContentCredentials {
    pub selection_id: DecimalU64,
    pub expected_batch_revision: DecimalU64,
    pub job_ids: Vec<u32>,
    pub mode: PngMode,
    pub output: CredentialsOutput,
    pub consent: CredentialsConsent,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum TaskMutation {
    Import(ImportTask),
    Start(StartTask),
    Clear(SelectTask),
    Retry(RetryTask),
    ConfirmContentCredentials(ConfirmContentCredentials),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct TaskMutationRequest {
    pub subscription_id: DecimalU64,
    pub operation: TaskMutation,
}

/// 仅证明命令已被接纳，不代表扫描/编码/输出成功；最终结果读取任务快照。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct TaskMutationAccepted {
    pub selection_id: DecimalU64,
}

#[derive(Debug, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum MutationError {
    Subscription { error: SubscriptionError },
    Task { error: TaskFailureDto },
    SelectionBusy,
    StaleGrant,
    InvalidSelection,
    InvalidRetry,
    Closed,
    IdExhausted,
    NativeDialogFailed,
    ServiceFault,
}

#[cfg(feature = "bindings")]
pub(super) fn declarations() -> String {
    use ts_rs::{Config, TS};
    let config = Config::default();
    let mut output = format!(
        "export const MAX_NATIVE_IMPORT_ROOTS = {MAX_NATIVE_IMPORT_ROOTS};\nexport const MAX_RETRY_JOBS = {MAX_RETRY_JOBS};\n"
    );
    macro_rules! export {
        ($($ty:ty),+ $(,)?) => { $(output.push_str(&format!("export {}\n", <$ty>::decl(&config)));)+ };
    }
    export!(
        NativeSelectionKind,
        NativeSelectionRequest,
        NativeImportGrant,
        TaskOutput,
        TaskSettingsDto,
        ImportTask,
        StartTask,
        SelectTask,
        RetryTask,
        CredentialsConsent,
        CredentialsOutput,
        ConfirmContentCredentials,
        TaskMutation,
        TaskMutationRequest,
        TaskMutationAccepted,
        MutationError
    );
    output
}
