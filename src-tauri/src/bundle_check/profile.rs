//! 隔离照片/大图验收：复用桌面资源配置和TaskRuntime，采样宿主与直接helper工作集。
//! 仅bundle-check特性启用；输出必须是新目录，输入不覆盖，不启动GUI。

use super::{Result, require};
use crate::{
    resources,
    tasks::{TaskControl, TaskError, TaskPhase, TaskRuntime, TaskSettings, TaskSnapshot},
};
use pixofold_core::{
    batch::{BatchParameters, ImageEngines, ImageKind, ImageReport, JobState},
    import::{CopyLayout, ImportIssueKind, ImportOutput},
    jpeg::{JpegEngine, JpegProcessing},
    model::{OutputDirectory, PngMode},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

const INTERVAL: Duration = Duration::from_millis(50);
const DEADLINE: Duration = Duration::from_secs(300);

#[derive(Default)]
struct Samples {
    system: System,
    count: usize,
    active: usize,
    reserved: u64,
    parent_rss: u64,
    helper_rss: u64,
    combined_rss: u64,
    helpers: usize,
    largest_gap: Duration,
    sampled_at: Option<Instant>,
}
impl Samples {
    fn sample(&mut self, snapshot: &TaskSnapshot, force: bool) -> Result<()> {
        if let Some(batch) = &snapshot.batch {
            self.active = self.active.max(batch.active_workers);
            self.reserved = self.reserved.max(batch.reserved_working_bytes.0);
        }
        if !force && self.sampled_at.is_some_and(|at| at.elapsed() < INTERVAL) {
            return Ok(());
        }
        let at = Instant::now();
        if let Some(previous) = self.sampled_at {
            self.largest_gap = self.largest_gap.max(at.duration_since(previous));
        }
        self.sampled_at = Some(at);
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_memory(),
        );
        let pid = sysinfo::get_current_pid()?;
        let parent = self
            .system
            .process(pid)
            .ok_or("无法读取宿主工作集")?
            .memory();
        let mut helper = 0_u64;
        let mut count = 0;
        for process in self.system.processes().values() {
            if process.parent() == Some(pid)
                && process.name().to_str().is_some_and(|name| {
                    name == "pixofold-jpeg-helper" || name == "pixofold-jpeg-helper.exe"
                })
            {
                helper = helper
                    .checked_add(process.memory())
                    .ok_or("helper工作集溢出")?;
                count += 1;
            }
        }
        self.count += 1;
        self.parent_rss = self.parent_rss.max(parent);
        self.helper_rss = self.helper_rss.max(helper);
        self.combined_rss = self
            .combined_rss
            .max(parent.checked_add(helper).ok_or("总工作集溢出")?);
        self.helpers = self.helpers.max(count);
        Ok(())
    }

    fn wait(&mut self, control: &TaskControl, expected: TaskPhase) -> Result<TaskSnapshot> {
        let started = Instant::now();
        loop {
            let snapshot = control.snapshot();
            self.sample(&snapshot, snapshot.phase == expected)?;
            if snapshot.phase == expected {
                return Ok(snapshot);
            }
            require(started.elapsed() < DEADLINE, "验收等待超时")?;
            require(
                !matches!(snapshot.phase, TaskPhase::Rejected | TaskPhase::Closed),
                "验收任务意外拒绝或关闭",
            )?;
            match control.wait_for_change(snapshot.revision, INTERVAL) {
                Ok(_) | Err(TaskError::TimedOut) => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
}

fn name(path: &Path) -> Result<&str> {
    path.file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "验收文件名无效".into())
}

/// 使用实际桌面配置处理调用方准备的隔离语料，并返回终态、扫描拒绝及采样指标。
/// 输入仅扫描/读取；root必须不存在，所有提交使用该目录下的副本策略。
/// 工作集是离散采样，包含宿主及当时可见的直接JPEG helper，不包含WebView/GUI。
/// # Errors
/// 工具身份、目录、任务/采样失败或预算/终态不变量被破坏时返回错误；Drop安全收尾。
pub fn profile(engine: JpegEngine, inputs: &Path, root: &Path, mode: PngMode) -> Result<Value> {
    fs::create_dir(root)?;
    let outputs = root.join("results");
    fs::create_dir(&outputs)?;
    let config = resources::task_config();
    let mut runtime = TaskRuntime::with_engines(config, ImageEngines::with_jpeg(engine))?;
    let control = runtime.control();
    let started = Instant::now();
    let mut samples = Samples::default();
    let selection = control.import(vec![inputs.to_owned()], None)?;
    let ready = samples.wait(&control, TaskPhase::Ready)?;
    let scan = ready.import.as_ref().ok_or("缺少扫描结果")?;
    require(!scan.files().is_empty(), "无可处理验收样本")?;
    let scan_issues = scan
        .issues()
        .iter()
        .map(|issue| {
            let code = match &issue.kind {
                ImportIssueKind::Failure(failure) => format!("{:?}", failure.code),
                ImportIssueKind::Unsupported(_) => "UnsupportedFormat".to_owned(),
                ImportIssueKind::Duplicate { .. } => "Duplicate".to_owned(),
                ImportIssueKind::GeneratedArtifact => "GeneratedArtifact".to_owned(),
            };
            Ok(json!({"file": name(&issue.path)?, "code": code}))
        })
        .collect::<Result<Vec<_>>>()?;
    let scan_ms = started.elapsed().as_millis();
    control.start(
        selection,
        TaskSettings {
            parameters: BatchParameters {
                mode,
                ..BatchParameters::default()
            },
            output: ImportOutput::CopyToAuthorized {
                directory: OutputDirectory::open(&outputs)?,
                layout: CopyLayout::Flat,
            },
        },
    )?;
    let processing = Instant::now();
    let done = samples.wait(&control, TaskPhase::Finished)?;
    let batch = done.batch.as_ref().ok_or("缺少完成批次")?;
    require(batch.summary.terminal == batch.summary.total, "终态未闭合")?;
    require(
        batch.active_workers == 0 && batch.reserved_working_bytes.0 == 0,
        "资源未释放",
    )?;
    require(
        samples.active <= config.batch.workers,
        "活动数超过固定worker池",
    )?;
    require(
        samples.reserved <= config.batch.working_set_budget.0,
        "预约超过桌面预算",
    )?;
    let jobs = batch
        .jobs
        .iter()
        .map(|job| {
            let (state, report, error) = match &job.state {
                JobState::Succeeded(report) => ("succeeded", Some(report), None),
                JobState::NoGain(report) => ("no_gain", Some(report), None),
                JobState::Failed(failure) => ("failed", None, Some(format!("{:?}", failure.code))),
                JobState::Cancelled => ("cancelled", None, None),
                _ => return Err("存在非终态行".into()),
            };
            let lossless = report.is_some_and(|report| match report {
                ImageReport::Jpeg(report) => {
                    !matches!(report.processing, JpegProcessing::Lossy { .. })
                }
                ImageReport::Png(_) => mode == PngMode::Lossless,
            });
            Ok(json!({
                "file": name(&job.request.source)?,
                "format": if job.request.format() == ImageKind::Jpeg { "jpeg" } else { "png" },
                "state": state, "errorCode": error, "lossless": lossless,
                "inputBytes": job.input_bytes.map(|bytes| bytes.0),
                "outputBytes": report.map(ImageReport::output_bytes).map(|bytes| bytes.0),
            }))
        })
        .collect::<Result<Vec<_>>>()?;
    let processing_ms = processing.elapsed().as_millis();
    runtime.shutdown()?;
    samples.sample(&control.snapshot(), true)?;
    let report = json!({
        "result": "completed", "configuredWorkers": config.batch.workers,
        "workingSetBudgetBytes": config.batch.working_set_budget.0,
        "scanMs": scan_ms, "processingMs": processing_ms, "scanIssues": scan_issues, "jobs": jobs,
        "succeeded": batch.summary.succeeded, "noGain": batch.summary.no_gain,
        "failed": batch.summary.failed, "cancelled": batch.summary.cancelled,
        "sampleIntervalMs": INTERVAL.as_millis(), "samples": samples.count,
        "sampledMaxGapMs": samples.largest_gap.as_millis(),
        "sampledPeakActive": samples.active, "sampledPeakReservedBytes": samples.reserved,
        "sampledPeakParentRssBytes": samples.parent_rss,
        "sampledPeakHelperRssBytes": samples.helper_rss,
        "sampledPeakCombinedRssBytes": samples.combined_rss,
        "sampledPeakHelpers": samples.helpers, "shutdown": "completed",
    });
    fs::write(
        root.join("profile.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}
