//! 只读打开/复查原生选择的输出根；目录身份模型不反向依赖文件提交模块。
//! 校验与提交仍是路径级复查，不提供针对恶意文件系统替换的原子CAS。

use super::paths;
use crate::model::{OutputDirectory, ProcessingError};
use std::path::Path;

impl OutputDirectory {
    /// 只读固定目录身份，不创建目录或测试文件。
    /// # Errors
    /// 非目录、链接、读取失败或选择期间身份变化均拒绝。
    pub fn open(path: &Path) -> Result<Self, ProcessingError> {
        let path = paths::directory(path)?;
        let identity = same_file::Handle::from_path(&path)
            .map_err(|e| ProcessingError::io("读取输出目录身份", e))?;
        let result = Self::from_identity(path, identity);
        result.verify()?;
        Ok(result)
    }

    pub(crate) fn verify(&self) -> Result<(), ProcessingError> {
        let path = paths::directory(self.path())?;
        let identity = same_file::Handle::from_path(&path)
            .map_err(|e| ProcessingError::io("复查输出目录身份", e))?;
        if !self.has_identity(&path, &identity) {
            return Err(ProcessingError::TargetConflict);
        }
        Ok(())
    }
}
