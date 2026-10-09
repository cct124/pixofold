//! 显式无GUI桌面验收；只编入开发检查feature，复用生产任务/DTO/展示服务。
//! 调用方提供自生成语料和全新隔离输出目录；不接收IPC、不启动窗口。

use crate::{
    assets::{
        self, AssetService, AssetState, BatchAssetRequest, JobAssetRequest, Operation,
        OutputDirectoriesRequest, RevealTarget,
    },
    ipc::{self, DecimalU64},
    tasks::{TaskConfig, TaskControl, TaskPhase, TaskRuntime, TaskSettings, TaskSnapshot},
};
use pixofold_core::{
    batch::{
        BatchParameters, ImageEngines, ImageKind, ImageReport, JobErrorCode, JobState, RetryJob,
        RetryRequest,
    },
    import::{CopyLayout, ImportOutput},
    jpeg::{self, JpegEngine, JpegLimits, JpegProcessing},
    model::{
        CancellationToken, OutputDirectory, PngMetadataPolicy, PngMode, ProcessingOutcome,
        QualityValue,
    },
};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn phase(control: &TaskControl, expected: TaskPhase) -> Result<TaskSnapshot> {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut snapshot = control.snapshot();
    while snapshot.phase != expected {
        require(
            !matches!(snapshot.phase, TaskPhase::Rejected | TaskPhase::Closed),
            "任务意外拒绝或关闭",
        )?;
        snapshot = control.wait_for_change(
            snapshot.revision,
            deadline.saturating_duration_since(Instant::now()),
        )?;
    }
    Ok(snapshot)
}
fn query(control: &TaskControl, collection: &str) -> Result<Value> {
    let request = serde_json::from_value(
        json!({"expectedRevision": null, "collection": collection, "offset": 0, "limit": 100}),
    )?;
    let value = ipc::query(control, request).map_err(|error| format!("DTO转换失败：{error:?}"))?;
    Ok(serde_json::to_value(value)?)
}
fn settings(output: ImportOutput, mode: PngMode) -> TaskSettings {
    TaskSettings {
        parameters: BatchParameters {
            mode,
            ..BatchParameters::default()
        },
        output,
    }
}
fn copy_to(path: &Path) -> Result<ImportOutput> {
    fs::create_dir(path)?;
    Ok(ImportOutput::CopyToAuthorized {
        directory: OutputDirectory::open(path)?,
        layout: CopyLayout::Flat,
    })
}
fn asset<T>(value: std::result::Result<T, assets::AssetError>) -> Result<T> {
    value.map_err(|error| format!("展示验收失败：{error:?}").into())
}
fn identity(
    snapshot: &TaskSnapshot,
    job: &pixofold_core::batch::JobSnapshot,
) -> Result<JobAssetRequest> {
    Ok(JobAssetRequest {
        subscription_id: DecimalU64(1),
        selection_id: DecimalU64(snapshot.selection.ok_or("缺少清单")?.get()),
        job_id: u32::try_from(job.id.get())?,
        attempt: job.attempt,
        expected_state: match job.state {
            JobState::Succeeded(_) => AssetState::Succeeded,
            JobState::NoGain(_) => AssetState::NoGain,
            JobState::Failed(_) => AssetState::Failed,
            JobState::Cancelled => AssetState::Cancelled,
            _ => return Err("缺少终态".into()),
        },
    })
}

