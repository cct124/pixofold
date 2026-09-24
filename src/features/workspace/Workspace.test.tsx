import { StrictMode } from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import {
  TASK_PROTOCOL_VERSION,
  type TaskPageRequest,
  type TaskSnapshotDto,
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
  bridge.channels.length = 0;
  bridge.id = 0;
  vi.mocked(isTauri).mockReturnValue(true);
  vi.mocked(invoke)
    .mockReset()
    .mockImplementation(async (command, args) => reply(command, args));
  useCompressionPreferences.setState({ mode: 'lossy', quality: 80, output: 'overwrite' });
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
  it('segments output without granting unsupported directory or advanced controls', async () => {
    await mount();
    fireEvent.click(screen.getByRole('button', { name: '另存副本' }));
    expect(screen.getByRole('button', { name: '另存副本' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    expect(screen.getByText('原文件夹')).toBeVisible();
    expect(
      screen
        .getAllByRole('button', { name: '选择目录' })
        .filter((button) => button.hasAttribute('disabled')),
    ).toHaveLength(1);
    fireEvent.click(screen.getByText('高级选项'));
    expect(screen.getByText(/覆盖前建立备份/)).toBeVisible();
    expect(writes()).toHaveLength(0);
  });
  it('uses the native import shortcut once and does not navigate on unsupported drops', async () => {
    await mount();
    fireEvent.keyDown(window, { key: 'o', ctrlKey: true });
    await waitFor(() => expect(writes()).toHaveLength(1));
    fireEvent.keyDown(window, { key: 'o', ctrlKey: true, repeat: true });
    expect(writes()).toHaveLength(1);
    fireEvent.drop(screen.getByText('选择图片或文件夹'));
    expect(screen.getByRole('status')).toHaveTextContent('拖放尚未接入');
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
    expect(screen.getByRole('button', { name: '选择目录' })).toBeDisabled();
    await act(async () => dialog.resolve({ grantId: '8', rootCount: 1 }));
    expect(writes()).toEqual([
      {
        request: {
          subscriptionId: '1',
          operation: {
            kind: 'import',
            grantId: '8',
            settings: { mode: { kind: 'lossy', quality: 80 }, output: 'overwrite' },
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
