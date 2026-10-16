import type {
  ApngFallbackDto,
  FallbackDto,
  GifFallbackDto,
  JpegFallbackDto,
  JobDto,
  JobErrorDto,
  JobFailureDto,
  TaskSnapshotDto,
} from './tasks.generated';

const failures = {
  invalid_input: true,
  unsupported_format: true,
  unsupported_animation: true,
  unsupported_content_credentials: true,
  unsupported_jpeg_credentials: true,
  unsupported_gif_mode: true,
  unsupported_apng: true,
  unsupported_apng_metadata: true,
  unsupported_apng_mode: true,
  unsupported_metadata: true,
  resource_limit: true,
  decode: true,
  encode: true,
  validation: true,
  target_conflict: true,
  source_changed: true,
  io: true,
  commit_failed: true,
  cleanup_failed: true,
  worker_panicked: true,
  service_fault: true,
  tool_identity: true,
  tool_io: true,
  tool_exit: true,
  timed_out: true,
} satisfies Record<JobErrorDto, true>;
const pngFallbacks = {
  high_bit_depth: true,
  color_metadata: true,
  representation_metadata: true,
  transparency_guard: true,
  no_size_benefit: true,
  quality_below_target: true,
} satisfies Record<FallbackDto['kind'], true>;
const jpegFallbacks = {
  color_profile: true,
  four_component_color: true,
  embedded_thumbnail: true,
  ambiguous_color: true,
} satisfies Record<JpegFallbackDto['kind'], true>;
const gifFallbacks = {
  quality_below_target: true,
  insufficient_gain: true,
} satisfies Record<GifFallbackDto['kind'], true>;
const apngFallbacks = {
  quality_below_target: true,
  insufficient_gain: true,
} satisfies Record<ApngFallbackDto['kind'], true>;

