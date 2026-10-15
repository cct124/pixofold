// 显式实验：共享色表候选、独立比较、严格无损基线、完整失败证据；不接生产输出。
import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { root, digest, writeFixtures } from './apng-lab/fixtures.mjs';
import { lossyFixtures } from './apng-lossy-lab/fixtures.mjs';
import { htmlReport } from './apng-lossy-lab/report.mjs';
assert.deepEqual(process.argv.slice(2), ['check'], '参数：check');
function run(command, args) {
  const r = spawnSync(command, args, {
    cwd: root,
    encoding: 'utf8',
    maxBuffer: 16 << 20,
    timeout: 300000,
    windowsHide: true,
  });
  if (r.status !== 0)
    throw new Error([command + '失败', r.stdout, r.stderr].join(String.fromCharCode(10)), {
      cause: r.error,
    });
  return r.stdout;
}
const corpusCount = writeFixtures(true);
console.log(
  run('cargo', [
    'test',
    '--locked',
    '-p',
    'pixofold-core',
    '--features',
    'apng-lossy-lab',
    '--lib',
    'apng::validation::quantization',
  ]),
);
console.log(
  run('cargo', [
    'build',
    '--locked',
    '--release',
    '-p',
    'pixofold-core',
    '--features',
    'apng-lossy-lab',
    '--example',
    'apng_lossy_lab',
    '--example',
    'apng_experiment',
  ]),
);
mkdirSync(path.join(root, 'target'), { recursive: true });
const directory = mkdtempSync(path.join(root, 'target/apng-lossy-'));
const absolute = (file) => path.join(directory, file);
for (const file of ['inputs', 'outputs', 'baselines']) mkdirSync(absolute(file));
cpSync(path.join(root, 'tests/fixtures/apng'), absolute('inputs/generated'), { recursive: true });
cpSync(path.join(root, 'tests/manual-images/apng'), absolute('inputs/public'), { recursive: true });
cpSync(path.join(root, 'tests/manual-images/SOURCES.json'), absolute('SOURCES.json'));
cpSync(path.join(root, 'tests/manual-images/licenses'), absolute('licenses'), { recursive: true });
const read = (file) => JSON.parse(readFileSync(file, 'utf8'));
const sources = read(absolute('SOURCES.json'));
for (const license of sources.license_documents)
  assert.equal(digest(readFileSync(absolute(license.file))), license.sha256);
const samples = read(absolute('inputs/generated/manifest.json')).samples.map((s) => ({
  ...s,
  file: 'inputs/generated/' + s.file,
  group: 'structural',
  expect:
    s.expect !== 'ok'
      ? s.expect
      : s.structure.depth === 16
        ? 'unsupported_depth'
        : s.structure.interlace === 1
          ? 'unsupported_interlace'
          : 'ok',
}));
samples.push(
  ...sources.files
    .filter((s) => s.file.startsWith('apng/'))
    .map((s) => ({
      file: 'inputs/public/' + path.basename(s.file),
      sha256: s.sha256,
      group: 'public',
      expect: s.file === 'apng/mode_palette_alpha.apng' ? 'unsupported_representation' : 'ok',
    })),
);
mkdirSync(absolute('inputs/synthetic'));
for (const sample of lossyFixtures()) {
  const file = 'inputs/synthetic/' + sample.name + '.apng';
  writeFileSync(absolute(file), sample.bytes, { flag: 'wx' });
  samples.push({ file, group: 'synthetic', sha256: digest(sample.bytes), expect: sample.expect });
}
const exe = (name) =>
  path.join(root, 'target/release/examples/' + name + (process.platform === 'win32' ? '.exe' : ''));
const binary = exe('apng_lossy_lab'),
  lossless = exe('apng_experiment');
