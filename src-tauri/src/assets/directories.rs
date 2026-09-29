//! 只从完成批次的成功输出推导目录；纯路径分组不做I/O，打开前才检查目录及授权身份。
use super::*;
use pixofold_core::{
    batch::{BatchPhase, BatchSnapshot, JobSnapshot},
    model::{OutputDirectory, OutputPolicy},
};
use std::collections::HashMap;

pub(crate) struct DirectoryTarget {
    pub path: PathBuf,
    authority: Option<OutputDirectory>,
}
impl DirectoryTarget {
    pub fn validate(&self) -> Result<(), AssetError> {
        validate_path(&self.path, true)?;
        if let Some(root) = &self.authority {
            root.verify().map_err(|_| AssetError::FileChanged)?;
        }
        Ok(())
    }
}

pub(crate) fn validate_batch<'a>(
    snapshot: &'a TaskSnapshot,
    request: &BatchAssetRequest,
) -> Result<&'a BatchSnapshot, AssetError> {
    let batch = snapshot.batch.as_deref().ok_or(AssetError::StaleTask)?;
    if snapshot.phase != TaskPhase::Finished
        || snapshot.selection.map(|id| id.get()) != Some(request.selection_id.0)
        || batch.id.get() != request.batch_id.0
        || batch.revision != request.batch_revision.0
        || batch.phase != BatchPhase::Finished
        || batch.jobs.iter().any(|job| !job.state.is_terminal())
    {
        return Err(AssetError::StaleTask);
    }
    Ok(batch)
}

fn target(job: &JobSnapshot) -> Result<DirectoryTarget, AssetError> {
    let JobState::Succeeded(report) = &job.state else {
        return Err(AssetError::Unavailable);
    };
    let ProcessingOutcome::Optimized { output, .. } = &report.outcome else {
        return Err(AssetError::Unavailable);
    };
    let (path, authority) = match &job.request.output {
        OutputPolicy::CopyTreeAuthorized { root, relative }
            if root.path().join(relative) == *output =>
        {
            (root.path().to_path_buf(), Some(root.clone()))
        }
        OutputPolicy::CopyTree { root, relative } if root.join(relative) == *output => {
            (root.clone(), None)
        }
        OutputPolicy::CopyTreeAuthorized { .. } | OutputPolicy::CopyTree { .. } => {
            return Err(AssetError::StaleTask);
        }
        _ => (
            output
                .parent()
                .ok_or(AssetError::Unavailable)?
                .to_path_buf(),
            None,
        ),
    };
    Ok(DirectoryTarget { path, authority })
}

pub(crate) fn resolve_directory(
    snapshot: &TaskSnapshot,
    request: &OpenOutputDirectoryRequest,
) -> Result<DirectoryTarget, AssetError> {
    let batch = validate_batch(snapshot, &request.batch)?;
    let job = batch
        .jobs
        .iter()
        .find(|job| job.id.get() == request.job_id as usize)
        .ok_or(AssetError::StaleTask)?;
    target(job)
}

pub(crate) fn directory_page(
    snapshot: &TaskSnapshot,
    request: &OutputDirectoriesRequest,
) -> Result<OutputDirectoryPage, AssetError> {
    let batch = validate_batch(snapshot, &request.batch)?;
    let mut index = HashMap::<PathBuf, usize>::new();
    let mut entries: Vec<(&JobSnapshot, u32)> = Vec::new();
    for job in &batch.jobs {
        if !matches!(job.state, JobState::Succeeded(_)) {
            continue;
        }
        let directory = target(job)?;
        if let Some(&slot) = index.get(&directory.path) {
            entries[slot].1 += 1;
        } else {
            index.insert(directory.path, entries.len());
            entries.push((job, 1));
        }
    }
    let total = u32::try_from(entries.len()).map_err(|_| AssetError::ResourceLimit)?;
    if request.offset > total {
        return Err(AssetError::StaleTask);
    }
    Ok(OutputDirectoryPage {
        total,
        offset: request.offset,
        items: entries
            .into_iter()
            .skip(request.offset as usize)
            .take(OUTPUT_DIRECTORY_PAGE_SIZE)
            .map(|(job, result_count)| {
                Ok(OutputDirectoryDto {
                    job_id: u32::try_from(job.id.get()).map_err(|_| AssetError::ResourceLimit)?,
                    name: crate::ipc::display_name(&target(job)?.path),
                    example_name: crate::ipc::display_name(&job.request.source),
                    result_count,
                })
            })
            .collect::<Result<_, AssetError>>()?,
    })
}
