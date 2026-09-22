//! 手动验收入口；强制显式选择副本或覆盖，避免开发实验误触原图。

use std::{env, error::Error, path::PathBuf, process::ExitCode};

use pixofold_core::{
    model::{CancellationToken, OutputPolicy, PngRequest, ProcessingError},
    pipeline::optimize_png,
};

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let output = match args.as_slice() {
        [_, mode] if mode == "--overwrite" => OutputPolicy::Overwrite,
        [_, mode, destination] if mode == "--copy" => OutputPolicy::Copy {
            destination: PathBuf::from(destination),
        },
        _ => {
            return Err(
                "用法：optimize_png <源文件> --copy <新文件> | <源文件> --overwrite".into(),
            );
        }
    };
    let mut request = PngRequest::new(&args[0]);
    request.output = output;
    let report = optimize_png(&request, &CancellationToken::default(), |stage| {
        eprintln!("{stage:?}")
    })?;
    println!(
        "{} -> {} bytes, {:.3} ms\n{:?}",
        report.input_bytes.0,
        report.output_bytes.0,
        report.elapsed.as_secs_f64() * 1000.0,
        report.outcome
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
