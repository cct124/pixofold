import { useEffect, useState, useSyncExternalStore } from 'react';
import type { DisplayName, JobDto, TaskPageDto } from '../../lib/ipc/tasks.generated';
import type { Language } from '../../stores/preferences';
import { getWorkspace, WORKSPACE_PAGE_SIZE, type WorkspaceController } from './controller';
import { draftSettings, qualityValue, useCompressionPreferences } from './settings';
import { formatBytes } from './format';
import { detailText, workspaceText, type WorkspaceText } from './messages';
import styles from './Workspace.module.css';

function Name({ value, t }: { value: DisplayName; t: WorkspaceText }) {
  return (
    <span title={value.text}>
      {value.text}
      {(value.truncated || value.lossy || value.sanitized) && (
        <abbr title={t('nameWarning')}> *</abbr>
      )}
    </span>
  );
}

function JobResult({ job, language }: { job: JobDto; language: Language }) {
  const t = workspaceText(language);
  const state = job.state;
  return (
    <>
      <strong data-state={state.kind}>
        {t(state.kind)}
        {state.kind === 'running' && ' · ' + t(state.stage)}
      </strong>
      <small>
        {t(job.mode.kind)}
        {job.mode.kind === 'lossy' && ' ' + job.mode.quality} · {t('attempt')} {job.attempt}
      </small>
      {(state.kind === 'succeeded' || state.kind === 'no_gain') && (
        <>
          <small>
            {t(state.report.processing.kind)}
            {state.report.processing.kind === 'lossless_fallback' &&
              ' · ' + t(state.report.processing.reason.kind)}{' '}
            · {state.report.elapsedMs} ms
          </small>
          {state.report.outputName && (
            <small>
              {t('outputName')}: <Name value={state.report.outputName} t={t} />
            </small>
          )}
          {state.report.backupName && (
            <small>
              {t('backup')}: <Name value={state.report.backupName} t={t} />
            </small>
          )}
        </>
      )}
      {state.kind === 'failed' && (
        <>
          <small>{t(state.failure.code)}</small>
          {state.failure.recovery?.backupName && (
            <small>
              {t('backup')}: <Name value={state.failure.recovery.backupName} t={t} />
            </small>
          )}
          {state.failure.recovery?.temporaryName && (
            <small>
              {t('temporary')}: <Name value={state.failure.recovery.temporaryName} t={t} />
            </small>
          )}
          {state.failure.recovery?.originalError && (
            <small>
              {t('originalError')}:{' '}
              {state.failure.recovery.originalError.kind === 'cancelled'
                ? t('cancelled')
                : t(state.failure.recovery.originalError.code)}
            </small>
          )}
        </>
      )}
    </>
  );
}

function Rows({ page, language }: { page: TaskPageDto; language: Language }) {
  const t = workspaceText(language);
  return (
    <table className={styles.table}>
      <thead>
        <tr>
          <th>{t('name')}</th>
          <th>{t('size')}</th>
          <th>{t('result')}</th>
        </tr>
      </thead>
      <tbody>
        {page.kind === 'jobs' &&
          page.items.map((job) => (
            <tr key={job.id}>
              <td>
                <Name value={job.sourceName} t={t} />
              </td>
              <td>
                {formatBytes(job.inputBytes)}
                {(job.state.kind === 'succeeded' || job.state.kind === 'no_gain') && (
                  <small>→ {formatBytes(job.state.report.outputBytes)}</small>
                )}
              </td>
              <td>
                <JobResult job={job} language={language} />
              </td>
            </tr>
          ))}
        {page.kind === 'candidates' &&
          page.items.map((item) => (
            <tr key={item.index}>
              <td>
                <Name value={item.sourceName} t={t} />
              </td>
              <td>{formatBytes(item.inputBytes)}</td>
              <td>
                {item.width} × {item.height} · PNG
              </td>
            </tr>
          ))}
        {page.kind === 'issues' &&
          page.items.map((item) => (
            <tr key={item.index}>
              <td>
                <Name value={item.sourceName} t={t} />
              </td>
              <td>—</td>
              <td>
                {item.issue.kind === 'failure' ? t(item.issue.failure.code) : t(item.issue.kind)}
                {item.issue.kind === 'duplicate' && (
                  <small>
                    <Name value={item.issue.firstName} t={t} />
                  </small>
                )}
                {item.issue.kind === 'unsupported' && (
                  <small>{item.issue.format.toUpperCase()}</small>
                )}
              </td>
            </tr>
          ))}
      </tbody>
    </table>
  );
}

