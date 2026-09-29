//! 完整导入结果到显式BatchRequest的只读规划；不创建输出目录，不绕过批次预检。

use super::model::*;
use crate::{
    batch::{self, BatchItem, BatchParameters, BatchRequest, JobFailure},
    model::{OutputPolicy, PngRequest, ProcessingError},
};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

fn root_name(root: &Path) -> &OsStr {
    root.file_name().unwrap_or_else(|| OsStr::new("_root"))
}

fn copy_name(source: &Path) -> Result<PathBuf, ProcessingError> {
    source
        .file_name()
        .map(PathBuf::from)
        .ok_or(ProcessingError::InvalidPath)
}

/// 生成同目录副本策略，不读写文件；目标存在/别名冲突仍由批次和输出层检查。
/// 副本保留原名，同目录会与原图冲突；不能回退为覆盖或自动改名。
/// # Errors
/// 源没有有效文件名时返回InvalidPath。
pub fn copy_beside(source: &Path) -> Result<OutputPolicy, ProcessingError> {
    Ok(OutputPolicy::Copy {
        destination: source.with_file_name(copy_name(source)?),
    })
}

impl ImportOutput {
    /// 由Rust保存的导入来源规划目标，不执行I/O；初次导入和选中行确认共用布局规则。
    /// 调用方仍须交给BatchService进行完整冲突检查，不能直接提交输出。
    /// # Errors
    /// 无有效文件名时拒绝，返回输入切片中的候选索引。
    pub fn outputs_for(&self, files: &[&ImportedFile]) -> Result<Vec<OutputPolicy>, ImportError> {
        // 同名目录根可合并布局，实际文件冲突由批次逐项标记，不能连带拒绝无冲突图片。
        let mut outputs = Vec::with_capacity(files.len());
        for (index, file) in files.iter().enumerate() {
            let policy = (|| -> Result<OutputPolicy, ProcessingError> {
                Ok(match self {
                    ImportOutput::Overwrite => OutputPolicy::Overwrite,
                    ImportOutput::OverwriteWithoutBackup => OutputPolicy::OverwriteWithoutBackup,
                    ImportOutput::CopyBeside => copy_beside(&file.source)?,
                    ImportOutput::CopyTo { layout, .. }
                    | ImportOutput::CopyToAuthorized { layout, .. } => {
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
                        match self {
                            ImportOutput::CopyTo { directory, .. } => OutputPolicy::CopyTree {
                                root: directory.clone(),
                                relative,
                            },
                            ImportOutput::CopyToAuthorized { directory, .. } => {
                                OutputPolicy::CopyTreeAuthorized {
                                    root: directory.clone(),
                                    relative,
                                }
                            }
                            _ => return Err(ProcessingError::InvalidPath),
                        }
                    }
                })
            })()
            .map_err(|e| ImportError::File {
                index,
                failure: JobFailure::processing(e),
            })?;
            outputs.push(policy);
        }
        Ok(outputs)
    }
}

impl ImportScan {
    /// 冻结当前设置并只读预检全部目标，不启动任务或创建目录。失败时清单不变。
    /// 返回请求仍须交给BatchService::start进行准入复查。
    /// # Errors
    /// 未完整扫描、无候选、无效参数或重复源等整批准入错误拒绝。
    /// 单项文件/目标错误保留在请求中，由BatchService启动时复查并记录该行失败。
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
        let outputs = output.outputs_for(&self.files.iter().collect::<Vec<_>>())?;
        let mut requests = Vec::with_capacity(self.files.len());
        for (file, policy) in self.files.iter().zip(outputs) {
            requests.push(PngRequest {
                source: file.source.clone(),
                output: policy,
                mode: parameters.mode,
                limits: parameters.limits,
                metadata: Default::default(),
            });
        }
        let jobs = batch::preview(requests).map_err(ImportError::Batch)?;
        let mut items = Vec::with_capacity(jobs.len());
        for job in jobs {
            items.push(BatchItem {
                source: job.request.source,
                output: job.request.output,
            });
        }
        Ok(BatchRequest { items, parameters })
    }
}
