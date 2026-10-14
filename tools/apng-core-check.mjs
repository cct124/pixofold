// 固定O1单文件核心+隔离输出验收；不依赖桌面、外部编码器或真实网络。
import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { root, digest, writeFixtures, stressFixture } from './apng-lab/fixtures.mjs';
assert.equal(process.argv.length, 2, '不接受额外参数');
function run(args) {
  const result = spawnSync('cargo', args, { cwd: root, stdio: 'inherit', windowsHide: true });
  if (result.status !== 0) throw new Error('APNG核心检查失败', { cause: result.error });
}
writeFixtures(true);
run(['test', '-p', 'pixofold-core', '--locked', '--test', 'apng_core']);
mkdirSync(path.join(root, 'target'), { recursive: true });
const directory = mkdtempSync(path.join(root, 'target/apng-core-'));
const inputs = path.join(directory, 'inputs');
mkdirSync(inputs);
mkdirSync(path.join(directory, 'outputs'));
cpSync(path.join(root, 'tests/fixtures/apng'), path.join(inputs, 'generated'), { recursive: true });
cpSync(path.join(root, 'tests/manual-images/apng'), path.join(inputs, 'public'), {
  recursive: true,
});
cpSync(path.join(root, 'tests/manual-images/licenses'), path.join(directory, 'licenses'), {
  recursive: true,
});
cpSync(path.join(root, 'tests/manual-images/SOURCES.json'), path.join(directory, 'SOURCES.json'));
const manifest = JSON.parse(readFileSync(path.join(inputs, 'generated/manifest.json'), 'utf8'));
const sources = JSON.parse(readFileSync(path.join(directory, 'SOURCES.json'), 'utf8'));
for (const license of sources.license_documents)
  assert.equal(digest(readFileSync(path.join(directory, license.file))), license.sha256);
const samples = manifest.samples
  .filter((s) => s.expect === 'ok')
  .map((s) => ({ file: 'generated/' + s.file, sha256: s.sha256 }));
for (const s of sources.files.filter((s) => s.file.startsWith('apng/')))
  samples.push({
    file: 'public/' + path.basename(s.file),
    sha256: s.sha256,
    ...(s.file.endsWith('mode_palette_alpha.apng') ? { reject: 'unsupported_representation' } : {}),
  });
const stress = stressFixture();
writeFileSync(path.join(inputs, 'stress.apng'), stress, { flag: 'wx' });
samples.push({ file: 'stress.apng', sha256: digest(stress) });
writeFileSync(path.join(directory, 'cases.json'), JSON.stringify({ samples }, null, 2) + '\n');
run([
  'run',
  '--release',
  '--locked',
  '-p',
  'pixofold-core',
  '--example',
  'apng_core_check',
  '--',
  directory,
]);
const report = JSON.parse(readFileSync(path.join(directory, 'report.json'), 'utf8'));
assert.equal(report.result, 'passed');
const binary = path.join(
  root,
  'target/release/examples/apng_core_check' + (process.platform === 'win32' ? '.exe' : ''),
);
writeFileSync(
  path.join(directory, 'identity.json'),
  JSON.stringify(
    {
      png: '0.18.1',
      oxipng: '10.2.1',
      fdeflate: '0.3.7',
      preset: 1,
      platform: process.platform,
      arch: process.arch,
      cargoLockSha256: digest(readFileSync(path.join(root, 'Cargo.lock'))),
      binarySha256: digest(readFileSync(binary)),
      recipe: Object.fromEntries(
        [
          'tools/apng-core-check.mjs',
          'crates/pixofold-core/examples/apng_core_check.rs',
          ...[
            'mod',
            'model',
            'pipeline',
            'validation/mod',
            'validation/decode',
            'validation/structure',
          ].map((n) => 'crates/pixofold-core/src/apng/' + n + '.rs'),
        ].map((f) => [f, digest(readFileSync(path.join(root, f)))]),
      ),
    },
    null,
    2,
  ) + '\n',
);
console.log('APNG核心证据：' + directory);
