import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { getLogStatus, openLogDirectory } from '../../lib/ipc/diagnostics';
import { Diagnostics } from './Diagnostics';
vi.mock('../../lib/ipc/diagnostics', () => ({ getLogStatus: vi.fn(), openLogDirectory: vi.fn() }));
beforeEach(() => {
  vi.mocked(getLogStatus).mockReset();
  vi.mocked(openLogDirectory).mockReset();
});
it('shows Chinese browser status and disables native actions', async () => {
  vi.mocked(getLogStatus).mockResolvedValue(null);
  render(<Diagnostics language="zh-CN" />);
  expect(await screen.findByText('浏览器预览不写入日志，请在桌面版查看。')).toBeVisible();
  expect(screen.getByRole('button', { name: '打开日志目录' })).toBeDisabled();
});
it('shows English degraded counts, retries and sanitizes errors', async () => {
  vi.mocked(getLogStatus).mockResolvedValue({
    state: 'busy',
    droppedEvents: '8',
    writeFailures: '1',
    canOpen: true,
  });
  vi.mocked(openLogDirectory).mockRejectedValue(new Error('PRIVATE_PATH'));
  render(<Diagnostics language="en" />);
  expect(await screen.findByText('Log folder busy; writing paused')).toBeVisible();
  expect(screen.getByText('Dropped events: 8 · Write failures: 1')).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: 'Open log folder' }));
  expect(await screen.findByRole('alert')).not.toHaveTextContent('PRIVATE_PATH');
  vi.mocked(getLogStatus).mockResolvedValue({
    state: 'ready',
    droppedEvents: '8',
    writeFailures: '1',
    canOpen: true,
  });
  fireEvent.click(screen.getByRole('button', { name: 'Refresh status' }));
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Recording'));
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});
