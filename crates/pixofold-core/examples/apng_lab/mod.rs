//! APNG开发验证器，独立解析控制块并解码原始子帧；不进入生产格式或输出提交层。
//! png仅解码重建的静态子帧，合成与时间语义不依赖oxipng或png的APNG控制流。
mod decode;
mod structure;
#[cfg(test)]
mod tests;

use pixofold_core::model::CancellationToken;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
pub use structure::Control;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub max_input_bytes: usize,
    pub max_candidate_bytes: usize,
    pub max_canvas_pixels: u64,
    pub max_frame_pixels: u64,
    pub max_total_pixels: u64,
    pub max_decoded_bytes: u64,
    pub max_frames: usize,
    pub max_chunks: usize,
    pub max_metadata_bytes: usize,
    pub max_validation_bytes: u64,
    pub deadline_ms: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: 16 << 20,
            max_candidate_bytes: 16 << 20,
            max_canvas_pixels: 4 << 20,
            max_frame_pixels: 4 << 20,
            max_total_pixels: 64 << 20,
            max_decoded_bytes: 512 << 20,
            max_frames: 256,
            max_chunks: 4096,
            max_metadata_bytes: 1 << 20,
            max_validation_bytes: 128 << 20,
            deadline_ms: 10_000,
        }
    }
}
impl Limits {
    // 本阶段仅允许收紧实验额度；先验证上界再做索引/分配运算。
    fn check(&self) -> Result<()> {
        let d = Self::default();
        require(
            self.max_input_bytes <= d.max_input_bytes
                && self.max_candidate_bytes <= d.max_candidate_bytes
                && self.max_canvas_pixels <= d.max_canvas_pixels
                && self.max_frame_pixels <= d.max_frame_pixels
                && self.max_total_pixels <= d.max_total_pixels
                && self.max_decoded_bytes <= d.max_decoded_bytes
                && self.max_frames <= d.max_frames
                && self.max_chunks <= d.max_chunks
                && self.max_metadata_bytes <= d.max_metadata_bytes
                && self.max_validation_bytes <= d.max_validation_bytes
                && self.deadline_ms <= d.deadline_ms,
            "invalid_limits",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub &'static str);
pub type Result<T> = std::result::Result<T, Error>;
pub fn require(condition: bool, code: &'static str) -> Result<()> {
    if condition { Ok(()) } else { Err(Error(code)) }
}
pub fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
pub struct Guard<'a> {
    started: Instant,
    deadline: Duration,
    cancel: &'a CancellationToken,
}
impl<'a> Guard<'a> {
    pub fn new(limits: &Limits, cancel: &'a CancellationToken) -> Self {
        Self {
            started: Instant::now(),
            deadline: Duration::from_millis(limits.deadline_ms),
            cancel,
        }
    }
    pub fn check(&self) -> Result<()> {
        require(!self.cancel.is_cancelled(), "cancelled")?;
        require(self.started.elapsed() < self.deadline, "deadline")
    }
}

/// 控制字段保留原始分数；effective_delay不约分，不引入播放器延时钳制或浮点累计。
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameSummary {
    pub control: Control,
    pub effective_delay: [u16; 2],
    pub raw_sha256: String,
    pub canvas_sha256: String,
    // 小图才保留可审阅的完整画布；大图只保留摘要，避免累计帧缓存。
    pub canvas: Option<Vec<[u16; 4]>>,
}
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub width: u32,
    pub height: u32,
    pub depth: u8,
    pub color: u8,
    pub interlace: u8,
    pub plays: u32,
    pub default_participates: bool,
    pub default_raw_sha256: String,
    pub metadata_sha256: String,
    pub frames: Vec<FrameSummary>,
    pub final_canvas_sha256: String,
    pub second_play_canvases: Vec<String>,
    pub loop_start: &'static str,
    pub decoded_bytes: u64,
    pub decoded_pixels: u64,
    pub validation_bound: u64,
}

pub fn inspect(bytes: &[u8], limits: &Limits, cancel: &CancellationToken) -> Result<Summary> {
    limits.check()?;
    let guard = Guard::new(limits, cancel);
    let parsed = structure::parse(bytes, limits, &guard)?;
    decode::inspect(&parsed, bytes.len(), limits, &guard)
}

/// 结构/原始子帧/隐藏RGB/元数据/逐帧合成/原始延时全部保持才接纳。
pub fn compare(
    source: &[u8],
    candidate: &[u8],
    limits: &Limits,
    cancel: &CancellationToken,
) -> Result<()> {
    limits.check()?;
    require(
        candidate.len() <= limits.max_candidate_bytes,
        "candidate_budget",
    )?;
    // 两份压缩输入持续存活，预留候选容量后再分配验证器缓冲。
    let mut reduced = *limits;
    reduced.max_validation_bytes = limits
        .max_validation_bytes
        .checked_sub(source.len().max(candidate.len()) as u64)
        .ok_or(Error("validation_budget"))?;
    let guard = Guard::new(limits, cancel);
    let a = structure::parse(source, &reduced, &guard)?;
    let mut first = decode::inspect(&a, source.len(), &reduced, &guard)?;
    // 一次比较共用累计工作额度，不能给源和候选各自重新发放完整预算。
    reduced.max_decoded_bytes = reduced
        .max_decoded_bytes
        .checked_sub(first.decoded_bytes)
        .ok_or(Error("decoded_budget"))?;
    reduced.max_total_pixels = reduced
        .max_total_pixels
        .checked_sub(first.decoded_pixels)
        .ok_or(Error("total_pixel_budget"))?;
    let b = structure::parse(candidate, &reduced, &guard)?;
    let mut second = decode::inspect(&b, candidate.len(), &reduced, &guard)?;
    // 工作集依赖压缩长度，不是动画语义。
    first.validation_bound = 0;
    second.validation_bound = 0;
    require(first == second, "not_equivalent")
}
