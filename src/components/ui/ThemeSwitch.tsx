import type { Theme } from '../../stores/preferences';
import styles from './ThemeSwitch.module.css';

interface ThemeSwitchProps {
  theme: Theme;
  label: string;
  labels: { light: string; dark: string };
  onChange: (theme: Theme) => void;
}

export function ThemeSwitch({ theme, label, labels, onChange }: ThemeSwitchProps) {
  return (
    <fieldset className={styles.switch} aria-label={label}>
      {(['light', 'dark'] as const).map((choice) => (
        <button
          key={choice}
          type="button"
          aria-label={labels[choice]}
          title={labels[choice]}
          aria-pressed={theme === choice}
          onClick={() => onChange(choice)}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
            {choice === 'light' ? (
              <>
                <circle cx="12" cy="12" r="4" />
                <path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5m11 11L19 19M5 19l1.5-1.5m11-11L19 5" />
              </>
            ) : (
              <path d="M20.5 14A8.5 8.5 0 0 1 10 3.5 8.5 8.5 0 1 0 20.5 14Z" />
            )}
          </svg>
        </button>
      ))}
    </fieldset>
  );
}
