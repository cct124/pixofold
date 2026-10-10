//! 开发验收调用生产独立验证器，人工预期仍由独立生成器固定。
use super::structure::{Code, LabError, Limits};
pub use pixofold_core::gif::{
    GifEvent as Event, GifPlayback as Playback, normalize_timeline as normalize,
};
use pixofold_core::{
    gif::{GifError, GifLimits, validate_gif},
    model::CancellationToken,
};
pub fn inspect(bytes: &[u8], validation: Limits) -> Result<Playback, LabError> {
    validate_gif(
        bytes,
        GifLimits {
            validation,
            ..GifLimits::default()
        },
        &CancellationToken::default(),
    )
    .map_err(|error| match error {
        GifError::Validation(error) => error,
        GifError::ResourceLimit(reason) => LabError {
            code: Code::ResourceLimit,
            reason,
        },
        GifError::Timeout => LabError {
            code: Code::TimedOut,
            reason: "验证期限",
        },
        GifError::Cancelled => LabError {
            code: Code::Cancelled,
            reason: "验证取消",
        },
        _ => LabError {
            code: Code::ResourceLimit,
            reason: "验证配置无效",
        },
    })
}
