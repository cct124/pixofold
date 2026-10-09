//! 真实批次目录分组/身份/分页回归；不调用系统文件管理器。
use super::*;
use crate::tasks::{TaskConfig, TaskRuntime, TaskSettings};
use pixofold_core::{
    batch::BatchParameters,
    import::{CopyLayout, ImportOutput},
    model::OutputDirectory,
};
use std::{
    fs,
    time::{Duration, Instant},
};

fn completed(source: &Path, output: ImportOutput) -> (TaskRuntime, TaskSnapshot) {
    let runtime = TaskRuntime::new(TaskConfig::default()).unwrap();
    let control = runtime.control();
    control
        .import(
            vec![source.to_path_buf()],
            Some(TaskSettings {
                output,
                parameters: BatchParameters::default(),
            }),
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut snapshot = control.snapshot();
    while snapshot.phase != TaskPhase::Finished {
        snapshot = control
            .wait_for_change(
                snapshot.revision,
                deadline.saturating_duration_since(Instant::now()),
            )
            .unwrap();
    }
    (runtime, snapshot)
}
fn batch(snapshot: &TaskSnapshot) -> BatchAssetRequest {
    let batch = snapshot.batch.as_ref().unwrap();
    BatchAssetRequest {
        subscription_id: crate::ipc::DecimalU64(1),
        selection_id: crate::ipc::DecimalU64(snapshot.selection.unwrap().get()),
        batch_id: crate::ipc::DecimalU64(batch.id.get()),
        batch_revision: crate::ipc::DecimalU64(batch.revision),
    }
}
fn png(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/png/rgb8.png"),
        path,
    )
    .unwrap();
}

#[test]
fn output_directories_use_authorized_root_for_flat_and_tree_layouts_not_current_draft() {
    for layout in [CopyLayout::Flat, CopyLayout::PreserveRoots] {
        let dir = tempfile::tempdir().unwrap();
        let photos = dir.path().join("照片");
        let out = dir.path().join("输出");
        png(&photos.join("child/a.png"));
        png(&photos.join("b.png"));
        fs::create_dir(&out).unwrap();
        let root = OutputDirectory::open(&out).unwrap();
        let expected = root.path().to_path_buf();
        let (_runtime, snapshot) = completed(
            &photos,
            ImportOutput::CopyToAuthorized {
                directory: root,
                layout,
            },
        );
        let request = OutputDirectoriesRequest {
            batch: batch(&snapshot),
            offset: 0,
        };
        let page = directory_page(&snapshot, &request).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].result_count, 2);
        assert_eq!(page.items[0].name.text, "输出");
        let target = resolve_directory(
            &snapshot,
            &OpenOutputDirectoryRequest {
                batch: request.batch,
                job_id: page.items[0].job_id,
            },
        )
        .unwrap();
        assert_eq!(target.path, expected);
        target.validate().unwrap();
        assert!(
            !serde_json::to_string(&page)
                .unwrap()
                .contains(&dir.path().to_string_lossy().to_string())
        );
        // 根目录同名替换不能被旧任务重新授权；身份句柄仍由已完成任务持有。
        fs::rename(&out, dir.path().join("moved")).unwrap();
        assert_eq!(target.validate(), Err(AssetError::FileMissing));
        fs::create_dir(&out).unwrap();
        assert_eq!(target.validate(), Err(AssetError::FileChanged));
    }
}

#[test]
fn output_directories_are_deduplicated_and_paged_across_all_successful_rows() {
    let dir = tempfile::tempdir().unwrap();
    for index in 0..52 {
        png(&dir.path().join(format!("folder-{index:02}/image.png")));
    }
    png(&dir.path().join("folder-00/second.png"));
    fs::write(dir.path().join("broken.png"), b"not png").unwrap();
    let (_runtime, snapshot) = completed(dir.path(), ImportOutput::OverwriteWithoutBackup);
    let request = OutputDirectoriesRequest {
        batch: batch(&snapshot),
        offset: 0,
    };
    let first = directory_page(&snapshot, &request).unwrap();
    assert_eq!(first.total, 52);
    assert_eq!(first.items.len(), OUTPUT_DIRECTORY_PAGE_SIZE);
    assert_eq!(first.items[0].result_count, 2);
    let second = directory_page(
        &snapshot,
        &OutputDirectoriesRequest {
            offset: 50,
            ..request
        },
    )
    .unwrap();
    assert_eq!(second.items.len(), 2);
    assert!(
        first
            .items
            .iter()
            .all(|a| second.items.iter().all(|b| a.job_id != b.job_id))
    );
    assert!(
        directory_page(
            &snapshot,
            &OutputDirectoriesRequest {
                offset: 53,
                ..request
            }
        )
        .is_err()
    );
}

#[test]
fn output_directories_reject_old_batches_running_rows_clear_and_non_results() {
    let dir = tempfile::tempdir().unwrap();
    png(&dir.path().join("a.png"));
    let (_runtime, mut snapshot) = completed(dir.path(), ImportOutput::OverwriteWithoutBackup);
    let request = OutputDirectoriesRequest {
        batch: batch(&snapshot),
        offset: 0,
    };
    for field in ["selectionId", "batchId", "batchRevision"] {
        let mut wire = serde_json::json!({"subscriptionId":"1","selectionId":request.batch.selection_id,"batchId":request.batch.batch_id,"batchRevision":request.batch.batch_revision});
        wire[field] = serde_json::json!("99999");
        let invalid = serde_json::from_value(wire).unwrap();
        assert!(
            directory_page(
                &snapshot,
                &OutputDirectoriesRequest {
                    batch: invalid,
                    offset: 0
                }
            )
            .is_err()
        );
    }
    for phase in [
        TaskPhase::Running,
        TaskPhase::Preparing,
        TaskPhase::Clearing,
        TaskPhase::Closing,
    ] {
        snapshot.phase = phase;
        assert!(directory_page(&snapshot, &request).is_err());
    }
    snapshot.phase = TaskPhase::Finished;
    let job = &mut Arc::make_mut(snapshot.batch.as_mut().unwrap()).jobs[0];
    let JobState::Succeeded(report) = &job.state else {
        panic!("real result expected")
    };
    let mut no_gain = report.clone();
    match &mut no_gain {
        pixofold_core::batch::ImageReport::Png(r) => r.outcome = ProcessingOutcome::NoGain,
        _ => panic!("expected PNG"),
    }
    job.state = JobState::NoGain(no_gain);
    assert_eq!(directory_page(&snapshot, &request).unwrap().total, 0);
    assert!(
        resolve_directory(
            &snapshot,
            &OpenOutputDirectoryRequest {
                batch: request.batch,
                job_id: 1
            }
        )
        .is_err()
    );
    Arc::make_mut(snapshot.batch.as_mut().unwrap()).jobs[0].state = JobState::Queued;
    assert!(directory_page(&snapshot, &request).is_err());
}

#[test]
fn directory_access_refuses_files_and_symlink_components() {
    let dir = tempfile::tempdir().unwrap();
    png(&dir.path().join("a.png"));
    assert_eq!(
        validate_path(&dir.path().join("a.png"), true),
        Err(AssetError::UnsafePath)
    );
    assert_eq!(
        validate_path(Path::new("relative"), true),
        Err(AssetError::UnsafePath)
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.path(), dir.path().join("alias")).unwrap();
        assert_eq!(
            validate_path(&dir.path().join("alias"), true),
            Err(AssetError::UnsafePath)
        );
    }
}