/** 只呈现Rust快照；设置草稿独立于批次，组件重挂载不创建第二份任务或Channel。 */
export function Workspace({
  language,
  controller = getWorkspace(),
}: {
  language: Language;
  controller?: WorkspaceController;
}) {
  const view = useSyncExternalStore(controller.subscribe, controller.getSnapshot);
  const preferences = useCompressionPreferences();
  const [quality, setQuality] = useState(String(preferences.quality));
  const [confirm, setConfirm] = useState(false);
  const t = workspaceText(language);
  useEffect(() => {
    const saved = useCompressionPreferences.getState();
    controller.setSettings(draftSettings(saved.mode, String(saved.quality), saved.output), false);
  }, [controller]);
  const invalid = preferences.mode === 'lossy' && qualityValue(quality) === null;
  const changeQuality = (value: string) => {
    setQuality(value);
    const parsed = qualityValue(value);
    if (parsed !== null) preferences.setQuality(parsed);
    controller.setSettings(draftSettings(preferences.mode, value, preferences.output));
  };
  const snapshot = view.snapshot;
  const summary = snapshot?.batch?.summary;
  const canChange = controller.canChange;
  const active = snapshot && ['scanning', 'preparing', 'running'].includes(snapshot.phase);
  const clearable =
    snapshot && ['ready', 'finished', 'cancelled', 'rejected'].includes(snapshot.phase);
  const page = view.page?.revision === snapshot?.revision ? view.page?.page : null;
  const retryable =
    snapshot?.phase === 'finished' &&
    page?.kind === 'jobs' &&
    page.items.some((job) => ['failed', 'cancelled'].includes(job.state.kind));
  const importButtons = (
    <div className={styles.buttons}>
      <button
        className={styles.primary}
        disabled={!canChange || !controller.canImport}
        onClick={() => {
          setConfirm(false);
          void controller.import('files');
        }}
      >
        {t('files')}
      </button>
      <button
        disabled={!canChange || !controller.canImport}
        onClick={() => {
          setConfirm(false);
          void controller.import('folder');
        }}
      >
        {t('folder')}
      </button>
    </div>
  );
  return (
    <main className={styles.layout}>
      <section className={styles.workspace} aria-labelledby="workspaceTitle">
        <div className={styles.content}>
          <div className={styles.title}>
            <h1 id="workspaceTitle">{t('title')}</h1>
            <span>{t('subtitle')}</span>
          </div>
          <output className={styles.connection}>
            {view.connection === 'failed'
              ? t('connection')
              : view.connection === 'idle'
                ? t('connecting')
                : t(view.connection)}
            {view.pending && <span>{t('pending')}</span>}
          </output>
          {(view.error || view.connection === 'reload_required') && (
            <div className={styles.error} role="alert">
              {view.error && <p>{t(view.error)}</p>}
              {view.connection === 'reload_required' ? (
                <button onClick={() => window.location.reload()}>{t('reload')}</button>
              ) : view.needsRecovery ? (
                <button
                  disabled={
                    view.pending || ['connecting', 'disconnecting'].includes(view.connection)
                  }
                  onClick={() => void controller.reconnect()}
                >
                  {t('reconnect')}
                </button>
              ) : (
                <button onClick={() => controller.setPage(view.collection, view.offset)}>
                  {t('pageRetry')}
                </button>
              )}
            </div>
          )}
          {!snapshot?.selectionId ? (
            <div className={styles.empty}>
              <div className={styles.fileMark} aria-hidden="true">
                PNG
              </div>
              <h2>{t('emptyTitle')}</h2>
              <p>{t('emptyHint')}</p>
              {importButtons}
              <p className={styles.scope}>{t('scope')}</p>
            </div>
          ) : (
            <>
              <div className={styles.toolbar}>
                {importButtons}
                {active && (
                  <button disabled={!canChange} onClick={() => void controller.cancel()}>
                    {t('cancel')}
                  </button>
                )}
                {clearable && (
                  <button disabled={!canChange} onClick={() => setConfirm(true)}>
                    {t('clear')}
                  </button>
                )}
              </div>
              <div className={styles.phase}>
                <strong>{t(snapshot.phase)}</strong>
                <small>
                  {t('selection')} #{snapshot.selectionId}
                </small>
              </div>
              {snapshot.error && (
                <div className={styles.error} role="alert">
                  {t('taskError')}: {detailText(snapshot.error.code, language)}
                  {snapshot.error.code === 'file' && ' · ' + t(snapshot.error.failure.code)}
                </div>
              )}
              {snapshot.scan && (
                <div className={styles.scan}>
                  <span>
                    {t(snapshot.scan.status.kind)}
                    {snapshot.scan.status.kind === 'limited' &&
                      ' · ' +
                        t(
                          snapshot.scan.status.limit === 'files'
                            ? 'filesLimit'
                            : snapshot.scan.status.limit,
                        )}
                  </span>
                  <span>
                    {t('examined')} {snapshot.scan.examined} / {t('discovered')}{' '}
                    {snapshot.scan.discovered} · {t('accepted')} {snapshot.scan.accepted} ·{' '}
                    {t('duplicates')} {snapshot.scan.duplicates} · {t('excluded')}{' '}
                    {snapshot.scan.excluded} · {t('rejectedCount')} {snapshot.scan.rejected}
                  </span>
                </div>
              )}
              {confirm && (
                <div className={styles.confirm} role="alertdialog" aria-label={t('clear')}>
                  <p>{t('clearHint')}</p>
                  <div className={styles.buttons}>
                    <button onClick={() => setConfirm(false)}>{t('keep')}</button>
                    <button
                      disabled={!canChange || !clearable}
                      onClick={() => {
                        setConfirm(false);
                        void controller.clear();
                      }}
                    >
                      {t('confirmClear')}
                    </button>
                  </div>
                </div>
              )}
              <fieldset className={styles.tabs} aria-label={t('title')}>
                {(['jobs', 'candidates', 'issues'] as const).map((collection) => (
                  <button
                    key={collection}
                    aria-pressed={view.collection === collection}
                    disabled={view.connection !== 'connected'}
                    onClick={() => controller.setPage(collection)}
                  >
                    {t(collection)}
                  </button>
                ))}
              </fieldset>
              <div className={styles.rows} aria-busy={view.pageLoading}>
                {view.pageLoading ? (
                  <p>{t('loading')}</p>
                ) : page?.items.length ? (
                  <Rows page={page} language={language} />
                ) : (
                  <p>{t('noRows')}</p>
                )}
              </div>
              <div className={styles.pagination}>
                <small>
                  {page
                    ? Math.min(page.total, page.offset + 1) +
                      '–' +
                      Math.min(page.total, page.offset + page.items.length) +
                      ' / ' +
                      page.total
                    : '—'}
                </small>
                <div className={styles.buttons}>
                  <button
                    disabled={
                      !page ||
                      view.pageLoading ||
                      view.offset === 0 ||
                      view.connection !== 'connected'
                    }
                    onClick={() =>
                      controller.setPage(
                        view.collection,
                        Math.max(0, view.offset - WORKSPACE_PAGE_SIZE),
                      )
                    }
                  >
                    {t('previous')}
                  </button>
                  <button
                    disabled={
                      !page ||
                      view.pageLoading ||
                      page.offset + page.items.length >= page.total ||
                      view.connection !== 'connected'
                    }
                    onClick={() =>
                      controller.setPage(view.collection, view.offset + WORKSPACE_PAGE_SIZE)
                    }
                  >
                    {t('next')}
                  </button>
                </div>
              </div>
              <small className={styles.pageHint}>{t('pageHint')}</small>
            </>
          )}
        </div>
        <footer className={styles.summary} aria-label={t('total')}>
          <div className={styles.summaryNumbers}>
            <div>
              <small>{t('processed')}</small>
              <strong>{summary ? summary.processed + ' / ' + summary.total : '—'}</strong>
            </div>
            <div>
              <small>{t('before')}</small>
              <strong>{formatBytes(summary?.inputBytes ?? null)}</strong>
            </div>
            <div>
              <small>{t('after')}</small>
              <strong>{formatBytes(summary?.currentBytes ?? null)}</strong>
            </div>
            <div>
              <small>{t('saved')}</small>
              <strong>{formatBytes(summary?.savedBytes ?? null)}</strong>
            </div>
          </div>
          {summary && (
            <>
              <progress
                aria-label={t('processed')}
                value={summary.processed}
                max={Math.max(1, summary.total)}
              />
              <div className={styles.summaryDetails}>
                <span>
                  {t('succeeded')} {summary.succeeded} · {t('no_gain')} {summary.noGain} ·{' '}
                  {t('failed')} {summary.failed} · {t('cancelled')} {summary.cancelled}
                </span>
                <button
                  disabled={!canChange || invalid || !retryable}
                  onClick={() => void controller.retryPage()}
                >
                  {t('retry')}
                </button>
              </div>
            </>
          )}
        </footer>
      </section>
      <aside className={styles.settings} aria-label={t('settings')}>
        <h2>{t('settings')}</h2>
        <fieldset>
          <legend>{t('mode')}</legend>
          <div className={styles.mode}>
            {(['lossy', 'lossless'] as const).map((mode) => (
              <button
                key={mode}
                aria-pressed={preferences.mode === mode}
                onClick={() => {
                  preferences.setMode(mode);
                  controller.setSettings(draftSettings(mode, quality, preferences.output));
                }}
              >
                {t(mode)}
              </button>
            ))}
          </div>
        </fieldset>
        {preferences.mode === 'lossy' ? (
          <div className={styles.quality}>
            <label htmlFor="qualityInput">{t('quality')}</label>
            <input
              id="qualityInput"
              type="text"
              inputMode="numeric"
              autoComplete="off"
              value={quality}
              aria-invalid={invalid}
              aria-describedby="qualityHint"
              onChange={(event) => changeQuality(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Escape') changeQuality(String(preferences.quality));
              }}
            />
            <input
              type="range"
              aria-label={t('quality')}
              min="0"
              max="100"
              step="1"
              value={qualityValue(quality) ?? preferences.quality}
              onChange={(event) => changeQuality(event.target.value)}
            />
            <p id="qualityHint" className={invalid ? styles.invalid : undefined}>
              {t(invalid ? 'invalidQuality' : 'qualityHint')}
            </p>
          </div>
        ) : (
          <p>{t('losslessHint')}</p>
        )}
        <fieldset>
          <legend>{t('output')}</legend>
          {(['overwrite', 'copy_beside'] as const).map((output) => (
            <label className={styles.radio} key={output}>
              <input
                type="radio"
                name="output"
                checked={preferences.output === output}
                onChange={() => {
                  preferences.setOutput(output);
                  controller.setSettings(draftSettings(preferences.mode, quality, output));
                }}
              />
              {t(output)}
            </label>
          ))}
        </fieldset>
        <p>{t('outputHint')}</p>
        <div className={styles.safety}>{t('frozen')}</div>
        <p>{t('settingsHint')}</p>
      </aside>
    </main>
  );
}
