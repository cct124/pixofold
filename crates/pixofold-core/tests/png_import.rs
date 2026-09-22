//! 只读导入、输出规划与真实批量闭环；所有写入隔离临时目录，取消由回调确定时序。

use pixofold_core::{
    batch::{
        BatchConfig, BatchError, BatchParameters, BatchService, JobErrorCode, JobState,
        PathConflictKind,
    },
    import::*,
    model::{
        ByteCount, CancellationToken, OutputPolicy, PngMode, PngRequest, ProcessingError,
        ProcessingOutcome, ProcessingStage, QualityValue,
    },
    pipeline::optimize_png,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/png")
            .join(name),
    )
    .unwrap()
}
fn sample(root: &Path, relative: &str, name: &str) -> PathBuf {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, fixture(name)).unwrap();
    path
}
fn collect(roots: &[PathBuf]) -> ImportScan {
    scan(
        roots,
        ScanOptions::default(),
        &CancellationToken::default(),
        |_| {},
    )
    .unwrap()
}
fn run(scan: &ImportScan, output: ImportOutput) -> pixofold_core::batch::BatchSnapshot {
    let request = scan.plan(&output, BatchParameters::default()).unwrap();
    let service = BatchService::new(BatchConfig {
        workers: 2,
        ..BatchConfig::default()
    })
    .unwrap();
    let id = service.start(request).unwrap();
    service.wait(id, Duration::from_secs(30)).unwrap()
}

#[test]
fn nested_overlapping_roots_and_hardlinks_keep_stable_first_ownership() {
    let dir = tempfile::tempdir().unwrap();
    let a = sample(dir.path(), "a.data", "rgb8.png");
    let b = sample(dir.path(), "sub/b.png", "rgba8.png");
    fs::hard_link(&a, dir.path().join("z-alias.png")).unwrap();
    let roots = [dir.path().to_owned(), dir.path().join("sub"), a.clone()];
    let found = collect(&roots);
    assert_eq!(found.progress().status, ScanStatus::Complete);
    assert_eq!(found.files().len(), 2);
    assert_eq!(found.progress().duplicates, 3);
    assert_eq!(found.files()[0].source, a.canonicalize().unwrap());
    assert_eq!(found.files()[1].source, b.canonicalize().unwrap());
    assert_eq!(found.files()[1].relative_path, Path::new("sub/b.png"));
    assert!(
        found
            .files()
            .iter()
            .all(|f| f.root_is_directory && f.root == dir.path().canonicalize().unwrap())
    );
    let again = collect(&roots);
    assert_eq!(
        found.files().iter().map(|f| &f.source).collect::<Vec<_>>(),
        again.files().iter().map(|f| &f.source).collect::<Vec<_>>()
    );
    let first_file = collect(&[b, dir.path().to_owned()]);
    assert!(!first_file.files()[0].root_is_directory);
}

#[test]
fn classification_uses_content_and_rejects_animation_and_bad_structure() {
    let dir = tempfile::tempdir().unwrap();
    sample(dir.path(), "good.jpeg", "rgb8.png");
    for name in [
        "animated.png",
        "animation-after-idat.png",
        "bad-crc.png",
        "truncated.png",
    ] {
        sample(dir.path(), name, name);
    }
    for (name, bytes) in [
        ("jpeg.png", b"\xff\xd8\xffx".as_slice()),
        ("gif.png", b"GIF89a..".as_slice()),
        ("webp.png", b"RIFF....WEBP".as_slice()),
        ("empty.png", b"".as_slice()),
    ] {
        fs::write(dir.path().join(name), bytes).unwrap();
    }
    let found = collect(&[dir.path().to_owned()]);
    assert_eq!(found.files().len(), 1);
    assert_eq!(found.files()[0].source.file_name().unwrap(), "good.jpeg");
    assert_eq!(found.progress().rejected, 8);
    let code_count = |code| {
        found
            .issues()
            .iter()
            .filter(|i| matches!(&i.kind, ImportIssueKind::Failure(f) if f.code == code))
            .count()
    };
    assert_eq!(code_count(JobErrorCode::UnsupportedAnimation), 2);
    assert_eq!(code_count(JobErrorCode::InvalidInput), 2);
    for format in [
        UnsupportedFormat::Jpeg,
        UnsupportedFormat::Gif,
        UnsupportedFormat::WebP,
        UnsupportedFormat::Other,
    ] {
        assert!(
            found
                .issues()
                .iter()
                .any(|i| matches!(i.kind, ImportIssueKind::Unsupported(f) if f == format))
        );
    }
    let request = found
        .plan(&ImportOutput::CopyBeside, BatchParameters::default())
        .unwrap();
    let OutputPolicy::Copy { destination } = &request.items[0].output else {
        panic!("应为副本");
    };
    assert_eq!(destination.file_name().unwrap(), "good_compressed.png");
    assert!(!destination.exists());
}

