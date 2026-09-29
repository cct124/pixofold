import { invoke, isTauri } from '@tauri-apps/api/core';
import { parseDecimalU64 } from './tasks';
import { MAX_THUMBNAIL_WIDTH, MAX_THUMBNAIL_HEIGHT, MAX_THUMBNAIL_BYTES } from './tasks.generated';
import type { AssetError, JobAssetRequest, RevealTarget, ThumbnailDto } from './tasks.generated';

export type AssetIdentity = Omit<JobAssetRequest, 'subscriptionId'>;
export type ThumbnailView =
  | { kind: 'loading' }
  | { kind: 'ready'; url: string; width: number; height: number }
  | { kind: 'unavailable'; code: AssetError['code'] };
type Listener = (view: ThumbnailView) => void;
type Entry = {
  request: JobAssetRequest;
  listeners: Set<Listener>;
  view: ThumbnailView;
  queued: boolean;
};
const MAX_VISIBLE_THUMBNAILS = 64;
const errorCodes: readonly AssetError['code'][] = [
  'session_unavailable',
  'stale_task',
  'unavailable',
  'busy',
  'file_missing',
  'file_changed',
  'unsafe_path',
  'resource_limit',
  'decode_failed',
  'reveal_failed',
  'service_fault',
];
export function assetErrorCode(error: unknown): AssetError['code'] {
  if (typeof error === 'object' && error !== null && 'code' in error) {
    return errorCodes.find((code) => code === error.code) ?? 'service_fault';
  }
  return 'service_fault';
}
function fail(code: AssetError['code']): AssetError {
  return { code };
}
function requestFor(session: string | null, identity: AssetIdentity): JobAssetRequest {
  if (!isTauri() || !session) throw fail('session_unavailable');
  if (
    parseDecimalU64(session) === 0n ||
    parseDecimalU64(identity.selectionId) === 0n ||
    !Number.isSafeInteger(identity.jobId) ||
    identity.jobId < 1 ||
    identity.jobId > 0xffff_ffff ||
    !Number.isSafeInteger(identity.attempt) ||
    identity.attempt < 1 ||
    identity.attempt > 0xffff_ffff ||
    !['succeeded', 'no_gain', 'failed', 'cancelled'].includes(identity.expectedState)
  ) {
    throw fail('stale_task');
  }
  // 显式构造信封；展示名称、路径或调用者的额外字段不进入IPC。
  return {
    subscriptionId: session,
    selectionId: identity.selectionId,
    jobId: identity.jobId,
    attempt: identity.attempt,
    expectedState: identity.expectedState,
  };
}
function thumbnail(value: unknown): ThumbnailDto {
  if (
    typeof value !== 'object' ||
    value === null ||
    !('width' in value) ||
    !('height' in value) ||
    !('png' in value)
  )
    throw fail('service_fault');
  const { width, height, png } = value;
  if (
    typeof width !== 'number' ||
    !Number.isInteger(width) ||
    width < 1 ||
    width > MAX_THUMBNAIL_WIDTH ||
    typeof height !== 'number' ||
    !Number.isInteger(height) ||
    height < 1 ||
    height > MAX_THUMBNAIL_HEIGHT ||
    !Array.isArray(png) ||
    png.length < 33 ||
    png.length > MAX_THUMBNAIL_BYTES ||
    !png.every(
      (byte: unknown) =>
        typeof byte === 'number' && Number.isInteger(byte) && byte >= 0 && byte <= 255,
    ) ||
    ![137, 80, 78, 71, 13, 10, 26, 10].every((byte, index) => png[index] === byte) ||
    ![73, 72, 68, 82].every((byte, index) => png[12 + index] === byte)
  ) {
    throw fail('service_fault');
  }
  const bytes: number[] = png;
  const header = new DataView(Uint8Array.from(bytes.slice(16, 24)).buffer);
  if (header.getUint32(0) !== width || header.getUint32(4) !== height) throw fail('service_fault');
  return { width, height, png: bytes };
}

/**
 * 页面级可见缩略图队列，最多64项/一个IPC在途。无新Channel或文件路径。
 * 最后一个观察者离开即撤销Blob URL；不可取消的解码返回后丢弃，不重发写操作。
 */
