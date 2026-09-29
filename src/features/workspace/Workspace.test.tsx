import { StrictMode } from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import {
  TASK_PROTOCOL_VERSION,
  type TaskPageRequest,
  type TaskSnapshotDto,
  type ConfirmationDto,
} from '../../lib/ipc/tasks.generated';
import { Workspace } from './Workspace';
import { useCompressionPreferences } from './settings';

const bridge = vi.hoisted(() => ({
  channels: [] as { onmessage: (value: unknown) => void }[],
  id: 0,
}));
vi.mock('@tauri-apps/api/core', () => ({
  isTauri: vi.fn(),
  invoke: vi.fn(),
  Channel: class {
    onmessage = (_value: unknown) => {};
    constructor() {
      bridge.channels.push(this);
    }
  },
}));
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
function snapshot(): TaskSnapshotDto {
  return {
    protocolVersion: TASK_PROTOCOL_VERSION,
    revision: '0',
    selectionId: null,
    phase: 'idle',
    scan: null,
    error: null,
    batch: null,
    page: { kind: 'jobs', offset: 0, total: 0, items: [] },
  };
}
const name = (text: string) => ({ text, truncated: false, sanitized: false, lossy: false });
let current: TaskSnapshotDto;
let confirmationRows: ConfirmationDto[] = [];
let Controller: typeof import('./controller').WorkspaceController;
let controller: InstanceType<typeof Controller>;
function reply(command: string, args: unknown): unknown {
  switch (command) {
    case 'subscribe_task_changes':
      return {
        protocolVersion: TASK_PROTOCOL_VERSION,
        subscriptionId: String(++bridge.id),
        revision: current.revision,
      };
    case 'acknowledge_task_changes':
    case 'release_native_drop':
      return null;
    case 'unsubscribe_task_changes':
      return true;
    case 'select_native_import':
      return { grantId: '1', rootCount: 1 };
    case 'apply_task_mutation':
      return { selectionId: current.selectionId ?? '1' };
    case 'get_task_snapshot': {
      // 本测试桥只接收本机适配器创建的请求，按请求返回对应集合而非混合页面。
      const { request } = args as { request: TaskPageRequest };
      const base = structuredClone(current);
      if (request.collection === 'confirmations') {
        base.page = {
          kind: 'confirmations',
          offset: request.offset,
          total: confirmationRows.length,
          items: confirmationRows.slice(request.offset, request.offset + request.limit),
        };
        return base;
      }
      if (request.collection !== base.page.kind)
        base.page = {
          kind: request.collection,
          offset: request.offset,
          total: request.offset,
          items: [],
        };
      return base;
    }
    default:
      throw new Error(command);
  }
}
async function update(value: TaskSnapshotDto) {
  current = value;
  await act(async () => {
    bridge.channels.at(-1)?.onmessage({
      protocolVersion: TASK_PROTOCOL_VERSION,
      subscriptionId: String(bridge.id),
      revision: current.revision,
    });
  });
  await waitFor(() => expect(controller.getSnapshot().snapshot?.revision).toBe(value.revision));
}
function writes() {
  return vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === 'apply_task_mutation')
    .map(([, args]) => args);
}
async function mount() {
  const ui = render(
    <StrictMode>
      <Workspace language="zh-CN" controller={controller} />
    </StrictMode>,
  );
  await waitFor(() => expect(controller.getSnapshot().connection).toBe('connected'));
  return ui;
}
beforeEach(async () => {
  vi.resetModules();
  ({ WorkspaceController: Controller } = await import('./controller'));
  controller = new Controller();
  current = snapshot();
  confirmationRows = [];
  bridge.channels.length = 0;
  bridge.id = 0;
  vi.mocked(isTauri).mockReturnValue(true);
  vi.mocked(invoke)
    .mockReset()
    .mockImplementation(async (command, args) => reply(command, args));
  useCompressionPreferences.setState({
    mode: 'lossy',
    quality: 80,
    output: 'overwrite',
    backupBeforeOverwrite: false,
    customOutput: false,
    preserveStructure: false,
  });
});

function credentialsSnapshot(count = 2): TaskSnapshotDto {
  confirmationRows = Array.from({ length: count }, (_, i) => ({
    id: i + 7,
    sourceName: name('同名.png'),
    sourceLabel: name('目录' + (i + 1)),
    inputBytes: '2048',
  }));
  return {
    ...snapshot(),
    revision: '20',
    selectionId: '3',
    phase: 'finished',
    batch: {
      id: '1',
      revision: '9007199254740993',
      phase: 'finished',
      mode: { kind: 'lossless' },
      confirmationCount: count,
      summary: {
        total: count,
        queued: 0,
        running: 0,
        succeeded: 0,
        noGain: 0,
        failed: count,
        cancelled: 0,
        processed: count,
        terminal: count,
        inputBytes: '4096',
        currentBytes: '4096',
        savedBytes: '0',
      },
    },
    page: { kind: 'jobs', offset: 0, total: 0, items: [] },
  };
}

