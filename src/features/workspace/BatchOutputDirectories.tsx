import { useCallback, useEffect, useId, useRef, useState } from 'react';
import { FloatingPanel } from '../../components/ui/FloatingPanel';
import {
  assetErrorCode,
  type BatchAssetIdentity,
  type TaskAssets,
} from '../../lib/ipc/task-assets';
import type { AssetError, OutputDirectoryPage } from '../../lib/ipc/tasks.generated';
import { OUTPUT_DIRECTORY_PAGE_SIZE } from '../../lib/ipc/tasks.generated';
import type { Language } from '../../stores/preferences';
import { workspaceText } from './messages';
import styles from './Workspace.module.css';

/** 父层按批次版本/可用性设key；不从当前设置草稿推导路径，不在挂载时打开或查询目录。 */
export function BatchOutputDirectories({
  assets,
  identity,
  language,
  enabled,
}: {
  assets: TaskAssets;
  identity: BatchAssetIdentity;
  language: Language;
  enabled: boolean;
}) {
  const t = workspaceText(language);
  const anchor = useRef<HTMLButtonElement>(null);
  const id = useId();
  const [page, setPage] = useState<OutputDirectoryPage | null>(null);
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<AssetError['code'] | null>(null);
  const [requested, setRequested] = useState(false);
  const errorText =
    error === 'file_missing'
      ? t('directoryMissing')
      : error === 'file_changed'
        ? t('directoryChanged')
        : error === 'unavailable'
          ? t('directoryUnavailable')
          : error
            ? t(`asset_${error}`)
            : '';
  const busy = useRef(false);
  const life = useRef({ active: true });
  useEffect(() => {
    const token = { active: true };
    life.current = token;
    return () => {
      token.active = false;
    };
  }, []);
  const close = useCallback((restoreFocus: boolean) => {
    setOpen(false);
    if (restoreFocus) anchor.current?.focus();
  }, []);
  const run = async (offset: number | null, jobId?: number) => {
    if (
      !enabled ||
      busy.current ||
      document.querySelector('dialog[open]:not([data-floating-panel])')
    )
      return;
    const token = life.current;
    busy.current = true;
    setPending(true);
    setError(null);
    setRequested(false);
    try {
      if (offset !== null) {
        const next = await assets.outputDirectories(identity, offset);
        // 关于等应用级模态可能在查询期间打开，不能让迟到的单目录查询自动弹出Explorer。
        if (!token.active || document.querySelector('dialog[open]:not([data-floating-panel])'))
          return;
        if (next.total === 0) {
          setOpen(false);
          setError('unavailable');
        } else if (next.total === 1 && next.items[0]) {
          await assets.openOutputDirectory(identity, next.items[0].jobId);
          if (token.active) {
            setOpen(false);
            setRequested(true);
          }
        } else {
          setPage(next);
          setOpen(true);
        }
      } else if (jobId !== undefined) {
        await assets.openOutputDirectory(identity, jobId);
        if (token.active) {
          setOpen(false);
          setRequested(true);
        }
      }
    } catch (value) {
      if (token.active) setError(assetErrorCode(value));
    } finally {
      if (token.active) {
        busy.current = false;
        setPending(false);
      }
    }
  };
  return (
    <div className={styles.batchDirectories}>
      <button
        ref={anchor}
        type="button"
        className="text-button"
        disabled={!enabled || pending}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        onClick={() => {
          if (open) close(false);
          else void run(0);
        }}
      >
        {t('openOutputDirectory')}
      </button>
      <output className={styles.directoryNotice}>
        {pending
          ? t('revealPending')
          : error
            ? errorText
            : requested
              ? t('directoryRequested')
              : ''}
      </output>
      {open && enabled && page && (
        <FloatingPanel id={id} label={t('chooseResultDirectory')} anchor={anchor} onClose={close}>
          <strong>
            {t('chooseResultDirectory')} ({page.total})
          </strong>
          <ul className={styles.directoryList}>
            {page.items.map((item) => (
              <li key={item.jobId}>
                <button
                  type="button"
                  className="text-button"
                  disabled={pending}
                  onClick={() => void run(null, item.jobId)}
                >
                  <strong>{item.name.text}</strong>
                  <small>
                    #{item.jobId} · {item.exampleName.text} · {item.resultCount}{' '}
                    {t('directoryResultCount')}
                  </small>
                </button>
              </li>
            ))}
          </ul>
          {page.total > OUTPUT_DIRECTORY_PAGE_SIZE && (
            <div className={styles.directoryPages}>
              <button
                className="text-button"
                disabled={pending || page.offset === 0}
                onClick={() => void run(Math.max(0, page.offset - OUTPUT_DIRECTORY_PAGE_SIZE))}
              >
                {t('previous')}
              </button>
              <span>
                {page.offset + 1}–{page.offset + page.items.length} / {page.total}
              </span>
              <button
                className="text-button"
                disabled={pending || page.offset + page.items.length >= page.total}
                onClick={() => void run(page.offset + page.items.length)}
              >
                {t('next')}
              </button>
            </div>
          )}
          {error && <output>{errorText}</output>}
        </FloatingPanel>
      )}
    </div>
  );
}
