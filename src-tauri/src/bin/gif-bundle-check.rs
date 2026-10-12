//! 无GUI的真实部署验收，复用生产身份/核心验证；不接受外部期望哈希。

#[path = "../gif_bundle.rs"]
mod gif_bundle;

use pixofold_core::{
    gif::{GifError, GifLimits, GifRequest, optimize_gif, validate_gif},
    model::{CancellationToken, OutputPolicy, ProcessingOutcome},
};
use std::{error::Error, fs, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 2 && args[0] == "--capabilities" {
        println!(
            "{}",
            serde_json::to_string(&pixofold_desktop_lib::bundle_check::capabilities(
                Path::new(&args[1])
            ))?
        );
        return Ok(());
    }
    if args.len() != 1 && args.len() != 3 {
        return Err("参数：资源根目录 [样本GIF 新输出目录]".into());
    }
    let engine = gif_bundle::load(Path::new(&args[0]))?;
    if args.len() == 1 {
        println!("verified");
        return Ok(());
    }
    let source = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    fs::create_dir(output)?;
    let original = fs::read(source)?;
    let cancel = CancellationToken::default();
    let before = validate_gif(&original, GifLimits::default(), &cancel)?;
    let destination = output.join("lossless.gif");
    let mut request = GifRequest::new(source);
    request.output = OutputPolicy::Copy {
        destination: destination.clone(),
    };
    let report = optimize_gif(&request, &engine, &cancel, |_| {})?;
    if !matches!(
        report.outcome,
        ProcessingOutcome::Optimized { backup: None, .. }
    ) {
        return Err("部署样本未产生真实压缩结果".into());
    }
    let optimized = fs::read(&destination)?;
    let after = validate_gif(&optimized, GifLimits::default(), &cancel)?;
    if !before.equivalent(&after) {
        return Err("部署输出的独立播放验证失败".into());
    }
    let mut repeat = GifRequest::new(&destination);
    repeat.output = OutputPolicy::Copy {
        destination: output.join("no-gain.gif"),
    };
    let repeated = optimize_gif(&repeat, &engine, &cancel, |_| {})?;
    if repeated.outcome != ProcessingOutcome::NoGain || output.join("no-gain.gif").exists() {
        return Err("重复无损未保持NoGain契约".into());
    }
    cancel.cancel();
    if !matches!(
        optimize_gif(&repeat, &engine, &cancel, |_| {}),
        Err(GifError::Cancelled)
    ) {
        return Err("预取消未被接纳".into());
    }
    if fs::read(source)? != original
        || fs::read(&destination)? != optimized
        || fs::read_dir(output)?.count() != 1
    {
        return Err("源文件变化或存在意外临时产物".into());
    }
    let desktop = pixofold_desktop_lib::bundle_check::verify_bundled(
        Path::new(&args[0]),
        &source.parent().ok_or("缺少样本目录")?.join("samples"),
        &output.join("desktop"),
    )?;
    println!(
        "{}",
        serde_json::json!({"result":"passed", "inputBytes":report.input_bytes.0, "outputBytes":report.output_bytes.0, "independentPlayback":true, "noGain":true, "cancelled":true, "desktop":desktop})
    );
    Ok(())
}
