import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { TASK_PROTOCOL_VERSION, type TaskMutation, type TaskSettingsDto } from './tasks.generated';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(),
  Channel: class {
    onmessage: (value: unknown) => void = () => {};
  },
}));

let Subscription: typeof import('./task-subscription').TaskSnapshotSubscription;
let Actions: typeof import('./task-actions').TaskActions;
let session = 0;
const snapshot = {
  protocolVersion: TASK_PROTOCOL_VERSION,
  revision: '0',
  selectionId: null,
  phase: 'idle',
  scan: null,
  error: null,
  batch: null,
  page: { kind: 'jobs', offset: 0, total: 0, items: [] },
};
function reply(command: string): unknown {
  switch (command) {
    case 'subscribe_task_changes':
      return {
        protocolVersion: TASK_PROTOCOL_VERSION,
        subscriptionId: String(++session),
        revision: '0',
      };
    case 'get_task_snapshot':
      return snapshot;
    case 'acknowledge_task_changes':
      return null;
    case 'unsubscribe_task_changes':
      return true;
    case 'select_native_import':
      return { grantId: '9007199254740993', rootCount: 1 };
    case 'apply_task_mutation':
      return { selectionId: '9007199254740994' };
    default:
      throw new Error('Unexpected command ' + command);
  }
}
function deferred<T>() {
  let resolve: (value: T) => void = () => {
    throw new Error('not initialized');
  };
  let reject: (reason: unknown) => void = () => {
    throw new Error('not initialized');
  };
  const promise = new Promise<T>((accept, fail) => {
    resolve = accept;
    reject = fail;
  });
  return { promise, resolve, reject };
}
beforeEach(async () => {
  vi.resetModules();
  ({ TaskSnapshotSubscription: Subscription } = await import('./task-subscription'));
  ({ TaskActions: Actions } = await import('./task-actions'));
  session = 0;
  vi.mocked(isTauri).mockReturnValue(true);
  vi.mocked(invoke)
    .mockReset()
    .mockImplementation(async (command) => reply(command));
});
async function connected() {
  const stream = new Subscription();
  await stream.connect();
  return { stream, actions: new Actions(stream) };
}

