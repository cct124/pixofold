import { create } from 'zustand';
import { createJSONStorage, persist } from 'zustand/middleware';
import type { DirectoryTarget, TaskSettingsDto } from '../../lib/ipc/tasks.generated';

export interface CompressionPreferences {
  mode: 'lossy' | 'lossless';
  quality: number;
  output: 'overwrite' | 'copy_beside';
  backupBeforeOverwrite: boolean;
  customOutput: boolean;
  preserveStructure: boolean;
  setMode: (mode: 'lossy' | 'lossless') => void;
  setQuality: (quality: number) => void;
  setOutput: (output: CompressionPreferences['output']) => void;
  setBackupBeforeOverwrite: (backup: boolean) => void;
  setCustomOutput: (custom: boolean) => void;
  setPreserveStructure: (preserve: boolean) => void;
}

/** 输入暂态不持久化；只接受完整的0–100整数文本，不截断、不四舍五入。 */
export function qualityValue(text: string): number | null {
  if (!/^(0|[1-9][0-9]?|100)$/.test(text)) return null;
  return Number(text);
}

/** 普通重试只更新编码参数，不依赖新草稿的目录授权。 */
export function draftMode(
  mode: CompressionPreferences['mode'],
  quality: string,
): TaskSettingsDto['mode'] | null {
  if (mode === 'lossless') return { kind: 'lossless' };
  const value = qualityValue(quality);
  return value === null ? null : { kind: 'lossy', quality: value };
}

export function draftSettings(
  mode: CompressionPreferences['mode'],
  quality: string,
  outputMode: CompressionPreferences['output'],
  backupBeforeOverwrite = false,
  directory?: DirectoryTarget | null,
): TaskSettingsDto | null {
  // 记忆“指定目录”意图但不持久化授权；失效时只扫描，绝不回退到同目录/覆盖。
  if (outputMode === 'copy_beside' && directory === null) return null;
  const output =
    outputMode === 'copy_beside'
      ? directory
        ? { copy_to: directory }
        : 'copy_beside'
      : backupBeforeOverwrite
        ? 'overwrite'
        : 'overwrite_without_backup';
  const parameters = draftMode(mode, quality);
  return parameters === null ? null : { mode: parameters, output };
}

/** 与外观偏好分离的v1压缩设置；不持久化任务、路径授权或无效输入草稿。 */
export const useCompressionPreferences = create<CompressionPreferences>()(
  persist(
    (set) => ({
      mode: 'lossy',
      quality: 80,
      output: 'overwrite',
      backupBeforeOverwrite: false,
      customOutput: false,
      preserveStructure: false,
      setMode: (mode) => set({ mode }),
      setQuality: (quality) => {
        if (Number.isInteger(quality) && quality >= 0 && quality <= 100) set({ quality });
      },
      setOutput: (output) => set({ output }),
      setBackupBeforeOverwrite: (backupBeforeOverwrite) => set({ backupBeforeOverwrite }),
      setCustomOutput: (customOutput) => set({ customOutput }),
      setPreserveStructure: (preserveStructure) => set({ preserveStructure }),
    }),
    {
      name: 'pixofold.compression',
      version: 1,
      storage: createJSONStorage(() => localStorage),
      partialize: ({
        mode,
        quality,
        output,
        backupBeforeOverwrite,
        customOutput,
        preserveStructure,
      }) => ({
        mode,
        quality,
        output,
        backupBeforeOverwrite,
        customOutput,
        preserveStructure,
      }),
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
          backupBeforeOverwrite:
            'backupBeforeOverwrite' in persisted && persisted.backupBeforeOverwrite === true,
          customOutput: 'customOutput' in persisted && persisted.customOutput === true,
          preserveStructure:
            'preserveStructure' in persisted && persisted.preserveStructure === true,
        };
      },
    },
  ),
);
