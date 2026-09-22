//! 所有写入均在隔离目录；回调提供确定性故障/取消注入，不依赖休眠或网络。

use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

use pixofold_core::{
    model::{
        ByteCount, CancellationToken, OutputPolicy, PngRequest, ProcessingError, ProcessingOutcome,
        ProcessingStage, ResourceLimits,
    },
    pipeline::optimize_png,
    probe::inspect_png,
};
use tempfile::TempDir;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/png")
}

fn fixture(name: &str) -> Vec<u8> {
    fs::read(fixtures().join(name)).expect("读取已提交的自生成语料")
}

fn workspace(name: &str) -> (TempDir, PngRequest, Vec<u8>) {
    let directory = tempfile::tempdir().expect("隔离目录");
    let source = directory.path().join("输入图片.png");
    let original = fixture(name);
    fs::write(&source, &original).expect("写测试源图");
    (directory, PngRequest::new(source), original)
}

fn run(request: &PngRequest) -> Result<pixofold_core::model::ProcessingReport, ProcessingError> {
    optimize_png(request, &CancellationToken::default(), |_| {})
}

fn files(directory: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = fs::read_dir(directory)
        .expect("读取隔离目录")
        .map(|entry| entry.expect("读取目录项").path())
        .collect();
    files.sort();
    files
}

fn assert_source_only(directory: &TempDir, request: &PngRequest, original: &[u8]) {
    assert_eq!(fs::read(&request.source).expect("读取源图"), original);
    assert_eq!(
        files(directory.path()),
        vec![request.source.clone()],
        "不应遗留临时产物或备份"
    );
}

fn decoded(data: &[u8]) -> (png::OutputInfo, Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder.read_info().expect("独立解码头");
    let mut pixels = vec![0; reader.output_buffer_size().expect("解码大小")];
    let output = reader.next_frame(&mut pixels).expect("独立解码完整图像");
    pixels.truncate(output.buffer_size());
    reader.finish().expect("完整读取 IEND");
    let palette = reader
        .info()
        .palette
        .as_deref()
        .unwrap_or_default()
        .to_vec();
    let transparency = reader.info().trns.as_deref().unwrap_or_default().to_vec();
    (output, pixels, palette, transparency)
}

fn non_idat_chunks(data: &[u8]) -> Vec<Vec<u8>> {
    let mut chunks = Vec::new();
    let mut offset = 8;
    while offset < data.len() {
        let len = u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        let end = offset + len + 12;
        if &data[offset + 4..offset + 8] != b"IDAT" {
            chunks.push(data[offset..end].to_vec());
        }
        offset = end;
    }
    chunks
}

#[test]
fn every_static_fixture_preserves_samples_bit_depth_palette_and_source() {
    let manifest: serde_json::Value = serde_json::from_slice(&fixture("manifest.json")).unwrap();
    for entry in manifest["fixtures"].as_array().unwrap() {
        if !matches!(entry["expected"].as_str(), Some("static" | "no-gain")) {
            continue;
        }
        let name = entry["file"].as_str().unwrap();
        let (directory, mut request, original) = workspace(name);
        let destination = directory.path().join("output.png");
        request.output = OutputPolicy::Copy {
            destination: destination.clone(),
        };
        let report = run(&request).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(fs::read(&request.source).unwrap(), original, "{name}");
        assert_eq!(report.input_bytes.0, original.len() as u64);
        if entry["expected"] == "no-gain" {
            assert_eq!(report.outcome, ProcessingOutcome::NoGain);
            assert_eq!(report.output_bytes, report.input_bytes);
            assert!(!destination.exists());
        } else {
            assert!(
                matches!(
                    report.outcome,
                    ProcessingOutcome::Optimized { backup: None, .. }
                ),
                "{name}"
            );
            let output = fs::read(&destination).unwrap();
            assert_eq!(
                non_idat_chunks(&original),
                non_idat_chunks(&output),
                "{name}: 完整元数据及其位置顺序"
            );
            assert_eq!(output.len() as u64, report.output_bytes.0);
            assert!(report.output_bytes < report.input_bytes);
            let (before, pixels, palette, trns) = decoded(&original);
            let (after, actual, actual_palette, actual_trns) = decoded(&output);
            assert_eq!(
                (
                    before.width,
                    before.height,
                    before.bit_depth,
                    before.color_type
                ),
                (after.width, after.height, after.bit_depth, after.color_type),
                "{name}"
            );
            assert_eq!(pixels, actual, "{name}: 含隐藏 RGB 的原始样本");
            assert_eq!(
                (palette, trns),
                (actual_palette, actual_trns),
                "{name}: 调色板和透明信息"
            );
            assert_eq!(files(directory.path()).len(), 2);
        }
    }
}

