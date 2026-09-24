import { useEffect, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import logo from '../assets/logo.svg';
import { ThemeSwitch } from '../components/ui/ThemeSwitch';
import { Dialog } from '../components/ui/Dialog';
import { Icon } from '../components/ui/Icon';
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
  const [about, setAbout] = useState(false);
  const [windowError, setWindowError] = useState(false);
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
  const windowAction = async (action: 'minimize' | 'toggleMaximize' | 'close') => {
    if (!isTauri()) return;
    try {
      await getCurrentWindow()[action]();
      setWindowError(false);
    } catch {
      setWindowError(true);
    }
  };
  const footer = (
    <footer className={styles.settingsFooter}>
      <Icon name="globe" />
      <select
        aria-label={text.language}
        value={language}
        onChange={(event) => setLanguage(event.target.value === 'en' ? 'en' : 'zh-CN')}
      >
        <option value="zh-CN">简体中文</option>
        <option value="en">English</option>
      </select>
      <span className={styles.divider} />
      <button className="text-button" onClick={() => setAbout(true)}>
        {text.about}
      </button>
    </footer>
  );
  return (
    <div className={styles.app}>
      <header className={styles.header} data-tauri-drag-region>
        <div className={styles.brand}>
          <img src={logo} alt="" width="36" height="36" />
          <span>PixoFold · 轻图</span>
        </div>
        <div className={styles.spacer} data-tauri-drag-region />
        <ThemeSwitch theme={theme} label={text.theme} labels={text} onChange={setTheme} />
        <div className={styles.windowActions}>
          <button
            className="icon-button"
            disabled={!isTauri()}
            aria-label={text.minimize}
            title={text.minimize}
            onClick={() => void windowAction('minimize')}
          >
            <Icon name="minimize" />
          </button>
          <button
            className="icon-button"
            disabled={!isTauri()}
            aria-label={text.maximize}
            title={text.maximize}
            onClick={() => void windowAction('toggleMaximize')}
          >
            <Icon name="maximize" />
          </button>
          <button
            className="icon-button"
            disabled={!isTauri()}
            aria-label={text.close}
            title={text.close}
            onClick={() => void windowAction('close')}
          >
            <Icon name="close" />
          </button>
        </div>
      </header>
      {windowError && (
        <p className={styles.windowError} role="alert">
          {text.windowError}
        </p>
      )}
      <Workspace language={language} settingsFooter={footer} />
      {about && (
        <Dialog title={text.about} closeLabel={text.closeDialog} onClose={() => setAbout(false)}>
          <div className="dialog-body">
            <div className={styles.aboutBrand}>
              <img src={logo} width="64" height="64" alt="" />
              <div>
                <h3>PixoFold · 轻图</h3>
                {failed ? (
                  <button className="text-button" onClick={() => setAttempt((value) => value + 1)}>
                    {text.retry}
                  </button>
                ) : (
                  <p className="hint">{version ? 'v' + version : text.preview}</p>
                )}
              </div>
            </div>
            <p>{text.description}</p>
            <p className="hint">{text.license}</p>
          </div>
        </Dialog>
      )}
    </div>
  );
}