describe('controlled native task actions', () => {
  it('requires native environment and completed initial snapshot/ACK', async () => {
    const stream = new Subscription();
    const actions = new Actions(stream);
    vi.mocked(isTauri).mockReturnValue(false);
    await expect(actions.selectAndImport('files')).rejects.toThrow('browser');
    expect(invoke).not.toHaveBeenCalled();
    vi.mocked(isTauri).mockReturnValue(true);
    await expect(actions.selectAndImport('files')).rejects.toThrow('acknowledge');
    const ack = deferred<null>();
    const reached = deferred<void>();
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === 'acknowledge_task_changes') {
        reached.resolve();
        return ack.promise;
      }
      return reply(command);
    });
    const connecting = stream.connect();
    await reached.promise;
    expect(stream.mutationSession).toBeNull();
    await expect(actions.apply({ kind: 'clear', selectionId: '1' })).rejects.toThrow('acknowledge');
    ack.resolve(null);
    await connecting;
    expect(stream.mutationSession).toBe('1');
    await stream.disconnect();
    expect(stream.mutationSession).toBeNull();
  });

  it('selects then imports fixed settings without paths or fake task progress', async () => {
    const { stream, actions } = await connected();
    const selected = deferred<unknown>();
    vi.mocked(invoke).mockImplementation(async (command) =>
      command === 'select_native_import' ? selected.promise : reply(command),
    );
    const draft: TaskSettingsDto = { mode: { kind: 'lossy', quality: 80 }, output: 'copy_beside' };
    const importing = actions.selectAndImport('folder', draft);
    draft.mode = { kind: 'lossless' };
    draft.output = 'overwrite';
    expect(actions.busy).toBe(true);
    await expect(actions.select('files')).rejects.toThrow('pending');
    selected.resolve({ grantId: '9007199254740993', rootCount: 1 });
    expect(await importing).toEqual({ selectionId: '9007199254740994' });
    expect(invoke).toHaveBeenCalledWith('select_native_import', {
      request: { subscriptionId: '1', kind: 'folder' },
    });
    expect(invoke).toHaveBeenLastCalledWith('apply_task_mutation', {
      request: {
        subscriptionId: '1',
        operation: {
          kind: 'import',
          grantId: '9007199254740993',
          settings: { mode: { kind: 'lossy', quality: 80 }, output: 'copy_beside' },
        },
      },
    });
    expect(stream.current).toEqual(snapshot);
    expect(actions.busy).toBe(false);
    await stream.disconnect();
  });

  it('uses independent default lossy80/overwrite drafts and supports scan-only', async () => {
    const { defaultTaskSettings } = await import('./task-actions');
    const first = defaultTaskSettings();
    first.output = 'copy_beside';
    expect(defaultTaskSettings()).toEqual({
      mode: { kind: 'lossy', quality: 80 },
      output: 'overwrite',
    });
    const { stream, actions } = await connected();
    await actions.selectAndImport('files');
    expect(invoke).toHaveBeenLastCalledWith('apply_task_mutation', {
      request: {
        subscriptionId: '1',
        operation: { kind: 'import', grantId: '9007199254740993', settings: defaultTaskSettings() },
      },
    });
    await actions.selectAndImport('files', null);
    expect(invoke).toHaveBeenLastCalledWith('apply_task_mutation', {
      request: {
        subscriptionId: '1',
        operation: { kind: 'import', grantId: '9007199254740993', settings: null },
      },
    });
    await stream.disconnect();
  });

  it('cancelled native selection does not issue a mutation', async () => {
    const { stream, actions } = await connected();
    vi.mocked(invoke).mockImplementation(async (command) =>
      command === 'select_native_import' ? null : reply(command),
    );
    expect(await actions.selectAndImport('files')).toBeNull();
    expect(
      vi.mocked(invoke).mock.calls.some(([command]) => command === 'apply_task_mutation'),
    ).toBe(false);
    await stream.disconnect();
  });

  it('rejects a late native result after disconnect/reconnect instead of auto-importing', async () => {
    const { stream, actions } = await connected();
    const selected = deferred<unknown>();
    vi.mocked(invoke).mockImplementation(async (command) =>
      command === 'select_native_import' ? selected.promise : reply(command),
    );
    const importing = actions.selectAndImport('files');
    const rejected = expect(importing).rejects.toThrow('connection changed');
    await stream.disconnect();
    await stream.connect();
    expect(stream.mutationSession).toBe('2');
    selected.resolve({ grantId: '2', rootCount: 1 });
    await rejected;
    expect(
      vi.mocked(invoke).mock.calls.some(([command]) => command === 'apply_task_mutation'),
    ).toBe(false);
    await stream.disconnect();
  });

  it('rejects invalid grant responses without writing or automatic retry', async () => {
    const { stream, actions } = await connected();
    for (const bad of [
      undefined,
      {},
      { grantId: 1, rootCount: 1 },
      { grantId: '0', rootCount: 1 },
      { grantId: '01', rootCount: 1 },
      { grantId: '1', rootCount: 0 },
      { grantId: '1', rootCount: 1001 },
    ]) {
      vi.mocked(invoke).mockImplementation(async (command) =>
        command === 'select_native_import' ? bad : reply(command),
      );
      await expect(actions.selectAndImport('files')).rejects.toThrow();
      expect(actions.busy).toBe(false);
    }
    expect(
      vi.mocked(invoke).mock.calls.some(([command]) => command === 'apply_task_mutation'),
    ).toBe(false);
    await stream.disconnect();
  });

  it('preserves operation errors and does not retry uncertain acceptance', async () => {
    const { stream, actions } = await connected();
    const error = { code: 'task', error: { code: 'stale_batch' } };
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === 'apply_task_mutation') throw error;
      return reply(command);
    });
    await expect(actions.apply({ kind: 'clear', selectionId: '1' })).rejects.toEqual(error);
    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === 'apply_task_mutation'),
    ).toHaveLength(1);
    expect(actions.busy).toBe(false);
    expect(stream.current).toEqual(snapshot);
    await stream.disconnect();
  });

  it('sends exact identities for start/clear/retry and rejects invalid row sets locally', async () => {
    const { stream, actions } = await connected();
    const selectionId = '9007199254740994';
    const operations: TaskMutation[] = [
      {
        kind: 'start',
        selectionId,
        settings: { mode: { kind: 'lossless' }, output: 'copy_beside' },
      },
      { kind: 'clear', selectionId },
      {
        kind: 'confirm_content_credentials',
        selectionId,
        expectedBatchRevision: '9007199254740993',
        jobIds: [7],
        mode: { kind: 'lossless' },
        output: 'copy_beside',
        consent: 'remove_content_credentials',
      },
      {
        kind: 'retry',
        selectionId,
        expectedBatchRevision: '9007199254740993',
        jobIds: [1, 5],
        mode: { kind: 'lossy', quality: 80 },
      },
    ];
    for (const operation of operations) {
      expect(await actions.apply(operation)).toEqual({ selectionId });
      expect(invoke).toHaveBeenLastCalledWith('apply_task_mutation', {
        request: { subscriptionId: '1', operation },
      });
    }
    const before = vi.mocked(invoke).mock.calls.length;
    for (const jobIds of [[], [0, 0], [-1], [1.5], Array.from({ length: 1001 }, (_, i) => i)]) {
      await expect(
        actions.apply({
          kind: 'retry',
          selectionId,
          expectedBatchRevision: '3',
          jobIds,
          mode: { kind: 'lossless' },
        }),
      ).rejects.toThrow('Invalid retry');
    }
    expect(vi.mocked(invoke).mock.calls).toHaveLength(before);
    await stream.disconnect();
  });

  it('does not treat malformed or mismatched acceptance as success', async () => {
    const { stream, actions } = await connected();
    for (const value of [
      null,
      {},
      { selectionId: 2 },
      { selectionId: '0' },
      { selectionId: '3' },
    ]) {
      vi.mocked(invoke).mockImplementation(async (command) =>
        command === 'apply_task_mutation' ? value : reply(command),
      );
      await expect(actions.apply({ kind: 'clear', selectionId: '2' })).rejects.toThrow();
    }
    await stream.disconnect();
  });
});
