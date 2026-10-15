//! 有界APNG验证器，独立解析控制块并解码原始子帧；不持有文件或提交权限。
//! png仅解码重建的静态子帧，合成与时间语义不依赖oxipng或png的APNG控制流。
mod decode;
pub mod quantization;
mod structure;
use crate::model::CancellationToken;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
pub use structure::Control;

/// APNG验证上限；字节/像素/帧数及毫秒均显式计费，仅允许收紧默认上界。
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub max_input_bytes: usize,
    pub max_candidate_bytes: usize,
    pub max_canvas_pixels: u64,
    pub max_dimension: u32,
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
            max_dimension: 4 << 20,
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
    // 先验证上界再做索引/分配运算；零额度用于确定性拒绝测试。
    pub(super) fn check(&self) -> Result<()> {
        let d = Self::default();
        require(
            self.max_input_bytes <= d.max_input_bytes
                && self.max_dimension <= d.max_dimension
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
/// 固定拒绝代码，不含输入路径、文件名或元数据内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub &'static str);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "APNG验证拒绝：{}", self.0)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
pub fn require(condition: bool, code: &'static str) -> Result<()> {
    if condition { Ok(()) } else { Err(Error(code)) }
}
pub fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
pub(super) struct Guard<'a> {
    deadline: Instant,
    cancel: &'a CancellationToken,
}
impl<'a> Guard<'a> {
    pub fn new(limits: &Limits, cancel: &'a CancellationToken) -> Self {
        Self {
            deadline: Instant::now() + Duration::from_millis(limits.deadline_ms),
            cancel,
        }
    }
    pub fn check(&self) -> Result<()> {
        require(!self.cancel.is_cancelled(), "cancelled")?;
        require(Instant::now() < self.deadline, "deadline")
    }
    pub(super) fn until(deadline: Instant, cancel: &'a CancellationToken) -> Self {
        Self { deadline, cancel }
    }
}

/// 控制字段保留原始分数；effective_delay不约分，不引入播放器延时钳制或浮点累计。
/// 原始像素与逐帧播放摘要；用于独立验证，不授予输出或凭据移除权限。
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
    inspect_with_guard(bytes, limits, &guard)
}
pub(super) fn inspect_with_guard(
    bytes: &[u8],
    limits: &Limits,
    guard: &Guard<'_>,
) -> Result<Summary> {
    let parsed = structure::parse(bytes, limits, guard)?;
    decode::inspect(&parsed, bytes.len(), limits, guard)
}
pub(super) fn canvas_pixels(bytes: &[u8], limits: &Limits, guard: &Guard<'_>) -> Result<u64> {
    let parsed = structure::parse(bytes, limits, guard)?;
    Ok(u64::from(structure::u32_at(parsed.ihdr, 0)) * u64::from(structure::u32_at(parsed.ihdr, 4)))
}

pub(super) fn has_animation_chunks(bytes: &[u8]) -> bool {
    if !bytes.starts_with(structure::SIGNATURE) {
        return false;
    }
    let mut at = 8usize;
    while bytes.len().saturating_sub(at) >= 12 {
        if matches!(&bytes[at + 4..at + 8], b"acTL" | b"fcTL" | b"fdAT") {
            return true;
        }
        let Some(end) = at
            .checked_add(structure::u32_at(bytes, at) as usize)
            .and_then(|v| v.checked_add(12))
        else {
            return false;
        };
        if end > bytes.len() {
            return false;
        }
        at = end;
    }
    false
}
pub(super) fn structure_info(
    bytes: &[u8],
    limits: &Limits,
    cancel: &CancellationToken,
) -> Result<super::ApngInfo> {
    limits.check()?;
    let guard = Guard::new(limits, cancel);
    let p = structure::parse(bytes, limits, &guard)?;
    Ok(super::ApngInfo {
        width: structure::u32_at(p.ihdr, 0),
        height: structure::u32_at(p.ihdr, 4),
        bit_depth: p.ihdr[8],
        color_type: p.ihdr[9],
        interlaced: p.ihdr[12] == 1,
        frames: p.frames.len() as u32,
        num_plays: p.plays,
        default_participates: p.default_participates,
    })
}
pub(super) fn preview(
    bytes: &[u8],
    limits: &Limits,
    cancel: &CancellationToken,
) -> Result<super::ApngPreview> {
    limits.check()?;
    let guard = Guard::new(limits, cancel);
    let parsed = structure::parse(bytes, limits, &guard)?;
    decode::preview(&parsed, bytes.len(), limits, &guard)
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
    reduced.max_input_bytes = reduced.max_candidate_bytes;
    let b = structure::parse(candidate, &reduced, &guard)?;
    let mut second = decode::inspect(&b, candidate.len(), &reduced, &guard)?;
    // 工作集依赖压缩长度，不是动画语义。
    first.validation_bound = 0;
    second.validation_bound = 0;
    require(first == second, "not_equivalent")
}
