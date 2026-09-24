import { useEffect, useState, useSyncExternalStore, type ReactNode } from 'react';
import type { Language } from '../../stores/preferences';
import { Icon } from '../../components/ui/Icon';
import { Dialog } from '../../components/ui/Dialog';
import { getWorkspace, WORKSPACE_PAGE_SIZE, type WorkspaceController } from './controller';
import { draftSettings, useCompressionPreferences } from './settings';
import { formatBytes, formatReduction } from './format';
import { detailText, workspaceText } from './messages';
import { WorkspaceRows } from './WorkspaceRows';
import { WorkspaceSettings } from './WorkspaceSettings';
import styles from './Workspace.module.css';

/** 只呈现Rust快照；组件重挂载不创建第二份任务或Channel。 */
export function Workspace({
  language,
  controller = getWorkspace(),
  settingsFooter,
}: {
  language: Language;
  controller?: WorkspaceController;
  settingsFooter?: ReactNode;
}) {
  const view = useSyncExternalStore(controller.subscribe, controller.getSnapshot);
  const [invalid, setInvalid] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [dropNotice, setDropNotice] = useState(false);
  const t = workspaceText(language);
  useEffect(() => {
    const saved = useCompressionPreferences.getState();
    controller.setSettings(draftSettings(saved.mode, String(saved.quality), saved.output), false);
    const shortcut = (event: KeyboardEvent) => {
      if (
        (event.ctrlKey || event.metaKey) &&
        !event.altKey &&
        event.key.toLowerCase() === 'o' &&
        !document.querySelector('dialog[open]')
      ) {
        event.preventDefault();
        if (!event.repeat && controller.canChange && controller.canImport)
          void controller.import('files');
      }
    };
    window.addEventListener('keydown', shortcut);
    return () => window.removeEventListener('keydown', shortcut);
  }, [controller]);
  const snapshot = view.snapshot;
  const summary = snapshot?.batch?.summary;
  const canChange = controller.canChange;
  const clearable =
    snapshot && ['ready', 'finished', 'cancelled', 'rejected'].includes(snapshot.phase);
  const page = view.page?.revision === snapshot?.revision ? view.page?.page : null;
  const showList = Boolean(
    snapshot?.selectionId &&
    (summary ||
      (snapshot.scan?.accepted ?? 0) > 0 ||
      snapshot.phase === 'ready' ||
      view.collection === 'issues'),
  );
  const retryable =
    snapshot?.phase === 'finished' &&
    page?.kind === 'jobs' &&
    page.items.some((job) => ['failed', 'cancelled'].includes(job.state.kind));
  const importButtons = (compact: boolean) => (
    <>
      <button
        className={compact ? 'text-button' : 'button primary'}
        disabled={!canChange || !controller.canImport}
        onClick={() => {
          setDropNotice(false);
          void controller.import('files');
        }}
      >
        {t('files')}
      </button>
      <button
        className={compact ? 'text-button' : 'button'}
        disabled={!canChange || !controller.canImport}
        onClick={() => {
          setDropNotice(false);
          void controller.import('folder');
        }}
      >
        {t('folder')}
      </button>
    </>
  );
  return (
    <main className={styles.layout}>
      <section className={styles.workspace} aria-labelledby="workspaceTitle">
        <div className={styles.title}>
          <h1 id="workspaceTitle">{t('title')}</h1>
          {snapshot?.selectionId && (
            <fieldset className={styles.views} aria-label={t('listLabel')}>
              {(['jobs', 'candidates', 'issues'] as const).map((collection) => (
                <button
                  className="text-button"
                  key={collection}
                  aria-pressed={view.collection === collection}
                  disabled={view.connection !== 'connected'}
                  onClick={() => controller.setPage(collection)}
                >
                  {t(collection)}
                </button>
              ))}
            </fieldset>
          )}
        </div>
        {view.connection !== 'connected' && (
          <output className={styles.connection}>
            {view.connection === 'failed'
              ? t('connection')
              : view.connection === 'idle'
                ? t('connecting')
                : t(view.connection)}
          </output>
        )}
        {(view.error || view.connection === 'reload_required') && (
          <div className={styles.error} role="alert">
            {view.error && <p>{t(view.error)}</p>}
            {view.connection === 'reload_required' ? (
              <button className="text-button" onClick={() => window.location.reload()}>
                {t('reload')}
              </button>
            ) : view.needsRecovery ? (
              <button
                className="text-button"
                disabled={view.pending || ['connecting', 'disconnecting'].includes(view.connection)}
                onClick={() => void controller.reconnect()}
              >
                {t('reconnect')}
              </button>
            ) : (
              <button
                className="text-button"
                onClick={() => controller.setPage(view.collection, view.offset)}
              >
                {t('pageRetry')}
              </button>
            )}
          </div>
        )}
        {snapshot?.error && (
          <div className={styles.error} role="alert">
            {t('taskError')}: {detailText(snapshot.error.code, language)}
            {snapshot.error.code === 'file' && ' · ' + t(snapshot.error.failure.code)}
          </div>
        )}
        <div
          className={styles.dropzone}
          data-has-files={showList}
          onDragOver={(event) => event.preventDefault()}
          onDrop={(event) => {
            event.preventDefault();
            setDropNotice(true);
          }}
        >
          {showList ? (
            <>
              <section
                className={styles.rows}
                aria-label={t('listLabel')}
                aria-busy={view.pageLoading}
              >
                {view.pageLoading ? (
                  <p className={styles.listNotice}>{t('loading')}</p>
                ) : page?.items.length ? (
                  <WorkspaceRows page={page} language={language} />
                ) : (
                  <p className={styles.listNotice}>{t('noRows')}</p>
                )}
              </section>
              {page && page.total > WORKSPACE_PAGE_SIZE && (
                <div className={styles.pagination}>
                  <small title={t('pageHint')}>
                    {Math.min(page.total, page.offset + 1)}–
                    {Math.min(page.total, page.offset + page.items.length)} / {page.total}
                  </small>
                  <div>
                    <button
                      className="text-button"
                      disabled={
                        view.pageLoading || view.offset === 0 || view.connection !== 'connected'
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
                      className="text-button"
                      disabled={
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
              )}
            </>
          ) : (
            <div className={styles.empty}>
              <div className={styles.dropArt} aria-hidden="true">
                <Icon name="image" />
                <span>
                  <Icon name="plus" />
                </span>
              </div>
              <p className={styles.dropTitle}>{t('emptyTitle')}</p>
              <div className={styles.formats}>
                <span>PNG</span>
                {['JPG', 'GIF', 'APNG'].map((format) => (
                  <span key={format} aria-disabled="true" title={t('formatUnavailable')}>
                    {format}
                  </span>
                ))}
              </div>
              <div className={styles.importActions}>{importButtons(false)}</div>
              <p className={styles.dropHint}>{t('emptyHint')}</p>
              <p className={styles.scope}>{t('scope')}</p>
            </div>
          )}
        </div>
        {dropNotice && <output className={styles.connection}>{t('dropUnavailable')}</output>}
        <footer className={styles.summary} aria-label={summary ? t('total') : t('tips')}>
          <div className={styles.summaryHeading}>
            <p className={styles.workStatus} aria-live="polite">
              {view.pending ? t('pending') : snapshot?.selectionId ? t(snapshot.phase) : t('tips')}
            </p>
            <div className={styles.workActions}>
              {retryable && (
                <button
                  className="text-button"
                  disabled={!canChange || invalid}
                  onClick={() => void controller.retryPage()}
                >
                  {t('retry')}
                </button>
              )}
              {showList && importButtons(true)}
              {clearable && (
                <button
                  className="text-button"
                  disabled={!canChange}
                  onClick={() => setConfirm(true)}
                >
                  {t('clear')}
                </button>
              )}
            </div>
          </div>
          {summary ? (
            <>
              <dl className={styles.summaryNumbers}>
                <div>
                  <dt>{t('count')}</dt>
                  <dd>{summary.total}</dd>
                </div>
                <div>
                  <dt>{t('before')}</dt>
                  <dd>{formatBytes(summary.inputBytes)}</dd>
                </div>
                <div>
                  <dt>{t('after')}</dt>
                  <dd>{formatBytes(summary.currentBytes)}</dd>
                </div>
                <div>
                  <dt>{t('saved')}</dt>
                  <dd>
                    {formatBytes(summary.savedBytes)}{' '}
                    <small>({formatReduction(summary.inputBytes, summary.currentBytes)})</small>
                  </dd>
                </div>
              </dl>
              <div className={styles.progress}>
                <div>
                  <span>
                    {t('processed')} {summary.processed} / {summary.total}
                  </span>
                  <span>
                    {summary.total ? Math.floor((summary.processed / summary.total) * 100) : 0}%
                  </span>
                </div>
                <progress
                  aria-label={t('progress')}
                  value={summary.processed}
                  max={Math.max(1, summary.total)}
                />
              </div>
            </>
          ) : (
            <dl className={styles.tips}>
              {(
                [
                  ['tipImportTitle', 'tipImport'],
                  ['tipSettingsTitle', 'tipSettings'],
                  ['tipShortcutTitle', 'tipShortcut'],
                ] as const
              ).map(([title, description]) => (
                <div key={title}>
                  <dt>{t(title)}</dt>
                  <dd>{t(description)}</dd>
                </div>
              ))}
            </dl>
          )}
          <div className={styles.summaryNotes}>
            <div className={styles.summaryDetails}>
              {summary && (
                <span>
                  {t('succeeded')} {summary.succeeded} · {t('no_gain')} {summary.noGain} ·{' '}
                  {t('failed')} {summary.failed}
                  {summary.cancelled > 0 && (
                    <>
                      {' '}
                      · {t('cancelled')} {summary.cancelled}
                    </>
                  )}
                </span>
              )}
              {snapshot?.scan && (
                <details>
                  <summary>
                    {t(snapshot.scan.status.kind)} · {t('accepted')} {snapshot.scan.accepted} ·{' '}
                    {t('issues')}
                  </summary>
                  <p>
                    {snapshot.scan.status.kind === 'limited' &&
                      t(
                        snapshot.scan.status.limit === 'files'
                          ? 'filesLimit'
                          : snapshot.scan.status.limit,
                      )}{' '}
                    {t('examined')} {snapshot.scan.examined} / {t('discovered')}{' '}
                    {snapshot.scan.discovered} · {t('duplicates')} {snapshot.scan.duplicates} ·{' '}
                    {t('excluded')} {snapshot.scan.excluded} · {t('rejectedCount')}{' '}
                    {snapshot.scan.rejected}
                  </p>
                </details>
              )}
            </div>
            <span className={styles.privacy}>{t('privacy')}</span>
          </div>
        </footer>
      </section>
      <WorkspaceSettings
        controller={controller}
        t={t}
        footer={settingsFooter}
        onValidityChange={setInvalid}
      />
      {confirm && (
        <Dialog
          title={t('clear')}
          closeLabel={t('closeDialog')}
          onClose={() => setConfirm(false)}
          alert
        >
          <div className="dialog-body">
            <p>{t('clearHint')}</p>
          </div>
          <div className="dialog-actions">
            <button className="button" onClick={() => setConfirm(false)}>
              {t('keep')}
            </button>
            <button
              className="button primary"
              disabled={!canChange || !clearable}
              onClick={() => {
                setConfirm(false);
                void controller.clear();
              }}
            >
              {t('confirmClear')}
            </button>
          </div>
        </Dialog>
      )}
    </main>
  );
}
