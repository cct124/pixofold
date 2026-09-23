import { Channel, invoke, isTauri } from '@tauri-apps/api/core';
import { getTaskSnapshot, parseDecimalU64 } from './tasks';
import type { TaskChangeNotice, TaskCollection, TaskSnapshotDto } from './tasks.generated';
import { MAX_TASK_PAGE_SIZE, TASK_PROTOCOL_VERSION } from './tasks.generated';

export type TaskConnectionState =
  | 'idle'
  | 'unavailable'
  | 'connecting'
  | 'connected'
  | 'disconnecting'
  | 'failed'
  | 'reload_required';

// 页面只有一个Channel所有者；禁止重挂载/反复失败创建无界回调。
let liveOwner: object | null = null;

interface Connection {
  owner: object;
  channel: Channel<unknown>;
  registration: Promise<unknown>;
  opening: Promise<TaskSnapshotDto | null> | null;
  closing: Promise<void> | null;
  ticket: TaskChangeNotice | null;
  pending: TaskChangeNotice | null;
  lastNotice: bigint | null;
  stopped: boolean;
  ready: boolean;
  processing: boolean;
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}
function subscriptionId(value: unknown): string {
  if (!record(value) || typeof value.subscriptionId !== 'string')
    throw new Error('Invalid task subscription identity');
  if (parseDecimalU64(value.subscriptionId) === 0n)
    throw new Error('Invalid task subscription identity');
  return value.subscriptionId;
}
function notice(value: unknown): TaskChangeNotice {
  const id = subscriptionId(value);
  if (
    !record(value) ||
    value.protocolVersion !== TASK_PROTOCOL_VERSION ||
    typeof value.revision !== 'string'
  )
    throw new Error('Invalid task notification envelope');
  parseDecimalU64(value.revision);
  return { protocolVersion: TASK_PROTOCOL_VERSION, subscriptionId: id, revision: value.revision };
}

/**
 * 应用级单页只读观察器：先订阅、读取最新第一页，再确认票据；最多一个查询和一个待处理通知。
 * 不轮询、不重跑任务、不累积全量行。其他页继续用TaskSnapshotReader保持同revision。
 * disconnect立即屏蔽迟到结果，等原生订阅释放，不取消Rust任务；connect可恢复最新终态。
 * 首次注册响应失败时无法可靠释放未知会话，状态为reload_required，须重载WebView；
 * 已知会话清理失败则可重试disconnect。禁止使用SDK私有回调接口或无界自动重试。
 */
export class TaskSnapshotSubscription {
  #connection: Connection | null = null;
  #state: TaskConnectionState = 'idle';
  #current: TaskSnapshotDto | null = null;
  #error: unknown = null;
  readonly #collection: TaskCollection;
  readonly #limit: number;
  readonly #onSnapshot: (snapshot: TaskSnapshotDto) => void;
  readonly #onError: (error: unknown) => void;

  constructor(
    onSnapshot: (snapshot: TaskSnapshotDto) => void = () => {},
    onError: (error: unknown) => void = () => {},
    collection: TaskCollection = 'jobs',
    limit = MAX_TASK_PAGE_SIZE,
  ) {
    if (
      !['jobs', 'candidates', 'issues'].includes(collection) ||
      !Number.isInteger(limit) ||
      limit < 1 ||
      limit > MAX_TASK_PAGE_SIZE
    )
      throw new Error('Invalid subscription page');
    this.#collection = collection;
    this.#limit = limit;
    this.#onSnapshot = onSnapshot;
    this.#onError = onError;
  }

  get state(): TaskConnectionState {
    return this.#state;
  }
  get current(): TaskSnapshotDto | null {
    return this.#current;
  }
  get error(): unknown {
    return this.#error;
  }

