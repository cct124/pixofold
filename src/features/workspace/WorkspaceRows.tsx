import type { DisplayName, JobDto, TaskPageDto } from '../../lib/ipc/tasks.generated';
import type { Language } from '../../stores/preferences';
import { Icon } from '../../components/ui/Icon';
import { formatBytes, formatReduction } from './format';
import { workspaceText, type WorkspaceText } from './messages';
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

/** 表格字段全部来自任务快照；尚未接入缩略图时仅显示文件图标。 */
export function WorkspaceRows({ page, language }: { page: TaskPageDto; language: Language }) {
  const t = workspaceText(language);
  const jobs = page.kind === 'jobs';
  return (
    <table className={styles.table}>
      {jobs && (
        <colgroup>
          <col />
          <col />
          <col />
          <col />
          <col />
        </colgroup>
      )}
      <thead>
        <tr>
          <th>{t('name')}</th>
          <th>{t('before')}</th>
          {jobs && (
            <>
              <th>{t('resultSize')}</th>
              <th>{t('reduction')}</th>
            </>
          )}
          <th>{t('result')}</th>
        </tr>
      </thead>
      <tbody>
        {page.kind === 'jobs' &&
          page.items.map((job) => {
            const state = job.state;
            const report =
              state.kind === 'succeeded' || state.kind === 'no_gain' ? state.report : null;
            return (
              <tr key={job.id} data-status={state.kind}>
                <td aria-label={job.sourceName.text}>
                  <div className={styles.fileIdentity}>
                    <span className={styles.fileThumb}>
                      <Icon name="image" />
                    </span>
                    <div className={styles.fileText}>
                      <Name value={job.sourceName} t={t} />
                      <small>PNG · #{job.id}</small>
                    </div>
                  </div>
                </td>
                <td data-label={t('before')}>
                  {formatBytes(report?.inputBytes ?? job.inputBytes)}
                </td>
                <td data-label={t('resultSize')}>{formatBytes(report?.outputBytes ?? null)}</td>
                <td
                  className={report && state.kind === 'succeeded' ? styles.saving : undefined}
                  data-label={t('reduction')}
                >
                  {formatReduction(report?.inputBytes ?? null, report?.outputBytes ?? null)}
                </td>
                <td>
                  <span data-state={state.kind}>
                    {t(state.kind)}
                    {state.kind === 'running' && ' · ' + t(state.stage)}
                  </span>
                  <details className={styles.rowDetails}>
                    <summary>{t('details')}</summary>
                    <JobResult job={job} language={language} />
                  </details>
                </td>
              </tr>
            );
          })}
        {page.kind === 'candidates' &&
          page.items.map((item) => (
            <tr key={item.index}>
              <td>
                <Name value={item.sourceName} t={t} />
              </td>
              <td data-label={t('before')}>{formatBytes(item.inputBytes)}</td>
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
