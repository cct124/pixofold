//! 有界深度优先扫描；压缩字节一次只持有一个文件，不解压像素，不写文件。
//! 保留身份句柄用于去重，全部在返回前关闭；取消在条目和64KiB读取边界协作检查。

use super::model::*;
use crate::{
    batch::JobFailure,
    model::{ByteCount, CancellationToken, ProcessingError},
    output::{self, paths},
    probe,
};
use std::{
    collections::HashMap,
    ffi::OsString,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

const READ_CHUNK: usize = 64 * 1024;
const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

struct Pending {
    path: PathBuf,
    // 子项携带首次根及相对路径，避免依赖字符串剥离或文件系统大小写形式。
    root: Option<PathBuf>,
    relative: PathBuf,
    depth: usize,
}

/// 只读识别文件/目录列表；根顺序保留，目录内排序，重复项以首次归属为准。
/// 空列表（例如取消对话框）返回完整空结果；单项错误仅写入issues。
/// 取消/资源上限返回可展示的部分结果，但ImportScan::plan拒绝启动。
/// # Errors
/// 配置非法或显式根数过多在扫描前返回错误，不读取图片。OS调用本身不能强制中断。
pub fn scan(
    roots: &[PathBuf],
    options: ScanOptions,
    cancel: &CancellationToken,
    on_progress: impl FnMut(&ScanProgress),
) -> Result<ImportScan, ImportError> {
    if !(1..=100_000).contains(&options.max_files)
        || !(1..=1_000_000).contains(&options.max_entries)
        || options.max_depth > 256
        || options.max_read_bytes.0 == 0
        || options.probe_limits.validate().is_err()
    {
        return Err(ImportError::InvalidOptions);
    }
    if roots.len() > ScanOptions::MAX_ROOTS || roots.len() > options.max_entries {
        return Err(ImportError::TooManyRoots);
    }
    let mut scanner = Scanner {
        report: ImportScan {
            files: Vec::new(),
            issues: Vec::new(),
            progress: ScanProgress {
                status: ScanStatus::Scanning,
                discovered: roots.len(),
                examined: 0,
                accepted: 0,
                duplicates: 0,
                excluded: 0,
                rejected: 0,
                read_bytes: ByteCount(0),
            },
        },
        options,
        cancel,
        on_progress,
        visited: HashMap::new(),
        identities: HashMap::new(),
    };
    let mut pending: Vec<_> = roots
        .iter()
        .rev()
        .map(|path| Pending {
            path: path.clone(),
            root: None,
            relative: PathBuf::new(),
            depth: 0,
        })
        .collect();
    scanner.notify();
    while scanner.running() {
        let Some(entry) = pending.pop() else {
            break;
        };
        scanner.report.progress.examined += 1;
        let path = entry.path.clone();
        if let Err(error) = scanner.visit(entry, &mut pending) {
            scanner.issue(
                path,
                ImportIssueKind::Failure(JobFailure::processing(error)),
            );
        }
        scanner.notify();
    }
    if scanner.running() {
        scanner.report.progress.status = ScanStatus::Complete;
    }
    scanner.notify();
    // report无句柄；Scanner其余字段在返回前释放，不能阻止Windows后续覆盖。
    Ok(scanner.report)
}

struct Scanner<'a, F> {
    report: ImportScan,
    options: ScanOptions,
    cancel: &'a CancellationToken,
    on_progress: F,
    visited: HashMap<OsString, PathBuf>,
    identities: HashMap<same_file::Handle, PathBuf>,
}
impl<F: FnMut(&ScanProgress)> Scanner<'_, F> {
    fn running(&mut self) -> bool {
        if self.report.progress.status == ScanStatus::Scanning && self.cancel.is_cancelled() {
            self.report.progress.status = ScanStatus::Cancelled;
        }
        self.report.progress.status == ScanStatus::Scanning
    }
    fn notify(&mut self) {
        (self.on_progress)(&self.report.progress);
    }
    fn limit(&mut self, limit: ScanLimit) {
        self.report.progress.status = ScanStatus::Limited(limit);
    }
    fn issue(&mut self, path: PathBuf, kind: ImportIssueKind) {
        match &kind {
            ImportIssueKind::Duplicate { .. } => self.report.progress.duplicates += 1,
            ImportIssueKind::GeneratedArtifact => self.report.progress.excluded += 1,
            _ => self.report.progress.rejected += 1,
        }
        self.report.issues.push(ImportIssue { path, kind });
    }

    fn visit(
        &mut self,
        mut entry: Pending,
        pending: &mut Vec<Pending>,
    ) -> Result<(), ProcessingError> {
        entry.path = paths::entry_path(&entry.path)?;
        let metadata = fs::symlink_metadata(&entry.path)
            .map_err(|e| ProcessingError::io("读取导入条目", e))?;
        if paths::is_link(&metadata) || (!metadata.is_dir() && !metadata.is_file()) {
            return Err(ProcessingError::InvalidPath);
        }
        let path = if metadata.is_dir() {
            paths::directory(&entry.path)?
        } else {
            output::absolute_leaf(&entry.path)?
        };
        if let Some(first) = self.visited.get(&paths::key(&path)) {
            self.issue(
                path,
                ImportIssueKind::Duplicate {
                    first: first.clone(),
                },
            );
            return Ok(());
        }
        self.visited.insert(paths::key(&path), path.clone());
        if metadata.is_dir() {
            if entry.depth > self.options.max_depth {
                self.limit(ScanLimit::Depth);
                return Ok(());
            }
            let root = entry.root.unwrap_or_else(|| path.clone());
            let children =
                fs::read_dir(&path).map_err(|e| ProcessingError::io("枚举导入目录", e))?;
            let mut entries = Vec::new();
            for child in children {
                if !self.running() {
                    return Ok(());
                }
                if self.report.progress.discovered == self.options.max_entries {
                    self.limit(ScanLimit::Entries);
                    return Ok(());
                }
                self.report.progress.discovered += 1;
                match child {
                    Ok(child) => entries.push(Pending {
                        path: child.path(),
                        root: Some(root.clone()),
                        relative: entry.relative.join(child.file_name()),
                        depth: entry.depth + 1,
                    }),
                    Err(e) => self.issue(
                        path.clone(),
                        ImportIssueKind::Failure(JobFailure::processing(ProcessingError::io(
                            "读取目录子项",
                            e,
                        ))),
                    ),
                }
                self.notify();
            }
            entries.sort_by(|a, b| a.path.cmp(&b.path));
            pending.extend(entries.into_iter().rev());
            return Ok(());
        }
        if !self.options.include_artifacts && paths::is_artifact(&path) {
            self.issue(path, ImportIssueKind::GeneratedArtifact);
            return Ok(());
        }
        output::regular_metadata(&path)?;
        let file = File::open(&path).map_err(|e| ProcessingError::io("打开导入文件", e))?;
        let mut identity = same_file::Handle::from_file(file)
            .map_err(|e| ProcessingError::io("读取导入文件身份", e))?;
        if let Some(first) = self.identities.get(&identity) {
            self.issue(
                path,
                ImportIssueKind::Duplicate {
                    first: first.clone(),
                },
            );
            return Ok(());
        }
        let Some(bytes) = self.read_png(identity.as_file_mut(), &path)? else {
            return Ok(());
        };
        if !self.running() {
            return Ok(());
        }
        let image = probe::inspect_structure(&bytes, self.options.probe_limits)?;
        if !self.running() {
            return Ok(());
        }
        output::regular_metadata(&path)?;
        if identity
            != same_file::Handle::from_path(&path)
                .map_err(|e| ProcessingError::io("复查导入路径身份", e))?
        {
            return Err(ProcessingError::SourceChanged);
        }
        if self.report.files.len() == self.options.max_files {
            self.limit(ScanLimit::Files);
            return Ok(());
        }
        let is_directory = entry.root.is_some();
        let relative = if is_directory {
            entry.relative
        } else {
            PathBuf::from(path.file_name().ok_or(ProcessingError::InvalidPath)?)
        };
        self.report.files.push(ImportedFile {
            source: path.clone(),
            root: entry.root.unwrap_or_else(|| path.clone()),
            root_is_directory: is_directory,
            relative_path: relative,
            input_bytes: ByteCount(bytes.len() as u64),
            image,
        });
        self.identities.insert(identity, path);
        self.report.progress.accepted += 1;
        Ok(())
    }

    fn read_png(
        &mut self,
        file: &mut File,
        path: &Path,
    ) -> Result<Option<Vec<u8>>, ProcessingError> {
        let before = file
            .metadata()
            .map_err(|e| ProcessingError::io("读取导入文件属性", e))?;
        if !before.is_file() {
            return Err(ProcessingError::InvalidPath);
        }
        if before.len() > self.options.probe_limits.max_input_bytes.0 {
            return Err(ProcessingError::ResourceLimit("导入单文件字节数"));
        }
        let modified = before
            .modified()
            .map_err(|e| ProcessingError::io("读取导入修改时间", e))?;
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; READ_CHUNK];
        // 先读签名，非PNG不耗费整文件预算；PNG再逐块读取，避免扫描内持有解码像素。
        let mut remaining = before.len();
        let mut first = true;
        while remaining > 0 {
            if !self.running() {
                return Ok(None);
            }
            let allowance = self.options.max_read_bytes.0 - self.report.progress.read_bytes.0;
            if allowance == 0 {
                self.limit(ScanLimit::ReadBytes);
                return Ok(None);
            }
            let count = remaining
                .min(allowance)
                .min(if first { 12 } else { READ_CHUNK as u64 }) as usize;
            let read = match file.read(&mut buffer[..count]) {
                Ok(0) => return Err(ProcessingError::SourceChanged),
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(ProcessingError::io("读取导入数据", e)),
            };
            bytes
                .try_reserve(read)
                .map_err(|_| ProcessingError::ResourceLimit("导入压缩数据缓冲"))?;
            bytes.extend_from_slice(&buffer[..read]);
            remaining -= read as u64;
            self.report.progress.read_bytes.0 += read as u64;
            self.notify();
            // Read可能短读；拿到完整前缀（或EOF）再分类，不能把短读当坏签名。
            if first && (bytes.len() >= 12 || remaining == 0) {
                first = false;
                if !bytes.starts_with(SIGNATURE) {
                    let format = if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
                        UnsupportedFormat::Jpeg
                    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
                        UnsupportedFormat::Gif
                    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
                        UnsupportedFormat::WebP
                    } else {
                        UnsupportedFormat::Other
                    };
                    self.issue(path.to_owned(), ImportIssueKind::Unsupported(format));
                    return Ok(None);
                }
            }
        }
        if bytes.is_empty() {
            self.issue(
                path.to_owned(),
                ImportIssueKind::Unsupported(UnsupportedFormat::Other),
            );
            return Ok(None);
        }
        let after = file
            .metadata()
            .map_err(|e| ProcessingError::io("复查导入属性", e))?;
        if after.len() != before.len()
            || after
                .modified()
                .map_err(|e| ProcessingError::io("复查导入修改时间", e))?
                != modified
        {
            return Err(ProcessingError::SourceChanged);
        }
        Ok(Some(bytes))
    }
}