describe('native drop import workflow', () => {
  const originalPoint = Object.getOwnPropertyDescriptor(document, 'elementFromPoint');
  const originalScale = Object.getOwnPropertyDescriptor(window, 'devicePixelRatio');
  const hit = vi.fn<(_: number, __: number) => Element | null>();
  beforeEach(() => {
    hit.mockReset().mockImplementation(() => document.querySelector('[data-has-files]'));
    Object.defineProperty(document, 'elementFromPoint', { configurable: true, value: hit });
    Object.defineProperty(window, 'devicePixelRatio', { configurable: true, value: 2 });
  });
  afterEach(() => {
    if (originalPoint) Object.defineProperty(document, 'elementFromPoint', originalPoint);
    else Reflect.deleteProperty(document, 'elementFromPoint');
    if (originalScale) Object.defineProperty(window, 'devicePixelRatio', originalScale);
  });
  async function drop(id = '8', session = String(bridge.id), authorized = true) {
    await act(async () =>
      bridge.channels.at(-1)?.onmessage({
        kind: 'native_drop',
        protocolVersion: TASK_PROTOCOL_VERSION,
        subscriptionId: session,
        offer: { offerId: id, grant: authorized ? { grantId: id, rootCount: 2 } : null },
        position: { x: 400, y: 300 },
      }),
    );
  }
  const releases = () =>
    vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === 'release_native_drop');

  it('hits the actual image area with scaled physical coordinates, freezes settings and imports once', async () => {
    const accepted = deferred<unknown>();
    vi.mocked(invoke).mockImplementation(async (cmd, args) =>
      cmd === 'apply_task_mutation' ? accepted.promise : reply(cmd, args),
    );
    await mount();
    fireEvent.click(screen.getByRole('button', { name: '另存副本' }));
    await drop();
    expect(hit).toHaveBeenCalledWith(200, 150);
    expect(writes()).toHaveLength(1);
    fireEvent.change(screen.getByRole('spinbutton', { name: '精细调整' }), {
      target: { value: '91' },
    });
    await drop(); // 同一票据重复投递不再次写入或提前释放。
    expect(writes()).toEqual([
      {
        request: {
          subscriptionId: '1',
          operation: {
            kind: 'import',
            grantId: '8',
            settings: { mode: { kind: 'lossy', quality: 80 }, output: 'copy_beside' },
          },
        },
      },
    ]);
    expect(releases()).toHaveLength(0);
    await act(async () => accepted.resolve({ selectionId: '1' }));
    await waitFor(() => expect(controller.getSnapshot().pending).toBe(false));
    expect(releases()).toHaveLength(1);
    expect(bridge.channels).toHaveLength(1);
    expect(invoke).not.toHaveBeenCalledWith('select_native_import', expect.anything());
  });

  it('releases outside, unauthorised and busy drops without writing or navigating', async () => {
    await mount();
    hit.mockReturnValue(screen.getByRole('heading', { name: '压缩设置' }));
    await drop();
    expect(writes()).toHaveLength(0);
    expect(screen.getByRole('status')).toHaveTextContent('左侧图片区域');
    hit.mockImplementation(() => document.querySelector('[data-has-files]'));
    await drop('9', '1', false);
    expect(screen.getByRole('status')).toHaveTextContent('本次拖放未接纳');
    await update({ ...snapshot(), revision: '1', selectionId: '1', phase: 'running' });
    await drop('10');
    expect(writes()).toHaveLength(0);
    expect(releases()).toHaveLength(3);
    const event = new Event('drop', { bubbles: true, cancelable: true });
    fireEvent(document.body, event);
    expect(event.defaultPrevented).toBe(true);
  });

  it('does not import underneath a modal and does not accept a late drop after unmount', async () => {
    current = credentialsSnapshot();
    const ui = await mount();
    fireEvent.click(screen.getByRole('button', { name: '需要确认的图片 (2)' }));
    await screen.findByRole('button', { name: '移除内容凭据后压缩 (2)' });
    await drop();
    expect(writes()).toHaveLength(0);
    expect(releases()).toHaveLength(1);
    ui.unmount();
    await waitFor(() => expect(controller.getSnapshot().connection).toBe('idle'));
    await drop('9');
    expect(writes()).toHaveLength(0);
    expect(releases()).toHaveLength(1);
  });

  it('scans with invalid quality and automatically starts once when the draft is corrected', async () => {
    await mount();
    const input = screen.getByRole('spinbutton', { name: '精细调整' });
    fireEvent.change(input, { target: { value: '' } });
    await drop();
    await waitFor(() => expect(controller.getSnapshot().pending).toBe(false));
    expect(writes()[0]).toMatchObject({
      request: { operation: { kind: 'import', settings: null } },
    });
    await update({ ...snapshot(), selectionId: '1', revision: '1', phase: 'ready' });
    expect(writes()).toHaveLength(1);
    fireEvent.keyDown(input, { key: 'Escape' });
    await waitFor(() => expect(writes()).toHaveLength(2));
    expect(writes()[1]).toMatchObject({
      request: { operation: { kind: 'start', selectionId: '1' } },
    });
  });

  it('releases uncertain writes without retrying and ignores old-session offers', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd === 'apply_task_mutation') throw new Error('response lost after admission');
      return reply(cmd, args);
    });
    await mount();
    await drop();
    await waitFor(() => expect(controller.getSnapshot().needsRecovery).toBe(true));
    expect(writes()).toHaveLength(1);
    expect(releases()).toHaveLength(1);
    await drop('9');
    expect(writes()).toHaveLength(1);
    await drop('10', '999');
    expect(releases()).toHaveLength(2);
    expect(controller.getSnapshot().connection).toBe('connected');
  });

  it('blocks changes after uncertain release and lets the user reconnect explicitly', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd === 'release_native_drop') throw new Error('release response lost');
      return reply(cmd, args);
    });
    await mount();
    hit.mockReturnValue(null);
    await drop();
    await waitFor(() => expect(controller.getSnapshot().needsRecovery).toBe(true));
    expect(writes()).toHaveLength(0);
    expect(screen.getByRole('button', { name: '重新连接任务' })).toBeEnabled();
  });
});

