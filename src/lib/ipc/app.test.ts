import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { getAppInfo } from './app';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }));

describe('desktop IPC boundary', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('does not pretend to have a Rust backend in browser preview', async () => {
    vi.mocked(isTauri).mockReturnValue(false);
    await expect(getAppInfo()).resolves.toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });

  it('calls the registered native command and preserves backend failures', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    const error = new Error('IPC unavailable');
    vi.mocked(invoke).mockRejectedValue(error);
    await expect(getAppInfo()).rejects.toBe(error);
    expect(invoke).toHaveBeenCalledWith('get_app_info');
  });
});
