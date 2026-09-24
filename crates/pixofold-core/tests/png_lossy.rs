//! 有损真实文件回归；故障在阶段边界注入，不依赖 GUI、网络或休眠。

use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

use pixofold_core::{
    model::{
        ByteCount, CancellationToken, LossyFallbackReason, OutputPolicy, PngColorType, PngMode,
        PngProcessing, PngRequest, ProcessingError, ProcessingOutcome, ProcessingReport,
        ProcessingStage, QualityValue, ResourceLimits,
    },
    pipeline::optimize_png,
    probe::inspect_png,
};
use tempfile::TempDir;

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/png")
            .join(name),
    )
    .unwrap()
}

fn workspace(name: &str, q: i32) -> (TempDir, PngRequest, Vec<u8>) {
    let directory = tempfile::tempdir().unwrap();
    let original = fixture(name);
    let source = directory.path().join("输入.png");
    fs::write(&source, &original).unwrap();
    let mut request = PngRequest::new(source);
    request.mode = PngMode::Lossy {
        quality: QualityValue::new(q).unwrap(),
    };
    (directory, request, original)
}

fn run(request: &PngRequest) -> Result<ProcessingReport, ProcessingError> {
    optimize_png(request, &CancellationToken::default(), |_| {})
}

fn files(directory: &Path) -> Vec<PathBuf> {
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect()
}

fn source_only(directory: &TempDir, request: &PngRequest, source: &[u8]) {
    assert_eq!(fs::read(&request.source).unwrap(), source);
    assert_eq!(files(directory.path()), vec![request.source.clone()]);
}

fn copy(request: &PngRequest, destination: PathBuf) -> (ProcessingReport, Vec<u8>) {
    let mut request = request.clone();
    request.output = OutputPolicy::Copy {
        destination: destination.clone(),
    };
    let report = run(&request).unwrap();
    let bytes = match report.outcome {
        ProcessingOutcome::Optimized { .. } => fs::read(destination).unwrap(),
        ProcessingOutcome::NoGain => fs::read(request.source).unwrap(),
    };
    (report, bytes)
}

fn rgba(data: &[u8]) -> Vec<[u8; 4]> {
    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut buffer = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut buffer).unwrap();
    reader.finish().unwrap();
    assert_eq!(frame.bit_depth, png::BitDepth::Eight);
    buffer[..frame.buffer_size()]
        .chunks_exact(frame.color_type.samples())
        .map(|p| match frame.color_type {
            png::ColorType::Rgb => [p[0], p[1], p[2], 255],
            png::ColorType::Rgba => [p[0], p[1], p[2], p[3]],
            png::ColorType::Grayscale => [p[0], p[0], p[0], 255],
            png::ColorType::GrayscaleAlpha => [p[0], p[0], p[0], p[1]],
            _ => panic!("未展开调色板"),
        })
        .collect()
}

fn ancillary(data: &[u8]) -> Vec<(bool, Vec<u8>)> {
    let mut output = Vec::new();
    let mut offset = 8;
    let mut after = false;
    while offset < data.len() {
        let length = u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        let name = &data[offset + 4..offset + 8];
        let end = offset + length + 12;
        if name == b"IDAT" {
            after = true;
        }
        if !matches!(name, b"IHDR" | b"PLTE" | b"tRNS" | b"IDAT" | b"IEND") {
            output.push((after, data[offset..end].to_vec()));
        }
        offset = end;
    }
    output
}

fn chunk(name: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut bytes = (payload.len() as u32).to_be_bytes().to_vec();
    bytes.extend_from_slice(name);
    bytes.extend_from_slice(payload);
    bytes.extend_from_slice(&crc32fast::hash(&bytes[4..]).to_be_bytes());
    bytes
}

#[test]
fn workspace_design_png_content_credentials_are_reported_without_touching_source() {
    let directory = tempfile::tempdir().unwrap();
    let original = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/UI界面设计/PixoFold-亮色.png"),
    )
    .unwrap();
    let source = directory.path().join("PixoFold-亮色.png");
    fs::write(&source, &original).unwrap();
    for mode in [
        PngMode::Lossless,
        PngMode::Lossy {
            quality: QualityValue::new(68).unwrap(),
        },
    ] {
        for output in [
            OutputPolicy::Overwrite,
            OutputPolicy::Copy {
                destination: directory.path().join("result.png"),
            },
        ] {
            let mut request = PngRequest::new(source.clone());
            request.mode = mode;
            request.output = output;
            let mut stages = Vec::new();
            let result = optimize_png(&request, &CancellationToken::default(), |stage| {
                stages.push(stage)
            });
            let error = result.unwrap_err();
            assert!(matches!(
                error,
                ProcessingError::ContentCredentialsRequireConsent(_)
            ));
            assert!(error.to_string().contains("C2PA"));
            assert!(!error.to_string().contains("验证失败"));
            assert!(!stages.contains(&ProcessingStage::Validating));
            assert!(!stages.contains(&ProcessingStage::BeforeCommit));
            source_only(&directory, &request, &original);
        }
    }
}

