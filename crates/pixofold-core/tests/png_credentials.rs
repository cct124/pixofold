//! 内容凭据显式策略回归；所有写入只发生在隔离副本，不验证或伪造凭据签名。
use pixofold_core::{
    model::{
        CancellationToken, ContentCredentialsSource, OutputPolicy, PngMetadataPolicy, PngMode,
        PngRequest, ProcessingError, ProcessingOutcome, ProcessingReport, ProcessingStage,
        QualityValue, ResourceLimits,
    },
    pipeline::optimize_png,
    probe::inspect_png,
};
use std::{fs, path::Path};

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/png")
            .join(name),
    )
    .unwrap()
}
fn insert(data: &[u8], name: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut chunk = (payload.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(name);
    chunk.extend_from_slice(payload);
    chunk.extend_from_slice(&crc32fast::hash(&chunk[4..]).to_be_bytes());
    [data[..33].to_vec(), chunk, data[33..].to_vec()].concat()
}
fn run(request: &PngRequest) -> Result<ProcessingReport, ProcessingError> {
    optimize_png(request, &CancellationToken::default(), |_| {})
}
fn consent(request: &PngRequest) -> ContentCredentialsSource {
    match run(request).unwrap_err() {
        ProcessingError::ContentCredentialsRequireConsent(version) => version,
        other => panic!("{other:?}"),
    }
}
fn workspace() -> (tempfile::TempDir, PngRequest, Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let data = insert(
        &fixture("gradient-rgb8.png"),
        b"caBX",
        b"synthetic credentials",
    );
    let request = PngRequest::new(dir.path().join("凭据.png"));
    fs::write(&request.source, &data).unwrap();
    (dir, request, data)
}
fn ancillary(data: &[u8]) -> Vec<Vec<u8>> {
    let mut chunks = Vec::new();
    let mut offset = 8;
    while offset < data.len() {
        let length = u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        let end = offset + length + 12;
        let name = &data[offset + 4..offset + 8];
        if name[0] & 32 != 0 && !matches!(name, b"caBX" | b"tRNS") {
            chunks.push(data[offset..end].to_vec());
        }
        offset = end;
    }
    chunks
}

#[test]
fn explicit_consent_removes_only_credentials_and_backs_up_complete_source() {
    for mode in [
        PngMode::Lossless,
        PngMode::Lossy {
            quality: QualityValue::new(68).unwrap(),
        },
    ] {
        for overwrite in [false, true] {
            let (dir, mut request, original) = workspace();
            let original = insert(&original, b"tEXt", b"Comment\0preserved");
            fs::write(&request.source, &original).unwrap();
            request.mode = mode;
            request.metadata = PngMetadataPolicy::RemoveContentCredentials(consent(&request));
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
            if !overwrite {
                request.output = OutputPolicy::Copy {
                    destination: dir.path().join("copy.png"),
                };
            }
            let report = run(&request).unwrap();
            assert!(report.content_credentials_removed);
            assert_eq!(report.input_bytes.0, original.len() as u64);
            let ProcessingOutcome::Optimized { output, backup } = report.outcome else {
                panic!("expected gain")
            };
            let result = fs::read(output).unwrap();
            inspect_png(&result, ResourceLimits::default()).unwrap();
            assert!(!result.windows(4).any(|w| w == b"caBX"));
            assert_eq!(ancillary(&result), ancillary(&original));
            if overwrite {
                assert_eq!(fs::read(backup.unwrap()).unwrap(), original);
                assert!(matches!(run(&request), Err(ProcessingError::SourceChanged)));
            } else {
                assert!(backup.is_none());
                assert_eq!(fs::read(&request.source).unwrap(), original);
                assert!(matches!(
                    run(&request),
                    Err(ProcessingError::TargetConflict)
                ));
            }
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        }
    }
}

#[test]
fn mixed_unsafe_chunks_are_not_eligible_regardless_of_order() {
    for credentials_first in [true, false] {
        let (dir, request, original) = workspace();
        let data = if credentials_first {
            insert(&original, b"vpAG", b"unsafe")
        } else {
            insert(
                &insert(&fixture("gradient-rgb8.png"), b"vpAG", b"unsafe"),
                b"caBX",
                b"credentials",
            )
        };
        fs::write(&request.source, &data).unwrap();
        assert!(
            matches!(run(&request), Err(ProcessingError::UnsupportedMetadata(name)) if name == *b"vpAG")
        );
        assert_eq!(fs::read(&request.source).unwrap(), data);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}

#[test]
fn consent_is_bound_to_path_and_same_length_content() {
    for change_path in [false, true] {
        let (dir, mut request, original) = workspace();
        request.metadata = PngMetadataPolicy::RemoveContentCredentials(consent(&request));
        let modified = fs::metadata(&request.source).unwrap().modified().unwrap();
        if change_path {
            request.source = dir.path().join("another.png");
        }
        let changed = if change_path {
            original.clone()
        } else {
            insert(
                &fixture("gradient-rgb8.png"),
                b"caBX",
                b"changed___credentials",
            )
        };
        assert_eq!(changed.len(), original.len());
        fs::write(&request.source, &changed).unwrap();
        fs::File::options()
            .write(true)
            .open(&request.source)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert_eq!(
            fs::metadata(&request.source).unwrap().modified().unwrap(),
            modified
        );
        assert!(matches!(run(&request), Err(ProcessingError::SourceChanged)));
        assert_eq!(fs::read(&request.source).unwrap(), changed);
    }
}

#[test]
fn damaged_image_never_yields_credentials_consent() {
    let dir = tempfile::tempdir().unwrap();
    let request = PngRequest::new(dir.path().join("damaged.png"));
    let data = insert(&fixture("bad-deflate.png"), b"caBX", b"credentials");
    fs::write(&request.source, &data).unwrap();
    let error = run(&request).unwrap_err();
    assert!(!matches!(
        error,
        ProcessingError::ContentCredentialsRequireConsent(_)
    ));
    assert_eq!(fs::read(&request.source).unwrap(), data);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn consent_never_bypasses_cancel_validation_or_source_change() {
    for (fault, without_backup) in
        (0..3).flat_map(|fault| [false, true].map(|without_backup| (fault, without_backup)))
    {
        let (dir, mut request, original) = workspace();
        if without_backup {
            request.output = OutputPolicy::OverwriteWithoutBackup;
        }
        request.metadata = PngMetadataPolicy::RemoveContentCredentials(consent(&request));
        let cancel = CancellationToken::default();
        let result = optimize_png(&request, &cancel, |stage| {
            if stage == ProcessingStage::Validating {
                match fault {
                    0 => cancel.cancel(),
                    1 => {
                        let temp = fs::read_dir(dir.path())
                            .unwrap()
                            .map(|e| e.unwrap().path())
                            .find(|p| p != &request.source)
                            .unwrap();
                        fs::write(temp, fixture("gray8.png")).unwrap();
                    }
                    _ => fs::write(&request.source, b"external change").unwrap(),
                }
            }
        });
        assert!(
            match fault {
                0 => matches!(result, Err(ProcessingError::Cancelled)),
                1 => matches!(result, Err(ProcessingError::ValidationFailed(_))),
                _ => matches!(result, Err(ProcessingError::SourceChanged)),
            },
            "{result:?}"
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert_eq!(
            fs::read(&request.source).unwrap(),
            if fault == 2 {
                b"external change".to_vec()
            } else {
                original
            }
        );
    }
}

#[test]
fn explicitly_unbacked_overwrite_commits_without_backup_in_both_modes() {
    for mode in [
        PngMode::Lossless,
        PngMode::Lossy {
            quality: QualityValue::new(68).unwrap(),
        },
    ] {
        let (dir, mut request, original) = workspace();
        request.mode = mode;
        request.metadata = PngMetadataPolicy::RemoveContentCredentials(consent(&request));
        request.output = OutputPolicy::OverwriteWithoutBackup;
        let report = run(&request).unwrap();
        assert!(report.content_credentials_removed);
        assert!(matches!(
            report.outcome,
            ProcessingOutcome::Optimized { backup: None, .. }
        ));
        let result = fs::read(&request.source).unwrap();
        inspect_png(&result, ResourceLimits::default()).unwrap();
        assert!(!result.windows(4).any(|w| w == b"caBX"));
        assert!(result.len() < original.len());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}

#[test]
fn unbacked_overwrite_still_rechecks_cancel_candidate_and_source_before_replacement() {
    for fault in 0..3 {
        let (dir, mut request, original) = workspace();
        request.metadata = PngMetadataPolicy::RemoveContentCredentials(consent(&request));
        request.output = OutputPolicy::OverwriteWithoutBackup;
        let cancel = CancellationToken::default();
        let result = optimize_png(&request, &cancel, |stage| {
            if stage != ProcessingStage::BeforeCommit {
                return;
            }
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
            match fault {
                0 => cancel.cancel(),
                1 => {
                    let temp = fs::read_dir(dir.path())
                        .unwrap()
                        .map(|e| e.unwrap().path())
                        .find(|p| p != &request.source)
                        .unwrap();
                    fs::write(temp, b"changed candidate").unwrap();
                }
                _ => fs::write(&request.source, b"external change").unwrap(),
            }
        });
        assert!(
            match fault {
                0 => matches!(result, Err(ProcessingError::Cancelled)),
                1 => matches!(result, Err(ProcessingError::ValidationFailed(_))),
                _ => matches!(result, Err(ProcessingError::SourceChanged)),
            },
            "{result:?}"
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert_eq!(
            fs::read(&request.source).unwrap(),
            if fault == 2 {
                b"external change".to_vec()
            } else {
                original
            }
        );
    }
}

#[test]
fn unbacked_overwrite_no_gain_keeps_original_timestamp_and_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let original = fixture("already-optimized.png");
    let mut request = PngRequest::new(dir.path().join("small.png"));
    request.output = OutputPolicy::OverwriteWithoutBackup;
    fs::write(&request.source, &original).unwrap();
    let modified = fs::metadata(&request.source).unwrap().modified().unwrap();
    let report = run(&request).unwrap();
    assert_eq!(report.outcome, ProcessingOutcome::NoGain);
    assert!(!report.content_credentials_removed);
    assert_eq!(fs::read(&request.source).unwrap(), original);
    assert_eq!(
        fs::metadata(&request.source).unwrap().modified().unwrap(),
        modified
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[cfg(windows)]
#[test]
fn unbacked_overwrite_sharing_violation_never_deletes_original_or_leaves_output() {
    use std::os::windows::fs::OpenOptionsExt;
    let (dir, mut request, original) = workspace();
    request.metadata = PngMetadataPolicy::RemoveContentCredentials(consent(&request));
    request.output = OutputPolicy::OverwriteWithoutBackup;
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&request.source)
        .unwrap();
    let result = run(&request);
    drop(held);
    assert!(
        matches!(result, Err(ProcessingError::Io { .. })),
        "{result:?}"
    );
    assert_eq!(fs::read(&request.source).unwrap(), original);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}
