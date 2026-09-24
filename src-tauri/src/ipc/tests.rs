//! 契约测试覆盖真实隔离文件和纯转换；不把模拟IPC当作原生GUI验证。

use super::{convert, dto::*, query};
use crate::tasks::*;
use pixofold_core::{batch::*, import::*, model::*};
use serde_json::{Value, json};
use std::{
    fs, io,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

fn request(collection: TaskCollection) -> TaskPageRequest {
    TaskPageRequest {
        expected_revision: None,
        collection,
        offset: 0,
        limit: 100,
    }
}
fn wire(value: impl serde::Serialize) -> Value {
    serde_json::to_value(value).unwrap()
}
fn phase(control: &TaskControl, target: TaskPhase) -> TaskSnapshot {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut snapshot = control.snapshot();
    while snapshot.phase != target {
        assert_ne!(snapshot.phase, TaskPhase::Closed);
        snapshot = control
            .wait_for_change(
                snapshot.revision,
                deadline.saturating_duration_since(Instant::now()),
            )
            .unwrap();
    }
    snapshot
}
fn sample(dir: &Path, name: &str, fixture: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/png")
            .join(fixture),
        &path,
    )
    .unwrap();
    path
}
fn completed() -> (tempfile::TempDir, TaskRuntime, TaskSnapshot) {
    let dir = tempfile::tempdir().unwrap();
    let good = sample(dir.path(), "good.png", "rgb8.png");
    let bad = sample(dir.path(), "bad.png", "bad-deflate.png");
    let small = sample(dir.path(), "small.png", "already-optimized.png");
    let runtime = TaskRuntime::new(TaskConfig::default()).unwrap();
    let control = runtime.control();
    control
        .import(
            vec![good, bad, small],
            Some(TaskSettings {
                parameters: BatchParameters::default(),
                output: ImportOutput::CopyBeside,
            }),
        )
        .unwrap();
    let snapshot = phase(&control, TaskPhase::Finished);
    (dir, runtime, snapshot)
}

#[test]
fn decimal_u64_is_lossless_and_rejects_noncanonical_wire_values() {
    for value in [0, 1, 9_007_199_254_740_993, u64::MAX] {
        let encoded = wire(DecimalU64(value));
        assert_eq!(encoded, value.to_string());
        assert_eq!(
            serde_json::from_value::<DecimalU64>(encoded).unwrap().0,
            value
        );
    }
    for value in [
        json!(1),
        json!(-1),
        json!(null),
        json!(""),
        json!("01"),
        json!("+1"),
        json!("-1"),
        json!(" 1"),
        json!("1.0"),
        json!("1e3"),
        json!("１"),
        json!("18446744073709551616"),
    ] {
        assert!(serde_json::from_value::<DecimalU64>(value).is_err());
    }
}

#[test]
fn request_deserialization_rejects_extra_fields_wrong_types_and_ranges() {
    let valid = json!({"expectedRevision": null, "collection": "jobs", "offset": 0, "limit": 100});
    serde_json::from_value::<TaskPageRequest>(valid.clone())
        .unwrap()
        .validate()
        .unwrap();
    for (key, value) in [
        ("path", json!("private/file.png")),
        ("offset", json!(-1)),
        ("limit", json!(1.5)),
        ("limit", json!(65536)),
        ("collection", json!("all")),
        ("expectedRevision", json!(12)),
    ] {
        let mut input = valid.clone();
        input[key] = value;
        assert!(serde_json::from_value::<TaskPageRequest>(input).is_err());
    }
    for limit in [0, 101, u16::MAX] {
        let mut input = request(TaskCollection::Jobs);
        input.limit = limit;
        assert_eq!(input.validate(), Err(QueryError::InvalidPage));
    }
    let mut input = request(TaskCollection::Jobs);
    input.offset = 1;
    assert_eq!(input.validate(), Err(QueryError::InvalidPage));
}

