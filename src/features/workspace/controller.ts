import { TaskActions } from '../../lib/ipc/task-actions';
import {
  TaskSnapshotSubscription,
  type TaskConnectionState,
} from '../../lib/ipc/task-subscription';
import { getTaskSnapshot, parseDecimalU64 } from '../../lib/ipc/tasks';
import type {
  NativeSelectionKind,
  TaskCollection,
  TaskMutation,
  TaskSettingsDto,
  TaskSnapshotDto,
} from '../../lib/ipc/tasks.generated';

export const WORKSPACE_PAGE_SIZE = 50;
export interface WorkspaceView {
  connection: TaskConnectionState;
  snapshot: TaskSnapshotDto | null;
  page: TaskSnapshotDto | null;
  collection: TaskCollection;
  offset: number;
  pageLoading: boolean;
  pending: boolean;
  error: 'connection' | 'operation' | 'page' | null;
  needsRecovery: boolean;
}
const initialView: WorkspaceView = {
  connection: 'idle',
  snapshot: null,
  page: null,
  collection: 'jobs',
  offset: 0,
  pageLoading: false,
  pending: false,
  error: null,
  needsRecovery: false,
};

/**
 * 页面唯一工作台会话：最多一个观察器、一个额外分页查询和一个写操作。
 * subscribe的微任务延迟清理兼容StrictMode，不用固定sleep；重挂载只恢复显示，不重跑任务。
 * 操作失败可能已经接纳，阻止继续写入直到显式恢复快照；不会自动重发不确定请求。
 */
