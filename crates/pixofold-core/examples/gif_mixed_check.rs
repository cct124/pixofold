//! PNG/JPEG/GIF真实核心混合验收；GIF开放桌面仍由后续协议和随包阶段决定。
use pixofold_core::{batch::*, gif::*, import::*, jpeg::JpegEngine, model::*};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
fn digest(text: &std::ffi::OsStr) -> [u8; 32] {
    let text = text.to_str().unwrap();
    assert_eq!(text.len(), 64);
    assert!(text.is_ascii());
    std::array::from_fn(|i| u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap())
}
fn folder(root: &Path, name: &str) -> PathBuf {
    let path = root.join(name);
    fs::create_dir(&path).unwrap();
    path
}
fn put(directory: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = directory.join(name);
    fs::write(&path, bytes).unwrap();
    path
}
fn done(service: &BatchService, id: BatchId) -> BatchSnapshot {
    let result = service.wait(id, Duration::from_secs(60)).unwrap();
    assert_eq!(result.phase, BatchPhase::Finished);
    assert_eq!(result.summary.terminal, result.summary.total);
    assert_eq!(result.active_workers, 0);
    assert_eq!(result.reserved_working_bytes, ByteCount(0));
    result
}
fn main() {
    let memory = memory::Sampler::start();
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    assert_eq!(args.len(), 5);
    let engines =
        ImageEngines::with_jpeg(JpegEngine::load(Path::new(&args[2]), digest(&args[3])).unwrap())
            .add_gif(GifEngine::load(Path::new(&args[0]), digest(&args[1])).unwrap());
    let fixtures = Path::new(&args[4]);
    let root = folder(fixtures, "mixed-results");
    let input = folder(&root, "inputs");
    let output = folder(&root, "outputs");
    let png = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/png/gradient-rgb8.png"),
    )
    .unwrap();
    let gif = fs::read(fixtures.join("larger-pattern.gif")).unwrap();
    let jpeg = fs::read(fixtures.join("baseline-420.jpg")).unwrap();
    put(&input, "图片.PNG", &png);
    put(&input, "图片.JPG", &jpeg);
    let gif_source = put(&input, "动画.GiF", &gif);
    put(
        &input,
        "bad.gif",
        &fs::read(fixtures.join("bad-lzw.gif")).unwrap(),
    );
    put(
        &input,
        "already.gif",
        &fs::read(fixtures.join("interlaced-dictionary.gif")).unwrap(),
    );
    // 普通扫描保持默认能力；显式引擎扫描才接纳GIF。
    let default = scan(
        std::slice::from_ref(&gif_source),
        ScanOptions::default(),
        &CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    assert!(default.files().is_empty());
    let found = scan_with_engines(
        std::slice::from_ref(&input),
        ScanOptions::default(),
        &CancellationToken::default(),
        |_| {},
        &engines,
    )
    .unwrap();
    assert_eq!(found.files().len(), 5);
    let originals: Vec<_> = found
        .files()
        .iter()
        .map(|file| (file.source.clone(), fs::read(&file.source).unwrap()))
        .collect();
    let request = found
        .plan(
            &ImportOutput::CopyTo {
                directory: output.clone(),
                layout: CopyLayout::Flat,
            },
            BatchParameters::default(),
        )
        .unwrap();
    let mut service = BatchService::with_engines(
        BatchConfig {
            workers: 3,
            working_set_budget: ByteCount(256 * 1024 * 1024),
            ..BatchConfig::default()
        },
        engines.clone(),
    )
    .unwrap();
    let id = service.start(request).unwrap();
    let finished = done(&service, id);
    assert!(
        finished
            .jobs
            .iter()
            .any(|row| row.request.format() == ImageKind::Png && !row.state.can_retry())
    );
    assert!(
        finished
            .jobs
            .iter()
            .any(|row| matches!(&row.state, JobState::Succeeded(ImageReport::Jpeg(_))))
    );
    assert!(
        finished
            .jobs
            .iter()
            .any(|row| matches!(&row.state, JobState::Succeeded(ImageReport::Gif(_))))
    );
    assert!(finished.jobs.iter().any(
        |row| matches!(&row.state,JobState::Failed(error) if error.code==JobErrorCode::Decode)
    ));
    assert!(
        finished
            .jobs
            .iter()
            .any(|row| matches!(&row.state, JobState::NoGain(ImageReport::Gif(_))))
    );
    for (source, bytes) in &originals {
        assert_eq!(fs::read(source).unwrap(), *bytes);
    }
    assert!(
        validate_gif(&gif, GifLimits::default(), &CancellationToken::default())
            .unwrap()
            .equivalent(
                &validate_gif(
                    &fs::read(output.join("动画.GiF")).unwrap(),
                    GifLimits::default(),
                    &CancellationToken::default()
                )
                .unwrap()
            )
    );
    // 原生预算失败后仅重试失败行；保留其稳定ID/源和成功行结果。
    let retry_source = put(&root, "retry.gif", &gif);
    let retry_output = root.join("retry-output.gif");
    let mut parameters = BatchParameters::default();
    parameters.gif.max_native_bytes = ByteCount(1);
    let id = service
        .start(BatchRequest {
            items: vec![BatchItem {
                source: retry_source.clone(),
                output: OutputPolicy::Copy {
                    destination: retry_output.clone(),
                },
                format: ImageKind::Gif,
            }],
            parameters,
            engines: engines.clone(),
        })
        .unwrap();
    let failed = done(&service, id);
    assert!(
        matches!(&failed.jobs[0].state,JobState::Failed(error)if error.code==JobErrorCode::ResourceLimit)
    );
    let stable = failed.jobs[0].id;
    service
        .retry(
            id,
            RetryRequest {
                jobs: vec![RetryJob {
                    id: stable,
                    output: OutputPolicy::Copy {
                        destination: retry_output.clone(),
                    },
                    metadata: PngMetadataPolicy::Preserve,
                }],
                parameters: BatchParameters::default(),
            },
        )
        .unwrap();
    let retried = done(&service, id);
    assert_eq!(retried.jobs[0].id, stable);
    assert_eq!(retried.jobs[0].attempt, 2);
    assert!(matches!(
        &retried.jobs[0].state,
        JobState::Succeeded(ImageReport::Gif(_))
    ));
    assert_eq!(fs::read(&retry_source).unwrap(), gif);
    // 不足全图预约进入Finished失败行；释放预算后可继续接纳任务。
    let mut tight = BatchService::with_engines(
        BatchConfig {
            workers: 2,
            working_set_budget: ByteCount(32 * 1024 * 1024),
            ..BatchConfig::default()
        },
        engines.clone(),
    )
    .unwrap();
    let id = tight
        .start(BatchRequest {
            items: vec![BatchItem {
                source: retry_source.clone(),
                output: OutputPolicy::Copy {
                    destination: root.join("tight.gif"),
                },
                format: ImageKind::Gif,
            }],
            parameters: BatchParameters::default(),
            engines: engines.clone(),
        })
        .unwrap();
    assert!(
        matches!(&done(&tight,id).jobs[0].state,JobState::Failed(error)if error.code==JobErrorCode::ResourceLimit)
    );
    tight.shutdown().unwrap();
    // GIF有损仅该行明确拒绝；GIF无损不会继承PNG凭据授权。
    let parameters = BatchParameters {
        mode: PngMode::Lossy {
            quality: QualityValue::new(80).unwrap(),
        },
        ..BatchParameters::default()
    };
    let id = service
        .start(BatchRequest {
            items: vec![BatchItem {
                source: retry_source.clone(),
                output: OutputPolicy::Copy {
                    destination: root.join("lossy.gif"),
                },
                format: ImageKind::Gif,
            }],
            parameters,
            engines: engines.clone(),
        })
        .unwrap();
    let unsupported = done(&service, id);
    assert!(
        matches!(&unsupported.jobs[0].state,JobState::Failed(error)if error.code==JobErrorCode::UnsupportedFormat)
    );
    assert!(unsupported.jobs[0].content_credentials_source().is_none());
    let occupied = root.join("occupied.gif");
    fs::write(&occupied, b"occupied").unwrap();
    let success = root.join("good-copy.PNG");
    let id = service
        .start(BatchRequest {
            items: vec![
                BatchItem {
                    source: retry_source.clone(),
                    output: OutputPolicy::Copy {
                        destination: occupied.clone(),
                    },
                    format: ImageKind::Gif,
                },
                BatchItem {
                    source: input.join("图片.PNG"),
                    output: OutputPolicy::Copy {
                        destination: success,
                    },
                    format: ImageKind::Png,
                },
            ],
            parameters: BatchParameters::default(),
            engines: engines.clone(),
        })
        .unwrap();
    let conflict = done(&service, id);
    assert!(
        matches!(&conflict.jobs[0].state,JobState::Failed(error)if error.code==JobErrorCode::TargetConflict)
    );
    assert!(!conflict.jobs[1].state.can_retry());
    assert_eq!(fs::read(occupied).unwrap(), b"occupied");
    service.shutdown().unwrap();
    let rows:Vec<_>=finished.jobs.iter().map(|row|json!({"file":row.request.source.file_name().unwrap().to_str().unwrap(),"format":match row.request.format(){ImageKind::Png=>"png",ImageKind::Jpeg=>"jpeg",ImageKind::Gif=>"gif"},"state":match row.state{JobState::Succeeded(_)=>"succeeded",JobState::NoGain(_)=>"no_gain",JobState::Failed(_)=>"failed",_=>"unexpected"}})).collect();
    fs::write(root.join("report.json"),serde_json::to_vec_pretty(&json!({"result":"passed","jobs":rows,"retryStableId":stable.get(),"workerBudgetMiB":256,"sourceGifSha256":format!("{:x}",Sha256::digest(&gif)),"boundaries":["bad pixels","NoGain","native quota retry","total budget","unsupported lossy","per-row conflicts"]})).unwrap()).unwrap();
    fs::write(
        root.join("metrics.json"),
        serde_json::to_vec_pretty(&memory.finish()).unwrap(),
    )
    .unwrap();
    println!("PNG/JPEG/GIF真实混合核心验收通过");
}
#[path = "gif_lab/memory.rs"]
mod memory;
