//! 固定可信helper的有损实验CLI；所有输出独占创建，输入在处理前后核对SHA256。
#[path = "gif_lab/memory.rs"]
mod memory;
#[path = "gif_lab/quality_fixtures.rs"]
mod quality_fixtures;
use pixofold_core::{
    gif::{GifEngine, GifLimits, lab},
    model::CancellationToken,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
    time::Instant,
};

fn read(path: &Path) -> Result<Vec<u8>, Box<dyn Error>> {
    let maximum = GifLimits::default().resources.max_input_bytes.0;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err("实验输入字节超限".into());
    }
    Ok(bytes)
}
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 3 && args[0] == "fixtures" {
        quality_fixtures::generate(Path::new(&args[1]), &read(Path::new(&args[2]))?)?;
        println!("{}", json!({"result":"generated","samples":5}));
        return Ok(());
    }
    if args.len() != 5 {
        return Err("参数：HELPER_DIR SHA256 INPUT OUTPUT lossless|0..100".into());
    }
    let sha = args[1].to_str().ok_or("工具哈希无效")?;
    let hash: [u8; 32] = (0..sha.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(sha.get(i..i + 2).ok_or("工具哈希长度无效")?, 16)
                .map_err(|_| "工具哈希无效")
        })
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| "工具哈希长度无效")?;
    let engine = GifEngine::load(Path::new(&args[0]), hash)?;
    let input = read(Path::new(&args[2]))?;
    let input_sha = Sha256::digest(&input);
    let mode = args[4].to_str().ok_or("强度无效")?;
    let lossiness = if mode == "lossless" {
        None
    } else {
        Some(mode.parse::<u16>()?)
    };
    let sampler = memory::Sampler::start();
    let started = Instant::now();
    let candidate = lab::candidate(
        &input,
        &engine,
        GifLimits::default(),
        lossiness,
        &CancellationToken::default(),
    )?;
    let encode_ms = started.elapsed().as_millis();
    let comparison = lab::compare(
        &input,
        &candidate,
        GifLimits::default(),
        &CancellationToken::default(),
    )?;
    if Sha256::digest(read(Path::new(&args[2]))?) != input_sha {
        return Err("实验源文件变化".into());
    }
    // 语义不合格候选仍可用于人工研究，仅写独占实验文件，不经过产品输出层。
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[3])?;
    file.write_all(&candidate)?;
    file.sync_all()?;
    let report = json!({"result":"measured","inputBytes":input.len(),"outputBytes":candidate.len(),"inputSha256":format!("{input_sha:x}"),"outputSha256":format!("{:x}",Sha256::digest(&candidate)),"lossiness":lossiness,"encodeMs":encode_ms,"elapsedMs":started.elapsed().as_millis(),"comparison":comparison,"memory":sampler.finish(),"sourceUnchanged":true});
    println!("{report}");
    Ok(())
}
