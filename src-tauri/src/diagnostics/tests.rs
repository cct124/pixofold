use super::*;
use pixofold_core::{
    model::{
        CancellationToken, OutputPolicy, PngMetadataPolicy, PngRequest, ProcessingError,
        ProcessingOutcome,
    },
    pipeline::optimize_png,
};
use std::fs;

fn with_logger<T>(logger: Arc<Logger>, run: impl FnOnce() -> T) -> T {
    // 生产在启动worker前安装全局订阅器。并行测试也保留一个不写盘的全局registry：
    // tracing-core 0.1.36只有一个作用域订阅器时，其他线程首次触发调用点可缓存never。
    // 空registry不接收本测试日志；各线程仍由自己的EventLayer隔离会话与任务字段。
    static BASELINE: OnceLock<()> = OnceLock::new();
    BASELINE.get_or_init(|| {
        tracing::subscriber::set_global_default(tracing_subscriber::registry())
            .expect("测试全局空订阅器只安装一次");
    });
    tracing::subscriber::with_default(
        tracing_subscriber::registry().with(layer::EventLayer(logger)),
        run,
    )
}

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
fn unscoped_first_callsite_does_not_hide_later_scoped_events() {
    fn emit() {
        tracing::info!(target: "pixofold", event = "cold_callsite");
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("logs");
    let logger = Logger::start(path.clone(), false);
    with_logger(logger.clone(), || {
        // 明确在没有本测试订阅器的线程先注册同一调用点，不靠调度概率或休眠。
        thread::spawn(emit).join().unwrap();
        let span = tracing::info_span!(target: "pixofold", "task", batch_id = 7);
        let _entered = span.enter();
        emit();
        let dispatcher = tracing::dispatcher::get_default(Clone::clone);
        thread::spawn(move || tracing::dispatcher::with_default(&dispatcher, emit))
            .join()
            .unwrap();
    });
    logger.shutdown();
    let values = records(&path);
    let events: Vec<_> = values
        .iter()
        .filter(|v| v["event"] == "cold_callsite")
        .collect();
    assert_eq!(
        events.len(),
        2,
        "作用域外事件不可混入，作用域内事件不能丢失"
    );
    assert_eq!(events[0]["batch_id"], 7);
    assert!(events[1].get("batch_id").is_none());
    assert_eq!(logger.health.dropped.load(Ordering::Relaxed), 0);
    assert_eq!(logger.health.failures.load(Ordering::Relaxed), 0);
}

#[test]
fn structured_events_keep_task_context_but_exclude_paths_messages_and_third_party_data() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("logs");
    let logger = Logger::start(path.clone(), false);
    with_logger(logger.clone(), || {
        let span = tracing::info_span!(target: "pixofold", "task", batch_id = 4, job_id = 2, attempt = 3, path = "PRIVATE_SOURCE", backup_name = "PRIVATE_SPAN-backup-Ab12xY.png");
        let _entered = span.enter();
        tracing::info!(target: "pixofold", event = "job_started", format="gif",mode="lossy",quality=80_u64,processing="lossless_fallback",fallback_reason="insufficient_gain",input_bytes = 123, request=?"PRIVATE_REQUEST",report=?"PRIVATE_REPORT",metadata="PRIVATE_METADATA",path = "PRIVATE_SOURCE", backup_name = "PRIVATE_EVENT-backup-Ab12xY.png", error = ?"PRIVATE_ERROR", "PRIVATE_MESSAGE");
        tracing::info!(target: "third_party", event = "PRIVATE_THIRD_PARTY");
        tracing::info!(target: "pixofold", event = "bounded", stage = ?"长".repeat(20000));
    });
    logger.shutdown();
    let values = records(&path);
    assert_eq!(values[0]["event"], "session_started");
    assert_eq!(
        values[0]["protocol_version"],
        crate::ipc::TASK_PROTOCOL_VERSION
    );
    assert_eq!(values[0]["build_source"], env!("PIXOFOLD_BUILD_SOURCE"));
    assert_eq!(values.last().unwrap()["event"], "session_finished");
    let line = values.iter().find(|v| v["event"] == "job_started").unwrap();
    assert_eq!(line["batch_id"], 4);
    assert_eq!(line["job_id"], 2);
    assert_eq!(line["attempt"], 3);
    assert_eq!(line["input_bytes"], 123);
    assert_eq!(line["format"], "gif");
    assert_eq!(line["mode"], "lossy");
    assert_eq!(line["quality"], 80);
    assert_eq!(line["processing"], "lossless_fallback");
    assert_eq!(line["fallback_reason"], "insufficient_gain");
    assert!(line.get("backup_name").is_none());
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
fn real_scan_records_bounded_rejection_categories_without_private_names() {
    use pixofold_core::{
        batch::ImageEngines,
        import::{ScanOptions, scan_with_engines},
        jpeg::JpegEngine,
    };
    use sha2::{Digest, Sha256};
    let temp = tempfile::tempdir().unwrap();
    let log_path = temp.path().join("logs");
    let logger = Logger::start(log_path.clone(), false);
    let tool = b"scan identity fixture; never executed";
    let tool_name = if cfg!(windows) {
        "pixofold-jpeg-helper.exe"
    } else {
        "pixofold-jpeg-helper"
    };
    fs::write(temp.path().join(tool_name), tool).unwrap();
    let engines = ImageEngines::with_jpeg(
        JpegEngine::load(temp.path(), Sha256::digest(tool).into()).unwrap(),
    );
    let source = temp.path().join("PRIVATE_SOURCE.png");
    fs::write(&source, fixture("rgba8.png")).unwrap();
    let animated = temp.path().join("PRIVATE_ANIMATION.png");
    fs::write(&animated, fixture("animated.png")).unwrap();
    let backup = temp.path().join("PRIVATE_BACKUP-backup-Ab12xY.png");
    fs::write(&backup, fixture("rgba8.png")).unwrap();
    // 上游真实JPEG插入APP11/XMP式未知APP1，在扫描元数据边界拒绝，不调用helper。
    let jpeg = fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/manual-images/jpeg/hopper_gray.jpg"),
    )
    .unwrap();
    let mut roots = vec![
        source.clone(),
        source,
        animated,
        backup,
        temp.path().join("PRIVATE_MISSING.png"),
    ];
    for (name, marker) in [
        ("PRIVATE_CREDENTIALS.jpg", 0xeb),
        ("PRIVATE_METADATA.jpg", 0xe1),
    ] {
        let path = temp.path().join(name);
        let mut bytes = jpeg[..2].to_vec();
        bytes.extend_from_slice(&[0xff, marker, 0, 5, 1, 2, 3]);
        bytes.extend_from_slice(&jpeg[2..]);
        fs::write(&path, bytes).unwrap();
        roots.push(path);
    }
    with_logger(logger.clone(), || {
        let span = tracing::info_span!(target: "pixofold", "selection", selection_id = 42);
        let _entered = span.enter();
        let scan = scan_with_engines(
            &roots,
            ScanOptions::default(),
            &CancellationToken::default(),
            &mut |_: &pixofold_core::import::ScanProgress| {},
            &engines,
        )
        .unwrap();
        crate::tasks::record_scan(&scan);
        drop(_entered);
        let span = tracing::info_span!(target: "pixofold", "selection", selection_id = 43);
        let _entered = span.enter();
        let mut options = ScanOptions::default();
        options.probe_limits.max_input_bytes = pixofold_core::model::ByteCount(1);
        let limited = scan_with_engines(
            &roots[..1],
            options,
            &CancellationToken::default(),
            &mut |_: &pixofold_core::import::ScanProgress| {},
            &ImageEngines::default(),
        )
        .unwrap();
        crate::tasks::record_scan(&limited);
    });
    logger.shutdown();
    let values = records(&log_path);
    let summary = values
        .iter()
        .find(|v| v["event"] == "import_scan_finished")
        .unwrap();
    assert_eq!(summary["accepted"], 1);
    assert_eq!(summary["rejected"], 4);
    assert_eq!(summary["duplicates"], 1);
    assert_eq!(summary["excluded"], 1);
    for code in [
        "unsupported_animation",
        "unsupported_jpeg_credentials",
        "unsupported_metadata",
        "io",
        "duplicate",
        "generated_artifact",
    ] {
        let entry = values
            .iter()
            .find(|v| v["event"] == "import_scan_issue_summary" && v["error_code"] == code)
            .unwrap();
        assert_eq!(entry["count"], 1);
        assert_eq!(entry["selection_id"], 42);
    }
    assert!(!serde_json::to_string(&values).unwrap().contains("PRIVATE_"));
    let resource = values
        .iter()
        .find(|v| v["event"] == "import_scan_issue_summary" && v["selection_id"] == 43)
        .unwrap();
    assert_eq!(resource["error_code"], "resource_limit");
    assert_eq!(resource["count"], 1);
    assert_eq!(values.last().unwrap()["event"], "session_finished");
    assert_eq!(values.last().unwrap()["dropped_events"], 0);
    assert_eq!(values.last().unwrap()["write_failures"], 0);
}

