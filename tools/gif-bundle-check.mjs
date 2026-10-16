// 在开发环境之外复制生产资源，以编入的可信身份运行GIF核心及桌面验收。
import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync, renameSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { root, target, execute, digest, helper, verifyTools } from './gif-lab/build.mjs';
import { verifyTools as verifyJpegTools } from './jpeg-lab/build.mjs';
import { createSamples } from './jpeg-lab/experiment.mjs';

const args = process.argv.slice(2);
if (args.length && !(args.length === 2 && args[0] === '--resources' && path.isAbsolute(args[1])))
  throw new Error('参数：可选--resources 已解包的绝对资源根目录');
const resourceSource = args[1] ?? path.join(root, 'src-tauri/resources');
execute(
  'cargo',
  [
    'build',
    '--locked',
    '--release',
    '-p',
    'pixofold-desktop',
    '--features',
    'bundle-check',
    '--bin',
    'gif-bundle-check',
  ],
  { stdio: 'inherit', timeout: 600000 },
);
const extension = process.platform === 'win32' ? '.exe' : '';
const isolated = mkdtempSync(path.join(tmpdir(), 'pixofold-gif-bundle-'));
const install = path.join(isolated, '中文 安装路径');
mkdirSync(install);
cpSync(path.join(resourceSource, 'gif'), path.join(install, 'gif'), { recursive: true });
cpSync(path.join(resourceSource, 'jpeg'), path.join(install, 'jpeg'), { recursive: true });
const checker = path.join(install, 'gif-bundle-check' + extension);
cpSync(path.join(root, 'target/release/gif-bundle-check' + extension), checker);
const input = path.join(isolated, '动画.GIF');
const original = readFileSync(path.join(root, 'tests/fixtures/gif/larger-pattern.gif'));
writeFileSync(input, original, { flag: 'wx' });
const sampleDirectory = path.join(isolated, 'samples');
mkdirSync(sampleDirectory);
const { samples } = createSamples({ root, tools: verifyJpegTools(), directory: sampleDirectory });
for (const [name, bytes] of samples)
  writeFileSync(path.join(sampleDirectory, name + '.jpg'), bytes, { flag: 'wx' });
// 在开发阶段生成可复现语料；隔离验收进程只使用随包资源。
execute(
  'cargo',
  [
    'run',
    '--release',
    '--locked',
    '-p',
    'pixofold-core',
    '--features',
    'gif-lossy-lab',
    '--example',
    'gif_lossy_lab',
    '--',
    'fixtures',
    sampleDirectory,
    path.join(root, 'tests/manual-images/gif/hopper.gif'),
  ],
  { stdio: 'inherit' },
);
verifyTools();
execute(helper, [
  '--memory-bytes',
  '33554432',
  '--careful',
  '-O2',
  '--resize-fit',
  '320x320',
  '--output',
  path.join(sampleDirectory, 'openrefine-320.gif'),
  path.join(root, 'tests/fixtures/gif-lossy/openrefine-wikidata.gif'),
]);
const cleanEnv =
  process.platform === 'win32'
    ? {
        SystemRoot: process.env.SystemRoot,
        WINDIR: process.env.WINDIR,
        TEMP: isolated,
        TMP: isolated,
        PATH: path.join(process.env.SystemRoot, 'System32'),
      }
    : { TMPDIR: isolated, PATH: '/usr/bin:/bin', LC_ALL: 'C' };
const run = (args) => {
  const result = spawnSync(checker, args, {
    cwd: isolated,
    env: cleanEnv,
    timeout: 60000,
    maxBuffer: 4 * 1024 * 1024,
    windowsHide: true,
  });
  if (result.error) throw result.error;
  return result;
};
const output = path.join(isolated, 'outputs');
const checked = run([install, input, output]);
assert.equal(checked.status, 0, checked.stderr?.toString());
const report = JSON.parse(checked.stdout);
assert.equal(report.result, 'passed');
assert.equal(report.desktop.protocolVersion, 15);
assert.equal(report.desktop.jpegMetadata.result, 'passed');
assert.equal(report.desktop.jpegMetadata.matrix.length, 4);
assert.equal(report.desktop.apng.result, 'passed');
assert.equal(report.desktop.result, 'passed');
assert.equal(report.desktop.previews, 15);
assert.equal(report.desktop.gifLossy.result, 'passed');
const capabilities = () => {
  const value = run(['--capabilities', install]);
  assert.equal(value.status, 0, value.stderr?.toString());
  return JSON.parse(value.stdout);
};
assert.deepEqual(capabilities(), ['png', 'jpeg', 'gif', 'apng']);
const jpegBinary = path.join(install, 'jpeg/runtime/pixofold-jpeg-helper' + extension);
renameSync(jpegBinary, jpegBinary + '.saved');
assert.deepEqual(capabilities(), ['png', 'gif', 'apng'], 'JPEG失效不得撤销其他格式能力');
renameSync(jpegBinary + '.saved', jpegBinary);
const runtime = path.join(install, 'gif/runtime');
const binary = path.join(runtime, 'pixofold-gif-helper' + extension);
const helperSha256 = digest(readFileSync(binary));
renameSync(binary, binary + '.saved');
assert.notEqual(run([install]).status, 0, '缺工具必须拒绝');
assert.deepEqual(capabilities(), ['png', 'jpeg', 'apng'], 'GIF失效不得撤销其他格式能力');
writeFileSync(binary, Buffer.from('untrusted replacement'));
writeFileSync(
  path.join(runtime, 'manifest.json'),
  JSON.stringify({ files: { [path.basename(binary)]: digest(readFileSync(binary)) } }),
);
assert.notEqual(run([install]).status, 0, '篡改旁置清单不得授权新工具');
assert.deepEqual(capabilities(), ['png', 'jpeg', 'apng']);
renameSync(jpegBinary, jpegBinary + '.saved');
assert.deepEqual(capabilities(), ['png', 'apng'], '两个工具失效仍须保留内建PNG/APNG');
assert.equal(digest(readFileSync(input)), digest(original));
const evidence = mkdtempSync(path.join(root, 'target/gif-bundle-'));
cpSync(output, path.join(evidence, 'outputs'), { recursive: true });
cpSync(input, path.join(evidence, 'input.gif'));
writeFileSync(
  path.join(evidence, 'report.json'),
  JSON.stringify(
    {
      ...report,
      target,
      helperSha256,
      isolatedDirectory: isolated,
      missingRejected: true,
      forgedManifestRejected: true,
      independentCapabilities: true,
      sourceUnchanged: true,
    },
    null,
    2,
  ) + '\n',
);
console.log('GIF随包资源隔离验证通过：' + evidence);
