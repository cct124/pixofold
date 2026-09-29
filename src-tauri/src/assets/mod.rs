//! 当前任务的只读展示能力：受控文件定位与有界缩略图。
//! 单个解码/定位在途，不创建无界队列；I/O不持状态锁，清除/重载使迟到结果失效。

mod dto;
mod png;
#[cfg(test)]
mod tests;
pub(crate) use dto::*;

use crate::tasks::{TaskPhase, TaskSnapshot};
use pixofold_core::{
    batch::JobState,
    model::{ProcessingError, ProcessingOutcome},
};
use std::{
    collections::VecDeque,
    fs::{File, Metadata},
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    time::SystemTime,
};

const MAX_CACHE_ENTRIES: usize = 64;
const MAX_CACHE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
struct State {
    closed: bool,
    generation: u64,
    thumbnail_busy: bool,
    reveal_busy: bool,
    cache: VecDeque<CacheEntry>,
    cache_bytes: usize,
}
#[derive(Default)]
pub(crate) struct AssetService {
    state: Mutex<State>,
    idle: Condvar,
}
#[derive(Clone, Copy)]
pub(crate) enum Operation {
    Thumbnail,
    Reveal,
}
pub(crate) struct Permit {
    owner: Arc<AssetService>,
    generation: u64,
    operation: Operation,
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut state = self
            .owner
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        match self.operation {
            Operation::Thumbnail => state.thumbnail_busy = false,
            Operation::Reveal => state.reveal_busy = false,
        }
        self.owner.idle.notify_all();
    }
}
#[derive(Clone, PartialEq, Eq)]
struct FileStamp {
    bytes: u64,
    modified: SystemTime,
    created: Option<SystemTime>,
}
impl FileStamp {
    fn new(metadata: &Metadata) -> Result<Self, AssetError> {
        Ok(Self {
            bytes: metadata.len(),
            modified: metadata.modified().map_err(|_| AssetError::Unavailable)?,
            created: metadata.created().ok(),
        })
    }
}
pub(crate) struct PreparedThumbnail {
    request: JobAssetRequest,
    path: PathBuf,
    stamp: FileStamp,
    image: ThumbnailDto,
}
type CacheEntry = PreparedThumbnail;

impl AssetService {
    pub(crate) fn reserve(self: &Arc<Self>, operation: Operation) -> Result<Permit, AssetError> {
        let mut state = self.state.lock().map_err(|_| AssetError::ServiceFault)?;
        if state.closed {
            return Err(AssetError::Unavailable);
        }
        let busy = match operation {
            Operation::Thumbnail => &mut state.thumbnail_busy,
            Operation::Reveal => &mut state.reveal_busy,
        };
        if *busy {
            return Err(AssetError::Busy);
        }
        *busy = true;
        Ok(Permit {
            owner: self.clone(),
            generation: state.generation,
            operation,
        })
    }

    /// 只释放内存缓存，不等待在途I/O；世代使旧工作不能重新填充缓存。
    pub(crate) fn invalidate(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.generation = state.generation.wrapping_add(1);
        state.cache.clear();
        state.cache_bytes = 0;
    }

    pub(crate) fn close(&self) {
        self.invalidate();
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .closed = true;
    }