  /** 重复connect复用当前握手；不允许在未完成disconnect时抢占其他观察器。 */
  connect(): Promise<TaskSnapshotDto | null> {
    const existing = this.#connection;
    if (existing) {
      if (existing.stopped)
        return Promise.reject(new Error('Disconnect or reload before reconnecting'));
      if (existing.ready) return Promise.resolve(this.#current);
      return existing.opening ?? Promise.resolve(this.#current);
    }
    if (this.#state === 'reload_required') return Promise.reject(new Error('Reload required'));
    if (!isTauri()) {
      this.#state = 'unavailable';
      return Promise.resolve(null);
    }
    if (liveOwner) return Promise.reject(new Error('Another task subscription owns this page'));
    const owner = {};
    liveOwner = owner;
    let channel: Channel<unknown>;
    try {
      channel = new Channel<unknown>();
    } catch (error) {
      // 构造未完成时也不能证明回调未注册；保守保留页面槽，重载才释放。
      this.#state = 'reload_required';
      this.#report(error);
      return Promise.reject(error);
    }
    const connection: Connection = {
      owner,
      channel,
      // 下一微任务才调用invoke，确保处理器已就绪，不丢失早到消息。
      registration: Promise.resolve().then(() =>
        invoke<unknown>('subscribe_task_changes', { onChange: channel }),
      ),
      opening: null,
      closing: null,
      ticket: null,
      pending: null,
      lastNotice: null,
      stopped: false,
      ready: false,
      processing: false,
    };
    this.#connection = connection;
    this.#state = 'connecting';
    this.#error = null;
    channel.onmessage = (value) => this.#receive(connection, value);
    connection.opening = this.#open(connection);
    return connection.opening;
  }

  async #open(connection: Connection): Promise<TaskSnapshotDto | null> {
    try {
      connection.ticket = notice(await connection.registration);
      if (connection.stopped) return null;
      await this.#refresh(connection, connection.ticket);
      if (connection.stopped) return null;
      connection.ready = true;
      this.#state = 'connected';
      this.#drain(connection);
      return this.#current;
    } catch (error) {
      if (connection.stopped) return null;
      this.#report(error);
      try {
        await this.disconnect();
      } catch (cleanupError) {
        this.#report(
          new AggregateError([error, cleanupError], 'Task subscription setup/cleanup failed'),
        );
      }
      throw error;
    }
  }

  #receive(connection: Connection, value: unknown): void {
    if (connection.stopped) return;
    try {
      const next = notice(value);
      if (connection.ticket && next.subscriptionId !== connection.ticket.subscriptionId) return;
      if (connection.lastNotice !== null && parseDecimalU64(next.revision) <= connection.lastNotice)
        return;
      if (connection.pending) {
        if (
          connection.pending.subscriptionId === next.subscriptionId &&
          connection.pending.revision === next.revision
        )
          return;
        throw new Error('Task notification exceeded the in-flight bound');
      }
      connection.pending = next;
      this.#drain(connection);
    } catch (error) {
      this.#fail(connection, error);
    }
  }

  #drain(connection: Connection): void {
    if (!connection.ready || connection.processing || connection.stopped) return;
    connection.processing = true;
    void (async () => {
      while (connection.pending && !connection.stopped) {
        const next = connection.pending;
        connection.pending = null;
        if (next.subscriptionId !== connection.ticket?.subscriptionId) continue;
        if (
          connection.lastNotice !== null &&
          parseDecimalU64(next.revision) <= connection.lastNotice
        )
          continue;
        await this.#refresh(connection, next);
      }
    })()
      .catch((error: unknown) => this.#fail(connection, error))
      .finally(() => {
        connection.processing = false;
        if (connection.pending) this.#drain(connection);
      });
  }

  async #refresh(connection: Connection, next: TaskChangeNotice): Promise<void> {
    connection.lastNotice = parseDecimalU64(next.revision);
    const snapshot = await getTaskSnapshot({
      collection: this.#collection,
      limit: this.#limit,
      offset: 0,
      expectedRevision: null,
    });
    if (connection.stopped) return;
    if (
      snapshot === null ||
      parseDecimalU64(snapshot.revision) < connection.lastNotice ||
      (this.#current !== null &&
        parseDecimalU64(snapshot.revision) < parseDecimalU64(this.#current.revision))
    )
      throw new Error('Snapshot is older than the notification or current view');
    this.#current = snapshot;
    this.#onSnapshot(snapshot);
    if (connection.stopped) return;
    const confirmed = await invoke<unknown>('acknowledge_task_changes', {
      request: { subscriptionId: next.subscriptionId, revision: next.revision },
    });
    if (confirmed !== null) throw new Error('Invalid acknowledgement response');
  }

  #report(error: unknown): void {
    this.#error = error;
    // 业务回调不能抛出到Channel，否则SDK的消息索引/end帧清理会被中断。
    try {
      this.#onError(error);
    } catch (observerError) {
      this.#error = new AggregateError([error, observerError], 'Task error observer failed');
    }
  }
  #fail(connection: Connection, error: unknown): void {
    if (connection.stopped) return;
    this.#report(error);
    void this.disconnect().catch((cleanupError: unknown) => {
      this.#report(new AggregateError([error, cleanupError], 'Task subscription cleanup failed'));
    });
  }

  /** 可重复调用；清理失败保留页面槽以供重试，不假装资源已释放。保留最后好快照。 */
  disconnect(): Promise<void> {
    const connection = this.#connection;
    if (!connection)
      return this.#state === 'reload_required'
        ? Promise.reject(new Error('Reload required'))
        : Promise.resolve();
    if (connection.closing) return connection.closing;
    connection.stopped = true;
    connection.pending = null;
    connection.channel.onmessage = () => {};
    this.#state = 'disconnecting';
    connection.closing = (async () => {
      let id: string;
      try {
        id = subscriptionId(await connection.registration);
      } catch (error) {
        this.#state = 'reload_required';
        throw new Error(
          'Subscription identity is unknown; reload the WebView to release its callback',
          { cause: error },
        );
      }
      try {
        const removed = await invoke<unknown>('unsubscribe_task_changes', {
          request: { subscriptionId: id },
        });
        if (typeof removed !== 'boolean') throw new Error('Invalid unsubscribe response');
      } catch (error) {
        this.#state = 'failed';
        this.#error = error;
        throw error;
      }
      if (this.#connection === connection) {
        this.#connection = null;
        if (liveOwner === connection.owner) liveOwner = null;
        this.#state = this.#error === null ? 'idle' : 'failed';
      }
    })().finally(() => {
      connection.closing = null;
    });
    return connection.closing;
  }
}
