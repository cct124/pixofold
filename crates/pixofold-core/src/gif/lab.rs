//! 显式开发实验可生成尚未验证的内存候选；生产输出只能经optimize_gif。
pub use super::comparison::{Comparison, Metrics, compare};
use super::{GifEngine, GifError, GifLimits, validate_gif};
use crate::model::CancellationToken;

/// lossiness为原生强度0–100，None使用无损配方；此函数不提交文件。
/// # Errors
/// 输入/资源、工具身份、取消/超时及回收失败。返回候选仍须独立验证。
pub fn candidate(
    bytes: &[u8],
    engine: &GifEngine,
    limits: GifLimits,
    lossiness: Option<u16>,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, GifError> {
    if lossiness.is_some_and(|value| value > 100) {
        return Err(GifError::InvalidLimits);
    }
    validate_gif(bytes, limits, cancel)?;
    engine.run(bytes, limits, cancel, lossiness)
}
