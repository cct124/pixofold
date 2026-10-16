import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import {
  TASK_PROTOCOL_VERSION,
  type TaskPageRequest,
  type TaskSnapshotDto,
  type JobDto,
} from './tasks.generated';
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
    protocolVersion: TASK_PROTOCOL_VERSION,
    supportedFormats: ['png'],
    revision,
    selectionId: null,
    phase: 'idle',
    scan: null,
    error: null,
    batch: null,
    page: { kind: 'jobs', offset: 0, total: 0, items: [] },
  };
}

it('validates actual format capabilities and JPEG reports at the IPC boundary', async () => {
  vi.mocked(invoke).mockReset();
  vi.mocked(isTauri).mockReturnValue(true);
  const job: JobDto = {
    id: 1,
    attempt: 1,
    format: 'jpeg',
    sourceName: { text: 'sample.JPG', lossy: false, truncated: false, sanitized: false },
    mode: { kind: 'lossy', quality: 0 },
    inputBytes: '1000',
    state: {
      kind: 'no_gain',
      report: {
        inputBytes: '1000',
        outputBytes: '1000',
        elapsedMs: '4',
        outputName: null,
        backupName: null,
        contentCredentialsRemoved: false,
        processing: {
          format: 'jpeg',
          details: { kind: 'lossy', mappingVersion: 1, nativeQuality: 1 },
        },
      },
    },
  };
  const response: TaskSnapshotDto = {
    ...snapshot(),
    supportedFormats: ['png', 'jpeg'],
    page: { kind: 'jobs', offset: 0, total: 1, items: [job] },
  };
  vi.mocked(invoke).mockResolvedValue(response);
  await expect(getTaskSnapshot(request())).resolves.toEqual(response);
  for (const formats of [undefined, [], ['jpeg'], ['png', 'png'], ['png', 'gif'], ['png']]) {
    vi.mocked(invoke).mockResolvedValue({ ...response, supportedFormats: formats });
    await expect(getTaskSnapshot(request())).rejects.toThrow('image contract');
  }
  if (job.state.kind !== 'no_gain') throw new Error('fixture');
  const removed = { ...job.state.report, contentCredentialsRemoved: true };
  vi.mocked(invoke).mockResolvedValue({
    ...response,
    page: { ...response.page, items: [{ ...job, state: { kind: 'no_gain', report: removed } }] },
  });
  await expect(getTaskSnapshot(request())).rejects.toThrow('image contract');
  vi.mocked(invoke).mockResolvedValue({
    ...response,
    page: {
      ...response.page,
      items: [
        {
          ...job,
          attempt: 2,
          state: {
            kind: 'succeeded',
            report: { ...removed, outputBytes: '800', outputName: job.sourceName },
          },
        },
      ],
    },
  });
  await expect(getTaskSnapshot(request())).resolves.toBeDefined();
  for (const processing of [
    { format: 'png', details: { kind: 'lossless' } },
    { format: 'jpeg', details: { kind: 'lossy', mappingVersion: 1, nativeQuality: 0 } },
    { format: 'jpeg', details: { kind: 'lossy', mappingVersion: 1, measuredQuality: 80 } },
    {
      format: 'jpeg',
      details: {
        kind: 'lossless_fallback',
        mappingVersion: 1,
        nativeQuality: 1,
        reason: { kind: 'transparency_guard' },
      },
    },
  ]) {
    const row = { ...job, state: { ...job.state, report: { ...job.state.report, processing } } };
    vi.mocked(invoke).mockResolvedValue({ ...response, page: { ...response.page, items: [row] } });
    await expect(getTaskSnapshot(request())).rejects.toThrow('image contract');
  }
});

