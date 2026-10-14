//! 真实APNG无损与共享输出行为回归；所有源/副本/故障注入均在隔离目录。
#[path = "support/native_names.rs"]
mod native_names;
use pixofold_core::{
    apng::*,
    model::{
        ByteCount, CancellationToken, OutputDirectory, OutputPolicy, ProcessingError,
        ProcessingOutcome, ProcessingStage,
    },
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/apng")
            .join(name),
    )
    .unwrap()
}
fn setup() -> (tempfile::TempDir, ApngRequest, Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("动画.APNG");
    let bytes = fixture("split-data.apng");
    fs::write(&source, &bytes).unwrap();
    (dir, ApngRequest::new(source), bytes)
}
fn run(request: &ApngRequest) -> Result<ApngReport, ApngError> {
    optimize_apng(request, &CancellationToken::default(), |_| {})
}
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut files = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    files.sort();
    files
}
fn source_only(dir: &Path, request: &ApngRequest, original: &[u8]) {
    assert_eq!(fs::read(&request.source).unwrap(), original);
    assert_eq!(files(dir), std::slice::from_ref(&request.source));
}
fn candidate(dir: &Path) -> PathBuf {
    files(dir)
        .into_iter()
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".pixofold-output-")
        })
        .unwrap()
}

#[test]
fn real_copy_backup_and_no_backup_preserve_playback_and_report_input_structure() {
    for policy in 0..3 {
        let (dir, mut request, original) = setup();
        let copy = dir.path().join("副本.apng");
        request.output = match policy {
            0 => OutputPolicy::Overwrite,
            1 => OutputPolicy::OverwriteWithoutBackup,
            _ => OutputPolicy::Copy {
                destination: copy.clone(),
            },
        };
        let report = run(&request).unwrap();
        assert_eq!(report.processing, ApngProcessing::Lossless);
        assert_eq!(report.image.frames, 2);
        assert_eq!(report.image.num_plays, 0);
        assert!(report.image.default_participates);
        assert_eq!(report.input_bytes, ByteCount(original.len() as u64));
        assert!(report.output_bytes.0 < report.input_bytes.0);
        let ProcessingOutcome::Optimized { output, backup } = report.outcome else {
            panic!("expected gain")
        };
        compare_apng(
            &original,
            &fs::read(&output).unwrap(),
            &ApngValidationLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap();
        assert_eq!(backup.is_some(), policy == 0);
        if let Some(backup) = backup {
            assert_eq!(fs::read(&backup).unwrap(), original);
            assert_eq!(backup.extension().unwrap(), "APNG");
        }
        if policy == 2 {
            assert_eq!(fs::read(&request.source).unwrap(), original);
            assert_eq!(output, copy.canonicalize().unwrap());
        }
        assert_eq!(files(dir.path()).len(), if policy == 1 { 1 } else { 2 });
    }
}
#[test]
fn no_gain_never_creates_backup_or_copy_and_checks_source_first() {
    for policy in 0..3 {
        let (dir, mut request, _) = setup();
        let original = fixture("default-in.png");
        fs::write(&request.source, &original).unwrap();
        request.output = match policy {
            0 => OutputPolicy::Overwrite,
            1 => OutputPolicy::OverwriteWithoutBackup,
            _ => OutputPolicy::Copy {
                destination: dir.path().join("copy.apng"),
            },
        };
        let report = run(&request).unwrap();
        assert_eq!(report.outcome, ProcessingOutcome::NoGain);
        assert_eq!(report.input_bytes, report.output_bytes);
        source_only(dir.path(), &request, &original);
        let external = fixture("changed-loop.apng");
        let result = optimize_apng(&request, &CancellationToken::default(), |stage| {
            if stage == ProcessingStage::Validating {
                fs::write(&request.source, &external).unwrap();
            }
        });
        assert!(
            matches!(result, Err(ApngError::File(ProcessingError::SourceChanged))),
            "{result:?}"
        );
        source_only(dir.path(), &request, &external);
    }
}
#[test]
fn cancellation_at_all_stages_cleans_staged_output_and_backup() {
    for target in [
        ProcessingStage::Reading,
        ProcessingStage::Optimizing,
        ProcessingStage::Validating,
        ProcessingStage::BeforeCommit,
    ] {
        let (dir, request, original) = setup();
        let cancel = CancellationToken::default();
        let result = optimize_apng(&request, &cancel, |stage| {
            if stage == target {
                cancel.cancel();
            }
        });
        assert!(
            matches!(result, Err(ApngError::Cancelled)),
            "{target:?} {result:?}"
        );
        source_only(dir.path(), &request, &original);
    }
    let (dir, request, original) = setup();
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(matches!(
        optimize_apng(&request, &cancel, |_| panic!("pre-cancel")),
        Err(ApngError::Cancelled)
    ));
    source_only(dir.path(), &request, &original);
}
#[test]
fn same_length_source_mutation_or_identity_replacement_aborts_before_commit() {
    for target in [ProcessingStage::Optimizing, ProcessingStage::BeforeCommit] {
        let (dir, request, mut external) = setup();
        external[60] ^= 1;
        let modified = fs::metadata(&request.source).unwrap().modified().unwrap();
        let result = optimize_apng(&request, &CancellationToken::default(), |stage| {
            if stage == target {
                fs::write(&request.source, &external).unwrap();
                fs::File::options()
                    .write(true)
                    .open(&request.source)
                    .unwrap()
                    .set_modified(modified)
                    .unwrap();
            }
        });
        assert!(
            matches!(result, Err(ApngError::File(ProcessingError::SourceChanged))),
            "{result:?}"
        );
        source_only(dir.path(), &request, &external);
    }
    let (dir, request, original) = setup();
    let moved = dir.path().join("moved.apng");
    let result = optimize_apng(&request, &CancellationToken::default(), |stage| {
        if stage == ProcessingStage::BeforeCommit {
            fs::rename(&request.source, &moved).unwrap();
            fs::write(&request.source, &original).unwrap();
        }
    });
    assert!(
        matches!(result, Err(ApngError::File(ProcessingError::SourceChanged))),
        "{result:?}"
    );
    assert_eq!(files(dir.path()).len(), 2);
    assert_eq!(fs::read(moved).unwrap(), original);
}
#[test]
fn candidate_changes_at_validation_and_at_commit_are_rejected() {
    for target in [ProcessingStage::Validating, ProcessingStage::BeforeCommit] {
        let (dir, request, original) = setup();
        let result = optimize_apng(&request, &CancellationToken::default(), |stage| {
            if stage == target {
                fs::write(candidate(dir.path()), fixture("changed-pixel.apng")).unwrap();
            }
        });
        assert!(
            matches!(
                result,
                Err(ApngError::ValidationFailed)
                    | Err(ApngError::File(ProcessingError::ValidationFailed(_)))
            ),
            "{target:?} {result:?}"
        );
        source_only(dir.path(), &request, &original);
    }
}
#[test]
fn copy_conflicts_and_readonly_source_keep_original_bytes() {
    let (dir, mut request, original) = setup();
    let copy = dir.path().join("copy.apng");
    request.output = OutputPolicy::Copy {
        destination: request.source.clone(),
    };
    assert!(matches!(
        run(&request),
        Err(ApngError::File(ProcessingError::TargetConflict))
    ));
    request.output = OutputPolicy::Copy {
        destination: copy.clone(),
    };
    fs::hard_link(&request.source, &copy).unwrap();
    assert!(matches!(
        run(&request),
        Err(ApngError::File(ProcessingError::TargetConflict))
    ));
    fs::remove_file(&copy).unwrap();
    let result = optimize_apng(&request, &CancellationToken::default(), |stage| {
        if stage == ProcessingStage::BeforeCommit {
            fs::write(&copy, b"existing output").unwrap();
        }
    });
    assert!(matches!(
        result,
        Err(ApngError::File(ProcessingError::TargetConflict))
    ));
    assert_eq!(fs::read(&copy).unwrap(), b"existing output");
    fs::remove_file(&copy).unwrap();
    let permissions = fs::metadata(&request.source).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&request.source, readonly).unwrap();
    request.output = OutputPolicy::Overwrite;
    let denied = run(&request);
    request.output = OutputPolicy::Copy {
        destination: copy.clone(),
    };
    let copied = run(&request);
    fs::set_permissions(&request.source, permissions).unwrap();
    assert!(matches!(
        denied,
        Err(ApngError::File(ProcessingError::Io { .. }))
    ));
    assert!(copied.is_ok(), "{copied:?}");
    assert_eq!(fs::read(&request.source).unwrap(), original);
    assert_eq!(files(dir.path()).len(), 2);
}
#[test]
fn authorized_tree_remains_bound_and_preserves_original_name() {
    let (dir, mut request, original) = setup();
    let root = dir.path().join("output");
    fs::create_dir(&root).unwrap();
    let target = OutputDirectory::open(&root).unwrap();
    request.output = OutputPolicy::CopyTreeAuthorized {
        root: target,
        relative: PathBuf::from("sub/original.APNG"),
    };
    let report = run(&request).unwrap();
    assert!(matches!(
        report.outcome,
        ProcessingOutcome::Optimized { backup: None, .. }
    ));
    assert!(root.join("sub/original.APNG").is_file());
    assert_eq!(fs::read(&request.source).unwrap(), original);
    let root2 = dir.path().join("replacement");
    fs::create_dir(&root2).unwrap();
    let target = OutputDirectory::open(&root2).unwrap();
    request.output = OutputPolicy::CopyTreeAuthorized {
        root: target,
        relative: PathBuf::from("file.apng"),
    };
    let result = optimize_apng(&request, &CancellationToken::default(), |stage| {
        if stage == ProcessingStage::Optimizing {
            fs::rename(&root2, dir.path().join("old-root")).unwrap();
            fs::create_dir(&root2).unwrap();
        }
    });
    assert!(
        matches!(
            result,
            Err(ApngError::File(ProcessingError::TargetConflict))
        ),
        "{result:?}"
    );
    assert_eq!(fs::read(&request.source).unwrap(), original);
    assert!(files(&root2).is_empty());
}
#[test]
fn limits_unsupported_metadata_and_invalid_input_never_write_outputs() {
    for name in [
        "low-depth.apng",
        "metadata-caBX.apng",
        "metadata-iCCP.apng",
        "metadata-eXIf.apng",
        "bad-crc.apng",
        "missing-adler.apng",
        "zlib-trailing.apng",
        "outside.apng",
    ] {
        let (dir, request, _) = setup();
        let bytes = fixture(name);
        fs::write(&request.source, &bytes).unwrap();
        assert!(
            matches!(run(&request), Err(ApngError::Validation(_))),
            "{name}"
        );
        source_only(dir.path(), &request, &bytes);
    }
    for kind in 0..6 {
        let (dir, mut request, original) = setup();
        match kind {
            0 => request.limits.validation.max_input_bytes = 10,
            1 => request.limits.validation.max_canvas_pixels = 1,
            2 => request.limits.max_working_set_bytes = ByteCount(1),
            3 => request.limits.validation.max_validation_bytes = 1,
            4 => request.limits.preparation_timeout = Duration::ZERO,
            _ => request.limits.validation.max_canvas_pixels = u64::MAX,
        }
        let error = run(&request).unwrap_err();
        assert!(
            matches!(
                error,
                ApngError::ResourceLimit(_)
                    | ApngError::Timeout
                    | ApngError::InvalidLimits
                    | ApngError::File(ProcessingError::ResourceLimit(_))
            ),
            "{error:?}"
        );
        source_only(dir.path(), &request, &original);
    }
}
#[test]
fn backup_extensions_and_native_stems_are_preserved_and_excluded_from_scan() {
    use pixofold_core::import::{ScanOptions, scan};
    for suffix in [".png", ".PNG", ".apng", ".aPnG"] {
        let dir = tempfile::tempdir().unwrap();
        let original = fixture("split-data.apng");
        let source = native_names::write_native_name(dir.path(), suffix, &original);
        let report = run(&ApngRequest::new(&source)).unwrap();
        let ProcessingOutcome::Optimized {
            backup: Some(backup),
            ..
        } = report.outcome
        else {
            panic!("backup")
        };
        let mut expected = source.file_stem().unwrap().to_os_string();
        expected.push("-backup-");
        let token = backup
            .file_name()
            .unwrap()
            .as_encoded_bytes()
            .strip_prefix(expected.as_encoded_bytes())
            .unwrap()
            .strip_suffix(suffix.as_bytes())
            .unwrap();
        assert_eq!(token.len(), 6);
        assert!(token.iter().all(u8::is_ascii_alphanumeric));
        assert_eq!(fs::read(&backup).unwrap(), original);
        let scan = scan(
            &[dir.path().to_owned()],
            ScanOptions::default(),
            &CancellationToken::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!(scan.progress().excluded, 1);
    }
}

#[cfg(windows)]
#[test]
fn windows_commit_failure_keeps_recoverable_backup_and_cleanup_failure_keeps_cause() {
    use std::os::windows::fs::OpenOptionsExt;
    let (dir, request, original) = setup();
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&request.source)
        .unwrap();
    let error = run(&request).unwrap_err();
    drop(held);
    let ApngError::File(ProcessingError::CommitFailed { backup, .. }) = error else {
        panic!("{error:?}")
    };
    assert_eq!(fs::read(&request.source).unwrap(), original);
    assert_eq!(fs::read(backup).unwrap(), original);
    assert_eq!(files(dir.path()).len(), 2);
    let (dir, request, original) = setup();
    let token = CancellationToken::default();
    let mut held = None;
    let result = optimize_apng(&request, &token, |stage| {
        if stage == ProcessingStage::Validating {
            held = Some(
                fs::OpenOptions::new()
                    .read(true)
                    .share_mode(3)
                    .open(candidate(dir.path()))
                    .unwrap(),
            );
            token.cancel();
        }
    });
    drop(held);
    let ApngError::Cleanup {
        original: cause,
        temporary,
        ..
    } = result.unwrap_err()
    else {
        panic!("cleanup error")
    };
    assert!(matches!(*cause, ApngError::Cancelled));
    assert!(temporary.exists());
    fs::remove_file(temporary).unwrap();
    source_only(dir.path(), &request, &original);
}
#[cfg(unix)]
#[test]
fn symlink_source_or_output_is_rejected() {
    use std::os::unix::fs::symlink;
    let (dir, mut request, original) = setup();
    let link = dir.path().join("link.apng");
    symlink(&request.source, &link).unwrap();
    assert!(matches!(
        run(&ApngRequest::new(&link)),
        Err(ApngError::File(ProcessingError::InvalidPath))
    ));
    request.output = OutputPolicy::Copy { destination: link };
    assert!(matches!(
        run(&request),
        Err(ApngError::File(ProcessingError::TargetConflict))
    ));
    assert_eq!(fs::read(&request.source).unwrap(), original);
}

