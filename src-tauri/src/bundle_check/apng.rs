//! v13真实四格式混批、APNG冻结重试/无损报告/首帧/备份；仅显式部署验收编译。
use super::*;
use pixofold_core::apng::{ApngValidationLimits, compare_apng};

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
    let mut matrix = Vec::new();
    for (label, mode) in [
        ("lossless", PngMode::Lossless),
        (
            "lossy80",
            PngMode::Lossy {
                quality: QualityValue::new(80)?,
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
        require(
            batch.summary.failed == 2 && batch.summary.no_gain >= 1 && batch.summary.succeeded >= 1,
            "四格式终态不符",
        )?;
        let wire = query(control, "jobs")?;
        for row in wire["page"]["items"].as_array().ok_or("缺少DTO行")? {
            if row["format"] == "apng"
                && matches!(row["state"]["kind"].as_str(), Some("succeeded" | "no_gain"))
            {
                require(
                    row["state"]["report"]["processing"]["details"]["kind"] == "lossless"
                        && row["state"]["report"]["contentCredentialsRemoved"] == false,
                    "APNG实际模式/许可报告不符",
                )?;
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
                jobs: vec![RetryJob {
                    id: conflict.id,
                    output: conflict.request.output.clone(),
                    metadata: PngMetadataPolicy::Preserve,
                }],
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
    fs::write(&source, gain)?;
    control.import(
        vec![source],
        Some(settings(ImportOutput::Overwrite, PngMode::Lossless)),
    )?;
    let snapshot = phase(control, TaskPhase::Finished)?;
    let job = &snapshot.batch.as_ref().ok_or("缺少APNG备份批次")?.jobs[0];
    let backup = asset(assets::resolve(
        &snapshot,
        &identity(&snapshot, job)?,
        Some(RevealTarget::Backup),
    ))?;
    require(
        backup.extension().is_some_and(|e| e == "APNG") && fs::read(&backup)? == gain,
        "APNG备份不符",
    )?;
    Ok(
        json!({"result":"passed", "matrix":matrix,"independentPlayback":true,"frozenRetry":true,"backup":true,"previews":true,"revokedPreview":true,"sourceUnchanged":true}),
    )
}
