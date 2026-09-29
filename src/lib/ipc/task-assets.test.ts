import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { TaskAssets, assetErrorCode, type AssetIdentity, type ThumbnailView } from './task-assets';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }));
const png = Array.from(
  atob(
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLttAAAAABJRU5ErkJggg==',
  ),
  (byte) => byte.charCodeAt(0),
);
const image = () => ({ width: 1, height: 1, png: [...png] });
const identity: AssetIdentity = {
  selectionId: '1',
  jobId: 1,
  attempt: 1,
  expectedState: 'succeeded',
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
}
let session: string | null;
let assets: TaskAssets;
const create = vi.fn(() => 'blob:preview');
const revoke = vi.fn();
beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(isTauri).mockReturnValue(true);
  create.mockClear();
  revoke.mockClear();
  vi.stubGlobal('URL', { createObjectURL: create, revokeObjectURL: revoke });
  session = '1';
  assets = new TaskAssets(() => session);
});
afterEach(() => {
  assets.reset();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

it('deduplicates visible observers and releases a bounded PNG Blob when the last leaves', async () => {
  vi.mocked(invoke).mockResolvedValue(image());
  const a = vi.fn(),
    b = vi.fn();
  const stopA = assets.watch(identity, a),
    stopB = assets.watch(identity, b);
  await vi.waitFor(() => expect(create).toHaveBeenCalledTimes(1));
  expect(invoke).toHaveBeenCalledExactlyOnceWith('get_task_thumbnail', {
    request: { subscriptionId: '1', ...identity },
  });
  expect(a).toHaveBeenLastCalledWith({ kind: 'ready', url: 'blob:preview', width: 1, height: 1 });
  expect(b).toHaveBeenLastCalledWith({ kind: 'ready', url: 'blob:preview', width: 1, height: 1 });
  stopA();
  expect(revoke).not.toHaveBeenCalled();
  stopB();
  expect(revoke).toHaveBeenCalledExactlyOnceWith('blob:preview');
});

it('serializes decoding and drops late results after clear without stalling the next page', async () => {
  const first = deferred<unknown>();
  vi.mocked(invoke).mockReturnValueOnce(first.promise).mockResolvedValue(image());
  const old = vi.fn<(view: ThumbnailView) => void>(),
    next = vi.fn();
  assets.watch(identity, old);
  assets.watch({ ...identity, jobId: 2 }, old);
  expect(invoke).toHaveBeenCalledTimes(1);
  assets.reset();
  assets.watch({ ...identity, selectionId: '2' }, next);
  expect(invoke).toHaveBeenCalledTimes(1);
  first.resolve(image());
  await vi.waitFor(() =>
    expect(next).toHaveBeenLastCalledWith(expect.objectContaining({ kind: 'ready' })),
  );
  expect(invoke).toHaveBeenCalledTimes(2);
  expect(create).toHaveBeenCalledTimes(1);
  expect(old.mock.calls.some(([value]: [ThumbnailView]) => value.kind === 'ready')).toBe(false);
});

it('bounds waiting visible entries and never issues offscreen queued work', async () => {
  const first = deferred<unknown>();
  vi.mocked(invoke).mockReturnValue(first.promise);
  const listeners = Array.from({ length: 80 }, () => vi.fn());
  const releases = listeners.map((listener, index) =>
    assets.watch({ ...identity, jobId: index + 1 }, listener),
  );
  expect(invoke).toHaveBeenCalledTimes(1);
  expect(listeners[64]).toHaveBeenLastCalledWith({ kind: 'unavailable', code: 'resource_limit' });
  releases.forEach((release) => release());
  first.resolve(image());
  await first.promise;
  await Promise.resolve();
  expect(create).not.toHaveBeenCalled();
  expect(invoke).toHaveBeenCalledTimes(1);
});

it('retries only transient Busy reads with a strict cap, never writes', async () => {
  vi.useFakeTimers();
  vi.mocked(invoke).mockRejectedValue({ code: 'busy' });
  const listener = vi.fn();
  assets.watch(identity, listener);
  await vi.advanceTimersByTimeAsync(1000);
  expect(invoke).toHaveBeenCalledTimes(3);
  expect(listener).toHaveBeenLastCalledWith({ kind: 'unavailable', code: 'busy' });
  expect(vi.mocked(invoke).mock.calls.every(([command]) => command === 'get_task_thumbnail')).toBe(
    true,
  );
});

it.each([
  null,
  {},
  { ...image(), width: 129 },
  { ...image(), height: 97 },
  { ...image(), width: 2 },
  { ...image(), png: Array(65537).fill(0) },
  { ...image(), png: [999, ...png.slice(1)] },
  { ...image(), png: [...png.slice(0, 12), 0, ...png.slice(13)] },
])('rejects malformed or oversized preview response %#', async (value) => {
  vi.mocked(invoke).mockResolvedValue(value);
  const listener = vi.fn();
  assets.watch(identity, listener);
  await vi.waitFor(() =>
    expect(listener).toHaveBeenLastCalledWith({ kind: 'unavailable', code: 'service_fault' }),
  );
  expect(create).not.toHaveBeenCalled();
});

it('does not read files from browser mode, before handshake, or with invalid identities', async () => {
  const listener = vi.fn();
  vi.mocked(isTauri).mockReturnValue(false);
  assets.watch(identity, listener);
  await expect(assets.reveal(identity, 'result')).rejects.toEqual({ code: 'session_unavailable' });
  vi.mocked(isTauri).mockReturnValue(true);
  session = null;
  assets.watch(identity, listener);
  session = '1';
  assets.watch({ ...identity, jobId: 0 }, listener);
  expect(invoke).not.toHaveBeenCalled();
});

it('reveals a task target without paths, suppresses double requests and checks session on return', async () => {
  const pending = deferred<unknown>();
  vi.mocked(invoke).mockReturnValueOnce(pending.promise);
  const first = assets.reveal(identity, 'backup');
  await expect(assets.reveal(identity, 'backup')).rejects.toEqual({ code: 'busy' });
  expect(invoke).toHaveBeenCalledExactlyOnceWith('reveal_task_file', {
    request: { job: { subscriptionId: '1', ...identity }, target: 'backup' },
  });
  session = '2';
  pending.resolve('requested');
  await expect(first).rejects.toEqual({ code: 'session_unavailable' });
  vi.mocked(invoke).mockResolvedValue('requested');
  await assets.reveal(identity, 'result');
  expect(invoke).toHaveBeenCalledTimes(2);
});

it('does not retry reveal failures and never shows raw native paths/errors', async () => {
  vi.mocked(invoke).mockRejectedValue({ code: 'file_missing', message: 'private/path' });
  await expect(assets.reveal(identity, 'result')).rejects.toMatchObject({ code: 'file_missing' });
  expect(invoke).toHaveBeenCalledTimes(1);
  expect(assetErrorCode(new Error('private/path'))).toBe('service_fault');
});