export class WorkspaceController {
  #view: WorkspaceView = initialView;
  #listeners = new Set<() => void>();
  #stream = new TaskSnapshotSubscription(
    (snapshot) => this.#observe(snapshot),
    () => this.#publish({ error: 'connection', needsRecovery: true }),
    'jobs',
    WORKSPACE_PAGE_SIZE,
    (connection) => this.#publish({ connection }),
  );
  #actions = new TaskActions(this.#stream);
  #opening: Promise<void> | null = null;
  #closing: Promise<void> | null = null;
  #settings: TaskSettingsDto | null = null;
  #settingsVersion = 0;
  #autoSelection: string | null = null;
  #attempt: { selection: string; version: number; revision: string } | null = null;
  #pageKey = '';
  #reading = false;
  #lifecycle = 0;

  getSnapshot = (): WorkspaceView => this.#view;
  subscribe = (listener: () => void): (() => void) => {
    this.#listeners.add(listener);
    if (this.#listeners.size === 1) void this.#attach();
    return () => {
      this.#listeners.delete(listener);
      queueMicrotask(() => {
        if (this.#listeners.size === 0) void this.#disconnect();
      });
    };
  };

  async #attach(): Promise<void> {
    // 重挂载可能发生在异步释放中，先等旧连接/握手收尾再分配新会话。
    await this.#closing;
    await this.#opening;
    if (this.#listeners.size && !this.#view.needsRecovery) await this.connect();
  }

  #publish(change: Partial<WorkspaceView>): void {
    this.#view = { ...this.#view, ...change };
    this.#listeners.forEach((listener) => listener());
  }

  async connect(): Promise<void> {
    if (this.#opening) return this.#opening;
    this.#opening = (async () => {
      if (this.#closing) await this.#closing;
      this.#publish({ connection: 'connecting' });
      try {
        await this.#stream.connect();
        this.#publish({ connection: this.#stream.state });
        this.#maybeStart();
      } catch {
        this.#publish({ connection: this.#stream.state, error: 'connection', needsRecovery: true });
      }
    })().finally(() => {
      this.#opening = null;
    });
    return this.#opening;
  }

  async #disconnect(): Promise<void> {
    if (this.#closing) return this.#closing;
    ++this.#lifecycle;
    this.#pageKey = '';
    this.#autoSelection = null;
    this.#publish({ connection: 'disconnecting', pageLoading: false });
    this.#closing = this.#stream
      .disconnect()
      .catch(() => {
        this.#publish({ error: 'connection', needsRecovery: true });
      })
      .finally(() => {
        this.#publish({ connection: this.#stream.state });
        this.#closing = null;
      });
    return this.#closing;
  }

  /** 恢复前先销毁旧连接，不自动重发失败操作；首次会话未知时仍须重载页面。 */
  async reconnect(): Promise<void> {
    if (this.#view.pending || this.#opening) return;
    await this.#disconnect();
    if (this.#stream.state === 'reload_required') return;
    this.#publish({ error: null, needsRecovery: false, offset: 0 });
    await this.connect();
  }

  setSettings(settings: TaskSettingsDto | null, userChange = true): void {
    if (JSON.stringify(settings) === JSON.stringify(this.#settings)) return;
    this.#settings = structuredClone(settings);
    ++this.#settingsVersion;
    if (userChange && this.#view.snapshot?.phase === 'ready')
      this.#autoSelection = this.#view.snapshot.selectionId;
    this.#maybeStart();
  }

  async import(kind: NativeSelectionKind): Promise<void> {
    if (!this.canChange || !this.canImport) return;
    const version = this.#settingsVersion;
    await this.#perform(async () => {
      const accepted = await this.#actions.selectAndImport(kind, this.#settings);
      if (accepted) {
        this.#autoSelection = accepted.selectionId;
        this.#attempt = { selection: accepted.selectionId, version, revision: '' };
      }
    });
    this.#maybeStart();
  }

  get canChange(): boolean {
    return (
      this.#stream.mutationSession !== null && !this.#view.pending && !this.#view.needsRecovery
    );
  }
  get canImport(): boolean {
    const phase = this.#view.snapshot?.phase;
    return phase !== undefined && ['idle', 'finished', 'cancelled', 'rejected'].includes(phase);
  }

  async clear(): Promise<void> {
    const selectionId = this.#view.snapshot?.selectionId;
    if (selectionId && this.canChange) {
      this.#autoSelection = null;
      await this.#mutate({ kind: 'clear', selectionId });
    }
  }
  async retryPage(): Promise<void> {
    const { snapshot, page } = this.#view;
    if (
      !this.canChange ||
      !this.#settings ||
      !snapshot?.selectionId ||
      !snapshot.batch ||
      snapshot.phase !== 'finished' ||
      page?.revision !== snapshot.revision ||
      page.page.kind !== 'jobs'
    )
      return;
    const jobIds = page.page.items
      .filter((job) => ['failed', 'cancelled'].includes(job.state.kind))
      .map((job) => job.id);
    if (!jobIds.length) return;
    await this.#mutate({
      kind: 'retry',
      selectionId: snapshot.selectionId,
      expectedBatchRevision: snapshot.batch.revision,
      jobIds,
      mode: this.#settings.mode,
    });
  }

  async #mutate(operation: TaskMutation): Promise<void> {
    await this.#perform(async () => {
      await this.#actions.apply(operation);
    });
  }
  async #perform(action: () => Promise<void>): Promise<void> {
    this.#publish({ pending: true, error: null });
    try {
      await action();
    } catch {
      this.#publish({ error: 'operation', needsRecovery: true });
    } finally {
      this.#publish({ pending: false, connection: this.#stream.state });
      // 后台Ready通知可早于接纳响应；响应期间修正的草稿也要在此重新检查。
      queueMicrotask(() => this.#maybeStart());
    }
  }

  #maybeStart(): void {
    const snapshot = this.#view.snapshot;
    if (
      !this.canChange ||
      !this.#settings ||
      snapshot?.phase !== 'ready' ||
      !snapshot.selectionId ||
      snapshot.selectionId !== this.#autoSelection
    )
      return;
    if (
      this.#attempt?.selection === snapshot.selectionId &&
      (this.#attempt.version === this.#settingsVersion ||
        this.#attempt.revision === snapshot.revision)
    )
      return;
    this.#attempt = {
      selection: snapshot.selectionId,
      version: this.#settingsVersion,
      revision: snapshot.revision,
    };
    void this.#mutate({
      kind: 'start',
      selectionId: snapshot.selectionId,
      settings: this.#settings,
    });
  }

  #observe(snapshot: TaskSnapshotDto): void {
    const previous = this.#view.snapshot;
    if (previous && parseDecimalU64(snapshot.revision) < parseDecimalU64(previous.revision)) return;
    const changed = snapshot.selectionId !== previous?.selectionId;
    const collection = changed
      ? snapshot.batch
        ? 'jobs'
        : 'candidates'
      : snapshot.batch && !previous?.batch
        ? 'jobs'
        : this.#view.collection;
    this.#publish({
      snapshot,
      collection,
      offset: snapshot.revision !== previous?.revision ? 0 : this.#view.offset,
    });
    this.#wantPage();
    queueMicrotask(() => this.#maybeStart());
  }

  setPage(collection: TaskCollection, offset = 0): void {
    if (this.#view.connection !== 'connected') return;
    if (!Number.isInteger(offset) || offset < 0 || offset > 0xffffffff) return;
    this.#publish({
      collection,
      offset,
      error: this.#view.error === 'page' ? null : this.#view.error,
    });
    this.#wantPage();
  }
  #wantPage(): void {
    const { snapshot, collection, offset } = this.#view;
    if (!snapshot) return;
    const key = JSON.stringify([collection, offset, snapshot.revision]);
    if (key === this.#pageKey && this.#view.page) return;
    this.#pageKey = key;
    if (snapshot.page.kind === collection && snapshot.page.offset === offset) {
      this.#publish({ page: snapshot, pageLoading: false });
      return;
    }
    this.#publish({ page: null, pageLoading: true });
    void this.#readPage();
  }
  async #readPage(): Promise<void> {
    if (this.#reading) return;
    this.#reading = true;
    try {
      while (this.#pageKey && this.#view.pageLoading) {
        const key = this.#pageKey;
        const epoch = this.#lifecycle;
        const { snapshot, collection, offset } = this.#view;
        if (!snapshot) break;
        try {
          const page = await getTaskSnapshot({
            collection,
            offset,
            limit: WORKSPACE_PAGE_SIZE,
            expectedRevision: snapshot.revision,
          });
          if (epoch !== this.#lifecycle || key !== this.#pageKey) continue;
          if (!page) throw new Error('Desktop snapshot unavailable');
          this.#publish({ page, pageLoading: false });
        } catch (error) {
          if (epoch !== this.#lifecycle || key !== this.#pageKey) continue;
          if (
            typeof error === 'object' &&
            error !== null &&
            'code' in error &&
            error.code === 'stale_snapshot'
          ) {
            // 只读恢复首屏；最新摘要/页面来自同一响应，不拼接过期页，不重发写操作。
            try {
              const latest = await getTaskSnapshot({
                collection,
                offset: 0,
                limit: WORKSPACE_PAGE_SIZE,
                expectedRevision: null,
              });
              if (epoch !== this.#lifecycle || key !== this.#pageKey) continue;
              if (!latest) throw new Error('Desktop snapshot unavailable');
              this.#observe(latest);
            } catch {
              if (epoch === this.#lifecycle && key === this.#pageKey)
                this.#publish({ pageLoading: false, error: 'page' });
            }
          } else this.#publish({ pageLoading: false, error: 'page' });
        }
      }
    } finally {
      this.#reading = false;
    }
  }
}

let workspace: WorkspaceController | null = null;
export function getWorkspace(): WorkspaceController {
  workspace ??= new WorkspaceController();
  return workspace;
}