#[test]
fn overwrite_keeps_exact_recoverable_backup_and_real_sizes() {
    let (directory, request, original) = workspace("rgba16.png");
    let mut stages = Vec::new();
    let report = optimize_png(&request, &CancellationToken::default(), |stage| {
        stages.push(stage)
    })
    .unwrap();
    let ProcessingOutcome::Optimized {
        output,
        backup: Some(backup),
    } = report.outcome
    else {
        panic!("必须覆盖并备份")
    };
    assert_eq!(output, request.source.canonicalize().unwrap());
    assert_eq!(fs::read(&backup).unwrap(), original);
    assert_eq!(
        fs::metadata(&request.source).unwrap().len(),
        report.output_bytes.0
    );
    assert_eq!(report.input_bytes.0, original.len() as u64);
    assert!(report.output_bytes < report.input_bytes);
    assert_eq!(files(directory.path()).len(), 2);
    assert_eq!(
        stages,
        [
            ProcessingStage::Reading,
            ProcessingStage::Optimizing,
            ProcessingStage::Validating,
            ProcessingStage::BeforeCommit
        ]
    );
}

#[test]
fn no_gain_does_not_overwrite_or_make_a_backup() {
    let (directory, request, original) = workspace("already-optimized.png");
    let before = fs::metadata(&request.source).unwrap().modified().unwrap();
    let report = run(&request).unwrap();
    assert_eq!(report.outcome, ProcessingOutcome::NoGain);
    assert_eq!(report.output_bytes, report.input_bytes);
    assert_source_only(&directory, &request, &original);
    assert_eq!(
        fs::metadata(&request.source).unwrap().modified().unwrap(),
        before
    );
}

#[test]
fn invalid_inputs_and_both_animation_layouts_never_write() {
    for name in [
        "fake.png",
        "truncated.png",
        "bad-crc.png",
        "bad-deflate.png",
        "trailing-data.png",
        "animated.png",
        "animation-after-idat.png",
    ] {
        let (directory, request, original) = workspace(name);
        let error = run(&request).unwrap_err();
        match name {
            "fake.png" => assert!(matches!(error, ProcessingError::UnsupportedFormat)),
            "animated.png" | "animation-after-idat.png" => {
                assert!(matches!(error, ProcessingError::UnsupportedAnimation))
            }
            _ => assert!(
                matches!(
                    error,
                    ProcessingError::InvalidPng(_) | ProcessingError::Decode(_)
                ),
                "{name}: {error:?}"
            ),
        }
        assert_source_only(&directory, &request, &original);
    }
}

#[test]
fn resource_limits_reject_before_writing() {
    for limits in [
        ResourceLimits {
            max_input_bytes: ByteCount(100),
            ..ResourceLimits::default()
        },
        ResourceLimits {
            max_decoded_bytes: ByteCount(100),
            ..ResourceLimits::default()
        },
        ResourceLimits {
            max_pixels: 1,
            ..ResourceLimits::default()
        },
        ResourceLimits {
            max_dimension: 1,
            ..ResourceLimits::default()
        },
    ] {
        let (directory, mut request, original) = workspace("rgb8.png");
        request.limits = limits;
        assert!(matches!(
            run(&request),
            Err(ProcessingError::ResourceLimit(_))
        ));
        assert_source_only(&directory, &request, &original);
    }
    let (_, mut request, _) = workspace("rgb8.png");
    request.limits.max_input_bytes = ByteCount(0);
    assert!(matches!(run(&request), Err(ProcessingError::InvalidLimits)));
}

