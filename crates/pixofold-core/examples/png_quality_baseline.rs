//! 自生成渐变质量基线；黑白背景误差是独立的编码值 RMSE，不是 SSIM 或人工验收。

use std::{error::Error, fs, io::Cursor, path::Path};

use pixofold_core::{
    model::{
        CancellationToken, OutputPolicy, PngMode, PngProcessing, PngRequest, ProcessingOutcome,
        QualityValue,
    },
    pipeline::optimize_png,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let persistent = match args.next() {
        None => None,
        Some(flag) if flag == "--output-dir" => Some(args.next().ok_or("缺少新输出目录")?),
        _ => return Err("用法：png_quality_baseline [--output-dir <不存在的目录>]".into()),
    };
    if args.next().is_some() {
        return Err("不接受额外参数".into());
    }
    let temp = tempfile::tempdir()?;
    let destination = persistent.as_deref().map(Path::new).unwrap_or(temp.path());
    if persistent.is_some() {
        // 只允许创建新目录，绝不覆盖已有用户内容；部分失败的基线产物保留供诊断。
        fs::create_dir(destination)?;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/png");
    println!(
        "fixture,q,path,score,input_bytes,output_bytes,elapsed_ms,black_rmse,white_rmse,max_alpha_error"
    );
    for name in [
        "gradient-rgb8.png",
        "gradient-rgba8.png",
        "gradient-binary-alpha.png",
        "gradient-display.png",
        "gradient-gamma.png",
    ] {
        let source = fs::read(root.join(name))?;
        let original = rgba(&source)?;
        for q in [None, Some(0), Some(40), Some(80), Some(100)] {
            let label = q.map_or_else(|| "lossless".to_owned(), |q| q.to_string());
            let mut request = PngRequest::new(root.join(name));
            if let Some(q) = q {
                request.mode = PngMode::Lossy {
                    quality: QualityValue::new(q)?,
                };
            }
            request.output = OutputPolicy::Copy {
                destination: destination.join(format!("{label}-{name}")),
            };
            let report = optimize_png(&request, &CancellationToken::default(), |_| {})?;
            let output = match &report.outcome {
                ProcessingOutcome::Optimized { output, .. } => fs::read(output)?,
                ProcessingOutcome::NoGain => source.clone(),
            };
            let (path, score) = match report.processing {
                PngProcessing::Lossless => ("lossless".to_owned(), String::new()),
                PngProcessing::Lossy {
                    measured_quality, ..
                } => ("lossy".to_owned(), measured_quality.to_string()),
                PngProcessing::LosslessFallback { reason, .. } => {
                    (format!("fallback:{reason:?}"), String::new())
                }
            };
            let (black, white, alpha) = error_metrics(&original, &rgba(&output)?)?;
            println!(
                "{name},{label},{path},{score},{},{},{:.3},{black:.3},{white:.3},{alpha}",
                report.input_bytes.0,
                report.output_bytes.0,
                report.elapsed.as_secs_f64() * 1000.0
            );
            if fs::read(&request.source)? != source {
                return Err("基线源图意外改变".into());
            }
        }
    }
    if persistent.is_some() {
        eprintln!("基线产物：{}", destination.canonicalize()?.display());
    }
    temp.close()?;
    Ok(())
}

fn rgba(bytes: &[u8]) -> Result<Vec<[u8; 4]>, Box<dyn Error>> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info()?;
    let mut buffer = vec![0; reader.output_buffer_size().ok_or("解码尺寸溢出")?];
    let info = reader.next_frame(&mut buffer)?;
    reader.finish()?;
    if info.bit_depth != png::BitDepth::Eight {
        return Err("本基线仅接受展开后的 8-bit 样本".into());
    }
    let channels = info.color_type.samples();
    buffer[..info.buffer_size()]
        .chunks_exact(channels)
        .map(|p| {
            Ok(match info.color_type {
                png::ColorType::Rgb => [p[0], p[1], p[2], 255],
                png::ColorType::Rgba => [p[0], p[1], p[2], p[3]],
                png::ColorType::Grayscale => [p[0], p[0], p[0], 255],
                png::ColorType::GrayscaleAlpha => [p[0], p[0], p[0], p[1]],
                _ => return Err("索引色没有展开".into()),
            })
        })
        .collect()
}

fn error_metrics(before: &[[u8; 4]], after: &[[u8; 4]]) -> Result<(f64, f64, u8), Box<dyn Error>> {
    if before.len() != after.len() || before.is_empty() {
        return Err("基线像素数量不一致或为空".into());
    }
    let mut sums = [0.0_f64; 2];
    let mut max_alpha = 0;
    for (a, b) in before.iter().zip(after) {
        max_alpha = max_alpha.max(a[3].abs_diff(b[3]));
        for (sum, background) in sums.iter_mut().zip([0.0, 255.0]) {
            for channel in 0..3 {
                let composite = |p: &[u8; 4]| {
                    (f64::from(p[channel]) * f64::from(p[3])
                        + background * (255.0 - f64::from(p[3])))
                        / 255.0
                };
                *sum += (composite(a) - composite(b)).powi(2);
            }
        }
    }
    let samples = (before.len() * 3) as f64;
    Ok((
        (sums[0] / samples).sqrt(),
        (sums[1] / samples).sqrt(),
        max_alpha,
    ))
}
