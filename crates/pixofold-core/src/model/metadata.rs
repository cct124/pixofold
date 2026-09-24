//! PNG元数据变更必须显式绑定已检查的源版本；令牌不序列化、不包含源文件内容。

use std::{fmt, path::PathBuf, time::SystemTime};

/// 内容凭据处理策略；默认不允许删除。移除授权只适用于产生该版本的原文件。
#[derive(Debug, Clone, Default)]
pub enum PngMetadataPolicy {
    #[default]
    Preserve,
    RemoveContentCredentials(ContentCredentialsSource),
}

/// 完整解码/元数据检查后返回的不透明来源版本；调用方不能自行构造或修改。
/// 仅从ContentCredentialsRequireConsent取得；不是凭据签名/真实性验证结果。
#[derive(Clone, PartialEq, Eq)]
pub struct ContentCredentialsSource {
    pub(crate) path: PathBuf,
    pub(crate) length: u64,
    pub(crate) modified: SystemTime,
    pub(crate) created: Option<SystemTime>,
    pub(crate) readonly: bool,
    pub(crate) sha256: [u8; 32],
}

impl fmt::Debug for ContentCredentialsSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ContentCredentialsSource(<private>)")
    }
}
