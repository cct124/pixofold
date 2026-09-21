import { create } from 'zustand';
import { createJSONStorage, persist } from 'zustand/middleware';

export type Theme = 'light' | 'dark';
export type Language = 'zh-CN' | 'en';
type ThemePreference = Theme | 'system';

interface Preferences {
  theme: ThemePreference;
  language: Language;
  setTheme: (theme: ThemePreference) => void;
  setLanguage: (language: Language) => void;
}

export const usePreferences = create<Preferences>()(
  persist(
    (set) => ({
      theme: 'system',
      language: 'zh-CN',
      setTheme: (theme) => set({ theme }),
      setLanguage: (language) => set({ language }),
    }),
    {
      name: 'pixofold.preferences',
      version: 1,
      storage: createJSONStorage(() => localStorage),
      partialize: ({ theme, language }) => ({ theme, language }),
      merge: (persisted, current) => {
        if (!persisted || typeof persisted !== 'object') return current;
        const theme = 'theme' in persisted ? persisted.theme : undefined;
        const language = 'language' in persisted ? persisted.language : undefined;
        return {
          ...current,
          theme: theme === 'light' || theme === 'dark' || theme === 'system' ? theme : 'system',
          language: language === 'en' || language === 'zh-CN' ? language : 'zh-CN',
        };
      },
    },
  ),
);
