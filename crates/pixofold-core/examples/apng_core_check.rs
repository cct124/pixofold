//! 真实单文件核心验收；仅操作脚本复制到target的测试输入和独立输出目录。
#[path = "apng_lab/memory.rs"]
mod memory;
use pixofold_core::{
    apng::*,
    model::{CancellationToken, OutputPolicy, ProcessingOutcome},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Instant};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 1 {
        return Err("usage: apng_core_check ISOLATED_ROOT".into());
    }
    let root = Path::new(&args[0]);
    let manifest: Value = serde_json::from_slice(&fs::read(root.join("cases.json"))?)?;
    let sampler = memory::Sampler::start();
    let mut rows = Vec::new();
    for sample in manifest["samples"].as_array().ok_or("samples missing")? {
        let name = sample["file"].as_str().ok_or("name missing")?;
        let source = root.join("inputs").join(name);
        let original = fs::read(&source)?;
        assert_eq!(hash(&original), sample["sha256"]);
        let destination = root.join("outputs").join(name);
        fs::create_dir_all(destination.parent().ok_or("parent missing")?)?;
        let mut request = ApngRequest::new(&source);
        request.output = OutputPolicy::Copy {
            destination: destination.clone(),
        };
        let begin = Instant::now();
        let report = optimize_apng(&request, &CancellationToken::default(), |_| {});
        assert_eq!(
            hash(&fs::read(&source)?),
            sample["sha256"],
            "source changed"
        );
        let row = match report {
            Err(ApngError::Validation(error)) if sample["reject"] == error.0 => {
                assert!(!destination.exists());
                json!({"file":name,"result":"rejected","reason":error.0,"sourceUnchanged":true})
            }
            Err(error) => return Err(error.into()),
            Ok(report) => {
                assert!(sample.get("reject").is_none(), "unexpected accept");
                let outcome = match &report.outcome {
                    ProcessingOutcome::NoGain => {
                        assert!(!destination.exists());
                        assert_eq!(report.input_bytes, report.output_bytes);
                        "no_gain"
                    }
                    ProcessingOutcome::Optimized { output, backup } => {
                        assert!(backup.is_none());
                        let stored = fs::read(output)?;
                        compare_apng(
                            &original,
                            &stored,
                            &request.limits.validation,
                            &CancellationToken::default(),
                        )
                        .map_err(|e| format!("validation {}", e.0))?;
                        assert!(stored.len() < original.len());
                        "gain"
                    }
                };
                json!({"file":name,"result":outcome,"inputBytes":report.input_bytes.0,"outputBytes":report.output_bytes.0,"elapsedMs":begin.elapsed().as_millis(),"image":report.image,"sourceUnchanged":true,"workingSetBytes":request.limits.working_set(u64::from(report.image.width)*u64::from(report.image.height))?.0})
            }
        };
        rows.push(row);
    }
    // 压力语料通过真实覆盖路径验证备份、原始扩展名和重复无收益。
    let overwrite = root.join("overwrite.APNG");
    let original = fs::read(root.join("inputs/stress.apng"))?;
    fs::write(&overwrite, &original)?;
    let request = ApngRequest::new(&overwrite);
    let report = optimize_apng(&request, &CancellationToken::default(), |_| {})?;
    let ProcessingOutcome::Optimized {
        backup: Some(backup),
        ..
    } = report.outcome
    else {
        return Err("stress must optimize with backup".into());
    };
    assert_eq!(fs::read(&backup)?, original);
    assert_eq!(backup.extension().and_then(|s| s.to_str()), Some("APNG"));
    let count = fs::read_dir(root)?.count();
    let repeated = optimize_apng(&request, &CancellationToken::default(), |_| {})?;
    assert_eq!(repeated.outcome, ProcessingOutcome::NoGain);
    assert_eq!(fs::read_dir(root)?.count(), count);
    let result = json!({"result":"passed","cases":rows,"memory":sampler.finish(),"overwriteBackupAndNoGain":true,"validation":"same core validator with independently generated oracle tested by apng_lab/apng_core"});
    fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!(
        "APNG真实核心验收通过：{}项，覆盖/备份/无收益与源hash保持",
        rows.len()
    );
    Ok(())
}
