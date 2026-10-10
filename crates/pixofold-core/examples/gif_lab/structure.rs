//! 兼容首轮manifest的验证额度/错误类型，实际结构检查由生产核心拥有。
pub use pixofold_core::gif::{
    GifValidationCode as Code, GifValidationError as LabError, GifValidationLimits as Limits,
};