export class TaskAssets {
  readonly #session: () => string | null;
  #entries = new Map<string, Entry>();
  #running = false;
  #revealing = false;
  #generation = 0;
  constructor(session: () => string | null) {
    this.#session = session;
  }

  watch(identity: AssetIdentity, listener: Listener): () => void {
    let request: JobAssetRequest;
    try {
      request = requestFor(this.#session(), identity);
    } catch (error) {
      listener({ kind: 'unavailable', code: assetErrorCode(error) });
      return () => {};
    }
    const key = JSON.stringify(request);
    let entry = this.#entries.get(key);
    if (!entry) {
      if (this.#entries.size >= MAX_VISIBLE_THUMBNAILS) {
        listener({ kind: 'unavailable', code: 'resource_limit' });
        return () => {};
      }
      entry = { request, listeners: new Set(), view: { kind: 'loading' }, queued: true };
      this.#entries.set(key, entry);
    }
    const owned = entry;
    entry.listeners.add(listener);
    listener(entry.view);
    void this.#pump();
    return () => {
      owned.listeners.delete(listener);
      if (owned.listeners.size === 0 && this.#entries.get(key) === owned) {
        this.#entries.delete(key);
        if (owned.view.kind === 'ready') URL.revokeObjectURL(owned.view.url);
      }
    };
  }

  /** 清除、换批、重载/断连释放前端资源；不把停止等待当作后端停止计算。 */
  reset(): void {
    ++this.#generation;
    for (const entry of this.#entries.values()) {
      if (entry.view.kind === 'ready') URL.revokeObjectURL(entry.view.url);
      entry.listeners.forEach((listener) => listener({ kind: 'unavailable', code: 'stale_task' }));
    }
    this.#entries.clear();
  }

  async reveal(identity: AssetIdentity, target: RevealTarget): Promise<void> {
    const job = requestFor(this.#session(), identity);
    const generation = this.#generation;
    if (this.#revealing) throw fail('busy');
    if (target !== 'result' && target !== 'backup') throw fail('unavailable');
    this.#revealing = true;
    try {
      const result = await invoke<unknown>('reveal_task_file', { request: { job, target } });
      if (this.#session() !== job.subscriptionId) throw fail('session_unavailable');
      if (generation !== this.#generation) throw fail('stale_task');
      if (result !== 'requested') throw fail('service_fault');
    } finally {
      this.#revealing = false;
    }
  }

  #live(key: string, entry: Entry): boolean {
    return this.#entries.get(key) === entry && entry.request.subscriptionId === this.#session();
  }

  async #pump(): Promise<void> {
    if (this.#running) return;
    this.#running = true;
    try {
      for (;;) {
        const next = [...this.#entries].find(([, entry]) => entry.queued);
        if (!next) break;
        const [key, entry] = next;
        entry.queued = false;
        if (!this.#live(key, entry)) continue;
        try {
          // 页面重载时旧解码可能仍在途；仅对无副作用读取的Busy最多退避两次。
          let result: unknown;
          for (let retry = 0; ; retry++) {
            try {
              result = await invoke<unknown>('get_task_thumbnail', { request: entry.request });
              break;
            } catch (error) {
              if (assetErrorCode(error) !== 'busy' || retry >= 2 || !this.#live(key, entry))
                throw error;
              await new Promise<void>((resolve) => setTimeout(resolve, 150 * (retry + 1)));
              if (!this.#live(key, entry)) throw fail('stale_task');
            }
          }
          if (!this.#live(key, entry)) continue;
          const image = thumbnail(result);
          const url = URL.createObjectURL(
            new Blob([Uint8Array.from(image.png)], { type: 'image/png' }),
          );
          entry.view = { kind: 'ready', url, width: image.width, height: image.height };
        } catch (error) {
          if (!this.#live(key, entry)) continue;
          entry.view = { kind: 'unavailable', code: assetErrorCode(error) };
        }
        entry.listeners.forEach((listener) => listener(entry.view));
      }
    } finally {
      this.#running = false;
    }
  }
}
