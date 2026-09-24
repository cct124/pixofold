import { useState } from 'react';
import { Dialog } from '../../components/ui/Dialog';
import type {
  ConfirmationDto,
  CredentialsOutput,
  TaskSettingsDto,
} from '../../lib/ipc/tasks.generated';
import type { Language } from '../../stores/preferences';
import { type WorkspaceController, type WorkspaceView } from './controller';
import { Name } from './WorkspaceRows';
import { formatBytes } from './format';
import { workspaceText, type WorkspaceText } from './messages';
import styles from './ContentCredentialsDialog.module.css';

/** 全量同版本列表加载后默认全选；按钮确认授权，不额外要求同意勾选。 */
export function ContentCredentialsDialog({
  controller,
  view,
  language,
}: {
  controller: WorkspaceController;
  view: WorkspaceView;
  language: Language;
}) {
  const t = workspaceText(language);
  const ready =
    view.confirmationRows &&
    view.confirmationRevision === view.snapshot?.revision &&
    !view.pageLoading;
  return (
    <Dialog
      title={t('confirmations') + ' (' + (view.snapshot?.batch?.confirmationCount ?? 0) + ')'}
      className={styles.dialog}
      closeLabel={t('closeDialog')}
      onClose={() => controller.closeConfirmations()}
    >
      <div className="dialog-body">
        <p>{t('credentialsIntro')}</p>
        {view.error || view.needsRecovery ? (
          <p role="alert">{t(view.error ?? 'connection')}</p>
        ) : null}
        {ready && view.confirmationRows && view.confirmationRevision ? (
          <SelectionForm
            key={view.confirmationRevision}
            rows={view.confirmationRows}
            revision={view.confirmationRevision}
            settings={view.confirmationSettings}
            t={t}
            controller={controller}
            disabled={!controller.canChange || view.needsRecovery}
          />
        ) : (
          <>
            <output>{view.error ? t('page') : t('loading')}</output>
            <div className="dialog-actions">
              {view.error === 'page' && !view.needsRecovery && (
                <button className="button" onClick={() => controller.setPage('confirmations')}>
                  {t('pageRetry')}
                </button>
              )}
              <button className="button" onClick={() => controller.closeConfirmations()}>
                {t('credentialsDefer')}
              </button>
            </div>
          </>
        )}
      </div>
    </Dialog>
  );
}

function SelectionForm({
  rows,
  revision,
  settings,
  t,
  controller,
  disabled,
}: {
  rows: ConfirmationDto[];
  revision: string;
  settings: TaskSettingsDto | null;
  t: WorkspaceText;
  controller: WorkspaceController;
  disabled: boolean;
}) {
  const [selected, setSelected] = useState(() => new Set(rows.map((row) => row.id)));
  const [backup, setBackup] = useState(true);
  const mode = settings?.mode;
  const output: CredentialsOutput =
    settings?.output === 'copy_beside'
      ? 'copy_beside'
      : backup
        ? 'overwrite_with_backup'
        : 'overwrite_without_backup';
  return (
    <>
      <p className={styles.mode}>
        {mode ? (
          <>
            {t('mode')}:{' '}
            <strong>
              {t(mode.kind)}
              {mode.kind === 'lossy'
                ? ' · ' + t('quality') + ' ' + mode.quality
                : ' · ' + t('credentialsPixelsOnly')}
            </strong>
          </>
        ) : (
          t('credentialsInvalid')
        )}
      </p>
      <div className={styles.selection}>
        <label className={styles.check}>
          <input
            type="checkbox"
            disabled={disabled || !rows.length}
            checked={rows.length > 0 && selected.size === rows.length}
            onChange={(e) =>
              setSelected(new Set(e.target.checked ? rows.map((row) => row.id) : []))
            }
          />
          {t('selectAll')}
        </label>
        <span>
          {t('selected')} {selected.size} / {rows.length}
        </span>
      </div>
      <div className={styles.list}>
        <table>
          <thead>
            <tr>
              <th>{t('name')}</th>
              <th>{t('reason')}</th>
              <th>{t('impact')}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.id}>
                <td>
                  <label className={styles.check}>
                    <input
                      type="checkbox"
                      disabled={disabled}
                      aria-label={
                        row.sourceName.text + ' · ' + row.sourceLabel.text + ' · #' + row.id
                      }
                      checked={selected.has(row.id)}
                      onChange={(e) =>
                        setSelected((previous) => {
                          const next = new Set(previous);
                          if (e.target.checked) next.add(row.id);
                          else next.delete(row.id);
                          return next;
                        })
                      }
                    />
                    <span>
                      <Name value={row.sourceName} t={t} />
                      <small>
                        <Name value={row.sourceLabel} t={t} /> · #{row.id} ·{' '}
                        {formatBytes(row.inputBytes)}
                      </small>
                    </span>
                  </label>
                </td>
                <td>{t('credentialsReason')}</td>
                <td>{t('credentialsImpact')}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {settings?.output === 'overwrite' && (
        <fieldset className={styles.output} disabled={disabled}>
          <legend>{t('output')}</legend>
          <div className={styles.options}>
            <label className={styles.check}>
              <input
                type="radio"
                name="credentials-backup"
                checked={!backup}
                onChange={() => setBackup(false)}
              />
              {t('credentialsOverwrite')}
            </label>
            <label className={styles.check}>
              <input
                type="radio"
                name="credentials-backup"
                checked={backup}
                onChange={() => setBackup(true)}
              />
              {t('credentialsBackup')}
            </label>
          </div>
          <p>{t(backup ? 'credentialsBackupHint' : 'credentialsOverwriteHint')}</p>
        </fieldset>
      )}
      <div className="dialog-actions">
        <button className="button" onClick={() => controller.closeConfirmations()}>
          {t('credentialsDefer')}
        </button>
        <button
          className="button primary"
          disabled={disabled || !settings || !selected.size}
          onClick={() => void controller.confirmContentCredentials(revision, [...selected], output)}
        >
          {t('credentialsSubmit')} ({selected.size})
        </button>
      </div>
    </>
  );
}
