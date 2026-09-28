use super::*;
use pixofold_core::{
    model::{
        CancellationToken, OutputPolicy, PngMetadataPolicy, PngRequest, ProcessingError,
        ProcessingOutcome,
    },
    pipeline::optimize_png,
};
use std::fs;

fn records(path: &std::path::Path) -> Vec<Value> {
    let mut values = Vec::new();
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "jsonl") {
            for line in fs::read_to_string(path).unwrap().lines() {
                values.push(serde_json::from_str(line).unwrap());
            }
        }
    }
    values
}

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/png")
            .join(name),
    )
    .unwrap()
}

#[test]
fn structured_events_keep_task_context_but_exclude_paths_messages_and_third_party_data() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("logs");
    let logger = Logger::start(path.clone(), false);
    tracing::subscriber::with_default(
        tracing_subscriber::registry().with(layer::EventLayer(logger.clone())),
        || {
            let span = tracing::info_span!(target: "pixofold", "task", batch_id = 4, job_id = 2, attempt = 3, path = "PRIVATE_SOURCE");
            let _entered = span.enter();
            tracing::info!(target: "pixofold", event = "job_started", input_bytes = 123, path = "PRIVATE_SOURCE", error = ?"PRIVATE_ERROR", "PRIVATE_MESSAGE");
            tracing::info!(target: "third_party", event = "PRIVATE_THIRD_PARTY");
            tracing::info!(target: "pixofold", event = "bounded", stage = ?"长".repeat(20000));
        },
    );
    logger.shutdown();
    let values = records(&path);
    assert_eq!(values[0]["event"], "session_started");
    assert_eq!(values.last().unwrap()["event"], "session_finished");
    let line = values.iter().find(|v| v["event"] == "job_started").unwrap();
    assert_eq!(line["batch_id"], 4);
    assert_eq!(line["job_id"], 2);
    assert_eq!(line["attempt"], 3);
    assert_eq!(line["input_bytes"], 123);
    assert!(!serde_json::to_string(&values).unwrap().contains("PRIVATE_"));
    assert!(
        values
            .iter()
            .all(|v| serde_json::to_vec(v).unwrap().len() <= MAX_EVENT_BYTES)
    );
}

#[test]
fn full_queue_is_nonblocking_and_oversized_event_is_still_bounded_json() {
    let (sender, _receiver) = mpsc::sync_channel(1);
    let (_, finished) = mpsc::channel();
    let logger = Logger {
        sender,
        health: Arc::new(Health::new()),
        directory: None,
        thread: Mutex::new(None),
        finished: Mutex::new(finished),
        session: "test".into(),
        started: Instant::now(),
        sequence: AtomicU64::new(1),
    };
    logger.event("INFO", json!({"event":"one"}));
    logger.event("INFO", json!({"event":"two"}));
    assert_eq!(logger.health.dropped.load(Ordering::Relaxed), 1);
    let encoded = logger.encode("INFO", json!({"event":"x".repeat(MAX_EVENT_BYTES * 2)}));
    assert!(encoded.len() <= MAX_EVENT_BYTES);
    assert_eq!(
        serde_json::from_slice::<Value>(&encoded).unwrap()["event"],
        "event_truncated"
    );
}

#[test]
fn unavailable_logging_does_not_change_processing_and_shutdown_remains_safe() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("not-a-directory");
    fs::write(&path, b"preserve").unwrap();
    let logger = Logger::start(path.clone(), false);
    let source = temp.path().join("private.png");
    fs::write(&source, fixture("gradient-rgb8.png")).unwrap();
    tracing::subscriber::with_default(
        tracing_subscriber::registry().with(layer::EventLayer(logger.clone())),
        || {
            let mut request = PngRequest::new(source);
            request.output = OutputPolicy::Copy {
                destination: temp.path().join("copy.png"),
            };
            assert!(optimize_png(&request, &CancellationToken::default(), |_| {}).is_ok());
        },
    );
    logger.shutdown();
    logger.shutdown();
    assert_eq!(fs::read(path).unwrap(), b"preserve");
    assert!(logger.health.dropped.load(Ordering::Relaxed) > 0);
    assert!(logger.health.failures.load(Ordering::Relaxed) > 0);
}

