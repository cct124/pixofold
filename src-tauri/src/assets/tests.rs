//! 真实PNG与隔离文件的展示边界回归；系统文件管理器另交原生手测。
use super::*;
use crate::{
    ipc::DecimalU64,
    tasks::{TaskConfig, TaskRuntime, TaskSettings},
};
use pixofold_core::{
    batch::{BatchParameters, JobErrorCode, JobFailure},
    import::ImportOutput,
};
use std::{
    fs,
    io::Cursor,
    time::{Duration, Instant},
};

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/png")
            .join(name),
    )
    .unwrap()
}
fn request(snapshot: &TaskSnapshot) -> JobAssetRequest {
    let job = &snapshot.batch.as_ref().unwrap().jobs[0];
    JobAssetRequest {
        subscription_id: DecimalU64(1),
        selection_id: DecimalU64(snapshot.selection.unwrap().get()),
        job_id: job.id.get() as u32,
        attempt: job.attempt,
        expected_state: AssetState::Succeeded,
    }
}
fn completed(output: ImportOutput, dir: &Path) -> (TaskRuntime, TaskSnapshot) {
    let source = dir.join("中文, image.png");
    fs::write(&source, fixture("rgb8.png")).unwrap();
    let runtime = TaskRuntime::new(TaskConfig::default()).unwrap();
    let control = runtime.control();
    control
        .import(
            vec![source],
            Some(TaskSettings {
                output,
                parameters: BatchParameters::default(),
            }),
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut snapshot = control.snapshot();
    while snapshot.phase != TaskPhase::Finished {
        snapshot = control
            .wait_for_change(
                snapshot.revision,
                deadline.saturating_duration_since(Instant::now()),
            )
            .unwrap();
    }
    assert_eq!(snapshot.batch.as_ref().unwrap().summary.succeeded, 1);
    (runtime, snapshot)
}

#[test]
fn png_variants_decode_to_small_sanitized_previews_without_mutating_input() {
    for name in [
        "rgb8.png",
        "rgba8.png",
        "rgb16.png",
        "rgba16.png",
        "gray1.png",
        "gray8.png",
        "gray16.png",
        "gray-alpha8.png",
        "gray-alpha16.png",
        "indexed1.png",
        "indexed2.png",
        "indexed4.png",
        "indexed8.png",
        "trns-rgb8.png",
        "trns-gray16.png",
        "adam7-rgba8.png",
        "icc-rgb8.png",
        "content-credentials.png",
    ] {
        let original = fixture(name);
        let image = png::decode(&original).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(fixture(name), original);
        assert!((1..=png::MAX_WIDTH).contains(&image.width));
        assert!((1..=png::MAX_HEIGHT).contains(&image.height));
        assert!(image.png.len() <= png::MAX_PNG_BYTES);
        let mut reader = ::png::Decoder::new(Cursor::new(&image.png))
            .read_info()
            .unwrap();
        assert_eq!(reader.info().width, image.width);
        assert_eq!(reader.info().height, image.height);
        assert!(reader.info().icc_profile.is_none());
        let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut bytes).unwrap();
        reader.finish().unwrap();
        let mut offset = 8;
        while offset < image.png.len() {
            let length =
                u32::from_be_bytes(image.png[offset..offset + 4].try_into().unwrap()) as usize;
            assert!(
                [b"IHDR", b"IDAT", b"IEND"]
                    .contains(&image.png[offset + 4..offset + 8].try_into().unwrap())
            );
            offset += length + 12;
        }
    }
}

#[test]
fn transparent_resampling_does_not_mix_in_hidden_rgb_and_keeps_aspect_ratio() {
    let mut input = Vec::new();
    {
        let mut encoder = ::png::Encoder::new(&mut input, 256, 2);
        encoder.set_color(::png::ColorType::Rgba);
        encoder.set_depth(::png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let pixels: Vec<u8> = (0..256)
            .flat_map(|_| [255, 0, 0, 255, 0, 255, 0, 0])
            .collect();
        writer.write_image_data(&pixels).unwrap();
    }
    let image = png::decode(&input).unwrap();
    assert_eq!((image.width, image.height), (128, 1));
    let mut reader = ::png::Decoder::new(Cursor::new(image.png))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut pixels).unwrap();
    assert!(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [255, 0, 0, 127])
    );
}