#[test]
fn real_quantization_is_smaller_than_lossless_and_reports_measured_quality() {
    for (name, q) in [
        ("gradient-rgb8.png", 40),
        ("gradient-gamma.png", 80),
        ("gradient-binary-alpha.png", 80),
    ] {
        let (directory, mut request, original) = workspace(name, q);
        let (report, output) = copy(&request, directory.path().join("quantized.png"));
        let PngProcessing::Lossy {
            mapping,
            measured_quality,
        } = report.processing
        else {
            panic!("{name}: {:?}", report.processing);
        };
        assert_eq!(mapping.target, q as u8);
        assert_eq!(mapping.version, 1);
        assert!(measured_quality >= mapping.target);
        assert_eq!(report.output_image.color_type, PngColorType::Indexed);
        assert_eq!(report.output_image.bit_depth, 8);
        assert_eq!(
            report.output_image,
            inspect_png(&output, ResourceLimits::default()).unwrap()
        );
        assert_eq!(report.output_bytes.0, output.len() as u64);
        assert_eq!(report.input_bytes.0, original.len() as u64);
        request.mode = PngMode::Lossless;
        let (lossless, _) = copy(&request, directory.path().join("lossless.png"));
        assert!(report.output_bytes < lossless.output_bytes);
        assert_ne!(rgba(&original), rgba(&output), "必须覆盖真实有损路径");
        assert_eq!(fs::read(&request.source).unwrap(), original);
        assert_eq!(files(directory.path()).len(), 3);
    }
}

#[test]
fn uniform_partial_alpha_can_quantize_without_becoming_opaque() {
    let (directory, request, original) = workspace("gradient-rgb8.png", 40);
    let pixels: Vec<_> = rgba(&original)
        .into_iter()
        .flat_map(|[r, g, b, _]| [r, g, b, 128])
        .collect();
    let mut source = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut source, 192, 128);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::NoCompression);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&pixels).unwrap();
        writer.finish().unwrap();
    }
    fs::write(&request.source, &source).unwrap();
    let (report, output) = copy(&request, directory.path().join("output.png"));
    assert!(
        matches!(report.processing, PngProcessing::Lossy { .. }),
        "{report:?}"
    );
    for pixel in rgba(&output) {
        assert!((1..255).contains(&pixel[3]));
        assert!(pixel[3].abs_diff(128) <= 8);
    }
    assert_eq!(fs::read(&request.source).unwrap(), source);
}

#[test]
fn quality_anchors_and_transparency_never_silently_degrade() {
    for name in [
        "gradient-rgb8.png",
        "gradient-rgba8.png",
        "gradient-binary-alpha.png",
    ] {
        for q in [0, 40, 80, 100] {
            let (directory, request, original) = workspace(name, q);
            let (report, output) = copy(&request, directory.path().join("output.png"));
            match report.processing {
                PngProcessing::Lossy {
                    measured_quality, ..
                } => assert!(i32::from(measured_quality) >= q),
                PngProcessing::LosslessFallback { .. } => {
                    assert_eq!(rgba(&original), rgba(&output))
                }
                PngProcessing::Lossless => panic!("不能将有损请求伪装成主动无损"),
            }
            for (before, after) in rgba(&original).iter().zip(rgba(&output)) {
                match before[3] {
                    0 | 255 => assert_eq!(before[3], after[3]),
                    alpha => {
                        assert!((1..255).contains(&after[3]));
                        assert!(alpha.abs_diff(after[3]) <= 8);
                    }
                }
            }
            if q == 100 {
                assert!(matches!(
                    report.processing,
                    PngProcessing::LosslessFallback {
                        reason: LossyFallbackReason::QualityBelowTarget { .. },
                        ..
                    }
                ));
            }
            if name == "gradient-rgba8.png" && q == 0 {
                assert!(matches!(
                    report.processing,
                    PngProcessing::LosslessFallback {
                        reason: LossyFallbackReason::TransparencyGuard,
                        ..
                    }
                ));
            }
        }
    }
}

