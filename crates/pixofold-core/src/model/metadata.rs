//! PNG/JPEG元数据变更显式绑定已检查的源版本和格式；令牌不序列化、不包含源文件内容。

use std::{fmt, path::PathBuf, time::SystemTime};

/// 内容凭据处理策略；默认不允许删除。移除授权只适用于产生该版本的原文件。
#[derive(Debug, Clone, Default)]
pub enum MetadataPolicy {
    #[default]
    Preserve,
    RemoveContentCredentials(ContentCredentialsSource),
}

/// 保留既有PNG调用方名称；底层来源版本仍区分格式。
pub type PngMetadataPolicy = MetadataPolicy;
/// JPEG内容凭据策略，必须从JPEG完整检查的错误取得来源版本。
pub type JpegMetadataPolicy = MetadataPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CredentialsFormat {
    Png,
    Jpeg,
}

/// 完整解码/元数据检查后返回的不透明来源版本；调用方不能自行构造或修改。
/// 仅从ContentCredentialsRequireConsent取得；不是凭据签名/真实性验证结果。
#[derive(Clone, PartialEq, Eq)]
pub struct ContentCredentialsSource {
    pub(crate) format: CredentialsFormat,
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