#[test]
fn malformed_animated_and_oversized_images_fail_without_an_unbounded_decode() {
    for name in [
        "fake.png",
        "truncated.png",
        "bad-crc.png",
        "bad-deflate.png",
        "animated.png",
        "animation-after-idat.png",
    ] {
        assert!(png::decode(&fixture(name)).is_err(), "{name}");
    }
    let mut header = fixture("rgb8.png");
    header[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(png::decode(&header).unwrap_err(), AssetError::ResourceLimit);
    header[16..20].copy_from_slice(&4096_u32.to_be_bytes());
    header[20..24].copy_from_slice(&4096_u32.to_be_bytes());
    assert_eq!(png::decode(&header).unwrap_err(), AssetError::ResourceLimit);
    assert_eq!(
        png::decode(&vec![0; png::MAX_INPUT_BYTES as usize + 1]).unwrap_err(),
        AssetError::ResourceLimit
    );
}

#[test]
fn resolver_uses_committed_output_and_optional_backup_not_a_requested_copy_name() {
    for backup in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let policy = if backup {
            ImportOutput::Overwrite
        } else {
            ImportOutput::OverwriteWithoutBackup
        };
        let (_runtime, mut snapshot) = completed(policy, dir.path());
        let mut req = request(&snapshot);
        let result = resolve(&snapshot, &req, Some(RevealTarget::Result)).unwrap();
        assert!(result.is_file());
        let original_backup = resolve(&snapshot, &req, Some(RevealTarget::Backup));
        assert_eq!(original_backup.is_ok(), backup);
        if let Ok(path) = original_backup {
            assert_eq!(fs::read(path).unwrap(), fixture("rgb8.png"));
        }
        assert_eq!(resolve(&snapshot, &req, None).unwrap(), result);
        let batch = Arc::make_mut(snapshot.batch.as_mut().unwrap());
        let job = &mut batch.jobs[0];
        let JobState::Succeeded(report) = &job.state else {
            panic!("expected report")
        };
        let mut no_gain = report.clone();
        match &mut no_gain {
            pixofold_core::batch::ImageReport::Png(r) => r.outcome = ProcessingOutcome::NoGain,
            _ => panic!("expected PNG"),
        }
        job.state = JobState::NoGain(no_gain);
        req.expected_state = AssetState::NoGain;
        assert_eq!(
            resolve(&snapshot, &req, Some(RevealTarget::Result)).unwrap(),
            result
        );
        assert_eq!(
            resolve(&snapshot, &req, Some(RevealTarget::Backup)),
            Err(AssetError::Unavailable)
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("output");
    fs::create_dir(&output).unwrap();
    let (_runtime, snapshot) = completed(
        ImportOutput::CopyTo {
            directory: output.clone(),
            layout: pixofold_core::import::CopyLayout::Flat,
        },
        dir.path(),
    );
    assert_eq!(
        resolve(&snapshot, &request(&snapshot), None)
            .unwrap()
            .parent()
            .unwrap(),
        fs::canonicalize(output).unwrap()
    );
}

#[test]
fn stale_attempt_selection_state_and_nonterminal_phases_cannot_resolve_assets() {
    let dir = tempfile::tempdir().unwrap();
    let (_runtime, mut snapshot) = completed(ImportOutput::OverwriteWithoutBackup, dir.path());
    let valid = request(&snapshot);
    for bad in [
        JobAssetRequest {
            selection_id: DecimalU64(999),
            ..valid
        },
        JobAssetRequest {
            job_id: 999,
            ..valid
        },
        JobAssetRequest {
            attempt: 2,
            ..valid
        },
        JobAssetRequest {
            expected_state: AssetState::Failed,
            ..valid
        },
    ] {
        assert_eq!(resolve(&snapshot, &bad, None), Err(AssetError::StaleTask));
    }
    for phase in [
        TaskPhase::Preparing,
        TaskPhase::Clearing,
        TaskPhase::Closing,
        TaskPhase::Closed,
        TaskPhase::Idle,
    ] {
        snapshot.phase = phase;
        assert_eq!(resolve(&snapshot, &valid, None), Err(AssetError::StaleTask));
    }
    snapshot.phase = TaskPhase::Running;
    Arc::make_mut(snapshot.batch.as_mut().unwrap()).jobs[0].state = JobState::Queued;
    assert_eq!(
        resolve(&snapshot, &valid, None),
        Err(AssetError::Unavailable)
    );
}

#[test]
fn failed_commit_can_locate_its_recovery_backup_but_not_a_fake_result() {
    let dir = tempfile::tempdir().unwrap();
    let (_runtime, mut snapshot) = completed(ImportOutput::OverwriteWithoutBackup, dir.path());
    let mut req = request(&snapshot);
    req.expected_state = AssetState::Failed;
    let backup = dir.path().join("saved-backup-abc123.png");
    let job = &mut Arc::make_mut(snapshot.batch.as_mut().unwrap()).jobs[0];
    job.state = JobState::Failed(JobFailure {
        code: JobErrorCode::CleanupFailed,
        cause: Some(Arc::new(
            ProcessingError::CleanupFailed {
                temporary: dir.path().join("temp"),
                source: std::io::Error::other("private error"),
                original: Some(Box::new(ProcessingError::CommitFailed {
                    backup: backup.clone(),
                    source: std::io::Error::other("private path"),
                })),
            }
            .into(),
        )),
    });
    assert_eq!(
        resolve(&snapshot, &req, Some(RevealTarget::Backup)).unwrap(),
        backup
    );
    assert_eq!(
        resolve(&snapshot, &req, Some(RevealTarget::Result)),
        Err(AssetError::Unavailable)
    );
}

#[test]
fn cache_is_bounded_single_decoder_is_admitted_and_invalidation_rejects_late_results() {
    let dir = tempfile::tempdir().unwrap();
    let (_runtime, snapshot) = completed(ImportOutput::OverwriteWithoutBackup, dir.path());
    let req = request(&snapshot);
    let path = resolve(&snapshot, &req, None).unwrap();
    let service = Arc::new(AssetService::default());
    let permit = service.reserve(Operation::Thumbnail).unwrap();
    assert_eq!(
        service.reserve(Operation::Thumbnail).err(),
        Some(AssetError::Busy)
    );
    let reveal = service.reserve(Operation::Reveal).unwrap();
    assert_eq!(
        service.reserve(Operation::Reveal).err(),
        Some(AssetError::Busy)
    );
    drop(reveal);
    for id in 1..=80 {
        let mut prepared = service.prepare(&permit, req, path.clone()).unwrap();
        prepared.request.job_id = id;
        // 注入最大合法载荷，单独验证缓存额度；PNG有效性由上一组真实解码用例覆盖。
        prepared.image.png.resize(png::MAX_PNG_BYTES, 0);
        service.finish(&permit, prepared).unwrap();
    }
    {
        let state = service.state.lock().unwrap();
        assert_eq!(state.cache.len(), MAX_CACHE_ENTRIES);
        assert_eq!(state.cache_bytes, MAX_CACHE_BYTES);
    }
    let late = service.prepare(&permit, req, path).unwrap();
    service.invalidate();
    assert_eq!(service.state.lock().unwrap().cache_bytes, 0);
    assert_eq!(
        service.finish(&permit, late).unwrap_err(),
        AssetError::StaleTask
    );
    drop(permit);
    service.close();
    service.wait_idle();
    assert_eq!(
        service.reserve(Operation::Thumbnail).err(),
        Some(AssetError::Unavailable)
    );
}

#[test]
fn cached_files_are_rechecked_and_preview_never_changes_original_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let (_runtime, snapshot) = completed(ImportOutput::OverwriteWithoutBackup, dir.path());
    let req = request(&snapshot);
    let path = resolve(&snapshot, &req, None).unwrap();
    let original = fs::read(&path).unwrap();
    let service = Arc::new(AssetService::default());
    let permit = service.reserve(Operation::Thumbnail).unwrap();
    let first = service
        .finish(
            &permit,
            service.prepare(&permit, req, path.clone()).unwrap(),
        )
        .unwrap();
    let cached = service
        .finish(
            &permit,
            service.prepare(&permit, req, path.clone()).unwrap(),
        )
        .unwrap();
    assert_eq!(first.png, cached.png);
    assert_eq!(fs::read(&path).unwrap(), original);
    fs::write(&path, fixture("rgba16.png")).unwrap();
    let changed = service.prepare(&permit, req, path.clone()).unwrap();
    assert_ne!(changed.image.png, cached.png);
    fs::remove_file(&path).unwrap();
    assert_eq!(
        service.prepare(&permit, req, path.clone()).err(),
        Some(AssetError::FileMissing)
    );
    fs::create_dir(&path).unwrap();
    assert_eq!(validate_file(&path), Err(AssetError::UnsafePath));
    assert_eq!(
        validate_file(Path::new("relative.png")),
        Err(AssetError::UnsafePath)
    );
}

#[cfg(unix)]
#[test]
fn symlinks_and_linked_parent_directories_are_not_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.png");
    fs::write(&path, fixture("rgb8.png")).unwrap();
    let link = dir.path().join("link.png");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert_eq!(validate_file(&link), Err(AssetError::UnsafePath));
    let linked = dir.path().join("linked");
    std::os::unix::fs::symlink(dir.path(), &linked).unwrap();
    assert_eq!(
        validate_file(&linked.join("image.png")),
        Err(AssetError::UnsafePath)
    );
}
