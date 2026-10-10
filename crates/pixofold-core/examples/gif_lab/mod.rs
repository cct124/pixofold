//! GIF首轮实验验证模块，编译到example和开发测试；不暴露产品格式能力。
pub mod playback;
pub mod structure;

use playback::{Event, inspect, normalize};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{error::Error, fs, io::Read, path::Path};
use structure::Limits;

pub fn read_bounded(path: &Path, limits: Limits) -> Result<Vec<u8>, Box<dyn Error>> {
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(limits.max_input_bytes as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limits.max_input_bytes {
        return Err(structure::LabError {
            code: structure::Code::ResourceLimit,
            reason: "输入字节",
        }
        .into());
    }
    Ok(bytes)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expected {
    width: u16,
    height: u16,
    loop_count: Option<u16>,
    comment_hex: Vec<String>,
    transparent: Vec<Event>,
    logical: Vec<Event>,
    // 有循环接缝的样本另给出人工第二轮画布，防止错误重置画布仍通过首轮预期。
    transparent_second_cycle: Option<Vec<Event>>,
    logical_second_cycle: Option<Vec<Event>>,
}
#[derive(Deserialize)]
struct Sample {
    file: String,
    sha256: String,
    expect: String,
    #[serde(default)]
    limits: Limits,
    expected: Option<Expected>,
}
#[derive(Deserialize)]
struct Comparison {
    left: String,
    right: String,
    equivalent: bool,
}
#[derive(Deserialize)]
struct Manifest {
    samples: Vec<Sample>,
    comparisons: Vec<Comparison>,
}

pub fn corpus(directory: &Path, manifest: &Path) -> Result<Value, Box<dyn Error>> {
    let manifest: Manifest = serde_json::from_slice(&fs::read(manifest)?)?;
    let mut results = Vec::new();
    for sample in &manifest.samples {
        let bytes = read_bounded(&directory.join(&sample.file), Limits::default())?;
        if format!("{:x}", Sha256::digest(&bytes)) != sample.sha256 {
            return Err("语料身份改变".into());
        }
        match inspect(&bytes, sample.limits) {
            Ok(report) => {
                if sample.expect != "ok" {
                    return Err(format!("应拒绝：{}", sample.file).into());
                }
                let expected = sample.expected.as_ref().ok_or("缺少人工预期")?;
                if report.width != expected.width
                    || report.height != expected.height
                    || report.loop_count != expected.loop_count
                    || report.comment_sha256
                        != expected
                            .comment_hex
                            .iter()
                            .map(|hex| {
                                let (pairs, remainder) = hex.as_bytes().as_chunks::<2>();
                                if !remainder.is_empty() {
                                    return Err("评论hex长度必须为偶数".into());
                                }
                                let bytes = pairs
                                    .iter()
                                    .map(|pair| {
                                        Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?)
                                    })
                                    .collect::<Result<Vec<u8>, Box<dyn Error>>>()?;
                                Ok(format!("{:x}", Sha256::digest(bytes)))
                            })
                            .collect::<Result<Vec<String>, Box<dyn Error>>>()?
                    || report.transparent.first() != Some(&normalize(&expected.transparent)?)
                    || report.logical.first() != Some(&normalize(&expected.logical)?)
                {
                    return Err(format!("人工画布/时间轴不符：{}", sample.file).into());
                }
                for (events, actual) in [
                    (&expected.transparent_second_cycle, &report.transparent),
                    (&expected.logical_second_cycle, &report.logical),
                ] {
                    if let Some(events) = events
                        && actual.get(1) != Some(&normalize(events)?)
                    {
                        return Err(format!("人工循环接缝不符：{}", sample.file).into());
                    }
                }
                results.push(
                    json!({"file":sample.file,"result":"accepted","frames":report.frame_count}),
                );
            }
            Err(error) => {
                if format!("{:?}", error.code) != sample.expect {
                    return Err(format!("拒绝类别不符：{}，{}", sample.file, error).into());
                }
                results.push(json!({"file":sample.file,"result":"rejected","code":error.code}));
            }
        }
    }
    for comparison in &manifest.comparisons {
        let before = inspect(
            &read_bounded(&directory.join(&comparison.left), Limits::default())?,
            Limits::default(),
        )?;
        let after = inspect(
            &read_bounded(&directory.join(&comparison.right), Limits::default())?,
            Limits::default(),
        )?;
        if before.equivalent(&after) != comparison.equivalent {
            return Err(
                format!("语义正负例不符：{} / {}", comparison.left, comparison.right).into(),
            );
        }
    }
    Ok(json!({"result":"passed","samples":results,"comparisons":manifest.comparisons.len()}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independently_decoded_corpus_matches_manual_canvases_and_negative_changes() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/gif");
        corpus(&directory, &directory.join("manifest.json")).unwrap();
    }
    #[test]
    fn decoded_budget_covers_both_backgrounds_and_representative_loops() {
        let still = include_bytes!("../../../../tests/fixtures/gif/static87.gif");
        let looping = include_bytes!("../../../../tests/fixtures/gif/finite-loop1.gif");
        // 人工语料均为3×2像素：单帧×双背景=48字节，双帧×双背景×双轮次=192字节。
        for (bytes, required) in [(still.as_slice(), 48), (looping.as_slice(), 192)] {
            let limits = Limits {
                max_total_decoded_bytes: required,
                ..Limits::default()
            };
            assert!(inspect(bytes, limits).is_ok());
            assert!(matches!(
                inspect(
                    bytes,
                    Limits {
                        max_total_decoded_bytes: required - 1,
                        ..limits
                    }
                ),
                Err(structure::LabError {
                    code: structure::Code::ResourceLimit,
                    ..
                })
            ));
        }
    }
    #[test]
    fn truncation_of_every_prefix_and_structural_bit_flips_never_panic() {
        let bytes = include_bytes!("../../../../tests/fixtures/gif/static87.gif");
        for end in 0..bytes.len() {
            assert!(inspect(&bytes[..end], Limits::default()).is_err());
        }
        for at in 0..bytes.len() {
            for bit in 0..8 {
                let mut modified = bytes.to_vec();
                modified[at] ^= 1 << bit;
                let _ = inspect(&modified, Limits::default());
            }
        }
    }
}
