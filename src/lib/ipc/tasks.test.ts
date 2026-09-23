import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import type { TaskPageRequest, TaskSnapshotDto } from './tasks.generated';
import { getTaskSnapshot, parseDecimalU64, TaskSnapshotReader } from './tasks';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }));

function request(): TaskPageRequest {
  return { expectedRevision: null, collection: 'jobs', offset: 0, limit: 100 };
}
function deferred<T>() {
  let resolve: (value: T) => void = () => {
    throw new Error('Promise not initialized');
  };
  let reject: (reason: unknown) => void = () => {
    throw new Error('Promise not initialized');
  };
  const promise = new Promise<T>((accept, fail) => {
    resolve = accept;
    reject = fail;
  });
  return { promise, resolve, reject };
}
function snapshot(revision = '0'): TaskSnapshotDto {
  return {
    protocolVersion: 1,
    revision,
    selectionId: null,
    phase: 'idle',
    scan: null,
    error: null,
    batch: null,
    page: { kind: 'jobs', offset: 0, total: 0, items: [] },
  };
}

describe('read-only task IPC', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauri).mockReturnValue(true);
  });

  it('preserves u64 precision and rejects ambiguous decimal values', () => {
    expect(parseDecimalU64('9007199254740993')).toBe(9007199254740993n);
    expect(parseDecimalU64('18446744073709551615')).toBe(18446744073709551615n);
    for (const value of [
      '',
      '01',
      '+1',
      '-1',
      ' 1',
      '1\n',
      '1\r\n',
      '1\u2028',
      '1.0',
      '1e3',
      '１',
      '18446744073709551616',
    ]) {
      expect(() => parseDecimalU64(value)).toThrow();
    }
  });

  it('never simulates tasks or invokes Rust in browser preview', async () => {
    vi.mocked(isTauri).mockReturnValue(false);
    await expect(getTaskSnapshot(request())).resolves.toBeNull();
    await expect(new TaskSnapshotReader().read(request())).resolves.toEqual({
      kind: 'unavailable',
    });
    expect(invoke).not.toHaveBeenCalled();
  });

  it('calls only the read-only command and freezes request data', async () => {
    const reply = deferred<TaskSnapshotDto>();
    vi.mocked(invoke).mockReturnValue(reply.promise);
    const input = request();
    const pending = getTaskSnapshot(input);
    input.offset = 99;
    reply.resolve(snapshot());
    await expect(pending).resolves.toEqual(snapshot());
    expect(invoke).toHaveBeenCalledExactlyOnceWith('get_task_snapshot', { request: request() });
  });

  it('rejects invalid pages before IPC and preserves stable Rust/transport failures', async () => {
    for (const overrides of [
      { limit: 0 },
      { limit: 101 },
      { offset: -1 },
      { offset: 1 },
      { offset: 0.5 },
      { offset: 2 ** 32 },
    ]) {
      await expect(getTaskSnapshot({ ...request(), ...overrides })).rejects.toEqual({
        code: 'invalid_page',
      });
    }
    expect(invoke).not.toHaveBeenCalled();
    for (const error of [
      { code: 'stale_snapshot', currentRevision: '9007199254740993' },
      new Error('transport closed'),
    ]) {
      vi.mocked(invoke).mockRejectedValueOnce(error);
      await expect(getTaskSnapshot(request())).rejects.toBe(error);
    }
  });

  it('validates protocol, revision and requested page envelope', async () => {
    const invalid: TaskSnapshotDto[] = [
      { ...snapshot(), protocolVersion: 2 },
      { ...snapshot(), revision: '01' },
      { ...snapshot(), page: { kind: 'issues', offset: 0, total: 0, items: [] } },
      { ...snapshot(), page: { kind: 'jobs', offset: 1, total: 1, items: [] } },
      { ...snapshot(), page: { kind: 'jobs', offset: 0, total: 1, items: [] } },
    ];
    for (const response of invalid) {
      vi.mocked(invoke).mockResolvedValueOnce(response);
      await expect(getTaskSnapshot(request())).rejects.toThrow();
    }
    vi.mocked(invoke).mockResolvedValueOnce(snapshot('2'));
    await expect(getTaskSnapshot({ ...request(), expectedRevision: '1' })).rejects.toThrow(
      'envelope',
    );
  });

  it('does not let late responses from an old query replace the current page', async () => {
    const old = deferred<TaskSnapshotDto>();
    vi.mocked(invoke)
      .mockReturnValueOnce(old.promise)
      .mockResolvedValueOnce(snapshot('9007199254740993'));
    const reader = new TaskSnapshotReader();
    const first = reader.read(request());
    await expect(reader.read(request())).resolves.toMatchObject({ kind: 'applied' });
    old.resolve(snapshot('9007199254740992'));
    await expect(first).resolves.toEqual({ kind: 'superseded' });
    expect(reader.current?.revision).toBe('9007199254740993');
    vi.mocked(invoke).mockResolvedValueOnce(snapshot('9007199254740992'));
    await expect(reader.read(request())).resolves.toEqual({ kind: 'superseded' });
  });

  it('can replace a page at the same revision without merging stale rows', async () => {
    const reader = new TaskSnapshotReader();
    vi.mocked(invoke).mockResolvedValueOnce(snapshot('8'));
    await reader.read(request());
    const response: TaskSnapshotDto = {
      ...snapshot('8'),
      page: { kind: 'issues', offset: 0, total: 0, items: [] },
    };
    vi.mocked(invoke).mockResolvedValueOnce(response);
    await expect(
      reader.read({ ...request(), expectedRevision: '8', collection: 'issues' }),
    ).resolves.toMatchObject({ kind: 'applied' });
    expect(reader.current?.page.kind).toBe('issues');
  });

  it('retains the last good snapshot after errors and recovers from a new first page', async () => {
    const error = { code: 'stale_snapshot', currentRevision: '3' };
    const reader = new TaskSnapshotReader();
    vi.mocked(invoke)
      .mockResolvedValueOnce(snapshot('1'))
      .mockRejectedValueOnce(error)
      .mockResolvedValueOnce(snapshot('3'));
    await reader.read(request());
    await expect(reader.read({ ...request(), expectedRevision: '1' })).rejects.toBe(error);
    expect(reader.current?.revision).toBe('1');
    await reader.read(request());
    expect(reader.current?.revision).toBe('3');
  });

  it('suppresses stale failures and releases responses on dispose without cancelling Rust', async () => {
    const old = deferred<TaskSnapshotDto>();
    vi.mocked(invoke).mockReturnValueOnce(old.promise).mockResolvedValueOnce(snapshot('5'));
    const reader = new TaskSnapshotReader();
    const first = reader.read(request());
    await reader.read(request());
    old.reject(new Error('late error'));
    await expect(first).resolves.toEqual({ kind: 'superseded' });
    const late = deferred<TaskSnapshotDto>();
    vi.mocked(invoke).mockReturnValueOnce(late.promise);
    const pending = reader.read(request());
    reader.dispose();
    late.resolve(snapshot('6'));
    await expect(pending).resolves.toEqual({ kind: 'superseded' });
    await expect(reader.read(request())).resolves.toEqual({ kind: 'superseded' });
    expect(reader.current).toBeNull();
    expect(vi.mocked(invoke).mock.calls.every(([command]) => command === 'get_task_snapshot')).toBe(
      true,
    );
    vi.mocked(invoke).mockResolvedValueOnce(snapshot('0'));
    await expect(new TaskSnapshotReader().read(request())).resolves.toMatchObject({
      kind: 'applied',
    });
  });
});