#[test]
fn empty_inputs_and_empty_directories_never_create_batches() {
    let dir = tempfile::tempdir().unwrap();
    for found in [
        collect(&[]),
        collect(&[dir.path().to_owned()]),
        collect(&[dir.path().join("missing")]),
    ] {
        assert_eq!(found.progress().status, ScanStatus::Complete);
        assert!(found.files().is_empty());
        assert!(matches!(
            found.plan(&ImportOutput::Overwrite, BatchParameters::default()),
            Err(ImportError::NoFiles)
        ));
    }
}

#[test]
fn all_scan_limits_stop_with_non_startable_partial_results() {
    let dir = tempfile::tempdir().unwrap();
    let a = sample(dir.path(), "a.png", "rgb8.png");
    let b = sample(dir.path(), "sub/b.png", "rgba8.png");
    let defaults = ScanOptions::default();
    for (roots, options, limit) in [
        (
            vec![dir.path().to_owned()],
            ScanOptions {
                max_entries: 2,
                ..defaults
            },
            ScanLimit::Entries,
        ),
        (
            vec![a.clone(), b.clone()],
            ScanOptions {
                max_files: 1,
                ..defaults
            },
            ScanLimit::Files,
        ),
        (
            vec![dir.path().to_owned()],
            ScanOptions {
                max_depth: 0,
                ..defaults
            },
            ScanLimit::Depth,
        ),
        (
            vec![a.clone()],
            ScanOptions {
                max_read_bytes: ByteCount(12),
                ..defaults
            },
            ScanLimit::ReadBytes,
        ),
    ] {
        let found = scan(&roots, options, &CancellationToken::default(), |_| {}).unwrap();
        assert_eq!(found.progress().status, ScanStatus::Limited(limit));
        assert!(found.progress().discovered <= options.max_entries);
        assert!(found.files().len() <= options.max_files);
        assert!(found.progress().read_bytes <= options.max_read_bytes);
        assert!(matches!(
            found.plan(&ImportOutput::Overwrite, BatchParameters::default()),
            Err(ImportError::IncompleteScan)
        ));
    }
    let found = scan(
        std::slice::from_ref(&a),
        ScanOptions {
            max_files: 1,
            max_entries: 1,
            max_read_bytes: ByteCount(fs::metadata(&a).unwrap().len()),
            ..defaults
        },
        &CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(
        found.progress().status,
        ScanStatus::Complete,
        "恰好达到上限且没有额外输入时允许完整结束"
    );
}

#[test]
fn invalid_scan_options_and_root_overflow_fail_before_io() {
    for options in [
        ScanOptions {
            max_files: 0,
            ..ScanOptions::default()
        },
        ScanOptions {
            max_entries: 1_000_001,
            ..ScanOptions::default()
        },
        ScanOptions {
            max_depth: 257,
            ..ScanOptions::default()
        },
        ScanOptions {
            max_read_bytes: ByteCount(0),
            ..ScanOptions::default()
        },
    ] {
        assert!(matches!(
            scan(&[], options, &CancellationToken::default(), |_| {}),
            Err(ImportError::InvalidOptions)
        ));
    }
    let roots = vec![PathBuf::from("missing"); 1001];
    assert!(matches!(
        scan(
            &roots,
            ScanOptions::default(),
            &CancellationToken::default(),
            |_| {}
        ),
        Err(ImportError::TooManyRoots)
    ));
}

#[test]
fn cancellation_during_enumeration_and_reading_preserves_inputs_and_partial_feedback() {
    let dir = tempfile::tempdir().unwrap();
    let a = sample(dir.path(), "a.png", "gradient-rgb8.png");
    sample(dir.path(), "b.png", "rgba8.png");
    for during_read in [false, true] {
        let token = CancellationToken::default();
        let mut last = None;
        let found = scan(
            &[dir.path().to_owned()],
            ScanOptions::default(),
            &token,
            |p| {
                if let Some(previous) = last {
                    assert!(p.discovered >= previous);
                }
                last = Some(p.discovered);
                if (during_read && p.read_bytes.0 >= 12) || (!during_read && p.discovered >= 2) {
                    token.cancel();
                }
            },
        )
        .unwrap();
        assert_eq!(found.progress().status, ScanStatus::Cancelled);
        assert!(found.files().is_empty());
        assert!(matches!(
            found.plan(&ImportOutput::CopyBeside, BatchParameters::default()),
            Err(ImportError::IncompleteScan)
        ));
        assert_eq!(fs::read(&a).unwrap(), fixture("gradient-rgb8.png"));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }
    let token = CancellationToken::default();
    token.cancel();
    assert_eq!(
        scan(&[a], ScanOptions::default(), &token, |_| {})
            .unwrap()
            .progress()
            .examined,
        0
    );
}

#[test]
fn per_file_resource_failures_do_not_hide_other_candidates() {
    let dir = tempfile::tempdir().unwrap();
    sample(dir.path(), "large.png", "gradient-rgb8.png");
    sample(dir.path(), "small.png", "already-optimized.png");
    let mut options = ScanOptions::default();
    options.probe_limits.max_input_bytes = ByteCount(1024);
    let found = scan(
        &[dir.path().to_owned()],
        options,
        &CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(found.progress().status, ScanStatus::Complete);
    assert_eq!((found.files().len(), found.progress().rejected), (1, 1));
    assert!(
        matches!(&found.issues()[0].kind, ImportIssueKind::Failure(e) if e.code == JobErrorCode::ResourceLimit)
    );
}

#[test]
fn reserved_artifacts_are_explicit_and_not_all_hidden_or_compressed_images() {
    let dir = tempfile::tempdir().unwrap();
    for name in [
        ".pixofold-backup-Ab12xY.png",
        ".pixofold-output-Zy98aB.tmp",
        ".pixofold-backup-family-photo.png",
        ".hidden.png",
        "photo_compressed.png",
    ] {
        sample(dir.path(), name, "rgb8.png");
    }
    let found = collect(&[dir.path().to_owned()]);
    assert_eq!((found.files().len(), found.progress().excluded), (3, 2));
    let included = scan(
        &[dir.path().to_owned()],
        ScanOptions {
            include_artifacts: true,
            ..ScanOptions::default()
        },
        &CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(included.files().len(), 5);
}

#[test]
fn parameter_and_destination_errors_preserve_the_frozen_selection() {
    let dir = tempfile::tempdir().unwrap();
    let path = sample(dir.path(), "a.png", "rgb8.png");
    let found = collect(&[path]);
    let mut parameters = BatchParameters::default();
    parameters.limits.max_pixels = 0;
    assert!(matches!(
        found.plan(&ImportOutput::Overwrite, parameters),
        Err(ImportError::Batch(BatchError::InvalidParameters(_)))
    ));
    let target = dir.path().join("a_compressed.png");
    fs::write(&target, b"keep existing").unwrap();
    assert!(matches!(
        found.plan(&ImportOutput::CopyBeside, BatchParameters::default()),
        Err(ImportError::File { .. })
    ));
    assert_eq!(fs::read(target).unwrap(), b"keep existing");
    assert_eq!(found.files().len(), 1);
    parameters = BatchParameters {
        mode: PngMode::Lossy {
            quality: QualityValue::new(40).unwrap(),
        },
        ..BatchParameters::default()
    };
    assert_eq!(
        found
            .plan(&ImportOutput::Overwrite, parameters)
            .unwrap()
            .parameters
            .mode,
        parameters.mode
    );
}

#[test]
fn flat_collisions_root_collisions_and_output_input_overlap_reject_before_writes() {
    let dir = tempfile::tempdir().unwrap();
    let a = sample(dir.path(), "left/photos/same.png", "rgb8.png");
    let b = sample(dir.path(), "right/photos/same.jpg", "rgba8.png");
    let out = dir.path().join("out");
    fs::create_dir(&out).unwrap();
    let found = collect(&[
        a.parent().unwrap().to_owned(),
        b.parent().unwrap().to_owned(),
    ]);
    for layout in [CopyLayout::Flat, CopyLayout::PreserveRoots] {
        let result = found.plan(
            &ImportOutput::CopyTo {
                directory: out.clone(),
                layout,
            },
            BatchParameters::default(),
        );
        if layout == CopyLayout::Flat {
            assert!(matches!(
                result,
                Err(ImportError::Batch(BatchError::PathConflict {
                    kind: PathConflictKind::DuplicateOutput,
                    ..
                }))
            ));
        } else {
            assert!(matches!(result, Err(ImportError::RootNameConflict { .. })));
        }
        assert_eq!(fs::read_dir(&out).unwrap().count(), 0);
    }
    let already = sample(dir.path(), "left/photos/same_compressed.png", "rgba8.png");
    let found = collect(&[a.clone(), already.clone()]);
    assert!(matches!(
        found.plan(&ImportOutput::CopyBeside, BatchParameters::default()),
        Err(ImportError::Batch(BatchError::PathConflict {
            kind: PathConflictKind::OutputIsInput,
            ..
        }))
    ));
    assert_eq!(fs::read(a).unwrap(), fixture("rgb8.png"));
    assert_eq!(fs::read(already).unwrap(), fixture("rgba8.png"));
}

#[test]
fn planned_output_file_cannot_also_be_another_output_parent() {
    let dir = tempfile::tempdir().unwrap();
    let loose = sample(dir.path(), "photos.png", "rgb8.png");
    let nested = sample(dir.path(), "photos_compressed.png/child.png", "rgba8.png");
    let out = dir.path().join("out");
    fs::create_dir(&out).unwrap();
    let found = collect(&[loose, nested.parent().unwrap().to_owned()]);
    assert!(matches!(
        found.plan(
            &ImportOutput::CopyTo {
                directory: out.clone(),
                layout: CopyLayout::PreserveRoots
            },
            BatchParameters::default()
        ),
        Err(ImportError::Batch(BatchError::PathConflict {
            kind: PathConflictKind::OutputHierarchy,
            ..
        }))
    ));
    assert_eq!(fs::read_dir(out).unwrap().count(), 0);
}

#[test]
fn chosen_directory_inside_input_tree_is_only_written_after_scan_finishes() {
    let dir = tempfile::tempdir().unwrap();
    let source = sample(dir.path(), "pictures/sub/photo.data", "rgb8.png");
    let root = dir.path().join("pictures");
    let out = root.join("output");
    fs::create_dir(&out).unwrap();
    let found = collect(&[root]);
    let strategy = ImportOutput::CopyTo {
        directory: out.clone(),
        layout: CopyLayout::PreserveRoots,
    };
    found.plan(&strategy, BatchParameters::default()).unwrap();
    assert_eq!(fs::read_dir(&out).unwrap().count(), 0, "规划必须只读");
    let done = run(&found, strategy);
    assert_eq!(done.summary.succeeded, 1);
    assert!(out.join("pictures/sub/photo_compressed.png").exists());
    assert_eq!(found.files().len(), 1, "清单不会捕获本批新产物");
    assert_eq!(fs::read(source).unwrap(), fixture("rgb8.png"));
}

#[test]
fn scan_to_batch_keeps_real_results_and_does_not_treat_header_check_as_decode() {
    let dir = tempfile::tempdir().unwrap();
    let source = sample(dir.path(), "a.png", "rgb8.png");
    sample(dir.path(), "b.png", "bad-deflate.png");
    sample(dir.path(), "c.png", "already-optimized.png");
    let found = collect(&[dir.path().to_owned()]);
    assert_eq!(
        found.files().len(),
        3,
        "坏DEFLATE仍有有效chunk结构，最终解码必须再验证"
    );
    let done = run(&found, ImportOutput::Overwrite);
    assert_eq!(
        (
            done.summary.succeeded,
            done.summary.failed,
            done.summary.no_gain
        ),
        (1, 1, 1)
    );
    let JobState::Succeeded(report) = &done.jobs[0].state else {
        panic!("第一项应成功");
    };
    let ProcessingOutcome::Optimized {
        backup: Some(backup),
        ..
    } = &report.outcome
    else {
        panic!("覆盖必须有备份");
    };
    assert_eq!(fs::read(backup).unwrap(), fixture("rgb8.png"));
    assert!(fs::metadata(source).unwrap().len() < fixture("rgb8.png").len() as u64);
    let after = collect(&[dir.path().to_owned()]);
    assert_eq!(after.progress().excluded, 1, "真实备份符合输出层保留名规则");
}

#[test]
fn copy_tree_rejects_escape_missing_root_and_parent_files_without_changing_legacy_copy() {
    let dir = tempfile::tempdir().unwrap();
    let source = sample(dir.path(), "source.png", "rgb8.png");
    let mut request = PngRequest::new(&source);
    for relative in [
        PathBuf::new(),
        PathBuf::from("../outside.png"),
        dir.path().join("absolute.png"),
    ] {
        request.output = OutputPolicy::CopyTree {
            root: dir.path().to_owned(),
            relative,
        };
        assert!(matches!(
            optimize_png(&request, &CancellationToken::default(), |_| {}),
            Err(ProcessingError::InvalidPath)
        ));
    }
    request.output = OutputPolicy::CopyTree {
        root: dir.path().join("missing-root"),
        relative: "out.png".into(),
    };
    assert!(optimize_png(&request, &CancellationToken::default(), |_| {}).is_err());
    request.output = OutputPolicy::CopyTree {
        root: dir.path().to_owned(),
        relative: "source.png/out.png".into(),
    };
    assert!(optimize_png(&request, &CancellationToken::default(), |_| {}).is_err());
    request.output = OutputPolicy::Copy {
        destination: dir.path().join("legacy-missing/out.png"),
    };
    assert!(optimize_png(&request, &CancellationToken::default(), |_| {}).is_err());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    assert_eq!(fs::read(source).unwrap(), fixture("rgb8.png"));
}

#[test]
fn copy_tree_cancel_no_gain_and_late_conflict_keep_sources_and_no_temporary_files() {
    for kind in ["cancel", "no-gain", "conflict"] {
        let dir = tempfile::tempdir().unwrap();
        let sample_name = if kind == "no-gain" {
            "already-optimized.png"
        } else {
            "rgb8.png"
        };
        let source = sample(dir.path(), "source.png", sample_name);
        let mut request = PngRequest::new(&source);
        request.output = OutputPolicy::CopyTree {
            root: dir.path().to_owned(),
            relative: "new/sub/output.png".into(),
        };
        let destination = dir.path().join("new/sub/output.png");
        let token = CancellationToken::default();
        let result = optimize_png(&request, &token, |stage| {
            if stage == ProcessingStage::BeforeCommit {
                if kind == "cancel" {
                    token.cancel();
                }
                if kind == "conflict" {
                    fs::write(&destination, b"external").unwrap();
                }
            }
        });
        match kind {
            "cancel" => assert!(matches!(result, Err(ProcessingError::Cancelled))),
            "no-gain" => assert!(matches!(result.unwrap().outcome, ProcessingOutcome::NoGain)),
            _ => assert!(matches!(result, Err(ProcessingError::TargetConflict))),
        }
        assert_eq!(fs::read(&source).unwrap(), fixture(sample_name));
        assert!(
            destination.parent().unwrap().is_dir(),
            "显式结构目录不自动回收"
        );
        assert_eq!(
            fs::read_dir(destination.parent().unwrap()).unwrap().count(),
            usize::from(kind == "conflict")
        );
        if kind == "conflict" {
            assert_eq!(fs::read(destination).unwrap(), b"external");
        }
    }
}

#[cfg(windows)]
#[test]
fn windows_locked_input_reports_one_failure_and_other_files_remain_importable() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = tempfile::tempdir().unwrap();
    let locked = sample(dir.path(), "a.png", "rgb8.png");
    sample(dir.path(), "b.png", "rgba8.png");
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&locked)
        .unwrap();
    let found = collect(&[dir.path().to_owned()]);
    assert_eq!((found.files().len(), found.progress().rejected), (1, 1));
    assert!(
        matches!(&found.issues()[0].kind, ImportIssueKind::Failure(f) if f.code == JobErrorCode::Io)
    );
    drop(held);
    assert_eq!(fs::read(locked).unwrap(), fixture("rgb8.png"));
}

#[cfg(unix)]
#[test]
fn symlink_directories_files_and_tree_output_parents_are_rejected() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let source = sample(dir.path(), "source.png", "rgb8.png");
    sample(outside.path(), "outside.png", "rgb8.png");
    symlink(outside.path(), dir.path().join("linked-dir")).unwrap();
    symlink(&source, dir.path().join("linked-file.png")).unwrap();
    assert_eq!(
        collect(&[dir.path().join("linked-dir").join(".")])
            .progress()
            .rejected,
        1
    );
    let found = collect(&[dir.path().to_owned()]);
    assert_eq!((found.files().len(), found.progress().rejected), (1, 2));
    assert!(found.issues().iter().all(
        |i| matches!(&i.kind, ImportIssueKind::Failure(f) if f.code == JobErrorCode::InvalidInput)
    ));
    let mut request = PngRequest::new(&source);
    request.output = OutputPolicy::CopyTree {
        root: dir.path().to_owned(),
        relative: "linked-dir/out.png".into(),
    };
    assert!(matches!(
        optimize_png(&request, &CancellationToken::default(), |_| {}),
        Err(ProcessingError::InvalidPath)
    ));
    assert!(!outside.path().join("out.png").exists());
}

