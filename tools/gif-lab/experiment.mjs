// 使用真实Gifsicle优化，再由Rust gif/weezl独立复验双背景/循环时间轴；不提交产品文件。
import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import {
  root,
  target,
  source,
  engine,
  binary,
  build,
  execute,
  digest,
  verifyTools,
} from './build.mjs';
import { writeFixtures, stressFixture } from './fixtures.mjs';

export function runExperiment() {
  const identity = verifyTools();
  writeFixtures(path.join(root, 'tests/fixtures/gif'), true);
  execute(
    'cargo',
    ['build', '--release', '--locked', '-p', 'pixofold-core', '--example', 'gif_validate'],
    { stdio: 'inherit' },
  );
  const validator = path.join(
    root,
    'target/release/examples/gif_validate' + (process.platform === 'win32' ? '.exe' : ''),
  );
  mkdirSync(path.join(root, 'target'), { recursive: true });
  const directory = mkdtempSync(path.join(root, 'target/gif-lab-'));
  const inputs = path.join(directory, 'inputs');
  cpSync(path.join(root, 'tests/fixtures/gif'), inputs, { recursive: true });
  const manifestFile = path.join(inputs, 'manifest.json');
  const manifest = JSON.parse(readFileSync(manifestFile, 'utf8'));
  const corpus = JSON.parse(
    execute(validator, ['corpus', inputs, manifestFile], { encoding: 'utf8' }).stdout,
  );
  assert.equal(corpus.result, 'passed');
  const stress = stressFixture();
  writeFileSync(path.join(inputs, 'stress.gif'), stress, { flag: 'wx' });
  assert.equal(
    JSON.parse(
      execute(validator, ['inspect', path.join(inputs, 'stress.gif')], { encoding: 'utf8' }).stdout,
    ).result,
    'accepted',
  );
  const outputs = path.join(directory, 'outputs');
  mkdirSync(outputs);
  const records = [];
  for (const sample of [
    ...manifest.samples.filter((sample) => sample.expect === 'ok'),
    { file: 'stress.gif', sha256: digest(stress) },
  ]) {
    const input = path.join(inputs, sample.file);
    const original = readFileSync(input);
    for (const policy of ['default', 'careful']) {
      for (const level of ['1', '2', '3']) {
        const label = policy + '-O' + level + '-' + sample.file;
        const output = path.join(outputs, label);
        const run = JSON.parse(
          execute(validator, ['optimize', binary, input, output, level, policy], {
            encoding: 'utf8',
          }).stdout,
        );
        const comparison = JSON.parse(
          execute(validator, ['compare', input, output], { encoding: 'utf8' }).stdout,
        );
        const candidate = readFileSync(output);
        assert.equal(digest(readFileSync(input)), sample.sha256, '原始输入被改变');
        if (!comparison.equivalent) {
          assert.ok(
            policy === 'default' &&
              level !== '1' &&
              ['larger-pattern.gif', 'stress.gif'].includes(sample.file) &&
              comparison.candidateRejected === 'InvalidGif' &&
              comparison.reason === '透明索引越界',
            '出现未解释的结构/播放变化',
          );
        }
        records.push({
          file: sample.file,
          level: Number(level),
          policy,
          inputBytes: original.length,
          outputBytes: candidate.length,
          outcome: !comparison.equivalent
            ? 'validation_rejected'
            : candidate.length < original.length
              ? 'gain'
              : 'no_gain',
          ...comparison,
          ...run,
          inputSha256: sample.sha256,
          outputSha256: digest(candidate),
        });
      }
    }
  }
  const summaries = [];
  for (const policy of ['default', 'careful']) {
    for (const level of [1, 2, 3]) {
      const rows = records.filter((row) => row.policy === policy && row.level === level);
      summaries.push({
        policy,
        level,
        gain: rows.filter((row) => row.outcome === 'gain').length,
        noGain: rows.filter((row) => row.outcome === 'no_gain').length,
        validationRejected: rows.filter((row) => row.outcome === 'validation_rejected').length,
        elapsedMs: rows.reduce((sum, row) => sum + row.elapsedMs, 0),
        sampledPeakRssBytes: Math.max(...rows.map((row) => row.sampledPeakRssBytes)),
      });
    }
  }
  // 完整保守子集须至少有一种配方通过所有人工语料，否则不推荐生产参数。
  const safeRecipes = summaries.filter((row) => row.validationRejected === 0);
  assert.ok(
    safeRecipes.some((row) => row.policy === 'careful' && row.level === 2),
    '保守O2未通过完整语料',
  );
  for (const notice of ['COPYING', 'README.md'])
    cpSync(path.join(source, notice), path.join(directory, 'Gifsicle-' + notice));
  for (const notice of ['gif-LICENSE-MIT', 'weezl-LICENSE-MIT'])
    cpSync(path.join(root, 'tools/gif-lab/licenses', notice), path.join(directory, notice));
  cpSync(path.join(build, 'identity.json'), path.join(directory, 'engine-identity.json'));
  const report = {
    result: safeRecipes.length ? 'passed' : 'boundary_found',
    engine,
    target,
    validatorSha256: digest(readFileSync(validator)),
    toolSha256: identity.binarySha256,
    corpus,
    cases: records.length,
    records,
    summaries,
    safeRecipes,
    sourceUnchanged: true,
    limits: {
      maxInputMiB: 16,
      maxCanvasPixels: 4194304,
      maxFrames: 256,
      maxTotalDecodedMiB: 64,
      timeoutSeconds: 10,
    },
    playbackScope:
      'exact centiseconds; transparent and logical backgrounds; first cycle and carrying canvas across loop seam; no browser delay clamping claim',
    resourceScope:
      'bounded validation and child deadline; sampled Gifsicle working set, not RSS hard quota; no production scheduler',
    sourceArchive: engine.archive,
  };
  writeFileSync(
    path.join(directory, 'report.json'),
    JSON.stringify(report, null, 2) + String.fromCharCode(10),
  );
  for (const summary of summaries) console.log(JSON.stringify(summary));
  console.log('GIF无损实验记录：' + directory);
}
