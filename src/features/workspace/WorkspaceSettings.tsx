import { useEffect, useRef, useState, useSyncExternalStore, type ReactNode } from 'react';
import { Icon } from '../../components/ui/Icon';
import { draftMode, draftSettings, qualityValue, useCompressionPreferences } from './settings';
import type { WorkspaceController } from './controller';
import type { WorkspaceText } from './messages';
import styles from './Workspace.module.css';
import { Name } from './WorkspaceRows';

/** 设置草稿独立于当前批次；尚未支持的参数不发送给后端。 */
export function WorkspaceSettings({
  controller,
  t,
  footer,
  onValidityChange,
}: {
  controller: WorkspaceController;
  t: WorkspaceText;
  footer?: ReactNode;
  onValidityChange: (invalid: boolean) => void;
}) {
  const preferences = useCompressionPreferences();
  const view = useSyncExternalStore(controller.subscribe, controller.getSnapshot);
  const initialized = useRef(false);
  const [quality, setQuality] = useState(String(preferences.quality));
  const lossless = preferences.mode === 'lossless';
  const invalid = !lossless && qualityValue(quality) === null;
  useEffect(() => onValidityChange(invalid), [invalid, onValidityChange]);
  const directoryId = view.outputDirectory?.directoryId;
  const { mode, output, backupBeforeOverwrite, customOutput, preserveStructure } = preferences;
  useEffect(() => {
    controller.setSettings(
      draftSettings(
        mode,
        quality,
        output,
        backupBeforeOverwrite,
        customOutput ? (directoryId ? { directoryId, preserveStructure } : null) : undefined,
      ),
      initialized.current,
      draftMode(mode, quality),
    );
    initialized.current = true;
  }, [
    controller,
    mode,
    quality,
    output,
    backupBeforeOverwrite,
    customOutput,
    preserveStructure,
    directoryId,
  ]);
  const current = qualityValue(quality) ?? preferences.quality;
  const level =
    current < 30
      ? 'qualityLow'
      : current < 50
        ? 'qualityLower'
        : current < 70
          ? 'qualityMedium'
          : current < 85
            ? 'qualityHigh'
            : current < 95
              ? 'qualityHigher'
              : 'qualityHighest';
  const changeQuality = (value: string) => {
    setQuality(value);
    const parsed = qualityValue(value);
    if (parsed !== null) preferences.setQuality(parsed);
  };
  return (
    <aside className={styles.settings} aria-labelledby="settingsTitle">
      <div className={styles.settingsScroll}>
        <h2 id="settingsTitle">{t('settings')}</h2>
        <fieldset className={styles.mode} aria-label={t('mode')}>
          {(['lossy', 'lossless'] as const).map((mode) => (
            <button
              key={mode}
              aria-pressed={preferences.mode === mode}
              onClick={() => {
                preferences.setMode(mode);
              }}
            >
              {t(mode)}
            </button>
          ))}
        </fieldset>
        <section className={styles.quality} aria-labelledby="qualityTitle">
          <div className={styles.qualityHeading}>
            <h3 id="qualityTitle">{t('quality')}</h3>
            <span className={styles.qualityDescription}>{t(lossless ? 'lossless' : level)}</span>
          </div>
          <div className={styles.qualityControls} data-disabled={lossless}>
            <div className={styles.qualityRail}>
              <progress className={styles.railTrack} value={current} max={100} aria-hidden="true" />
              <input
                className={styles.qualityRange}
                type="range"
                aria-label={t('quality')}
                aria-describedby="qualityHint"
                min="0"
                max="100"
                step="1"
                value={current}
                disabled={lossless}
                onChange={(event) => changeQuality(event.target.value)}
              />
            </div>
            <div className={styles.numericRow}>
              <label htmlFor="qualityInput">{t('fineTune')}</label>
              <input
                className="field"
                id="qualityInput"
                type="number"
                min="0"
                max="100"
                step="1"
                inputMode="numeric"
                autoComplete="off"
                aria-label={t('fineTune')}
                aria-describedby="qualityHint"
                aria-invalid={invalid}
                value={quality}
                disabled={lossless}
                onChange={(event) => changeQuality(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Escape') changeQuality(String(preferences.quality));
                }}
              />
              <span>0–100</span>
            </div>
          </div>
          <p id="qualityHint" className={invalid ? styles.invalid : styles.qualityHelp}>
            {t(invalid ? 'invalidQuality' : lossless ? 'losslessHint' : 'qualityHint')}
          </p>
        </section>
        <section className={styles.output} aria-labelledby="outputTitle">
          <h3 id="outputTitle">{t('output')}</h3>
          <fieldset className={styles.mode} aria-labelledby="outputTitle">
            {(['overwrite', 'copy_beside'] as const).map((output) => (
              <button
                key={output}
                aria-pressed={preferences.output === output}
                onClick={() => {
                  preferences.setOutput(output);
                }}
              >
                {t(output)}
              </button>
            ))}
          </fieldset>
          {preferences.output === 'copy_beside' && (
            <div className={styles.outputDirectory}>
              <span>{t('outputDirectory')}</span>
              <div className={styles.directoryControl}>
                <div>
                  <Icon name="folder" />
                  <span>
                    {customOutput ? (
                      view.outputDirectory ? (
                        <Name value={view.outputDirectory.name} t={t} />
                      ) : (
                        t('outputRequired')
                      )
                    ) : (
                      t('originalFolder')
                    )}
                  </span>
                </div>
                <button
                  disabled={!controller.canChange || view.confirmationOpen}
                  onClick={() =>
                    void controller.selectOutputDirectory(() => preferences.setCustomOutput(true))
                  }
                >
                  {t('chooseOutput')}
                </button>
              </div>
              {customOutput && (
                <button
                  className="text-button"
                  disabled={!controller.canChange || view.confirmationOpen}
                  onClick={() =>
                    void controller.useOriginalFolder(() => preferences.setCustomOutput(false))
                  }
                >
                  {t('useOriginalFolder')}
                </button>
              )}
              <p className={customOutput && !directoryId ? styles.invalid : 'hint'}>
                {t(customOutput && !directoryId ? 'outputRequiredHint' : 'settingsHint')}
              </p>
            </div>
          )}
        </section>
        <section className={styles.advanced} aria-labelledby="advancedTitle">
          <h3 id="advancedTitle">{t('advanced')}</h3>
          <div className={styles.advancedBody}>
            {preferences.output === 'overwrite' && (
              <label className={styles.backupOption}>
                <input
                  type="checkbox"
                  checked={preferences.backupBeforeOverwrite}
                  aria-describedby="backupHint"
                  onChange={(event) => {
                    const backup = event.target.checked;
                    preferences.setBackupBeforeOverwrite(backup);
                  }}
                />
                {t('backupBeforeOverwrite')}
              </label>
            )}
            {output === 'copy_beside' && customOutput && (
              <label className={styles.backupOption}>
                <input
                  type="checkbox"
                  checked={preserveStructure}
                  onChange={(event) => preferences.setPreserveStructure(event.target.checked)}
                />
                {t('preserveStructure')}
              </label>
            )}
            <p id="backupHint">
              {t(
                preferences.output === 'copy_beside'
                  ? 'copyHint'
                  : preferences.backupBeforeOverwrite
                    ? 'backupHint'
                    : 'overwriteHint',
              )}
            </p>
            {output === 'copy_beside' && !customOutput && (
              <p className="hint">{t('copyOriginalHint')}</p>
            )}
            <p>{t('frozen')}</p>
          </div>
        </section>
      </div>
      {footer && <div className={styles.settingsBottom}>{footer}</div>}
    </aside>
  );
}
