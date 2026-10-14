//! 实验复用核心验证入口，人工画布/原始像素oracle继续独立于编码与验证器。
pub use pixofold_core::apng::{
    ApngValidationLimits as Limits, compare_apng as compare, validate_apng as inspect,
};
pub fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
#[cfg(test)]
mod tests;
