import { parseDecimalU64 } from '../../lib/ipc/tasks';

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
