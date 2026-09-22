//! 完整导入结果到显式BatchRequest的只读规划；不创建输出目录，不绕过批次预检。

use super::model::*;
use crate::{
    batch::{self, BatchItem, BatchParameters, BatchRequest, JobFailure, JobState},
    model::{OutputPolicy, PngRequest, ProcessingError},
    output::paths,
};
use std::{
    collections::HashMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};

fn root_name(root: &Path) -> &OsStr {
    root.file_name().unwrap_or_else(|| OsStr::new("_root"))
}

fn copy_name(source: &Path) -> Result<PathBuf, ProcessingError> {
    let mut stem = source
        .file_stem()
        .ok_or(ProcessingError::InvalidPath)?
        .to_os_string();
    stem.push("_compressed.png");
    Ok(PathBuf::from(stem))
}

impl ImportScan {
    /// 冻结当前设置并只读预检全部目标。失败时self不变，可修正设置后重试规划。
    /// 此处不启动批次/创建目录；返回请求仍须交给BatchService::start进行准入复查。
    /// # Errors
    /// 未完整扫描、无候选、无效资源配置、重名/源目标交叉或文件系统错误均拒绝规划。
    pub fn plan(
        &self,
        output: &ImportOutput,
        parameters: BatchParameters,
    ) -> Result<BatchRequest, ImportError> {
        if self.progress.status != ScanStatus::Complete {
            return Err(ImportError::IncompleteScan);
        }
        if self.files.is_empty() {
            return Err(ImportError::NoFiles);
        }
        batch::estimate_working_set(parameters).map_err(ImportError::Batch)?;
        if matches!(
            output,
            ImportOutput::CopyTo {
                layout: CopyLayout::PreserveRoots,
                ..
            }
        ) {
            let mut names = HashMap::new();
            for file in self.files.iter().filter(|f| f.root_is_directory) {
                if let Some(first) =
                    names.insert(paths::key(Path::new(root_name(&file.root))), &file.root)
                    && paths::key(first) != paths::key(&file.root)
                {
                    return Err(ImportError::RootNameConflict {
                        first: first.clone(),
                        second: file.root.clone(),
                    });
                }
            }
        }
        let mut requests = Vec::with_capacity(self.files.len());
        for (index, file) in self.files.iter().enumerate() {
            let policy = (|| -> Result<OutputPolicy, ProcessingError> {
                Ok(match output {
                    ImportOutput::Overwrite => OutputPolicy::Overwrite,
                    ImportOutput::CopyBeside => OutputPolicy::Copy {
                        destination: file.source.with_file_name(copy_name(&file.source)?),
                    },
                    ImportOutput::CopyTo { directory, layout } => {
                        let name = copy_name(&file.source)?;
                        let relative =
                            if *layout == CopyLayout::PreserveRoots && file.root_is_directory {
                                let parent = file
                                    .relative_path
                                    .parent()
                                    .ok_or(ProcessingError::InvalidPath)?;
                                PathBuf::from(root_name(&file.root)).join(parent).join(name)
                            } else {
                                name
                            };
                        OutputPolicy::CopyTree {
                            root: directory.clone(),
                            relative,
                        }
                    }
                })
            })()
            .map_err(|e| ImportError::File {
                index,
                failure: JobFailure::processing(e),
            })?;
            requests.push(PngRequest {
                source: file.source.clone(),
                output: policy,
                mode: parameters.mode,
                limits: parameters.limits,
            });
        }
        let jobs = batch::preview(requests).map_err(ImportError::Batch)?;
        let mut items = Vec::with_capacity(jobs.len());
        for (index, job) in jobs.into_iter().enumerate() {
            if let JobState::Failed(failure) = job.state {
                return Err(ImportError::File { index, failure });
            }
            items.push(BatchItem {
                source: job.request.source,
                output: job.request.output,
            });
        }
        Ok(BatchRequest { items, parameters })
    }
}
