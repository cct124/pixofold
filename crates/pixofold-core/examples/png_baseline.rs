//! 自生成语料基线；只在隔离目录写副本，耗时为当前机器的实际测量。

use std::{error::Error, fs, path::Path};

use pixofold_core::{
    model::{CancellationToken, OutputPolicy, PngRequest},
    pipeline::optimize_png,
};

fn main() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/png");
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("manifest.json"))?)?;
    let temp = tempfile::tempdir()?;
    println!("fixture,input_bytes,output_bytes,elapsed_ms,outcome");
    for entry in manifest["fixtures"]
        .as_array()
        .ok_or("清单 fixtures 缺失")?
    {
        if !matches!(entry["expected"].as_str(), Some("static" | "no-gain")) {
            continue;
        }
        let name = entry["file"].as_str().ok_or("清单 file 缺失")?;
        let mut request = PngRequest::new(root.join(name));
        request.output = OutputPolicy::Copy {
            destination: temp.path().join(name),
        };
        let report = optimize_png(&request, &CancellationToken::default(), |_| {})?;
        let outcome = match report.outcome {
            pixofold_core::model::ProcessingOutcome::Optimized { .. } => "optimized",
            pixofold_core::model::ProcessingOutcome::NoGain => "no-gain",
        };
        println!(
            "{name},{},{},{:.3},{outcome}",
            report.input_bytes.0,
            report.output_bytes.0,
            report.elapsed.as_secs_f64() * 1000.0
        );
    }
    temp.close()?;
    Ok(())
}