#[test]
fn idle_query_has_stable_contract_and_never_starts_or_cancels_work() {
    let mut runtime = TaskRuntime::new(TaskConfig::default()).unwrap();
    let control = runtime.control();
    let before = control.snapshot();
    for _ in 0..3 {
        assert_eq!(
            wire(query(&control, request(TaskCollection::Jobs)).unwrap()),
            json!({
                "protocolVersion": TASK_PROTOCOL_VERSION, "revision": before.revision.to_string(), "selectionId": null,
                "phase": "idle", "scan": null, "error": null, "batch": null,
                "page": {"kind": "jobs", "offset": 0, "total": 0, "items": []},
            })
        );
    }
    assert_eq!(control.snapshot().revision, before.revision);
    runtime.shutdown().unwrap();
    assert_eq!(
        wire(query(&control, request(TaskCollection::Jobs)).unwrap())["phase"],
        "closed"
    );
}

#[test]
fn real_batch_serializes_success_no_gain_failure_and_exact_totals_without_paths() {
    let (dir, mut runtime, snapshot) = completed();
    let result = wire(query(&runtime.control(), request(TaskCollection::Jobs)).unwrap());
    assert_eq!(result["phase"], "finished");
    let batch = snapshot.batch.as_ref().unwrap();
    assert_eq!(result["batch"]["summary"]["processed"], 3);
    assert_eq!(result["batch"]["summary"]["succeeded"], 1);
    assert_eq!(result["batch"]["summary"]["failed"], 1);
    assert_eq!(result["batch"]["summary"]["noGain"], 1);
    assert_eq!(
        result["batch"]["summary"]["savedBytes"],
        batch.summary.saved_bytes.unwrap().0.to_string()
    );
    let rows = result["page"]["items"].as_array().unwrap();
    let successful = rows
        .iter()
        .find(|r| r["sourceName"]["text"] == "good.png")
        .unwrap();
    assert_eq!(successful["state"]["kind"], "succeeded");
    assert_eq!(
        successful["state"]["report"]["outputName"]["text"],
        "good_compressed.png"
    );
    assert_eq!(successful["mode"]["kind"], "lossless");
    let failed = rows
        .iter()
        .find(|r| r["sourceName"]["text"] == "bad.png")
        .unwrap();
    assert_eq!(failed["state"]["failure"]["code"], "decode");
    assert!(
        !result
            .to_string()
            .contains(dir.path().file_name().unwrap().to_string_lossy().as_ref())
    );
    assert_eq!(
        fs::read(dir.path().join("good.png")).unwrap(),
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/png/rgb8.png"))
            .unwrap()
    );
    runtime.shutdown().unwrap();
}

#[test]
fn pages_are_bounded_at_one_revision_and_stale_requests_cannot_mix_after_clear() {
    let (_dir, mut runtime, snapshot) = completed();
    let control = runtime.control();
    let mut input = request(TaskCollection::Jobs);
    input.limit = 1;
    let first = query(&control, input.clone()).unwrap();
    input.expected_revision = Some(first.revision);
    input.offset = 1;
    let second = wire(query(&control, input.clone()).unwrap());
    assert_eq!(second["page"]["items"].as_array().unwrap().len(), 1);
    assert_ne!(
        wire(first)["page"]["items"][0]["id"],
        second["page"]["items"][0]["id"]
    );
    input.offset = 3;
    assert_eq!(
        wire(query(&control, input.clone()).unwrap())["page"]["items"],
        json!([])
    );
    input.offset = u32::MAX;
    assert_eq!(
        query(&control, input.clone()).unwrap_err(),
        QueryError::InvalidPage
    );
    control.clear(snapshot.selection.unwrap()).unwrap();
    let cleared = phase(&control, TaskPhase::Idle);
    input.offset = 0;
    assert_eq!(
        query(&control, input).unwrap_err(),
        QueryError::StaleSnapshot {
            current_revision: DecimalU64(cleared.revision)
        }
    );
    runtime.shutdown().unwrap();
}

