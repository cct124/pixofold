import type { DisplayName, OutputDirectoryPage } from './tasks.generated';
import { OUTPUT_DIRECTORY_PAGE_SIZE } from './tasks.generated';

function record(value: unknown): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value))
    throw { code: 'service_fault' };
  return value as Record<string, unknown>;
}
function integer(value: unknown, minimum: number, maximum = 0xffff_ffff): number {
  if (
    typeof value !== 'number' ||
    !Number.isSafeInteger(value) ||
    value < minimum ||
    value > maximum
  )
    throw { code: 'service_fault' };
  return value;
}
function name(value: unknown): DisplayName {
  const data = record(value);
  if (
    typeof data.text !== 'string' ||
    data.text.length > 512 ||
    typeof data.truncated !== 'boolean' ||
    typeof data.lossy !== 'boolean' ||
    typeof data.sanitized !== 'boolean'
  )
    throw { code: 'service_fault' };
  return {
    text: data.text,
    truncated: data.truncated,
    lossy: data.lossy,
    sanitized: data.sanitized,
  };
}

/** 固定版本的一页目录显示名；不接受路径、残缺分页或重复代表任务。 */
export function outputDirectoryPage(value: unknown, expectedOffset: number): OutputDirectoryPage {
  const data = record(value);
  const total = integer(data.total, 0, 100_000),
    offset = integer(data.offset, 0, total);
  if (
    offset !== expectedOffset ||
    !Array.isArray(data.items) ||
    data.items.length !== Math.min(OUTPUT_DIRECTORY_PAGE_SIZE, total - offset)
  )
    throw { code: 'service_fault' };
  const seen = new Set<number>();
  const items = data.items.map((value: unknown) => {
    const item = record(value),
      jobId = integer(item.jobId, 1);
    if (seen.has(jobId)) throw { code: 'service_fault' };
    seen.add(jobId);
    return {
      jobId,
      name: name(item.name),
      exampleName: name(item.exampleName),
      resultCount: integer(item.resultCount, 1, 100_000),
    };
  });
  return { total, offset, items };
}