#[test]
fn unavailable_logging_does_not_change_processing_and_shutdown_remains_safe() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("not-a-directory");
    fs::write(&path, b"preserve").unwrap();
    let logger = Logger::start(path.clone(), false);
    let source = temp.path().join("private.png");
    fs::write(&source, fixture("gradient-rgb8.png")).unwrap();
    with_logger(logger.clone(), || {
        let mut request = PngRequest::new(source);
        request.output = OutputPolicy::Copy {
            destination: temp.path().join("copy.png"),
        };
        assert!(optimize_png(&request, &CancellationToken::default(), |_| {}).is_ok());
    });
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
        with_logger(logger.clone(), || {
            let span = tracing::info_span!(target: "pixofold", "task", batch_id = 1, job_id = 1);
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
                assert!(
                    backup
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .starts_with("PRIVATE_IMAGE-backup-")
                );
                assert_eq!(fs::read(backup).unwrap(), original);
            }
            if policy == 2 {
                assert_eq!(fs::read(&source).unwrap(), original);
            }
        });
        logger.shutdown();
        let values = records(&path);
        let events: Vec<_> = values.iter().filter_map(|v| v["event"].as_str()).collect();
        assert_eq!(
            logger.health.dropped.load(Ordering::Relaxed),
            0,
            "policy={policy}, events={events:?}"
        );
        assert_eq!(
            logger.health.failures.load(Ordering::Relaxed),
            0,
            "policy={policy}, events={events:?}"
        );
        assert_eq!(
            events.last(),
            Some(&"session_finished"),
            "policy={policy}, events={events:?}"
        );
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
            .unwrap_or_else(|| panic!("missing commit event: policy={policy}, events={events:?}"));
        assert!(waiting < removing && removing < committed);
        assert_eq!(events.contains(&"backup_creating"), policy == 0);
        assert_eq!(events.contains(&"backup_retained"), policy == 0);
        assert!(
            values
                .iter()
                .all(|value| value.get("backup_name").is_none())
        );
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
    with_logger(logger.clone(), || {
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
                    format: pixofold_core::batch::ImageKind::Png,
                    source,
                    output: OutputPolicy::Copy {
                        destination: temp.path().join(format!("copy_{i}.png")),
                    },
                }
            })
            .collect();
        let id = service
            .start(BatchRequest {
                engines: Default::default(),
                items,
                parameters: Default::default(),
            })
            .unwrap();
        let view = service.wait(id, Duration::from_secs(15)).unwrap();
        assert_eq!(view.phase, BatchPhase::Finished);
        assert_eq!(view.summary.succeeded, 4);
        service.shutdown().unwrap();
    });
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
        assert_eq!(row["format"], "png");
        assert_eq!(row["mode"], "lossless");
        assert!(row.get("quality").is_none());
        assert_eq!(row["processing"], "lossless");
        assert_eq!(row["fallback_reason"], "none");
        assert!(
            values
                .iter()
                .any(|v| v["event"] == "output_commit_succeeded" && v["job_id"] == job)
        );
    }
    assert!(!serde_json::to_string(&values).unwrap().contains("PRIVATE_"));
}

