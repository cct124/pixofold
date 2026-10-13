//! GIF双输入的独立语义与质量检查；只比较内存，不负责候选选择或提交。
//! 时间/循环/透明为硬约束，编码RGB误差与误差随时间变化为观测，不称感知质量分数。
mod render;
#[cfg(test)]
mod tests;
use super::{
    GifError, GifLimits, Work, playback,
    structure::{self, Code, check},
};
use crate::model::CancellationToken;
use serde::Serialize;
use std::{collections::BTreeSet, time::Instant};

const MAX_COMPARED_PIXELS: u64 = 256 * 1024 * 1024;
const COMPARISON_FIXED_BYTES: u64 = 2 * 1024 * 1024;

/// 时间加权RGB均方根误差；时域量只在误差画面实际变化时累计，不随等价拆帧稀释。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub rmse: f64,
    pub worst_interval_rmse: f64,
    pub max_channel_error: u16,
    pub temporal_error_rmse: f64,
    pub worst_temporal_rmse: f64,
    pub changed_pixel_fraction: f64,
    pub intervals: usize,
}
/// 两种背景各有独立观测；调用方另行判断质量、收益与安全输出。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub hard_constraints: bool,
    pub rejection: Option<&'static str>,
    pub lossless_equivalent: bool,
    pub input_frames: usize,
    pub output_frames: usize,
    pub working_bytes_upper_bound: u64,
    pub transparent: Option<Metrics>,
    pub logical: Option<Metrics>,
}

#[derive(Default)]
struct Accumulator {
    square: f64,
    weighted_channels: f64,
    worst: f64,
    max: u16,
    temporal_square: f64,
    temporal_channels: u64,
    worst_temporal: f64,
    changed: f64,
    weighted_pixels: f64,
    intervals: usize,
    previous: Vec<i16>,
}
impl Accumulator {
    fn add(
        &mut self,
        source: &[u8],
        candidate: &[u8],
        duration: u64,
        work: &Work<'_>,
    ) -> structure::Result<bool> {
        let mut sum = 0u64;
        let mut changed = 0u64;
        let mut alpha_equal = true;
        let mut temporal_square = 0u64;
        if self.previous.is_empty() {
            self.previous.resize(source.len() / 4 * 3, 0);
        }
        for (index, (a, b)) in source
            .as_chunks::<4>()
            .0
            .iter()
            .zip(candidate.as_chunks::<4>().0)
            .enumerate()
        {
            if index % 4096 == 0 {
                work.check()?;
            }
            alpha_equal &= a[3] == b[3];
            let mut pixel_changed = false;
            for channel in 0..3 {
                let delta = if a[3] == 0 && b[3] == 0 {
                    0
                } else {
                    i16::from(b[channel]) - i16::from(a[channel])
                };
                let at = index * 3 + channel;
                sum += i64::from(delta).pow(2) as u64;
                self.max = self.max.max(delta.unsigned_abs());
                pixel_changed |= delta != 0;
                if self.intervals > 0 {
                    let difference = i32::from(delta) - i32::from(self.previous[at]);
                    temporal_square += i64::from(difference).pow(2) as u64;
                }
                self.previous[at] = delta;
            }
            changed += u64::from(pixel_changed);
        }
        let pixels = (source.len() / 4) as f64;
        let channels = pixels * 3.0;
        // 连续相同误差画面的拆分不会添加“零变化”分母；瞬态和接缝另受最差跳变限制。
        if temporal_square != 0 {
            self.temporal_square += temporal_square as f64;
            self.temporal_channels += source.len() as u64 / 4 * 3;
            self.worst_temporal = self
                .worst_temporal
                .max((temporal_square as f64 / channels).sqrt());
        }
        self.square += sum as f64 * duration as f64;
        self.weighted_channels += channels * duration as f64;
        self.worst = self.worst.max((sum as f64 / channels).sqrt());
        self.changed += changed as f64 * duration as f64;
        self.weighted_pixels += pixels * duration as f64;
        self.intervals += 1;
        Ok(alpha_equal)
    }
    fn finish(self) -> Metrics {
        Metrics {
            rmse: (self.square / self.weighted_channels).sqrt(),
            worst_interval_rmse: self.worst,
            max_channel_error: self.max,
            temporal_error_rmse: if self.temporal_channels == 0 {
                0.0
            } else {
                (self.temporal_square / self.temporal_channels as f64).sqrt()
            },
            changed_pixel_fraction: self.changed / self.weighted_pixels,
            worst_temporal_rmse: self.worst_temporal,
            intervals: self.intervals,
        }
    }
}

fn time_compatible(source: &playback::Playback, target: &structure::Structure) -> bool {
    let mut boundaries = BTreeSet::new();
    let mut target_total = 0u64;
    for frame in &target.frames {
        target_total += u64::from(frame.control.delay);
        boundaries.insert(target_total);
    }
    // 原图每个真实画面转换必须保留时间点；候选可拆帧，原图连续相同画面可合并。
    // 候选把不同原图画面量化成相同时仍保留帧边界，首版不许可删除不同画面。
    source
        .transparent
        .iter()
        .chain(&source.logical)
        .all(|cycle| {
            let mut total = 0;
            for event in cycle {
                total += event.delay_cs;
                if !boundaries.contains(&total) {
                    return false;
                }
            }
            total == target_total
        })
}

