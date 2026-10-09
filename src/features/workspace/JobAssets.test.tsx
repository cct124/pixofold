import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { TaskAssets } from '../../lib/ipc/task-assets';
import type { JobDto, ReportDto } from '../../lib/ipc/tasks.generated';
import { JobThumbnail, JobFileActions, type RowAssets } from './JobAssets';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }));
const png = Array.from(
  atob(
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLttAAAAABJRU5ErkJggg==',
  ),
  (byte) => byte.charCodeAt(0),
);
const name = (text: string) => ({ text, truncated: false, lossy: false, sanitized: false });
const report: ReportDto = {
  inputBytes: '1000',
  outputBytes: '500',
  elapsedMs: '10',
  processing: { format: 'png', details: { kind: 'lossless' } },
  outputName: name('result.png'),
  backupName: null,
  contentCredentialsRemoved: false,
};
const job: JobDto = {
  id: 1,
  format: 'png',
  attempt: 1,
  sourceName: name('image.png'),
  inputBytes: '1000',
  mode: { kind: 'lossless' },
  state: { kind: 'succeeded', report },
};
const observers: Observer[] = [];
class Observer implements IntersectionObserver {
  readonly root = null;
  readonly rootMargin = '0px';
  readonly scrollMargin = '0px';
  readonly thresholds = [0];
  readonly callback: IntersectionObserverCallback;
  target: Element | null = null;
  constructor(callback: IntersectionObserverCallback) {
    this.callback = callback;
    observers.push(this);
  }
  observe(target: Element) {
    this.target = target;
  }
  unobserve() {
    this.target = null;
  }
  disconnect() {
    this.target = null;
  }
  takeRecords(): IntersectionObserverEntry[] {
    return [];
  }
  show(visible: boolean) {
    if (!this.target) return;
    this.callback(
      [
        {
          target: this.target,
          isIntersecting: visible,
          intersectionRatio: visible ? 1 : 0,
          boundingClientRect: new DOMRect(),
          intersectionRect: new DOMRect(),
          rootBounds: null,
          time: 0,
        },
      ],
      this,
    );
  }
}
let access: RowAssets;
const create = vi.fn(() => 'blob:preview'),
  revoke = vi.fn();
beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(isTauri).mockReturnValue(true);
  vi.mocked(invoke).mockImplementation(async (command) =>
    command === 'get_task_thumbnail' ? { width: 1, height: 1, png } : 'requested',
  );
  create.mockClear();
  revoke.mockClear();
  observers.length = 0;
  vi.stubGlobal('IntersectionObserver', Observer);
  vi.stubGlobal('URL', { createObjectURL: create, revokeObjectURL: revoke });
  access = { assets: new TaskAssets(() => '1'), selectionId: '1', enabled: true };
});
afterEach(() => {
  access.assets.reset();
  vi.unstubAllGlobals();
});

it('loads only a visible completed row and releases its Blob on scroll/unmount', async () => {
  const ui = render(<JobThumbnail job={job} language="zh-CN" access={access} />);
  expect(invoke).not.toHaveBeenCalled();
  act(() => observers[0]?.show(true));
  expect(await screen.findByAltText('当前结果文件预览')).toHaveAttribute('src', 'blob:preview');
  ui.rerender(<JobThumbnail job={job} language="en" access={access} />);
  expect(screen.getByAltText('Current result preview')).toBeInTheDocument();
  expect(invoke).toHaveBeenCalledTimes(1);
  act(() => observers[0]?.show(false));
  expect(screen.queryByRole('img')).not.toBeInTheDocument();
  expect(revoke).toHaveBeenCalledTimes(1);
  act(() => observers[0]?.show(true));
  await screen.findByAltText('Current result preview');
  ui.unmount();
  expect(revoke).toHaveBeenCalledTimes(2);
});