describe('session-bound custom output folders', () => {
  const selected = { directoryId: '91', name: name('导出目录') };
  function nativeOutput() {
    vi.mocked(invoke).mockImplementation(async (cmd, args) =>
      cmd === 'select_output_directory'
        ? selected
        : cmd === 'release_output_directory'
          ? null
          : reply(cmd, args),
    );
  }
  async function choose() {
    fireEvent.click(screen.getByRole('button', { name: '选择输出目录' }));
    await waitFor(() => expect(screen.getByText('导出目录')).toBeVisible());
    await waitFor(() => expect(controller.getSnapshot().pending).toBe(false));
  }

  it('selects without importing, preserves toggles and cancellation, and restores original folder', async () => {
    nativeOutput();
    await mount();
    fireEvent.click(screen.getByRole('button', { name: '另存副本' }));
    await choose();
    expect(writes()).toHaveLength(0);
    const structure = screen.getByRole('checkbox', { name: '保留输入目录结构' });
    expect(structure).not.toBeChecked();
    fireEvent.click(structure);
    fireEvent.click(screen.getByRole('button', { name: '原图覆盖' }));
    expect(screen.queryByRole('button', { name: '选择输出目录' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '另存副本' }));
    expect(screen.getByRole('checkbox', { name: '保留输入目录结构' })).toBeChecked();
    vi.mocked(invoke).mockImplementation(async (cmd, args) =>
      cmd === 'select_output_directory' || cmd === 'release_output_directory'
        ? null
        : reply(cmd, args),
    );
    await choose(); // 取消保留已选择的目录和布局。
    fireEvent.click(screen.getByRole('button', { name: '选择文件' }));
    await waitFor(() => expect(writes()).toHaveLength(1));
    expect(writes()[0]).toMatchObject({
      request: {
        operation: {
          settings: { output: { copy_to: { directoryId: '91', preserveStructure: true } } },
        },
      },
    });
    await waitFor(() => expect(controller.getSnapshot().pending).toBe(false));
    fireEvent.click(screen.getByRole('button', { name: '恢复原文件夹' }));
    await waitFor(() => expect(screen.getByText('原文件夹')).toBeVisible());
    expect(invoke).toHaveBeenCalledWith('release_output_directory', {
      request: { subscriptionId: '1', directoryId: '91' },
    });
    expect(screen.queryByRole('checkbox', { name: '保留输入目录结构' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '选择文件' }));
    await waitFor(() => expect(writes()).toHaveLength(2));
    expect(writes()[1]).toMatchObject({
      request: { operation: { settings: { output: 'copy_beside' } } },
    });
  });

  it('keeps missing authorization scan-only and starts Ready once after selecting a folder', async () => {
    nativeOutput();
    useCompressionPreferences.setState({ output: 'copy_beside', customOutput: true });
    await mount();
    expect(screen.getByText(/当前导入只扫描/)).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: '选择文件' }));
    await waitFor(() => expect(writes()).toHaveLength(1));
    expect(writes()[0]).toMatchObject({ request: { operation: { settings: null } } });
    await waitFor(() => expect(controller.getSnapshot().pending).toBe(false));
    await update({ ...snapshot(), selectionId: '1', revision: '1', phase: 'ready' });
    await choose();
    await waitFor(() => expect(writes()).toHaveLength(2));
    expect(writes()[1]).toMatchObject({
      request: {
        operation: {
          kind: 'start',
          settings: { output: { copy_to: { directoryId: '91', preserveStructure: false } } },
        },
      },
    });
    await update({ ...current, revision: '2' });
    expect(writes()).toHaveLength(2);
  });

  it('reports an explicitly rejected folder without losing the draft or requiring reconnect', async () => {
    nativeOutput();
    useCompressionPreferences.setState({ output: 'copy_beside' });
    const ui = await mount();
    await choose();
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd === 'select_output_directory') throw { code: 'invalid_output_directory' };
      return reply(cmd, args);
    });
    fireEvent.click(screen.getByRole('button', { name: '选择输出目录' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('所选输出目录不可用');
    expect(controller.getSnapshot().needsRecovery).toBe(false);
    expect(controller.getSnapshot().outputDirectory?.directoryId).toBe('91');
    expect(screen.getByRole('button', { name: '选择输出目录' })).toBeEnabled();
    ui.rerender(
      <StrictMode>
        <Workspace language="en" controller={controller} />
      </StrictMode>,
    );
    expect(screen.getByRole('alert')).toHaveTextContent(
      'The selected output folder is unavailable',
    );
    expect(screen.getByRole('button', { name: 'Choose output folder' })).toBeEnabled();
    expect(screen.getByRole('checkbox', { name: 'Keep input folder structure' })).not.toBeChecked();
    expect(writes()).toHaveLength(0);
    expect(bridge.channels).toHaveLength(1);
  });

  it('retries frozen jobs after reconnect without a new directory grant but still requires valid quality', async () => {
    nativeOutput();
    useCompressionPreferences.setState({ output: 'copy_beside' });
    current = credentialsSnapshot(1);
    current.batch!.confirmationCount = 0;
    current.page = {
      kind: 'jobs',
      offset: 0,
      total: 1,
      items: [
        {
          id: 7,
          attempt: 1,
          sourceName: name('failed.png'),
          mode: { kind: 'lossless' },
          inputBytes: '2048',
          state: { kind: 'failed', failure: { code: 'io', recovery: null } },
        },
      ],
    };
    await mount();
    await choose();
    await act(async () => controller.reconnect());
    expect(controller.getSnapshot().outputDirectory).toBeNull();
    expect(writes()).toHaveLength(0);
    const quality = screen.getByRole('spinbutton', { name: '精细调整' });
    fireEvent.change(quality, { target: { value: '' } });
    await act(async () => controller.retryPage());
    expect(writes()).toHaveLength(0);
    fireEvent.change(quality, { target: { value: '70' } });
    fireEvent.click(screen.getByRole('button', { name: '重试本页未完成项' }));
    await waitFor(() => expect(writes()).toHaveLength(1));
    expect(writes()[0]).toEqual({
      request: {
        subscriptionId: '2',
        operation: {
          kind: 'retry',
          selectionId: '3',
          expectedBatchRevision: '9007199254740993',
          jobIds: [7],
          mode: { kind: 'lossy', quality: 70 },
        },
      },
    });
  });

  it('invalidates the folder on reconnect without restarting the current batch', async () => {
    nativeOutput();
    useCompressionPreferences.setState({ output: 'copy_beside' });
    await mount();
    await choose();
    await act(async () => controller.reconnect());
    expect(controller.getSnapshot().outputDirectory).toBeNull();
    expect(screen.getByText(/当前导入只扫描/)).toBeVisible();
    expect(writes()).toHaveLength(0);
    expect(useCompressionPreferences.getState().customOutput).toBe(true);
  });

  it('freezes the output grant and layout for credentials confirmation and ignores later draft changes', async () => {
    nativeOutput();
    useCompressionPreferences.setState({ output: 'copy_beside' });
    current = credentialsSnapshot();
    await mount();
    await choose();
    fireEvent.click(screen.getByRole('checkbox', { name: '保留输入目录结构' }));
    fireEvent.click(screen.getByRole('button', { name: '需要确认的图片 (2)' }));
    const submit = await screen.findByRole('button', { name: '移除内容凭据后压缩 (2)' });
    expect(screen.queryByRole('radio', { name: '覆盖原图' })).not.toBeInTheDocument();
    await act(async () =>
      useCompressionPreferences.setState({ output: 'overwrite', preserveStructure: false }),
    );
    await act(async () =>
      controller.confirmContentCredentials('20', [7], {
        copy_to: { directoryId: '999', preserveStructure: true },
      }),
    );
    expect(writes()).toHaveLength(0);
    fireEvent.click(submit);
    await waitFor(() => expect(writes()).toHaveLength(1));
    expect(writes()[0]).toMatchObject({
      request: {
        operation: {
          kind: 'confirm_content_credentials',
          jobIds: [7, 8],
          output: { copy_to: { directoryId: '91', preserveStructure: true } },
        },
      },
    });
  });
});

