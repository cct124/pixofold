//! 显式APNG实验CLI，只写调用方新建的隔离目录；不存在生产输出/IPC入口。
mod apng_lab;
#[path = "apng_lab/memory.rs"]
mod memory;
use apng_lab::{Limits, compare, hash, inspect};
use pixofold_core::model::CancellationToken;
use serde_json::json;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

fn bounded(path: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "input budget",
        ));
    }
    Ok(bytes)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let limits = Limits::default();
    let cancel = CancellationToken::default();
    let valid = match args.first().map(String::as_str) {
        Some("inspect") => args.len() == 2,
        Some("optimize" | "cancel" | "deadline") => args.len() == 4,
        _ => false,
    };
    if !valid {
        return Err(
            "usage: inspect INPUT | optimize/cancel/deadline INPUT NEW_OUTPUT PRESET(0..2)".into(),
        );
    }
    let sampler = memory::Sampler::start();
    let source = bounded(Path::new(&args[1]), limits.max_input_bytes)?;
    let before = hash(&source);
    let started = Instant::now();
    let info = match inspect(&source, &limits, &cancel) {
        Ok(info) => info,
        Err(error) => {
            println!(
                "{}",
                json!({"result":"rejected","reason":error.0,"sourceSha256":before,"inputBytes":source.len(),"sourceUnchanged":true,"elapsedMs":started.elapsed().as_millis(),"memory":sampler.finish()})
            );
            return Ok(());
        }
    };
    if args[0] == "inspect" {
        println!("{}", json!({"result":"accepted","summary":info}));
        return Ok(());
    }
    let output = Path::new(&args[2]);
    if output.exists() {
        return Err("experiment never replaces output".into());
    }
    let preset: u8 = args[3].parse()?;
    if preset > 2 {
        return Err("preset must be 0..2".into());
    }
    let soft_ms = if args[0] == "deadline" { 1 } else { 10_000 };
    let options = oxipng::Options {
        fix_errors: false,
        force: true,
        interlace: None,
        optimize_alpha: false,
        bit_depth_reduction: false,
        color_type_reduction: false,
        palette_reduction: false,
        grayscale_reduction: false,
        scale_16: false,
        strip: oxipng::StripChunks::None,
        idat_recoding: true,
        timeout: Some(Duration::from_millis(soft_ms)),
        max_decompressed_size: Some(limits.max_decoded_bytes as usize),
        ..oxipng::Options::from_preset(preset)
    };
    let encode_started = Instant::now();
    // 取消只在编码开始后请求；同步编码不支持抢占，join/返回前继续持有全部资源。
    let (finished, wait) = mpsc::channel();
    let signal = cancel.clone();
    let cancel_mode = args[0] == "cancel";
    let timer = thread::spawn(move || {
        if cancel_mode && wait.recv_timeout(Duration::from_millis(1)).is_err() {
            signal.cancel();
            Some(Instant::now())
        } else {
            None
        }
    });
    let candidate = oxipng::optimize_from_memory(&source, &options);
    let returned = Instant::now();
    let encode_ms = encode_started.elapsed().as_millis();
    let _ = finished.send(());
    let cancelled_at = timer.join().map_err(|_| "cancellation timer failed")?;
    let return_after_cancel =
        cancelled_at.map(|at| returned.saturating_duration_since(at).as_millis());
    let mut result = "encoder_rejected";
    let mut reason = None;
    let mut candidate_hash = None;
    let mut candidate_bytes = None;
    if let Ok(candidate) = candidate {
        candidate_bytes = Some(candidate.len());
        candidate_hash = Some(hash(&candidate));
        if cancel.is_cancelled() {
            result = "cancelled";
        } else if args[0] == "deadline" && encode_ms >= u128::from(soft_ms) {
            result = "deadline";
        } else {
            match compare(&source, &candidate, &limits, &cancel) {
                Ok(()) => {
                    result = if candidate.len() < source.len() {
                        "gain"
                    } else {
                        "no_gain"
                    };
                }
                Err(error) => {
                    result = "validation_rejected";
                    reason = Some(error.0);
                }
            }
            // 保留被拒候选用于审阅；不将其复制回任何原件。
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?;
            file.write_all(&candidate)?;
        }
    }
    if hash(&bounded(Path::new(&args[1]), limits.max_input_bytes)?) != before {
        return Err("source changed".into());
    }
    let memory = sampler.finish();
    println!(
        "{}",
        json!({"result":result,"reason":reason,"inputBytes":source.len(),"candidateBytes":candidate_bytes,
        "sourceSha256":before,"sourceUnchanged":true,"candidateSha256":candidate_hash,"preset":preset,
        "encodeMs":encode_ms,"elapsedMs":started.elapsed().as_millis(),"memory":memory,"summary":info,
        "softDeadlineMs":soft_ms,"cancellationRequested":cancelled_at.is_some(),
        "returnAfterCancellationMs":return_after_cancel,
        "candidateWritten":output.exists(),"limits":limits})
    );
    Ok(())
}
