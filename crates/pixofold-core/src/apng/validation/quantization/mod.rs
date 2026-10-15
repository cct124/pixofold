//! APNG共享调色板编解码与独立有损比较；生产和实验复用，没有文件提交权限。
//! 生产只选择冻结策略，实验对照入口由apng-lossy-lab特性导出。
mod budget;
mod compare;
mod encode;
mod palette;
mod remap;
mod score;
#[cfg(test)]
mod tests;

use super::{
    Error, Guard, Limits, Result, decode, require,
    structure::{self, Control, Frame, Parsed},
};
#[cfg(any(feature="apng-lossy-lab",test))]
use crate::model::CancellationToken;
use budget::Budget;
pub use compare::{Comparison, FrameQuality, QualityMetrics};
pub use remap::Stats as RemappingStats;
use serde::{Deserialize, Serialize};
use std::time::Instant;
#[cfg(any(feature="apng-lossy-lab",test))]
use std::time::Duration;

/// 实验筛选版本；不是生产质量保证或跨格式质量尺度。
pub const POLICY_VERSION: u32 = 2;

/// 准备流程的累计工作账本，包含严格无损检查、量化及实际落盘后的复验。
#[derive(Debug,Default,Clone,Copy)]
pub(in crate::apng) struct Usage {
    pub pixels:u64,
    pub decoded_bytes:u64,
    pub palette_searches:u64,
}
impl Usage {
    pub fn remaining(self,limits:Limits)->Result<Limits> {
        Ok(Limits {
            max_total_pixels:limits.max_total_pixels.checked_sub(self.pixels).ok_or(Error("total_pixel_budget"))?,
            max_decoded_bytes:limits.max_decoded_bytes.checked_sub(self.decoded_bytes).ok_or(Error("decoded_budget"))?,
            ..limits
        })
    }
    pub fn charge(&mut self,pixels:u64,decoded:u64,limits:Limits)->Result<()> {
        self.pixels=self.pixels.checked_add(pixels).ok_or(Error("total_pixel_budget"))?;
        self.decoded_bytes=self.decoded_bytes.checked_add(decoded).ok_or(Error("decoded_budget"))?;
        self.remaining(limits).map(|_|())
    }
}