#[test]
fn import_candidates_and_issues_have_stable_indexes_and_explicit_scan_status() {
    let dir = tempfile::tempdir().unwrap();
    let good = sample(dir.path(), "good.png", "rgb8.png");
    let unsupported = sample(dir.path(), "unsupported.png", "fake.png");
    let mut runtime = TaskRuntime::new(TaskConfig::default()).unwrap();
    let control = runtime.control();
    control
        .import(vec![good.clone(), good, unsupported], None)
        .unwrap();
    phase(&control, TaskPhase::Ready);
    let candidates = wire(query(&control, request(TaskCollection::Candidates)).unwrap());
    assert_eq!(candidates["scan"]["status"]["kind"], "complete");
    assert_eq!(candidates["scan"]["duplicates"], 1);
    assert_eq!(candidates["page"]["total"], 1);
    assert_eq!(candidates["page"]["items"][0]["index"], 0);
    let issues = wire(query(&control, request(TaskCollection::Issues)).unwrap());
    let rows = issues["page"]["items"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|r| r["issue"]["kind"] == "duplicate"));
    assert!(rows.iter().any(|r| r["issue"]["kind"] == "unsupported"));
    runtime.shutdown().unwrap();
}

#[test]
fn read_page_never_serializes_unrequested_rows_and_handles_u64_max_revision() {
    let (_dir, _runtime, mut snapshot) = completed();
    snapshot.revision = u64::MAX;
    let batch = Arc::make_mut(snapshot.batch.as_mut().unwrap());
    let mut outside_page = batch.jobs[0].clone();
    let JobState::Succeeded(report) = &mut outside_page.state else {
        panic!("expected success");
    };
    report.elapsed = Duration::MAX;
    let mut row = batch.jobs[0].clone();
    row.request.source = Path::new("secret-parent").join(format!("{}\n.png", "中".repeat(300)));
    row.input_bytes = Some(ByteCount(u64::MAX));
    row.state = JobState::Queued;
    batch.jobs = vec![row; 105];
    // 第101行若被转换会返回InvalidSnapshot；它不在请求页面，不能触碰。
    batch.jobs[100] = outside_page;
    let result = wire(convert::snapshot(&snapshot, &request(TaskCollection::Jobs)).unwrap());
    assert_eq!(result["revision"], u64::MAX.to_string());
    assert_eq!(result["page"]["total"], 105);
    assert_eq!(result["page"]["items"].as_array().unwrap().len(), 100);
    let first = &result["page"]["items"][0];
    assert_eq!(
        first["sourceName"]["text"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        MAX_DISPLAY_CHARS
    );
    assert_eq!(first["sourceName"]["truncated"], true);
    assert_eq!(first["inputBytes"], u64::MAX.to_string());
    assert!(!result.to_string().contains("secret-parent"));
}

#[test]
fn cancelled_unknown_sizes_and_processing_stages_are_not_fabricated_percentages() {
    let (_dir, _runtime, mut snapshot) = completed();
    let batch = Arc::make_mut(snapshot.batch.as_mut().unwrap());
    batch.jobs[0].state = JobState::Cancelled;
    batch.jobs[0].input_bytes = None;
    batch.jobs[1].state = JobState::Running {
        stage: ProcessingStage::BeforeCommit,
        cancel_requested: true,
    };
    batch.summary = BatchSummary {
        total: 3,
        cancelled: 1,
        running: 1,
        no_gain: 1,
        processed: 1,
        terminal: 2,
        ..BatchSummary::default()
    };
    let result = wire(convert::snapshot(&snapshot, &request(TaskCollection::Jobs)).unwrap());
    assert_eq!(result["batch"]["summary"]["processed"], 1);
    assert_eq!(result["batch"]["summary"]["terminal"], 2);
    assert!(result["batch"]["summary"]["savedBytes"].is_null());
    assert!(result["page"]["items"][0]["inputBytes"].is_null());
    assert_eq!(
        result["page"]["items"][1]["state"],
        json!({"kind": "running", "stage": "before_commit", "cancelRequested": true})
    );
}

#[test]
fn protected_metadata_failure_reaches_real_task_snapshot_without_modifying_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut inputs = Vec::new();
    for name in ["content-credentials.png", "unsafe-metadata.png"] {
        let source = sample(dir.path(), name, name);
        let bytes = fs::read(&source).unwrap();
        inputs.push((source, bytes));
    }
    let mut runtime = TaskRuntime::new(TaskConfig::default()).unwrap();
    let control = runtime.control();
    control
        .import(
            inputs.iter().map(|(path, _)| path.clone()).collect(),
            Some(TaskSettings::default()),
        )
        .unwrap();
    phase(&control, TaskPhase::Finished);
    let result = wire(query(&control, request(TaskCollection::Jobs)).unwrap());
    let rows = result["page"]["items"].as_array().unwrap();
    let codes: Vec<_> = rows
        .iter()
        .map(|row| row["state"]["failure"]["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"unsupported_content_credentials"));
    assert!(codes.contains(&"unsupported_metadata"));
    assert_eq!(result["batch"]["summary"]["failed"], 2);
    assert_eq!(result["batch"]["confirmationCount"], 1);
    let confirmations = wire(query(&control, request(TaskCollection::Confirmations)).unwrap());
    assert_eq!(confirmations["page"]["total"], 1);
    assert_eq!(
        confirmations["page"]["items"][0]["sourceName"]["text"],
        "content-credentials.png"
    );
    assert!(
        !confirmations
            .to_string()
            .contains(&dir.path().to_string_lossy().to_string())
    );
    assert_eq!(result["batch"]["summary"]["savedBytes"], "0");
    assert_eq!(
        result["batch"]["summary"]["currentBytes"],
        result["batch"]["summary"]["inputBytes"]
    );
    assert!(
        rows.iter()
            .all(|row| row["state"]["failure"]["recovery"].is_null())
    );
    for (source, bytes) in &inputs {
        assert_eq!(fs::read(source).unwrap(), *bytes);
    }
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    assert!(
        !result
            .to_string()
            .contains(&dir.path().to_string_lossy().to_string())
    );
    runtime.shutdown().unwrap();
}

#[test]
fn metadata_recovery_causes_keep_distinct_wire_categories() {
    let (_dir, _runtime, mut snapshot) = completed();
    for (chunk, code) in [
        (b"caBX", "unsupported_content_credentials"),
        (b"vpAG", "unsupported_metadata"),
    ] {
        Arc::make_mut(snapshot.batch.as_mut().unwrap()).jobs[0].state =
            JobState::Failed(JobFailure {
                code: JobErrorCode::CleanupFailed,
                cause: Some(Arc::new(ProcessingError::CleanupFailed {
                    original: Some(Box::new(ProcessingError::UnsupportedMetadata(*chunk))),
                    source: io::Error::other("private reason"),
                    temporary: Path::new("private-parent").join("temp.png"),
                })),
            });
        let result = wire(convert::snapshot(&snapshot, &request(TaskCollection::Jobs)).unwrap());
        assert_eq!(
            result["page"]["items"][0]["state"]["failure"]["recovery"]["originalError"],
            json!({"kind":"failed","code":code})
        );
    }
}

#[test]
fn confirmation_pages_include_only_eligible_rows_and_distinguish_same_names_without_paths() {
    let dir = tempfile::tempdir().unwrap();
    let mut inputs = Vec::new();
    for folder in ["one", "two", "three"] {
        let parent = dir.path().join(folder);
        fs::create_dir(&parent).unwrap();
        inputs.push(sample(&parent, "same.png", "content-credentials.png"));
    }
    inputs.push(sample(dir.path(), "unsafe.png", "unsafe-metadata.png"));
    let mut runtime = TaskRuntime::new(TaskConfig::default()).unwrap();
    let control = runtime.control();
    control
        .import(inputs, Some(TaskSettings::default()))
        .unwrap();
    let finished = phase(&control, TaskPhase::Finished);
    let mut first_request = request(TaskCollection::Confirmations);
    first_request.limit = 2;
    first_request.expected_revision = Some(DecimalU64(finished.revision));
    let first = wire(query(&control, first_request.clone()).unwrap());
    assert_eq!(first["batch"]["confirmationCount"], 3);
    assert_eq!(first["page"]["total"], 3);
    let first_rows = first["page"]["items"].as_array().unwrap();
    assert_eq!(first_rows.len(), 2);
    assert_ne!(first_rows[0]["id"], first_rows[1]["id"]);
    assert_eq!(first_rows[0]["sourceName"], first_rows[1]["sourceName"]);
    assert_ne!(first_rows[0]["sourceLabel"], first_rows[1]["sourceLabel"]);
    let mut second_request = first_request.clone();
    second_request.offset = 2;
    let second = wire(query(&control, second_request.clone()).unwrap());
    assert_eq!(second["revision"], first["revision"]);
    assert_eq!(second["page"]["items"].as_array().unwrap().len(), 1);
    assert_ne!(second["page"]["items"][0]["id"], first_rows[0]["id"]);
    assert_ne!(second["page"]["items"][0]["id"], first_rows[1]["id"]);
    assert!(
        !first
            .to_string()
            .contains(&dir.path().to_string_lossy().to_string())
    );
    control.clear(finished.selection.unwrap()).unwrap();
    phase(&control, TaskPhase::Idle);
    assert!(matches!(
        query(&control, second_request),
        Err(QueryError::StaleSnapshot { .. })
    ));
    runtime.shutdown().unwrap();
}

#[test]
fn error_conversion_preserves_recovery_categories_but_never_raw_io_or_absolute_paths() {
    let (_dir, _runtime, mut snapshot) = completed();
    let error = JobFailure {
        code: JobErrorCode::CleanupFailed,
        cause: Some(Arc::new(ProcessingError::CleanupFailed {
            original: Some(Box::new(ProcessingError::CommitFailed {
                source: io::Error::other("secret source content"),
                backup: Path::new("private-parent").join("backup.png"),
            })),
            source: io::Error::other("secret source content"),
            temporary: Path::new("private-parent").join("temp.png"),
        })),
    };
    Arc::make_mut(snapshot.batch.as_mut().unwrap()).jobs[0].state = JobState::Failed(error.clone());
    snapshot.error = Some(
        ImportError::File {
            index: 0,
            failure: error,
        }
        .into(),
    );
    let result = wire(convert::snapshot(&snapshot, &request(TaskCollection::Jobs)).unwrap());
    let failure = &result["error"]["failure"];
    assert_eq!(failure["code"], "cleanup_failed");
    assert_eq!(failure["recovery"]["backupName"]["text"], "backup.png");
    assert_eq!(failure["recovery"]["temporaryName"]["text"], "temp.png");
    assert_eq!(
        failure["recovery"]["originalError"],
        json!({"kind": "failed", "code": "commit_failed"})
    );
    let text = result.to_string();
    assert!(!text.contains("private-parent"));
    assert!(!text.contains("secret source content"));
}

#[test]
fn lossy_fallback_mapping_and_elapsed_milliseconds_come_from_actual_report() {
    let (_dir, _runtime, mut snapshot) = completed();
    let batch = Arc::make_mut(snapshot.batch.as_mut().unwrap());
    let JobState::Succeeded(report) = &mut batch.jobs[0].state else {
        panic!("expected success");
    };
    report.processing = PngProcessing::LosslessFallback {
        mapping: PngQualityMapping {
            version: 3,
            quality: QualityValue::default(),
            minimum: 70,
            target: 80,
        },
        reason: LossyFallbackReason::QualityBelowTarget { measured: Some(69) },
    };
    report.elapsed = Duration::from_micros(1_234_567);
    let result = wire(convert::snapshot(&snapshot, &request(TaskCollection::Jobs)).unwrap());
    let wire_report = &result["page"]["items"][0]["state"]["report"];
    assert_eq!(wire_report["elapsedMs"], "1234");
    assert_eq!(
        wire_report["processing"],
        json!({"kind": "lossless_fallback", "mappingVersion": 3,
        "reason": {"kind": "quality_below_target", "measured": 69}})
    );
    let JobState::Succeeded(report) =
        &mut Arc::make_mut(snapshot.batch.as_mut().unwrap()).jobs[0].state
    else {
        unreachable!()
    };
    report.elapsed = Duration::MAX;
    assert_eq!(
        convert::snapshot(&snapshot, &request(TaskCollection::Jobs)).unwrap_err(),
        QueryError::InvalidSnapshot
    );
}

#[test]
fn scan_limit_and_error_remain_visible_without_batch() {
    let dir = tempfile::tempdir().unwrap();
    let first = sample(dir.path(), "one.png", "rgb8.png");
    let second = sample(dir.path(), "two.png", "rgb8.png");
    let mut runtime = TaskRuntime::new(TaskConfig {
        scan: ScanOptions {
            max_files: 1,
            ..ScanOptions::default()
        },
        ..TaskConfig::default()
    })
    .unwrap();
    let control = runtime.control();
    control.import(vec![first, second], None).unwrap();
    phase(&control, TaskPhase::Rejected);
    let result = wire(query(&control, request(TaskCollection::Candidates)).unwrap());
    assert_eq!(
        result["scan"]["status"],
        json!({"kind": "limited", "limit": "files"})
    );
    assert_eq!(result["error"]["code"], "incomplete_scan");
    assert!(result["batch"].is_null());
    runtime.shutdown().unwrap();
}

#[test]
fn cancelled_cleanup_is_not_mislabeled_service_fault_and_controls_are_visible_as_sanitized() {
    let (_dir, _runtime, mut snapshot) = completed();
    let batch = Arc::make_mut(snapshot.batch.as_mut().unwrap());
    batch.jobs[0].request.source = Path::new("private-parent").join("line\nname.png");
    batch.jobs[0].state = JobState::Failed(JobFailure {
        code: JobErrorCode::CleanupFailed,
        cause: Some(Arc::new(ProcessingError::CleanupFailed {
            original: Some(Box::new(ProcessingError::Cancelled)),
            source: io::Error::other("private"),
            temporary: Path::new("private-parent").join("temp.png"),
        })),
    });
    let result = wire(convert::snapshot(&snapshot, &request(TaskCollection::Jobs)).unwrap());
    let row = &result["page"]["items"][0];
    assert_eq!(row["sourceName"]["text"], "line�name.png");
    assert_eq!(row["sourceName"]["sanitized"], true);
    assert_eq!(
        row["state"]["failure"]["recovery"]["originalError"],
        json!({"kind": "cancelled"})
    );
}

#[test]
fn task_errors_are_stable_codes_and_count_overflow_fails_without_panicking() {
    let runtime = TaskRuntime::new(TaskConfig::default()).unwrap();
    let mut snapshot = runtime.control().snapshot();
    for (error, expected) in [
        (TaskError::StaleSelection, "stale_selection"),
        (TaskError::StaleBatch, "stale_batch"),
        (
            TaskError::WorkerStart(Arc::new(io::Error::other("private"))),
            "worker_start",
        ),
        (
            BatchError::InvalidParameters(ProcessingError::InvalidLimits).into(),
            "invalid_parameters",
        ),
        (
            ImportError::RootNameConflict {
                first: "private1".into(),
                second: "private2".into(),
            }
            .into(),
            "root_name_conflict",
        ),
    ] {
        snapshot.error = Some(error);
        assert_eq!(
            wire(convert::snapshot(&snapshot, &request(TaskCollection::Jobs)).unwrap())["error"],
            json!({"code": expected})
        );
    }
    if usize::BITS > 32 {
        snapshot.scan_progress = Some(ScanProgress {
            status: ScanStatus::Scanning,
            discovered: usize::MAX,
            examined: 0,
            accepted: 0,
            duplicates: 0,
            excluded: 0,
            rejected: 0,
            read_bytes: ByteCount(0),
        });
        assert_eq!(
            convert::snapshot(&snapshot, &request(TaskCollection::Jobs)).unwrap_err(),
            QueryError::InvalidSnapshot
        );
    }
}

#[cfg(any(unix, windows))]
#[test]
fn non_unicode_names_are_explicitly_lossy_and_not_used_as_identity() {
    #[cfg(unix)]
    let source = {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(vec![b'n', 0xff, b'm'])
    };
    #[cfg(windows)]
    let source = {
        use std::os::windows::ffi::OsStringExt;
        std::ffi::OsString::from_wide(&[0x006e, 0xd800, 0x006d])
    };
    let (_dir, _runtime, mut snapshot) = completed();
    Arc::make_mut(snapshot.batch.as_mut().unwrap()).jobs[0]
        .request
        .source = source.into();
    let id = snapshot.batch.as_ref().unwrap().jobs[0].id.get();
    let result = wire(convert::snapshot(&snapshot, &request(TaskCollection::Jobs)).unwrap());
    assert_eq!(result["page"]["items"][0]["sourceName"]["lossy"], true);
    assert_eq!(result["page"]["items"][0]["sourceName"]["text"], "n�m");
    assert_eq!(result["page"]["items"][0]["id"], id);
}
