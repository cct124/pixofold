import { beforeEach, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { outputDirectoryPage } from './output-directories';
import { TaskAssets } from './task-assets';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }));
const name = { text: 'out', truncated: false, lossy: false, sanitized: false };
const item = { jobId: 1, name, exampleName: name, resultCount: 2 };
const identity = { selectionId: '1', batchId: '2', batchRevision: '3' };
beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(isTauri).mockReturnValue(true);
});

it('validates bounded exact pages, names and representative row identifiers', () => {
  expect(outputDirectoryPage({ total: 1, offset: 0, items: [item] }, 0).items[0]).toEqual(item);
  for (const value of [
    null,
    { total: 1, offset: 1, items: [] },
    { total: 51, offset: 0, items: [item] },
    { total: 2, offset: 0, items: [item, item] },
    { total: 1, offset: 0, items: [{ ...item, jobId: 0 }] },
    { total: 1, offset: 0, items: [{ ...item, name: { ...name, text: 'x'.repeat(1000) } }] },
    { total: 100001, offset: 0, items: [] },
  ])
    expect(() => outputDirectoryPage(value, 0)).toThrow();
});

it('constructs only versioned identity fields and refuses stale sessions, bad ids and duplicate requests', async () => {
  let session: string | null = '1';
  const assets = new TaskAssets(() => session);
  let resolve!: (value: unknown) => void;
  vi.mocked(invoke).mockReturnValueOnce(
    new Promise((yes) => {
      resolve = yes;
    }),
  );
  const extra = { ...identity, path: 'private' };
  const read = assets.outputDirectories(extra, 0);
  await expect(assets.openOutputDirectory(identity, 1)).rejects.toEqual({ code: 'busy' });
  session = '2';
  resolve({ total: 1, offset: 0, items: [item] });
  await expect(read).rejects.toEqual({ code: 'session_unavailable' });
  expect(invoke).toHaveBeenCalledExactlyOnceWith('get_output_directories', {
    request: { batch: { subscriptionId: '1', ...identity }, offset: 0 },
  });
  await expect(assets.openOutputDirectory(identity, 0)).rejects.toEqual({ code: 'stale_task' });
});

it('does not accept a late query after reset, or unexpected open acknowledgements', async () => {
  const assets = new TaskAssets(() => '1');
  let resolve!: (value: unknown) => void;
  vi.mocked(invoke).mockReturnValueOnce(
    new Promise((yes) => {
      resolve = yes;
    }),
  );
  const read = assets.outputDirectories(identity, 0);
  assets.reset();
  resolve({ total: 1, offset: 0, items: [item] });
  await expect(read).rejects.toEqual({ code: 'stale_task' });
  vi.mocked(invoke).mockResolvedValue('opened');
  await expect(assets.openOutputDirectory(identity, 1)).rejects.toEqual({ code: 'service_fault' });
});
