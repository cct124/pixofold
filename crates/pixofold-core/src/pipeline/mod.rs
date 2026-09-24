//! 同步单文件处理编排，不依赖 GUI，不创建后台线程，不维护另一套任务状态。
//! 调用方应在有界后台任务中执行；本入口的编码器已关闭内部并行。

use std::{borrow::Cow, time::Instant};

use crate::{
    codecs::{png, png_lossy},
    model::{
        ByteCount, CancellationToken, PngMetadataPolicy, PngProcessing, PngRequest,
        ProcessingError, ProcessingOutcome, ProcessingReport, ProcessingStage,
    },
    output::{self, Destination, Source},
    probe,
};

/// 执行静态 PNG 处理，默认无损，有损按版本化质量映射生成候选；结果包含真实路径与回退原因。
///
/// 只通过输出层写入临时产物并提交；无收益不生成副本或备份，覆盖默认保留原始备份。
/// 显式OverwriteWithoutBackup不建立备份，但仍执行完整校验及安全替换。
/// 阶段回调在当前线程执行，应快速返回，不应 panic；没有虚构编码百分比。
/// imagequant 在进度回调协作取消；其他不可中断阶段等待返回后检查令牌。
/// 取消不等于即时终止计算，提交临界区内不再响应取消。
///
/// # Errors
/// 非 PNG/APNG、不支持改写的元数据、损坏或资源超限、校验失败、取消、源变化、冲突和 I/O 失败均有独立错误。
/// 提交失败的错误携带恢复备份；清理失败携带残留临时路径。成功不是断电事务保证。
pub fn optimize_png(
    request: &PngRequest,
    cancel: &CancellationToken,
    mut on_stage: impl FnMut(ProcessingStage),
) -> Result<ProcessingReport, ProcessingError> {
    let started = Instant::now();
    request.limits.validate()?;
    cancel.check()?;
    on_stage(ProcessingStage::Reading);
    cancel.check()?;
    let source = Source::read(&request.source, request.limits)?;
    if let PngMetadataPolicy::RemoveContentCredentials(expected) = &request.metadata {
        source.verify_credentials_version(expected)?;
    }
    let decoded = probe::decode(&source.bytes, request.limits)?;
    let has_credentials = png::check_metadata(&source.bytes)?;
    let working = match &request.metadata {
        PngMetadataPolicy::Preserve if has_credentials => {
            return Err(ProcessingError::ContentCredentialsRequireConsent(
                source.credentials_version(),
            ));
        }
        PngMetadataPolicy::Preserve => Cow::Borrowed(source.bytes.as_slice()),
        PngMetadataPolicy::RemoveContentCredentials(_) if has_credentials => {
            Cow::Owned(png::remove_content_credentials(&source.bytes)?)
        }
        PngMetadataPolicy::RemoveContentCredentials(_) => {
            return Err(ProcessingError::SourceChanged);
        }
    };
    let destination = Destination::plan(&source, &request.output)?;
    on_stage(ProcessingStage::Optimizing);
    cancel.check()?;
    let candidate = png_lossy::prepare(&working, &decoded, request.mode, request.limits, cancel)?;
    cancel.check()?;
    // 量化器已对最终 palette/indices 做独立 RGBA 回读比较。落盘后以该候选为验证基准；
    // 无损及回退仍与原始像素及策略工作副本的元数据比较，仅豁免已明确同意的caBX。
    let quantized = matches!(candidate.processing, PngProcessing::Lossy { .. });
    let expected = if quantized {
        Some(probe::decode(&candidate.bytes, request.limits)?)
    } else {
        None
    };
    let output_image = expected.as_ref().unwrap_or(&decoded).info.clone();
    let processing = candidate.processing;
    let mut temp = destination.stage()?;
    let validated = (|| {
        output::write_candidate(&mut temp, &candidate.bytes, &source)?;
        on_stage(ProcessingStage::Validating);
        cancel.check()?;
        let stored = output::read_candidate(&temp, request.limits)?;
        if let Some(expected) = &expected {
            png::validate(&candidate.bytes, expected, &stored, request.limits)?;
        } else {
            png::validate(&working, &decoded, &stored, request.limits)?;
        }
        source.verify_unchanged(request.limits)?;
        cancel.check()?;
        Ok(stored)
    })();
    drop(candidate);
    let validated_bytes = match validated {
        Ok(bytes) => bytes,
        Err(error) => return Err(output::discard(temp, error)),
    };
    let output_size = validated_bytes.len() as u64;
    let input_bytes = ByteCount(source.bytes.len() as u64);
    drop(working);
    let (outcome, output_bytes) = if output_size >= input_bytes.0 {
        output::discard_no_gain(temp)?;
        (ProcessingOutcome::NoGain, input_bytes)
    } else {
        let outcome = output::commit(
            temp,
            &validated_bytes,
            &destination,
            source,
            request.limits,
            cancel,
            &mut on_stage,
        )?;
        (outcome, ByteCount(output_size))
    };
    Ok(ProcessingReport {
        image: decoded.info,
        output_image,
        processing,
        content_credentials_removed: has_credentials
            && matches!(outcome, ProcessingOutcome::Optimized { .. }),
        input_bytes,
        output_bytes,
        elapsed: started.elapsed(),
        outcome,
    })
}
