//! JPEG保守有损的真实核心与安全输出验收；jpeg:core:check提供隔离语料及来源记录。

use pixofold_core::{
    jpeg::{JpegEngine, JpegError, JpegMode, JpegProcessing, JpegRequest, optimize_jpeg},
    model::{
        CancellationToken, OutputPolicy, ProcessingError, ProcessingOutcome, ProcessingStage,
        QualityValue,
    },
};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn run(
    request: &JpegRequest,
    engine: &JpegEngine,
) -> Result<pixofold_core::jpeg::JpegReport, JpegError> {
    optimize_jpeg(request, engine, &CancellationToken::default(), |_| {})
}

fn case(root: &Path, name: &str, bytes: &[u8]) -> (PathBuf, JpegRequest) {
    let directory = root.join(name);
    fs::create_dir(&directory).unwrap();
    let source = directory.join("图片.JpEg");
    fs::write(&source, bytes).unwrap();
    let mut request = JpegRequest::new(source);
    request.mode = JpegMode::Lossy {
        quality: QualityValue::default(),
    };
    (directory, request)
}

fn unchanged(directory: &Path, source: &Path, original: &[u8], files: usize) {
    assert_eq!(fs::read(source).unwrap(), original);
    assert_eq!(
        fs::read_dir(directory).unwrap().count(),
        files,
        "不能残留候选或未经提交的备份"
    );
}

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    assert_eq!(args.len(), 3, "需要固定工具目录、可信SHA256和隔离语料目录");
    let hash = args[1].to_str().unwrap();
    assert_eq!(hash.len(), 64);
    let digest = std::array::from_fn(|i| u8::from_str_radix(&hash[i * 2..i * 2 + 2], 16).unwrap());
    let engine = JpegEngine::load(Path::new(&args[0]), digest).unwrap();
    let fixtures = Path::new(&args[2]);
    let root = fixtures.join("lossy-results");
    fs::create_dir(&root).unwrap();
    let base = fs::read(fixtures.join("baseline-420.jpg")).unwrap();
    let mut results = Vec::new();
    for entry in fs::read_dir(fixtures).unwrap() {
        let source = entry.unwrap().path();
        if source.extension().is_none_or(|ext| ext != "jpg") {
            continue;
        }
        let name = source.file_stem().unwrap().to_str().unwrap();
        let before = fs::read(&source).unwrap();
        for q in [0, 40, 80, 100] {
            let quality = QualityValue::new(q).unwrap();
            let output_name = format!("{name}-q{q}.jpg");
            let mut request = JpegRequest::new(&source);
            request.mode = JpegMode::Lossy { quality };
            request.output = OutputPolicy::Copy {
                destination: root.join(&output_name),
            };
            let result = run(&request, &engine);
            if name == "opaque-app11" {
                assert!(matches!(result, Err(JpegError::ProtectedMetadata(0xeb))));
                assert!(!root.join(output_name).exists());
                continue;
            }
            let report = result.unwrap_or_else(|error| panic!("{name}, q={q}: {error}"));
            let expected_fallback = match name {
                "icc" | "icc-segmented" => Some("ColorProfile"),
                "cmyk" | "ycck" => Some("FourComponentColor"),
                "jfif-thumbnail" => Some("EmbeddedThumbnail"),
                "ambiguous-color" => Some("AmbiguousColor"),
                _ => None,
            };
            let fallback = match report.processing {
                JpegProcessing::Lossy { parameters } => {
                    assert_eq!(parameters.quality, quality);
                    assert_eq!(parameters.native_quality, (q as u8).max(1));
                    None
                }
                JpegProcessing::LosslessFallback { parameters, reason } => {
                    assert_eq!(parameters.quality, quality);
                    Some(format!("{reason:?}"))
                }
                JpegProcessing::Lossless => panic!("有损请求须有实际处理报告"),
            };
            assert_eq!(fallback.as_deref(), expected_fallback);
            let optimized = matches!(report.outcome, ProcessingOutcome::Optimized { .. });
            assert_eq!(root.join(&output_name).exists(), optimized);
            if optimized {
                assert!(report.output_bytes < report.input_bytes);
            } else {
                assert_eq!(report.output_bytes, report.input_bytes);
            }
            results.push(json!({"sample":name,"quality":q,"fallback":fallback,"optimized":optimized,
                "inputBytes":report.input_bytes.0,"outputBytes":report.output_bytes.0,"output":output_name}));
            assert_eq!(fs::read(&source).unwrap(), before);
        }
    }
    assert_eq!(results.len(), 23 * 4);

    for policy in 0..3 {
        let (dir, mut request) = case(&root, &format!("policy-{policy}"), &base);
        request.output = match policy {
            0 => OutputPolicy::Overwrite,
            1 => OutputPolicy::OverwriteWithoutBackup,
            _ => OutputPolicy::Copy {
                destination: dir.join("副本.jpg"),
            },
        };
        let report = run(&request, &engine).unwrap();
        assert!(matches!(report.processing, JpegProcessing::Lossy { .. }));
        let ProcessingOutcome::Optimized { output, backup } = report.outcome else {
            panic!("需要有收益语料");
        };
        assert_eq!(backup.is_some(), policy == 0);
        if let Some(backup) = backup {
            assert_eq!(backup.extension(), request.source.extension());
            assert_eq!(fs::read(backup).unwrap(), base);
        }
        if policy == 2 {
            assert_eq!(fs::read(&request.source).unwrap(), base);
        }
        assert!(fs::read(output).unwrap().len() < base.len());
        assert_eq!(
            fs::read_dir(dir).unwrap().count(),
            if policy == 1 { 1 } else { 2 }
        );
    }

    let tiny = fs::read(fixtures.join("lossy-extra/no-gain.jpg")).unwrap();
    for policy in 0..3 {
        let (dir, mut request) = case(&root, &format!("no-gain-{policy}"), &tiny);
        request.mode = JpegMode::Lossy {
            quality: QualityValue::new(100).unwrap(),
        };
        request.output = match policy {
            0 => OutputPolicy::Overwrite,
            1 => OutputPolicy::OverwriteWithoutBackup,
            _ => OutputPolicy::Copy {
                destination: dir.join("not-created.jpg"),
            },
        };
        let report = run(&request, &engine).unwrap();
        assert!(matches!(report.processing, JpegProcessing::Lossy { .. }));
        assert!(matches!(report.outcome, ProcessingOutcome::NoGain));
        unchanged(&dir, &request.source, &tiny, 1);
    }

    for stop in [
        ProcessingStage::Reading,
        ProcessingStage::Optimizing,
        ProcessingStage::Validating,
        ProcessingStage::BeforeCommit,
    ] {
        let (dir, request) = case(&root, &format!("cancel-{stop:?}"), &base);
        let cancel = CancellationToken::default();
        let result = optimize_jpeg(&request, &engine, &cancel, |stage| {
            if stage == stop {
                cancel.cancel();
            }
        });
        assert!(matches!(result, Err(JpegError::Cancelled)));
        unchanged(&dir, &request.source, &base, 1);
    }
    for tamper in [ProcessingStage::Validating, ProcessingStage::BeforeCommit] {
        let (dir, request) = case(&root, &format!("valid-jpeg-tamper-{tamper:?}"), &base);
        let result = optimize_jpeg(&request, &engine, &CancellationToken::default(), |stage| {
            if stage == tamper {
                for entry in fs::read_dir(&dir).unwrap() {
                    let path = entry.unwrap().path();
                    if path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(".pixofold-output-")
                    {
                        fs::write(path, &base).unwrap(); // 同尺寸/元数据、完全可解码的另一张JPEG也必须拒绝。
                    }
                }
            }
        });
        assert!(matches!(
            result,
            Err(JpegError::ValidationFailed | JpegError::File(ProcessingError::ValidationFailed(_)))
        ));
        unchanged(&dir, &request.source, &base, 1);
    }
    let (dir, request) = case(&root, "source-change", &base);
    let result = optimize_jpeg(&request, &engine, &CancellationToken::default(), |stage| {
        if stage == ProcessingStage::BeforeCommit {
            fs::write(&request.source, b"external-change").unwrap();
        }
    });
    assert!(matches!(
        result,
        Err(JpegError::File(ProcessingError::SourceChanged))
    ));
    unchanged(&dir, &request.source, b"external-change", 1);
    for late in [false, true] {
        let (dir, mut request) = case(&root, &format!("conflict-{late}"), &base);
        let target = dir.join("already.jpg");
        request.output = OutputPolicy::Copy {
            destination: target.clone(),
        };
        if !late {
            fs::write(&target, b"preserve").unwrap();
        }
        let result = optimize_jpeg(&request, &engine, &CancellationToken::default(), |stage| {
            if late && stage == ProcessingStage::BeforeCommit {
                fs::write(&target, b"preserve").unwrap();
            }
        });
        assert!(matches!(
            result,
            Err(JpegError::File(ProcessingError::TargetConflict))
        ));
        assert_eq!(fs::read(target).unwrap(), b"preserve");
        unchanged(&dir, &request.source, &base, 2);
    }
    let (dir, mut request) = case(&root, "pixel-budget", &base);
    request.limits.resources.max_decoded_bytes.0 = 1280 * 1024;
    assert!(matches!(
        run(&request, &engine),
        Err(JpegError::ResourceLimit(_))
    ));
    unchanged(&dir, &request.source, &base, 1);
    // 同一限额仍足够系数无损，证明有损附加缓冲确实被单独计费。
    request.mode = JpegMode::Lossless;
    assert!(run(&request, &engine).is_ok());

    for entry in fs::read_dir(fixtures.join("lossy-extra/rejected")).unwrap() {
        let path = entry.unwrap().path();
        let bytes = fs::read(&path).unwrap();
        let (dir, request) = case(&root, path.file_stem().unwrap().to_str().unwrap(), &bytes);
        assert!(run(&request, &engine).is_err(), "损坏或受保护JPEG必须拒绝");
        unchanged(&dir, &request.source, &bytes, 1);
    }
    fs::write(
        fixtures.join("lossy-results.json"),
        serde_json::to_vec_pretty(&results).unwrap(),
    )
    .unwrap();
    println!(
        "JPEG有损核心通过：{}个质量/回退组合，NoGain、备份、副本、取消、源/候选变化及资源拒绝。",
        results.len()
    );
}
