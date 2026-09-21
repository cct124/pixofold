import { useEffect, useState } from 'react';
import logo from '../assets/logo.svg';
import { ThemeSwitch } from '../components/ui/ThemeSwitch';
import { getAppInfo } from '../lib/ipc/app';
import type { AppInfo } from '../lib/ipc/generated';
import { usePreferences } from '../stores/preferences';
import { messages } from './messages';
import { useAppearance } from './useAppearance';
import styles from './App.module.css';

type Connection = { status: 'loading' | 'browser' | 'failed' } | { status: 'ready'; info: AppInfo };

export function App() {
  const { theme, language } = useAppearance();
  const setTheme = usePreferences((state) => state.setTheme);
  const setLanguage = usePreferences((state) => state.setLanguage);
  const [connection, setConnection] = useState<Connection>({ status: 'loading' });
  const [attempt, setAttempt] = useState(0);
  const text = messages[language];

  useEffect(() => {
    let ignore = false;
    getAppInfo()
      .then((info) => {
        if (!ignore) setConnection(info ? { status: 'ready', info } : { status: 'browser' });
      })
      .catch(() => {
        if (!ignore) setConnection({ status: 'failed' });
      });
    return () => {
      ignore = true;
    };
  }, [attempt]);

  const hint =
    connection.status === 'ready'
      ? text.readyHint
      : connection.status === 'failed'
        ? text.failedHint
        : connection.status === 'browser'
          ? text.browserHint
          : text.loading;

  return (
    <div className={styles.app}>
      <header className={styles.header}>
        <div className={styles.brand}>
          <img src={logo} alt="" width="40" height="40" />
          <div>
            <strong>PixoFold · 轻图</strong>
            <span>{text.subtitle}</span>
          </div>
        </div>
        <div className={styles.headerActions}>
          <span className={styles.badge}>{text.badge}</span>
          <ThemeSwitch theme={theme} label={text.theme} labels={text} onChange={setTheme} />
        </div>
      </header>
      <main className={styles.main}>
        <section className={styles.intro} aria-labelledby="welcomeTitle">
          <img className={styles.heroLogo} src={logo} alt="" width="86" height="86" />
          <h1 id="welcomeTitle">{text.heading}</h1>
          <p className={styles.description}>{text.description}</p>
        </section>
        <section className={styles.statusCard} aria-label={text.badge}>
          <output className={styles.connection}>
            <span className={styles.dot} data-status={connection.status} aria-hidden="true" />
            <strong>{text[connection.status]}</strong>
            {connection.status === 'ready' && (
              <span className={styles.version}>v{connection.info.version}</span>
            )}
          </output>
          <p className={styles.hint}>{hint}</p>
          {connection.status === 'failed' && (
            <button
              className={styles.retry}
              onClick={() => {
                setConnection({ status: 'loading' });
                setAttempt((value) => value + 1);
              }}
            >
              {text.retry}
            </button>
          )}
          {connection.status === 'ready' && (
            <div className={styles.formats}>
              <span>{text.plannedFormats}</span>
              <ul aria-label={text.plannedFormats}>
                {connection.info.plannedFormats.map((format) => (
                  <li key={format}>{format.toUpperCase()}</li>
                ))}
              </ul>
            </div>
          )}
          <div className={styles.nextStep}>
            <strong>{text.scaffold}</strong>
            <p>{text.scaffoldHint}</p>
          </div>
        </section>
      </main>
      <footer className={styles.footer}>
        <span>{text.local}</span>
        <div>
          <span>{text.license}</span>
          <select
            aria-label={text.language}
            value={language}
            onChange={(event) => setLanguage(event.target.value === 'en' ? 'en' : 'zh-CN')}
          >
            <option value="zh-CN">简体中文</option>
            <option value="en">English</option>
          </select>
        </div>
      </footer>
    </div>
  );
}
