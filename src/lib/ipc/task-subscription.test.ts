import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import type { TaskChangeNotice, TaskSnapshotDto } from './tasks.generated';

const bridge = vi.hoisted(() => ({
  channels: [] as { onmessage: (value: unknown) => void }[],
  revision: '0',
  id: 0,
}));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(),
  Channel: class {
    onmessage: (value: unknown) => void = () => {};
    constructor() {
      bridge.channels.push(this);
    }
  },
}));

function deferred<T>() {
  let resolve: (value: T) => void = () => {
    throw new Error('not initialized');
  };
  let reject: (error: unknown) => void = () => {
    throw new Error('not initialized');
  };
  const promise = new Promise<T>((accept, fail) => {
    resolve = accept;
    reject = fail;
  });
  return { promise, resolve, reject };
}
function ticket(id = '1', revision = bridge.revision): TaskChangeNotice {
  return { protocolVersion: 1, subscriptionId: id, revision };
}
function snapshot(revision = bridge.revision): TaskSnapshotDto {
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
function defaultReply(command: string): unknown {
  switch (command) {
    case 'subscribe_task_changes':
      return ticket(String(++bridge.id));
    case 'get_task_snapshot':
      return snapshot();
    case 'acknowledge_task_changes':
      return null;
    case 'unsubscribe_task_changes':
      return true;
    default:
      throw new Error('Unexpected command: ' + command);
  }
}
function send(value: unknown, index = bridge.channels.length - 1) {
  const channel = bridge.channels[index];
  if (!channel) throw new Error('No channel');
  channel.onmessage(value);
}

let Subscription: typeof import('./task-subscription').TaskSnapshotSubscription;
beforeEach(async () => {
  // 模拟新WebView模块生命周期，连首次归属未知失败的页面槽也只能这样释放。
  vi.resetModules();
  ({ TaskSnapshotSubscription: Subscription } = await import('./task-subscription'));
  bridge.channels.length = 0;
  bridge.id = 0;
  bridge.revision = '0';
  vi.mocked(isTauri).mockReturnValue(true);
  vi.mocked(invoke)
    .mockReset()
    .mockImplementation(async (command) => defaultReply(command));
});

describe('bounded task snapshot subscription', () => {
  it('does not allocate a Channel or invent state in browser preview', async () => {
    vi.mocked(isTauri).mockReturnValue(false);
    const stream = new Subscription();
    expect(await stream.connect()).toBeNull();
    expect(stream.state).toBe('unavailable');
    expect(stream.current).toBeNull();
    await stream.disconnect();
    expect(invoke).not.toHaveBeenCalled();
    expect(bridge.channels).toHaveLength(0);
  });

  it('subscribes, reads a bounded page and acknowledges the exact u64 ticket', async () => {
    bridge.revision = '9007199254740993';
    const applied = vi.fn();
    const stream = new Subscription(applied, undefined, 'jobs', 7);
    const connecting = stream.connect();
    expect(stream.connect()).toBe(connecting);
    expect(await connecting).toEqual(snapshot());
    expect(stream.state).toBe('connected');
    expect(invoke).toHaveBeenNthCalledWith(2, 'get_task_snapshot', {
      request: { collection: 'jobs', limit: 7, offset: 0, expectedRevision: null },
    });
    expect(invoke).toHaveBeenNthCalledWith(3, 'acknowledge_task_changes', {
      request: { subscriptionId: '1', revision: '9007199254740993' },
    });
    expect(applied).toHaveBeenCalledOnce();
    await expect(new Subscription().connect()).rejects.toThrow('owns this page');
    expect(bridge.channels).toHaveLength(1);
    await stream.disconnect();
    await stream.disconnect();
    expect(stream.current).toEqual(snapshot());
    expect(stream.state).toBe('idle');
  });

  it('waits for the bootstrap snapshot before ACK and handles a notice arriving before its ACK response', async () => {
    const firstRead = deferred<TaskSnapshotDto>();
    const queried = deferred<void>();
    const updated = deferred<TaskSnapshotDto>();
    let reads = 0;
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === 'get_task_snapshot') {
        if (++reads === 1) {
          queried.resolve();
          return firstRead.promise;
        }
        return snapshot('3');
      }
      if (command === 'acknowledge_task_changes' && reads === 1) send(ticket('1', '2'));
      return defaultReply(command);
    });
    const stream = new Subscription((value) => {
      if (value.revision === '3') updated.resolve(value);
    });
    const starting = stream.connect();
    await queried.promise;
    expect(invoke).not.toHaveBeenCalledWith('acknowledge_task_changes', expect.anything());
    firstRead.resolve(snapshot('1'));
    await starting;
    await updated.promise;
    await stream.disconnect();
    expect(reads).toBe(2);
    expect(stream.current?.revision).toBe('3');
  });

  it('serializes reads while ACK is pending and ignores duplicate/foreign/old notifications', async () => {
    const ackPending = deferred<null>();
    const ackEntered = deferred<void>();
    const applied = deferred<void>();
    const stream = new Subscription((value) => {
      if (value.revision === '5') applied.resolve();
    });
    await stream.connect();
    let reads = 0;
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === 'get_task_snapshot') return snapshot(++reads === 1 ? '3' : '5');
      if (command === 'acknowledge_task_changes' && reads === 1) {
        ackEntered.resolve();
        return ackPending.promise;
      }
      return defaultReply(command);
    });
    send(ticket('999', '100'));
    send(ticket('1', '0'));
    send(ticket('1', '2'));
    await ackEntered.promise;
    send(ticket('1', '2'));
    send(ticket('1', '4'));
    send(ticket('1', '4'));
    expect(reads).toBe(1);
    ackPending.resolve(null);
    await applied.promise;
    expect((await stream.connect())?.revision).toBe('5');
    await stream.disconnect();
    expect(reads).toBe(2);
    expect(stream.current?.revision).toBe('5');
  });

  it('disconnects during registration without querying, then allows a new observer', async () => {
    const registration = deferred<TaskChangeNotice>();
    vi.mocked(invoke).mockImplementation(async (command) =>
      command === 'subscribe_task_changes' ? registration.promise : defaultReply(command),
    );
    const stream = new Subscription();
    const connecting = stream.connect();
    const closing = stream.disconnect();
    expect(stream.disconnect()).toBe(closing);
    registration.resolve(ticket());
    await closing;
    expect(await connecting).toBeNull();
    expect(invoke).not.toHaveBeenCalledWith('get_task_snapshot', expect.anything());
    expect(invoke).not.toHaveBeenCalledWith('acknowledge_task_changes', expect.anything());
    vi.mocked(invoke).mockImplementation(async (command) => defaultReply(command));
    const remounted = new Subscription();
    await remounted.connect();
    await remounted.disconnect();
  });

  it('drops late query results and old channel callbacks after disconnect/reconnect', async () => {
    const applied = vi.fn();
    const stream = new Subscription(applied);
    await stream.connect();
    const query = deferred<TaskSnapshotDto>();
    const queried = deferred<void>();
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === 'get_task_snapshot') {
        queried.resolve();
        return query.promise;
      }
      return defaultReply(command);
    });
    const oldCallback = bridge.channels[0]?.onmessage;
    send(ticket('1', '1'));
    await queried.promise;
    await stream.disconnect();
    bridge.revision = '10';
    vi.mocked(invoke).mockImplementation(async (command) => defaultReply(command));
    await stream.connect();
    query.resolve(snapshot('100'));
    oldCallback?.(ticket('1', '101'));
    await stream.disconnect();
    expect(stream.current?.revision).toBe('10');
    expect(applied.mock.calls.map(([value]) => value.revision)).toEqual(['0', '10']);
  });

  it('retains the last good snapshot on query failure and restores terminal state on reconnect', async () => {
    const failed = deferred<unknown>();
    const stream = new Subscription(undefined, failed.resolve);
    await stream.connect();
    const error = new Error('read failed');
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === 'get_task_snapshot') throw error;
      return defaultReply(command);
    });
    send(ticket('1', '1'));
    expect(await failed.promise).toBe(error);
    await stream.disconnect();
    expect(stream.state).toBe('failed');
    expect(stream.current?.revision).toBe('0');
    bridge.revision = '20';
    vi.mocked(invoke).mockImplementation(async (command) =>
      command === 'get_task_snapshot'
        ? { ...snapshot(), phase: 'finished' }
        : defaultReply(command),
    );
    await stream.connect();
    expect(stream.current?.phase).toBe('finished');
    expect(stream.current?.revision).toBe('20');
    expect(stream.error).toBeNull();
    await stream.disconnect();
  });

  it('reports ACK failure without discarding an already applied snapshot or retrying automatically', async () => {
    const error = { code: 'stale_subscription' };
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === 'acknowledge_task_changes') throw error;
      return defaultReply(command);
    });
    const stream = new Subscription();
    await expect(stream.connect()).rejects.toBe(error);
    expect(stream.current).toEqual(snapshot());
    expect(stream.state).toBe('failed');
    expect(invoke).toHaveBeenLastCalledWith('unsubscribe_task_changes', {
      request: { subscriptionId: '1' },
    });
    expect(bridge.channels).toHaveLength(1);
  });

  it('retains ownership on cleanup failure, and permits retrying the same disconnect', async () => {
    const stream = new Subscription();
    await stream.connect();
    vi.mocked(invoke).mockRejectedValueOnce(new Error('transport unavailable'));
    await expect(stream.disconnect()).rejects.toThrow('transport unavailable');
    expect(stream.state).toBe('failed');
    await expect(new Subscription().connect()).rejects.toThrow('owns this page');
    await expect(stream.connect()).rejects.toThrow('Disconnect or reload');
    expect(bridge.channels).toHaveLength(1);
    await stream.disconnect();
    const next = new Subscription();
    await next.connect();
    await next.disconnect();
  });

  it('bounds callbacks after uncertain registration failure and explicitly requires a page reload', async () => {
    const error = new Error('invoke disconnected before reply');
    vi.mocked(invoke).mockRejectedValue(error);
    const stream = new Subscription();
    await expect(stream.connect()).rejects.toBe(error);
    expect(stream.state).toBe('reload_required');
    await expect(stream.disconnect()).rejects.toThrow('identity is unknown');
    await expect(stream.connect()).rejects.toThrow('Disconnect or reload');
    await expect(new Subscription().connect()).rejects.toThrow('owns this page');
    expect(bridge.channels).toHaveLength(1);
    expect(stream.current).toBeNull();
  });

  it.each([
    { protocolVersion: 2, subscriptionId: '1', revision: '0' },
    { protocolVersion: 1, subscriptionId: '1', revision: '01' },
  ])('rejects malformed bootstrap envelopes but releases a known session: %o', async (bad) => {
    vi.mocked(invoke).mockImplementation(async (command) =>
      command === 'subscribe_task_changes' ? bad : defaultReply(command),
    );
    const stream = new Subscription();
    await expect(stream.connect()).rejects.toThrow();
    expect(stream.state).toBe('failed');
    expect(stream.current).toBeNull();
    expect(invoke).toHaveBeenLastCalledWith('unsubscribe_task_changes', {
      request: { subscriptionId: '1' },
    });
  });

  it('fails safely on malformed notifications without throwing into the SDK callback', async () => {
    const failed = deferred<unknown>();
    const stream = new Subscription(undefined, failed.resolve);
    await stream.connect();
    expect(() => send({ ...ticket(), revision: 1 })).not.toThrow();
    await failed.promise;
    await stream.disconnect();
    expect(stream.current?.revision).toBe('0');
    expect(stream.state).toBe('failed');
  });

  it('rejects snapshots older than their notice instead of ACKing away the final update', async () => {
    const failed = deferred<unknown>();
    const stream = new Subscription(undefined, failed.resolve);
    await stream.connect();
    send(ticket('1', '9007199254740993'));
    await failed.promise;
    await stream.disconnect();
    expect(stream.current?.revision).toBe('0');
    expect(invoke).not.toHaveBeenCalledWith('acknowledge_task_changes', {
      request: { subscriptionId: '1', revision: '9007199254740993' },
    });
  });

  it('bounds pending notifications and silences late failures after cancellation', async () => {
    const read = deferred<TaskSnapshotDto>();
    const entered = deferred<void>();
    const failed = deferred<unknown>();
    const stream = new Subscription(undefined, failed.resolve);
    await stream.connect();
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === 'get_task_snapshot') {
        entered.resolve();
        return read.promise;
      }
      return defaultReply(command);
    });
    send(ticket('1', '1'));
    await entered.promise;
    send(ticket('1', '2'));
    send(ticket('1', '3'));
    expect(await failed.promise).toEqual(
      new Error('Task notification exceeded the in-flight bound'),
    );
    await stream.disconnect();
    read.reject(new Error('late query failure'));
    expect(stream.current?.revision).toBe('0');
  });

  it('contains observer exceptions and validates page settings before allocating resources', async () => {
    expect(() => new Subscription(undefined, undefined, 'jobs', 101)).toThrow(
      'Invalid subscription page',
    );
    expect(bridge.channels).toHaveLength(0);
    const stream = new Subscription(
      () => {
        throw new Error('render failed');
      },
      () => {
        throw new Error('error observer failed');
      },
    );
    await expect(stream.connect()).rejects.toThrow('render failed');
    expect(stream.error).toBeInstanceOf(AggregateError);
    expect(stream.current).toEqual(snapshot());
    expect(stream.state).toBe('failed');
  });

  it('rejects malformed ACK responses and does not hide failed cleanup', async () => {
    const stream = new Subscription();
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === 'acknowledge_task_changes') return true;
      if (command === 'unsubscribe_task_changes') return null;
      return defaultReply(command);
    });
    await expect(stream.connect()).rejects.toThrow('Invalid acknowledgement response');
    expect(stream.state).toBe('failed');
    expect(stream.error).toBeInstanceOf(AggregateError);
    await expect(new Subscription().connect()).rejects.toThrow('owns this page');
    vi.mocked(invoke).mockImplementation(async (command) => defaultReply(command));
    await stream.disconnect();
  });
});