/// Standard为普通对照，AlphaConstrained仅修正索引；AlphaPalette先分组生成色表再约束索引。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Remapping {
    Standard,
    AlphaConstrained,
    #[default]
    AlphaPalette,
}
/// 有界实验配置。所有额度仅允许收紧默认值；时间/工作集不是OS强制配额。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    pub quality: u8,
    pub dithering: bool,
    pub remapping: Remapping,
    pub limits: Limits,
    /// 所有图像累计采样数，每帧分得至少一个名额；全部像素仍独立验证。
    pub max_histogram_samples: usize,
    pub max_working_set_bytes: u64,
    /// 累计色表条目检查次数，含alpha可行集合预计算，独立于像素/解码额度。
    pub max_palette_searches: u64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            quality: 80,
            dithering: false,
            remapping: Remapping::AlphaPalette,
            limits: Limits::default(),
            max_histogram_samples: 65536,
            max_working_set_bytes: 512 << 20,
            max_palette_searches: 64 << 20,
        }
    }
}
impl Options {
    fn check(&self) -> Result<()> {
        self.limits.check()?;
        require(
            self.quality <= 100
                && self.max_histogram_samples > 0
                && self.max_histogram_samples <= 65536
                && self.max_working_set_bytes > 0
                && self.max_working_set_bytes <= 512 << 20
                && self.max_palette_searches <= 64 << 20,
            "invalid_limits",
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Histogram,
    Quantizing,
    Remapping,
    Comparing,
}

/// 保留被质量/收益筛选淘汰的候选供审阅；硬错误由Result返回，不转为无损回退。
#[derive(Debug)]
pub struct Candidate {
    pub bytes: Vec<u8>,
    pub report: Report,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub policy_version: u32,
    pub palette_entries: usize,
    pub histogram_samples: usize,
    pub minimum_remapping_quality: u8,
    pub remapping: Remapping,
    pub remapping_stats: RemappingStats,
    pub alpha_groups: usize,
    pub representative_samples: usize,
    pub comparison: Comparison,
    pub quality_passed: bool,
    pub quality_rejections: Vec<QualityRejection>,
    pub working_set_bound: u64,
    pub processed_pixels: u64,
    pub decoded_bytes: u64,
    pub palette_searches: u64,
    pub elapsed_ms: u128,
}

/// 初始实验筛选的具体拒绝原因；不等同于容器/解码等硬错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityRejection {
    InvalidQuality,
    RemappingBelowTarget,
    ActualPixelsBelowTarget,
    AlphaPaletteUnavailable,
    AlphaClassChanged,
    AlphaError,
    BackgroundError,
    FrameError,
    TemporalError,
    ChannelError,
}

/// 生成有界8位索引APNG候选。只读输入；取消/期限为协作检查，同步函数返回前须保留许可。
/// # Errors
/// 损坏、未支持表示/颜色、资源超限、取消/期限及编码/候选硬语义错误返回固定代码。
#[cfg(any(feature="apng-lossy-lab",test))]
pub fn quantize(
    bytes: &[u8],
    options: Options,
    cancel: &CancellationToken,
    on_stage: impl FnMut(Stage),
) -> Result<Candidate> {
    options.check()?;
    let started = Instant::now();
    let guard = Guard::until(
        started + Duration::from_millis(options.limits.deadline_ms),
        cancel,
    );
    quantize_with_usage(bytes,options,&guard,&mut Usage::default(),0,on_stage)
}

pub(in crate::apng) fn quantize_with_usage(
    bytes:&[u8],options:Options,guard:&Guard<'_>,usage:&mut Usage,resident:u64,
    mut on_stage:impl FnMut(Stage),
)->Result<Candidate> {
    options.check()?;
    let started=Instant::now();
    let parsed = structure::parse(bytes, &options.limits, guard)?;
    let gamma = supported(&parsed, bytes)?;
    let mut budget = Budget::resume(&parsed, bytes.len(), options,*usage,resident)?;
    let encoded = encode::run(
        bytes,
        &parsed,
        gamma,
        options,
        guard,
        &mut budget,
        &mut on_stage,
    )?;
    on_stage(Stage::Comparing);
    guard.check()?;
    let comparison = compare::run(bytes, &encoded.bytes, &options.limits, guard, &mut budget)?;
    guard.check()?;
    let mut quality_rejections = quality_rejections(
        &comparison,
        options.quality,
        encoded.minimum_remapping_quality,
    );
    if encoded.remapping_stats.unrepresentable_pixels > 0 {
        quality_rejections.push(QualityRejection::AlphaPaletteUnavailable);
    }
    let quality_passed = quality_rejections.is_empty();
    *usage=budget.usage();
    Ok(Candidate {
        bytes: encoded.bytes,
        report: Report {
            policy_version: POLICY_VERSION,
            palette_entries: encoded.palette_entries,
            histogram_samples: encoded.histogram_samples,
            minimum_remapping_quality: encoded.minimum_remapping_quality,
            remapping: options.remapping,
            remapping_stats: encoded.remapping_stats,
            alpha_groups: encoded.alpha_groups,
            representative_samples: encoded.representative_samples,
            comparison,
            quality_passed,
            quality_rejections,
            working_set_bound: budget.bound,
            processed_pixels: budget.pixels,
            decoded_bytes: budget.decoded_bytes,
            palette_searches: budget.palette_searches,
            elapsed_ms: started.elapsed().as_millis(),
        },
    })
}

/// 独立比较实际APNG字节；源/候选共用累计工作预算，保留两轮播放和默认图语义。
/// # Errors
/// 坏结构、控制/元数据变化、坏像素、资源及取消/期限返回固定错误；像素误差见报告。
#[cfg(any(feature="apng-lossy-lab",test))]
pub fn compare(
    source: &[u8],
    candidate: &[u8],
    options: Options,
    cancel: &CancellationToken,
) -> Result<Comparison> {
    options.check()?;
    let guard = Guard::new(&options.limits, cancel);
    compare_with_usage(source,candidate,options,&guard,&mut Usage::default(),0)
}
pub(in crate::apng) fn compare_with_usage(source:&[u8],candidate:&[u8],options:Options,guard:&Guard<'_>,usage:&mut Usage,resident:u64)->Result<Comparison> {
    options.check()?;
    let parsed = structure::parse(source, &options.limits, guard)?;
    supported(&parsed, source)?;
    let mut budget = Budget::resume(&parsed,source.len(),options,*usage,resident)?;
    let compared=compare::run(source,candidate,&options.limits,guard,&mut budget)?;
    *usage=budget.usage();
    Ok(compared)
}

/// 初始实验筛选：逐帧质量不低于请求值；黑/白背景、alpha及时间误差同时通过。
/// 指标是编码值误差，尚不是感知保证；q100要求显示像素精确，不能据此叫生产严格无损。
pub fn meets_quality(c: &Comparison, quality: u8, minimum_remapping_quality: u8) -> bool {
    quality_rejections(c, quality, minimum_remapping_quality).is_empty()
}
fn quality_rejections(
    c: &Comparison,
    quality: u8,
    minimum_remapping_quality: u8,
) -> Vec<QualityRejection> {
    let limit = f64::from(100 - quality.min(100)) * 0.2;
    let backgrounds = [&c.black, &c.white];
    let conditions = [
        (quality > 100, QualityRejection::InvalidQuality),
        (
            minimum_remapping_quality < quality,
            QualityRejection::RemappingBelowTarget,
        ),
        (
            c.minimum_pixel_quality < quality,
            QualityRejection::ActualPixelsBelowTarget,
        ),
        (
            c.alpha_class_changes != 0,
            QualityRejection::AlphaClassChanged,
        ),
        (
            c.max_alpha_error > if quality == 100 { 0.0 } else { 8.0 },
            QualityRejection::AlphaError,
        ),
        (
            backgrounds.iter().any(|m| m.rmse > limit),
            QualityRejection::BackgroundError,
        ),
        (
            backgrounds.iter().any(|m| m.worst_frame_rmse > limit * 1.5),
            QualityRejection::FrameError,
        ),
        (
            backgrounds.iter().any(|m| {
                m.temporal_error_rmse > limit * 2.0 || m.worst_temporal_rmse > limit * 2.0
            }),
            QualityRejection::TemporalError,
        ),
        (
            backgrounds
                .iter()
                .any(|m| m.max_channel_error > if quality == 100 { 0.0 } else { 64.0 }),
            QualityRejection::ChannelError,
        ),
    ];
    conditions
        .into_iter()
        .filter_map(|(rejected, reason)| rejected.then_some(reason))
        .collect()
}

fn supported(p: &Parsed<'_>, bytes: &[u8]) -> Result<f64> {
    require(p.ihdr[8] == 8, "unsupported_depth")?;
    require(p.ihdr[12] == 0, "unsupported_interlace")?;
    let mut gamma = 0.45455;
    let mut srgb = false;
    for (kind, data) in chunks(bytes) {
        match kind {
            b"cHRM" => return Err(Error("unsupported_color")),
            b"gAMA" => {
                gamma = f64::from(structure::u32_at(data, 0)) / 100000.0;
                require(gamma > 0.0 && gamma < 1.0, "unsupported_color")?;
            }
            b"sRGB" => srgb = true,
            _ => {}
        }
    }
    require(
        !srgb || (gamma - 0.45455).abs() < f64::EPSILON,
        "unsupported_color",
    )?;
    Ok(gamma)
}
// 仅用于parse完整成功的输入或固定png编码器刚生成的静态PNG，不为外部输入提供旁路。
fn chunks(bytes: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    let mut at = 8;
    std::iter::from_fn(move || {
        if at >= bytes.len() {
            return None;
        }
        let n = structure::u32_at(bytes, at) as usize;
        let result = (&bytes[at + 4..at + 8], &bytes[at + 8..at + 8 + n]);
        at += n + 12;
        Some(result)
    })
}
fn poster<'a>(p: &Parsed<'a>) -> Frame<'a> {
    Frame {
        control: Control {
            width: structure::u32_at(p.ihdr, 0),
            height: structure::u32_at(p.ihdr, 4),
            x: 0,
            y: 0,
            delay_num: 0,
            delay_den: 0,
            dispose: 0,
            blend: 0,
        },
        data: p.default.clone(),
    }
}
fn quant_error(e: imagequant::Error, guard: &Guard<'_>) -> Error {
    if let Err(error) = guard.check() {
        return error;
    }
    match e {
        imagequant::Error::OutOfMemory => Error("quantizer_memory"),
        _ => Error("quantizer"),
    }
}
