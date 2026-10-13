// 固定配方的语义/质量预期；已知拒绝需精确匹配，不依赖平台耗时或输出字节一致。
import assert from 'node:assert/strict';
const rejected = new Set(['transparent-illustration.gif', 'transparent-seam.gif']);
const qualitySamples = new Set([
  'public-Rotating_earth_large.gif',
  'public-hopper.gif',
  'photo-pan.gif',
]);
export function assertCandidate(sample, strength, run) {
  const comparison = run.comparison;
  assert.equal(run.sourceUnchanged, true, '源文件必须保持');
  if (rejected.has(sample.name)) {
    assert.equal(comparison.hardConstraints, false, sample.name);
    assert.equal(comparison.rejection, 'transparency', sample.name);
    assert.equal(comparison.losslessEquivalent, false, sample.name);
    return;
  }
  assert.equal(comparison.hardConstraints, true, '非预期硬拒绝：' + sample.name);
  assert.equal(comparison.rejection, null, sample.name);
  if (strength === 'lossless' || strength === '0' || sample.name === 'ui-motion.gif')
    assert.equal(comparison.losslessEquivalent, true, '无损基准变化：' + sample.name);
  for (const metrics of [comparison.transparent, comparison.logical]) {
    for (const key of [
      'rmse',
      'worstIntervalRmse',
      'maxChannelError',
      'temporalErrorRmse',
      'worstTemporalRmse',
      'changedPixelFraction',
    ])
      assert.ok(Number.isFinite(metrics[key]) && metrics[key] >= 0, '非法质量度量：' + key);
    assert.ok(metrics.changedPixelFraction <= 1);
  }
  if (qualitySamples.has(sample.name) && ['10', '20'].includes(strength)) {
    assert.equal(comparison.losslessEquivalent, false, '正例须实际有损');
    assert.ok(run.outputBytes < run.inputBytes, '代表性有损正例应有收益');
    for (const m of [comparison.transparent, comparison.logical])
      assert.ok(
        m.rmse <= 2.5 &&
          m.worstIntervalRmse <= 4 &&
          m.maxChannelError <= 64 &&
          m.temporalErrorRmse <= 3 &&
          m.worstTemporalRmse <= 6,
        '质量正例越界：' + sample.name,
      );
  }
}
