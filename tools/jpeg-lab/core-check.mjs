// JPEG实际核心回归入口；工具或构建身份缺失必须失败，不能静默跳过原生测试。
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { createSamples, extraMarkers } from './experiment.mjs';
import { runLossyChecks } from './lossy-check.mjs';

export function runCoreChecks({ root, build, tools, source }) {
  const hash = createHash('sha256').update(readFileSync(tools.helper)).digest('hex');
  assert.equal(hash, readFileSync(path.join(build, 'helper.sha256'), 'utf8').trim());
  mkdirSync(path.join(root, 'target'), { recursive: true });
  const directory = mkdtempSync(path.join(root, 'target', 'jpeg-lab-core-'));
  const { samples } = createSamples({ root, tools, directory });
  for (const [name, bytes] of samples) {
    writeFileSync(path.join(directory, name + '.jpg'), bytes, { flag: 'wx' });
  }
  const execute = (file, args, input) => {
    const result = spawnSync(file, args, {
      input,
      timeout: 30000,
      maxBuffer: 64 * 1024 * 1024,
      windowsHide: true,
    });
    if (result.error) throw result.error;
    assert.equal(result.status, 0, '自生成样本的原生验证失败');
    return result.stdout;
  };
  for (const [name, original] of samples.filter(([name]) => name !== 'opaque-app11')) {
    const output = execute(
      tools.helper,
      ['optimize', '16384', '16777216', '134217728', '64'],
      original,
    );
    assert.deepEqual(
      execute(tools.coefficients, [], output),
      execute(tools.coefficients, [], original),
      name + '独立系数复验',
    );
    assert.deepEqual(
      execute(tools.djpeg, ['-strict', '-ppm'], output),
      execute(tools.djpeg, ['-strict', '-ppm'], original),
      name + '独立像素复验',
    );
    assert.deepEqual(extraMarkers(output), extraMarkers(original), name + '独立元数据复验');
  }
  const run = spawnSync(
    'cargo',
    [
      'run',
      '--locked',
      '-p',
      'pixofold-core',
      '--example',
      'jpeg_core_check',
      '--',
      path.dirname(tools.helper),
      hash,
      directory,
    ],
    { cwd: root, stdio: 'inherit', windowsHide: true, timeout: 180000 },
  );
  if (run.error) throw run.error;
  assert.equal(run.status, 0, 'JPEG核心真实引擎/安全输出检查失败');
  runLossyChecks({ root, tools, directory, hash, source });
  const mixed = spawnSync(
    'cargo',
    [
      'run',
      '--locked',
      '-p',
      'pixofold-core',
      '--example',
      'jpeg_mixed_check',
      '--',
      path.dirname(tools.helper),
      hash,
      directory,
    ],
    { cwd: root, stdio: 'inherit', windowsHide: true, timeout: 180000 },
  );
  if (mixed.error) throw mixed.error;
  assert.equal(mixed.status, 0, 'PNG/JPEG真实混合批次检查失败');
  const mixedChecks = JSON.parse(
    readFileSync(path.join(directory, 'mixed-results', 'checks.json'), 'utf8'),
  );
  for (const entry of mixedChecks) {
    const original = readFileSync(entry.source);
    const output = readFileSync(entry.output);
    assert.deepEqual(extraMarkers(output), extraMarkers(original), '混合批次独立元数据复验');
    assert.ok(execute(tools.djpeg, ['-strict', '-ppm'], output).length > 0, '混合候选完整解码');
    if (entry.lossless) {
      assert.deepEqual(
        execute(tools.coefficients, [], output),
        execute(tools.coefficients, [], original),
        '混合批次独立系数复验',
      );
    }
  }
  writeFileSync(
    path.join(directory, 'core-report.json'),
    JSON.stringify(
      {
        helperSha256: hash,
        platform: process.platform,
        samples: samples.length,
        mixedOutputsVerified: mixedChecks.length,
        result: 'passed',
      },
      null,
      2,
    ) + '\n',
    { flag: 'wx' },
  );
  console.log('JPEG核心验证记录：' + directory);
}