#[test]
fn real_credentials_processing_logs_wait_then_only_the_chosen_backup_policy() {
    for policy in 0..3 {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("logs");
        let logger = Logger::start(path.clone(), false);
        // 将现成caBX样本的完整块复制到可压缩图，保留CRC；不引入真实凭据。
        let credentials = fixture("content-credentials.png");
        let offset = credentials.windows(4).position(|v| v == b"caBX").unwrap();
        let len = u32::from_be_bytes(credentials[offset - 4..offset].try_into().unwrap()) as usize;
        let gradient = fixture("gradient-rgb8.png");
        let original = [
            &gradient[..33],
            &credentials[offset - 4..offset + len + 8],
            &gradient[33..],
        ]
        .concat();
        let source = temp.path().join("PRIVATE_IMAGE.png");
        fs::write(&source, &original).unwrap();
        tracing::subscriber::with_default(
            tracing_subscriber::registry().with(layer::EventLayer(logger.clone())),
            || {
                let span =
                    tracing::info_span!(target: "pixofold", "task", batch_id = 1, job_id = 1);
                let _entered = span.enter();
                let mut request = PngRequest::new(source.clone());
                request.output = match policy {
                    0 => OutputPolicy::Overwrite,
                    1 => OutputPolicy::OverwriteWithoutBackup,
                    _ => OutputPolicy::Copy {
                        destination: temp.path().join("copy.png"),
                    },
                };
                let ProcessingError::ContentCredentialsRequireConsent(version) =
                    optimize_png(&request, &CancellationToken::default(), |_| {}).unwrap_err()
                else {
                    panic!("expected consent");
                };
                assert_eq!(fs::read(&source).unwrap(), original);
                assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2); // 只有原图和logs目录
                request.metadata = PngMetadataPolicy::RemoveContentCredentials(version);
                let report = optimize_png(&request, &CancellationToken::default(), |_| {}).unwrap();
                let ProcessingOutcome::Optimized { backup, .. } = report.outcome else {
                    panic!("expected gain");
                };
                assert_eq!(backup.is_some(), policy == 0);
                if let Some(backup) = backup {
                    assert_eq!(fs::read(backup).unwrap(), original);
                }
                if policy == 2 {
                    assert_eq!(fs::read(&source).unwrap(), original);
                }
            },
        );
        logger.shutdown();
        let values = records(&path);
        let events: Vec<_> = values.iter().filter_map(|v| v["event"].as_str()).collect();
        let waiting = events
            .iter()
            .position(|e| *e == "credentials_waiting_for_consent")
            .unwrap();
        let removing = events
            .iter()
            .position(|e| *e == "credentials_removing_in_memory")
            .unwrap();
        let committed = events
            .iter()
            .position(|e| *e == "output_commit_succeeded")
            .unwrap();
        assert!(waiting < removing && removing < committed);
        assert_eq!(events.contains(&"backup_creating"), policy == 0);
        if policy == 0 {
            let backup = events.iter().position(|e| *e == "backup_creating").unwrap();
            assert!(removing < backup && backup < committed);
        }
        assert!(
            !serde_json::to_string(&values)
                .unwrap()
                .contains("PRIVATE_IMAGE")
        );
    }
}

#[test]
fn dropped_events_are_reported_on_recovery_and_at_normal_exit() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("logs");
    let logger = Logger::start(path.clone(), false);
    logger.health.dropped.store(7, Ordering::Relaxed);
    logger.event("INFO", json!({"event":"recovered"}));
    logger.shutdown();
    let values = records(&path);
    let loss = values
        .iter()
        .find(|v| v["event"] == "logging_loss_summary")
        .unwrap();
    assert_eq!(loss["dropped_events"], 7);
    assert_eq!(values.last().unwrap()["dropped_events"], 7);
}

#[test]
fn real_worker_pool_logs_each_image_with_its_own_batch_job_and_attempt() {
    use pixofold_core::batch::{BatchConfig, BatchItem, BatchPhase, BatchRequest, BatchService};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("logs");
    let logger = Logger::start(path.clone(), false);
    tracing::subscriber::with_default(
        tracing_subscriber::registry().with(layer::EventLayer(logger.clone())),
        || {
            let mut service = BatchService::new(BatchConfig {
                workers: 2,
                ..Default::default()
            })
            .unwrap();
            let items = (0..4)
                .map(|i| {
                    let source = temp.path().join(format!("PRIVATE_{i}.png"));
                    fs::write(&source, fixture("gradient-rgb8.png")).unwrap();
                    BatchItem {
                        source,
                        output: OutputPolicy::Copy {
                            destination: temp.path().join(format!("copy_{i}.png")),
                        },
                    }
                })
                .collect();
            let id = service
                .start(BatchRequest {
                    items,
                    parameters: Default::default(),
                })
                .unwrap();
            let view = service.wait(id, Duration::from_secs(15)).unwrap();
            assert_eq!(view.phase, BatchPhase::Finished);
            assert_eq!(view.summary.succeeded, 4);
            service.shutdown().unwrap();
        },
    );
    logger.shutdown();
    let values = records(&path);
    let finished: Vec<_> = values
        .iter()
        .filter(|v| v["event"] == "job_finished")
        .collect();
    assert_eq!(finished.len(), 4);
    for job in 1..=4 {
        let row = finished.iter().find(|v| v["job_id"] == job).unwrap();
        assert_eq!(row["batch_id"], 1);
        assert_eq!(row["attempt"], 1);
        assert_eq!(row["result"], "succeeded");
        assert!(
            values
                .iter()
                .any(|v| v["event"] == "output_commit_succeeded" && v["job_id"] == job)
        );
    }
    assert!(!serde_json::to_string(&values).unwrap().contains("PRIVATE_"));
}
