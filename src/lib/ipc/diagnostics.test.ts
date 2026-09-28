import { beforeEach, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { getLogStatus, openLogDirectory } from './diagnostics';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }));
beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(isTauri).mockReturnValue(true);
});
it('does not access desktop logs from browser preview', async () => {
  vi.mocked(isTauri).mockReturnValue(false);
  await expect(getLogStatus()).resolves.toBeNull();
  await expect(openLogDirectory()).rejects.toThrow('desktop_only');
  expect(invoke).not.toHaveBeenCalled();
});
it('validates generated status and opens only the parameterless backend command', async () => {
  const value = { state: 'ready', droppedEvents: '0', writeFailures: '2', canOpen: true };
  vi.mocked(invoke).mockResolvedValue(value);
  await expect(getLogStatus()).resolves.toEqual(value);
  await openLogDirectory();
  expect(invoke).toHaveBeenCalledWith('get_log_status');
  expect(invoke).toHaveBeenCalledWith('open_log_directory');
});
it.each([
  null,
  {},
  { state: 'ready', droppedEvents: -1, writeFailures: '0', canOpen: true },
  { state: 'unknown', droppedEvents: '0', writeFailures: '0', canOpen: true },
])('rejects malformed native status %#', async (value) => {
  vi.mocked(invoke).mockResolvedValue(value);
  await expect(getLogStatus()).rejects.toThrow('invalid_log_status');
});
