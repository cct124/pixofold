//! 开发实验CLI。输入只读，输出独占创建；取消/超时完成同步收尾后才返回。
#[path = "apng_lab/derive.rs"]
mod derive;
#[path = "apng_lab/memory.rs"]
mod memory;
use pixofold_core::{
    apng::lossy_lab::{self, Options, Stage},
    model::CancellationToken,
};
use serde_json::json;
use sha2::{Digest, Sha256};
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
        return Err(std::io::Error::other("input budget"));
    }
    Ok(bytes)
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() == 3 && args[0] == "derive" {
        return derive::run(Path::new(&args[1]), Path::new(&args[2]));
    }
    if args.len() != 4 || !matches!(args[0].as_str(), "quantize" | "cancel" | "deadline") {
        return Err(
            "usage: quantize/cancel/deadline INPUT NEW_OUTPUT OPTIONS_JSON | derive PNG NEW_APNG"
                .into(),
        );
    }
    let sampler = memory::Sampler::start();
    let mut options: Options = serde_json::from_str(&args[3])?;
    let source = bounded(
        Path::new(&args[1]),
        Options::default().limits.max_input_bytes,
    )?;
    let before = hash(&source);
    let output = Path::new(&args[2]);
    if output.exists() {
        return Err("experiment never replaces output".into());
    }
    if args[0] == "deadline" {
        options.limits.deadline_ms = 1;
    }
    let token = CancellationToken::default();
    let started = Instant::now();
    let (finished, wait) = mpsc::channel();
    let mut wait = Some(wait);
    let mut timer = None;
    let mut stages = Vec::new();
    let result = lossy_lab::quantize(&source, options, &token, |stage| {
        stages.push(format!("{stage:?}"));
        if args[0] == "cancel"
            && stage == Stage::Quantizing
            && let Some(wait) = wait.take()
        {
            let signal = token.clone();
            timer = Some(thread::spawn(move || {
                if wait.recv_timeout(Duration::from_millis(1)).is_err() {
                    let at = Instant::now();
                    signal.cancel();
                    Some(at)
                } else {
                    None
                }
            }));
        }
    });
    let returned = Instant::now();
    let _ = finished.send(());
    let cancelled_at = timer
        .map(|t| t.join().map_err(|_| "timer failed"))
        .transpose()?
        .flatten();
    let mut evidence = match result {
        Ok(candidate) => {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?;
            file.write_all(&candidate.bytes)?;
            file.sync_all()?;
            // 实验保留质量/收益被淘汰候选供审阅，所有结果均不得覆盖输入。
            json!({"result":"candidate","candidateBytes":candidate.bytes.len(),"candidateSha256":hash(&candidate.bytes),"report":candidate.report})
        }
        Err(error) => json!({"result":"rejected","reason":error.0}),
    };
    if hash(&bounded(
        Path::new(&args[1]),
        Options::default().limits.max_input_bytes,
    )?) != before
    {
        return Err("source changed".into());
    }
    evidence["inputBytes"] = json!(source.len());
    evidence["sourceSha256"] = json!(before);
    evidence["sourceUnchanged"] = json!(true);
    evidence["candidateWritten"] = json!(output.exists());
    evidence["elapsedMs"] = json!(started.elapsed().as_millis());
    evidence["stages"] = json!(stages);
    evidence["options"] = json!(options);
    evidence["memory"] = sampler.finish();
    evidence["cancellationRequested"] = json!(cancelled_at.is_some());
    evidence["returnAfterCancellationMs"] =
        json!(cancelled_at.map(|at| returned.saturating_duration_since(at).as_millis()));
    println!("{evidence}");
    Ok(())
}
