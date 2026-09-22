//! 唯一最终文件提交层：独占临时文件、同步、复查、备份、同目录替换。
//! 不采用先删除源文件的回退；覆盖备份在成功和提交失败后均保留。
//! 检查点不是文件系统 compare-and-swap；不承诺抵御恶意路径竞争或断电事务性。

use std::{
    fs::{self, File, Metadata},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

use tempfile::{Builder, NamedTempFile};

use crate::model::{
    CancellationToken, OutputPolicy, ProcessingError, ProcessingOutcome, ProcessingStage,
    ResourceLimits,
};

#[derive(Debug, PartialEq, Eq)]
struct Fingerprint {
    len: u64,
    modified: SystemTime,
    created: Option<SystemTime>,
    readonly: bool,
}

impl Fingerprint {
    fn read(metadata: &Metadata) -> Result<Self, ProcessingError> {
        Ok(Self {
            len: metadata.len(),
            modified: metadata
                .modified()
                .map_err(|e| ProcessingError::io("读取修改时间", e))?,
            // 某些 Unix 文件系统没有 birth time；内容与修改时间复查仍为必需。
            created: metadata.created().ok(),
            readonly: metadata.permissions().readonly(),
        })
    }
}

pub(crate) struct Source {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    metadata: Metadata,
    fingerprint: Fingerprint,
    identity: same_file::Handle,
}

impl Source {
    pub fn read(path: &Path, limits: ResourceLimits) -> Result<Self, ProcessingError> {
        let path = absolute_leaf(path)?;
        regular_metadata(&path)?;
        let mut file = File::open(&path).map_err(|e| ProcessingError::io("打开源文件", e))?;
        let metadata = file
            .metadata()
            .map_err(|e| ProcessingError::io("读取源属性", e))?;
        if !metadata.is_file() {
            return Err(ProcessingError::InvalidPath);
        }
        let fingerprint = Fingerprint::read(&metadata)?;
        let bytes = read_bounded(&mut file, limits.max_input_bytes.0)?;
        let after = file
            .metadata()
            .map_err(|e| ProcessingError::io("复查源属性", e))?;
        if bytes.len() as u64 != fingerprint.len || Fingerprint::read(&after)? != fingerprint {
            return Err(ProcessingError::SourceChanged);
        }
        let identity = same_file::Handle::from_file(file)
            .map_err(|e| ProcessingError::io("读取源文件身份", e))?;
        regular_metadata(&path)?;
        if identity
            != same_file::Handle::from_path(&path)
                .map_err(|e| ProcessingError::io("复查源路径身份", e))?
        {
            return Err(ProcessingError::SourceChanged);
        }
        Ok(Self {
            path,
            bytes,
            metadata,
            fingerprint,
            identity,
        })
    }

    pub fn verify_unchanged(&self, limits: ResourceLimits) -> Result<(), ProcessingError> {
        let current = Self::read(&self.path, limits).map_err(|error| match error {
            ProcessingError::InvalidPath | ProcessingError::ResourceLimit(_) => {
                ProcessingError::SourceChanged
            }
            ProcessingError::Io { ref source, .. } if source.kind() == io::ErrorKind::NotFound => {
                ProcessingError::SourceChanged
            }
            other => other,
        })?;
        if self.identity != current.identity
            || self.fingerprint != current.fingerprint
            || self.bytes != current.bytes
        {
            return Err(ProcessingError::SourceChanged);
        }
        Ok(())
    }
}

pub(crate) struct Destination {
    pub path: PathBuf,
    overwrite: bool,
}

impl Destination {
    pub fn plan(source: &Source, policy: &OutputPolicy) -> Result<Self, ProcessingError> {
        match policy {
            OutputPolicy::Overwrite => {
                if source.metadata.permissions().readonly() {
                    return Err(ProcessingError::io(
                        "源文件为只读",
                        io::ErrorKind::PermissionDenied.into(),
                    ));
                }
                Ok(Self {
                    path: source.path.clone(),
                    overwrite: true,
                })
            }
            OutputPolicy::Copy { destination } => {
                let path = absolute_leaf(destination)?;
                ensure_absent(&path)?;
                Ok(Self {
                    path,
                    overwrite: false,
                })
            }
        }
    }

    pub fn stage(&self) -> Result<NamedTempFile, ProcessingError> {
        new_temp(parent(&self.path)?, ".pixofold-output-", ".tmp")
    }
}

pub(crate) fn write_candidate(
    temp: &mut NamedTempFile,
    bytes: &[u8],
    source: &Source,
) -> Result<(), ProcessingError> {
    temp.write_all(bytes)
        .map_err(|e| ProcessingError::io("写入临时产物", e))?;
    temp.flush()
        .map_err(|e| ProcessingError::io("刷新临时产物", e))?;
    // 只读输入仍可另存；不能让临时产物也变只读，导致 Windows 清理失败。
    // 覆盖模式已拒绝只读源图，副本的只读属性不继承。
    if !source.metadata.permissions().readonly() {
        temp.as_file()
            .set_permissions(source.metadata.permissions())
            .map_err(|e| ProcessingError::io("设置输出权限", e))?;
    }
    temp.as_file()
        .sync_all()
        .map_err(|e| ProcessingError::io("同步临时产物", e))
}

pub(crate) fn read_candidate(
    temp: &NamedTempFile,
    limits: ResourceLimits,
) -> Result<Vec<u8>, ProcessingError> {
    regular_metadata(temp.path())?;
    let identity = same_file::Handle::from_file(
        temp.as_file()
            .try_clone()
            .map_err(|e| ProcessingError::io("读取临时文件句柄", e))?,
    )
    .map_err(|e| ProcessingError::io("读取临时文件身份", e))?;
    if identity
        != same_file::Handle::from_path(temp.path())
            .map_err(|e| ProcessingError::io("复查临时路径身份", e))?
    {
        return Err(ProcessingError::ValidationFailed("临时路径被替换"));
    }
    // reopen 绑定原文件而非重新信任临时路径；重新从文件读取实际落盘内容。
    let mut file = temp
        .reopen()
        .map_err(|e| ProcessingError::io("重新打开临时产物", e))?;
    read_bounded(&mut file, limits.max_input_bytes.0)
}

pub(crate) fn commit(
    temp: NamedTempFile,
    validated_bytes: &[u8],
    destination: &Destination,
    source: Source,
    limits: ResourceLimits,
    cancel: &CancellationToken,
    on_stage: &mut impl FnMut(ProcessingStage),
) -> Result<ProcessingOutcome, ProcessingError> {
    let mut backup = None;
    let prepared = (|| {
        cancel.check()?;
        source.verify_unchanged(limits)?;
        if destination.overwrite {
            let mut file = new_temp(parent(&source.path)?, ".pixofold-backup-", ".png")?;
            if let Err(error) = write_candidate(&mut file, &source.bytes, &source) {
                return Err(discard(file, error));
            }
            backup = Some(file);
        }
        on_stage(ProcessingStage::BeforeCommit);
        cancel.check()?;
        if read_candidate(&temp, limits)? != validated_bytes {
            return Err(ProcessingError::ValidationFailed(
                "临时产物在验证后发生变化",
            ));
        }
        if let Some(file) = &backup
            && read_candidate(file, limits)? != source.bytes
        {
            return Err(ProcessingError::ValidationFailed("备份与输入不一致"));
        }
        source.verify_unchanged(limits)?;
        if !destination.overwrite {
            ensure_absent(&destination.path)?;
        }
        cancel.check()?;
        Ok(())
    })();
    if let Err(mut error) = prepared {
        if let Some(file) = backup {
            error = discard(file, error);
        }
        return Err(discard(temp, error));
    }
    // Windows 的替换可能被本进程的源句柄阻止，身份检查完成后必须关闭。
    // 关闭与替换之间不具备 compare-and-swap，不能据此宣称消除了外部路径竞争。
    drop(source);
    // 临界区起点：之后的取消不再改变结果。先保留完整备份，再执行单次替换。
    if let Some(file) = backup {
        let backup_path = match file.keep() {
            Ok((handle, path)) => {
                drop(handle);
                path
            }
            Err(error) => {
                let cause = ProcessingError::io("保留恢复备份", error.error);
                return Err(discard(temp, discard(error.file, cause)));
            }
        };
        match temp.persist(&destination.path) {
            Ok(handle) => {
                drop(handle);
                Ok(ProcessingOutcome::Optimized {
                    output: destination.path.clone(),
                    backup: Some(backup_path),
                })
            }
            Err(error) => {
                let cause = ProcessingError::CommitFailed {
                    source: error.error,
                    backup: backup_path,
                };
                Err(discard(error.file, cause))
            }
        }
    } else {
        match temp.persist_noclobber(&destination.path) {
            Ok(handle) => {
                drop(handle);
                Ok(ProcessingOutcome::Optimized {
                    output: destination.path.clone(),
                    backup: None,
                })
            }
            Err(error) => {
                let cause = if error.error.kind() == io::ErrorKind::AlreadyExists {
                    ProcessingError::TargetConflict
                } else {
                    ProcessingError::io("提交副本", error.error)
                };
                Err(discard(error.file, cause))
            }
        }
    }
}

pub(crate) fn discard(temp: NamedTempFile, original: ProcessingError) -> ProcessingError {
    let path = temp.path().to_path_buf();
    match temp.close() {
        Ok(()) => original,
        Err(source) => ProcessingError::CleanupFailed {
            original: Some(Box::new(original)),
            source,
            temporary: path,
        },
    }
}

pub(crate) fn discard_no_gain(temp: NamedTempFile) -> Result<(), ProcessingError> {
    let temporary = temp.path().to_path_buf();
    temp.close()
        .map_err(|source| ProcessingError::CleanupFailed {
            original: None,
            source,
            temporary,
        })
}

fn new_temp(
    directory: &Path,
    prefix: &str,
    suffix: &str,
) -> Result<NamedTempFile, ProcessingError> {
    Builder::new()
        .prefix(prefix)
        .suffix(suffix)
        .tempfile_in(directory)
        .map_err(|e| ProcessingError::io("创建独占临时文件", e))
}

fn parent(path: &Path) -> Result<&Path, ProcessingError> {
    path.parent().ok_or(ProcessingError::InvalidPath)
}

fn absolute_leaf(path: &Path) -> Result<PathBuf, ProcessingError> {
    let leaf = path.file_name().ok_or(ProcessingError::InvalidPath)?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let units: Vec<_> = leaf.encode_wide().collect();
        // 不将 NTFS 备用数据流或 Win32 尾随字符别名当作独立图片输出。
        if units.contains(&u16::from(b':')) || matches!(units.last(), Some(32 | 46)) {
            return Err(ProcessingError::InvalidPath);
        }
    }
    let directory = parent(path)?;
    let directory = if directory.as_os_str().is_empty() {
        Path::new(".")
    } else {
        directory
    };
    let directory = directory
        .canonicalize()
        .map_err(|e| ProcessingError::io("解析父目录", e))?;
    Ok(directory.join(leaf))
}

fn regular_metadata(path: &Path) -> Result<Metadata, ProcessingError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|e| ProcessingError::io("读取文件属性", e))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(ProcessingError::InvalidPath);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(ProcessingError::InvalidPath);
        }
    }
    Ok(metadata)
}

fn ensure_absent(path: &Path) -> Result<(), ProcessingError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(ProcessingError::TargetConflict),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ProcessingError::io("检查目标冲突", error)),
    }
}

fn read_bounded(file: &mut File, limit: u64) -> Result<Vec<u8>, ProcessingError> {
    let size = file
        .metadata()
        .map_err(|e| ProcessingError::io("读取文件大小", e))?
        .len();
    if size > limit {
        return Err(ProcessingError::ResourceLimit("输入文件字节数"));
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(size as usize)
        .map_err(|_| ProcessingError::ResourceLimit("文件内存分配"))?;
    file.take(limit + 1)
        .read_to_end(&mut result)
        .map_err(|e| ProcessingError::io("读取文件", e))?;
    if result.len() as u64 > limit {
        return Err(ProcessingError::ResourceLimit("输入文件字节数"));
    }
    Ok(result)
}
