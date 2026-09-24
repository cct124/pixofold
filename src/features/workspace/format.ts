import { parseDecimalU64 } from '../../lib/ipc/tasks';

/** 仅由真实字节派生减少比例；未知/零字节基准不伪造比例。 */
export function formatReduction(before: string | null, after: string | null): string {
  if (before === null || after === null) return '—';
  const input = parseDecimalU64(before);
  if (input === 0n) return '—';
  const output = parseDecimalU64(after);
  const tenths = ((input - output) * 1000n) / input;
  return (
    (tenths < 0n ? '+' : '') +
    (tenths < 0n ? -tenths / 10n : tenths / 10n) +
    '.' +
    (tenths < 0n ? -tenths % 10n : tenths % 10n) +
    '%'
  );
}

/** 字节保持bigint计算，null代表未知而非0；显示两位小数，不丢失u64精度。 */
export function formatBytes(value: string | null): string {
  if (value === null) return '—';
  const bytes = parseDecimalU64(value);
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB', 'EiB'];
  let divisor = 1n;
  let index = 0;
  while (bytes >= divisor * 1024n && index < units.length - 1) {
    divisor *= 1024n;
    ++index;
  }
  if (!index) return bytes + ' B';
  const scaled = (bytes * 100n) / divisor;
  return scaled / 100n + '.' + String(scaled % 100n).padStart(2, '0') + ' ' + units[index];
}
