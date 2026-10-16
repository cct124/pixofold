import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type {
  ApngProcessingDto,
  GifProcessingDto,
  JobDto,
  TaskPageDto,
} from '../../lib/ipc/tasks.generated';
import { TaskAssets } from '../../lib/ipc/task-assets';
import { WorkspaceRows } from './WorkspaceRows';

const name = (text: string) => ({ text, truncated: false, lossy: false, sanitized: false });
const job: JobDto = {
  id: 1,
  format: 'png',
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
      processing: { format: 'png', details: { kind: 'lossless' } },
      outputName: name('result.png'),
      backupName: name('source-backup-abc123.png'),
      contentCredentialsRemoved: false,
    },
  },
};
const access = { assets: new TaskAssets(() => null), selectionId: '1', enabled: true };
const page: TaskPageDto = { kind: 'jobs', offset: 0, total: 1, items: [job] };

it.each(['zh-CN', 'en'] as const)(
  'shows APNG actual lossless and total plays in %s',
  (language) => {
    if (job.state.kind !== 'succeeded') throw new Error('fixture');
    const apng: JobDto = {
      ...job,
      format: 'apng',
      mode: { kind: 'lossy', quality: 100 },
      state: {
        kind: 'succeeded',
        report: {
          ...job.state.report,
          processing: {
            format: 'apng',
            details: { kind: 'lossless', frames: 2, numPlays: 3, defaultParticipates: false },
          },
        },
      },
    };
    render(
      <WorkspaceRows
        page={{ kind: 'jobs', offset: 0, total: 1, items: [apng] }}
        language={language}
        access={access}
      />,
    );
    expect(screen.getByText('APNG · #1')).toBeVisible();
    fireEvent.mouseEnter(
      screen.getByRole('button', { name: language === 'en' ? 'Details' : '详情' }),
    );
    const details = screen.getByRole('dialog');
    expect(details).toHaveTextContent(language === 'en' ? 'Lossless' : '无损优化');
    expect(details).toHaveTextContent(language === 'en' ? 'Total plays: 3' : '总播放次数: 3');
    expect(details).toHaveTextContent(
      language === 'en' ? 'Default image is separate' : '默认图独立于动画',
    );
  },
);

it.each(['zh-CN', 'en'] as const)(
  'shows APNG lossy and both fallback reasons in %s',
  (language) => {
    if (job.state.kind !== 'succeeded') throw new Error('fixture');
    const shared = {
      frames: 4,
      numPlays: 0,
      defaultParticipates: true,
      mappingVersion: 1,
      policyVersion: 2,
    };
    const paths: ApngProcessingDto[] = [
      {
        ...shared,
        kind: 'lossy',
        quality: {
          paletteEntries: 27,
          minimumPixelQuality: 82,
          minimumRemappingQuality: 82,
          maxAlphaError: 0,
          worstFrameRmse: 3.5,
          worstTemporalRmse: 5,
        },
      },
      { ...shared, kind: 'lossless_fallback', reason: { kind: 'quality_below_target' } },
      { ...shared, kind: 'lossless_fallback', reason: { kind: 'insufficient_gain' } },
    ];
    for (const processing of paths) {
      const apng: JobDto = {
        ...job,
        format: 'apng',
        mode: { kind: 'lossy', quality: 80 },
        state: {
          kind: 'succeeded',
          report: { ...job.state.report, processing: { format: 'apng', details: processing } },
        },
      };
      const rendered = render(
        <WorkspaceRows
          page={{ kind: 'jobs', offset: 0, total: 1, items: [apng] }}
          language={language}
          access={access}
        />,
      );
      fireEvent.mouseEnter(
        screen.getByRole('button', { name: language === 'en' ? 'Details' : '详情' }),
      );
      const details = screen.getByRole('dialog');
      if (processing.kind === 'lossy')
        expect(details).toHaveTextContent(language === 'en' ? 'Lossy' : '有损压缩');
      else if (processing.kind === 'lossless_fallback') {
        expect(details).toHaveTextContent(language === 'en' ? 'Lossless' : '无损');
        expect(details).toHaveTextContent(
          processing.reason.kind === 'insufficient_gain'
            ? language === 'en'
              ? 'Insufficient additional savings'
              : '额外压缩收益不足'
            : language === 'en'
              ? 'Quality'
              : '质量',
        );
      }
      rendered.unmount();
    }
  },
);

it.each(['zh-CN', 'en'] as const)(
  'shows mixed formats and JPEG fallback honestly in %s',
  (language) => {
    if (job.state.kind !== 'succeeded') throw new Error('fixture');
    const jpeg: JobDto = {
      ...job,
      id: 2,
      format: 'jpeg',
      sourceName: name('photo.JPG'),
      mode: { kind: 'lossy', quality: 80 },
      state: {
        kind: 'succeeded',
        report: {
          ...job.state.report,
          processing: {
            format: 'jpeg',
            details: {
              kind: 'lossless_fallback',
              mappingVersion: 1,
              nativeQuality: 80,
              reason: { kind: 'color_profile' },
            },
          },
          outputName: name('photo.JPG'),
          backupName: null,
        },
      },
    };
    render(
      <WorkspaceRows
        page={{ kind: 'jobs', offset: 0, total: 2, items: [job, jpeg] }}
        language={language}
        access={access}
      />,
    );
    expect(screen.getByText('PNG · #1')).toBeVisible();
    expect(screen.getByText('JPEG · #2')).toBeVisible();
    const details = language === 'en' ? 'Details' : '详情';
    fireEvent.mouseEnter(screen.getAllByRole('button', { name: details })[1]!);
    expect(screen.getByRole('dialog')).toHaveTextContent(
      language === 'en'
        ? 'Lossless fallback · ICC color profile protection'
        : '回退无损 · ICC 色彩配置保护',
    );
    expect(screen.queryByText(/measured|量化评估/)).not.toBeInTheDocument();
  },
);
afterEach(() => {
  vi.useRealTimers();
  access.assets.reset();
});

