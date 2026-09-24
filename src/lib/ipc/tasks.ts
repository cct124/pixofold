import { invoke, isTauri } from '@tauri-apps/api/core';
import type { DecimalU64, QueryError, TaskPageRequest, TaskSnapshotDto } from './tasks.generated';
import { MAX_TASK_PAGE_SIZE, TASK_PROTOCOL_VERSION } from './tasks.generated';

const MAX_U64 = 18446744073709551615n;

/** 精确比较revision/字节值；禁止经过Number丢失低位。 */
export function parseDecimalU64(value: DecimalU64): bigint {
  if (
    typeof value !== 'string' ||
    value.trim() !== value ||
    !/^(0|[1-9][0-9]{0,19})$/.test(value)
  ) {
    throw new Error('Invalid decimal u64');
  }
  const parsed = BigInt(value);
  if (parsed > MAX_U64) throw new Error('Decimal u64 out of range');
  return parsed;
}

function validateRequest(request: TaskPageRequest): void {
  if (
    !['jobs', 'candidates', 'issues', 'confirmations'].includes(request.collection) ||
    !Number.isInteger(request.limit) ||
    request.limit < 1 ||
    request.limit > MAX_TASK_PAGE_SIZE ||
    !Number.isInteger(request.offset) ||
    request.offset < 0 ||
    request.offset > 0xffffffff ||
    (request.offset > 0 && request.expectedRevision === null)
  ) {
    throw { code: 'invalid_page' } satisfies QueryError;
  }
  if (request.expectedRevision !== null) parseDecimalU64(request.expectedRevision);
}

/**
 * 查询一份权威版本的摘要及一页明细。浏览器返回null，不伪造任务。
 * 翻页使用首个响应revision；stale_snapshot应重新从offset=0、expectedRevision=null恢复。
 * 只读且不启动任务。Rust/传输错误原样抛出，调用方负责展示；不记录私人数据。
 */
export async function getTaskSnapshot(request: TaskPageRequest): Promise<TaskSnapshotDto | null> {
  if (!isTauri()) return null;
  // 固定本次请求，不受调用方在await期间修改设置对象影响。
  const fixed = { ...request };
  validateRequest(fixed);
  const snapshot = await invoke<TaskSnapshotDto>('get_task_snapshot', { request: fixed });
  // 完整行由本机Rust序列化/生成类型保证；在适配边界额外验证协议和版本/分页信封。
  if (snapshot.protocolVersion !== TASK_PROTOCOL_VERSION)
    throw new Error('Unsupported task protocol version');
  parseDecimalU64(snapshot.revision);
  if (snapshot.selectionId !== null) parseDecimalU64(snapshot.selectionId);
  if (
    (fixed.expectedRevision !== null && snapshot.revision !== fixed.expectedRevision) ||
    snapshot.page.kind !== fixed.collection ||
    snapshot.page.offset !== fixed.offset ||
    !Number.isSafeInteger(snapshot.page.total) ||
    snapshot.page.total < fixed.offset ||
    snapshot.page.items.length !== Math.min(fixed.limit, snapshot.page.total - fixed.offset)
  ) {
    throw new Error('Inconsistent task snapshot envelope');
  }
  return snapshot;
}

export type SnapshotReadResult =
  { kind: 'applied'; snapshot: TaskSnapshotDto } | { kind: 'superseded' } | { kind: 'unavailable' };

/**
 * 单个可见页面的会话：并发请求只应用最后一次选择，旧revision不覆盖新版。
 * 不累积跨版本页面、不轮询、不建立订阅；重挂载新建实例，dispose只丢弃迟到响应，不取消Rust任务。
 */
export class TaskSnapshotReader {
  #latest: TaskSnapshotDto | null = null;
  #pending: object | null = null;
  #disposed = false;

  get current(): TaskSnapshotDto | null {
    return this.#latest;
  }

  async read(request: TaskPageRequest): Promise<SnapshotReadResult> {
    if (this.#disposed) return { kind: 'superseded' };
    const token = {};
    this.#pending = token;
    try {
      const snapshot = await getTaskSnapshot(request);
      if (this.#pending !== token || this.#disposed) return { kind: 'superseded' };
      if (snapshot === null) return { kind: 'unavailable' };
      if (
        this.#latest !== null &&
        parseDecimalU64(snapshot.revision) < parseDecimalU64(this.#latest.revision)
      ) {
        return { kind: 'superseded' };
      }
      this.#latest = snapshot;
      return { kind: 'applied', snapshot };
    } catch (error) {
      if (this.#pending !== token || this.#disposed) return { kind: 'superseded' };
      throw error;
    }
  }

  dispose(): void {
    this.#disposed = true;
    this.#pending = null;
    this.#latest = null;
  }
}
