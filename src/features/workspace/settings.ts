import { create } from 'zustand';
import { createJSONStorage, persist } from 'zustand/middleware';
import type { TaskOutput, TaskSettingsDto } from '../../lib/ipc/tasks.generated';

export interface CompressionPreferences {
  mode: 'lossy' | 'lossless';
  quality: number;
  output: TaskOutput;
  setMode: (mode: 'lossy' | 'lossless') => void;
  setQuality: (quality: number) => void;
  setOutput: (output: TaskOutput) => void;
}

/** 输入暂态不持久化；只接受完整的0–100整数文本，不截断、不四舍五入。 */
export function qualityValue(text: string): number | null {
  if (!/^(0|[1-9][0-9]?|100)$/.test(text)) return null;
  return Number(text);
}

export function draftSettings(
  mode: CompressionPreferences['mode'],
  quality: string,
  output: TaskOutput,
): TaskSettingsDto | null {
  if (mode === 'lossless') return { mode: { kind: 'lossless' }, output };
  const value = qualityValue(quality);
  return value === null ? null : { mode: { kind: 'lossy', quality: value }, output };
}

/** 与外观偏好分离的v1压缩设置；不持久化任务、路径授权或无效输入草稿。 */
export const useCompressionPreferences = create<CompressionPreferences>()(
  persist(
    (set) => ({
      mode: 'lossy',
      quality: 80,
      output: 'overwrite',
      setMode: (mode) => set({ mode }),
      setQuality: (quality) => {
        if (Number.isInteger(quality) && quality >= 0 && quality <= 100) set({ quality });
      },
      setOutput: (output) => set({ output }),
    }),
    {
      name: 'pixofold.compression',
      version: 1,
      storage: createJSONStorage(() => localStorage),
      partialize: ({ mode, quality, output }) => ({ mode, quality, output }),
      merge: (persisted, current) => {
        if (!persisted || typeof persisted !== 'object') return current;
        const mode = 'mode' in persisted ? persisted.mode : null;
        const quality = 'quality' in persisted ? persisted.quality : null;
        const output = 'output' in persisted ? persisted.output : null;
        return {
          ...current,
          mode: mode === 'lossless' ? 'lossless' : 'lossy',
          quality:
            typeof quality === 'number' &&
            Number.isInteger(quality) &&
            quality >= 0 &&
            quality <= 100
              ? quality
              : 80,
          output: output === 'copy_beside' ? 'copy_beside' : 'overwrite',
        };
      },
    },
  ),
);