/// 使用同一可信引擎验收真实混合导入、冻结重试、报告、备份/目录和有界预览。
/// # Errors
/// 语料缺失、任何运行/契约断言失败或I/O错误均返回错误；运行时Drop仍安全收尾。
pub fn verify(engine: JpegEngine, samples: &Path, root: &Path) -> Result<Value> {
    fs::create_dir(root)?;
    let inputs = root.join("inputs");
    fs::create_dir(&inputs)?;
    let mut originals = Vec::new();
    for (name, bytes) in [
        (
            "a.png",
            include_bytes!("../../tests/fixtures/png/rgb8.png").as_slice(),
        ),
        (
            "bad.png",
            include_bytes!("../../tests/fixtures/png/bad-deflate.png").as_slice(),
        ),
        (
            "credentials.png",
            include_bytes!("../../tests/fixtures/png/content-credentials.png").as_slice(),
        ),
        (
            "small.png",
            include_bytes!("../../tests/fixtures/png/already-optimized.png").as_slice(),
        ),
    ] {
        fs::write(inputs.join(name), bytes)?;
        originals.push((inputs.join(name), bytes.to_vec()));
    }
    for (name, sample) in [
        ("b.JPEG", "baseline-420"),
        ("gray.jpg", "gray"),
        ("icc.jpg", "icc"),
        ("cmyk.jpg", "cmyk"),
        ("protected.jpg", "opaque-app11"),
    ] {
        let bytes = fs::read(samples.join(format!("{sample}.jpg")))?;
        fs::write(inputs.join(name), &bytes)?;
        originals.push((inputs.join(name), bytes));
    }
    for direction in 1..=8 {
        let name = format!("exif-{direction}.jpg");
        let bytes = fs::read(samples.join(&name))?;
        fs::write(inputs.join(&name), &bytes)?;
        originals.push((inputs.join(name), bytes));
    }
    let engines = ImageEngines::with_jpeg(engine);
    let service = Arc::new(AssetService::new(engines.clone()));
    let mut runtime = TaskRuntime::with_engines(TaskConfig::default(), engines.clone())?;
    let control = runtime.control();
    let output = root.join("mixed");
    let policy = copy_to(&output)?;
    fs::write(output.join("b.JPEG"), b"existing target")?;
    let selection = control.import(vec![inputs.clone()], None)?;
    phase(&control, TaskPhase::Ready)?;
    let candidates = query(&control, "candidates")?;
    require(
        candidates["supportedFormats"] == json!(["png", "jpeg"]),
        "能力未贯通",
    )?;
    let candidate_rows = candidates["page"]["items"].as_array().ok_or("候选页无效")?;
    require(
        candidate_rows.iter().any(|row| row["format"] == "png")
            && candidate_rows.iter().any(|row| row["format"] == "jpeg"),
        "混合候选缺少格式",
    )?;
    let issues = query(&control, "issues")?;
    require(
        issues["page"]["items"]
            .as_array()
            .ok_or("问题页无效")?
            .iter()
            .any(|row| row["issue"]["failure"]["code"] == "unsupported_jpeg_credentials"),
        "JPEG保护提示丢失",
    )?;
    let lossy = PngMode::Lossy {
        quality: QualityValue::new(40)?,
    };
    control.start(selection, settings(policy, lossy))?;
    let finished = phase(&control, TaskPhase::Finished)?;
    let initial_wire = query(&control, "jobs")?;
    require(
        initial_wire["batch"]["confirmationCount"] == 1,
        "凭据列表混入JPEG或遗漏PNG",
    )?;
    let batch = finished.batch.as_ref().ok_or("缺少批次")?;
    let failed = batch
        .jobs
        .iter()
        .find(|job| {
            job.request
                .source
                .file_name()
                .is_some_and(|name| name == "b.JPEG")
        })
        .ok_or("缺少JPEG行")?;
    require(
        matches!(&failed.state, JobState::Failed(failure) if failure.code == JobErrorCode::TargetConflict),
        "JPEG冲突未逐行失败",
    )?;
    require(batch.summary.succeeded > 0, "冲突阻断了其他行")?;
    let old_request = identity(&finished, failed)?;
    fs::remove_file(output.join("b.JPEG"))?;
    let unrelated = root.join("new-draft");
    let _new_draft = settings(copy_to(&unrelated)?, lossy);
    control.retry(
        selection,
        batch.revision,
        RetryRequest {
            jobs: vec![RetryJob {
                id: failed.id,
                output: failed.request.output.clone(),
                metadata: PngMetadataPolicy::Preserve,
            }],
            parameters: BatchParameters::default(),
        },
    )?;
    let retried = phase(&control, TaskPhase::Finished)?;
    require(
        output.join("b.JPEG").is_file() && fs::read_dir(&unrelated)?.next().is_none(),
        "重试未保留冻结目标",
    )?;
    require(
        matches!(
            assets::resolve(&retried, &old_request, None),
            Err(assets::AssetError::StaleTask)
        ),
        "旧attempt仍可查看",
    )?;
    let batch = retried.batch.as_ref().ok_or("缺少重试批次")?;
    let mut previews = 0;
    let mut outputs = Vec::new();
    for job in &batch.jobs {
        let name = job
            .request
            .source
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or("验收名称无效")?;
        let request = identity(&retried, job)?;
        if let JobState::Succeeded(report) = &job.state {
            let result = asset(assets::resolve(
                &retried,
                &request,
                Some(RevealTarget::Result),
            ))?;
            require(result.is_file(), "结果定位失效")?;
            outputs.push(json!({"file":format!("mixed/{name}"), "source":format!("inputs/{name}"), "lossless":matches!(report, ImageReport::Jpeg(r) if !matches!(r.processing, JpegProcessing::Lossy { .. })), "format":if job.request.format() == ImageKind::Jpeg { "jpeg" } else { "png" }}));
        }
        if name == "icc.jpg" || name == "cmyk.jpg" {
            let permit = asset(service.reserve(Operation::Thumbnail))?;
            let path = asset(assets::resolve(&retried, &request, None))?;
            require(
                matches!(
                    service.prepare(&permit, request, path),
                    Err(assets::AssetError::Unavailable)
                ),
                "不支持颜色被伪装为预览",
            )?;
        } else if name.starts_with("exif-")
            || name == "b.JPEG"
            || name == "gray.jpg"
            || name == "a.png"
        {
            let permit = asset(service.reserve(Operation::Thumbnail))?;
            let path = asset(assets::resolve(&retried, &request, None))?;
            let prepared = asset(service.prepare(&permit, request, path))?;
            let image = asset(service.finish(&permit, prepared))?;
            require(
                image.width <= 128 && image.height <= 96 && image.png.len() <= 65_536,
                "预览超限",
            )?;
            fs::write(root.join(format!("preview-{name}.png")), image.png)?;
            previews += 1;
        }
    }
    for orientation in 1..=8 {
        let bytes = fs::read(inputs.join(format!("exif-{orientation}.jpg")))?;
        let decoded = jpeg::decode_preview(
            &bytes,
            engines.jpeg().ok_or("缺少引擎")?,
            JpegLimits::default(),
            &CancellationToken::default(),
        )?;
        require(decoded.orientation == orientation, "真实JPEG方向解析不符")?;
    }
    let directories = asset(assets::directory_page(
        &retried,
        &OutputDirectoriesRequest {
            batch: BatchAssetRequest {
                subscription_id: DecimalU64(1),
                selection_id: DecimalU64(selection.get()),
                batch_id: DecimalU64(batch.id.get()),
                batch_revision: DecimalU64(batch.revision),
            },
            offset: 0,
        },
    ))?;
    require(directories.total == 1, "混合输出目录分组错误")?;
    for (path, original) in &originals {
        require(fs::read(path)? == *original, "副本处理改变原图")?;
    }
    service.invalidate();
    let no_gain_output = copy_to(&root.join("no-gain"))?;
    control.import(
        vec![output.join("b.JPEG"), inputs.join("small.png")],
        Some(settings(no_gain_output, PngMode::Lossless)),
    )?;
    let no_gain = phase(&control, TaskPhase::Finished)?;
    let batch = no_gain.batch.as_ref().ok_or("缺少NoGain批次")?;
    require(batch.summary.no_gain == 2, "PNG/JPEG未保留NoGain原图")?;
    for job in &batch.jobs {
        require(
            asset(assets::resolve(
                &no_gain,
                &identity(&no_gain, job)?,
                Some(RevealTarget::Result),
            ))? == job.request.source,
            "NoGain未定位源图",
        )?;
    }
    control.import(
        vec![inputs.join("a.png"), inputs.join("b.JPEG")],
        Some(settings(ImportOutput::CopyBeside, PngMode::Lossless)),
    )?;
    let conflicts = phase(&control, TaskPhase::Finished)?;
    require(
        conflicts
            .batch
            .as_ref()
            .ok_or("缺少冲突批次")?
            .summary
            .failed
            == 2,
        "全部副本冲突未进入Finished",
    )?;
    let backup_source = root.join("backup.JPEG");
    let original = fs::read(inputs.join("b.JPEG"))?;
    fs::write(&backup_source, &original)?;
    control.import(
        vec![backup_source.clone()],
        Some(settings(ImportOutput::Overwrite, lossy)),
    )?;
    let backup_snapshot = phase(&control, TaskPhase::Finished)?;
    let job = &backup_snapshot.batch.as_ref().ok_or("缺少备份批次")?.jobs[0];
    let backup = asset(assets::resolve(
        &backup_snapshot,
        &identity(&backup_snapshot, job)?,
        Some(RevealTarget::Backup),
    ))?;
    require(fs::read(&backup)? == original, "JPEG备份未保留原字节")?;
    if let JobState::Succeeded(ImageReport::Jpeg(report)) = &job.state {
        require(
            matches!(report.outcome, ProcessingOutcome::Optimized { .. }),
            "JPEG备份处理未成功",
        )?;
    } else {
        return Err("JPEG覆盖未成功".into());
    }
    outputs.push(
        json!({"file":"backup.JPEG", "source":"inputs/b.JPEG", "lossless":false, "format":"jpeg"}),
    );
    fs::write(
        root.join("snapshot.json"),
        serde_json::to_vec_pretty(&query(&control, "jobs")?)?,
    )?;
    service.close();
    service.wait_idle();
    runtime.shutdown()?;
    require(
        control.snapshot().phase == TaskPhase::Closed,
        "应用任务未安全关闭",
    )?;
    Ok(
        json!({"result":"passed", "protocolVersion":ipc::TASK_PROTOCOL_VERSION, "previews":previews, "outputs":outputs, "mixedRetry":true, "noGain":true, "allConflicts":true, "backup":true, "shutdown":true}),
    )
}
