import { describe, expect, it } from 'vitest';
import { draftSettings, qualityValue, useCompressionPreferences } from './settings';
import { formatBytes, formatReduction } from './format';

describe('workspace settings and exact display', () => {
  it('derives reduction only from known nonzero exact byte counts', () => {
    expect(formatReduction(null, '1')).toBe('—');
    expect(formatReduction('10', null)).toBe('—');
    expect(formatReduction('0', '0')).toBe('—');
    expect(formatReduction('10', '10')).toBe('0.0%');
    expect(formatReduction('1000', '751')).toBe('24.9%');
    expect(formatReduction('90071992547409930', '45035996273704965')).toBe('50.0%');
  });
  it('accepts only complete integer text and drops quality in lossless mode', () => {
    for (const text of ['', ' ', '01', '-1', '101', '1.5', '1e2', '+80', '80 '])
      expect(qualityValue(text)).toBeNull();
    for (const value of [0, 80, 100]) expect(qualityValue(String(value))).toBe(value);
    expect(draftSettings('lossy', '', 'overwrite')).toBeNull();
    expect(draftSettings('lossless', '', 'copy_beside')).toEqual({
      mode: { kind: 'lossless' },
      output: 'copy_beside',
    });
  });
  it('persists only validated settings, never drafts or task state', async () => {
    localStorage.setItem(
      'pixofold.compression',
      JSON.stringify({
        version: 1,
        state: { mode: 'invalid', quality: 100.5, output: '/private/path', selectionId: '123' },
      }),
    );
    await useCompressionPreferences.persist.rehydrate();
    const state = useCompressionPreferences.getState();
    expect(state).toMatchObject({ mode: 'lossy', quality: 80, output: 'overwrite' });
    state.setQuality(Number.NaN);
    state.setMode('lossless');
    expect(JSON.parse(localStorage.getItem('pixofold.compression') ?? '{}').state).toEqual({
      mode: 'lossless',
      quality: 80,
      output: 'overwrite',
    });
  });
  it('keeps unknown distinct from zero and formats full u64 values without Number conversion', () => {
    expect(formatBytes(null)).toBe('—');
    expect(formatBytes('0')).toBe('0 B');
    expect(formatBytes('1024')).toBe('1.00 KiB');
    expect(formatBytes('18446744073709551615')).toBe('15.99 EiB');
  });
});