mkdirSync(absolute('inputs/source-png'));
mkdirSync(absolute('inputs/derived'));
const derivations = [];
for (const original of [
  'png/PNG_transparency_demonstration_1.png',
  'png/transparent.png',
  'png/palette_sepia.png',
]) {
  const provenance = sources.files.find((s) => s.file === original);
  assert.ok(provenance, original);
  const source = 'inputs/source-png/' + path.basename(original);
  cpSync(path.join(root, 'tests/manual-images', original), absolute(source));
  assert.equal(digest(readFileSync(absolute(source))), provenance.sha256);
  const file = 'inputs/derived/' + path.basename(original) + '.apng';
  const evidence = JSON.parse(run(binary, ['derive', absolute(source), absolute(file)]));
  assert.equal(evidence.sourceSha256, provenance.sha256);
  assert.equal(digest(readFileSync(absolute(source))), provenance.sha256);
  samples.push({ file, sha256: evidence.sha256, group: 'derived', expect: 'ok' });
  derivations.push({ file, source, provenance, ...evidence });
}
writeFileSync(absolute('derived-sources.json'), JSON.stringify(derivations, null, 2));
cpSync(
  path.join(root, 'tools/apng-lossy-lab/imagequant-COPYRIGHT.txt'),
  absolute('licenses/imagequant-COPYRIGHT.txt'),
);
const recipe = [
  'Cargo.toml',
  'crates/pixofold-core/Cargo.toml',
  'tools/apng-lossy-lab.mjs',
  'tools/apng-lossy-lab/fixtures.mjs',
  'tools/apng-lossy-lab/report.mjs',
  'tools/apng-lab/fixtures.mjs',
  'crates/pixofold-core/examples/apng_lossy_lab.rs',
  'crates/pixofold-core/examples/apng_experiment.rs',
  'crates/pixofold-core/examples/apng_lab/mod.rs',
  'crates/pixofold-core/examples/apng_lab/memory.rs',
  'crates/pixofold-core/examples/apng_lab/derive.rs',
  'tools/apng-lossy-lab/imagequant-COPYRIGHT.txt',
  'crates/pixofold-core/src/apng/mod.rs',
  ...['mod', 'structure', 'decode'].map(
    (n) => 'crates/pixofold-core/src/apng/validation/' + n + '.rs',
  ),
  ...['mod', 'budget', 'encode', 'compare', 'palette', 'remap', 'score', 'tests'].map(
    (n) => 'crates/pixofold-core/src/apng/validation/quantization/' + n + '.rs',
  ),
];
const identity = {
  policyVersion: 2,
  features: ['apng-lossy-lab'],
  imagequant: '4.4.1',
  png: '0.18.1',
  fdeflate: '0.3.7',
  oxipng: '10.2.1',
  cargoLockSha256: digest(readFileSync(path.join(root, 'Cargo.lock'))),
  binarySha256: digest(readFileSync(binary)),
  baselineBinarySha256: digest(readFileSync(lossless)),
  platform: process.platform,
  arch: process.arch,
  node: process.version,
  rustc: run('rustc', ['-Vv']).trim(),
  head: run('git', ['rev-parse', 'HEAD']).trim(),
  workingTree: run('git', ['status', '--porcelain']).trim() ? 'modified' : 'clean',
  recipe: Object.fromEntries(recipe.map((f) => [f, digest(readFileSync(path.join(root, f)))])),
};
const report = {
  result: 'running',
  corpusCount,
  identity,
  samples,
  rows: [],
  boundaries: [],
  screening: {
    minimumExtraBytes: 1024,
    minimumExtraRatio: 0.01,
    qualityPolicy:
      'v2: v1 limits retained plus actual decoded pixel score; not perceptual certification',
  },
};
const write = (name, value) =>
  writeFileSync(absolute(name), JSON.stringify(value, null, 2) + String.fromCharCode(10));
write('engine-identity.json', identity);
const execute = (mode, file, output, options) =>
  JSON.parse(run(binary, [mode, absolute(file), absolute(output), JSON.stringify(options)]));