describe('content credentials confirmation workflow', () => {
  it.each([false, true])(
    'inherits backup=%s on confirmation without any extra agreement step',
    async (backup) => {
      useCompressionPreferences.setState({ backupBeforeOverwrite: backup });
      current = credentialsSnapshot();
      await mount();
      fireEvent.click(screen.getByRole('button', { name: '需要确认的图片 (2)' }));
      const submit = await screen.findByRole('button', { name: '移除内容凭据后压缩 (2)' });
      expect(screen.getByRole('radio', { name: backup ? '备份原图' : '覆盖原图' })).toBeChecked();
      expect(screen.queryByRole('checkbox', { name: /我理解|我同意/ })).not.toBeInTheDocument();
      fireEvent.click(submit);
      await waitFor(() => expect(writes()).toHaveLength(1));
      expect(writes()[0]).toMatchObject({
        request: {
          operation: {
            jobIds: [7, 8],
            output: backup ? 'overwrite_with_backup' : 'overwrite_without_backup',
            consent: 'remove_content_credentials',
          },
        },
      });
    },
  );

  it('does not expose a partial list or accept it while later chunks are pending or failed', async () => {
    current = credentialsSnapshot(151);
    const later = deferred<unknown>();
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd === 'get_task_snapshot') {
        const { request } = args as { request: TaskPageRequest };
        if (request.collection === 'confirmations' && request.offset === 100) return later.promise;
      }
      return reply(cmd, args);
    });
    await mount();
    fireEvent.click(screen.getByRole('button', { name: '需要确认的图片 (151)' }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('get_task_snapshot', {
        request: {
          collection: 'confirmations',
          offset: 100,
          limit: 100,
          expectedRevision: '20',
        },
      }),
    );
    expect(screen.queryByRole('checkbox', { name: '全选' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /移除内容凭据后压缩/ })).not.toBeInTheDocument();
    await act(async () => {
      await controller.confirmContentCredentials('20', [7], 'overwrite_without_backup');
      later.reject(new Error('second chunk failed'));
    });
    await waitFor(() => expect(controller.getSnapshot().error).toBe('page'));
    expect(writes()).toHaveLength(0);
    expect(controller.getSnapshot().confirmationRows).toBeNull();
    vi.mocked(invoke).mockImplementation(async (cmd, args) => reply(cmd, args));
    await act(async () => {
      controller.setPage('confirmations');
    });
    expect(
      await screen.findByRole('checkbox', { name: '同名.png · 目录151 · #157' }),
    ).toBeChecked();
    expect(within(screen.getByRole('dialog')).getAllByRole('checkbox')).toHaveLength(152);
  });

  it('discards in-flight old rows on revision change instead of mixing confirmation versions', async () => {
    current = credentialsSnapshot(151);
    const oldChunk = deferred<unknown>();
    let oldResponse: TaskSnapshotDto | null = null;
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd === 'get_task_snapshot') {
        const { request } = args as { request: TaskPageRequest };
        if (
          request.collection === 'confirmations' &&
          request.offset === 100 &&
          request.expectedRevision === '20'
        ) {
          oldResponse = reply(cmd, args) as TaskSnapshotDto;
          return oldChunk.promise;
        }
      }
      return reply(cmd, args);
    });
    await mount();
    fireEvent.click(screen.getByRole('button', { name: '需要确认的图片 (151)' }));
    await waitFor(() => expect(oldResponse).not.toBeNull());
    const replacement = credentialsSnapshot(2);
    confirmationRows = confirmationRows.map((row) => ({ ...row, id: row.id + 200 }));
    await update({ ...replacement, revision: '21' });
    await act(async () => {
      oldChunk.resolve(oldResponse);
    });
    expect(await screen.findByRole('checkbox', { name: '同名.png · 目录1 · #207' })).toBeChecked();
    expect(within(screen.getByRole('dialog')).getAllByRole('checkbox')).toHaveLength(3);
    expect(screen.queryByRole('checkbox', { name: /#7$/ })).not.toBeInTheDocument();
    expect(writes()).toHaveLength(0);
  });

  it('closing during all-row loading discards late results without submitting', async () => {
    current = credentialsSnapshot();
    const rows = deferred<unknown>();
    const saved = {
      ...current,
      page: { kind: 'confirmations', offset: 0, total: 2, items: confirmationRows },
    };
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (
        cmd === 'get_task_snapshot' &&
        (args as { request: TaskPageRequest }).request.collection === 'confirmations'
      )
        return rows.promise;
      return reply(cmd, args);
    });
    await mount();
    fireEvent.click(screen.getByRole('button', { name: '需要确认的图片 (2)' }));
    fireEvent.click(screen.getByRole('button', { name: '暂不处理' }));
    await act(async () => {
      rows.resolve(saved);
    });
    expect(controller.getSnapshot().confirmationRows).toBeNull();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(writes()).toHaveLength(0);
  });
  it('shows credential removal on successful rows without opening details', async () => {
    current = credentialsSnapshot(0);
    current.page = {
      kind: 'jobs',
      offset: 0,
      total: 1,
      items: [
        {
          id: 7,
          attempt: 2,
          sourceName: name('credentials.png'),
          mode: { kind: 'lossless' },
          inputBytes: '2048',
          state: {
            kind: 'succeeded',
            report: {
              inputBytes: '2048',
              outputBytes: '1024',
              elapsedMs: '5',
              processing: { kind: 'lossless' },
              outputName: name('credentials_compressed.png'),
              backupName: null,
              contentCredentialsRemoved: true,
            },
          },
        },
      ],
    };
    await mount();
    expect(screen.getByText('已移除内容凭据')).toBeVisible();
  });
  it('restores without prompting, selects all by default and freezes copy settings without an output module or consent checkbox', async () => {
    current = credentialsSnapshot();
    useCompressionPreferences.setState({ output: 'copy_beside' });
    await mount();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '需要确认的图片 (2)' }));
    const first = await screen.findByRole('checkbox', { name: '同名.png · 目录1 · #7' });
    expect(first).toBeChecked();
    expect(screen.queryByRole('radio')).not.toBeInTheDocument();
    expect(screen.queryByRole('checkbox', { name: /我理解|我同意/ })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('checkbox', { name: '全选' }));
    expect(screen.getByRole('button', { name: '移除内容凭据后压缩 (0)' })).toBeDisabled();
    fireEvent.click(first);
    expect(screen.getByRole('button', { name: '移除内容凭据后压缩 (1)' })).toBeEnabled();
    await act(async () => {
      controller.setSettings({ mode: { kind: 'lossless' }, output: 'overwrite' });
      await controller.confirmContentCredentials('19', [7], 'copy_beside');
      await controller.confirmContentCredentials('20', [8, 99], 'copy_beside');
      await controller.confirmContentCredentials('20', [7], 'overwrite_without_backup');
    });
    expect(writes()).toHaveLength(0);
    const submit = screen.getByRole('button', { name: '移除内容凭据后压缩 (1)' });
    fireEvent.click(submit);
    fireEvent.click(submit);
    await waitFor(() => expect(writes()).toHaveLength(1));
    expect(writes()[0]).toEqual({
      request: {
        subscriptionId: '1',
        operation: {
          kind: 'confirm_content_credentials',
          selectionId: '3',
          expectedBatchRevision: '9007199254740993',
          jobIds: [7],
          mode: { kind: 'lossy', quality: 80 },
          output: 'copy_beside',
          consent: 'remove_content_credentials',
        },
      },
    });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /取消处理/ })).not.toBeInTheDocument();
  });

  it('automatically offers once after observed work ends; close is read-only and manual reopen supports English', async () => {
    const finished = credentialsSnapshot();
    current = { ...finished, revision: '19', phase: 'running' };
    const ui = await mount();
    await update(finished);
    await screen.findByRole('checkbox', { name: '同名.png · 目录1 · #7' });
    fireEvent.click(screen.getByRole('button', { name: '暂不处理' }));
    await update({ ...finished, revision: '21' });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(writes()).toHaveLength(0);
    ui.rerender(<Workspace language="en" controller={controller} />);
    fireEvent.click(screen.getByRole('button', { name: 'Images needing confirmation (2)' }));
    expect(
      await screen.findByText(/Compression requires removing Content Credentials/),
    ).toBeVisible();
    fireEvent(screen.getByRole('dialog'), new Event('cancel', { bubbles: true, cancelable: true }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(writes()).toHaveLength(0);
  });

  it('loads every row, allows a local backup override and never resends an uncertain overwrite', async () => {
    useCompressionPreferences.setState({ backupBeforeOverwrite: true });
    current = credentialsSnapshot(151);
    await mount();
    fireEvent.click(screen.getByRole('button', { name: '需要确认的图片 (151)' }));
    const last = await screen.findByRole('checkbox', { name: '同名.png · 目录151 · #157' });
    expect(last).toBeChecked();
    expect(within(screen.getByRole('dialog')).getAllByRole('checkbox')).toHaveLength(152);
    expect(screen.queryByRole('button', { name: '下一页' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '上一页' })).not.toBeInTheDocument();
    expect(screen.getByRole('radio', { name: '备份原图' })).toBeChecked();
    expect(screen.getByRole('button', { name: '移除内容凭据后压缩 (151)' })).toBeEnabled();
    fireEvent.click(screen.getByRole('checkbox', { name: '全选' }));
    fireEvent.click(last);
    fireEvent.click(screen.getByRole('radio', { name: '覆盖原图' }));
    expect(screen.getByText('直接覆盖，不保留备份。')).toBeVisible();
    const submit = screen.getByRole('button', { name: '移除内容凭据后压缩 (1)' });
    expect(submit).toBeEnabled();
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd === 'apply_task_mutation') throw new Error('uncertain transport');
      return reply(cmd, args);
    });
    fireEvent.click(submit);
    await waitFor(() => expect(controller.getSnapshot().needsRecovery).toBe(true));
    expect(writes()).toHaveLength(1);
    expect(writes()[0]).toMatchObject({
      request: {
        operation: {
          jobIds: [157],
          output: 'overwrite_without_backup',
          consent: 'remove_content_credentials',
        },
      },
    });
    await act(async () => {
      await controller.reconnect();
    });
    expect(writes()).toHaveLength(1);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('replaces the list on a new revision, resets to inherited settings and does not prompt for normal RGB', async () => {
    useCompressionPreferences.setState({ backupBeforeOverwrite: true });
    current = credentialsSnapshot();
    await mount();
    fireEvent.click(screen.getByRole('button', { name: '需要确认的图片 (2)' }));
    fireEvent.click(await screen.findByRole('checkbox', { name: '全选' }));
    fireEvent.click(screen.getByRole('radio', { name: '覆盖原图' }));
    await update({ ...current, revision: '21' });
    expect(await screen.findByRole('checkbox', { name: '全选' })).toBeChecked();
    expect(screen.getByRole('radio', { name: '备份原图' })).toBeChecked();
    fireEvent.click(screen.getByRole('button', { name: '暂不处理' }));
    const normal = credentialsSnapshot(0);
    await update({ ...normal, revision: '22', selectionId: '4', phase: 'running' });
    await update({ ...normal, revision: '23', selectionId: '4' });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(writes()).toHaveLength(0);
  });
});

