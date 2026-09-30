//! 显式真实引擎回归入口，由jpeg:core:check提供固定工具和自生成语料。
//! 缺工具/语料即失败，不在普通测试中默默跳过；所有写入限隔离目录。

use pixofold_core::{
    import::{ScanOptions, scan},
    jpeg::{JpegEngine, JpegError, JpegRequest, optimize_jpeg},
    model::{CancellationToken, OutputPolicy, ProcessingError, ProcessingOutcome, ProcessingStage},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[path = "../tests/support/native_names.rs"]
mod native_names;

fn run(
    request: &JpegRequest,
    engine: &JpegEngine,
) -> Result<pixofold_core::jpeg::JpegReport, JpegError> {
    optimize_jpeg(request, engine, &CancellationToken::default(), |_| {})
}
fn assert_only(directory: &Path, count: usize) {
    assert_eq!(
        fs::read_dir(directory).unwrap().count(),
        count,
        "残留临时产物"
    );
}
fn case(root: &Path, name: &str, bytes: &[u8]) -> (PathBuf, JpegRequest) {
    let dir = root.join(name);
    fs::create_dir(&dir).unwrap();
    let source = dir.join("图片.JpEg");
    fs::write(&source, bytes).unwrap();
    (dir, JpegRequest::new(source))
}

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    assert_eq!(args.len(), 3, "需要固定工具目录、可信SHA256和隔离语料目录");
    let hash = args[1].to_str().unwrap();
    assert_eq!(hash.len(), 64);
    let digest: [u8; 32] =
        std::array::from_fn(|i| u8::from_str_radix(&hash[i * 2..i * 2 + 2], 16).unwrap());
    let engine = JpegEngine::load(Path::new(&args[0]), digest).unwrap();
    let fixtures = Path::new(&args[2]);
    let root = fixtures.join("core-results");
    fs::create_dir(&root).unwrap();
    let base = fs::read(fixtures.join("baseline-420.jpg")).unwrap();
    let mut optimized = 0;
    let mut accepted = 0;
    for entry in fs::read_dir(fixtures).unwrap() {
        let source = entry.unwrap().path();
        if source
            .extension()
            .is_none_or(|extension| extension != "jpg")
        {
            continue;
        }
        let before = fs::read(&source).unwrap();
        let mut request = JpegRequest::new(&source);
        request.output = OutputPolicy::Copy {
            destination: root.join(source.file_name().unwrap()),
        };
        let result = run(&request, &engine);
        if source.file_stem().unwrap() == "opaque-app11" {
            assert!(matches!(result, Err(JpegError::ProtectedMetadata(0xeb))));
            assert!(!root.join(source.file_name().unwrap()).exists());
        } else {
            let report = result.unwrap_or_else(|error| panic!("{}: {error}", source.display()));
            accepted += 1;
            match report.outcome {
                ProcessingOutcome::Optimized { output, backup } => {
                    assert!(backup.is_none());
                    assert!(fs::metadata(output).unwrap().len() < before.len() as u64);
                    optimized += 1;
                }
                ProcessingOutcome::NoGain => {
                    assert!(!root.join(source.file_name().unwrap()).exists())
                }
            }
        }
        assert_eq!(fs::read(&source).unwrap(), before);
    }
    assert_eq!(accepted, 20);
    assert!(optimized > 0);

    for (index, extension) in ["jpg", "jpeg", "JPG", "JpEg"].into_iter().enumerate() {
        let dir = root.join(format!("backup-{index}"));
        fs::create_dir(&dir).unwrap();
        let source = dir.join(format!("原图🌄.v2.{extension}"));
        fs::write(&source, &base).unwrap();
        let result = run(&JpegRequest::new(&source), &engine).unwrap();
        let ProcessingOutcome::Optimized {
            backup: Some(backup),
            ..
        } = result.outcome
        else {
            panic!("需要有收益备份");
        };
        assert_eq!(fs::read(&backup).unwrap(), base);
        assert_eq!(backup.extension(), source.extension());
        let name = backup.file_stem().unwrap().to_str().unwrap();
        let token = name.strip_prefix("原图🌄.v2-backup-").unwrap();
        assert_eq!(token.len(), 6);
        assert!(token.bytes().all(|b| b.is_ascii_alphanumeric()));
        let found = scan(
            std::slice::from_ref(&dir),
            ScanOptions::default(),
            &CancellationToken::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!(found.progress().excluded, 1);
        assert_only(&dir, 2);
    }

    let (dir, mut request) = case(&root, "without-backup", &base);
    request.output = OutputPolicy::OverwriteWithoutBackup;
    assert!(matches!(
        run(&request, &engine).unwrap().outcome,
        ProcessingOutcome::Optimized { backup: None, .. }
    ));
    assert_only(&dir, 1);

    let native_dir = root.join("native-name");
    fs::create_dir(&native_dir).unwrap();
    let native_path = native_names::write_native_name(&native_dir, ".JpEg", &base);
    let report = run(&JpegRequest::new(&native_path), &engine).unwrap();
    let ProcessingOutcome::Optimized {
        backup: Some(backup),
        ..
    } = report.outcome
    else {
        panic!("需要真实备份");
    };
    assert_eq!(fs::read(&backup).unwrap(), base);
    assert_eq!(backup.extension(), native_path.extension());
    let mut prefix = native_path.file_stem().unwrap().to_os_string();
    prefix.push("-backup-");
    assert!(
        backup
            .file_name()
            .unwrap()
            .as_encoded_bytes()
            .starts_with(prefix.as_encoded_bytes())
    );
    let found = scan(
        std::slice::from_ref(&native_dir),
        ScanOptions::default(),
        &CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(found.progress().excluded, 1);

    let (tree_dir, mut tree_request) = case(&root, "copy-tree", &base);
    let output_root = tree_dir.join("outputs");
    fs::create_dir(&output_root).unwrap();
    tree_request.output = OutputPolicy::CopyTree {
        root: output_root.clone(),
        relative: PathBuf::from("nested/original.JpEg"),
    };
    assert!(matches!(
        run(&tree_request, &engine).unwrap().outcome,
        ProcessingOutcome::Optimized { backup: None, .. }
    ));
    assert!(output_root.join("nested/original.JpEg").is_file());
    assert_eq!(fs::read(&tree_request.source).unwrap(), base);
    let optimized_input = fs::read(&request.source).unwrap();
    request.output = OutputPolicy::Overwrite;
    assert!(matches!(
        run(&request, &engine).unwrap().outcome,
        ProcessingOutcome::NoGain
    ));
    assert_eq!(fs::read(&request.source).unwrap(), optimized_input);
    assert_only(&dir, 1);
    request.output = OutputPolicy::Copy {
        destination: dir.join("new.jpg"),
    };
    assert!(matches!(
        run(&request, &engine).unwrap().outcome,
        ProcessingOutcome::NoGain
    ));
    assert_only(&dir, 1);

    let (dir, mut request) = case(&root, "conflict", &base);
    let target = dir.join("copy.jpg");
    fs::write(&target, b"existing").unwrap();
    request.output = OutputPolicy::Copy {
        destination: target.clone(),
    };
    assert!(matches!(
        run(&request, &engine),
        Err(JpegError::File(ProcessingError::TargetConflict))
    ));
    assert_eq!(fs::read(&target).unwrap(), b"existing");
    assert_only(&dir, 2);

    let (dir, mut request) = case(&root, "late-conflict", &base);
    let target = dir.join("copy.jpg");
    request.output = OutputPolicy::Copy {
        destination: target.clone(),
    };
    let result = optimize_jpeg(&request, &engine, &CancellationToken::default(), |stage| {
        if stage == ProcessingStage::BeforeCommit {
            fs::write(&target, b"late").unwrap();
        }
    });
    assert!(matches!(
        result,
        Err(JpegError::File(ProcessingError::TargetConflict))
    ));
    assert_eq!(fs::read(&target).unwrap(), b"late");
    assert_eq!(fs::read(&request.source).unwrap(), base);
    assert_only(&dir, 2);

    for stage_to_cancel in [
        ProcessingStage::Reading,
        ProcessingStage::Optimizing,
        ProcessingStage::Validating,
        ProcessingStage::BeforeCommit,
    ] {
        let (dir, request) = case(&root, &format!("cancel-{stage_to_cancel:?}"), &base);
        let cancel = CancellationToken::default();
        let result = optimize_jpeg(&request, &engine, &cancel, |stage| {
            if stage == stage_to_cancel {
                cancel.cancel();
            }
        });
        assert!(matches!(result, Err(JpegError::Cancelled)));
        assert_eq!(fs::read(&request.source).unwrap(), base);
        assert_only(&dir, 1);
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
    assert_eq!(fs::read(&request.source).unwrap(), b"external-change");
    assert_only(&dir, 1);

    for stage_to_change in [ProcessingStage::Validating, ProcessingStage::BeforeCommit] {
        let (dir, request) = case(&root, &format!("tamper-{stage_to_change:?}"), &base);
        let result = optimize_jpeg(&request, &engine, &CancellationToken::default(), |stage| {
            if stage == stage_to_change {
                for entry in fs::read_dir(&dir).unwrap() {
                    let path = entry.unwrap().path();
                    if path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(".pixofold-output-")
                    {
                        fs::write(path, b"bad candidate").unwrap();
                    }
                }
            }
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&request.source).unwrap(), base);
        assert_only(&dir, 1);
    }

    let (dir, mut request) = case(&root, "limits", &base);
    request.limits.resources.max_pixels = 1;
    assert!(matches!(
        run(&request, &engine),
        Err(JpegError::ResourceLimit(_))
    ));
    request.limits = Default::default();
    request.limits.resources.max_dimension = 1;
    assert!(matches!(
        run(&request, &engine),
        Err(JpegError::ResourceLimit(_))
    ));
    request.limits = Default::default();
    request.limits.resources.max_input_bytes.0 = 1;
    assert!(matches!(
        run(&request, &engine),
        Err(JpegError::File(ProcessingError::ResourceLimit(_)))
    ));
    request.limits = Default::default();
    request.limits.resources.max_decoded_bytes.0 = 1024 * 1024;
    assert!(matches!(
        run(&request, &engine),
        Err(JpegError::ResourceLimit(_))
    ));
    assert_only(&dir, 1);

    let (dir, request) = case(&root, "coefficient-tamper", &base);
    let result = optimize_jpeg(&request, &engine, &CancellationToken::default(), |stage| {
        if stage == ProcessingStage::Validating {
            for entry in fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().is_some_and(|e| e == "tmp") {
                    let mut bytes = fs::read(&path).unwrap();
                    let quant = bytes.windows(2).position(|w| w == [0xff, 0xdb]).unwrap();
                    bytes[quant + 5] = if bytes[quant + 5] == 1 { 2 } else { 1 };
                    fs::write(path, bytes).unwrap();
                }
            }
        }
    });
    assert!(matches!(result, Err(JpegError::ValidationFailed)));
    assert_eq!(fs::read(&request.source).unwrap(), base);
    assert_only(&dir, 1);

    // 保留合法结构/EOI但截断熵数据，必须由实际原生解码拒绝，不能只依赖Rust标记预检。
    let scan_start = base.windows(2).position(|w| w == [0xff, 0xda]).unwrap();
    let scan_length = u16::from_be_bytes([base[scan_start + 2], base[scan_start + 3]]) as usize;
    let mut bad_entropy = base[..scan_start + 2 + scan_length].to_vec();
    bad_entropy.extend_from_slice(&[0xff, 0xd9]);
    let (dir, request) = case(&root, "bad-entropy", &bad_entropy);
    assert!(matches!(
        run(&request, &engine),
        Err(JpegError::ToolExit(_))
    ));
    assert_only(&dir, 1);

    let mut twelve_bit = base.clone();
    let frame = twelve_bit
        .windows(2)
        .position(|w| w == [0xff, 0xc0])
        .unwrap();
    twelve_bit[frame + 4] = 12;
    let (dir, request) = case(&root, "twelve-bit", &twelve_bit);
    assert!(matches!(
        run(&request, &engine),
        Err(JpegError::UnsupportedJpeg)
    ));
    assert_only(&dir, 1);
    let (_, mut request) = case(
        &root,
        "scans",
        &fs::read(fixtures.join("progressive.jpg")).unwrap(),
    );
    request.limits.max_scans = 1;
    assert!(matches!(
        run(&request, &engine),
        Err(JpegError::ResourceLimit(_))
    ));

    for (name, bytes) in [
        ("truncated", &base[..base.len() / 2]),
        ("invalid", b"bad JPEG".as_slice()),
    ] {
        let (dir, request) = case(&root, name, bytes);
        assert!(matches!(
            run(&request, &engine),
            Err(JpegError::InvalidJpeg(_))
        ));
        assert_only(&dir, 1);
    }
    assert!(matches!(
        JpegEngine::load(Path::new(&args[0]), [0; 32]),
        Err(JpegError::ToolIdentity)
    ));
    assert!(matches!(
        JpegEngine::load(&root, digest),
        Err(JpegError::ToolIdentity)
    ));
    assert!(matches!(
        JpegEngine::load(Path::new("."), digest),
        Err(JpegError::ToolIdentity)
    ));
    let tool_copy = root.join("tool-change");
    fs::create_dir(&tool_copy).unwrap();
    let tool_name = if cfg!(windows) {
        "pixofold-jpeg-helper.exe"
    } else {
        "pixofold-jpeg-helper"
    };
    let copied = tool_copy.join(tool_name);
    fs::copy(Path::new(&args[0]).join(tool_name), &copied).unwrap();
    let changed_engine = JpegEngine::load(&tool_copy, digest).unwrap();
    fs::write(&copied, b"externally replaced bytes").unwrap();
    let (dir, request) = case(&root, "tool-changed-refused", &base);
    assert!(matches!(
        run(&request, &changed_engine),
        Err(JpegError::ToolIdentity)
    ));
    assert_eq!(fs::read(&request.source).unwrap(), base);
    assert_only(&dir, 1);
    println!(
        "JPEG核心通过：{accepted}项真实语料（{optimized}项有收益），APP11拒绝、输出/备份/NoGain/冲突/取消/变化/资源/工具身份回归。"
    );
}
