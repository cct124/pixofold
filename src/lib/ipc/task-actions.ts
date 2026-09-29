import { invoke, isTauri } from '@tauri-apps/api/core';
import { parseDecimalU64 } from './tasks';
import { MAX_NATIVE_IMPORT_ROOTS, MAX_RETRY_JOBS } from './tasks.generated';
import type { TaskSnapshotSubscription } from './task-subscription';
import type {
  NativeImportGrant,
  NativeOutputDirectory,
  NativeSelectionKind,
  TaskMutation,
  TaskMutationAccepted,
  TaskSettingsDto,
} from './tasks.generated';

/** 应用默认参数；返回独立草稿，不改变正在运行的任务。 */
export function defaultTaskSettings(): TaskSettingsDto {
  return { mode: { kind: 'lossy', quality: 80 }, output: 'overwrite_without_backup' };
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

function outputDirectory(value: unknown): NativeOutputDirectory | null {
  if (value === null) return null;
  if (!record(value) || !record(value.name)) throw new Error('Invalid output directory');
  const name = value.name;
  if (
    typeof name.text !== 'string' ||
    [...name.text].length > 240 ||
    typeof name.truncated !== 'boolean' ||
    typeof name.lossy !== 'boolean' ||
    typeof name.sanitized !== 'boolean'
  )
    throw new Error('Invalid output label');
  return {
    directoryId: positiveId(value.directoryId),
    name: {
      text: name.text,
      truncated: name.truncated,
      lossy: name.lossy,
      sanitized: name.sanitized,
    },
  };
}

function validateDirectoryTarget(value: unknown): void {
  if (
    !record(value) ||
    Object.keys(value).length !== 1 ||
    !record(value.copy_to) ||
    Object.keys(value.copy_to).length !== 2 ||
    typeof value.copy_to.preserveStructure !== 'boolean'
  )
    throw new Error('Invalid output directory target');
  positiveId(value.copy_to.directoryId);
}

/**
 * 一个应用级操作适配器，复用已经connect的唯一任务观察器，不创建Channel或TaskRuntime。
 * 操作最多一个在途；响应只是接纳票据，进度/成功/失败始终由订阅快照提供。
 * 不自动重试写操作。传输失败可能已接纳，调用方应先恢复权威快照再让用户决定下一步。
 * 不暴露文件路径；输入授权单次消费，输出目录授权限当前会话复用，布局由Rust规划。
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

  /** 仅选择输出目录，不导入或写文件；取消不改变现有目录。 */
  selectOutputDirectory(): Promise<NativeOutputDirectory | null> {
    return this.#run(async (session) => {
      const result = await invoke<unknown>('select_output_directory', {
        request: { subscriptionId: session },
      });
      this.#sameSession(session);
      return outputDirectory(result);
    });
  }

  releaseOutputDirectory(directoryId: string): Promise<void> {
    positiveId(directoryId);
    return this.#run(async (session) => {
      const result = await invoke<unknown>('release_output_directory', {
        request: { subscriptionId: session, directoryId },
      });
      this.#sameSession(session);
      if (result !== null) throw new Error('Invalid output release response');
    });
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

  /** 拖放只消费同一会话中的Rust授权；设置在接收时固定，不能由展示名还原路径。 */
  importNativeDrop(
    session: string,
    selected: NativeImportGrant,
    settings: TaskSettingsDto | null,
  ): Promise<TaskMutationAccepted> {
    const fixed = structuredClone(settings);
    return this.#run((current) => {
      if (current !== session) throw new Error('Native drop belongs to an old session');
      const accepted = grant(selected);
      if (!accepted) throw new Error('Native drop has no file authorization');
      return this.#mutate(current, { kind: 'import', grantId: accepted.grantId, settings: fixed });
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
    if (operation.kind === 'import' || operation.kind === 'start') {
      const output = operation.settings?.output;
      if (output !== undefined && typeof output !== 'string') validateDirectoryTarget(output);
    }
    if (operation.kind === 'confirm_content_credentials') {
      if (operation.consent !== 'remove_content_credentials')
        throw new Error('Explicit content credentials consent is required');
      if (typeof operation.output === 'string') {
        if (
          !['copy_beside', 'overwrite_with_backup', 'overwrite_without_backup'].includes(
            operation.output,
          )
        )
          throw new Error('Invalid content credentials output');
      } else validateDirectoryTarget(operation.output);
    }
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
