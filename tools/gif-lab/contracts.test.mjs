import assert from 'node:assert/strict';
import test from 'node:test';
import { assertCandidate } from './contracts.mjs';
const metrics = {
  rmse: 1,
  worstIntervalRmse: 2,
  maxChannelError: 10,
  temporalErrorRmse: 1,
  worstTemporalRmse: 2,
  changedPixelFraction: 0.1,
};
const valid = () => ({
  sourceUnchanged: true,
  inputBytes: 10000,
  outputBytes: 8000,
  comparison: {
    hardConstraints: true,
    rejection: null,
    losslessEquivalent: false,
    transparent: { ...metrics },
    logical: { ...metrics },
  },
});
test('有损正例的硬语义、局部瞬态和非有限误差不能静默进入报告', () => {
  const sample = { name: 'photo-pan.gif' };
  assertCandidate(sample, '20', valid());
  for (const mutate of [
    (r) => {
      r.comparison.hardConstraints = false;
    },
    (r) => {
      r.comparison.transparent.maxChannelError = 65;
    },
    (r) => {
      r.comparison.logical.worstTemporalRmse = 7;
    },
    (r) => {
      r.comparison.transparent.rmse = NaN;
    },
    (r) => {
      r.outputBytes = r.inputBytes;
    },
  ]) {
    const run = valid();
    mutate(run);
    assert.throws(() => assertCandidate(sample, '20', run));
  }
});
test('已知透明拒绝必须匹配原因，安全候选和无损基线不得被错记', () => {
  const rejected = valid();
  Object.assign(rejected.comparison, { hardConstraints: false, rejection: 'transparency' });
  assertCandidate({ name: 'transparent-seam.gif' }, 'lossless', rejected);
  assert.throws(() => assertCandidate({ name: 'transparent-seam.gif' }, 'lossless', valid()));
  rejected.comparison.rejection = 'timeline';
  assert.throws(() => assertCandidate({ name: 'transparent-seam.gif' }, 'lossless', rejected));
  assert.throws(() => assertCandidate({ name: 'corpus-test.gif' }, 'lossless', valid()));
});
