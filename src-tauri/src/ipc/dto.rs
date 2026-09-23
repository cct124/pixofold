//! Wire模型与输入校验；所有大整数使用规范十进制字符串，避免JS精度损失。

use pixofold_core::model::PngMode;
use serde::{Deserialize, Serialize};

pub(super) const MAX_PAGE_SIZE: u16 = 100;
pub(super) const MAX_DISPLAY_CHARS: usize = 240;
pub(crate) const TASK_PROTOCOL_VERSION: u32 = 1;

/// 无符号64位十进制字符串；拒绝数字JSON、符号、空白、前导零及溢出。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct DecimalU64(#[cfg_attr(feature = "bindings", ts(type = "string"))] pub u64);
impl Serialize for DecimalU64 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for DecimalU64 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.is_empty()
            || value.len() > 20
            || !value.bytes().all(|b| b.is_ascii_digit())
            || (value.len() > 1 && value.starts_with('0'))
        {
            return Err(serde::de::Error::custom("需要规范u64十进制字符串"));
        }
        value
            .parse()
            .map(Self)
            .map_err(|_| serde::de::Error::custom("u64超出范围"))
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum TaskCollection {
    Jobs,
    Candidates,
    Issues,
}

/// limit必须为1–100；翻页必须携带首个响应revision。null只用于从第一页恢复最新状态。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct TaskPageRequest {
    pub expected_revision: Option<DecimalU64>,
    pub collection: TaskCollection,
    pub offset: u32,
    pub limit: u16,
}
impl TaskPageRequest {
    pub(super) fn validate(&self) -> Result<(), QueryError> {
        if !(1..=MAX_PAGE_SIZE).contains(&self.limit)
            || (self.offset != 0 && self.expected_revision.is_none())
        {
            return Err(QueryError::InvalidPage);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(
    tag = "code",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum QueryError {
    InvalidPage,
    StaleSnapshot { current_revision: DecimalU64 },
    InvalidSnapshot,
}

/// 首次订阅票据和后续通知共用信封；不包含任务行、文件名或路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct TaskChangeNotice {
    pub protocol_version: u32,
    pub subscription_id: DecimalU64,
    pub revision: DecimalU64,
}

/// 只确认实际收到且已查询的票据；不能用未来revision释放背压。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct TaskChangeAck {
    pub subscription_id: DecimalU64,
    pub revision: DecimalU64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct TaskSubscriptionRequest {
    pub subscription_id: DecimalU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum SubscriptionError {
    Closed,
    StaleSubscription,
    InvalidAcknowledgement,
    IdExhausted,
    ServiceFault,
}
impl std::fmt::Display for SubscriptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Closed => "任务订阅已关闭",
            Self::StaleSubscription => "任务订阅已被替换或移除",
            Self::InvalidAcknowledgement => "确认版本不是已发送版本",
            Self::IdExhausted => "任务订阅标识已耗尽",
            Self::ServiceFault => "任务订阅服务异常",
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct DisplayName {
    pub text: String,
    pub truncated: bool,
    pub lossy: bool,
    pub sanitized: bool,
}

// 仅对一一对应的无数据枚举共用生成规则；转换穷尽匹配，源新增状态会导致编译失败。
macro_rules! wire_enum {
    ($name:ident, $source:path, [$($variant:ident),+ $(,)?]) => {
        #[derive(Debug, Clone, Copy, Serialize)]
        #[serde(rename_all = "snake_case")]
        #[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
        pub(crate) enum $name { $($variant),+ }
        impl From<$source> for $name {
            fn from(value: $source) -> Self {
                match value { $(<$source>::$variant => Self::$variant),+ }
            }
        }
    };
}
wire_enum!(
    TaskPhaseDto,
    crate::tasks::TaskPhase,
    [
        Idle, Scanning, Ready, Preparing, Running, Cancelling, Finished, Cancelled, Rejected,
        Clearing, Closing, Closed,
    ]
);
wire_enum!(
    BatchPhaseDto,
    pixofold_core::batch::BatchPhase,
    [Running, Cancelling, Finished]
);
wire_enum!(
    StageDto,
    pixofold_core::model::ProcessingStage,
    [Reading, Optimizing, Validating, BeforeCommit]
);
wire_enum!(
    ScanLimitDto,
    pixofold_core::import::ScanLimit,
    [Entries, Files, Depth, ReadBytes]
);
wire_enum!(
    JobErrorDto,
    pixofold_core::batch::JobErrorCode,
    [
        InvalidInput,
        UnsupportedFormat,
        UnsupportedAnimation,
        ResourceLimit,
        Decode,
        Encode,
        Validation,
        TargetConflict,
        SourceChanged,
        Io,
        CommitFailed,
        CleanupFailed,
        WorkerPanicked,
        ServiceFault,
    ]
);
wire_enum!(
    UnsupportedFormatDto,
    pixofold_core::import::UnsupportedFormat,
    [Jpeg, Gif, WebP, Other]
);
wire_enum!(
    PathConflictDto,
    pixofold_core::batch::PathConflictKind,
    [
        DuplicateSource,
        DuplicateOutput,
        OutputIsInput,
        OutputHierarchy
    ]
);

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum ScanStatusDto {
    Scanning,
    Complete,
    Cancelled,
    Limited { limit: ScanLimitDto },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct ScanProgressDto {
    pub status: ScanStatusDto,
    pub discovered: u32,
    pub examined: u32,
    pub accepted: u32,
    pub duplicates: u32,
    pub excluded: u32,
    pub rejected: u32,
    pub read_bytes: DecimalU64,
}

/// 仅供展示恢复提示；原生路径仍留在Rust，不能将这些名称传回当作文件权限。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct RecoveryDto {
    pub backup_name: Option<DisplayName>,
    pub temporary_name: Option<DisplayName>,
    pub original_error: Option<RecoveryCauseDto>,
}

/// 取消本身不是服务故障；清理失败可同时保留先前的取消或处理失败。
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum RecoveryCauseDto {
    Cancelled,
    Failed { code: JobErrorDto },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct JobFailureDto {
    pub code: JobErrorDto,
    pub recovery: Option<RecoveryDto>,
}

#[derive(Debug, Serialize)]
#[serde(
    tag = "code",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum TaskFailureDto {
    Busy,
    Closed,
    StaleSelection,
    StaleBatch,
    NotReady,
    TooManyRoots,
    IdExhausted,
    TimedOut,
    ServiceFault,
    WorkerPanicked,
    WorkerStart,
    InvalidConfig,
    EmptyBatch,
    TooManyJobs,
    NoSuchBatch,
    InvalidRetry,
    Cancelled,
    InvalidParameters,
    InvalidOptions,
    IncompleteScan,
    NoFiles,
    RootNameConflict,
    PathConflict {
        first: u32,
        second: u32,
        kind: PathConflictDto,
    },
    File {
        index: u32,
        failure: JobFailureDto,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct BatchSummaryDto {
    pub total: u32,
    pub queued: u32,
    pub running: u32,
    pub succeeded: u32,
    pub no_gain: u32,
    pub failed: u32,
    pub cancelled: u32,
    pub processed: u32,
    pub terminal: u32,
    pub input_bytes: Option<DecimalU64>,
    pub current_bytes: Option<DecimalU64>,
    pub saved_bytes: Option<DecimalU64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct BatchOverviewDto {
    pub id: DecimalU64,
    pub revision: DecimalU64,
    pub phase: BatchPhaseDto,
    pub mode: PngMode,
    pub summary: BatchSummaryDto,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum FallbackDto {
    HighBitDepth,
    ColorMetadata,
    RepresentationMetadata,
    TransparencyGuard,
    NoSizeBenefit,
    QualityBelowTarget { measured: Option<u8> },
}

#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum ProcessingDto {
    Lossless,
    Lossy {
        mapping_version: u32,
        measured_quality: u8,
    },
    LosslessFallback {
        mapping_version: u32,
        reason: FallbackDto,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct ReportDto {
    pub input_bytes: DecimalU64,
    pub output_bytes: DecimalU64,
    pub elapsed_ms: DecimalU64,
    pub processing: ProcessingDto,
    pub output_name: Option<DisplayName>,
    pub backup_name: Option<DisplayName>,
}

#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum JobStateDto {
    Queued,
    Running {
        stage: StageDto,
        cancel_requested: bool,
    },
    Succeeded {
        report: ReportDto,
    },
    NoGain {
        report: ReportDto,
    },
    Failed {
        failure: JobFailureDto,
    },
    Cancelled,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct JobDto {
    pub id: u32,
    pub attempt: u32,
    pub source_name: DisplayName,
    pub mode: PngMode,
    pub input_bytes: Option<DecimalU64>,
    pub state: JobStateDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct CandidateDto {
    pub index: u32,
    pub source_name: DisplayName,
    pub input_bytes: DecimalU64,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum IssueKindDto {
    Failure { failure: JobFailureDto },
    Unsupported { format: UnsupportedFormatDto },
    Duplicate { first_name: DisplayName },
    GeneratedArtifact,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct IssueDto {
    pub index: u32,
    pub source_name: DisplayName,
    pub issue: IssueKindDto,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) enum TaskPageDto {
    Jobs {
        offset: u32,
        total: u32,
        items: Vec<JobDto>,
    },
    Candidates {
        offset: u32,
        total: u32,
        items: Vec<CandidateDto>,
    },
    Issues {
        offset: u32,
        total: u32,
        items: Vec<IssueDto>,
    },
}

/// 一个原子版本的摘要与一页明细；不包含图像、绝对路径、底层错误或任意文件访问能力。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct TaskSnapshotDto {
    pub protocol_version: u32,
    pub revision: DecimalU64,
    pub selection_id: Option<DecimalU64>,
    pub phase: TaskPhaseDto,
    pub scan: Option<ScanProgressDto>,
    pub error: Option<TaskFailureDto>,
    pub batch: Option<BatchOverviewDto>,
    pub page: TaskPageDto,
}

#[cfg(feature = "bindings")]
pub(super) fn declarations() -> String {
    use ts_rs::{Config, TS};
    let config = Config::default();
    let mut output = String::from(
        "// 由 Rust 任务DTO生成；请运行 pnpm types:generate，勿手工编辑。\nimport type { PngMode } from './generated';\n",
    );
    output.push_str(&format!(
        "export const TASK_PROTOCOL_VERSION = {TASK_PROTOCOL_VERSION};\nexport const MAX_TASK_PAGE_SIZE = {MAX_PAGE_SIZE};\n"
    ));
    macro_rules! export {
        ($($ty:ty),+ $(,)?) => { $(output.push_str(&format!("export {}\n", <$ty>::decl(&config)));)+ };
    }
    export!(
        DecimalU64,
        TaskCollection,
        TaskPageRequest,
        QueryError,
        TaskChangeNotice,
        TaskChangeAck,
        TaskSubscriptionRequest,
        SubscriptionError,
        DisplayName,
        TaskPhaseDto,
        BatchPhaseDto,
        StageDto,
        ScanLimitDto,
        JobErrorDto,
        UnsupportedFormatDto,
        PathConflictDto,
        ScanStatusDto,
        ScanProgressDto,
        RecoveryDto,
        RecoveryCauseDto,
        JobFailureDto,
        TaskFailureDto,
        BatchSummaryDto,
        BatchOverviewDto,
        FallbackDto,
        ProcessingDto,
        ReportDto,
        JobStateDto,
        JobDto,
        CandidateDto,
        IssueKindDto,
        IssueDto,
        TaskPageDto,
        TaskSnapshotDto
    );
    output
}
