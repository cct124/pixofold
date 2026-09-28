import { invoke, isTauri } from '@tauri-apps/api/core';
import type { LogStatus } from './tasks.generated';

/** 日志状态是独立只读接口；不传递或返回任何原生路径。 */
export async function getLogStatus(): Promise<LogStatus | null> {
  if (!isTauri()) return null;
  const value: unknown = await invoke('get_log_status');
  if (
    typeof value !== 'object' ||
    value === null ||
    !('state' in value) ||
    !['starting', 'ready', 'unavailable', 'busy', 'stopped'].some(
      (state) => state === value.state,
    ) ||
    !('droppedEvents' in value) ||
    typeof value.droppedEvents !== 'string' ||
    !/^\d{1,20}$/.test(value.droppedEvents) ||
    !('writeFailures' in value) ||
    typeof value.writeFailures !== 'string' ||
    !/^\d{1,20}$/.test(value.writeFailures) ||
    !('canOpen' in value) ||
    typeof value.canOpen !== 'boolean'
  )
    throw new Error('invalid_log_status');
  // 上述边界验证覆盖生成DTO；不把未知扩展字段带入展示层。
  const state = value.state;
  if (
    state !== 'starting' &&
    state !== 'ready' &&
    state !== 'unavailable' &&
    state !== 'busy' &&
    state !== 'stopped'
  )
    throw new Error('invalid_log_status');
  return {
    state,
    droppedEvents: value.droppedEvents,
    writeFailures: value.writeFailures,
    canOpen: value.canOpen,
  };
}

/** 仅打开Rust预先确定并复查的日志目录，无通用opener权限或路径参数。 */
export async function openLogDirectory(): Promise<void> {
  if (!isTauri()) throw new Error('desktop_only');
  await invoke('open_log_directory');
}
