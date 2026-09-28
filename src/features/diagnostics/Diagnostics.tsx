import { useEffect, useState } from 'react';
import { getLogStatus, openLogDirectory } from '../../lib/ipc/diagnostics';
import type { LogStatus } from '../../lib/ipc/tasks.generated';
import type { Language } from '../../stores/preferences';
import styles from './Diagnostics.module.css';

const messages = {
  'zh-CN': {
    title: '诊断日志',
    open: '打开日志目录',
    refresh: '刷新状态',
    loading: '正在读取日志状态…',
    preview: '浏览器预览不写入日志，请在桌面版查看。',
    description:
      '仅本机保存，最多 10 份，每份不超过 5 MB。正式版日志位于系统临时目录，可能被系统清理。',
    privacy: '不记录图片内容、原图文件名或完整路径。日志写入失败不影响压缩。',
    error: '无法读取日志状态或打开目录，请重试。',
    dropped: '丢失事件',
    failures: '写入失败',
    states: {
      starting: '正在启动',
      ready: '正在记录',
      unavailable: '日志不可用',
      busy: '日志目录忙碌，暂时停止写入',
      stopped: '已停止记录',
    },
  },
  en: {
    title: 'Diagnostic logs',
    open: 'Open log folder',
    refresh: 'Refresh status',
    loading: 'Reading log status…',
    preview: 'Browser preview does not write logs. Use the desktop app.',
    description:
      'Local only: up to 10 files, 5 MB each. Release logs use the system temporary folder and may be cleared by the system.',
    privacy:
      'Image contents, source filenames and full paths are not logged. Logging failures do not stop compression.',
    error: 'Could not read log status or open the folder. Please retry.',
    dropped: 'Dropped events',
    failures: 'Write failures',
    states: {
      starting: 'Starting',
      ready: 'Recording',
      unavailable: 'Logs unavailable',
      busy: 'Log folder busy; writing paused',
      stopped: 'Stopped',
    },
  },
};

export function Diagnostics({ language }: { language: Language }) {
  const text = messages[language];
  const [status, setStatus] = useState<LogStatus | null>();
  const [attempt, setAttempt] = useState(0);
  const [error, setError] = useState(false);
  const [loading, setLoading] = useState(true);
  const [opening, setOpening] = useState(false);
  useEffect(() => {
    let active = true;
    void getLogStatus()
      .then((value) => {
        if (active) {
          setStatus(value);
          setError(false);
        }
      })
      .catch(() => {
        if (active) {
          setError(true);
          setStatus(undefined);
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [attempt]);
  async function open() {
    setOpening(true);
    setError(false);
    try {
      await openLogDirectory();
    } catch {
      setError(true);
    } finally {
      setOpening(false);
    }
  }
  return (
    <section className={styles.panel} aria-label={text.title}>
      <h3>{text.title}</h3>
      <p className="hint">{text.description}</p>
      <p className="hint">{text.privacy}</p>
      <output>
        {loading
          ? text.loading
          : status === null
            ? text.preview
            : status
              ? text.states[status.state]
              : ''}
      </output>
      {status && (
        <p className="hint">
          {text.dropped}: {status.droppedEvents} · {text.failures}: {status.writeFailures}
        </p>
      )}
      {error && <p role="alert">{text.error}</p>}
      <div className={styles.actions}>
        <button
          className="button"
          disabled={loading || opening || !status?.canOpen}
          onClick={() => void open()}
        >
          {text.open}
        </button>
        <button
          className="text-button"
          disabled={loading || opening || status === null}
          onClick={() => {
            setLoading(true);
            setAttempt((value) => value + 1);
          }}
        >
          {text.refresh}
        </button>
      </div>
    </section>
  );
}