    /// 仅退出收尾线程调用；不在任务/订阅锁中等待，不依赖页面清理。
    pub(crate) fn wait_idle(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        while state.thumbnail_busy || state.reveal_busy {
            state = self
                .idle
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    fn current(state: &State, permit: &Permit) -> Result<(), AssetError> {
        if state.closed || state.generation != permit.generation {
            return Err(AssetError::StaleTask);
        }
        Ok(())
    }

    /// 打开并复查授权路径，即使命中缓存也检查外部删除/变化。无锁读/解码，不写磁盘。
    pub(crate) fn prepare(
        &self,
        permit: &Permit,
        request: JobAssetRequest,
        path: PathBuf,
    ) -> Result<PreparedThumbnail, AssetError> {
        validate_file(&path)?;
        let file = File::open(&path).map_err(io_error)?;
        let stamp = FileStamp::new(&file.metadata().map_err(io_error)?)?;
        if stamp.bytes > png::MAX_INPUT_BYTES {
            return Err(AssetError::ResourceLimit);
        }
        let identity =
            same_file::Handle::from_file(file.try_clone().map_err(io_error)?).map_err(io_error)?;
        let cached = {
            let mut state = self.state.lock().map_err(|_| AssetError::ServiceFault)?;
            Self::current(&state, permit)?;
            let index = state.cache.iter().position(|entry| {
                entry.request == request && entry.path == path && entry.stamp == stamp
            });
            index
                .and_then(|index| state.cache.remove(index))
                .map(|entry| {
                    let image = entry.image.clone();
                    state.cache.push_back(entry);
                    image
                })
        };
        let image = match cached {
            Some(image) => image,
            None => {
                let mut input = Vec::new();
                input
                    .try_reserve_exact(stamp.bytes as usize)
                    .map_err(|_| AssetError::ResourceLimit)?;
                input.resize(stamp.bytes as usize, 0);
                let mut reader = &file;
                reader.read_exact(&mut input).map_err(io_error)?;
                if reader.read(&mut [0_u8; 1]).map_err(io_error)? != 0 {
                    return Err(AssetError::FileChanged);
                }
                png::decode(&input)?
            }
        };
        // 不是文件系统CAS：拒绝观测到的路径替换、符号链接及读期间变化。
        validate_file(&path)?;
        if stamp != FileStamp::new(&file.metadata().map_err(io_error)?)?
            || stamp != FileStamp::new(&std::fs::metadata(&path).map_err(io_error)?)?
            || identity != same_file::Handle::from_path(&path).map_err(io_error)?
        {
            return Err(AssetError::FileChanged);
        }
        Ok(PreparedThumbnail {
            request,
            path,
            stamp,
            image,
        })
    }

    /// 调用方先在会话锁域复查权威行，随后仅提交最多64KiB的内存结果。
    pub(crate) fn finish(
        &self,
        permit: &Permit,
        entry: PreparedThumbnail,
    ) -> Result<ThumbnailDto, AssetError> {
        let mut state = self.state.lock().map_err(|_| AssetError::ServiceFault)?;
        Self::current(&state, permit)?;
        if let Some(index) = state
            .cache
            .iter()
            .position(|cached| cached.request == entry.request)
            && let Some(old) = state.cache.remove(index)
        {
            state.cache_bytes -= old.image.png.len();
        }
        while state.cache.len() >= MAX_CACHE_ENTRIES
            || state.cache_bytes + entry.image.png.len() > MAX_CACHE_BYTES
        {
            let Some(old) = state.cache.pop_front() else {
                return Err(AssetError::ResourceLimit);
            };
            state.cache_bytes -= old.image.png.len();
        }
        let image = entry.image.clone();
        state.cache_bytes += entry.image.png.len();
        state.cache.push_back(entry);
        Ok(image)
    }
}

/// 只解析已结束的行；重试准备/清除/退出以及旧attempt均不能读到新任务的文件。
pub(crate) fn resolve(
    snapshot: &TaskSnapshot,
    request: &JobAssetRequest,
    target: Option<RevealTarget>,
) -> Result<PathBuf, AssetError> {
    if snapshot.selection.map(|id| id.get()) != Some(request.selection_id.0)
        || !matches!(snapshot.phase, TaskPhase::Running | TaskPhase::Finished)
    {
        return Err(AssetError::StaleTask);
    }
    let job = snapshot
        .batch
        .as_ref()
        .and_then(|batch| {
            batch
                .jobs
                .iter()
                .find(|job| job.id.get() == request.job_id as usize)
        })
        .ok_or(AssetError::StaleTask)?;
    let state = match job.state {
        JobState::Succeeded(_) => AssetState::Succeeded,
        JobState::NoGain(_) => AssetState::NoGain,
        JobState::Failed(_) => AssetState::Failed,
        JobState::Cancelled => AssetState::Cancelled,
        _ => return Err(AssetError::Unavailable),
    };
    if job.attempt != request.attempt || state != request.expected_state {
        return Err(AssetError::StaleTask);
    }
    match (&job.state, target) {
        (JobState::Succeeded(report), target) => match (&report.outcome, target) {
            (
                ProcessingOutcome::Optimized {
                    backup: Some(backup),
                    ..
                },
                Some(RevealTarget::Backup),
            ) => Ok(backup.clone()),
            (ProcessingOutcome::Optimized { output, .. }, None | Some(RevealTarget::Result)) => {
                Ok(output.clone())
            }
            _ => Err(AssetError::Unavailable),
        },
        (JobState::NoGain(_), None | Some(RevealTarget::Result)) => Ok(job.request.source.clone()),
        (JobState::Failed(failure), Some(RevealTarget::Backup)) => {
            let mut cause = failure.cause.as_deref();
            while let Some(error) = cause {
                match error {
                    ProcessingError::CommitFailed { backup, .. } => return Ok(backup.clone()),
                    ProcessingError::CleanupFailed { original, .. } => cause = original.as_deref(),
                    _ => break,
                }
            }
            Err(AssetError::Unavailable)
        }
        (JobState::Failed(_) | JobState::Cancelled, None) => Ok(job.request.source.clone()),
        _ => Err(AssetError::Unavailable),
    }
}

pub(crate) fn validate_file(path: &Path) -> Result<(), AssetError> {
    if !path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return Err(AssetError::UnsafePath);
    }
    for (index, ancestor) in path.ancestors().enumerate() {
        let metadata = std::fs::symlink_metadata(ancestor).map_err(io_error)?;
        if metadata.file_type().is_symlink()
            || (index == 0 && !metadata.is_file())
            || (index > 0 && !metadata.is_dir())
        {
            return Err(AssetError::UnsafePath);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(AssetError::UnsafePath);
            }
        }
    }
    Ok(())
}
fn io_error(error: std::io::Error) -> AssetError {
    if error.kind() == std::io::ErrorKind::NotFound {
        AssetError::FileMissing
    } else {
        AssetError::Unavailable
    }
}
