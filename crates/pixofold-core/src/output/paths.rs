//! 路径比较、保留产物名称及显式目录树输出的边界。只在create=true时创建目录。
//! 不自动删除结构目录，避免并发任务之间的目录所有权竞争；不提供文件系统CAS。

use crate::model::{OutputPolicy, ProcessingError};
use std::{
    ffi::{OsStr, OsString},
    fs::{self, Metadata},
    path::{Component, Path, PathBuf},
};

pub(super) const OUTPUT_PREFIX: &str = ".pixofold-output-";
const BACKUP_MARKER: &str = "-backup-";
pub(super) const RANDOM_LEN: usize = 6;

/// 保留原始stem（含非Unicode名称），不截断、不替换字符；路径过长由创建阶段安全拒绝。
pub(super) fn backup_prefix(source: &Path) -> Result<OsString, ProcessingError> {
    let stem = source.file_stem().ok_or(ProcessingError::InvalidPath)?;
    let mut prefix = stem.to_os_string();
    prefix.push(BACKUP_MARKER);
    Ok(prefix)
}

pub(crate) fn key(path: &Path) -> OsString {
    // Windows只为保守比较折叠；非Unicode及大小写敏感目录可能过度拒绝，绝不用于I/O。
    #[cfg(windows)]
    {
        path.as_os_str().to_string_lossy().to_uppercase().into()
    }
    #[cfg(not(windows))]
    {
        path.as_os_str().to_owned()
    }
}

pub(crate) fn is_link(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        } // FILE_ATTRIBUTE_REPARSE_POINT
    }
    false
}

pub(crate) fn validate_leaf(leaf: &OsStr) -> Result<(), ProcessingError> {
    if leaf.is_empty() {
        return Err(ProcessingError::InvalidPath);
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let units: Vec<_> = leaf.encode_wide().collect();
        if units.contains(&u16::from(b':')) || matches!(units.last(), Some(32 | 46)) {
            return Err(ProcessingError::InvalidPath);
        }
    }
    Ok(())
}

/// 去掉尾分隔符/点组件再做叶节点lstat，避免link/或link/.强制跟随链接。
/// 不折叠ParentDir或解析祖先；这不是路径沙箱，显式祖先别名沿用canonicalize边界。
pub(crate) fn entry_path(path: &Path) -> Result<PathBuf, ProcessingError> {
    if path.as_os_str().is_empty() {
        return Err(ProcessingError::InvalidPath);
    }
    Ok(path.components().collect())
}

pub(crate) fn directory(path: &Path) -> Result<PathBuf, ProcessingError> {
    let path = entry_path(path)?;
    let metadata =
        fs::symlink_metadata(&path).map_err(|e| ProcessingError::io("读取目录属性", e))?;
    if !metadata.is_dir() || is_link(&metadata) {
        return Err(ProcessingError::InvalidPath);
    }
    path.canonicalize()
        .map_err(|e| ProcessingError::io("解析目录", e))
}

/// 检查实际输出层保留的名称形状；不是任意文件的来源证明。不匹配_compressed或所有隐藏文件。
pub(crate) fn is_artifact(path: &Path) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };
    // 只比较ASCII边界，不把原始OS名称转成有损Unicode，非Unicode备份也必须排除。
    let name = name.as_encoded_bytes();
    #[cfg(windows)]
    let folded = name.to_ascii_lowercase();
    #[cfg(windows)]
    let name = folded.as_slice();
    let is_token =
        |token: &[u8]| token.len() == RANDOM_LEN && token.iter().all(u8::is_ascii_alphanumeric);
    if name
        .strip_prefix(OUTPUT_PREFIX.as_bytes())
        .and_then(|s| s.strip_suffix(b".tmp"))
        .is_some_and(is_token)
    {
        return true;
    }
    let Some(stem) = name.strip_suffix(b".png") else {
        return false;
    };
    let Some(split) = stem.len().checked_sub(RANDOM_LEN) else {
        return false;
    };
    let (prefix, token) = stem.split_at(split);
    // 旧.pixofold-backup-XXXXXX.png恰好以.pixofold为stem，同一严格规则继续兼容。
    is_token(token)
        && prefix
            .strip_suffix(BACKUP_MARKER.as_bytes())
            .is_some_and(|source_stem| !source_stem.is_empty())
}

pub(super) fn tree_path(
    root: &Path,
    relative: &Path,
    create: bool,
) -> Result<PathBuf, ProcessingError> {
    let root = directory(root)?;
    let parts: Vec<_> = relative.components().collect();
    if parts.is_empty() || parts.iter().any(|p| !matches!(p, Component::Normal(_))) {
        return Err(ProcessingError::InvalidPath);
    }
    let mut path = root;
    for (i, part) in parts.iter().enumerate() {
        validate_leaf(part.as_os_str())?;
        path.push(part.as_os_str());
        if i + 1 == parts.len() {
            break;
        }
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() && !is_link(&metadata) => {}
            Ok(_) => return Err(ProcessingError::InvalidPath),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if create {
                    match fs::create_dir(&path) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(e) => return Err(ProcessingError::io("创建输出结构目录", e)),
                    }
                    // 与其他worker共享父目录；创建竞争后仍检查类型，不信任AlreadyExists。
                    let metadata = fs::symlink_metadata(&path)
                        .map_err(|e| ProcessingError::io("复查输出目录", e))?;
                    if !metadata.is_dir() || is_link(&metadata) {
                        return Err(ProcessingError::InvalidPath);
                    }
                }
            }
            Err(e) => return Err(ProcessingError::io("检查输出结构目录", e)),
        }
    }
    Ok(path)
}

/// 只读解析并固定策略路径；None代表覆盖，不创建目录或最终产物。
pub(crate) fn copy_destination(
    policy: &mut OutputPolicy,
) -> Result<Option<PathBuf>, ProcessingError> {
    match policy {
        OutputPolicy::Overwrite | OutputPolicy::OverwriteWithoutBackup => Ok(None),
        OutputPolicy::Copy { destination } => {
            *destination = super::absolute_leaf(destination)?;
            Ok(Some(destination.clone()))
        }
        OutputPolicy::CopyTree { root, relative } => {
            *root = directory(root)?;
            tree_path(root, relative, false).map(Some)
        }
    }
}
