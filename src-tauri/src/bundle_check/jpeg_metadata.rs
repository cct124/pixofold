//! v15真实JPEG元数据及PNG/JPEG共同确认；复用生产DTO、mutation、任务和输出路径。
use super::super::{commands::verify_mutation, ingress::NativeImports, jpeg_metadata_sample};
use super::*;

fn mutate(control: &TaskControl, imports: &NativeImports, operation: Value) -> Result<()> {
    let request = serde_json::from_value(json!({"subscriptionId":"1","operation":operation}))?;
    verify_mutation(control, imports, request).map_err(|e| format!("JPEG确认变更失败：{e:?}"))?;
    Ok(())
}

pub(super) fn verify(
    control: &TaskControl,
    service: &Arc<AssetService>,
    samples: &Path,
    root: &Path,
) -> Result<Value> {
    let directory = root.join("jpeg-metadata");
    fs::create_dir(&directory)?;
    // checker在仓库外使用相对输出目录；与生产扫描保持同一规范化路径表示。
    let directory = fs::canonicalize(directory)?;
    let [ordinary, signed, broken, unknown] =
        jpeg_metadata_sample::samples(&fs::read(samples.join("baseline-420.jpg"))?);
    let png = include_bytes!("../../../tests/fixtures/png/content-credentials.png").as_slice();
    let mut matrix = Vec::new();
    for (label, mode) in [
        ("copy-flat", PngMode::Lossless),
        (
            "copy-tree",
            PngMode::Lossy {
                quality: QualityValue::default(),
            },
        ),
        ("backup", PngMode::Lossless),
        (
            "overwrite",
            PngMode::Lossy {
                quality: QualityValue::default(),
            },
        ),
    ] {
        service.invalidate();
        let current = directory.join(label);
        fs::create_dir(&current)?;
        let inputs = current.join("inputs");
        fs::create_dir(&inputs)?;
        let nested = inputs.join("sub");
        fs::create_dir(&nested)?;
        let jpeg_source = nested.join("credential.JpEg");
        fs::write(&jpeg_source, &signed)?;
        let png_source = inputs.join("credential.png");
        fs::write(&png_source, png)?;
        fs::write(inputs.join("ordinary.jpg"), &ordinary)?;
        fs::write(inputs.join("broken.jpg"), &broken)?;
        fs::write(inputs.join("unknown.jpg"), &unknown)?;
        let selection = control.import(vec![inputs.clone()], None)?;
        phase(control, TaskPhase::Ready)?;
        let candidates = query(control, "candidates")?;
        require(
            candidates["page"]["total"] == 3,
            "JPEG凭据未进入候选或坏输入被接纳",
        )?;
        require(
            query(control, "issues")?["page"]["total"] == 2,
            "坏/未知凭据拒绝数量错误",
        )?;
        control.start(
            selection,
            settings(copy_to(&current.join("initial"))?, mode),
        )?;
        let first = phase(control, TaskPhase::Finished)?;
        let batch = first.batch.as_ref().ok_or("缺少凭据批次")?;
        let selected: Vec<_> = batch
            .jobs
            .iter()
            .filter(|j| j.content_credentials_source().is_some())
            .map(|j| j.id.get())
            .collect();
        require(selected.len() == 2, "PNG/JPEG确认列表未合并")?;
        let jpeg_id = batch
            .jobs
            .iter()
            .find(|j| j.request.source == jpeg_source)
            .ok_or("缺少JPEG行")?
            .id
            .get();
        require(
            query(control, "confirmations")?["page"]["total"] == 2,
            "确认DTO漏项",
        )?;
        require(
            fs::read(&jpeg_source)? == signed && fs::read(&png_source)? == png,
            "确认前改写了原件",
        )?;
        let imports = Arc::new(NativeImports::default());
        let output_root = current.join("confirmed");
        fs::create_dir(&output_root)?;
        let grant = imports
            .reserve(DecimalU64(1))
            .map_err(|e| format!("目录选择：{e:?}"))?
            .complete_output(Some(OutputDirectory::open(&output_root)?))
            .map_err(|e| format!("目录授权：{e:?}"))?
            .ok_or("缺少目录授权")?;
        let output = match label {
            "backup" => json!("overwrite_with_backup"),
            "overwrite" => json!("overwrite_without_backup"),
            _ => {
                json!({"copy_to":{"directoryId":grant.directory_id.0.to_string(),"preserveStructure":label == "copy-tree"}})
            }
        };
        let confirm = |revision: u64, ids: &[usize]| json!({"kind":"confirm_content_credentials","selectionId":selection.get().to_string(),"expectedBatchRevision":revision.to_string(),"jobIds":ids,"mode":mode,"output":output,"consent":"remove_content_credentials"});
        require(
            mutate(control, &imports, confirm(batch.revision + 1, &selected)).is_err(),
            "接受了过期确认",
        )?;
        let conflict = output_root.join("credential.JpEg");
        if label == "copy-flat" {
            fs::write(&conflict, b"existing target")?;
        }
        mutate(control, &imports, confirm(batch.revision, &selected))?;
        let mut completed = phase(control, TaskPhase::Finished)?;
        if label == "copy-flat" {
            let batch = completed.batch.as_ref().ok_or("缺少冲突批次")?;
            let row = batch
                .jobs
                .iter()
                .find(|j| j.id.get() == jpeg_id)
                .ok_or("缺少冲突行")?;
            require(
                matches!(&row.state, JobState::Failed(f) if f.code == JobErrorCode::TargetConflict)
                    && row.content_credentials_source().is_none(),
                "冲突错误被伪装成凭据成功",
            )?;
            require(
                fs::read(&conflict)? == b"existing target" && fs::read(&jpeg_source)? == signed,
                "冲突改写文件",
            )?;
            fs::remove_file(&conflict)?;
            mutate(
                control,
                &imports,
                json!({"kind":"retry","selectionId":selection.get().to_string(),"expectedBatchRevision":batch.revision.to_string(),"jobIds":[jpeg_id],"mode":mode}),
            )?;
            let repeated = phase(control, TaskPhase::Finished)?;
            let batch = repeated.batch.as_ref().ok_or("缺少重试批次")?;
            require(
                batch
                    .jobs
                    .iter()
                    .find(|j| j.id.get() == jpeg_id)
                    .is_some_and(|j| j.content_credentials_source().is_some()),
                "普通重试继承了凭据移除许可",
            )?;
            mutate(control, &imports, confirm(batch.revision, &[jpeg_id]))?;
            completed = phase(control, TaskPhase::Finished)?;
        }
        let batch = completed.batch.as_ref().ok_or("缺少完成批次")?;
        for job in batch.jobs.iter().filter(|j| selected.contains(&j.id.get())) {
            let JobState::Succeeded(report) = &job.state else {
                return Err(format!("确认后未成功：{:?}", job.state).into());
            };
            require(report.credentials_removed(), "确认成功未报告凭据移除")?;
            let ProcessingOutcome::Optimized { output, backup } = report.outcome() else {
                return Err("确认输出无效".into());
            };
            let before = if job.request.format() == ImageKind::Jpeg {
                signed.as_slice()
            } else {
                png
            };
            if label.starts_with("copy-") {
                require(
                    fs::read(&job.request.source)? == before && backup.is_none(),
                    "副本修改原件或误建备份",
                )?;
            }
            if label == "backup" {
                let backup = backup.as_ref().ok_or("缺少凭据原件备份")?;
                require(
                    fs::read(backup)? == before
                        && backup.extension() == job.request.source.extension(),
                    "备份不保留原件或扩展名",
                )?;
            } else {
                require(backup.is_none(), "无备份策略意外备份")?;
            }
            if label == "copy-tree" {
                let relative = job.request.source.strip_prefix(&inputs)?;
                require(
                    output == &output_root.join("inputs").join(relative),
                    "凭据确认丢失输入目录结构",
                )?;
            }
            let request = identity(&completed, job)?;
            let permit = asset(service.reserve(Operation::Thumbnail))?;
            let prepared = asset(service.prepare(&permit, request, output.clone()))?;
            let preview = asset(service.finish(&permit, prepared))?;
            require(
                preview.width <= 128 && preview.height <= 96,
                "凭据输出预览越界",
            )?;
        }
        require(
            query(control, "confirmations")?["page"]["total"] == 0,
            "成功后凭据确认未清除",
        )?;
        let jobs = query(control, "jobs")?;
        require(
            jobs["page"]["items"]
                .as_array()
                .ok_or("缺少报告")?
                .iter()
                .filter(|r| r["state"]["report"]["contentCredentialsRemoved"] == true)
                .count()
                == 2,
            "移除报告数量错误",
        )?;
        fs::write(
            current.join("snapshot.json"),
            serde_json::to_vec_pretty(&jobs)?,
        )?;
        matrix.push(json!({"output":label,"confirmed":2,"previews":2,"result":"passed"}));
    }
    Ok(json!({"result":"passed","matrix":matrix}))
}
