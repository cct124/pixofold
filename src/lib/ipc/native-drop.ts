import { invoke } from '@tauri-apps/api/core';
import { parseDecimalU64 } from './tasks';
import { MAX_NATIVE_IMPORT_ROOTS, TASK_PROTOCOL_VERSION } from './tasks.generated';
import type { NativeDropNotice, NativeImportGrant } from './tasks.generated';

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}
function positive(value: unknown): value is string {
  return typeof value === 'string' && parseDecimalU64(value) > 0n;
}

function dropGrant(value: unknown, id: string): NativeImportGrant | null {
  if (value === null) return null;
  if (
    !record(value) ||
    value.grantId !== id ||
    typeof value.rootCount !== 'number' ||
    !Number.isInteger(value.rootCount) ||
    value.rootCount < 1 ||
    value.rootCount > MAX_NATIVE_IMPORT_ROOTS
  )
    throw new Error('Invalid native drop grant');
  return { grantId: id, rootCount: value.rootCount };
}

/** 校验无路径拖放信封；无效原生输入grant=null，不得变成空路径导入。 */
export function nativeDropNotice(value: unknown): NativeDropNotice {
  if (
    !record(value) ||
    value.kind !== 'native_drop' ||
    value.protocolVersion !== TASK_PROTOCOL_VERSION ||
    !positive(value.subscriptionId) ||
    !record(value.offer) ||
    !positive(value.offer.offerId) ||
    !record(value.position) ||
    typeof value.position.x !== 'number' ||
    !Number.isFinite(value.position.x) ||
    typeof value.position.y !== 'number' ||
    !Number.isFinite(value.position.y)
  )
    throw new Error('Invalid native drop notice');
  const grant = dropGrant(value.offer.grant, value.offer.offerId);
  return {
    kind: 'native_drop',
    protocolVersion: TASK_PROTOCOL_VERSION,
    subscriptionId: value.subscriptionId,
    offer: { offerId: value.offer.offerId, grant },
    position: { x: value.position.x, y: value.position.y },
  };
}

/** 只释放这一票据；消费后的释放幂等，不取消任务。失败由调用方进入显式恢复。 */
export async function releaseNativeDrop(notice: NativeDropNotice): Promise<void> {
  const result = await invoke<unknown>('release_native_drop', {
    request: { subscriptionId: notice.subscriptionId, offerId: notice.offer.offerId },
  });
  if (result !== null) throw new Error('Invalid native drop release response');
}
