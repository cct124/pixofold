//! 受限APNG无损单文件核心。验证独立于oxipng，只有共享output层可提交最终文件。
//! 不依赖Tauri/批量服务；不接受凭据移除许可，不把失败或无收益转换成静态PNG输出。
mod model;
mod pipeline;
mod validation;

use crate::model::{CancellationToken, ProcessingStage};
pub use model::{ApngError, ApngInfo, ApngLimits, ApngProcessing, ApngReport, ApngRequest};
pub use validation::{
    Control as ApngFrameControl, Error as ApngValidationError, FrameSummary as ApngFrame,
    Limits as ApngValidationLimits, Summary as ApngPlayback,
};

/// 仅按PNG块边界识别动画声明；不是有效性证明，调用方仍须结构预检/完整验证。
pub fn has_animation_chunks(bytes: &[u8]) -> bool {
    validation::has_animation_chunks(bytes)
}

/// 只读容器、CRC及帧控制，不分配像素、不执行编码；供导入扫描使用。
/// # Errors
/// 不支持的表示/元数据、损坏、结构额度、取消或期限到期返回APNG专属错误。
pub fn inspect_structure(
    bytes: &[u8],
    limits: ApngLimits,
    cancel: &CancellationToken,
) -> Result<ApngInfo, ApngError> {
    limits.check()?;
    Ok(validation::structure_info(
        bytes,
        &limits.validation,
        cancel,
    )?)
}

/// 首幅动画在透明画布上的RGBA8预览；独立默认图不参与，不播放后续帧。
#[derive(Debug)]
pub struct ApngPreview {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// 有界首帧解码；无I/O、无helper，调用方须持有单解码许可直到返回。
/// # Errors
/// 沿用支持矩阵；资源上限、坏输入、取消及协作期限到期返回专属错误。
pub fn decode_preview(
    bytes: &[u8],
    limits: &ApngValidationLimits,
    cancel: &CancellationToken,
) -> Result<ApngPreview, ApngError> {
    Ok(validation::preview(bytes, limits, cancel)?)
}

/// 有界验证APNG容器、原始默认图/子帧、合成、原始延时及第二轮播放。
/// 无I/O或写入；返回摘要只用于验证，不提供路径或重试权限。
/// # Errors
/// 不支持的表示/元数据、损坏、资源超限、取消或协作期限到期返回固定代码。
pub fn validate_apng(
    bytes: &[u8],
    limits: &ApngValidationLimits,
    cancel: &CancellationToken,
) -> Result<ApngPlayback, ApngValidationError> {
    validation::inspect(bytes, limits, cancel)
}
/// 严格比较默认图、原始子帧、控制字段、元数据及播放；源/候选共用累计工作额度。
/// 不写文件。候选块分片可变，原始时间分数/像素表示/隐藏RGB必须保持。
/// # Errors
/// 返回验证拒绝，或not_equivalent；取消和期限只在受控检查点响应。
pub fn compare_apng(
    source: &[u8],
    candidate: &[u8],
    limits: &ApngValidationLimits,
    cancel: &CancellationToken,
) -> Result<(), ApngValidationError> {
    validation::compare(source, candidate, limits, cancel)
}
/// 固定O1严格无损，实际候选落盘复验后通过共享输出层提交。
/// 默认覆盖并建立恢复备份；副本独占创建。无收益、失败和取消保留原图。
/// 准备阶段总期限包含读取/验证/同步编码；同步编码不能抢占，返回前调用方必须持有资源许可。
/// 进入提交层后沿用其取消检查和不可逆临界区契约，不在提交后改报超时。
/// # Errors
/// 资源/验证/编码、路径/权限/冲突/源变化、清理失败及取消分别返回可识别错误。
pub fn optimize_apng(
    request: &ApngRequest,
    cancel: &CancellationToken,
    on_stage: impl FnMut(ProcessingStage),
) -> Result<ApngReport, ApngError> {
    pipeline::run(request, cancel, on_stage)
}