#[test]
fn same_name_alias_directory_and_late_copy_conflicts_preserve_existing_files() {
    let (directory, mut request, original) = workspace("rgb8.png");
    request.output = OutputPolicy::Copy {
        destination: request.source.clone(),
    };
    assert!(matches!(
        run(&request),
        Err(ProcessingError::TargetConflict)
    ));
    let target = directory.path().join("already.png");
    fs::hard_link(&request.source, &target).unwrap();
    request.output = OutputPolicy::Copy {
        destination: target.clone(),
    };
    assert!(matches!(
        run(&request),
        Err(ProcessingError::TargetConflict)
    ));
    assert_eq!(fs::read(&target).unwrap(), original);
    fs::remove_file(&target).unwrap();
    fs::create_dir(&target).unwrap();
    assert!(matches!(
        run(&request),
        Err(ProcessingError::TargetConflict)
    ));
    fs::remove_dir(&target).unwrap();
    let result = optimize_png(&request, &CancellationToken::default(), |stage| {
        if stage == ProcessingStage::BeforeCommit {
            fs::write(&target, b"other application").unwrap();
        }
    });
    assert!(matches!(result, Err(ProcessingError::TargetConflict)));
    assert_eq!(fs::read(&target).unwrap(), b"other application");
    assert_eq!(fs::read(&request.source).unwrap(), original);
    assert_eq!(files(directory.path()).len(), 2);
}

#[test]
fn cancellation_at_every_stage_cleans_candidates_and_provisional_backup() {
    for stage_to_cancel in [
        ProcessingStage::Reading,
        ProcessingStage::Optimizing,
        ProcessingStage::Validating,
        ProcessingStage::BeforeCommit,
    ] {
        let (directory, request, original) = workspace("rgb8.png");
        let cancel = CancellationToken::default();
        let result = optimize_png(&request, &cancel, |stage| {
            if stage == stage_to_cancel {
                cancel.cancel();
            }
        });
        assert!(
            matches!(result, Err(ProcessingError::Cancelled)),
            "{stage_to_cancel:?}"
        );
        assert_source_only(&directory, &request, &original);
    }
    let (directory, request, original) = workspace("rgb8.png");
    let cancel = CancellationToken::default();
    cancel.cancel();
    let result = optimize_png(&request, &cancel, |_| panic!("预取消不能开始读取"));
    assert!(matches!(result, Err(ProcessingError::Cancelled)));
    assert_source_only(&directory, &request, &original);
}

#[test]
fn same_length_source_change_with_restored_timestamp_is_detected() {
    for change_stage in [ProcessingStage::Optimizing, ProcessingStage::BeforeCommit] {
        let (directory, request, mut external) = workspace("rgb8.png");
        let original_time = fs::metadata(&request.source).unwrap().modified().unwrap();
        external[60] ^= 1;
        let result = optimize_png(&request, &CancellationToken::default(), |stage| {
            if stage == change_stage {
                fs::write(&request.source, &external).unwrap();
                let file = fs::OpenOptions::new()
                    .write(true)
                    .open(&request.source)
                    .unwrap();
                file.set_times(fs::FileTimes::new().set_modified(original_time))
                    .unwrap();
            }
        });
        assert!(
            matches!(result, Err(ProcessingError::SourceChanged)),
            "{result:?}"
        );
        assert_source_only(&directory, &request, &external);
    }
}

#[test]
fn removed_source_is_not_recreated_by_commit() {
    let (directory, request, _) = workspace("rgb8.png");
    let result = optimize_png(&request, &CancellationToken::default(), |stage| {
        if stage == ProcessingStage::BeforeCommit {
            fs::remove_file(&request.source).unwrap();
        }
    });
    assert!(matches!(result, Err(ProcessingError::SourceChanged)));
    assert!(files(directory.path()).is_empty());
}

#[test]
fn replacing_source_with_identical_bytes_is_detected_by_file_identity() {
    let (directory, request, original) = workspace("rgb8.png");
    let moved = directory.path().join("externally-moved.png");
    let modified = fs::metadata(&request.source).unwrap().modified().unwrap();
    let result = optimize_png(&request, &CancellationToken::default(), |stage| {
        if stage == ProcessingStage::BeforeCommit {
            fs::rename(&request.source, &moved).unwrap();
            fs::write(&request.source, &original).unwrap();
            let file = fs::OpenOptions::new()
                .write(true)
                .open(&request.source)
                .unwrap();
            file.set_times(fs::FileTimes::new().set_modified(modified))
                .unwrap();
        }
    });
    assert!(matches!(result, Err(ProcessingError::SourceChanged)));
    assert_eq!(fs::read(&request.source).unwrap(), original);
    assert_eq!(fs::read(moved).unwrap(), original);
    assert_eq!(files(directory.path()).len(), 2);
}

