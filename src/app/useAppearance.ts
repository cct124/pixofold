import { useEffect, useSyncExternalStore } from 'react';
import { usePreferences } from '../stores/preferences';
import type { Theme } from '../stores/preferences';

const darkQuery = '(prefers-color-scheme: dark)';

function subscribe(onChange: () => void) {
  const media = window.matchMedia(darkQuery);
  media.addEventListener('change', onChange);
  return () => media.removeEventListener('change', onChange);
}

function systemTheme(): Theme {
  return window.matchMedia(darkQuery).matches ? 'dark' : 'light';
}

export function useAppearance() {
  const preference = usePreferences((state) => state.theme);
  const language = usePreferences((state) => state.language);
  const system = useSyncExternalStore<Theme>(subscribe, systemTheme, () => 'light');
  const theme = preference === 'system' ? system : preference;

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.lang = language;
  }, [theme, language]);

  return { theme, language };
}
