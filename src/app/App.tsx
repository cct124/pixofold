import { useEffect, useState } from 'react';
import logo from '../assets/logo.svg';
import { ThemeSwitch } from '../components/ui/ThemeSwitch';
import { Workspace } from '../features/workspace/Workspace';
import { getAppInfo } from '../lib/ipc/app';
import { usePreferences } from '../stores/preferences';
import { messages } from './messages';
import { useAppearance } from './useAppearance';
import styles from './App.module.css';

export function App() {
  const { theme, language } = useAppearance();
  const setTheme = usePreferences((state) => state.setTheme);
  const setLanguage = usePreferences((state) => state.setLanguage);
  const [version, setVersion] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const text = messages[language];
  useEffect(() => {
    let ignore = false;
    getAppInfo()
      .then((info) => {
        if (!ignore) {
          setVersion(info?.version ?? null);
          setFailed(false);
        }
      })
      .catch(() => {
        if (!ignore) setFailed(true);
      });
    return () => {
      ignore = true;
    };
  }, [attempt]);

  return (
    <div className={styles.app}>
      <header className={styles.header}>
        <div className={styles.brand}>
          <img src={logo} alt="" width="36" height="36" />
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
      <Workspace language={language} />
      <footer className={styles.footer}>
        <span>{text.local}</span>
        <div>
          {failed ? (
            <button onClick={() => setAttempt((value) => value + 1)}>{text.retry}</button>
          ) : (
            version && <span>v{version}</span>
          )}
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