#[test]
fn changes_to_validated_candidate_or_backup_are_rejected_before_commit() {
    for change_backup in [false, true] {
        let (directory, request, original) = workspace("rgb8.png");
        let result = optimize_png(&request, &CancellationToken::default(), |stage| {
            if stage == ProcessingStage::BeforeCommit {
                let other_files: Vec<_> = files(directory.path())
                    .into_iter()
                    .filter(|path| path != &request.source)
                    .collect();
                assert_eq!(other_files.len(), 2);
                let target = other_files
                    .into_iter()
                    .find(|path| (fs::read(path).unwrap() == original) == change_backup)
                    .unwrap();
                fs::write(target, b"external modification").unwrap();
            }
        });
        assert!(
            matches!(result, Err(ProcessingError::ValidationFailed(_))),
            "{result:?}"
        );
        assert_source_only(&directory, &request, &original);
    }
}

#[test]
fn unknown_unsafe_to_copy_metadata_is_not_silently_invalidated() {
    let (directory, request, original) = workspace("rgb8.png");
    for name in [b"caBX", b"iDOT", b"vpAG"] {
        let mut chunk = vec![0, 0, 0, 4];
        chunk.extend_from_slice(name);
        chunk.extend_from_slice(b"test");
        chunk.extend_from_slice(&crc32fast::hash(&chunk[4..]).to_be_bytes());
        let mut bytes = original[..33].to_vec();
        bytes.extend_from_slice(&chunk);
        bytes.extend_from_slice(&original[33..]);
        fs::write(&request.source, &bytes).unwrap();
        assert!(matches!(
            run(&request),
            Err(ProcessingError::ValidationFailed(_))
        ));
        assert_source_only(&directory, &request, &bytes);
    }
}

#[test]
fn corrupted_or_valid_but_different_temporary_output_is_never_committed() {
    for replacement in [b"invalid PNG".to_vec(), fixture("gray8.png")] {
        let (directory, request, original) = workspace("rgb8.png");
        let result = optimize_png(&request, &CancellationToken::default(), |stage| {
            if stage == ProcessingStage::Validating {
                let candidate = files(directory.path())
                    .into_iter()
                    .find(|path| path != &request.source)
                    .unwrap();
                fs::write(candidate, &replacement).unwrap();
            }
        });
        assert!(
            matches!(
                result,
                Err(ProcessingError::UnsupportedFormat | ProcessingError::ValidationFailed(_))
            ),
            "{result:?}"
        );
        assert_source_only(&directory, &request, &original);
    }
}

#[test]
fn readonly_source_and_missing_output_directory_fail_without_modification() {
    let (directory, mut request, original) = workspace("rgb8.png");
    let permissions = fs::metadata(&request.source).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&request.source, readonly).unwrap();
    let result = run(&request);
    fs::set_permissions(&request.source, permissions).unwrap();
    assert!(matches!(result, Err(ProcessingError::Io { .. })));
    assert_source_only(&directory, &request, &original);
    request.output = OutputPolicy::Copy {
        destination: directory.path().join("missing/output.png"),
    };
    assert!(matches!(run(&request), Err(ProcessingError::Io { .. })));
    assert_source_only(&directory, &request, &original);
}

#[test]
fn readonly_input_can_be_copied_and_no_gain_still_cleans_up() {
    for name in ["rgb8.png", "already-optimized.png"] {
        let (directory, mut request, original) = workspace(name);
        let permissions = fs::metadata(&request.source).unwrap().permissions();
        let mut readonly = permissions.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&request.source, readonly).unwrap();
        let destination = directory.path().join("copy.png");
        request.output = OutputPolicy::Copy {
            destination: destination.clone(),
        };
        let result = run(&request);
        fs::set_permissions(&request.source, permissions).unwrap();
        let report = result.unwrap();
        assert_eq!(fs::read(&request.source).unwrap(), original);
        if name == "already-optimized.png" {
            assert_eq!(report.outcome, ProcessingOutcome::NoGain);
            assert_source_only(&directory, &request, &original);
        } else {
            assert!(destination.exists());
            assert_eq!(files(directory.path()).len(), 2);
        }
    }
}

#[cfg(windows)]
#[test]
fn windows_alternate_stream_and_ambiguous_leaf_names_are_rejected() {
    let (directory, mut request, original) = workspace("rgb8.png");
    for leaf in ["copy.png:other", "copy.png.", "copy.png "] {
        request.output = OutputPolicy::Copy {
            destination: directory.path().join(leaf),
        };
        assert!(matches!(run(&request), Err(ProcessingError::InvalidPath)));
        assert_source_only(&directory, &request, &original);
    }
}