#[test]
fn real_worker_no_gain_and_failure_do_not_claim_committed_lossy_outputs() {
    use pixofold_core::{
        batch::{BatchConfig, BatchItem, BatchPhase, BatchRequest, BatchService, ImageKind},
        model::{PngMode, QualityValue},
    };
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("logs");
    let logger = Logger::start(path.clone(), false);
    with_logger(logger.clone(), || {
        let source = temp.path().join("PRIVATE_good.png");
        fs::write(&source, fixture("rgba16.png")).unwrap();
        let mut seed = PngRequest::new(&source);
        seed.output = OutputPolicy::OverwriteWithoutBackup;
        optimize_png(&seed, &CancellationToken::default(), |_| {}).unwrap();
        let invalid = temp.path().join("PRIVATE_bad.png");
        fs::write(&invalid, b"invalid").unwrap();
        let mut service = BatchService::new(BatchConfig::default()).unwrap();
        let parameters = pixofold_core::batch::BatchParameters {
            mode: PngMode::Lossy {
                quality: QualityValue::new(100).unwrap(),
            },
            ..Default::default()
        };
        let id = service
            .start(BatchRequest {
                engines: Default::default(),
                parameters,
                items: vec![
                    BatchItem {
                        format: ImageKind::Png,
                        source,
                        output: OutputPolicy::Copy {
                            destination: temp.path().join("out.png"),
                        },
                    },
                    BatchItem {
                        format: ImageKind::Png,
                        source: invalid,
                        output: OutputPolicy::Copy {
                            destination: temp.path().join("bad-out.png"),
                        },
                    },
                ],
            })
            .unwrap();
        let result = service.wait(id, Duration::from_secs(15)).unwrap();
        assert_eq!(result.phase, BatchPhase::Finished);
        assert_eq!(result.summary.no_gain, 1);
        assert_eq!(result.summary.failed, 1);
        service.shutdown().unwrap();
    });
    logger.shutdown();
    let values = records(&path);
    let finished: Vec<_> = values
        .iter()
        .filter(|v| v["event"] == "job_finished")
        .collect();
    assert_eq!(finished.len(), 2);
    for row in finished {
        assert_eq!(row["format"], "png");
        assert_eq!(row["mode"], "lossy");
        assert_eq!(row["quality"], 100);
        if row["result"] == "failed" {
            assert!(row.get("processing").is_none());
            assert!(row.get("fallback_reason").is_none());
        } else {
            assert_eq!(row["result"], "no_gain");
            assert_eq!(row["processing"], "lossless_fallback");
            assert_eq!(row["fallback_reason"], "high_bit_depth");
            assert_eq!(row["input_bytes"], row["output_bytes"]);
        }
    }
    assert!(!serde_json::to_string(&values).unwrap().contains("PRIVATE_"));
}