#[test]
fn protected_bit_depth_and_metadata_fallback_matches_explicit_lossless_bytes() {
    for (name, reason) in [
        ("gray16.png", LossyFallbackReason::HighBitDepth),
        ("gray-alpha16.png", LossyFallbackReason::HighBitDepth),
        ("rgb16.png", LossyFallbackReason::HighBitDepth),
        ("rgba16.png", LossyFallbackReason::HighBitDepth),
        ("trns-gray16.png", LossyFallbackReason::HighBitDepth),
        ("icc-rgb8.png", LossyFallbackReason::ColorMetadata),
    ] {
        let (directory, mut request, _) = workspace(name, 80);
        let (report, output) = copy(&request, directory.path().join("fallback.png"));
        assert!(
            matches!(report.processing, PngProcessing::LosslessFallback { reason: actual, .. } if actual == reason)
        );
        request.mode = PngMode::Lossless;
        let (_, expected) = copy(&request, directory.path().join("lossless.png"));
        assert_eq!(output, expected);
        assert_eq!(report.output_image, report.image);
    }
    for (name, payload, reason) in [
        (
            *b"sBIT",
            vec![8, 8, 8],
            LossyFallbackReason::RepresentationMetadata,
        ),
        (
            *b"bKGD",
            vec![0; 6],
            LossyFallbackReason::RepresentationMetadata,
        ),
        (
            *b"gAMA",
            100_000_u32.to_be_bytes().to_vec(),
            LossyFallbackReason::ColorMetadata,
        ),
        (
            *b"cICP",
            vec![1, 13, 0, 1],
            LossyFallbackReason::ColorMetadata,
        ),
    ] {
        let (directory, mut request, original) = workspace("gradient-rgb8.png", 80);
        let source = [&original[..33], &chunk(&name, &payload), &original[33..]].concat();
        fs::write(&request.source, &source).unwrap();
        let (report, output) = copy(&request, directory.path().join("fallback.png"));
        assert!(
            matches!(report.processing, PngProcessing::LosslessFallback { reason: actual, .. } if actual == reason),
            "{report:?}"
        );
        request.mode = PngMode::Lossless;
        let (_, expected) = copy(&request, directory.path().join("lossless.png"));
        assert_eq!(output, expected);
    }
}

#[test]
fn palette_gray_trns_and_adam7_inputs_keep_transparency_when_expanded() {
    for name in [
        "indexed1.png",
        "indexed2.png",
        "indexed4.png",
        "indexed8.png",
        "gray1.png",
        "gray8.png",
        "gray-alpha8.png",
        "trns-rgb8.png",
        "adam7-rgba8.png",
    ] {
        let (directory, request, original) = workspace(name, 100);
        let (_, output) = copy(&request, directory.path().join("output.png"));
        // q=100 或回退：可见像素必须不变；全透明隐藏 RGB 不是有损契约的一部分。
        for (a, b) in rgba(&original).iter().zip(rgba(&output)) {
            assert_eq!(a[3], b[3], "{name}");
            if a[3] != 0 {
                assert_eq!(*a, b, "{name}");
            }
        }
    }
}

#[test]
fn display_gamma_and_safe_unknown_chunks_keep_bytes_and_idat_side() {
    let (directory, request, original) = workspace("gradient-display.png", 40);
    let extra = chunk(b"tEXt", b"Comment\0after IDAT");
    let private = chunk(b"vpAg", &[1, 2, 3]);
    let source = [
        &original[..33],
        &private,
        &original[33..original.len() - 12],
        &extra,
        &original[original.len() - 12..],
    ]
    .concat();
    fs::write(&request.source, &source).unwrap();
    let (report, output) = copy(&request, directory.path().join("output.png"));
    assert!(matches!(report.processing, PngProcessing::Lossy { .. }));
    assert_eq!(ancillary(&source), ancillary(&output));
    let (directory, request, original) = workspace("gradient-gamma.png", 80);
    let (_, output) = copy(&request, directory.path().join("output.png"));
    assert_eq!(ancillary(&original), ancillary(&output));
}

