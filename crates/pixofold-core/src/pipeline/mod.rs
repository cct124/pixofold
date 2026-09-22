//! 同步单文件处理编排，不依赖 GUI，不创建后台线程，不维护另一套任务状态。
//! 调用方应在有界后台任务中执行；本入口的编码器已关闭内部并行。

use std::time::Instant;

use crate::{
    codecs::png,
    model::{
        ByteCount, CancellationToken, PngRequest, ProcessingError, ProcessingOutcome,
        ProcessingReport, ProcessingStage,
    },
    output::{self, Destination, Source},
    probe,
};

/// 执行静态 PNG 严格无损处理，结果包含真实体积和耗时。
///
/// 只通过输出层写入临时产物并提交；无收益不生成副本或备份，覆盖必保留原始备份。
/// 阶段回调在当前线程执行，应快速返回，不应 panic；没有虚构编码百分比。
/// 编码不支持即时取消：等待编码返回后检查令牌，提交临界区内不再响应取消。
///
/// # Errors
/// 非 PNG/APNG、损坏或资源超限、校验失败、取消、源变化、冲突和 I/O 失败均有独立错误。
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
    let decoded = probe::decode(&source.bytes, request.limits)?;
    let destination = Destination::plan(&source, &request.output)?;
    on_stage(ProcessingStage::Optimizing);
    cancel.check()?;
    let candidate = png::optimize(&source.bytes, request.limits)?;
    cancel.check()?;
    let mut temp = destination.stage()?;
    let validated = (|| {
        output::write_candidate(&mut temp, &candidate, &source)?;
        on_stage(ProcessingStage::Validating);
        cancel.check()?;
        let stored = output::read_candidate(&temp, request.limits)?;
        png::validate(&source.bytes, &decoded, &stored, request.limits)?;
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
        input_bytes,
        output_bytes,
        elapsed: started.elapsed(),
        outcome,
    })
}