#[test]
fn corpus_is_unchanged_and_all_rejections_precede_output() {
    use serde_json::Value;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/apng");
    let manifest: Value =
        serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
    for sample in manifest["samples"].as_array().unwrap() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = fixture(sample["file"].as_str().unwrap());
        let source = dir.path().join("source.png");
        fs::write(&source, &bytes).unwrap();
        let mut request = ApngRequest::new(&source);
        request.output = OutputPolicy::Copy {
            destination: dir.path().join("copy.apng"),
        };
        request.limits.validation = serde_json::from_value(
            sample
                .get("limits")
                .cloned()
                .unwrap_or(serde_json::json!({})),
        )
        .unwrap();
        let result = run(&request);
        assert_eq!(fs::read(&source).unwrap(), bytes);
        if sample["expect"] == "ok" {
            let report = result.unwrap_or_else(|e| panic!("{}: {e:?}", sample["file"]));
            match report.outcome {
                ProcessingOutcome::Optimized { output, backup } => {
                    assert!(backup.is_none());
                    compare_apng(
                        &bytes,
                        &fs::read(output).unwrap(),
                        &ApngValidationLimits::default(),
                        &CancellationToken::default(),
                    )
                    .unwrap();
                    assert_eq!(files(dir.path()).len(), 2);
                }
                ProcessingOutcome::NoGain => source_only(dir.path(), &request, &bytes),
            }
        } else {
            assert!(result.is_err(), "{}", sample["file"]);
            source_only(dir.path(), &request, &bytes);
        }
    }
}
