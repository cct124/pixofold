//! 图片展示专用契约：仅任务身份跨IPC，原生路径与文件句柄留在Rust。

use crate::ipc::DecimalU64;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum AssetState {
    Succeeded,
    NoGain,
    Failed,
    Cancelled,
}

/// 绑定会话、清单、稳定行及尝试号；其他行的进度更新不使本行展示失效。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct JobAssetRequest {
    pub subscription_id: DecimalU64,
    pub selection_id: DecimalU64,
    pub job_id: u32,
    pub attempt: u32,
    pub expected_state: AssetState,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum RevealTarget {
    Result,
    Backup,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct RevealRequest {
    pub job: JobAssetRequest,
    pub target: RevealTarget,
}

/// 只确认已经请求系统文件管理器；不保证所有平台都选中文件而非仅打开目录。
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum RevealResult {
    Requested,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct ThumbnailDto {
    pub width: u32,
    pub height: u32,
    pub png: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum AssetError {
    SessionUnavailable,
    StaleTask,
    Unavailable,
    Busy,
    FileMissing,
    FileChanged,
    UnsafePath,
    ResourceLimit,
    DecodeFailed,
    RevealFailed,
    ServiceFault,
}

#[cfg(feature = "bindings")]
pub(crate) fn declarations() -> String {
    use ts_rs::{Config, TS};
    let config = Config::default();
    let types = [
        AssetState::decl(&config),
        JobAssetRequest::decl(&config),
        RevealTarget::decl(&config),
        RevealRequest::decl(&config),
        RevealResult::decl(&config),
        ThumbnailDto::decl(&config),
        AssetError::decl(&config),
    ];
    format!(
        "export const MAX_THUMBNAIL_WIDTH = {};\nexport const MAX_THUMBNAIL_HEIGHT = {};\nexport const MAX_THUMBNAIL_BYTES = {};\n{}\n",
        super::png::MAX_WIDTH,
        super::png::MAX_HEIGHT,
        super::png::MAX_PNG_BYTES,
        types
            .into_iter()
            .map(|value| format!("export {value}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}