it('validates v14 APNG actual modes and rejects forged quality or fallback semantics', async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  const details = { kind: 'lossless', frames: 2, numPlays: 0, defaultParticipates: false } as const;
  const report = {
    inputBytes: '500',
    outputBytes: '500',
    elapsedMs: '2',
    outputName: null,
    backupName: null,
    contentCredentialsRemoved: false,
    processing: { format: 'apng', details },
  } as const;
  const job: JobDto = {
    id: 1,
    attempt: 1,
    format: 'apng',
    sourceName: { text: 'animation.PNG', lossy: false, truncated: false, sanitized: false },
    inputBytes: '500',
    mode: { kind: 'lossy', quality: 100 },
    state: { kind: 'no_gain', report },
  };
  const resolve = (row: unknown, formats = ['png', 'jpeg', 'gif', 'apng']) => {
    vi.mocked(invoke).mockResolvedValue({
      ...snapshot(),
      supportedFormats: formats,
      page: { kind: 'jobs', offset: 0, total: 1, items: [row] },
    });
    return getTaskSnapshot(request());
  };
  await expect(resolve(job)).resolves.toBeDefined();
  await expect(resolve({ ...job, mode: { kind: 'lossless' } })).resolves.toBeDefined();
  await expect(resolve({ ...job, mode: { kind: 'lossy', quality: 80 } })).rejects.toThrow(
    'image contract',
  );
  await expect(resolve(job, ['png'])).rejects.toThrow('image contract');
  for (const invalid of [
    { ...details, kind: 'lossy' },
    { ...details, frames: 0 },
    { ...details, frames: 257 },
    { ...details, numPlays: null },
    { ...details, numPlays: -1 },
    { ...details, numPlays: 0x100000000 },
    { ...details, defaultParticipates: 1 },
  ]) {
    await expect(
      resolve({
        ...job,
        state: {
          kind: 'no_gain',
          report: { ...report, processing: { format: 'apng', details: invalid } },
        },
      }),
    ).rejects.toThrow('image contract');
  }
  await expect(
    resolve({
      ...job,
      state: { kind: 'no_gain', report: { ...report, contentCredentialsRemoved: true } },
    }),
  ).rejects.toThrow('image contract');
  const measured = {
    paletteEntries: 32,
    minimumPixelQuality: 90,
    minimumRemappingQuality: 90,
    maxAlphaError: 4,
    worstFrameRmse: 3,
    worstTemporalRmse: 5,
  };
  const lossy = {
    ...details,
    kind: 'lossy',
    mappingVersion: 1,
    policyVersion: 2,
    quality: measured,
  };
  const row = (
    actual: unknown,
    mode: unknown = { kind: 'lossy', quality: 80 },
    kind = 'succeeded',
  ) => ({
    ...job,
    mode,
    state: {
      kind,
      report: { ...report, outputBytes: '200', processing: { format: 'apng', details: actual } },
    },
  });
  await expect(resolve(row(lossy))).resolves.toBeDefined();
  await expect(resolve(row(lossy, { kind: 'lossless' }))).rejects.toThrow('image contract');
  await expect(resolve(row(lossy, { kind: 'lossy', quality: 100 }))).rejects.toThrow(
    'image contract',
  );
  await expect(resolve(row(lossy, { kind: 'lossy', quality: 80 }, 'no_gain'))).rejects.toThrow(
    'image contract',
  );
  for (const invalid of [
    { ...lossy, mappingVersion: 2 },
    { ...lossy, policyVersion: 1 },
    { ...lossy, quality: null },
    ...[
      { paletteEntries: 257 },
      { minimumPixelQuality: 79 },
      { minimumRemappingQuality: 101 },
      { maxAlphaError: 9 },
      { worstFrameRmse: -1 },
      { worstTemporalRmse: 9 },
      { maxAlphaError: NaN },
      { worstFrameRmse: Infinity },
    ].map((change) => ({ ...lossy, quality: { ...measured, ...change } })),
  ])
    await expect(resolve(row(invalid))).rejects.toThrow('image contract');
  for (const kind of ['quality_below_target', 'insufficient_gain']) {
    const fallback = {
      ...details,
      kind: 'lossless_fallback',
      mappingVersion: 1,
      policyVersion: 2,
      reason: { kind },
    };
    await expect(resolve(row(fallback))).resolves.toBeDefined();
    await expect(
      resolve(row(fallback, { kind: 'lossy', quality: 80 }, 'no_gain')),
    ).resolves.toBeDefined();
    await expect(resolve(row({ ...fallback, quality: measured }))).rejects.toThrow(
      'image contract',
    );
    await expect(resolve(row({ ...fallback, reason: { kind: 'high_bit_depth' } }))).rejects.toThrow(
      'image contract',
    );
    await expect(resolve(row(fallback, { kind: 'lossy', quality: 100 }))).rejects.toThrow(
      'image contract',
    );
  }
  await expect(
    resolve({
      ...job,
      state: { kind: 'failed', failure: { code: 'unsupported_apng_mode', recovery: null } },
    }),
  ).resolves.toBeDefined();
});

