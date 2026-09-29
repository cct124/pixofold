import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { JobDto, TaskPageDto } from '../../lib/ipc/tasks.generated';
import { TaskAssets } from '../../lib/ipc/task-assets';
import { WorkspaceRows } from './WorkspaceRows';

const name = (text: string) => ({ text, truncated: false, lossy: false, sanitized: false });
const job: JobDto = {
  id: 1,
  attempt: 1,
  sourceName: name('source.png'),
  inputBytes: '1000',
  mode: { kind: 'lossless' },
  state: {
    kind: 'succeeded',
    report: {
      inputBytes: '1000',
      outputBytes: '500',
      elapsedMs: '42',
      processing: { kind: 'lossless' },
      outputName: name('result.png'),
      backupName: name('source-backup-abc123.png'),
      contentCredentialsRemoved: false,
    },
  },
};
const access = { assets: new TaskAssets(() => null), selectionId: '1', enabled: true };
const page: TaskPageDto = { kind: 'jobs', offset: 0, total: 1, items: [job] };
afterEach(() => {
  vi.useRealTimers();
  access.assets.reset();
});

it('adds a rightmost Actions column, hover details in a portal and a direct result button', () => {
  const ui = render(<WorkspaceRows page={page} language="zh-CN" access={access} />);
  expect(screen.getAllByRole('columnheader').at(-1)).toHaveTextContent('操作');
  expect(ui.container.querySelector('details')).toBeNull();
  expect(screen.queryByText('result.png')).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: '在文件夹中查看' })).toBeEnabled();
  fireEvent.mouseEnter(screen.getByRole('button', { name: '详情' }));
  const panel = screen.getByRole('dialog', { name: '详情' });
  expect(panel.parentElement).toBe(document.body);
  expect(within(panel).getByText('result.png')).toBeVisible();
  expect(within(panel).getByRole('button', { name: '定位备份' })).toBeVisible();
  expect(ui.container).not.toHaveTextContent('result.png');
  expect(parseFloat(panel.style.left)).toBeGreaterThanOrEqual(12);
});

it('keeps the hover panel while crossing the gap and reading it, then closes on leave', () => {
  vi.useFakeTimers();
  render(<WorkspaceRows page={page} language="en" access={access} />);
  const trigger = screen.getByRole('button', { name: 'Details' });
  fireEvent.mouseEnter(trigger);
  fireEvent.mouseLeave(trigger);
  act(() => vi.advanceTimersByTime(100));
  fireEvent.mouseEnter(screen.getByRole('dialog'));
  act(() => vi.advanceTimersByTime(300));
  expect(screen.getByRole('dialog')).toBeVisible();
  fireEvent.mouseLeave(screen.getByRole('dialog'));
  act(() => vi.advanceTimersByTime(150));
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
});

it('opens on keyboard focus, allows backup access, and Escape restores focus without reopening', () => {
  render(<WorkspaceRows page={page} language="en" access={access} />);
  const trigger = screen.getByRole('button', { name: 'Details' });
  act(() => trigger.focus());
  expect(screen.getByRole('dialog')).toBeVisible();
  fireEvent.keyDown(trigger, { key: 'Tab' });
  expect(screen.getByRole('button', { name: 'Locate backup' })).toHaveFocus();
  fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(trigger).toHaveFocus();
});

it('closes on source scroll, task changes and modal opening without showing stale details', async () => {
  const ui = render(<WorkspaceRows page={page} language="en" access={access} />);
  fireEvent.mouseEnter(screen.getByRole('button', { name: 'Details' }));
  fireEvent.scroll(window);
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  fireEvent.mouseEnter(screen.getByRole('button', { name: 'Details' }));
  ui.rerender(
    <WorkspaceRows
      page={{ ...page, items: [{ ...job, attempt: 2 }] }}
      language="en"
      access={access}
    />,
  );
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  fireEvent.mouseEnter(screen.getByRole('button', { name: 'Details' }));
  const modal = document.createElement('dialog');
  modal.open = true;
  await act(async () => {
    document.body.append(modal);
  });
  expect(screen.queryByRole('dialog', { name: 'Details' })).not.toBeInTheDocument();
  modal.remove();
});

it('shows only one hover panel and disables viewing for rows without an actual result', () => {
  render(
    <WorkspaceRows
      page={{
        ...page,
        total: 2,
        items: [
          job,
          { ...job, id: 2, state: { kind: 'failed', failure: { code: 'decode', recovery: null } } },
        ],
      }}
      language="en"
      access={access}
    />,
  );
  const details = screen.getAllByRole('button', { name: 'Details' });
  fireEvent.mouseEnter(details[0]!);
  fireEvent.mouseEnter(details[1]!);
  expect(screen.getAllByRole('dialog')).toHaveLength(1);
  expect(screen.getAllByRole('button', { name: 'Show in folder' })[1]).toBeDisabled();
});
