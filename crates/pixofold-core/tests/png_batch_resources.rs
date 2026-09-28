//! 收紧执行上限后的真实语料回归：与原同步pipeline输出一致，不改变色深/ICC/透明度策略。
use pixofold_core::{
    batch::{BatchConfig, BatchItem, BatchParameters, BatchRequest, BatchService, JobState},
    model::{
        ByteCount, CancellationToken, OutputPolicy, PngMode, PngRequest, ProcessingOutcome,
        QualityValue,
    },
    pipeline::optimize_png,
};
use std::{fs, path::Path, time::Duration};

#[test]
fn parallel_refined_limits_match_the_standalone_pipeline_for_every_static_fixture() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/png");
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(fixtures.join("manifest.json")).unwrap()).unwrap();
    for mode in [
        PngMode::Lossless,
        PngMode::Lossy {
            quality: QualityValue::new(68).unwrap(),
        },
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut items = Vec::new();
        for entry in manifest["fixtures"].as_array().unwrap() {
            if !matches!(entry["expected"].as_str(), Some("static" | "no-gain")) {
                continue;
            }
            let name = entry["file"].as_str().unwrap();
            let source = dir.path().join(name);
            fs::copy(fixtures.join(name), &source).unwrap();
            items.push(BatchItem {
                source,
                output: OutputPolicy::Copy {
                    destination: dir.path().join(format!("batch-{name}")),
                },
            });
        }
        let service = BatchService::new(BatchConfig {
            workers: 4,
            working_set_budget: ByteCount(512 * 1024 * 1024),
            ..BatchConfig::default()
        })
        .unwrap();
        let id = service
            .start(BatchRequest {
                items,
                parameters: BatchParameters {
                    mode,
                    ..BatchParameters::default()
                },
            })
            .unwrap();
        let done = service.wait(id, Duration::from_secs(30)).unwrap();
        assert_eq!(done.summary.failed, 0, "{:#?}", done.jobs);
        for job in &done.jobs {
            let name = job.request.source.file_name().unwrap();
            let original = fs::read(fixtures.join(name)).unwrap();
            assert_eq!(fs::read(&job.request.source).unwrap(), original);
            let mut standalone = PngRequest::new(&job.request.source);
            standalone.mode = mode;
            standalone.output = OutputPolicy::Copy {
                destination: dir
                    .path()
                    .join(format!("reference-{}", name.to_string_lossy())),
            };
            let reference =
                optimize_png(&standalone, &CancellationToken::default(), |_| {}).unwrap();
            let report = match &job.state {
                JobState::Succeeded(report) | JobState::NoGain(report) => report,
                other => panic!("意外结果: {other:?}"),
            };
            assert_eq!(report.processing, reference.processing);
            match (&report.outcome, reference.outcome) {
                (ProcessingOutcome::NoGain, ProcessingOutcome::NoGain) => {}
                (
                    ProcessingOutcome::Optimized { output, .. },
                    ProcessingOutcome::Optimized {
                        output: expected, ..
                    },
                ) => {
                    assert_eq!(
                        fs::read(output).unwrap(),
                        fs::read(expected).unwrap(),
                        "{name:?}"
                    );
                }
                other => panic!("并发与同步输出不同: {other:?}"),
            }
            assert_eq!(fs::read(&job.request.source).unwrap(), original);
        }
        assert_eq!((done.active_workers, done.reserved_working_bytes.0), (0, 0));
    }
}