#[cfg(windows)]
#[test]
fn windows_junction_is_not_traversed_or_used_as_a_copy_tree_parent() {
    use std::{os::windows::process::CommandExt, process::Command};
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let source = sample(dir.path(), "source.png", "rgb8.png");
    let external = sample(outside.path(), "external.png", "rgba8.png");
    // junction无需开发者模式/符号链接权限，且所有路径均由本测试隔离目录生成。
    let result = Command::new("cmd.exe")
        .args(["/C", "mklink", "/J"])
        .arg(dir.path().join("linked"))
        .arg(outside.path())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "创建测试junction失败: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        collect(&[dir.path().join("linked").join(".")])
            .progress()
            .rejected,
        1
    );
    let found = collect(&[dir.path().to_owned()]);
    assert_eq!((found.files().len(), found.progress().rejected), (1, 1));
    assert!(
        matches!(&found.issues()[0].kind, ImportIssueKind::Failure(f) if f.code == JobErrorCode::InvalidInput)
    );
    let mut request = PngRequest::new(source);
    request.output = OutputPolicy::CopyTree {
        root: dir.path().to_owned(),
        relative: "linked/out.png".into(),
    };
    assert!(matches!(
        optimize_png(&request, &CancellationToken::default(), |_| {}),
        Err(ProcessingError::InvalidPath)
    ));
    assert!(!outside.path().join("out.png").exists());
    assert_eq!(fs::read(external).unwrap(), fixture("rgba8.png"));
}

