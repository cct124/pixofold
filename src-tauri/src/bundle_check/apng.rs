//! v14真实四格式混批、APNG有损/回退/冻结重试/首帧/备份；仅显式部署验收编译。
use super::super::apng_test_sample as sample;
use super::*;
use pixofold_core::apng::{ApngProcessing, ApngValidationLimits, compare_apng, validate_apng};

pub(super) fn verify(
    control: &TaskControl,
    service: &Arc<AssetService>,
    samples: &Path,
    root: &Path,
) -> Result<Value> {
    let directory = root.join("apng");
    fs::create_dir(&directory)?;
    let inputs = directory.join("inputs");
    fs::create_dir(&inputs)?;
    let gain = include_bytes!("../../../tests/fixtures/apng/split-data.apng").as_slice();
    let mut originals = Vec::new();
    for (name, bytes) in [
        ("gain.APNG", gain),
        ("conflict.PNG", gain),
        (
            "small.png",
            include_bytes!("../../../tests/fixtures/apng/default-in.png").as_slice(),
        ),
        (
            "poster.apng",
            include_bytes!("../../../tests/fixtures/apng/first-over-transparent.apng").as_slice(),
        ),
        (
            "bad.apng",
            include_bytes!("../../../tests/fixtures/apng/bad-deflate.apng").as_slice(),
        ),
        (
            "signed.apng",
            include_bytes!("../../../tests/fixtures/apng/metadata-caBX.apng").as_slice(),
        ),
        (
            "static.png",
            include_bytes!("../../../tests/fixtures/png/rgb8.png").as_slice(),
        ),
        (
            "animation.gif",
            include_bytes!("../../../tests/fixtures/gif/larger-pattern.gif").as_slice(),
        ),
    ] {
        fs::write(inputs.join(name), bytes)?;
        originals.push((inputs.join(name), bytes.to_vec()));
    }
    let jpeg = fs::read(samples.join("baseline-420.jpg"))?;
    fs::write(inputs.join("photo.jpg"), &jpeg)?;
    originals.push((inputs.join("photo.jpg"), jpeg));
    for (name, bytes) in [
        ("palette.apng", sample::animation()),
        (
            "beach.apng",
            include_bytes!(
                "../../../tests/manual-images/apng/Animated_PNG_example_bouncing_beach_ball.png"
            )
            .to_vec(),
        ),
        (
            "deep.apng",
            include_bytes!("../../../tests/fixtures/apng/rgba16.apng").to_vec(),
        ),
        (
            "interlaced.apng",
            include_bytes!("../../../tests/fixtures/apng/adam7.apng").to_vec(),
        ),
    ] {
        fs::write(inputs.join(name), &bytes)?;
        originals.push((inputs.join(name), bytes));
    }
    let mut matrix = Vec::new();
    for (label, mode) in [
        ("lossless", PngMode::Lossless),
        (
            "lossy0",
            PngMode::Lossy {
                quality: QualityValue::new(0)?,
            },
        ),
        (
            "lossy50",
            PngMode::Lossy {
                quality: QualityValue::new(50)?,
            },
        ),
        (
            "lossy80",
            PngMode::Lossy {
                quality: QualityValue::new(80)?,
            },
        ),
        (
            "lossy95",
            PngMode::Lossy {
                quality: QualityValue::new(95)?,
            },
        ),
        (
            "lossy100",
            PngMode::Lossy {
                quality: QualityValue::new(100)?,
            },
        ),
    ] {
        service.invalidate();
        let output = directory.join(label);
        let policy = copy_to(&output)?;
        fs::write(output.join("conflict.PNG"), b"existing")?;
        let selection = control.import(vec![inputs.clone()], Some(settings(policy, mode)))?;
        let snapshot = phase(control, TaskPhase::Finished)?;
        let batch = snapshot.batch.as_ref().ok_or("缺少四格式批次")?;
        require(
            snapshot.import.as_ref().ok_or("缺少扫描")?.issues().len() == 1,
            "APNG保护元数据拒绝不符",
        )?;
        for format in [
            ImageKind::Png,
            ImageKind::Jpeg,
            ImageKind::Gif,
            ImageKind::Apng,
        ] {
            require(
                batch.jobs.iter().any(|j| j.request.format() == format),
                "四格式混批缺失格式",
            )?;
        }
        let quantized = matches!(mode, PngMode::Lossy { quality } if quality.get() < 100);
        require(
            batch.summary.failed == if quantized { 4 } else { 2 }
                && batch.summary.no_gain >= 1
                && batch.summary.succeeded >= 1,
            "四格式终态不符",
        )?;
        let wire = query(control, "jobs")?;
        for row in wire["page"]["items"].as_array().ok_or("缺少DTO行")? {
            if row["format"] == "apng"
                && matches!(row["state"]["kind"].as_str(), Some("succeeded" | "no_gain"))
            {
                require(
                    (if quantized {
                        matches!(
                            row["state"]["report"]["processing"]["details"]["kind"].as_str(),
                            Some("lossy" | "lossless_fallback")
                        )
                    } else {
                        row["state"]["report"]["processing"]["details"]["kind"] == "lossless"
                    }) && row["state"]["report"]["contentCredentialsRemoved"] == false,
                    "APNG实际模式/许可报告不符",
                )?;
            }
        }
        // q80同时具备成功有损和两类回退，不能只靠小图的无损回退验证接线。
        if label == "lossy80" {
            for (name, expected) in [
                ("palette.apng", "lossy"),
                ("beach.apng", "quality_below_target"),
                ("small.png", "insufficient_gain"),
            ] {
                let row = batch
                    .jobs
                    .iter()
                    .find(|j| j.request.source.file_name().is_some_and(|n| n == name))
                    .ok_or("缺少APNG模式样本")?;
                let report = match &row.state {
                    JobState::Succeeded(ImageReport::Apng(r))
                    | JobState::NoGain(ImageReport::Apng(r)) => r,
                    _ => return Err("APNG模式样本失败".into()),
                };
                let actual = match report.processing {
                    ApngProcessing::Lossy { .. } => "lossy",
                    ApngProcessing::Lossless => "lossless",
                    ApngProcessing::LosslessFallback { reason, .. } => match reason {
                        pixofold_core::apng::ApngFallbackReason::QualityBelowTarget => {
                            "quality_below_target"
                        }
                        pixofold_core::apng::ApngFallbackReason::InsufficientGain => {
                            "insufficient_gain"
                        }
                    },
                };
                require(actual == expected, "APNG实际候选路径不符")?;
            }
        }
        let conflict = batch
            .jobs
            .iter()
            .find(|j| {
                j.request
                    .source
                    .file_name()
                    .is_some_and(|n| n == "conflict.PNG")
            })
            .ok_or("缺少冲突行")?;
        require(
            matches!(&conflict.state, JobState::Failed(f) if f.code == JobErrorCode::TargetConflict),
            "APNG冲突未隔离",
        )?;
        require(
            fs::read(output.join("conflict.PNG"))? == b"existing",
            "已有副本被覆盖",
        )?;
        require(!output.join("small.png").exists(), "NoGain创建了副本")?;
        fs::remove_file(output.join("conflict.PNG"))?;
        control.retry(
            selection,
            batch.revision,
            RetryRequest {
                jobs: std::iter::once(RetryJob {
                    id: conflict.id,
                    output: conflict.request.output.clone(),
                    metadata: PngMetadataPolicy::Preserve,
                }).chain(batch.jobs.iter().filter(|j| matches!(&j.state, JobState::Failed(f) if f.code == JobErrorCode::UnsupportedApngMode)).map(|j| RetryJob { id: j.id, output: j.request.output.clone(), metadata: PngMetadataPolicy::Preserve })).collect(),
                parameters: BatchParameters::default(),
            },
        )?;
        let retried = phase(control, TaskPhase::Finished)?;
        let batch = retried.batch.as_ref().ok_or("缺少重试批次")?;
        let retried_job = batch
            .jobs
            .iter()
            .find(|j| j.id == conflict.id)
            .ok_or("稳定ID变化")?;
        require(
            retried_job.attempt == 2
                && matches!(retried_job.state, JobState::Succeeded(ImageReport::Apng(_))),
            "APNG重试不符",
        )?;
        compare_apng(
            gain,
            &fs::read(output.join("conflict.PNG"))?,
            &ApngValidationLimits::default(),
            &Default::default(),
        )?;
        for job in &batch.jobs {
            if job.request.format() != ImageKind::Apng
                || !matches!(job.state, JobState::Succeeded(_) | JobState::NoGain(_))
            {
                continue;
            }
            let report = match &job.state {
                JobState::Succeeded(ImageReport::Apng(r))
                | JobState::NoGain(ImageReport::Apng(r)) => r,
                _ => return Err("APNG报告缺失".into()),
            };
            require(
                report.requested_mode == job.request.apng()?.mode,
                "APNG冻结请求错配",
            )?;
            require(
                job.content_credentials_source().is_none(),
                "APNG获得了PNG凭据许可",
            )?;
            let result_path = asset(assets::resolve(
                &retried,
                &identity(&retried, job)?,
                Some(RevealTarget::Result),
            ))?;
            if let ProcessingOutcome::Optimized { .. } = report.outcome {
                let stored = fs::read(&result_path)?;
                if matches!(report.processing, ApngProcessing::Lossy { .. }) {
                    require(
                        (stored[24], stored[25]) == (8, 3) && report.quality.is_some(),
                        "有损输出不是索引APNG",
                    )?;
                    validate_apng(
                        &stored,
                        &ApngValidationLimits::default(),
                        &CancellationToken::default(),
                    )?;
                } else {
                    compare_apng(
                        &fs::read(&job.request.source)?,
                        &stored,
                        &ApngValidationLimits::default(),
                        &CancellationToken::default(),
                    )?;
                    require(report.quality.is_none(), "回退泄露淘汰候选质量")?;
                }
            } else {
                require(
                    result_path == job.request.source && report.quality.is_none(),
                    "NoGain未保留源图",
                )?;
            }
            let request = identity(&retried, job)?;
            let permit = asset(service.reserve(Operation::Thumbnail))?;
            let path = asset(assets::resolve(&retried, &request, None))?;
            let prepared = asset(service.prepare(&permit, request, path))?;
            let image = asset(service.finish(&permit, prepared))?;
            require(
                image.width <= 128 && image.height <= 96 && image.png.len() <= 65536,
                "APNG缩略图超限",
            )?;
            if job
                .request
                .source
                .file_name()
                .is_some_and(|n| n == "poster.apng")
            {
                fs::write(directory.join(format!("preview-{label}.png")), image.png)?;
                drop(permit);
                let permit = asset(service.reserve(Operation::Thumbnail))?;
                let request = identity(&retried, job)?;
                let path = asset(assets::resolve(&retried, &request, None))?;
                let prepared = asset(service.prepare(&permit, request, path))?;
                service.invalidate();
                require(
                    matches!(
                        service.finish(&permit, prepared),
                        Err(assets::AssetError::StaleTask)
                    ),
                    "APNG撤销后缓存复活",
                )?;
            }
        }
        fs::write(
            directory.join(format!("snapshot-{label}.json")),
            serde_json::to_vec_pretty(&query(control, "jobs")?)?,
        )?;
        matrix.push(json!({"mode":label,"jobs":batch.jobs.len(),"failed":batch.summary.failed,"succeeded":batch.summary.succeeded,"noGain":batch.summary.no_gain}));
    }
    for (path, bytes) in &originals {
        require(fs::read(path)? == *bytes, "混批源文件改变")?;
    }
    let source = directory.join("backup.APNG");
    let backup_original = sample::animation();
    fs::write(&source, &backup_original)?;
    control.import(
        vec![source],
        Some(settings(
            ImportOutput::Overwrite,
            PngMode::Lossy {
                quality: QualityValue::new(80)?,
            },
        )),
    )?;
    let snapshot = phase(control, TaskPhase::Finished)?;
    let job = &snapshot.batch.as_ref().ok_or("缺少APNG备份批次")?.jobs[0];
    let backup = asset(assets::resolve(
        &snapshot,
        &identity(&snapshot, job)?,
        Some(RevealTarget::Backup),
    ))?;
    require(
        backup.extension().is_some_and(|e| e == "APNG") && fs::read(&backup)? == backup_original,
        "APNG备份不符",
    )?;
    Ok(
        json!({"result":"passed", "matrix":matrix,"independentPlayback":true,"lossyAndFallbacks":true,"frozenRetry":true,"backup":true,"previews":true,"revokedPreview":true,"sourceUnchanged":true}),
    )
}
