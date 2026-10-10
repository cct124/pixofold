//! 显式真实GIF单文件/混合核心验收，仅写入调用方提供的隔离语料目录。
use pixofold_core::{
    gif::{GifEngine, GifError, GifLimits, GifRequest, optimize_gif, validate_gif},
    model::{ByteCount, CancellationToken, OutputPolicy, ProcessingOutcome, ProcessingStage},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn copied(fixtures: &Path, root: &Path, name: &str) -> PathBuf {
    let path = root.join(name);
    fs::copy(fixtures.join("larger-pattern.gif"), &path).unwrap();
    path
}
fn main() {
    let memory = memory::Sampler::start();
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    assert_eq!(args.len(), 3, "需要可信工具目录、SHA256和隔离语料目录");
    let text = args[1].to_str().unwrap();
    assert_eq!(text.len(), 64);
    assert!(text.is_ascii());
    let sha256 = std::array::from_fn(|index| {
        u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).unwrap()
    });
    let engine = GifEngine::load(Path::new(&args[0]), sha256).unwrap();
    assert!(GifEngine::load(Path::new(&args[0]), [0; 32]).is_err());
    let fixtures = Path::new(&args[2]);
    let root = fixtures.join("core-results");
    fs::create_dir(&root).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(fixtures.join("manifest.json")).unwrap()).unwrap();
    let mut rows = Vec::new();
    let mut gain = 0;
    let mut no_gain = 0;
    for sample in manifest["samples"].as_array().unwrap() {
        let file = sample["file"].as_str().unwrap();
        let input = fixtures.join(file);
        let original = fs::read(&input).unwrap();
        let mut request = GifRequest::new(&input);
        request.output = OutputPolicy::Copy {
            destination: root.join(file),
        };
        if let Some(limits) = sample.get("limits") {
            request.limits.validation = serde_json::from_value(limits.clone()).unwrap();
        }
        let result = optimize_gif(&request, &engine, &CancellationToken::default(), |_| {});
        if sample["expect"] == "ok" {
            let report = result.unwrap();
            match &report.outcome {
                ProcessingOutcome::Optimized { output, backup } => {
                    assert!(backup.is_none());
                    let candidate = fs::read(output).unwrap();
                    assert!(candidate.len() < original.len());
                    assert!(
                        validate_gif(&original, request.limits, &CancellationToken::default())
                            .unwrap()
                            .equivalent(
                                &validate_gif(
                                    &candidate,
                                    request.limits,
                                    &CancellationToken::default()
                                )
                                .unwrap()
                            )
                    );
                    gain += 1;
                }
                ProcessingOutcome::NoGain => {
                    assert!(!root.join(file).exists());
                    assert_eq!(report.input_bytes, report.output_bytes);
                    no_gain += 1;
                }
            }
            rows.push(json!({"file":file,"inputBytes":report.input_bytes.0,"outputBytes":report.output_bytes.0,"frames":report.image.frames,"inputSha256":hash(&original)}));
        } else {
            assert!(result.is_err(), "必须拒绝：{file}");
        }
        assert_eq!(fs::read(input).unwrap(), original);
    }
    assert!(gain > 0 && no_gain > 0);
    let mut stress = Vec::new();
    for file in ["large-static.gif", "stress.gif"] {
        let source = fixtures.join(file);
        let original = fs::read(&source).unwrap();
        let mut request = GifRequest::new(&source);
        request.output = OutputPolicy::Copy {
            destination: root.join(file),
        };
        let report =
            optimize_gif(&request, &engine, &CancellationToken::default(), |_| {}).unwrap();
        assert_eq!(fs::read(&source).unwrap(), original);
        stress.push(json!({"file":file,"width":report.image.width,"height":report.image.height,"frames":report.image.frames,"inputBytes":report.input_bytes.0,"outputBytes":report.output_bytes.0,"elapsedMs":report.elapsed.as_millis()}));
    }
    let source = copied(fixtures, &root, "原图.GiF");
    let original = fs::read(&source).unwrap();
    let report = optimize_gif(
        &GifRequest::new(&source),
        &engine,
        &CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    let ProcessingOutcome::Optimized {
        backup: Some(backup),
        ..
    } = report.outcome
    else {
        panic!("必须成功并有备份");
    };
    assert_eq!(backup.extension().unwrap(), "GiF");
    assert_eq!(fs::read(backup).unwrap(), original);
    let source = copied(fixtures, &root, "no-backup.gif");
    let mut request = GifRequest::new(&source);
    request.output = OutputPolicy::OverwriteWithoutBackup;
    assert!(matches!(
        optimize_gif(&request, &engine, &CancellationToken::default(), |_| {})
            .unwrap()
            .outcome,
        ProcessingOutcome::Optimized { backup: None, .. }
    ));
    for stage in [
        ProcessingStage::Reading,
        ProcessingStage::Optimizing,
        ProcessingStage::Validating,
        ProcessingStage::BeforeCommit,
    ] {
        let directory = root.join(format!("cancel-{stage:?}"));
        fs::create_dir(&directory).unwrap();
        let source = copied(fixtures, &directory, "input.gif");
        let bytes = fs::read(&source).unwrap();
        let cancel = CancellationToken::default();
        assert!(matches!(
            optimize_gif(&GifRequest::new(&source), &engine, &cancel, |current| {
                if current == stage {
                    cancel.cancel();
                }
            }),
            Err(GifError::Cancelled)
        ));
        assert_eq!(fs::read(source).unwrap(), bytes);
        assert_eq!(fs::read_dir(directory).unwrap().count(), 1);
    }
    let readonly = copied(fixtures, &root, "readonly.gif");
    for changed in [
        "changed-delay.gif",
        "changed-pixel.gif",
        "changed-comment.gif",
        "finite-loop3.gif",
    ] {
        let directory = root.join(format!("candidate-{changed}"));
        fs::create_dir(&directory).unwrap();
        let source = copied(fixtures, &directory, "source.gif");
        let original = fs::read(&source).unwrap();
        let replacement = fs::read(fixtures.join(changed)).unwrap();
        let result = optimize_gif(
            &GifRequest::new(&source),
            &engine,
            &CancellationToken::default(),
            |stage| {
                if stage == ProcessingStage::Validating {
                    let temporary = fs::read_dir(&directory)
                        .unwrap()
                        .map(|entry| entry.unwrap().path())
                        .find(|path| {
                            path.file_name()
                                .unwrap()
                                .to_str()
                                .unwrap()
                                .starts_with(".pixofold-output-")
                        })
                        .unwrap();
                    fs::write(temporary, &replacement).unwrap();
                }
            },
        );
        assert!(matches!(result, Err(GifError::ValidationFailed)));
        assert_eq!(fs::read(&source).unwrap(), original);
        assert_eq!(fs::read_dir(directory).unwrap().count(), 1);
    }
    let original = fs::read(&readonly).unwrap();
    let permissions = fs::metadata(&readonly).unwrap().permissions();
    let mut locked = permissions.clone();
    locked.set_readonly(true);
    fs::set_permissions(&readonly, locked).unwrap();
    assert!(matches!(
        optimize_gif(
            &GifRequest::new(&readonly),
            &engine,
            &CancellationToken::default(),
            |_| {}
        ),
        Err(GifError::File(_))
    ));
    assert_eq!(fs::read(&readonly).unwrap(), original);
    fs::set_permissions(&readonly, permissions).unwrap();
    let tool_root = root.join("tool-identity");
    fs::create_dir(&tool_root).unwrap();
    let name = if cfg!(windows) {
        "pixofold-gif-helper.exe"
    } else {
        "pixofold-gif-helper"
    };
    fs::copy(Path::new(&args[0]).join(name), tool_root.join(name)).unwrap();
    let replaced = GifEngine::load(&tool_root, sha256).unwrap();
    fs::write(tool_root.join(name), b"replaced").unwrap();
    let source = copied(fixtures, &root, "identity.gif");
    let original = fs::read(&source).unwrap();
    assert!(matches!(
        optimize_gif(
            &GifRequest::new(&source),
            &replaced,
            &CancellationToken::default(),
            |_| {}
        ),
        Err(GifError::ToolIdentity)
    ));
    assert_eq!(fs::read(&source).unwrap(), original);
    let source = copied(fixtures, &root, "quota.gif");
    let bytes = fs::read(&source).unwrap();
    let mut request = GifRequest::new(&source);
    request.limits.max_native_bytes = ByteCount(1);
    assert!(matches!(
        optimize_gif(&request, &engine, &CancellationToken::default(), |_| {}),
        Err(GifError::ResourceLimit("GIF原生分配"))
    ));
    assert_eq!(fs::read(&source).unwrap(), bytes);
    request.limits = GifLimits::default();
    request.limits.resources.max_decoded_bytes = ByteCount(1024);
    assert!(matches!(
        optimize_gif(&request, &engine, &CancellationToken::default(), |_| {}),
        Err(GifError::ResourceLimit(_))
    ));
    request.limits = GifLimits::default();
    request.limits.process_timeout = Duration::from_millis(1);
    let deadline = optimize_gif(&request, &engine, &CancellationToken::default(), |_| {});
    assert!(matches!(deadline, Ok(_) | Err(GifError::Timeout))); // 快速工具允许在期限前完成，阻塞期限由进程回归固定验证。
    let source = copied(fixtures, &root, "conflict.gif");
    let bytes = fs::read(&source).unwrap();
    let target = root.join("occupied.gif");
    fs::write(&target, b"occupied").unwrap();
    let mut request = GifRequest::new(&source);
    request.output = OutputPolicy::Copy {
        destination: target.clone(),
    };
    assert!(optimize_gif(&request, &engine, &CancellationToken::default(), |_| {}).is_err());
    assert_eq!(fs::read(target).unwrap(), b"occupied");
    assert_eq!(fs::read(&source).unwrap(), bytes);
    request.output = OutputPolicy::Copy {
        destination: root.join("absent-parent/out.gif"),
    };
    assert!(optimize_gif(&request, &engine, &CancellationToken::default(), |_| {}).is_err());
    assert_eq!(fs::read(&source).unwrap(), bytes);
    request.output = OutputPolicy::Overwrite;
    let changed = b"changed by fixture";
    assert!(matches!(
        optimize_gif(&request, &engine, &CancellationToken::default(), |stage| {
            if stage == ProcessingStage::Validating {
                fs::write(&source, changed).unwrap();
            }
        }),
        Err(GifError::File(_))
    ));
    assert_eq!(fs::read(&source).unwrap(), changed);
    fs::write(root.join("report.json"), serde_json::to_vec_pretty(&json!({"result":"passed","gain":gain,"noGain":no_gain,"cases":rows,"boundaries":"native quota, verifier quota, cancellation stages, backup, noclobber, source change, I/O failure"})).unwrap()).unwrap();
    fs::write(
        root.join("metrics.json"),
        serde_json::to_vec_pretty(&json!({"stress":stress,"memory":memory.finish()})).unwrap(),
    )
    .unwrap();
    println!("GIF真实核心回归通过：{gain}收益/{no_gain}无收益及2项压力图");
}
#[path = "gif_lab/memory.rs"]
mod memory;
