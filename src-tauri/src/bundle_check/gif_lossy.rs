//! v12真实混批：请求/实际路径、两种回退、q100、稳定ID重试和原图安全。
use super::*;
use pixofold_core::gif::{GifFallbackReason, GifMode, GifQualityMapping};

fn verify_result(snapshot: &TaskSnapshot, job: &pixofold_core::batch::JobSnapshot) -> Result<()> {
    let report = match &job.state {
        JobState::Succeeded(ImageReport::Gif(report))
        | JobState::NoGain(ImageReport::Gif(report)) => report,
        _ => return Err("GIF未进入带真实报告的终态".into()),
    };
    require(
        report.requested_mode == job.request.gif()?.mode,
        "GIF请求模式未冻结",
    )?;
    require(
        job.content_credentials_source().is_none(),
        "GIF获得了PNG凭据许可",
    )?;
    let path = asset(assets::resolve(
        snapshot,
        &identity(snapshot, job)?,
        Some(RevealTarget::Result),
    ))?;
    if matches!(job.state, JobState::NoGain(_)) {
        require(path == job.request.source, "GIF NoGain未定位原图")?;
    } else {
        let comparison = compare_gif(
            &fs::read(&job.request.source)?,
            &fs::read(path)?,
            GifLimits::default(),
            &CancellationToken::default(),
        )?;
        require(comparison.hard_constraints, "GIF最终文件破坏动画语义")?;
        require(
            if matches!(report.processing, GifProcessing::Lossy { .. }) {
                !comparison.lossless_equivalent && meets_quality(&comparison)
            } else {
                comparison.lossless_equivalent
            },
            "GIF最终像素与报告路径不符",
        )?;
    }
    Ok(())
}

