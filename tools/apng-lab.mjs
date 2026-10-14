// 锁定依赖的APNG无损实验，输入只复制到target，每次单独目录保留全部证据。
import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { root, digest, writeFixtures, stressFixture } from './apng-lab/fixtures.mjs';

const args = process.argv.slice(2);
assert.deepEqual(args, ['check'], '参数：check');
function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: root,
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
    timeout: 300000,
    windowsHide: true,
  });
  if (result.status !== 0)
    throw new Error(command + '失败\n' + result.stdout + '\n' + result.stderr, {
      cause: result.error,
    });
  return result.stdout;
}
const count = writeFixtures(true);
console.log(run('cargo', ['test', '--locked', '-p', 'pixofold-core', '--test', 'apng_lab']));
console.log(
  run('cargo', [
    'build',
    '--release',
    '--locked',
    '-p',
    'pixofold-core',
    '--example',
    'apng_experiment',
  ]),
);
const binary = path.join(
  root,
  'target/release/examples/apng_experiment' + (process.platform === 'win32' ? '.exe' : ''),
);
mkdirSync(path.join(root, 'target'), { recursive: true });
const directory = mkdtempSync(path.join(root, 'target/apng-lab-'));
const inputs = path.join(directory, 'inputs'),
  outputs = path.join(directory, 'outputs');
mkdirSync(inputs);
mkdirSync(outputs);
cpSync(path.join(root, 'tests/fixtures/apng'), path.join(inputs, 'generated'), { recursive: true });
cpSync(path.join(root, 'tests/manual-images/apng'), path.join(inputs, 'public'), {
  recursive: true,
});
cpSync(path.join(root, 'tests/manual-images/SOURCES.json'), path.join(directory, 'SOURCES.json'));
cpSync(path.join(root, 'tests/manual-images/licenses'), path.join(directory, 'licenses'), {
  recursive: true,
});
writeFileSync(path.join(inputs, 'stress.apng'), stressFixture(), { flag: 'wx' });
const manifest = JSON.parse(readFileSync(path.join(inputs, 'generated/manifest.json'), 'utf8'));
const publicSources = JSON.parse(
  readFileSync(path.join(root, 'tests/manual-images/SOURCES.json'), 'utf8'),
);
const sources = publicSources.files;
assert.ok(Array.isArray(sources), 'public source schema');
for (const license of publicSources.license_documents) {
  assert.equal(
    digest(readFileSync(path.join(directory, license.file))),
    license.sha256,
    license.file,
  );
}
const samples = manifest.samples
  .filter((s) => s.expect === 'ok')
  .map((s) => ({ ...s, file: 'generated/' + s.file }));
samples.push(
  ...sources
    .filter((s) => s.file.startsWith('apng/'))
    .map((s) => ({ ...s, file: 'public/' + path.basename(s.file) })),
);
samples.push({
  file: 'stress.apng',
  sha256: digest(readFileSync(path.join(inputs, 'stress.apng'))),
});
const rows = [];
for (const sample of samples) {
  const input = path.join(inputs, sample.file);
  assert.equal(digest(readFileSync(input)), sample.sha256, sample.file);
  for (const preset of [0, 1, 2]) {
    const output = path.join(outputs, 'O' + preset + '-' + path.basename(sample.file));
    const evidence = JSON.parse(run(binary, ['optimize', input, output, String(preset)]));
    const expectedReject = sample.file === 'public/mode_palette_alpha.apng';
    const accepted = expectedReject
      ? evidence.result === 'rejected' && evidence.reason === 'unsupported_representation'
      : ['gain', 'no_gain'].includes(evidence.result);
    rows.push({ file: sample.file, preset, accepted, ...evidence });
    assert.equal(digest(readFileSync(input)), sample.sha256, sample.file);
    console.log(
      sample.file +
        ' O' +
        preset +
        ': ' +
        evidence.result +
        (evidence.reason ? ' (' + evidence.reason + ')' : ''),
    );
  }
}
const boundaries = [];
for (const mode of ['cancel', 'deadline']) {
  const evidence = JSON.parse(
    run(binary, [mode, path.join(inputs, 'stress.apng'), path.join(outputs, mode + '.apng'), '2']),
  );
  const accepted =
    evidence.result === (mode === 'cancel' ? 'cancelled' : 'deadline') &&
    evidence.candidateWritten === false &&
    evidence.sourceUnchanged === true;
  boundaries.push({ mode, accepted, ...evidence });
}
const identity = {
  png: '0.18.1',
  oxipng: '10.2.1',
  fdeflate: '0.3.7',
  cargoLockSha256: digest(readFileSync(path.join(root, 'Cargo.lock'))),
  binarySha256: digest(readFileSync(binary)),
  platform: process.platform,
  arch: process.arch,
  node: process.version,
  head: run('git', ['rev-parse', 'HEAD']).trim(),
  workingTree: run('git', ['status', '--porcelain']).trim() ? 'modified' : 'clean',
  recipe: Object.fromEntries(
    [
      'tools/apng-lab.mjs',
      'tools/apng-lab/fixtures.mjs',
      'crates/pixofold-core/examples/apng_experiment.rs',
      ...['mod', 'memory', 'tests'].map(
        (n) => 'crates/pixofold-core/examples/apng_lab/' + n + '.rs',
      ),
      ...['mod', 'structure', 'decode'].map(
        (n) => 'crates/pixofold-core/src/apng/validation/' + n + '.rs',
      ),
      ...['mod', 'model', 'pipeline'].map((n) => 'crates/pixofold-core/src/apng/' + n + '.rs'),
    ].map((file) => [file, digest(readFileSync(path.join(root, file)))]),
  ),
};
const passed =
  rows.every((r) => r.accepted) &&
  rows.some((r) => r.result === 'gain') &&
  boundaries.every((r) => r.accepted);
const report = {
  result: passed ? 'passed' : 'failed',
  corpusCount: count,
  identity,
  rows,
  boundaries,
};
writeFileSync(path.join(directory, 'report.json'), JSON.stringify(report, null, 2) + '\n');
writeFileSync(
  path.join(directory, 'engine-identity.json'),
  JSON.stringify(identity, null, 2) + '\n',
);
console.log('APNG报告：' + directory);
assert.ok(
  rows.some((r) => r.result === 'gain'),
  '没有真实收益',
);
assert.ok(passed, '出现预期外输入/编码/验证拒绝，请审阅完整report.json');