/// 双背景、代表循环的时间对齐质量比较；硬约束拒绝返回报告，损坏/取消/超限返回错误。
/// # Errors
/// 两个输入各自须通过严格生产验证；同时存活缓冲计入max_decoded_bytes。
/// 附加全画布比较总量最多256Mi像素，期限覆盖整个比较而不是每帧重置。
pub fn compare(
    source: &[u8],
    target: &[u8],
    limits: GifLimits,
    cancel: &CancellationToken,
) -> Result<Comparison, GifError> {
    compare_with_resident(source, target, limits, cancel, 0)
}

pub(super) fn compare_with_resident(
    source: &[u8],
    target: &[u8],
    limits: GifLimits,
    cancel: &CancellationToken,
    resident_bytes: u64,
) -> Result<Comparison, GifError> {
    limits.validate()?;
    let work = Work {
        cancel,
        deadline: Instant::now() + limits.validation_timeout,
    };
    let input = structure::parse(source, limits.validation_limits(), &work)?;
    let output = structure::parse(target, limits.validation_limits(), &work)?;
    let pixels = (u64::from(input.width) * u64::from(input.height))
        .max(u64::from(output.width) * u64::from(output.height));
    // 完整解码前计入两个仍在存活的输入；尺寸取实际字节结构，不复用扫描的旧头。
    // 四RGBA画布、两个索引解码、上一误差i16 RGB、压缩缓存/输入及固定余量。
    let working = pixels * 24
        + (source.len() as u64 + target.len() as u64) * 4
        + COMPARISON_FIXED_BYTES
        + resident_bytes;
    check(
        working <= limits.resources.max_decoded_bytes.0,
        Code::ResourceLimit,
        "GIF有损比较同时存活缓冲",
    )?;
    let before = playback::inspect(
        source,
        limits.validation_limits(),
        limits.resources.max_decoded_bytes.0,
        limits.resources.max_dimension,
        &work,
    )?;
    work.check()?;
    let after = playback::inspect(
        target,
        limits.validation_limits(),
        limits.resources.max_decoded_bytes.0,
        limits.resources.max_dimension,
        &work,
    )?;
    work.check()?;
    let mut report = Comparison {
        hard_constraints: false,
        rejection: None,
        lossless_equivalent: before.equivalent(&after),
        input_frames: before.frame_count,
        output_frames: after.frame_count,
        working_bytes_upper_bound: working,
        transparent: None,
        logical: None,
    };
    report.rejection = if before.width != after.width || before.height != after.height {
        Some("canvas")
    } else if before.loop_count != after.loop_count {
        Some("loop")
    } else if before.comment_sha256 != after.comment_sha256 {
        Some("metadata")
    } else if !time_compatible(&before, &output) {
        Some("timeline")
    } else {
        None
    };
    if report.rejection.is_some() {
        return Ok(report);
    }
    let mut compared = 0u64;
    for logical in [false, true] {
        let mut left = render::Frames::new(source, &input, limits.validation_limits(), logical)?;
        let mut right = render::Frames::new(target, &output, limits.validation_limits(), logical)?;
        let mut stats = Accumulator::default();
        for _ in 0..if input.loop_count.is_some() { 2 } else { 1 } {
            let (mut li, mut ri) = (1usize, 1usize);
            let (mut remaining_left, mut remaining_right) = (left.next(&work)?, right.next(&work)?);
            loop {
                let duration = remaining_left.min(remaining_right);
                compared = compared
                    .checked_add(pixels)
                    .ok_or(GifError::ResourceLimit("GIF比较工作量溢出"))?;
                check(
                    compared <= MAX_COMPARED_PIXELS,
                    Code::ResourceLimit,
                    "GIF全画布比较总像素",
                )?;
                if !stats.add(&left.canvas, &right.canvas, duration, &work)? {
                    report.rejection = Some("transparency");
                    return Ok(report);
                }
                remaining_left -= duration;
                remaining_right -= duration;
                if remaining_left == 0 && li < input.frames.len() {
                    remaining_left = left.next(&work)?;
                    li += 1;
                }
                if remaining_right == 0 && ri < output.frames.len() {
                    remaining_right = right.next(&work)?;
                    ri += 1;
                }
                if remaining_left == 0 || remaining_right == 0 {
                    check(
                        remaining_left == 0 && remaining_right == 0,
                        Code::InvalidGif,
                        "实验时间轴未同时结束",
                    )?;
                    break;
                }
            }
        }
        if logical {
            report.logical = Some(stats.finish());
        } else {
            report.transparent = Some(stats.finish());
        }
    }
    work.check()?;
    report.hard_constraints = true;
    Ok(report)
}
