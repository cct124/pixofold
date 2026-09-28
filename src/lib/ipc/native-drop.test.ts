import { describe, expect, it } from 'vitest';
import { nativeDropNotice } from './native-drop';
import { TASK_PROTOCOL_VERSION, MAX_NATIVE_IMPORT_ROOTS } from './tasks.generated';

const value = () => ({
  kind: 'native_drop',
  protocolVersion: TASK_PROTOCOL_VERSION,
  subscriptionId: '9007199254740993',
  offer: { offerId: '8', grant: { grantId: '8', rootCount: 1 } },
  position: { x: 200, y: 180 },
});

describe('native drop wire validation', () => {
  it('preserves lossless identities and distinguishes rejected input from a valid grant', () => {
    expect(nativeDropNotice(value())).toEqual(value());
    expect(
      nativeDropNotice({ ...value(), offer: { offerId: '8', grant: null } }).offer.grant,
    ).toBeNull();
  });
  it('rejects old protocols, invalid coordinates, missing grants and unbounded root counts', () => {
    for (const invalid of [
      { ...value(), protocolVersion: TASK_PROTOCOL_VERSION - 1 },
      { ...value(), subscriptionId: 1 },
      { ...value(), subscriptionId: '01' },
      { ...value(), position: { x: Infinity, y: 0 } },
      { ...value(), position: { x: 1, y: NaN } },
      { ...value(), offer: { offerId: '8' } },
      { ...value(), offer: { offerId: '0', grant: null } },
      { ...value(), offer: { offerId: '8', grant: { grantId: '9', rootCount: 1 } } },
      { ...value(), offer: { offerId: '8', grant: { grantId: '8', rootCount: 0 } } },
      {
        ...value(),
        offer: { offerId: '8', grant: { grantId: '8', rootCount: MAX_NATIVE_IMPORT_ROOTS + 1 } },
      },
    ])
      expect(() => nativeDropNotice(invalid)).toThrow();
  });
});