#[cfg(windows)]
#[test]
fn windows_file_in_use_keeps_source_and_recovery_backup() {
    use std::os::windows::fs::OpenOptionsExt;
    let (directory, request, original) = workspace("rgb8.png");
    // FILE_SHARE_READ 允许重新验证，但禁止重命名/替换，模拟外部程序占用。
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&request.source)
        .unwrap();
    let error = run(&request).unwrap_err();
    drop(held);
    let ProcessingError::CommitFailed { backup, .. } = error else {
        panic!("{error:?}")
    };
    assert_eq!(fs::read(&backup).unwrap(), original);
    assert_eq!(fs::read(&request.source).unwrap(), original);
    assert_eq!(files(directory.path()).len(), 2);
}

#[cfg(windows)]
#[test]
fn windows_unreadable_file_returns_io_failure_without_writes() {
    use std::os::windows::fs::OpenOptionsExt;
    let (directory, request, original) = workspace("rgb8.png");
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&request.source)
        .unwrap();
    let result = run(&request);
    drop(held);
    assert!(matches!(result, Err(ProcessingError::Io { .. })));
    assert_source_only(&directory, &request, &original);
}

#[cfg(windows)]
#[test]
fn cleanup_failure_returns_remaining_path_and_original_cancellation() {
    use std::os::windows::fs::OpenOptionsExt;
    let (directory, request, original) = workspace("rgb8.png");
    let cancel = CancellationToken::default();
    let mut held = None;
    let result = optimize_png(&request, &cancel, |stage| {
        if stage == ProcessingStage::Validating {
            let candidate = files(directory.path())
                .into_iter()
                .find(|path| path != &request.source)
                .unwrap();
            held = Some(
                fs::OpenOptions::new()
                    .read(true)
                    .share_mode(3)
                    .open(candidate)
                    .unwrap(),
            );
            cancel.cancel();
        }
    });
    drop(held);
    let ProcessingError::CleanupFailed {
        original: cause,
        temporary,
        ..
    } = result.unwrap_err()
    else {
        panic!("必须报告清理失败")
    };
    assert!(matches!(cause.as_deref(), Some(ProcessingError::Cancelled)));
    assert!(temporary.exists());
    fs::remove_file(temporary).unwrap();
    assert_source_only(&directory, &request, &original);
}

#[cfg(unix)]
#[test]
fn symlink_sources_and_dangling_destinations_are_rejected() {
    use std::os::unix::fs::symlink;
    let (directory, mut request, original) = workspace("rgb8.png");
    let link = directory.path().join("link.png");
    symlink(&request.source, &link).unwrap();
    assert!(matches!(
        run(&PngRequest::new(&link)),
        Err(ProcessingError::InvalidPath)
    ));
    fs::remove_file(&link).unwrap();
    symlink(directory.path().join("missing"), &link).unwrap();
    request.output = OutputPolicy::Copy { destination: link };
    assert!(matches!(
        run(&request),
        Err(ProcessingError::TargetConflict)
    ));
    assert_eq!(fs::read(&request.source).unwrap(), original);
}

#[test]
fn bounded_parser_handles_all_truncations_and_invalid_header_combinations() {
    let data = fixture("already-optimized.png");
    for end in 0..data.len() {
        assert!(
            inspect_png(&data[..end], ResourceLimits::default()).is_err(),
            "长度 {end}"
        );
    }
    for (index, value) in [(24, 0), (24, 32), (25, 1), (26, 1), (27, 1), (28, 2)] {
        let mut bad = data.clone();
        bad[index] = value;
        let crc = crc32fast::hash(&bad[12..29]).to_be_bytes();
        bad[29..33].copy_from_slice(&crc);
        assert!(inspect_png(&bad, ResourceLimits::default()).is_err());
    }
    let mut large = data.clone();
    large[16..24].fill(255);
    large[24] = 16;
    large[25] = 6;
    let crc = crc32fast::hash(&large[12..29]).to_be_bytes();
    large[29..33].copy_from_slice(&crc);
    let limits = ResourceLimits {
        max_dimension: u32::MAX,
        max_pixels: u64::MAX,
        ..ResourceLimits::default()
    };
    assert!(matches!(
        inspect_png(&large, limits),
        Err(ProcessingError::ResourceLimit(_))
    ));
}