pub(super) fn verify(control: &TaskControl, samples: &Path, root: &Path) -> Result<Value> {
    let directory = root.join("gif-lossy");
    fs::create_dir(&directory)?;
    let inputs = directory.join("inputs");
    fs::create_dir(&inputs)?;
    let mut originals = Vec::new();
    for (name, source) in [
        ("photo.gif", "photo-pan.gif"),
        ("ui.gif", "ui-motion.gif"),
        ("noise.gif", "dark-noise.gif"),
        ("recording.gif", "openrefine-320.gif"),
        ("retry.GiF", "photo-pan.gif"),
    ] {
        let bytes = fs::read(samples.join(source))?;
        fs::write(inputs.join(name), &bytes)?;
        originals.push((inputs.join(name), bytes));
    }
    for (name, bytes) in [
        (
            "a.png",
            include_bytes!("../../../tests/fixtures/png/gradient-rgb8.png").to_vec(),
        ),
        ("b.jpg", fs::read(samples.join("baseline-420.jpg"))?),
    ] {
        fs::write(inputs.join(name), &bytes)?;
        originals.push((inputs.join(name), bytes));
    }
    let seam = directory.join("transparent-seam.gif");
    let seam_original = fs::read(samples.join("transparent-seam.gif"))?;
    fs::write(&seam, &seam_original)?;
    let mut evidence = Vec::new();
    for q in [0, 60, 80, 99, 100] {
        let quality = QualityValue::new(q)?;
        let mode = PngMode::Lossy { quality };
        let output = directory.join(format!("q{q}"));
        let policy = copy_to(&output)?;
        fs::write(output.join("retry.GiF"), b"occupied")?;
        let mut sources = vec![inputs.clone()];
        if q == 80 {
            sources.push(seam.clone());
        }
        let selection = control.import(sources, Some(settings(policy, mode)))?;
        let finished = phase(control, TaskPhase::Finished)?;
        let wire = query(control, "jobs")?;
        let batch = finished.batch.as_ref().ok_or("缺少有损混批")?;
        require(
            batch.summary.failed == if q == 80 { 2 } else { 1 },
            "GIF安全/冲突失败被误回退或影响其他行",
        )?;
        require(
            batch.summary.terminal == batch.summary.total
                && batch.active_workers == 0
                && batch.reserved_working_bytes.0 == 0,
            "GIF终态仍持有worker/预算",
        )?;
        for job in &batch.jobs {
            let name = job
                .request
                .source
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or("样本名无效")?;
            require(job.request.mode == mode, "混批请求质量未冻结")?;
            if name == "retry.GiF" {
                require(
                    matches!(&job.state, JobState::Failed(f) if f.code == JobErrorCode::TargetConflict),
                    "GIF目标冲突未逐行失败",
                )?;
                require(
                    fs::read(output.join(name))? == b"occupied",
                    "GIF覆盖了冲突目标",
                )?;
                continue;
            }
            if name == "transparent-seam.gif" {
                require(
                    matches!(&job.state, JobState::Failed(f) if f.code == JobErrorCode::Validation),
                    "透明安全拒绝被降为无损成功",
                )?;
                require(
                    !output.join(name).exists() && fs::read(&seam)? == seam_original,
                    "透明失败污染输入/输出",
                )?;
                continue;
            }
            if job.request.format() != ImageKind::Gif {
                require(
                    matches!(job.state, JobState::Succeeded(_) | JobState::NoGain(_)),
                    "GIF失败阻断其他格式",
                )?;
                continue;
            }
            verify_result(&finished, job)?;
            require(
                job.mapping
                    == Some(pixofold_core::batch::QualityMapping::Gif(
                        GifQualityMapping::new(quality),
                    )),
                "GIF质量映射未进入批量快照",
            )?;
            let report = match &job.state {
                JobState::Succeeded(ImageReport::Gif(r))
                | JobState::NoGain(ImageReport::Gif(r)) => r,
                _ => return Err("缺少GIF报告".into()),
            };
            if q == 100 || name == "ui.gif" {
                require(
                    report.processing == GifProcessing::Lossless,
                    "q100/UI实际无损结果不符",
                )?;
            }
            if name == "photo.gif" && q <= 80 {
                require(
                    matches!(report.processing, GifProcessing::Lossy { .. }),
                    "照片未实际有损",
                )?;
            }
            if name == "noise.gif" && q <= 60 {
                require(
                    matches!(
                        report.processing,
                        GifProcessing::LosslessFallback {
                            reason: GifFallbackReason::QualityBelowTarget,
                            ..
                        }
                    ),
                    "缺少质量回退",
                )?;
            }
            if name == "recording.gif" && q <= 60 {
                require(
                    matches!(
                        report.processing,
                        GifProcessing::LosslessFallback {
                            reason: GifFallbackReason::InsufficientGain,
                            ..
                        }
                    ) && matches!(job.state, JobState::NoGain(_)),
                    "微收益未回退并保留原图",
                )?;
            }
            if matches!(job.state, JobState::NoGain(_)) {
                require(!output.join(name).exists(), "NoGain产生了副本")?;
            }
        }
        let failed = batch
            .jobs
            .iter()
            .find(|j| {
                j.request
                    .source
                    .file_name()
                    .is_some_and(|n| n == "retry.GiF")
            })
            .ok_or("缺少GIF重试行")?;
        let old_identity = identity(&finished, failed)?;
        fs::remove_file(output.join("retry.GiF"))?;
        let next_mode = if q == 100 {
            PngMode::Lossy {
                quality: QualityValue::new(80)?,
            }
        } else {
            PngMode::Lossless
        };
        control.retry(
            selection,
            batch.revision,
            RetryRequest {
                jobs: vec![RetryJob {
                    id: failed.id,
                    output: failed.request.output.clone(),
                    metadata: PngMetadataPolicy::Preserve,
                }],
                parameters: BatchParameters {
                    mode: next_mode,
                    ..Default::default()
                },
            },
        )?;
        let retried = phase(control, TaskPhase::Finished)?;
        let retry_batch = retried.batch.as_ref().ok_or("缺少GIF重试批次")?;
        for job in &retry_batch.jobs {
            let previous = batch
                .jobs
                .iter()
                .find(|p| p.id == job.id)
                .ok_or("GIF重试ID变化")?;
            if job.id == failed.id {
                require(
                    job.attempt == previous.attempt + 1 && job.request.mode == next_mode,
                    "GIF切换模式/重试attempt错误",
                )?;
                verify_result(&retried, job)?;
                // Windows授权目录使用原生规范路径；比较文件身份而非路径前缀表示。
                require(
                    same_file::is_same_file(
                        asset(assets::resolve(
                            &retried,
                            &identity(&retried, job)?,
                            Some(RevealTarget::Result),
                        ))?,
                        output.join("retry.GiF"),
                    )?,
                    "GIF重试丢失原目标",
                )?;
            } else {
                require(
                    job.attempt == previous.attempt && job.request.mode == previous.request.mode,
                    "GIF重试改动未选中行",
                )?;
            }
        }
        require(
            matches!(
                assets::resolve(&retried, &old_identity, None),
                Err(assets::AssetError::StaleTask)
            ),
            "GIF旧attempt仍可定位",
        )?;
        evidence.push(json!({"quality":q, "initial":wire, "retried":query(control,"jobs")?}));
    }
    // 有损覆盖的备份必须保留字节和扩展名，最终文件独立验证质量与动画语义。
    let source = directory.join("backup.GiF");
    let original = fs::read(inputs.join("photo.gif"))?;
    fs::write(&source, &original)?;
    control.import(
        vec![source.clone()],
        Some(settings(
            ImportOutput::Overwrite,
            PngMode::Lossy {
                quality: QualityValue::new(80)?,
            },
        )),
    )?;
    let snapshot = phase(control, TaskPhase::Finished)?;
    let job = &snapshot.batch.as_ref().ok_or("缺少GIF有损备份批次")?.jobs[0];
    let JobState::Succeeded(ImageReport::Gif(report)) = &job.state else {
        return Err("GIF有损覆盖未成功".into());
    };
    require(
        matches!(report.processing, GifProcessing::Lossy { .. })
            && report.requested_mode
                == GifMode::Lossy {
                    quality: QualityValue::new(80)?,
                },
        "GIF有损覆盖报告错误",
    )?;
    let backup = asset(assets::resolve(
        &snapshot,
        &identity(&snapshot, job)?,
        Some(RevealTarget::Backup),
    ))?;
    require(
        backup.extension().is_some_and(|s| s == "GiF") && fs::read(backup)? == original,
        "GIF有损覆盖备份不符",
    )?;
    let comparison = compare_gif(
        &original,
        &fs::read(source)?,
        GifLimits::default(),
        &CancellationToken::default(),
    )?;
    require(
        comparison.hard_constraints
            && !comparison.lossless_equivalent
            && meets_quality(&comparison),
        "GIF有损覆盖输出不符",
    )?;
    for (path, bytes) in originals {
        require(fs::read(path)? == bytes, "GIF有损混批改变原图")?;
    }
    fs::write(
        directory.join("snapshots.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;
    Ok(
        json!({"result":"passed","qualities":[0,60,80,99,100],"qualityFallback":true,"insufficientGain":true,"transparentRejected":true,"stableRetry":true,"lossyBackup":true,"sourcesUnchanged":true}),
    )
}