#[test]
fn larger_quantization_falls_back_and_no_gain_never_creates_output() {
    let (directory, request, _) = workspace("rgb8.png", 80);
    let (report, _) = copy(&request, directory.path().join("output.png"));
    assert!(
        matches!(
            report.processing,
            PngProcessing::LosslessFallback {
                reason: LossyFallbackReason::NoSizeBenefit,
                ..
            }
        ),
        "{report:?}"
    );
    let (directory, mut request, original) = workspace("already-optimized.png", 80);
    for output in [
        OutputPolicy::Overwrite,
        OutputPolicy::Copy {
            destination: directory.path().join("output.png"),
        },
    ] {
        request.output = output;
        let report = run(&request).unwrap();
        assert_eq!(report.outcome, ProcessingOutcome::NoGain);
        assert_eq!(report.output_bytes, report.input_bytes);
        assert_eq!(report.output_image, report.image);
        source_only(&directory, &request, &original);
    }
}

#[test]
fn lossy_overwrite_keeps_original_backup_and_cancellation_cleans_all_stages() {
    let (directory, request, original) = workspace("gradient-binary-alpha.png", 80);
    let report = run(&request).unwrap();
    assert!(matches!(report.processing, PngProcessing::Lossy { .. }));
    let ProcessingOutcome::Optimized {
        backup: Some(backup),
        ..
    } = report.outcome
    else {
        panic!("覆盖必须备份");
    };
    assert_eq!(fs::read(backup).unwrap(), original);
    assert_eq!(
        fs::metadata(&request.source).unwrap().len(),
        report.output_bytes.0
    );
    assert_eq!(files(directory.path()).len(), 2);
    for stop in [
        ProcessingStage::Reading,
        ProcessingStage::Optimizing,
        ProcessingStage::Validating,
        ProcessingStage::BeforeCommit,
    ] {
        let (directory, request, original) = workspace("gradient-binary-alpha.png", 80);
        let token = CancellationToken::default();
        let result = optimize_png(&request, &token, |stage| {
            if stage == stop {
                token.cancel();
            }
        });
        assert!(matches!(result, Err(ProcessingError::Cancelled)));
        source_only(&directory, &request, &original);
    }
}

#[test]
fn lossy_candidate_backup_and_source_changes_cannot_be_committed() {
    for target in 0..3 {
        let (directory, request, mut original) = workspace("gradient-binary-alpha.png", 80);
        let result = optimize_png(&request, &CancellationToken::default(), |stage| {
            if stage != ProcessingStage::BeforeCommit {
                return;
            }
            let path = if target == 0 {
                request.source.clone()
            } else {
                files(directory.path())
                    .into_iter()
                    .filter(|path| path != &request.source)
                    .find(|path| (fs::read(path).unwrap() == original) == (target == 1))
                    .unwrap()
            };
            fs::write(path, b"external change").unwrap();
        });
        if target == 0 {
            assert!(matches!(result, Err(ProcessingError::SourceChanged)));
            original = b"external change".to_vec();
        } else {
            assert!(matches!(result, Err(ProcessingError::ValidationFailed(_))));
        }
        source_only(&directory, &request, &original);
    }
    // 落盘后被替换为另一张有效 PNG 也不能以「可解码」为由放行。
    let (directory, request, original) = workspace("gradient-binary-alpha.png", 80);
    let result = optimize_png(&request, &CancellationToken::default(), |stage| {
        if stage == ProcessingStage::Validating {
            let temp = files(directory.path())
                .into_iter()
                .find(|path| path != &request.source)
                .unwrap();
            fs::write(temp, fixture("rgb8.png")).unwrap();
        }
    });
    assert!(matches!(result, Err(ProcessingError::ValidationFailed(_))));
    source_only(&directory, &request, &original);
}

#[test]
fn lossy_late_copy_conflict_and_expanded_memory_limit_preserve_source() {
    let (directory, mut request, original) = workspace("gradient-binary-alpha.png", 80);
    let destination = directory.path().join("output.png");
    request.output = OutputPolicy::Copy {
        destination: destination.clone(),
    };
    let result = optimize_png(&request, &CancellationToken::default(), |stage| {
        if stage == ProcessingStage::BeforeCommit {
            fs::write(&destination, b"external").unwrap();
        }
    });
    assert!(matches!(result, Err(ProcessingError::TargetConflict)));
    assert_eq!(fs::read(destination).unwrap(), b"external");
    assert_eq!(fs::read(&request.source).unwrap(), original);
    assert_eq!(files(directory.path()).len(), 2);
    let (directory, mut request, original) = workspace("indexed1.png", 80);
    request.limits.max_decoded_bytes = ByteCount(4096);
    let result = run(&request);
    assert!(
        matches!(
            result,
            Err(ProcessingError::ResourceLimit("量化 RGBA 缓冲区"))
        ),
        "{result:?}"
    );
    source_only(&directory, &request, &original);
}