#[test]
fn file_changed_during_scan_is_not_kept_as_a_valid_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let path = sample(dir.path(), "changing.png", "gradient-rgb8.png");
    let mut changed = false;
    let found = scan(
        std::slice::from_ref(&path),
        ScanOptions::default(),
        &CancellationToken::default(),
        |p| {
            if p.read_bytes.0 >= 12 && !changed {
                fs::write(&path, b"external replacement").unwrap();
                changed = true;
            }
        },
    )
    .unwrap();
    assert!(changed);
    assert!(found.files().is_empty());
    assert!(
        matches!(&found.issues()[0].kind, ImportIssueKind::Failure(f) if f.code == JobErrorCode::SourceChanged)
    );
    assert_eq!(fs::read(path).unwrap(), b"external replacement");
}

#[test]
fn multiple_real_workers_share_new_output_directories_without_clobbering() {
    let dir = tempfile::tempdir().unwrap();
    let one = sample(dir.path(), "input/one.png", "rgb8.png");
    let two = sample(dir.path(), "input/two.png", "rgba8.png");
    let out = dir.path().join("out");
    fs::create_dir(&out).unwrap();
    let found = collect(&[dir.path().join("input")]);
    let done = run(
        &found,
        ImportOutput::CopyTo {
            directory: out.clone(),
            layout: CopyLayout::PreserveRoots,
        },
    );
    assert_eq!(done.summary.succeeded, 2);
    assert!(out.join("input/one_compressed.png").exists());
    assert!(out.join("input/two_compressed.png").exists());
    assert_eq!(fs::read(one).unwrap(), fixture("rgb8.png"));
    assert_eq!(fs::read(two).unwrap(), fixture("rgba8.png"));
    assert_eq!(fs::read_dir(out.join("input")).unwrap().count(), 2);
}
