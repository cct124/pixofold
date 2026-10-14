//! APNG读取/编码/落盘独立复验，最终文件只经共享output提交，失败统一清理。
use super::*;
use crate::{
    model::{ByteCount, ProcessingOutcome},
    output::{self, Destination, Source},
};
use std::time::{Duration, Instant};

fn encode(bytes: &[u8], limits: ApngLimits, remaining: Duration) -> Result<Vec<u8>, ApngError> {
    let options = oxipng::Options {
        fix_errors: false,
        force: true,
        interlace: None,
        optimize_alpha: false,
        bit_depth_reduction: false,
        color_type_reduction: false,
        palette_reduction: false,
        grayscale_reduction: false,
        scale_16: false,
        strip: oxipng::StripChunks::None,
        idat_recoding: true,
        timeout: Some(remaining.min(Duration::from_secs(10))),
        max_decompressed_size: Some(limits.validation.max_canvas_pixels as usize * 9),
        ..oxipng::Options::from_preset(1)
    };
    oxipng::optimize_from_memory(bytes, &options).map_err(ApngError::Encoding)
}
pub(super) fn run(
    request: &ApngRequest,
    cancel: &CancellationToken,
    on_stage: impl FnMut(ProcessingStage),
) -> Result<ApngReport, ApngError> {
    execute(request, cancel, on_stage, encode, Instant::now())
}
fn execute(
    request: &ApngRequest,
    cancel: &CancellationToken,
    mut on_stage: impl FnMut(ProcessingStage),
    encode: impl FnOnce(&[u8], ApngLimits, Duration) -> Result<Vec<u8>, ApngError>,
    started: Instant,
) -> Result<ApngReport, ApngError> {
    execute_timed(
        request,
        cancel,
        &mut on_stage,
        encode,
        started,
        Instant::now,
    )
}
fn execute_timed(
    request: &ApngRequest,
    cancel: &CancellationToken,
    mut on_stage: impl FnMut(ProcessingStage),
    encode: impl FnOnce(&[u8], ApngLimits, Duration) -> Result<Vec<u8>, ApngError>,
    started: Instant,
    now: impl Fn() -> Instant,
) -> Result<ApngReport, ApngError> {
    let limits = request.limits;
    limits.check()?;
    let deadline = started + limits.preparation_timeout;
    let check = || {
        if cancel.is_cancelled() {
            Err(ApngError::Cancelled)
        } else if now() >= deadline {
            Err(ApngError::Timeout)
        } else {
            Ok(())
        }
    };
    check()?;
    on_stage(ProcessingStage::Reading);
    check()?;
    limits.working_set(0)?;
    let mut source = Source::read(&request.source, limits.resources())?;
    source.use_apng_backup_suffix();
    // 读取器可能预留额外容量；后续配额/比较按实际存活字节计费。
    source.bytes = source.bytes.into_boxed_slice().into_vec();
    check()?;
    let phase_guard = || {
        validation::Guard::until(
            deadline.min(Instant::now() + Duration::from_millis(limits.validation.deadline_ms)),
            cancel,
        )
    };
    limits.working_set(validation::canvas_pixels(
        &source.bytes,
        &limits.validation,
        &phase_guard(),
    )?)?;
    let mut before =
        validation::inspect_with_guard(&source.bytes, &limits.validation, &phase_guard())?;
    if before.decoded_bytes > limits.validation.max_decoded_bytes / 2 {
        return Err(ApngError::ResourceLimit("decoded_budget"));
    }
    if before.decoded_pixels > limits.validation.max_total_pixels / 2 {
        return Err(ApngError::ResourceLimit("total_pixel_budget"));
    }
    let image = ApngInfo::from(&before);
    let destination = Destination::plan(&source, &request.output)?;
    on_stage(ProcessingStage::Optimizing);
    check()?;
    // 不另起可脱离任务的编码线程；直到同步调用返回，源/候选和调用方许可始终存活。
    let encoded = encode(
        &source.bytes,
        limits,
        deadline.saturating_duration_since(now()),
    );
    check()?;
    let candidate = encoded?;
    if candidate.len() > limits.validation.max_candidate_bytes {
        return Err(ApngError::ResourceLimit("candidate_budget"));
    }
    let expected_hash = validation::hash(&candidate);
    let mut temp = destination.stage()?;
    let validated = (|| {
        output::write_candidate(&mut temp, &candidate, &source)?;
        drop(candidate);
        on_stage(ProcessingStage::Validating);
        check()?;
        let mut candidate_resources = limits.resources();
        candidate_resources.max_input_bytes =
            ByteCount(limits.validation.max_candidate_bytes as u64);
        let stored = output::read_candidate(&temp, candidate_resources)?
            .into_boxed_slice()
            .into_vec();
        if validation::hash(&stored) != expected_hash {
            return Err(ApngError::ValidationFailed);
        }
        // 源已严格验证；候选只取剩余累计工作额度。两份压缩输入和已存摘要继续计费。
        let mut remaining = limits.validation;
        remaining.max_input_bytes = remaining.max_candidate_bytes;
        remaining.max_decoded_bytes = remaining
            .max_decoded_bytes
            .checked_sub(before.decoded_bytes)
            .ok_or(ApngError::ResourceLimit("decoded_budget"))?;
        remaining.max_total_pixels = remaining
            .max_total_pixels
            .checked_sub(before.decoded_pixels)
            .ok_or(ApngError::ResourceLimit("total_pixel_budget"))?;
        remaining.max_validation_bytes = remaining
            .max_validation_bytes
            .checked_sub(source.bytes.len() as u64)
            .ok_or(ApngError::ResourceLimit("validation_budget"))?;
        let mut after = validation::inspect_with_guard(&stored, &remaining, &phase_guard())?;
        before.validation_bound = 0;
        after.validation_bound = 0;
        if before != after {
            return Err(ApngError::ValidationFailed);
        }
        source.verify_unchanged(limits.resources())?;
        check()?;
        Ok(stored)
    })();
    let stored = match validated {
        Ok(stored) => stored,
        Err(error) => {
            let temporary = temp.path().to_owned();
            return Err(match temp.close() {
                Ok(()) => error,
                Err(source) => ApngError::Cleanup {
                    original: Box::new(error),
                    source,
                    temporary,
                },
            });
        }
    };
    let input_bytes = ByteCount(source.bytes.len() as u64);
    let (outcome, output_bytes) = if stored.len() >= source.bytes.len() {
        output::discard_no_gain(temp)?;
        (ProcessingOutcome::NoGain, input_bytes)
    } else {
        let size = ByteCount(stored.len() as u64);
        (
            output::commit(
                temp,
                &stored,
                &destination,
                source,
                limits.resources(),
                cancel,
                &mut on_stage,
            )?,
            size,
        )
    };
    Ok(ApngReport {
        image,
        processing: ApngProcessing::Lossless,
        input_bytes,
        output_bytes,
        elapsed: started.elapsed(),
        outcome,
    })
}

#[cfg(test)]
mod tests;