it('keeps running images as icons and degrades oversized/broken previews without blocking actions', async () => {
  const ui = render(
    <JobThumbnail
      job={{ ...job, state: { kind: 'running', stage: 'optimizing', cancelRequested: false } }}
      language="zh-CN"
      access={access}
    />,
  );
  expect(observers).toHaveLength(0);
  expect(invoke).not.toHaveBeenCalled();
  vi.mocked(invoke).mockRejectedValue({ code: 'resource_limit' });
  ui.rerender(<JobThumbnail job={job} language="zh-CN" access={access} />);
  act(() => observers[0]?.show(true));
  expect(await screen.findByTitle('图片超出预览资源限制；不影响压缩结果。')).toBeInTheDocument();
  expect(screen.queryByRole('img')).not.toBeInTheDocument();
});

it('never labels an overwritten result as an original preview and does not invent backup actions', async () => {
  render(<JobFileActions job={job} language="zh-CN" access={access} />);
  expect(screen.queryByRole('button', { name: '定位备份' })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: '在文件夹中查看' }));
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('已请求文件管理器定位'));
  expect(invoke).toHaveBeenCalledExactlyOnceWith('reveal_task_file', {
    request: {
      job: {
        subscriptionId: '1',
        selectionId: '1',
        jobId: 1,
        attempt: 1,
        expectedState: 'succeeded',
      },
      target: 'result',
    },
  });
});

it('locates the original on NoGain, and only an actual success/recovery backup', () => {
  const ui = render(
    <JobFileActions
      job={{
        ...job,
        state: { kind: 'no_gain', report: { ...report, outputName: null, backupName: null } },
      }}
      language="en"
      access={access}
    />,
  );
  expect(screen.getByRole('button', { name: 'Show in folder' })).toHaveAttribute(
    'title',
    'Show original in folder',
  );
  ui.rerender(
    <JobFileActions
      job={{
        ...job,
        state: {
          kind: 'succeeded',
          report: { ...report, backupName: name('image-backup-abc123.png') },
        },
      }}
      language="en"
      access={access}
      target="backup"
    />,
  );
  expect(screen.getByRole('button', { name: 'Locate backup' })).toBeInTheDocument();
  ui.rerender(
    <JobFileActions
      job={{
        ...job,
        state: {
          kind: 'failed',
          failure: {
            code: 'commit_failed',
            recovery: {
              backupName: name('recovery.png'),
              temporaryName: null,
              originalError: null,
            },
          },
        },
      }}
      language="en"
      access={access}
      target="backup"
    />,
  );
  expect(screen.getAllByRole('button')).toHaveLength(1);
  expect(screen.getByRole('button', { name: 'Locate backup' })).toBeInTheDocument();
});

it('reports missing files in both languages without displaying raw paths', async () => {
  vi.mocked(invoke).mockRejectedValue({ code: 'file_missing', message: 'private/path' });
  const ui = render(<JobFileActions job={job} language="zh-CN" access={access} />);
  fireEvent.click(screen.getByRole('button', { name: '在文件夹中查看' }));
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('文件已被移动或删除'));
  ui.rerender(<JobFileActions job={job} language="en" access={access} />);
  expect(screen.getByRole('status')).toHaveTextContent('File has been moved or deleted.');
  expect(screen.queryByText('private/path')).not.toBeInTheDocument();
});

it('drops a late location acknowledgement after a row changes attempt', async () => {
  let resolve!: (value: unknown) => void;
  vi.mocked(invoke).mockReturnValue(
    new Promise((yes) => {
      resolve = yes;
    }),
  );
  const ui = render(<JobFileActions job={job} language="en" access={access} />);
  fireEvent.click(screen.getByRole('button', { name: 'Show in folder' }));
  expect(screen.getByRole('button')).toBeDisabled();
  ui.rerender(<JobFileActions job={{ ...job, attempt: 2 }} language="en" access={access} />);
  await act(async () => {
    resolve('requested');
  });
  expect(screen.getByRole('status')).toBeEmptyDOMElement();
});