it.each(['zh-CN', 'en'] as const)(
  'shows GIF actual paths and retained source in %s',
  (language) => {
    if (job.state.kind !== 'succeeded') throw new Error('fixture');
    const paths: GifProcessingDto[] = [
      { kind: 'lossless', frames: 12, loopCount: null },
      { kind: 'lossy', frames: 12, loopCount: 0, mappingVersion: 1, nativeLossiness: 10 },
      {
        kind: 'lossless_fallback',
        frames: 12,
        loopCount: 3,
        mappingVersion: 1,
        nativeLossiness: 10,
        reason: { kind: 'quality_below_target' },
      },
      {
        kind: 'lossless_fallback',
        frames: 12,
        loopCount: 0,
        mappingVersion: 1,
        nativeLossiness: 10,
        reason: { kind: 'insufficient_gain' },
      },
    ];
    const labels =
      language === 'en'
        ? [
            'Lossless',
            'Lossy',
            'Lossless fallback · Quality target not met',
            'Lossless fallback · Insufficient additional savings',
          ]
        : ['无损', '有损', '回退无损 · 未达到质量目标', '回退无损 · 额外压缩收益不足'];
    for (const [i, details] of paths.entries()) {
      const noGain = i === 3;
      const gif: JobDto = {
        ...job,
        format: 'gif',
        sourceName: name('animation.GIF'),
        mode: { kind: 'lossy', quality: i === 0 ? 100 : 80 },
        state: {
          kind: noGain ? 'no_gain' : 'succeeded',
          report: {
            ...job.state.report,
            outputName: noGain ? null : name('animation.GIF'),
            backupName: null,
            processing: { format: 'gif', details },
          },
        },
      };
      const ui = render(
        <WorkspaceRows
          page={{ kind: 'jobs', offset: 0, total: 1, items: [gif] }}
          language={language}
          access={access}
        />,
      );
      fireEvent.mouseEnter(
        screen.getByRole('button', { name: language === 'en' ? 'Details' : '详情' }),
      );
      const panel = screen.getByRole('dialog');
      expect(panel).toHaveTextContent(labels[i]!);
      expect(panel).toHaveTextContent(language === 'en' ? 'Source frames: 12' : '原图帧数: 12');
      expect(panel).not.toHaveTextContent(/nativeLossiness|RMSE|measuredQuality/);
      if (noGain) expect(panel).not.toHaveTextContent('animation.GIF');
      ui.unmount();
    }
  },
);

it.each([
  {
    language: 'zh-CN',
    actions: '操作',
    view: '在文件夹中查看',
    details: '详情',
    backup: '定位备份',
    status: '已压缩',
  },
  {
    language: 'en',
    actions: 'Actions',
    view: 'Show in folder',
    details: 'Details',
    backup: 'Locate backup',
    status: 'Compressed',
  },
] as const)(
  'keeps $language details supplemental to the table with a direct result button',
  ({ language, actions, view, details, backup, status }) => {
    const ui = render(<WorkspaceRows page={page} language={language} access={access} />);
    expect(screen.getAllByRole('columnheader').at(-1)).toHaveTextContent(actions);
    expect(ui.container.querySelector('details')).toBeNull();
    expect(screen.queryByText('result.png')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: view })).toBeEnabled();
    fireEvent.mouseEnter(screen.getByRole('button', { name: details }));
    const panel = screen.getByRole('dialog', { name: details });
    expect(panel.parentElement).toBe(document.body);
    expect(within(panel).queryByText('source.png')).not.toBeInTheDocument();
    expect(within(panel).queryByText(status, { exact: true })).not.toBeInTheDocument();
    expect(within(ui.container).getByText('source.png')).toBeVisible();
    expect(within(ui.container).getByText(status, { exact: true })).toBeVisible();
    expect(within(panel).getByText('result.png')).toBeVisible();
    expect(within(panel).getByText(/42 ms/)).toBeVisible();
    expect(within(panel).getByText('source-backup-abc123.png')).toBeVisible();
    expect(within(panel).getByRole('button', { name: backup })).toBeVisible();
    expect(ui.container).not.toHaveTextContent('result.png');
    expect(parseFloat(panel.style.left)).toBeGreaterThanOrEqual(12);
  },
);

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
  const panel = screen.getByRole('dialog');
  expect(within(panel).queryByText('Failed', { exact: true })).not.toBeInTheDocument();
  expect(within(panel).getByText(/Could not read the image data/)).toBeVisible();
  expect(panel).toHaveTextContent('The original was not overwritten.');
  expect(panel).toHaveTextContent('Download or export it again, then retry.');
  expect(screen.getAllByRole('button', { name: 'Show in folder' })[1]).toBeDisabled();
});
