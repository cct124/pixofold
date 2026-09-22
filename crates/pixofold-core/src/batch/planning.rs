//! 整批只读准入：复用输出层路径边界，检查跨任务冲突，不持有源句柄进入编码。

use std::{collections::HashMap, ffi::OsString, fs};

use super::{Job, model::*};
use crate::output::paths::key;
use crate::{
    model::{ByteCount, OutputPolicy, PngMode, PngRequest, ProcessingError},
    output, quality,
};

/// 按请求上限保守计费，防止排队后图片变大绕过按旧文件头估计的预算。
/// 8份输入覆盖候选/实际回读/源与备份复查；8份解码覆盖两轮验证及编码临时缓冲。
/// 有损另留每像素128 bytes给RGBA、索引和编码器工作区；16 MiB为固定余量。
/// 这是准入估算，不是对第三方编码器内存上限的证明；默认仍保持单worker。
pub fn estimate_working_set(parameters: BatchParameters) -> Result<ByteCount, BatchError> {
    parameters
        .limits
        .validate()
        .map_err(BatchError::InvalidParameters)?;
    let limits = parameters.limits;
    let quantization = if matches!(parameters.mode, PngMode::Lossy { .. }) {
        limits.max_pixels.checked_mul(128)
    } else {
        Some(0)
    };
    limits
        .max_input_bytes
        .0
        .checked_mul(8)
        .zip(limits.max_decoded_bytes.0.checked_mul(8))
        .and_then(|(a, b)| a.checked_add(b))
        .zip(quantization)
        .and_then(|(a, b)| a.checked_add(b))
        .and_then(|n| n.checked_add(16 * 1024 * 1024))
        .map(ByteCount)
        .ok_or(BatchError::InvalidConfig)
}

struct Paths {
    source: Option<OsString>,
    target: Option<OsString>,
    identity: Option<same_file::Handle>,
    copy_identity: Option<same_file::Handle>,
    copy: bool,
}

pub(super) fn prepare(
    requests: Vec<PngRequest>,
    budget: ByteCount,
) -> Result<Vec<Job>, BatchError> {
    let mut paths = Vec::with_capacity(requests.len());
    let mut jobs = Vec::with_capacity(requests.len());
    for (i, mut request) in requests.into_iter().enumerate() {
        let mut failure = None;
        let mut identity = None;
        let mut size = None;
        let source = match output::absolute_leaf(&request.source) {
            Ok(path) => {
                request.source = path;
                match output::regular_metadata(&request.source).and_then(|metadata| {
                    size = Some(ByteCount(metadata.len()));
                    same_file::Handle::from_path(&request.source)
                        .map_err(|e| ProcessingError::io("读取批量源身份", e))
                }) {
                    Ok(handle) => identity = Some(handle),
                    Err(error) => failure = Some(JobFailure::processing(error)),
                }
                Some(key(&request.source))
            }
            Err(error) => {
                failure = Some(JobFailure::processing(error));
                None
            }
        };
        let mut copy_identity = None;
        let copy = !matches!(request.output, OutputPolicy::Overwrite);
        let target = match output::paths::copy_destination(&mut request.output) {
            Ok(Some(destination)) => {
                match fs::symlink_metadata(&destination) {
                    Ok(_) => {
                        // 已有目标无条件失败；身份仅补充硬链接诊断，读取失败也不会放行。
                        copy_identity = same_file::Handle::from_path(&destination).ok();
                        failure.get_or_insert_with(|| {
                            JobFailure::processing(ProcessingError::TargetConflict)
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => {
                        failure.get_or_insert_with(|| {
                            JobFailure::processing(ProcessingError::io("检查批量目标", e))
                        });
                    }
                }
                Some(key(&destination))
            }
            Ok(None) => source.clone(),
            Err(e) => {
                failure.get_or_insert_with(|| JobFailure::processing(e));
                None
            }
        };
        let parameters = BatchParameters {
            mode: request.mode,
            limits: request.limits,
        };
        let reservation = estimate_working_set(parameters)?;
        if reservation > budget {
            failure.get_or_insert_with(|| {
                JobFailure::processing(ProcessingError::ResourceLimit("批量工作集预算"))
            });
        }
        let mapping = match request.mode {
            PngMode::Lossless => None,
            PngMode::Lossy { quality: q } => Some(quality::png_quality(q)),
        };
        jobs.push(Job {
            view: JobSnapshot {
                id: JobId(i + 1),
                attempt: 1,
                request,
                mapping,
                input_bytes: size,
                state: failure.map_or(JobState::Queued, JobState::Failed),
            },
            reservation,
        });
        paths.push(Paths {
            source,
            target,
            identity,
            copy_identity,
            copy,
        });
    }
    // 身份句柄全部在函数返回前释放，避免Windows覆盖被本服务自己的预检句柄阻止。
    let mut sources = HashMap::new();
    let mut identities = HashMap::new();
    let mut targets = HashMap::new();
    let conflict = |a: usize, b: usize, kind| BatchError::PathConflict {
        first: JobId(a + 1),
        second: JobId(b + 1),
        kind,
    };
    for (i, path) in paths.iter().enumerate() {
        if let Some(source) = &path.source
            && let Some(first) = sources.insert(source, i)
        {
            return Err(conflict(first, i, PathConflictKind::DuplicateSource));
        }
        if let Some(identity) = &path.identity
            && let Some(first) = identities.insert(identity, i)
        {
            return Err(conflict(first, i, PathConflictKind::DuplicateSource));
        }
        if let Some(target) = &path.target
            && let Some(first) = targets.insert(target.clone(), i)
        {
            return Err(conflict(first, i, PathConflictKind::DuplicateOutput));
        }
    }
    for (i, path) in paths.iter().enumerate().filter(|(_, p)| p.copy) {
        // CopyTree尚不存在的目录也可能是另一项的最终文件，不能等worker争抢创建才发现。
        if let Some(target) = &path.target {
            for ancestor in std::path::Path::new(target).ancestors().skip(1) {
                if let Some(first) = targets.get(ancestor.as_os_str()) {
                    return Err(conflict(*first, i, PathConflictKind::OutputHierarchy));
                }
            }
        }
        let first = path
            .target
            .as_ref()
            .and_then(|p| sources.get(p))
            .copied()
            .or_else(|| {
                path.copy_identity
                    .as_ref()
                    .and_then(|p| identities.get(p))
                    .copied()
            });
        if let Some(first) = first {
            return Err(conflict(first, i, PathConflictKind::OutputIsInput));
        }
    }
    Ok(jobs)
}