it('validates v13 GIF actual paths, loops, mapping and credentials', async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  const report = {
    inputBytes: '500',
    outputBytes: '500',
    elapsedMs: '2',
    outputName: null,
    backupName: null,
    contentCredentialsRemoved: false,
    processing: { format: 'gif', details: { kind: 'lossless', frames: 3, loopCount: 0 } },
  } as const;
  const job: JobDto = {
    id: 1,
    attempt: 1,
    format: 'gif',
    mode: { kind: 'lossless' },
    inputBytes: '500',
    sourceName: { text: 'sample.GIF', lossy: false, truncated: false, sanitized: false },
    state: { kind: 'no_gain', report },
  };
  const response: TaskSnapshotDto = {
    ...snapshot(),
    supportedFormats: ['png', 'gif'],
    page: { kind: 'jobs', offset: 0, total: 1, items: [job] },
  };
  const resolve = async (
    row: unknown,
    formats: unknown = response.supportedFormats,
    version = TASK_PROTOCOL_VERSION,
  ) => {
    vi.mocked(invoke).mockResolvedValue({
      ...response,
      protocolVersion: version,
      supportedFormats: formats,
      page: { ...response.page, items: [row] },
    });
    return getTaskSnapshot(request());
  };
  for (const loopCount of [null, 0, 1, 65535]) {
    await expect(
      resolve({
        ...job,
        state: {
          kind: 'no_gain',
          report: {
            ...report,
            processing: { format: 'gif', details: { kind: 'lossless', frames: 3, loopCount } },
          },
        },
      }),
    ).resolves.toBeDefined();
  }
  await expect(resolve(job, ['png', 'jpeg', 'gif'])).resolves.toBeDefined();
  await expect(resolve(job, ['png'])).rejects.toThrow('image contract');
  await expect(resolve(job, ['png', 'gif', 'apng'])).resolves.toBeDefined();
  await expect(resolve(job, ['png', 'gif', 'webp'])).rejects.toThrow('image contract');
  await expect(resolve(job, ['png', 'gif'], TASK_PROTOCOL_VERSION - 1)).rejects.toThrow();
  for (const details of [
    { kind: 'lossy', frames: 3, loopCount: 0 },
    { kind: 'lossless', frames: 0, loopCount: 0 },
    { kind: 'lossless', frames: 1.5, loopCount: 0 },
    { kind: 'lossless', frames: 3, loopCount: -1 },
    { kind: 'lossless', frames: 3, loopCount: 65536 },
    { kind: 'lossless', frames: 3 },
  ]) {
    await expect(
      resolve({
        ...job,
        state: { kind: 'no_gain', report: { ...report, processing: { format: 'gif', details } } },
      }),
    ).rejects.toThrow('image contract');
  }
  await expect(
    resolve({
      ...job,
      state: { kind: 'no_gain', report: { ...report, contentCredentialsRemoved: true } },
    }),
  ).rejects.toThrow('image contract');
  for (const q of [0, 60, 80, 99, 100]) {
    await expect(resolve({ ...job, mode: { kind: 'lossy', quality: q } })).resolves.toBeDefined();
    if (q === 100) continue;
    const nativeLossiness = Math.min(20, Math.ceil((100 - q) / 2));
    for (const details of [
      { kind: 'lossy', frames: 3, loopCount: 0, mappingVersion: 1, nativeLossiness },
      ...['quality_below_target', 'insufficient_gain'].map((kind) => ({
        kind: 'lossless_fallback',
        frames: 3,
        loopCount: 0,
        mappingVersion: 1,
        nativeLossiness,
        reason: { kind },
      })),
    ]) {
      const state = {
        kind: 'no_gain',
        report: { ...report, processing: { format: 'gif', details } },
      };
      await expect(
        resolve({ ...job, mode: { kind: 'lossy', quality: q }, state }),
      ).resolves.toBeDefined();
      await expect(
        resolve({ ...job, mode: { kind: 'lossy', quality: 100 }, state }),
      ).rejects.toThrow('image contract');
      await expect(resolve({ ...job, state })).rejects.toThrow('image contract');
    }
  }
  for (const details of [
    { kind: 'lossy', frames: 3, loopCount: 0, mappingVersion: 2, nativeLossiness: 10 },
    { kind: 'lossy', frames: 3, loopCount: 0, mappingVersion: 1, nativeLossiness: 20 },
    {
      kind: 'lossless_fallback',
      frames: 3,
      loopCount: 0,
      mappingVersion: 1,
      nativeLossiness: 10,
      reason: { kind: 'color_profile' },
    },
  ])
    await expect(
      resolve({
        ...job,
        mode: { kind: 'lossy', quality: 80 },
        state: { kind: 'no_gain', report: { ...report, processing: { format: 'gif', details } } },
      }),
    ).rejects.toThrow('image contract');
  await expect(
    resolve({
      ...job,
      mode: { kind: 'lossy', quality: 80 },
      state: { kind: 'failed', failure: { code: 'unsupported_gif_mode', recovery: null } },
    }),
  ).resolves.toBeDefined();
});

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
      { ...snapshot(), protocolVersion: 1 },
      { ...snapshot(), protocolVersion: TASK_PROTOCOL_VERSION + 1 },
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
