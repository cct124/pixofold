//! 手动验收入口；强制显式选择副本或覆盖，避免开发实验误触原图。

use std::{env, error::Error, path::PathBuf, process::ExitCode};

use pixofold_core::{
    model::{CancellationToken, OutputPolicy, PngMode, PngRequest, ProcessingError, QualityValue},
    pipeline::optimize_png,
};

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let source = args.next().ok_or("缺少源文件；用法：optimize_png <源文件> --copy <新文件> | --overwrite [--lossy 0..100 | --lossless]")?;
    let mut output = None;
    let mut mode = None;
    while let Some(arg) = args.next() {
        if arg == "--copy" && output.is_none() {
            output = Some(OutputPolicy::Copy {
                destination: PathBuf::from(args.next().ok_or("--copy 缺少目标文件")?),
            });
        } else if arg == "--overwrite" && output.is_none() {
            output = Some(OutputPolicy::Overwrite);
        } else if arg == "--lossless" && mode.is_none() {
            mode = Some(PngMode::Lossless);
        } else if arg == "--lossy" && mode.is_none() {
            let value = args.next().ok_or("--lossy 缺少质量值")?;
            let value = value
                .to_str()
                .ok_or("质量必须为 0–100 整数")?
                .parse::<i32>()?;
            mode = Some(PngMode::Lossy {
                quality: QualityValue::new(value)?,
            });
        } else {
            return Err("未知、重复或冲突的选项".into());
        }
    }
    let mut request = PngRequest::new(source);
    request.output = output.ok_or("必须显式选择 --copy 或 --overwrite")?;
    request.mode = mode.unwrap_or_default();
    let report = optimize_png(&request, &CancellationToken::default(), |stage| {
        eprintln!("{stage:?}")
    })?;
    println!(
        "{} -> {} bytes, {:.3} ms\n{:?}\n{:?}",
        report.input_bytes.0,
        report.output_bytes.0,
        report.elapsed.as_secs_f64() * 1000.0,
        report.outcome,
        report.processing,
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            if let Some(source) = error.source() {
                eprintln!("原因：{source}");
            }
            if let Some(error) = error.downcast_ref::<ProcessingError>() {
                print_recovery_paths(error);
            }
            ExitCode::FAILURE
        }
    }
}

fn print_recovery_paths(error: &ProcessingError) {
    match error {
        ProcessingError::CommitFailed { backup, .. } => {
            eprintln!("原始恢复备份：{}", backup.display());
        }
        ProcessingError::CleanupFailed {
            original,
            temporary,
            ..
        } => {
            eprintln!("残留临时文件：{}", temporary.display());
            if let Some(original) = original {
                print_recovery_paths(original);
            }
        }
        _ => {}
    }
}
