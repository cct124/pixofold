//! 显式部署验收：复用生产加载器及内嵌哈希，只使用调用方新建的隔离语料目录。
//! 不启动Tauri/GUI，不随安装包分发，不新增IPC或接受外部期望哈希。

#[path = "../jpeg_bundle.rs"]
mod jpeg_bundle;

use pixofold_core::{
    jpeg::{JpegError, JpegMode, JpegProcessing, JpegRequest, optimize_jpeg},
    model::{CancellationToken, OutputPolicy, ProcessingOutcome, QualityValue},
};
use std::{error::Error, fs, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 1 && args.len() != 3 {
        return Err("参数：资源根目录 [样本JPEG 新输出目录]".into());
    }
    let engine = jpeg_bundle::load(Path::new(&args[0]))?;
    if args.len() == 1 {
        println!("verified");
        return Ok(());
    }
    let source = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    fs::create_dir(output)?;
    let original = fs::read(source)?;
    let mut checks = Vec::new();
    for (name, mode) in [
        ("lossless.jpg", JpegMode::Lossless),
        (
            "lossy.jpg",
            JpegMode::Lossy {
                quality: QualityValue::new(40)?,
            },
        ),
    ] {
        let mut request = JpegRequest::new(source);
        request.mode = mode;
        request.output = OutputPolicy::Copy {
            destination: output.join(name),
        };
        let report = optimize_jpeg(&request, &engine, &CancellationToken::default(), |_| {})?;
        if !matches!(
            report.outcome,
            ProcessingOutcome::Optimized { backup: None, .. }
        ) {
            return Err("部署样本应产生真实压缩结果".into());
        }
        if name == "lossy.jpg" && !matches!(report.processing, JpegProcessing::Lossy { .. }) {
            return Err("普通样本未走真实有损编码".into());
        }
        checks.push(serde_json::json!({"file":name,"inputBytes":report.input_bytes.0,"outputBytes":report.output_bytes.0}));
    }
    let optimized = output.join("lossless.jpg");
    let optimized_bytes = fs::read(&optimized)?;
    let mut repeat = JpegRequest::new(&optimized);
    repeat.output = OutputPolicy::Copy {
        destination: output.join("no-gain.jpg"),
    };
    let report = optimize_jpeg(&repeat, &engine, &CancellationToken::default(), |_| {})?;
    if report.outcome != ProcessingOutcome::NoGain || output.join("no-gain.jpg").exists() {
        return Err("重复无损未保持NoGain契约".into());
    }
    let cancel = CancellationToken::default();
    cancel.cancel();
    if !matches!(
        optimize_jpeg(&repeat, &engine, &cancel, |_| {}),
        Err(JpegError::Cancelled)
    ) {
        return Err("预取消未被接纳".into());
    }
    if fs::read(source)? != original || fs::read(optimized)? != optimized_bytes {
        return Err("验收源文件被修改".into());
    }
    if fs::read_dir(output)?.count() != 2 {
        return Err("存在意外输出或临时残留".into());
    }
    println!(
        "{}",
        serde_json::json!({"result":"passed","outputs":checks,"noGain":true,"cancelled":true})
    );
    Ok(())
}
