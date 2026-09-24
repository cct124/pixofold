import { TaskActions } from '../../lib/ipc/task-actions';
import {
  TaskSnapshotSubscription,
  type TaskConnectionState,
} from '../../lib/ipc/task-subscription';
import { getTaskSnapshot, parseDecimalU64 } from '../../lib/ipc/tasks';
import { MAX_RETRY_JOBS, MAX_TASK_PAGE_SIZE } from '../../lib/ipc/tasks.generated';
import type {
  ConfirmationDto,
  CredentialsOutput,
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
  confirmationOpen: boolean;
  confirmationSettings: TaskSettingsDto | null;
  confirmationRows: ConfirmationDto[] | null;
  confirmationRevision: string | null;
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
  confirmationOpen: false,
  confirmationSettings: null,
  confirmationRows: null,
  confirmationRevision: null,
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
  #confirmationBaseline: { selection: string; revision: string } | null = null;
  #offeredConfirmation = '';

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
        this.#offerConfirmation();
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
    this.#confirmationBaseline = null;
    this.#publish({
      connection: 'disconnecting',
      pageLoading: false,
      confirmationOpen: false,
      confirmationSettings: null,
      confirmationRows: null,
      confirmationRevision: null,
      ...(this.#view.confirmationOpen ? { collection: 'jobs' } : {}),
    });
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
    const revision = this.#view.snapshot?.revision ?? '0';
    await this.#perform(async () => {
      const accepted = await this.#actions.selectAndImport(kind, this.#settings);
      if (accepted) {
        this.#autoSelection = accepted.selectionId;
        this.#attempt = { selection: accepted.selectionId, version, revision: '' };
        this.#confirmationBaseline = { selection: accepted.selectionId, revision };
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
    if (
      operation.kind === 'start' ||
      operation.kind === 'retry' ||
      operation.kind === 'confirm_content_credentials'
    ) {
      this.#confirmationBaseline = {
        selection: operation.selectionId,
        revision: this.#view.snapshot?.revision ?? '0',
      };
    }
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
      this.#offerConfirmation();
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
    if (snapshot.selectionId && ['scanning', 'preparing', 'running'].includes(snapshot.phase)) {
      this.#confirmationBaseline = { selection: snapshot.selectionId, revision: snapshot.revision };
    }
    const collection =
      changed || (this.#view.confirmationOpen && snapshot.phase !== 'finished')
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
      ...(changed || snapshot.phase !== 'finished'
        ? {
            confirmationOpen: false,
            confirmationRows: null,
            confirmationRevision: null,
            confirmationSettings: null,
          }
        : {}),
    });
    this.#wantPage();
    this.#offerConfirmation();
    queueMicrotask(() => this.#maybeStart());
  }

  #offerConfirmation(): void {
    const { snapshot } = this.#view;
    const baseline = this.#confirmationBaseline;
    if (
      !this.canChange ||
      !snapshot?.batch ||
      snapshot.phase !== 'finished' ||
      !baseline ||
      baseline.selection !== snapshot.selectionId ||
      parseDecimalU64(snapshot.revision) <= parseDecimalU64(baseline.revision)
    )
      return;
    this.#confirmationBaseline = null;
    const key = `${snapshot.selectionId}/${snapshot.batch.revision}`;
    if (snapshot.batch.confirmationCount > 0 && key !== this.#offeredConfirmation) {
      this.#offeredConfirmation = key;
      this.openConfirmations();
    }
  }

  openConfirmations(): void {
    const snapshot = this.#view.snapshot;
    if (!this.canChange || snapshot?.phase !== 'finished' || !snapshot.batch?.confirmationCount)
      return;
    this.#publish({
      confirmationOpen: true,
      confirmationSettings: structuredClone(this.#settings),
      confirmationRows: null,
      confirmationRevision: null,
    });
    this.setPage('confirmations');
  }

  closeConfirmations(): void {
    if (!this.#view.confirmationOpen) return;
    this.#publish({
      confirmationOpen: false,
      confirmationSettings: null,
      confirmationRows: null,
      confirmationRevision: null,
    });
    this.setPage('jobs');
  }

  /** 确认按钮授权完整同revision列表中的所选行；路径及源版本仍由Rust保管/复验。 */
  async confirmContentCredentials(
    revision: string,
    jobIds: number[],
    output: CredentialsOutput,
  ): Promise<void> {
    const { snapshot, confirmationRows, confirmationRevision, confirmationSettings } = this.#view;
    if (
      !this.canChange ||
      !this.#view.confirmationOpen ||
      !confirmationSettings ||
      !confirmationRows ||
      this.#view.pageLoading ||
      snapshot?.phase !== 'finished' ||
      !snapshot.selectionId ||
      !snapshot.batch ||
      confirmationRevision !== snapshot.revision ||
      revision !== snapshot.revision ||
      confirmationRows.length !== snapshot.batch.confirmationCount ||
      (confirmationSettings.output === 'copy_beside'
        ? output !== 'copy_beside'
        : !['overwrite_with_backup', 'overwrite_without_backup'].includes(output)) ||
      !jobIds.length ||
      new Set(jobIds).size !== jobIds.length
    )
      return;
    const ids = new Set(confirmationRows.map((row) => row.id));
    if (jobIds.some((id) => !ids.has(id))) return;
    const operation: TaskMutation = {
      kind: 'confirm_content_credentials',
      selectionId: snapshot.selectionId,
      expectedBatchRevision: snapshot.batch.revision,
      jobIds,
      mode: confirmationSettings.mode,
      output,
      consent: 'remove_content_credentials',
    };
    this.closeConfirmations();
    await this.#mutate(operation);
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
    if (key === this.#pageKey && (this.#view.page || this.#view.confirmationRows)) return;
    this.#pageKey = key;
    if (
      collection !== 'confirmations' &&
      snapshot.page.kind === collection &&
      snapshot.page.offset === offset
    ) {
      this.#publish({ page: snapshot, pageLoading: false });
      return;
    }
    this.#publish({
      page: null,
      pageLoading: true,
      confirmationRows: null,
      confirmationRevision: null,
    });
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
          if (collection === 'confirmations') {
            const rows = await this.#readConfirmations(snapshot, key, epoch);
            if (rows === null || epoch !== this.#lifecycle || key !== this.#pageKey) continue;
            this.#publish({
              confirmationRows: rows,
              confirmationRevision: snapshot.revision,
              pageLoading: false,
            });
            continue;
          }
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
                collection: collection === 'confirmations' ? 'jobs' : collection,
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

  /** UI不分页；后台依然顺序分段读取。全部同版本、总数/身份一致才一次发布，最多1000行。 */
  async #readConfirmations(
    snapshot: TaskSnapshotDto,
    key: string,
    epoch: number,
  ): Promise<ConfirmationDto[] | null> {
    const total = snapshot.batch?.confirmationCount;
    if (snapshot.phase !== 'finished' || total === undefined || total > MAX_RETRY_JOBS)
      throw new Error('Invalid confirmation list');
    const rows: ConfirmationDto[] = [];
    const ids = new Set<number>();
    while (rows.length < total) {
      const result = await getTaskSnapshot({
        collection: 'confirmations',
        offset: rows.length,
        limit: MAX_TASK_PAGE_SIZE,
        expectedRevision: snapshot.revision,
      });
      if (epoch !== this.#lifecycle || key !== this.#pageKey) return null;
      if (
        !result ||
        result.page.kind !== 'confirmations' ||
        result.page.total !== total ||
        result.selectionId !== snapshot.selectionId ||
        result.batch?.revision !== snapshot.batch?.revision ||
        result.batch?.confirmationCount !== total ||
        result.phase !== 'finished'
      )
        throw new Error('Inconsistent confirmation list');
      for (const row of result.page.items) {
        if (ids.has(row.id)) throw new Error('Duplicate confirmation identity');
        ids.add(row.id);
        rows.push(row);
      }
    }
    return rows;
  }
}

let workspace: WorkspaceController | null = null;
export function getWorkspace(): WorkspaceController {
  workspace ??= new WorkspaceController();
  return workspace;
}
