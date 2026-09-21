import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getAppInfo } from '../lib/ipc/app';
import { usePreferences } from '../stores/preferences';
import { App } from './App';

vi.mock('../lib/ipc/app', () => ({ getAppInfo: vi.fn() }));

const appInfo = {
  name: 'PixoFold',
  version: '0.1.0',
  plannedFormats: ['png', 'jpeg', 'gif', 'apng'] as const,
  compressionAvailable: false,
};

describe('scaffold startup', () => {
  beforeEach(() => {
    localStorage.clear();
    usePreferences.setState({ theme: 'light', language: 'zh-CN' });
    vi.mocked(getAppInfo).mockReset();
  });

  it('shows browser preview and keeps appearance changes independent of IPC', async () => {
    vi.mocked(getAppInfo).mockResolvedValue(null);
    render(<App />);
    expect(await screen.findByRole('status')).toHaveTextContent('浏览器预览');
    const darkButton = screen.getByRole('button', { name: '深色' });
    expect(darkButton.textContent).toBe('');
    fireEvent.click(darkButton);
    expect(darkButton).toHaveAttribute('aria-pressed', 'true');
    expect(document.documentElement).toHaveAttribute('data-theme', 'dark');
    fireEvent.change(screen.getByRole('combobox', { name: '界面语言' }), {
      target: { value: 'en' },
    });
    expect(screen.getByRole('button', { name: 'Dark' })).toHaveAttribute('title', 'Dark');
    expect(document.documentElement).toHaveAttribute('lang', 'en');
    expect(getAppInfo).toHaveBeenCalledTimes(1);
    expect(JSON.parse(localStorage.getItem('pixofold.preferences') ?? '{}').state).toEqual({
      theme: 'dark',
      language: 'en',
    });
  });

  it('reports an IPC failure and reconnects to real version information', async () => {
    vi.mocked(getAppInfo)
      .mockRejectedValueOnce(new Error('unavailable'))
      .mockResolvedValueOnce({ ...appInfo, plannedFormats: [...appInfo.plannedFormats] });
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: '重新连接' }));
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('桌面核心已连接'));
    expect(screen.getByRole('status')).toHaveTextContent('v0.1.0');
    expect(screen.getByRole('list', { name: '规划支持的图片格式' })).toHaveTextContent('PNG');
    expect(screen.getByText('尚未接入压缩引擎，不会读取或修改您的图片。')).toBeVisible();
  });

  it('falls back safely for corrupted persisted preferences', async () => {
    localStorage.setItem(
      'pixofold.preferences',
      JSON.stringify({ version: 1, state: { theme: 'invalid', language: 42 } }),
    );
    await usePreferences.persist.rehydrate();
    expect(usePreferences.getState().theme).toBe('system');
    expect(usePreferences.getState().language).toBe('zh-CN');
  });
});
