import { invoke, isTauri } from '@tauri-apps/api/core';
import { parseDecimalU64 } from './tasks';
import { MAX_NATIVE_IMPORT_ROOTS, MAX_RETRY_JOBS } from './tasks.generated';
import type { TaskSnapshotSubscription } from './task-subscription';
import type {
  NativeImportGrant,
  NativeSelectionKind,
  TaskMutation,
  TaskMutationAccepted,
  TaskSettingsDto,
} from './tasks.generated';

/** 应用默认参数；返回独立草稿，不改变正在运行的任务。 */
export function defaultTaskSettings(): TaskSettingsDto {
  return { mode: { kind: 'lossy', quality: 80 }, output: 'overwrite' };
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}
function positiveId(value: unknown): string {
  if (typeof value !== 'string' || parseDecimalU64(value) === 0n)
    throw new Error('Invalid task operation identity');
  return value;
}
function grant(value: unknown): NativeImportGrant | null {
  if (value === null) return null;
  if (
    !record(value) ||
    typeof value.rootCount !== 'number' ||
    !Number.isInteger(value.rootCount) ||
    value.rootCount < 1 ||
    value.rootCount > MAX_NATIVE_IMPORT_ROOTS
  )
    throw new Error('Invalid native selection response');
  return { grantId: positiveId(value.grantId), rootCount: value.rootCount };
}

/**
 * 一个应用级操作适配器，复用已经connect的唯一任务观察器，不创建Channel或TaskRuntime。
 * 操作最多一个在途；响应只是接纳票据，进度/成功/失败始终由订阅快照提供。
 * 不自动重试写操作。传输失败可能已接纳，调用方应先恢复权威快照再让用户决定下一步。
 * 不暴露文件路径；同目录副本/覆盖由Rust规划，自选目录和拖放尚未接入。
 */
export class TaskActions {
  #busy = false;
  readonly #subscription: TaskSnapshotSubscription;

  constructor(subscription: TaskSnapshotSubscription) {
    this.#subscription = subscription;
  }

  get busy(): boolean {
    return this.#busy;
  }

  /** 仅返回单次授权（5分钟有效）；取消选择返回null。选择本身不启动处理。 */
  select(kind: NativeSelectionKind): Promise<NativeImportGrant | null> {
    return this.#run((session) => this.#select(session, kind));
  }

  /** 点击时固定设置；等待原生选择期间改变草稿不影响本次。null只扫描不自动启动。 */
  selectAndImport(
    kind: NativeSelectionKind,
    settings: TaskSettingsDto | null = defaultTaskSettings(),
  ): Promise<TaskMutationAccepted | null> {
    const fixed = structuredClone(settings);
    return this.#run(async (session) => {
      const selected = await this.#select(session, kind);
      if (selected === null) return null;
      // #select已经核对会话，#mutate再在投递前核对；旧页面选择不自动启动新任务。
      return this.#mutate(session, { kind: 'import', grantId: selected.grantId, settings: fixed });
    });
  }

  /** 显式操作携带当前所见selection/批次revision；Rust拒绝旧ID、重复消费及越界重试。 */
  apply(operation: TaskMutation): Promise<TaskMutationAccepted> {
    const fixed = structuredClone(operation);
    return this.#run((session) => this.#mutate(session, fixed));
  }

  async #run<T>(action: (session: string) => Promise<T>): Promise<T> {
    if (!isTauri()) throw new Error('Native task operations are unavailable in browser preview');
    const session = this.#subscription.mutationSession;
    if (!session) throw new Error('Connect and acknowledge a task snapshot before changing tasks');
    if (this.#busy) throw new Error('A task operation is already pending');
    this.#busy = true;
    try {
      return await action(session);
    } finally {
      this.#busy = false;
    }
  }

  #sameSession(session: string): void {
    if (this.#subscription.mutationSession !== session)
      throw new Error('Task connection changed; recover the current snapshot before continuing');
  }

  async #select(session: string, kind: NativeSelectionKind): Promise<NativeImportGrant | null> {
    if (kind !== 'files' && kind !== 'folder') throw new Error('Invalid native selection kind');
    this.#sameSession(session);
    const result = await invoke<unknown>('select_native_import', {
      request: { subscriptionId: session, kind },
    });
    this.#sameSession(session);
    return grant(result);
  }

  async #mutate(session: string, operation: TaskMutation): Promise<TaskMutationAccepted> {
    if (operation.kind === 'import') positiveId(operation.grantId);
    else positiveId(operation.selectionId);
    if (operation.kind === 'retry' || operation.kind === 'confirm_content_credentials') {
      parseDecimalU64(operation.expectedBatchRevision);
      if (
        operation.jobIds.length < 1 ||
        operation.jobIds.length > MAX_RETRY_JOBS ||
        new Set(operation.jobIds).size !== operation.jobIds.length ||
        operation.jobIds.some((id) => !Number.isInteger(id) || id < 1 || id > 0xffffffff)
      )
        throw new Error('Invalid retry selection');
    }
    if (
      operation.kind === 'confirm_content_credentials' &&
      (operation.consent !== 'remove_content_credentials' ||
        !['copy_beside', 'overwrite_with_backup', 'overwrite_without_backup'].includes(
          operation.output,
        ))
    )
      throw new Error('Explicit content credentials consent is required');
    this.#sameSession(session);
    const result = await invoke<unknown>('apply_task_mutation', {
      request: { subscriptionId: session, operation },
    });
    this.#sameSession(session);
    if (!record(result)) throw new Error('Invalid task acceptance response');
    const selectionId = positiveId(result.selectionId);
    if (operation.kind !== 'import' && selectionId !== operation.selectionId)
      throw new Error('Mismatched task acceptance response');
    return { selectionId };
  }
}
