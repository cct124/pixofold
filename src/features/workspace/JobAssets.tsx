import { useEffect, useMemo, useRef, useState } from 'react';
import { Icon } from '../../components/ui/Icon';
import {
  assetErrorCode,
  type AssetIdentity,
  type TaskAssets,
  type ThumbnailView,
} from '../../lib/ipc/task-assets';
import type { JobDto, RevealTarget } from '../../lib/ipc/tasks.generated';
import type { Language } from '../../stores/preferences';
import { workspaceText } from './messages';
import styles from './Workspace.module.css';

export type RowAssets = { assets: TaskAssets; selectionId: string; enabled: boolean };
function useIdentity(selectionId: string, job: JobDto): AssetIdentity | null {
  const {
    id,
    attempt,
    state: { kind },
  } = job;
  return useMemo(
    () =>
      kind === 'queued' || kind === 'running'
        ? null
        : {
            selectionId,
            jobId: id,
            attempt,
            expectedState: kind,
          },
    [selectionId, id, attempt, kind],
  );
}

/** 在可见区域内才申请；行离屏、翻页、清除或重试即解除观察并释放Blob。 */
export function JobThumbnail({
  job,
  language,
  access,
}: {
  job: JobDto;
  language: Language;
  access: RowAssets;
}) {
  const t = workspaceText(language);
  const element = useRef<HTMLSpanElement>(null);
  const identity = useIdentity(access.selectionId, job);
  const owner = useMemo(() => ({ identity, enabled: access.enabled }), [identity, access.enabled]);
  const [state, setState] = useState<{ owner: typeof owner; view: ThumbnailView } | null>(null);
  const view: ThumbnailView = state?.owner === owner ? state.view : { kind: 'loading' };
  useEffect(() => {
    if (!identity || !access.enabled || !element.current) return;
    const setView = (view: ThumbnailView) => setState({ owner, view });
    let stop: (() => void) | null = null;
    const visible = (show: boolean) => {
      if (show && !stop) stop = access.assets.watch(identity, setView);
      if (!show && stop) {
        stop();
        stop = null;
        setView({ kind: 'loading' });
      }
    };
    const observer =
      typeof IntersectionObserver === 'undefined'
        ? null
        : new IntersectionObserver((entries) => {
            visible(entries.some((entry) => entry.isIntersecting));
          });
    if (observer) observer.observe(element.current);
    else visible(true);
    return () => {
      observer?.disconnect();
      stop?.();
    };
  }, [identity, access.assets, access.enabled, owner]);
  const loaded = identity && access.enabled && view.kind === 'ready';
  const label = job.state.kind === 'succeeded' ? t('thumbnailResult') : t('thumbnailSource');
  return (
    <span
      ref={element}
      className={styles.fileThumb}
      title={
        loaded
          ? label
          : view.kind === 'unavailable'
            ? t(`asset_${view.code}`)
            : t('thumbnailPending')
      }
    >
      {loaded ? (
        <img
          src={view.url}
          alt={label}
          width={view.width}
          height={view.height}
          onError={() => setState({ owner, view: { kind: 'unavailable', code: 'decode_failed' } })}
        />
      ) : (
        <Icon name="image" />
      )}
    </span>
  );
}

/** 仅依据真实报告显示入口；无收益定位原图，没有备份就不生成“定位备份”。 */
export function JobFileActions({
  job,
  language,
  access,
  target = 'result',
}: {
  job: JobDto;
  language: Language;
  access: RowAssets;
  target?: RevealTarget;
}) {
  const t = workspaceText(language);
  const identity = useIdentity(access.selectionId, job);
  const state = job.state;
  const result = state.kind === 'succeeded' || state.kind === 'no_gain';
  const backup =
    state.kind === 'succeeded'
      ? state.report.backupName
      : state.kind === 'failed'
        ? state.failure.recovery?.backupName
        : null;
  const owner = useMemo(
    () => ({ identity, enabled: access.enabled, target }),
    [identity, access.enabled, target],
  );
  type Message = 'requested' | ReturnType<typeof assetErrorCode> | null;
  const [notice, setNotice] = useState<{
    owner: typeof owner;
    pending: boolean;
    message: Message;
  } | null>(null);
  const pending = notice?.owner === owner && notice.pending;
  const message = notice?.owner === owner ? notice.message : null;
  const lifetime = useRef<{ active: boolean } | null>(null);
  useEffect(() => {
    const token = { active: true };
    lifetime.current = token;
    return () => {
      token.active = false;
    };
  }, [owner]);
  const available = target === 'result' ? result : Boolean(backup);
  if (target === 'backup' && !backup) return null;
  const reveal = async () => {
    const token = lifetime.current;
    if (!identity || !available || pending || !access.enabled || !token?.active) return;
    setNotice({ owner, pending: true, message: null });
    let message: Message;
    try {
      await access.assets.reveal(identity, target);
      message = 'requested';
    } catch (error) {
      message = assetErrorCode(error);
    }
    if (token.active) setNotice({ owner, pending: false, message });
  };
  return (
    <div className={styles.fileActions}>
      <button
        className="text-button"
        disabled={!available || !access.enabled || pending}
        title={
          target === 'backup'
            ? t('revealBackup')
            : state.kind === 'no_gain'
              ? t('revealOriginal')
              : t('revealResult')
        }
        onClick={() => {
          void reveal();
        }}
      >
        {target === 'backup' ? t('revealBackup') : t('viewInFolder')}
      </button>
      <output className={styles.actionNotice}>
        {pending
          ? t('revealPending')
          : message === 'requested'
            ? t('revealRequested')
            : message
              ? t(`asset_${message}`)
              : ''}
      </output>
    </div>
  );
}