describe('real-state workspace over a deterministic mock IPC transport', () => {
  it.each([
    ['zh-CN', '取消处理', '选择文件', '清除记录', '处理中', '已取消'],
    ['en', 'Cancel processing', 'Choose files', 'Clear records', 'Processing', 'Cancelled'],
  ] as const)(
    'offers no manual cancellation during scanning, preparation or compression in %s',
    async (language, cancel, files, clear, running, cancelled) => {
      current = { ...snapshot(), revision: '1', selectionId: '1', phase: 'scanning' };
      const ui = await mount();
      ui.rerender(
        <StrictMode>
          <Workspace language={language} controller={controller} />
        </StrictMode>,
      );
      expect(screen.queryByRole('button', { name: cancel })).not.toBeInTheDocument();
      expect(screen.getByRole('button', { name: files })).toBeDisabled();
      await update({ ...current, revision: '2', phase: 'preparing' });
      expect(screen.queryByRole('button', { name: cancel })).not.toBeInTheDocument();
      await update({
        ...current,
        revision: '3',
        phase: 'running',
        batch: {
          confirmationCount: 0,
          id: '1',
          revision: '1',
          phase: 'running',
          mode: { kind: 'lossy', quality: 80 },
          summary: {
            total: 1,
            queued: 0,
            running: 1,
            succeeded: 0,
            noGain: 0,
            failed: 0,
            cancelled: 0,
            processed: 0,
            terminal: 0,
            inputBytes: '2048',
            currentBytes: '2048',
            savedBytes: '0',
          },
        },
        page: {
          kind: 'jobs',
          offset: 0,
          total: 1,
          items: [
            {
              id: 1,
              attempt: 1,
              sourceName: name('running.png'),
              mode: { kind: 'lossy', quality: 80 },
              inputBytes: '2048',
              state: { kind: 'running', stage: 'optimizing', cancelRequested: false },
            },
          ],
        },
      });
      expect(screen.getByText(running, { exact: true })).toBeVisible();
      expect(screen.queryByRole('button', { name: cancel })).not.toBeInTheDocument();
      expect(screen.queryByRole('button', { name: clear })).not.toBeInTheDocument();
      expect(screen.queryByText(new RegExp(`${cancelled} 0`))).not.toBeInTheDocument();
      expect(screen.getByRole('button', { name: files })).toBeDisabled();
      expect(screen.getByRole('progressbar')).toHaveAttribute('value', '0');
      fireEvent.keyDown(window, { key: 'Escape' });
      fireEvent.keyDown(window, { key: 'o', ctrlKey: true });
      fireEvent.change(screen.getByRole('spinbutton'), { target: { value: '42' } });
      expect(controller.getSnapshot().snapshot?.batch?.mode).toEqual({
        kind: 'lossy',
        quality: 80,
      });
      expect(writes()).toHaveLength(0);
      expect(vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === 'select_native_import')).toBe(
        false,
      );
    },
  );
  it('keeps the prototype quality controls visible but disabled in lossless mode', async () => {
    await mount();
    const input = screen.getByRole('spinbutton', { name: '精细调整' });
    const slider = screen.getByRole('slider', { name: '压缩质量' });
    fireEvent.change(input, { target: { value: '76' } });
    expect(slider).toHaveValue('76');
    expect(screen.getByText('高')).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: '无损优化' }));
    expect(input).toBeDisabled();
    expect(slider).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: '有损压缩' }));
    expect(input).toBeEnabled();
    expect(input).toHaveValue(76);
    expect(writes()).toHaveLength(0);
  });
  it('keeps advanced options visible and only shows backup for overwrite', async () => {
    await mount();
    const backup = screen.getByRole('checkbox', { name: '覆盖前备份原图' });
    expect(backup).toBeVisible();
    expect(backup).not.toBeChecked();
    expect(screen.getByRole('heading', { name: '高级选项' }).closest('details')).toBeNull();
    expect(screen.getByText('直接覆盖，不保留备份；无收益则保留原图。')).toBeVisible();
    fireEvent.click(backup);
    expect(backup).toBeChecked();
    fireEvent.click(screen.getByRole('button', { name: '另存副本' }));
    expect(screen.getByRole('button', { name: '另存副本' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    expect(screen.getByText('原文件夹')).toBeVisible();
    expect(screen.getByRole('button', { name: '选择输出目录' })).toBeEnabled();
    expect(screen.queryByRole('checkbox', { name: '覆盖前备份原图' })).not.toBeInTheDocument();
    expect(screen.getByText(/副本使用 _compressed.png/)).toBeVisible();
    expect(controller.getSnapshot().pending).toBe(false);
    fireEvent.click(screen.getByRole('button', { name: '原图覆盖' }));
    expect(screen.getByRole('checkbox', { name: '覆盖前备份原图' })).toBeChecked();
    expect(writes()).toHaveLength(0);
  });
  it.each([
    [false, 'overwrite', 'overwrite_without_backup'],
    [true, 'overwrite', 'overwrite'],
    [true, 'copy_beside', 'copy_beside'],
  ] as const)(
    'imports backup=%s/output=%s with explicit policy %s',
    async (backup, output, policy) => {
      await mount();
      if (backup) fireEvent.click(screen.getByRole('checkbox', { name: '覆盖前备份原图' }));
      if (output === 'copy_beside')
        fireEvent.click(screen.getByRole('button', { name: '另存副本' }));
      fireEvent.click(screen.getByRole('button', { name: '选择文件' }));
      await waitFor(() => expect(writes()).toHaveLength(1));
      expect(writes()[0]).toMatchObject({
        request: { operation: { settings: { output: policy } } },
      });
    },
  );
  it('renders the expanded backup option and warning in English', async () => {
    const ui = await mount();
    ui.rerender(<Workspace language="en" controller={controller} />);
    expect(screen.getByRole('heading', { name: 'Advanced options' })).toBeVisible();
    expect(
      screen.getByRole('checkbox', { name: 'Back up originals before overwriting' }),
    ).not.toBeChecked();
    expect(screen.getByText(/Overwrite directly without backups/)).toBeVisible();
  });
  it('uses the native import shortcut once and does not navigate on unsupported drops', async () => {
    await mount();
    fireEvent.keyDown(window, { key: 'o', ctrlKey: true });
    await waitFor(() => expect(writes()).toHaveLength(1));
    fireEvent.keyDown(window, { key: 'o', ctrlKey: true, repeat: true });
    expect(writes()).toHaveLength(1);
    fireEvent.drop(screen.getByText('选择图片或文件夹'));
    expect(screen.getByRole('status')).toHaveTextContent('此拖放没有桌面文件授权');
    expect(writes()).toHaveLength(1);
  });
  it.each([
    [
      'unsupported_content_credentials',
      '含 C2PA 内容凭据（caBX）',
      'C2PA Content Credentials (caBX)',
    ],
    [
      'unsupported_metadata',
      '含当前不支持安全改写的元数据',
      'Contains metadata that cannot currently be rewritten safely',
    ],
    ['validation', '结果验证失败', 'Result validation failed'],
  ] as const)(
    'renders %s distinctly without claiming success or changing the validation category',
    async (code, chinese, english) => {
      current = {
        ...snapshot(),
        revision: '8',
        selectionId: '1',
        phase: 'finished',
        batch: {
          confirmationCount: 0,
          id: '1',
          revision: '4',
          phase: 'finished',
          mode: { kind: 'lossy', quality: 68 },
          summary: {
            total: 1,
            queued: 0,
            running: 0,
            succeeded: 0,
            noGain: 0,
            failed: 1,
            cancelled: 0,
            processed: 1,
            terminal: 1,
            inputBytes: '1030066',
            currentBytes: '1030066',
            savedBytes: '0',
          },
        },
        page: {
          kind: 'jobs',
          offset: 0,
          total: 1,
          items: [
            {
              id: 1,
              attempt: 1,
              sourceName: name('PixoFold-亮色.png'),
              mode: { kind: 'lossy', quality: 68 },
              inputBytes: '1030066',
              state: { kind: 'failed', failure: { code, recovery: null } },
            },
          ],
        },
      };
      const ui = await mount();
      fireEvent.click(screen.getByText('详情'));
      expect(screen.getByText(chinese, { exact: false })).toBeVisible();
      expect(screen.getByText('0 B')).toBeVisible();
      if (code !== 'validation') expect(screen.queryByText('结果验证失败')).not.toBeInTheDocument();
      ui.rerender(
        <StrictMode>
          <Workspace language="en" controller={controller} />
        </StrictMode>,
      );
      expect(screen.getByText(english, { exact: false })).toBeVisible();
      expect(writes()).toHaveLength(0);
    },
  );
  it('applies a corrected draft when Ready arrives before the preceding start response', async () => {
    current = { ...snapshot(), selectionId: '1', phase: 'ready', revision: '1' };
    const accepted = deferred<unknown>();
    vi.mocked(invoke).mockImplementation(async (cmd, args) =>
      cmd === 'apply_task_mutation' ? accepted.promise : reply(cmd, args),
    );
    await mount();
    fireEvent.click(screen.getByRole('button', { name: '无损优化' }));
    await update({ ...current, revision: '2', error: { code: 'invalid_parameters' } });
    fireEvent.click(screen.getByRole('button', { name: '有损压缩' }));
    expect(writes()).toHaveLength(1);
    await act(async () => accepted.resolve({ selectionId: '1' }));
    await waitFor(() => expect(writes()).toHaveLength(2));
    expect(writes()[1]).toMatchObject({
      request: { operation: { kind: 'start', settings: { mode: { kind: 'lossy', quality: 80 } } } },
    });
  });

  it('publishes disconnect/failed after a live Channel error without discarding the last snapshot', async () => {
    await mount();
    await act(async () => bridge.channels[0]?.onmessage({ malformed: true }));
    await waitFor(() => expect(controller.getSnapshot().connection).toBe('failed'));
    expect(controller.getSnapshot().snapshot?.revision).toBe('0');
    expect(screen.getByRole('button', { name: '选择文件' })).toBeDisabled();
    expect(screen.getByRole('button', { name: '重新连接任务' })).toBeEnabled();
    expect(writes()).toHaveLength(0);
  });
  it('uses one StrictMode Channel, no fake sizes, and releases only the subscription on unmount', async () => {
    const ui = await mount();
    expect(bridge.channels).toHaveLength(1);
    expect(writes()).toHaveLength(0);
    expect(screen.getByText('批量导入')).toBeVisible();
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
    ui.rerender(
      <StrictMode>
        <Workspace language="en" controller={controller} />
      </StrictMode>,
    );
    expect(screen.getByRole('button', { name: 'Choose files' })).toBeEnabled();
    expect(bridge.channels).toHaveLength(1);
    ui.unmount();
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('unsubscribe_task_changes', {
        request: { subscriptionId: '1' },
      }),
    );
    expect(writes()).toHaveLength(0);
  });
  it('freezes the click-time settings while a native dialog is open; cancellation does not import', async () => {
    const dialog = deferred<unknown>();
    vi.mocked(invoke).mockImplementation(async (cmd, args) =>
      cmd === 'select_native_import' ? dialog.promise : reply(cmd, args),
    );
    await mount();
    fireEvent.click(screen.getByRole('button', { name: '选择文件' }));
    fireEvent.change(screen.getByRole('spinbutton', { name: '精细调整' }), {
      target: { value: '90' },
    });
    fireEvent.click(screen.getByRole('checkbox', { name: '覆盖前备份原图' }));
    expect(screen.getByRole('button', { name: '选择目录' })).toBeDisabled();
    await act(async () => dialog.resolve({ grantId: '8', rootCount: 1 }));
    expect(writes()).toEqual([
      {
        request: {
          subscriptionId: '1',
          operation: {
            kind: 'import',
            grantId: '8',
            settings: { mode: { kind: 'lossy', quality: 80 }, output: 'overwrite_without_backup' },
          },
        },
      },
    ]);
    vi.mocked(invoke).mockImplementation(async (cmd, args) =>
      cmd === 'select_native_import' ? null : reply(cmd, args),
    );
    fireEvent.click(screen.getByRole('button', { name: '选择目录' }));
    await waitFor(() => expect(controller.getSnapshot().pending).toBe(false));
    expect(writes()).toHaveLength(1);
  });
  it('scans invalid drafts, starts once when corrected, and never loops on a planning failure', async () => {
    await mount();
    const input = screen.getByRole('spinbutton', { name: '精细调整' });
    fireEvent.change(input, { target: { value: '' } });
    expect(input).toHaveAttribute('aria-invalid', 'true');
    fireEvent.click(screen.getByRole('button', { name: '选择文件' }));
    await waitFor(() => expect(writes()).toHaveLength(1));
    expect(writes()[0]).toMatchObject({
      request: { operation: { kind: 'import', settings: null } },
    });
    await update({ ...snapshot(), selectionId: '1', revision: '1', phase: 'ready' });
    expect(writes()).toHaveLength(1);
    fireEvent.keyDown(input, { key: 'Escape' });
    await waitFor(() => expect(writes()).toHaveLength(2));
    expect(writes()[1]).toMatchObject({
      request: {
        operation: {
          kind: 'start',
          selectionId: '1',
          settings: { mode: { kind: 'lossy', quality: 80 } },
        },
      },
    });
    await update({
      ...current,
      revision: '2',
      error: { code: 'path_conflict', first: 0, second: 1, kind: 'output_is_input' },
    });
    expect(writes()).toHaveLength(2);
    fireEvent.click(screen.getByRole('button', { name: '另存副本' }));
    await waitFor(() => expect(writes()).toHaveLength(3));
  });
  it('does not auto-run Ready recovered on page load until the user changes settings', async () => {
    current = { ...snapshot(), selectionId: '9', revision: '6', phase: 'ready' };
    await mount();
    expect(writes()).toHaveLength(0);
    fireEvent.click(screen.getByRole('button', { name: '无损优化' }));
    await waitFor(() => expect(writes()).toHaveLength(1));
  });
  it('retains the last good snapshot, blocks uncertain writes, and reconnects without resending', async () => {
    await mount();
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd === 'apply_task_mutation') throw new Error('response lost');
      return reply(cmd, args);
    });
    fireEvent.click(screen.getByRole('button', { name: '选择文件' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('操作结果不确定');
    expect(screen.getByRole('button', { name: '选择文件' })).toBeDisabled();
    expect(controller.getSnapshot().snapshot?.revision).toBe('0');
    fireEvent.click(screen.getByRole('button', { name: '重新连接任务' }));
    await waitFor(() => expect(bridge.channels).toHaveLength(2));
    await waitFor(() => expect(controller.canChange).toBe(true));
    expect(writes()).toHaveLength(1);
  });
  it('propagates asynchronous subscription cleanup state and unknown identity requires reload', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd === 'subscribe_task_changes') throw new Error('unknown session');
      return reply(cmd, args);
    });
    render(<Workspace language="en" controller={controller} />);
    expect(await screen.findByRole('button', { name: 'Reload page' })).toBeEnabled();
    expect(controller.getSnapshot().connection).toBe('reload_required');
    await controller.reconnect();
    expect(bridge.channels).toHaveLength(1);
  });
  it('renders actual summary/recovery data, retries stable IDs at batch revision, and confirms clear', async () => {
    current = {
      ...snapshot(),
      revision: '20',
      selectionId: '3',
      phase: 'finished',
      batch: {
        confirmationCount: 0,
        id: '1',
        revision: '9007199254740993',
        phase: 'finished',
        mode: { kind: 'lossless' },
        summary: {
          total: 2,
          queued: 0,
          running: 0,
          succeeded: 0,
          noGain: 0,
          failed: 1,
          cancelled: 1,
          processed: 1,
          terminal: 2,
          inputBytes: '4096',
          currentBytes: '4096',
          savedBytes: '0',
        },
      },
      page: {
        kind: 'jobs',
        offset: 0,
        total: 2,
        items: [
          {
            id: 7,
            attempt: 1,
            sourceName: name('failed.png'),
            mode: { kind: 'lossless' },
            inputBytes: '2048',
            state: {
              kind: 'failed',
              failure: {
                code: 'cleanup_failed',
                recovery: {
                  backupName: name('.pixofold-backup-safe'),
                  temporaryName: name('.pixofold-temp-safe'),
                  originalError: { kind: 'failed', code: 'io' },
                },
              },
            },
          },
          {
            id: 42,
            attempt: 2,
            sourceName: name('cancelled.png'),
            mode: { kind: 'lossless' },
            inputBytes: '2048',
            state: { kind: 'cancelled' },
          },
        ],
      },
    };
    await mount();
    expect(screen.getByRole('progressbar')).toHaveAttribute('value', '1');
    expect(screen.getByRole('progressbar')).toHaveAttribute('max', '2');
    fireEvent.click(screen.getAllByText('详情')[0]!);
    expect(screen.getByText('.pixofold-backup-safe')).toBeVisible();
    expect(screen.getByText(/已取消 1/)).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: '重试本页未完成项' }));
    await waitFor(() => expect(writes()).toHaveLength(1));
    expect(writes()[0]).toMatchObject({
      request: {
        operation: { kind: 'retry', jobIds: [7, 42], expectedBatchRevision: '9007199254740993' },
      },
    });
    fireEvent.click(screen.getByRole('button', { name: '清除记录' }));
    expect(writes()).toHaveLength(1);
    expect(screen.getByRole('alertdialog')).toHaveTextContent('不删除图片或备份');
    fireEvent.click(screen.getByRole('button', { name: '确认清除' }));
    await waitFor(() => expect(writes()).toHaveLength(2));
    expect(writes()[1]).toMatchObject({
      request: { operation: { kind: 'clear', selectionId: '3' } },
    });
  });
  it('keeps only the latest page request and resets stale revisions instead of mixing rows', async () => {
    current = { ...snapshot(), selectionId: '1', phase: 'ready', revision: '10' };
    await mount();
    const older = deferred<unknown>();
    let extraReads = 0;
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd === 'get_task_snapshot') {
        const { request } = args as { request: TaskPageRequest };
        if (request.expectedRevision !== null && ++extraReads === 1) return older.promise;
        if (request.expectedRevision !== null)
          throw { code: 'stale_snapshot', currentRevision: '11' };
      }
      return reply(cmd, args);
    });
    act(() => {
      controller.setPage('issues', 50);
      controller.setPage('candidates', 100);
    });
    expect(extraReads).toBe(1);
    current = { ...current, revision: '11' };
    await act(async () => older.reject({ code: 'stale_snapshot', currentRevision: '11' }));
    await waitFor(() => expect(controller.getSnapshot().snapshot?.revision).toBe('11'));
    expect(controller.getSnapshot()).toMatchObject({
      offset: 0,
      collection: 'candidates',
      pageLoading: false,
    });
    expect(controller.getSnapshot().page?.revision).toBe('11');
    expect(writes()).toHaveLength(0);
  });
  it('ignores an old pagination failure after unmount and waits for subscription disposal on remount', async () => {
    const ui = await mount();
    const closing = deferred<unknown>();
    const read = deferred<unknown>();
    vi.mocked(invoke).mockImplementation(async (cmd, args) =>
      cmd === 'unsubscribe_task_changes'
        ? closing.promise
        : cmd === 'get_task_snapshot'
          ? read.promise
          : reply(cmd, args),
    );
    act(() => controller.setPage('issues'));
    ui.unmount();
    await waitFor(() => expect(controller.getSnapshot().connection).toBe('disconnecting'));
    const next = render(<Workspace language="en" controller={controller} />);
    expect(bridge.channels).toHaveLength(1);
    vi.mocked(invoke).mockImplementation(async (cmd, args) => reply(cmd, args));
    await act(async () => {
      closing.resolve(true);
      read.reject(new Error('late read'));
    });
    await waitFor(() => expect(controller.getSnapshot().connection).toBe('connected'));
    expect(controller.getSnapshot().error).toBeNull();
    expect(bridge.channels).toHaveLength(2);
    next.unmount();
  });
});
