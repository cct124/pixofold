import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { TaskAssets, type BatchAssetIdentity } from '../../lib/ipc/task-assets';
import { BatchOutputDirectories } from './BatchOutputDirectories';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }));
const identity: BatchAssetIdentity = { selectionId: '1', batchId: '2', batchRevision: '3' };
const name = (text: string) => ({ text, truncated: false, lossy: false, sanitized: false });
const entry = (id: number) => ({
  jobId: id,
  name: name('output'),
  exampleName: name('sample-' + id + '.png'),
  resultCount: 1,
});
let assets: TaskAssets;
beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(isTauri).mockReturnValue(true);
  assets = new TaskAssets(() => '1');
});
afterEach(() => assets.reset());
function mount() {
  return render(
    <BatchOutputDirectories assets={assets} identity={identity} language="zh-CN" enabled />,
  );
}

it('only queries on click and opens the sole actual batch directory with no path/draft IPC', async () => {
  vi.mocked(invoke)
    .mockResolvedValueOnce({ total: 1, offset: 0, items: [entry(7)] })
    .mockResolvedValueOnce('requested');
  mount();
  expect(invoke).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: '打开输出目录' }));
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('已请求打开输出目录'));
  expect(invoke).toHaveBeenNthCalledWith(1, 'get_output_directories', {
    request: { batch: { subscriptionId: '1', ...identity }, offset: 0 },
  });
  expect(invoke).toHaveBeenNthCalledWith(2, 'open_output_directory', {
    request: { batch: { subscriptionId: '1', ...identity }, jobId: 7 },
  });
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
});

it('offers multiple directories with distinguishing task labels without opening every folder', async () => {
  vi.mocked(invoke)
    .mockResolvedValueOnce({ total: 2, offset: 0, items: [entry(7), entry(8)] })
    .mockResolvedValueOnce('requested');
  mount();
  fireEvent.click(screen.getByRole('button', { name: '打开输出目录' }));
  expect(await screen.findByRole('dialog')).toBeVisible();
  expect(invoke).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole('button', { name: /#8/ }));
  await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
  expect(invoke).toHaveBeenLastCalledWith('open_output_directory', {
    request: { batch: { subscriptionId: '1', ...identity }, jobId: 8 },
  });
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
});

it('bounds each menu page without mistaking one item on the final page for one total directory', async () => {
  vi.mocked(invoke)
    .mockResolvedValueOnce({
      total: 51,
      offset: 0,
      items: Array.from({ length: 50 }, (_, i) => entry(i + 1)),
    })
    .mockResolvedValueOnce({ total: 51, offset: 50, items: [entry(51)] });
  mount();
  fireEvent.click(screen.getByRole('button', { name: '打开输出目录' }));
  fireEvent.click(await screen.findByRole('button', { name: '下一页' }));
  await screen.findByRole('button', { name: /#51/ });
  expect(invoke).toHaveBeenLastCalledWith('get_output_directories', {
    request: { batch: { subscriptionId: '1', ...identity }, offset: 50 },
  });
  expect(invoke).toHaveBeenCalledTimes(2);
});

it('does not open a directory from a late query after clear or replacing the batch', async () => {
  let resolve!: (value: unknown) => void;
  vi.mocked(invoke).mockReturnValueOnce(
    new Promise((yes) => {
      resolve = yes;
    }),
  );
  const ui = mount();
  fireEvent.click(screen.getByRole('button', { name: '打开输出目录' }));
  expect(screen.getByRole('button')).toBeDisabled();
  fireEvent.click(screen.getByRole('button'));
  ui.unmount();
  await act(async () => {
    resolve({ total: 1, offset: 0, items: [entry(7)] });
  });
  expect(invoke).toHaveBeenCalledTimes(1);
});

it('does not automatically retry file-manager failures or show raw paths', async () => {
  vi.mocked(invoke)
    .mockResolvedValueOnce({ total: 1, offset: 0, items: [entry(7)] })
    .mockRejectedValueOnce({ code: 'file_missing', message: 'private/path' });
  mount();
  fireEvent.click(screen.getByRole('button', { name: '打开输出目录' }));
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('已被移动或删除'));
  expect(screen.queryByText('private/path')).not.toBeInTheDocument();
  expect(invoke).toHaveBeenCalledTimes(2);
});

it('does not open a folder behind a modal that appeared while the query was pending', async () => {
  let resolve!: (value: unknown) => void;
  vi.mocked(invoke).mockReturnValueOnce(
    new Promise((yes) => {
      resolve = yes;
    }),
  );
  mount();
  fireEvent.click(screen.getByRole('button', { name: '打开输出目录' }));
  const dialog = document.createElement('dialog');
  dialog.open = true;
  document.body.append(dialog);
  await act(async () => {
    resolve({ total: 1, offset: 0, items: [entry(7)] });
  });
  expect(invoke).toHaveBeenCalledTimes(1);
  dialog.remove();
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
});

it('does not invent results for an empty batch directory list', async () => {
  vi.mocked(invoke).mockResolvedValueOnce({ total: 0, offset: 0, items: [] });
  mount();
  fireEvent.click(screen.getByRole('button', { name: '打开输出目录' }));
  await waitFor(() => expect(screen.getByRole('status')).not.toBeEmptyDOMElement());
  expect(invoke).toHaveBeenCalledTimes(1);
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
});