function requireContract(condition: boolean): void {
  if (!condition) throw new Error('Inconsistent image contract');
}
function quality(value: number, min = 0): boolean {
  return Number.isInteger(value) && value >= min && value <= 100;
}
function failure(value: JobFailureDto): void {
  requireContract(Object.hasOwn(failures, value.code));
  const original = value.recovery?.originalError;
  if (original) {
    requireContract(original.kind === 'cancelled' || original.kind === 'failed');
    if (original.kind === 'failed') requireContract(Object.hasOwn(failures, original.code));
  }
}
function report(job: JobDto): void {
  requireContract(
    job.mode.kind === 'lossless' || (job.mode.kind === 'lossy' && quality(job.mode.quality)),
  );
  if (job.state.kind === 'failed') failure(job.state.failure);
  if (job.state.kind !== 'succeeded' && job.state.kind !== 'no_gain') return;
  requireContract(!job.state.report.contentCredentialsRemoved || job.state.kind === 'succeeded');
  const value = job.state.report.processing;
  requireContract(value.format === job.format);
  const details = value.details;
  requireContract(['lossless', 'lossy', 'lossless_fallback'].includes(details.kind));
  if (details.kind === 'lossless') {
    // GIF可能实际无损；APNG的q100在格式分支中核对。
    requireContract(
      value.format === 'gif' || value.format === 'apng' || job.mode.kind === 'lossless',
    );
  } else {
    requireContract(
      job.mode.kind === 'lossy' &&
        Number.isSafeInteger(details.mappingVersion) &&
        details.mappingVersion > 0,
    );
  }
  if (value.format === 'jpeg') {
    if (value.details.kind !== 'lossless') {
      requireContract(value.details.mappingVersion === 1);
      requireContract(quality(value.details.nativeQuality, 1));
      if (job.mode.kind === 'lossy')
        requireContract(value.details.nativeQuality === Math.max(1, job.mode.quality));
    }
    if (value.details.kind === 'lossless_fallback') {
      requireContract(Object.hasOwn(jpegFallbacks, value.details.reason.kind));
    }
  } else if (value.format === 'gif') {
    requireContract(job.state.report.contentCredentialsRemoved === false);
    if (value.details.kind !== 'lossless') {
      requireContract(job.mode.kind === 'lossy' && job.mode.quality < 100);
      requireContract(value.details.mappingVersion === 1);
      if (job.mode.kind === 'lossy') {
        requireContract(
          value.details.nativeLossiness === Math.min(20, Math.ceil((100 - job.mode.quality) / 2)),
        );
      }
    }
    if (value.details.kind === 'lossless_fallback')
      requireContract(Object.hasOwn(gifFallbacks, value.details.reason.kind));
    requireContract(
      Number.isInteger(value.details.frames) &&
        value.details.frames > 0 &&
        value.details.frames <= 0xffffffff,
    );
    const loops = value.details.loopCount;
    requireContract(loops === null || (Number.isInteger(loops) && loops >= 0 && loops <= 65535));
  } else if (value.format === 'apng') {
    requireContract(job.state.report.contentCredentialsRemoved === false);
    if (value.details.kind === 'lossless') {
      requireContract(
        job.mode.kind === 'lossless' || (job.mode.kind === 'lossy' && job.mode.quality === 100),
      );
      requireContract(!('quality' in value.details));
    } else {
      requireContract(job.mode.kind === 'lossy' && job.mode.quality < 100);
      requireContract(value.details.mappingVersion === 1 && value.details.policyVersion === 2);
      if (value.details.kind === 'lossless_fallback') {
        requireContract(Object.hasOwn(apngFallbacks, value.details.reason.kind));
        requireContract(!('quality' in value.details));
      } else {
        requireContract(job.state.kind === 'succeeded');
        const measured = value.details.quality;
        requireContract(!!measured && typeof measured === 'object');
        requireContract(
          Number.isInteger(measured.paletteEntries) &&
            measured.paletteEntries >= 1 &&
            measured.paletteEntries <= 256,
        );
        requireContract(
          quality(measured.minimumPixelQuality) && quality(measured.minimumRemappingQuality),
        );
        if (job.mode.kind === 'lossy') {
          const limit = (100 - job.mode.quality) * 0.2;
          requireContract(
            measured.minimumPixelQuality >= job.mode.quality &&
              measured.minimumRemappingQuality >= job.mode.quality,
          );
          requireContract(
            (
              [
                [measured.maxAlphaError, 8],
                [measured.worstFrameRmse, limit * 1.5],
                [measured.worstTemporalRmse, limit * 2],
              ] as const
            ).every(([value, maximum]) => Number.isFinite(value) && value >= 0 && value <= maximum),
          );
        }
      }
    }
    requireContract(
      Number.isInteger(value.details.frames) &&
        value.details.frames > 0 &&
        value.details.frames <= 256,
    );
    requireContract(
      Number.isInteger(value.details.numPlays) &&
        value.details.numPlays >= 0 &&
        value.details.numPlays <= 0xffffffff,
    );
    requireContract(typeof value.details.defaultParticipates === 'boolean');
  } else {
    if (value.details.kind === 'lossy') requireContract(quality(value.details.measuredQuality));
    if (value.details.kind === 'lossless_fallback') {
      requireContract(Object.hasOwn(pngFallbacks, value.details.reason.kind));
      if (value.details.reason.kind === 'quality_below_target') {
        const measured = value.details.reason.measured;
        requireContract(measured === null || quality(measured));
      }
    }
  }
}

/** v15：JPEG已确认的成功输出可标记凭据移除；GIF/APNG仍禁止，NoGain不能冒充已移除。 */
export function validateImages(snapshot: TaskSnapshotDto): void {
  const formats = snapshot.supportedFormats;
  requireContract(Array.isArray(formats) && formats.length >= 1 && formats.length <= 4);
  requireContract(formats.includes('png') && new Set(formats).size === formats.length);
  requireContract(
    formats.every(
      (format) => format === 'png' || format === 'jpeg' || format === 'gif' || format === 'apng',
    ),
  );
  if (snapshot.error?.code === 'file') failure(snapshot.error.failure);
  const page = snapshot.page;
  if (page.kind === 'candidates' || page.kind === 'jobs') {
    for (const row of page.items) requireContract(formats.includes(row.format));
  }
  if (page.kind === 'jobs') {
    for (const row of page.items) report(row);
  } else if (page.kind === 'issues') {
    for (const row of page.items) if (row.issue.kind === 'failure') failure(row.issue.failure);
  }
}