try {
  for (const [index, sample] of samples.entries()) {
    const original = readFileSync(absolute(sample.file));
    assert.equal(digest(original), sample.sha256, sample.file);
    sample.inputBytes = original.length;
    if (sample.expect === 'ok') {
      const file = 'baselines/' + index + '.apng';
      const baseline = JSON.parse(
        run(lossless, ['optimize', absolute(sample.file), absolute(file), '1']),
      );
      assert.ok(
        ['gain', 'no_gain'].includes(baseline.result),
        sample.file + ' strict baseline: ' + JSON.stringify(baseline),
      );
      sample.baseline = {
        file: baseline.result === 'gain' ? file : sample.file,
        candidateFile: file,
        effectiveBytes: Math.min(original.length, baseline.candidateBytes),
        ...baseline,
      };
    }
    for (const quality of sample.expect === 'ok' ? [0, 50, 80, 95, 100] : [80]) {
      for (const dithering of sample.expect === 'ok' ? [false, true] : [false]) {
        for (const remapping of sample.expect === 'ok'
          ? ['standard', 'alpha_constrained', 'alpha_palette']
          : ['alpha_palette']) {
          const output =
            'outputs/' +
            index +
            '-q' +
            quality +
            '-d' +
            Number(dithering) +
            '-' +
            remapping +
            '.apng';
          const evidence = execute('quantize', sample.file, output, {
            quality,
            dithering,
            remapping,
            ...(sample.limits ? { limits: sample.limits } : {}),
          });
          const accepted =
            sample.expect === 'ok'
              ? evidence.result === 'candidate'
              : evidence.result === 'rejected' && evidence.reason === sample.expect;
          const row = { sample: sample.file, output, accepted, ...evidence };
          if (evidence.result === 'candidate') {
            const candidate = readFileSync(absolute(output));
            assert.equal(digest(candidate), evidence.candidateSha256);
            assert.equal(candidate.length, evidence.candidateBytes);
            row.extraSavedBytes = sample.baseline.effectiveBytes - evidence.candidateBytes;
            row.savedRatio = (original.length - evidence.candidateBytes) / original.length;
            row.screening = !evidence.report.qualityPassed
              ? 'quality_rejected'
              : row.extraSavedBytes <
                  Math.max(1024, Math.ceil(sample.baseline.effectiveBytes * 0.01))
                ? 'insufficient_gain'
                : 'quality_candidate';
          } else {
            row.screening = 'hard_rejected';
          }
          report.rows.push(row);
          assert.equal(digest(readFileSync(absolute(sample.file))), sample.sha256);
        }
      }
    }
    write('report.json', report);
    console.log(
      sample.file +
        ': ' +
        report.rows
          .filter((r) => r.sample === sample.file)
          .map((r) => r.screening + (r.reason ? '/' + r.reason : ''))
          .join(', '),
    );
  }
  for (const mode of ['cancel', 'deadline']) {
    const evidence = execute(
      mode,
      'inputs/synthetic/changing-colors.apng',
      'outputs/' + mode + '.apng',
      { quality: 80, dithering: true },
    );
    report.boundaries.push({
      mode,
      accepted:
        evidence.result === 'rejected' &&
        evidence.reason === (mode === 'cancel' ? 'cancelled' : 'deadline') &&
        !evidence.candidateWritten &&
        evidence.sourceUnchanged &&
        (mode !== 'cancel' || evidence.cancellationRequested),
      ...evidence,
    });
  }
  // 新策略的透明覆盖是固定验收条件，不能只记录若干更小候选就宣布算法通过。
  const constrained = report.rows.filter(
    (r) => r.result === 'candidate' && r.options.remapping === 'alpha_palette',
  );
  report.alphaContract = {
    checked: constrained.length,
    passed: constrained.every(
      (r) =>
        r.report.remappingStats.unrepresentablePixels === 0 &&
        r.report.comparison.rawAlphaClassChanges === 0 &&
        r.report.comparison.maxRawAlphaError <= (r.options.quality === 100 ? 0 : 4),
    ),
  };
  const realGain = report.rows.some(
    (r) =>
      r.sample.startsWith('inputs/derived/') &&
      r.options.remapping === 'alpha_palette' &&
      r.options.quality >= 80 &&
      r.screening === 'quality_candidate',
  );
  report.result =
    report.rows.every((r) => r.accepted) &&
    report.boundaries.every((r) => r.accepted) &&
    report.alphaContract.passed &&
    realGain &&
    report.rows.some((r) => r.screening === 'quality_candidate')
      ? 'passed'
      : 'failed';
} catch (error) {
  report.result = 'failed';
  throw error;
} finally {
  // 运行结束再次核对仓库原件与隔离源；测试成功不掩盖对源的意外修改。
  const sourceChecks = [];
  const matches = (file, sha) => {
    try {
      return digest(readFileSync(file)) === sha;
    } catch {
      return false;
    }
  };
  for (const sample of samples) {
    sourceChecks.push(matches(absolute(sample.file), sample.sha256));
    if (sample.group === 'public' || sample.group === 'structural') {
      const original = path.join(
        root,
        sample.group === 'public' ? 'tests/manual-images/apng' : 'tests/fixtures/apng',
        path.basename(sample.file),
      );
      sourceChecks.push(matches(original, sample.sha256));
    }
  }
  for (const derived of derivations) {
    sourceChecks.push(matches(absolute(derived.source), derived.sourceSha256));
    sourceChecks.push(
      matches(
        path.join(root, 'tests/manual-images', derived.provenance.file),
        derived.sourceSha256,
      ),
    );
  }
  report.sourceVerification = { checked: sourceChecks.length, passed: sourceChecks.every(Boolean) };
  if (!report.sourceVerification.passed) report.result = 'failed';
  write('report.json', report);
  writeFileSync(absolute('comparison.html'), htmlReport(report));
  console.log('APNG有损实验报告：' + directory);
}
assert.equal(report.result, 'passed', '预期外拒绝、无有效收益或资源边界失败，详见完整报告');
