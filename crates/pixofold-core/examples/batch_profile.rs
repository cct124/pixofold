//! 可重复批量性能入口：只处理临时副本，原始素材绝不改写；不启动GUI。
//! cargo run -p pixofold-core --release --example batch_profile -- SOURCE WORKERS COPIES [--confirm-credentials]
//! 固定有损68/同目录测试副本；每20ms采样本进程RSS和权威活动数，采样峰值不是硬上限。

use pixofold_core::{
    batch::{
        BatchConfig, BatchError, BatchItem, BatchParameters, BatchService, BatchSnapshot, RetryJob,
        RetryRequest,
    },
    model::{OutputPolicy, PngMetadataPolicy, PngMode, QualityValue},
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Default)]
struct Samples {
    active: usize,
    reserved_bytes: u64,
    rss_bytes: u64,
    system: System,
}
impl Samples {
    fn wait(
        &mut self,
        service: &BatchService,
    ) -> Result<BatchSnapshot, Box<dyn std::error::Error>> {
        let id = service.snapshot().ok_or("缺少批次")?.id;
        let pid = sysinfo::get_current_pid()?;
        loop {
            let snapshot = service.snapshot().ok_or("缺少批次")?;
            self.active = self.active.max(snapshot.active_workers);
            self.reserved_bytes = self.reserved_bytes.max(snapshot.reserved_working_bytes.0);
            self.system.refresh_processes_specifics(
                ProcessesToUpdate::Some(&[pid]),
                false,
                ProcessRefreshKind::nothing().with_memory(),
            );
            if let Some(process) = self.system.process(pid) {
                self.rss_bytes = self.rss_bytes.max(process.memory());
            }
            match service.wait(id, Duration::from_millis(20)) {
                Ok(done) => return Ok(done),
                Err(BatchError::TimedOut) => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !(3..=4).contains(&args.len()) {
        return Err("用法: batch_profile SOURCE WORKERS COPIES [--confirm-credentials]".into());
    }
    let source = PathBuf::from(&args[0]);
    let workers: usize = args[1].to_str().ok_or("worker无效")?.parse()?;
    let copies: usize = args[2].to_str().ok_or("copies无效")?.parse()?;
    if !(1..=32).contains(&copies) {
        return Err("copies须为1–32".into());
    }
    let confirm = args
        .get(3)
        .is_some_and(|arg| arg == "--confirm-credentials");
    if args.len() == 4 && !confirm {
        return Err("未知参数".into());
    }
    let original = fs::read(&source)?;
    let dir = tempfile::tempdir()?;
    let mut items = Vec::new();
    for i in 0..copies {
        let input = dir.path().join(format!("input-{i}.png"));
        fs::write(&input, &original)?;
        items.push(BatchItem {
            format: pixofold_core::batch::ImageKind::Png,
            source: input,
            output: OutputPolicy::Copy {
                destination: dir.path().join(format!("output-{i}.png")),
            },
        });
    }
    let parameters = BatchParameters {
        mode: PngMode::Lossy {
            quality: QualityValue::new(68)?,
        },
        ..BatchParameters::default()
    };
    let mut service = BatchService::new(BatchConfig {
        workers,
        ..BatchConfig::default()
    })?;
    let started = Instant::now();
    service.start(pixofold_core::batch::BatchRequest {
        items,
        parameters,
        engines: Default::default(),
    })?;
    let mut samples = Samples::default();
    let mut done = samples.wait(&service)?;
    let initial_ms = started.elapsed().as_millis();
    let mut retry_ms = None;
    if confirm {
        let jobs: Vec<_> = done
            .jobs
            .iter()
            .filter_map(|job| {
                job.content_credentials_source().map(|source| RetryJob {
                    id: job.id,
                    output: job.request.output.clone(),
                    metadata: PngMetadataPolicy::RemoveContentCredentials(source.clone()),
                })
            })
            .collect();
        if jobs.len() != copies {
            return Err("确认样本应全部产生内容凭据待确认结果".into());
        }
        let retry = Instant::now();
        service.retry(done.id, RetryRequest { jobs, parameters })?;
        done = samples.wait(&service)?;
        retry_ms = Some(retry.elapsed().as_millis());
    }
    let elapsed_ms = started.elapsed().as_millis();
    let mut hashes = Vec::new();
    for i in 0..copies {
        if fs::read(dir.path().join(format!("input-{i}.png")))? != original {
            return Err("测试源文件被修改".into());
        }
        let output = dir.path().join(format!("output-{i}.png"));
        if output.exists() {
            hashes.push(format!("{:x}", Sha256::digest(fs::read(output)?)));
        }
    }
    println!(
        "{}",
        serde_json::json!({
            "workers": workers, "copies": copies, "initial_ms": initial_ms, "retry_ms": retry_ms,
            "elapsed_ms": elapsed_ms, "sampled_peak_active": samples.active,
            "sampled_peak_reserved_bytes": samples.reserved_bytes, "sampled_peak_rss_bytes": samples.rss_bytes,
            "succeeded": done.summary.succeeded, "no_gain": done.summary.no_gain,
            "failed": done.summary.failed, "sources_unchanged": true, "output_sha256": hashes,
        })
    );
    service.shutdown()?;
    if done.summary.failed != 0 {
        return Err("性能样本存在未恢复失败，不作为吞吐成功证据".into());
    }
    Ok(())
}
